use std::collections::HashSet;

use indexmap::IndexMap;

use crate::component;
use crate::entity_id::EntityId;
use crate::grid::{grid_spacing_for_settings, pixels_per_beat};
use crate::ui::hit_testing::canonical_rect;
use crate::instruments;
use crate::midi;
use crate::regions::{
    ExportRegion, LoopRegion, SelectArea, EXPORT_RENDER_PILL_H, EXPORT_RENDER_PILL_W,
    LOOP_BADGE_H, LOOP_BADGE_W,
};
use crate::settings::Settings;
use crate::ui;
use crate::ui::waveform::WaveformVertex;
use crate::{push_border, Camera, CanvasObject, HitTarget, InstanceRaw, WaveformView};


pub(crate) struct RenderContext<'a> {
    pub(crate) camera: &'a Camera,
    pub(crate) screen_w: f32,
    pub(crate) screen_h: f32,
    pub(crate) objects: &'a IndexMap<EntityId, CanvasObject>,
    pub(crate) waveforms: &'a IndexMap<EntityId, WaveformView>,
    pub(crate) hovered: Option<HitTarget>,
    pub(crate) selected: &'a HashSet<HitTarget>,
    pub(crate) selection_rect: Option<([f32; 2], [f32; 2])>,
    pub(crate) select_area: Option<&'a SelectArea>,
    pub(crate) file_hovering: bool,
    pub(crate) playhead_world_x: Option<f32>,
    pub(crate) export_regions: &'a IndexMap<EntityId, ExportRegion>,
    pub(crate) loop_regions: &'a IndexMap<EntityId, LoopRegion>,
    pub(crate) components: &'a IndexMap<EntityId, component::ComponentDef>,
    pub(crate) component_instances: &'a IndexMap<EntityId, component::ComponentInstance>,
    pub(crate) editing_component: Option<EntityId>,
    pub(crate) settings: &'a Settings,
    pub(crate) fade_curve_hovered: Option<(EntityId, bool)>,
    pub(crate) fade_curve_dragging: Option<(EntityId, bool)>,
    pub(crate) mouse_world: [f32; 2],
    pub(crate) bpm: f32,
    pub(crate) automation_mode: bool,
    pub(crate) active_automation_param: crate::automation::AutomationParam,
    pub(crate) midi_clips: &'a IndexMap<EntityId, midi::MidiClip>,
    pub(crate) instruments: &'a IndexMap<EntityId, instruments::Instrument>,
    pub(crate) text_notes: &'a IndexMap<EntityId, crate::text_note::TextNote>,
    pub(crate) editing_midi_clip: Option<EntityId>,
    pub(crate) selected_midi_notes: &'a [usize],
    pub(crate) midi_note_select_rect: Option<[f32; 4]>,
    pub(crate) groups: &'a IndexMap<EntityId, crate::group::Group>,
    pub(crate) remote_users: &'a std::collections::HashMap<crate::user::UserId, crate::user::RemoteUserState>,
    pub(crate) network_mode: crate::network::NetworkMode,
    pub(crate) hidden_take_children: &'a HashSet<EntityId>,
    pub(crate) solo_ids: &'a std::collections::HashSet<EntityId>,
    pub(crate) following_user: Option<crate::entity_id::EntityId>,
}

/// Returns true if an entity should appear dimmed on canvas due to solo/mute/disabled state.
fn is_dimmed_by_solo_mute(id: EntityId, ctx: &RenderContext) -> bool {
    // Check disabled on the entity itself
    if ctx.waveforms.get(&id).map_or(false, |wf| wf.disabled) {
        return true;
    }
    if ctx.instruments.get(&id).map_or(false, |inst| inst.disabled) {
        return true;
    }
    if ctx.groups.get(&id).map_or(false, |g| g.disabled) {
        return true;
    }
    !crate::is_entity_audible(id, ctx.solo_ids, ctx.groups)
}

pub(crate) fn build_instances(out: &mut Vec<InstanceRaw>, ctx: &RenderContext) {
    out.clear();

    let camera = ctx.camera;
    let world_left = camera.position[0];
    let world_top = camera.position[1];
    let world_right = world_left + ctx.screen_w / camera.zoom;
    let world_bottom = world_top + ctx.screen_h / camera.zoom;

    // --- musical grid ---
    if ctx.settings.grid_enabled {
        let spacing = grid_spacing_for_settings(ctx.settings, camera.zoom, ctx.bpm);
        let ppb = pixels_per_beat(ctx.bpm);
        let bar_spacing = ppb * 4.0;
        let line_w = 1.0 / camera.zoom;
        let major_line_w = 2.0 / camera.zoom;
        let bar_line_w = 2.5 / camera.zoom;
        let grid_i = ctx.settings.grid_line_intensity;
        let bg = ctx.settings.theme.bg_base;
        let lighten = |amt: f32| -> [f32; 4] {
            [(bg[0] + amt).min(1.0), (bg[1] + amt).min(1.0), (bg[2] + amt).min(1.0), 1.0]
        };
        let minor_color = lighten(grid_i * 0.08);
        let beat_color  = lighten(grid_i * 0.15);
        let bar_color   = lighten(grid_i * 0.25);

        let first_xi = (world_left / spacing).floor() as i64;
        let last_xi = (world_right / spacing).ceil() as i64;
        for i in first_xi..=last_xi {
            let x = i as f32 * spacing;
            let on_bar = (x / bar_spacing).round() * bar_spacing;
            let on_beat = (x / ppb).round() * ppb;
            let is_bar = (x - on_bar).abs() < 0.01;
            let is_beat = !is_bar && (x - on_beat).abs() < 0.01;
            let (w, c) = if is_bar {
                (bar_line_w, bar_color)
            } else if is_beat {
                (major_line_w, beat_color)
            } else {
                (line_w, minor_color)
            };
            out.push(InstanceRaw {
                position: [x - w * 0.5, world_top],
                size: [w, world_bottom - world_top],
                color: c,
                border_radius: 0.0,
            });
        }

        let first_yi = (world_top / spacing).floor() as i64;
        let last_yi = (world_bottom / spacing).ceil() as i64;
        for i in first_yi..=last_yi {
            let y = i as f32 * spacing;
            let is_major = i % 4 == 0;
            let w = if is_major { major_line_w } else { line_w };
            let c = if is_major { beat_color } else { minor_color };
            out.push(InstanceRaw {
                position: [world_left, y - w * 0.5],
                size: [world_right - world_left, w],
                color: c,
                border_radius: 0.0,
            });
        }
    }


    // --- midi clips ---
    for (&id, mc) in ctx.midi_clips.iter() {
        if mc.disabled { continue; }
        let mc_right = mc.position[0] + mc.size[0];
        let mc_bottom = mc.position[1] + mc.size[1];
        if mc_right < world_left
            || mc.position[0] > world_right
            || mc_bottom < world_top
            || mc.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::MidiClip(id));
        let is_hov = ctx.hovered == Some(HitTarget::MidiClip(id));
        let editing = ctx.editing_midi_clip == Some(id);
        out.extend(midi::build_midi_clip_instances(mc, camera, is_hov, is_sel, editing));
        let sel_notes = if editing {
            ctx.selected_midi_notes
        } else {
            &[]
        };
        out.extend(midi::build_midi_note_instances(
            mc, camera, sel_notes, editing,
        ));
        // Plugin name label badge at top-left of MIDI clip
        if let Some(inst_id) = mc.instrument_id {
            if let Some(inst) = ctx.instruments.get(&inst_id) {
                if !inst.plugin_name.is_empty() {
                    let badge_h = 14.0 / camera.zoom;
                    let badge_w = (inst.plugin_name.len() as f32 * 6.0 + 8.0) / camera.zoom;
                    out.push(InstanceRaw {
                        position: [mc.position[0] + 2.0 / camera.zoom, mc.position[1] + 2.0 / camera.zoom],
                        size: [badge_w, badge_h],
                        color: crate::theme::with_alpha(ctx.settings.theme.instrument_border_color, 0.65),
                        border_radius: 2.0 / camera.zoom,
                    });
                }
            }
        }
        // TODO: refactor velocity lane rendering before re-enabling
        // if editing {
        //     out.extend(midi::build_velocity_lane_instances(mc, camera, sel_notes));
        // }
    }

    // --- export regions ---
    for (&id, er) in ctx.export_regions.iter() {
        let p = er.position;
        let s = er.size;
        let er_right = p[0] + s[0];
        let er_bottom = p[1] + s[1];
        if er_right < world_left
            || p[0] > world_right
            || er_bottom < world_top
            || p[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::ExportRegion(id));
        let is_hov = ctx.hovered == Some(HitTarget::ExportRegion(id));

        out.push(InstanceRaw {
            position: p,
            size: s,
            color: ctx.settings.theme.export_fill_color,
            border_radius: 6.0 / camera.zoom,
        });

        let bw = if is_sel {
            2.5
        } else if is_hov {
            2.0
        } else {
            1.5
        } / camera.zoom;
        push_border(out, p, s, bw, ctx.settings.theme.export_border_color);

        let dash_h = 3.0 / camera.zoom;
        let dash_w = 20.0 / camera.zoom;
        let gap = 10.0 / camera.zoom;
        let dy = p[1] - dash_h - 2.0 / camera.zoom;
        let mut dx = p[0];
        while dx < er_right {
            let w = dash_w.min(er_right - dx);
            out.push(InstanceRaw {
                position: [dx, dy],
                size: [w, dash_h],
                color: ctx.settings.theme.export_border_color,
                border_radius: 1.0 / camera.zoom,
            });
            dx += dash_w + gap;
        }

        let pill_w = EXPORT_RENDER_PILL_W / camera.zoom;
        let pill_h = EXPORT_RENDER_PILL_H / camera.zoom;
        let pill_x = p[0] + 4.0 / camera.zoom;
        let pill_y = p[1] + 4.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [pill_x, pill_y],
            size: [pill_w, pill_h],
            color: ctx.settings.theme.export_render_pill_color,
            border_radius: pill_h * 0.5,
        });

        if is_sel {
            let handle_sz = 8.0 / camera.zoom;
            for &hx in &[p[0] - handle_sz * 0.5, er_right - handle_sz * 0.5] {
                for &hy in &[p[1] - handle_sz * 0.5, er_bottom - handle_sz * 0.5] {
                    out.push(InstanceRaw {
                        position: [hx, hy],
                        size: [handle_sz, handle_sz],
                        color: ctx.settings.theme.playhead,
                        border_radius: 2.0 / camera.zoom,
                    });
                }
            }
        }
    }

    // --- loop regions (viewport-height vertical strips) ---
    for (&id, lr) in ctx.loop_regions.iter() {
        let lr_right = lr.position[0] + lr.size[0];
        if lr_right < world_left || lr.position[0] > world_right {
            continue;
        }
        let (p, s) = lr.visual_bounds(world_top, world_bottom);
        let is_sel = ctx.selected.contains(&HitTarget::LoopRegion(id));
        let is_hov = ctx.hovered == Some(HitTarget::LoopRegion(id));
        let alpha_mul = if lr.enabled { 1.0 } else { 0.25 };

        let fill = [
            ctx.settings.theme.loop_fill_color[0],
            ctx.settings.theme.loop_fill_color[1],
            ctx.settings.theme.loop_fill_color[2],
            ctx.settings.theme.loop_fill_color[3] * alpha_mul,
        ];
        let border = [
            ctx.settings.theme.loop_border_color[0],
            ctx.settings.theme.loop_border_color[1],
            ctx.settings.theme.loop_border_color[2],
            ctx.settings.theme.loop_border_color[3] * alpha_mul,
        ];
        let badge = [
            ctx.settings.theme.loop_badge_color[0],
            ctx.settings.theme.loop_badge_color[1],
            ctx.settings.theme.loop_badge_color[2],
            ctx.settings.theme.loop_badge_color[3] * alpha_mul,
        ];

        out.push(InstanceRaw {
            position: p,
            size: s,
            color: fill,
            border_radius: 0.0,
        });

        let bw = if is_sel {
            2.5
        } else if is_hov {
            2.0
        } else {
            1.5
        } / camera.zoom;
        // Left and right borders only (full viewport height)
        out.push(InstanceRaw { position: p, size: [bw, s[1]], color: border, border_radius: 0.0 });
        out.push(InstanceRaw { position: [p[0] + s[0] - bw, p[1]], size: [bw, s[1]], color: border, border_radius: 0.0 });

        let dash_h = 3.0 / camera.zoom;
        let dash_w = 20.0 / camera.zoom;
        let gap = 10.0 / camera.zoom;
        let dy = world_top + 2.0 / camera.zoom;
        let mut dx = p[0];
        while dx < lr_right {
            let w = dash_w.min(lr_right - dx);
            out.push(InstanceRaw {
                position: [dx, dy],
                size: [w, dash_h],
                color: border,
                border_radius: 1.0 / camera.zoom,
            });
            dx += dash_w + gap;
        }

        let pill_w = LOOP_BADGE_W / camera.zoom;
        let pill_h = LOOP_BADGE_H / camera.zoom;
        let pill_x = p[0] + 4.0 / camera.zoom;
        let pill_y = world_top + 8.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [pill_x, pill_y],
            size: [pill_w, pill_h],
            color: badge,
            border_radius: pill_h * 0.5,
        });

        if is_sel {
            let handle_sz = 8.0 / camera.zoom;
            let mid_y = (world_top + world_bottom) * 0.5;
            // Left and right edge handles only (centered vertically)
            out.push(InstanceRaw {
                position: [p[0] - handle_sz * 0.5, mid_y - handle_sz * 0.5],
                size: [handle_sz, handle_sz],
                color: crate::theme::with_alpha(ctx.settings.theme.accent, 0.9 * alpha_mul),
                border_radius: 2.0 / camera.zoom,
            });
            out.push(InstanceRaw {
                position: [lr_right - handle_sz * 0.5, mid_y - handle_sz * 0.5],
                size: [handle_sz, handle_sz],
                color: crate::theme::with_alpha(ctx.settings.theme.accent, 0.9 * alpha_mul),
                border_radius: 2.0 / camera.zoom,
            });
        }
    }

    // --- canvas objects ---
    let ci = ctx.settings.color_intensity;
    let apply_intensity = |c: [f32; 4]| -> [f32; 4] {
        let s_mult = 0.05 + ci * 0.95;
        let lum = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        [
            (lum + (c[0] - lum) * s_mult).clamp(0.0, 1.0),
            (lum + (c[1] - lum) * s_mult).clamp(0.0, 1.0),
            (lum + (c[2] - lum) * s_mult).clamp(0.0, 1.0),
            c[3],
        ]
    };
    for (&id, obj) in ctx.objects.iter() {
        let obj_right = obj.position[0] + obj.size[0];
        let obj_bottom = obj.position[1] + obj.size[1];
        if obj_right < world_left
            || obj.position[0] > world_right
            || obj_bottom < world_top
            || obj.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Object(id));
        let is_hov = ctx.hovered == Some(HitTarget::Object(id));
        let mut color = apply_intensity(obj.color);
        if is_sel || is_hov {
            color[0] = (color[0] + 0.10).min(1.0);
            color[1] = (color[1] + 0.10).min(1.0);
            color[2] = (color[2] + 0.10).min(1.0);
        }
        out.push(InstanceRaw {
            position: obj.position,
            size: obj.size,
            color,
            border_radius: obj.border_radius,
        });
    }

    // --- text notes ---
    for (&id, tn) in ctx.text_notes.iter() {
        let tn_right = tn.position[0] + tn.size[0];
        let tn_bottom = tn.position[1] + tn.size[1];
        if tn_right < world_left
            || tn.position[0] > world_right
            || tn_bottom < world_top
            || tn.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::TextNote(id));
        let is_hov = ctx.hovered == Some(HitTarget::TextNote(id));
        let mut color = apply_intensity(tn.color);
        if is_sel || is_hov {
            color[0] = (color[0] + 0.08).min(1.0);
            color[1] = (color[1] + 0.08).min(1.0);
            color[2] = (color[2] + 0.08).min(1.0);
        }
        out.push(InstanceRaw {
            position: tn.position,
            size: tn.size,
            color,
            border_radius: tn.border_radius,
        });
    }

    // --- waveforms ---
    for (&id, wf) in ctx.waveforms.iter() {
        if ctx.hidden_take_children.contains(&id) { continue; }
        let wf_right = wf.position[0] + wf.size[0];
        let wf_bottom = wf.position[1] + wf.size[1];
        if wf_right < world_left
            || wf.position[0] > world_right
            || wf_bottom < world_top
            || wf.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Waveform(id));
        let is_hov = ctx.hovered == Some(HitTarget::Waveform(id));
        let mut wf_color = apply_intensity(wf.color);
        // Dim muted or non-soloed clips
        if is_dimmed_by_solo_mute(id, ctx) {
            wf_color[3] *= 0.35;
        }
        out.extend(ui::waveform::build_waveform_instances(
            wf,
            camera,
            world_left,
            world_right,
            is_hov,
            is_sel,
            wf_color,
        ));

        // Automation breakpoint dots (only in automation mode for active param)
        if ctx.automation_mode {
            out.extend(ui::waveform::build_automation_dot_instances(
                wf,
                camera,
                ctx.active_automation_param,
            ));
        }
    }

    // --- component definitions ---
    for (&id, def) in ctx.components.iter() {
        let def_right = def.position[0] + def.size[0];
        let def_bottom = def.position[1] + def.size[1];
        if def_right < world_left
            || def.position[0] > world_right
            || def_bottom < world_top
            || def.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::ComponentDef(id));
        let is_hov = ctx.hovered == Some(HitTarget::ComponentDef(id));
        let is_editing = ctx.editing_component == Some(id);
        out.extend(component::build_component_def_instances(
            def,
            camera,
            is_hov,
            is_sel || is_editing,
            &ctx.settings.theme,
        ));
    }

    // --- component instances ---
    for (&id, inst) in ctx.component_instances.iter() {
        if let Some(def) = ctx.components.get(&inst.component_id) {
            let inst_right = inst.position[0] + def.size[0];
            let inst_bottom = inst.position[1] + def.size[1];
            if inst_right < world_left
                || inst.position[0] > world_right
                || inst_bottom < world_top
                || inst.position[1] > world_bottom
            {
                continue;
            }
            let is_sel = ctx.selected.contains(&HitTarget::ComponentInstance(id));
            let is_hov = ctx.hovered == Some(HitTarget::ComponentInstance(id));
            out.extend(component::build_component_instance_instances(
                inst,
                def,
                ctx.waveforms,
                camera,
                world_left,
                world_right,
                is_hov,
                is_sel,
                &ctx.settings.theme,
            ));
        }
    }

    // --- groups ---
    for (&id, group) in ctx.groups.iter() {
        let g_right = group.position[0] + group.size[0];
        let g_bottom = group.position[1] + group.size[1];
        if g_right < world_left
            || group.position[0] > world_right
            || g_bottom < world_top
            || group.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Group(id));
        let is_hov = ctx.hovered == Some(HitTarget::Group(id));
        out.extend(crate::group::build_group_instances(
            group,
            camera,
            is_hov,
            is_sel,
            &ctx.settings.theme,
        ));
        // Dim overlay for muted/non-soloed groups
        if is_dimmed_by_solo_mute(id, ctx) {
            out.push(InstanceRaw {
                position: group.position,
                size: group.size,
                color: [ctx.settings.theme.bg_base[0], ctx.settings.theme.bg_base[1], ctx.settings.theme.bg_base[2], 0.65],
                border_radius: 4.0 / camera.zoom,
            });
        }
    }

    // --- edit mode dimming overlay ---
    if let Some(ec_id) = ctx.editing_component {
        if let Some(def) = ctx.components.get(&ec_id) {
            // Dim everything outside the component with 4 dark rectangles
            let dim_color = ctx.settings.theme.shadow_strong;
            // Top strip
            out.push(InstanceRaw {
                position: [world_left, world_top],
                size: [world_right - world_left, def.position[1] - world_top],
                color: dim_color,
                border_radius: 0.0,
            });
            // Bottom strip
            let bot_y = def.position[1] + def.size[1];
            out.push(InstanceRaw {
                position: [world_left, bot_y],
                size: [world_right - world_left, world_bottom - bot_y],
                color: dim_color,
                border_radius: 0.0,
            });
            // Left strip
            out.push(InstanceRaw {
                position: [world_left, def.position[1]],
                size: [def.position[0] - world_left, def.size[1]],
                color: dim_color,
                border_radius: 0.0,
            });
            // Right strip
            let right_x = def.position[0] + def.size[0];
            out.push(InstanceRaw {
                position: [right_x, def.position[1]],
                size: [world_right - right_x, def.size[1]],
                color: dim_color,
                border_radius: 0.0,
            });
        }
    }

    // --- selection highlights (rendered on top of everything) ---
    let sel_bw = 2.0 / camera.zoom;
    let handle_sz = 8.0 / camera.zoom;
    for target in ctx.selected.iter() {
        let Some((pos, size)) = target_rect(
            ctx.objects,
            ctx.waveforms,
            ctx.loop_regions,
            ctx.export_regions,
            ctx.components,
            ctx.component_instances,
            ctx.midi_clips,
            ctx.text_notes,
            ctx.groups,
            target,
        ) else {
            continue;
        };
        push_border(out, pos, size, sel_bw, ctx.settings.theme.selection);

        for &hx in &[pos[0] - handle_sz * 0.5, pos[0] + size[0] - handle_sz * 0.5] {
            for &hy in &[pos[1] - handle_sz * 0.5, pos[1] + size[1] - handle_sz * 0.5] {
                out.push(InstanceRaw {
                    position: [hx, hy],
                    size: [handle_sz, handle_sz],
                    color: [1.0, 1.0, 1.0, 1.0],
                    border_radius: 2.0 / camera.zoom,
                });
            }
        }
    }

    // --- selection rectangle (transient, during drag) ---
    if let Some((start, current)) = ctx.selection_rect {
        let (rp, rs) = canonical_rect(start, current);
        out.push(InstanceRaw {
            position: rp,
            size: rs,
            color: ctx.settings.theme.select_rect_fill,
            border_radius: 0.0,
        });
        let bw = 1.0 / camera.zoom;
        push_border(out, rp, rs, bw, ctx.settings.theme.select_rect_border);
    } else if let Some(sa) = ctx.select_area {
        let thin_threshold = 4.0 / camera.zoom;
        if sa.size[0] < thin_threshold {
            out.push(InstanceRaw {
                position: sa.position,
                size: sa.size,
                color: ctx.settings.theme.select_outline,
                border_radius: 0.0,
            });
        } else {
            out.push(InstanceRaw {
                position: sa.position,
                size: sa.size,
                color: ctx.settings.theme.select_rect_fill,
                border_radius: 0.0,
            });
            let bw = 1.0 / camera.zoom;
            push_border(out, sa.position, sa.size, bw, ctx.settings.theme.select_rect_border);
        }
    }

    // --- MIDI note selection rectangle (inside editing clip) ---
    if let Some([rx, ry, rw, rh]) = ctx.midi_note_select_rect {
        if rw > 0.0 && rh > 0.0 {
            out.push(InstanceRaw {
                position: [rx, ry],
                size: [rw, rh],
                color: crate::theme::with_alpha(ctx.settings.theme.select_rect_fill, 0.15),
                border_radius: 0.0,
            });
            let bw = 1.0 / camera.zoom;
            push_border(out, [rx, ry], [rw, rh], bw, crate::theme::with_alpha(ctx.settings.theme.select_rect_border, 0.6));
        }
    }

    // --- playback cursor ---
    if let Some(px) = ctx.playhead_world_x {
        let line_w = 2.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [px - line_w * 0.5, world_top],
            size: [line_w, world_bottom - world_top],
            color: crate::theme::with_alpha(ctx.settings.theme.text_primary, 0.85),
            border_radius: 0.0,
        });
        let head_sz = 10.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [px - head_sz * 0.5, world_top],
            size: [head_sz, head_sz],
            color: crate::theme::with_alpha(ctx.settings.theme.text_primary, 0.95),
            border_radius: 2.0 / camera.zoom,
        });
    }

    // --- remote user cursors ---
    for (_uid, remote) in ctx.remote_users {
        if !remote.online {
            continue;
        }
        if let Some(pos) = remote.cursor_world {
            let s = 1.0 / camera.zoom;
            let c = remote.user.color;

            // Compose a pointer-arrow shape from rects:
            // Main body: tall narrow rect (the shaft of the arrow)
            out.push(InstanceRaw {
                position: [pos[0], pos[1]],
                size: [3.0 * s, 18.0 * s],
                color: c,
                border_radius: 0.5 * s,
            });
            // Right wing: short rect extending right, offset down
            out.push(InstanceRaw {
                position: [pos[0] + 2.0 * s, pos[1] + 12.0 * s],
                size: [10.0 * s, 3.0 * s],
                color: c,
                border_radius: 0.5 * s,
            });
            // Diagonal fill: small rect bridging body and wing
            out.push(InstanceRaw {
                position: [pos[0] + 1.0 * s, pos[1] + 10.0 * s],
                size: [5.0 * s, 5.0 * s],
                color: c,
                border_radius: 0.5 * s,
            });

        }
        // Ghost outlines for drag previews
        if let Some(preview) = &remote.drag_preview {
            let ghost_color = [remote.user.color[0], remote.user.color[1], remote.user.color[2], 0.3];
            match preview {
                crate::user::DragPreview::ResizingEntity { new_position, new_size, .. } => {
                    out.push(InstanceRaw {
                        position: *new_position,
                        size: *new_size,
                        color: ghost_color,
                        border_radius: 4.0 / camera.zoom,
                    });
                }
                crate::user::DragPreview::MovingEntities { targets } => {
                    for (_target, pos, size) in targets {
                        out.push(InstanceRaw {
                            position: *pos,
                            size: *size,
                            color: ghost_color,
                            border_radius: 4.0 / camera.zoom,
                        });
                    }
                }
            }
        }
    }

    // --- remote user avatar circles (top-right corner) ---
    {
        let s = 1.0 / camera.zoom;
        let r = 14.0 * s;
        let margin = 12.0 * s;
        let gap = 6.0 * s;
        let titlebar_offset = 28.0 * s;
        let mut x = world_right - margin - r;
        let y = world_top + margin + r + titlebar_offset;

        // Sort by user ID for stable ordering
        let mut sorted_users: Vec<_> = ctx.remote_users.iter()
            .filter(|(_, state)| state.online)
            .collect();
        sorted_users.sort_by_key(|(uid, _)| **uid);

        for (uid, remote) in &sorted_users {
            // Follow ring (slightly larger circle behind)
            if ctx.following_user == Some(**uid) {
                let ring_r = r + 3.0 * s;
                out.push(InstanceRaw {
                    position: [x - ring_r, y - ring_r],
                    size: [ring_r * 2.0, ring_r * 2.0],
                    color: [remote.user.color[0], remote.user.color[1], remote.user.color[2], 0.4],
                    border_radius: ring_r,
                });
            }
            // Avatar circle
            out.push(InstanceRaw {
                position: [x - r, y - r],
                size: [r * 2.0, r * 2.0],
                color: remote.user.color,
                border_radius: r,
            });
            x -= r * 2.0 + gap;
        }

        // Connection status dot (to the left of avatars)
        use crate::network::NetworkMode;
        let dot_color = match ctx.network_mode {
            NetworkMode::Connected => Some([0.30, 0.85, 0.39, 0.90]),
            NetworkMode::Connecting => Some([1.00, 0.84, 0.00, 0.90]),
            NetworkMode::Disconnected => Some([1.00, 0.24, 0.19, 0.90]),
            NetworkMode::Offline => None,
        };
        if let Some(color) = dot_color {
            let dot_sz = 8.0 * s;
            out.push(InstanceRaw {
                position: [x - dot_sz, world_top + margin + titlebar_offset + r - dot_sz * 0.5],
                size: [dot_sz, dot_sz],
                color,
                border_radius: dot_sz / 2.0,
            });
        }
    }

    // --- follow mode viewport border ---
    if let Some(followed_id) = ctx.following_user {
        if let Some(remote) = ctx.remote_users.get(&followed_id) {
            let bw = 3.0 / camera.zoom;
            let border_color = [
                remote.user.color[0],
                remote.user.color[1],
                remote.user.color[2],
                0.6,
            ];
            push_border(
                out,
                [world_left, world_top],
                [world_right - world_left, world_bottom - world_top],
                bw,
                border_color,
            );
        }
    }

    // --- file drop zone overlay ---
    if ctx.file_hovering {
        out.push(InstanceRaw {
            position: [world_left, world_top],
            size: [world_right - world_left, world_bottom - world_top],
            color: ctx.settings.theme.drop_zone_fill,
            border_radius: 0.0,
        });
        let bw = 3.0 / camera.zoom;
        push_border(
            out,
            [world_left, world_top],
            [world_right - world_left, world_bottom - world_top],
            bw,
            ctx.settings.theme.drop_zone_border,
        );
    }
}

pub(crate) fn build_waveform_vertices(verts: &mut Vec<WaveformVertex>, ctx: &RenderContext) {
    verts.clear();
    let camera = ctx.camera;
    let world_left = camera.position[0];
    let world_right = world_left + ctx.screen_w / camera.zoom;
    let world_top = camera.position[1];
    let world_bottom = world_top + ctx.screen_h / camera.zoom;
    for (&id, wf) in ctx.waveforms.iter() {
        if ctx.hidden_take_children.contains(&id) { continue; }
        let wf_right = wf.position[0] + wf.size[0];
        let wf_bottom = wf.position[1] + wf.size[1];
        if wf_right < world_left
            || wf.position[0] > world_right
            || wf_bottom < world_top
            || wf.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Waveform(id));
        let is_hov = ctx.hovered == Some(HitTarget::Waveform(id));
        let ci = ctx.settings.color_intensity;
        let s_mult = 0.05 + ci * 0.95;
        let lum = 0.299 * wf.color[0] + 0.587 * wf.color[1] + 0.114 * wf.color[2];
        let wf_color = [
            (lum + (wf.color[0] - lum) * s_mult).clamp(0.0, 1.0),
            (lum + (wf.color[1] - lum) * s_mult).clamp(0.0, 1.0),
            (lum + (wf.color[2] - lum) * s_mult).clamp(0.0, 1.0),
            wf.color[3],
        ];
        verts.extend(ui::waveform::build_waveform_triangles(
            wf,
            camera,
            world_left,
            world_right,
            is_hov,
            is_sel,
            ctx.bpm,
            wf_color,
        ));
        // Fade curve lines as smooth triangles (line only when cursor is near)
        let mx = ctx.mouse_world[0];
        let my = ctx.mouse_world[1];
        let in_wf_y = my >= wf.position[1] && my <= wf.position[1] + wf.size[1];
        let mouse_in_fi = wf.fade_in_px > 0.0
            && in_wf_y
            && mx >= wf.position[0]
            && mx <= wf.position[0] + wf.fade_in_px;
        let mouse_in_fo = wf.fade_out_px > 0.0
            && in_wf_y
            && mx >= wf.position[0] + wf.size[0] - wf.fade_out_px
            && mx <= wf.position[0] + wf.size[0];
        let show_fi_line = mouse_in_fi
            || matches!(ctx.fade_curve_hovered, Some((fid, true)) if fid == id)
            || matches!(ctx.fade_curve_dragging, Some((fid, true)) if fid == id);
        let show_fo_line = mouse_in_fo
            || matches!(ctx.fade_curve_hovered, Some((fid, false)) if fid == id)
            || matches!(ctx.fade_curve_dragging, Some((fid, false)) if fid == id);
        verts.extend(ui::waveform::build_fade_curve_triangles(
            wf,
            camera,
            show_fi_line,
            show_fo_line,
        ));

        // Automation lines
        use crate::automation::AutomationParam;
        let vol_lane = &wf.automation.volume_lane();
        let pan_lane = &wf.automation.pan_lane();
        // Always show volume line if non-default; show with editing highlight if in automation mode + Volume
        if !vol_lane.is_default()
            || (ctx.automation_mode && ctx.active_automation_param == AutomationParam::Volume)
        {
            let is_editing =
                ctx.automation_mode && ctx.active_automation_param == AutomationParam::Volume;
            verts.extend(ui::waveform::build_automation_triangles(
                wf,
                camera,
                AutomationParam::Volume,
                is_editing,
            ));
        }
        // Same for pan
        if !pan_lane.is_default()
            || (ctx.automation_mode && ctx.active_automation_param == AutomationParam::Pan)
        {
            let is_editing =
                ctx.automation_mode && ctx.active_automation_param == AutomationParam::Pan;
            verts.extend(ui::waveform::build_automation_triangles(
                wf,
                camera,
                AutomationParam::Pan,
                is_editing,
            ));
        }
    }
}

pub(crate) fn target_rect(
    objects: &IndexMap<EntityId, CanvasObject>,
    waveforms: &IndexMap<EntityId, WaveformView>,
    loop_regions: &IndexMap<EntityId, LoopRegion>,
    export_regions: &IndexMap<EntityId, ExportRegion>,
    components: &IndexMap<EntityId, component::ComponentDef>,
    component_instances: &IndexMap<EntityId, component::ComponentInstance>,
    midi_clips: &IndexMap<EntityId, midi::MidiClip>,
    text_notes: &IndexMap<EntityId, crate::text_note::TextNote>,
    groups: &IndexMap<EntityId, crate::group::Group>,
    target: &HitTarget,
) -> Option<([f32; 2], [f32; 2])> {
    match target {
        HitTarget::Object(id) => {
            let o = objects.get(id)?;
            Some((o.position, o.size))
        }
        HitTarget::Waveform(id) => {
            let w = waveforms.get(id)?;
            Some((w.position, w.size))
        }
        HitTarget::LoopRegion(id) => {
            let l = loop_regions.get(id)?;
            Some((l.position, l.size))
        }
        HitTarget::ExportRegion(id) => {
            let e = export_regions.get(id)?;
            Some((e.position, e.size))
        }
        HitTarget::ComponentDef(id) => {
            let c = components.get(id)?;
            Some((c.position, c.size))
        }
        HitTarget::ComponentInstance(id) => {
            let inst = component_instances.get(id)?;
            let def = components.get(&inst.component_id);
            match def {
                Some(d) => Some((inst.position, d.size)),
                None => Some((inst.position, [100.0, 100.0])),
            }
        }
        HitTarget::MidiClip(id) => {
            let m = midi_clips.get(id)?;
            Some((m.position, m.size))
        }
        HitTarget::TextNote(id) => {
            let tn = text_notes.get(id)?;
            Some((tn.position, tn.size))
        }
        HitTarget::Group(id) => {
            let g = groups.get(id)?;
            Some((g.position, g.size))
        }
        HitTarget::Instrument(_) => None,
    }
}

pub(crate) fn default_objects() -> IndexMap<EntityId, CanvasObject> {
    IndexMap::new()
}
