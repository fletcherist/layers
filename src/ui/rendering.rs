use std::collections::HashSet;

use indexmap::IndexMap;

use crate::component;
use crate::effects;
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
    pub(crate) effect_regions: &'a IndexMap<EntityId, effects::EffectRegion>,
    pub(crate) plugin_blocks: &'a IndexMap<EntityId, effects::PluginBlock>,
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
    pub(crate) instrument_regions: &'a IndexMap<EntityId, instruments::InstrumentRegion>,
    pub(crate) text_notes: &'a IndexMap<EntityId, crate::text_note::TextNote>,
    pub(crate) editing_midi_clip: Option<EntityId>,
    pub(crate) selected_midi_notes: &'a [usize],
    pub(crate) midi_note_select_rect: Option<[f32; 4]>,
    pub(crate) remote_users: &'a std::collections::HashMap<crate::user::UserId, crate::user::RemoteUserState>,
    pub(crate) network_mode: crate::network::NetworkMode,
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

    // --- origin axes ---
    let axis_w = 2.0 / camera.zoom;
    if world_top <= 0.0 && world_bottom >= 0.0 {
        out.push(InstanceRaw {
            position: [world_left, -axis_w * 0.5],
            size: [world_right - world_left, axis_w],
            color: [0.85, 0.25, 0.25, 0.5],
            border_radius: 0.0,
        });
    }
    if world_left <= 0.0 && world_right >= 0.0 {
        out.push(InstanceRaw {
            position: [-axis_w * 0.5, world_top],
            size: [axis_w, world_bottom - world_top],
            color: [0.25, 0.85, 0.25, 0.5],
            border_radius: 0.0,
        });
    }

    // --- effect regions (rendered behind everything) ---
    for (&id, er) in ctx.effect_regions.iter() {
        let er_right = er.position[0] + er.size[0];
        let er_bottom = er.position[1] + er.size[1];
        if er_right < world_left
            || er.position[0] > world_right
            || er_bottom < world_top
            || er.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::EffectRegion(id));
        let is_hov = ctx.hovered == Some(HitTarget::EffectRegion(id));
        let is_active = ctx.playhead_world_x.map_or(false, |px| {
            px >= er.position[0] && px <= er.position[0] + er.size[0]
        });
        out.extend(effects::build_effect_region_instances(
            er, camera, is_hov, is_sel, is_active, &ctx.settings.theme,
        ));
    }

    // --- plugin blocks ---
    for (&id, pb) in ctx.plugin_blocks.iter() {
        let pb_right = pb.position[0] + pb.size[0];
        let pb_bottom = pb.position[1] + pb.size[1];
        if pb_right < world_left
            || pb.position[0] > world_right
            || pb_bottom < world_top
            || pb.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::PluginBlock(id));
        let is_hov = ctx.hovered == Some(HitTarget::PluginBlock(id));
        out.extend(effects::build_plugin_block_instances(
            pb, camera, is_hov, is_sel, &ctx.settings.theme,
        ));
    }

    // --- instrument regions ---
    for (&id, ir) in ctx.instrument_regions.iter() {
        let ir_right = ir.position[0] + ir.size[0];
        let ir_bottom = ir.position[1] + ir.size[1];
        if ir_right < world_left
            || ir.position[0] > world_right
            || ir_bottom < world_top
            || ir.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::InstrumentRegion(id));
        let is_hov = ctx.hovered == Some(HitTarget::InstrumentRegion(id));
        let is_active = ctx.playhead_world_x.map_or(false, |px| {
            px >= ir.position[0] && px <= ir.position[0] + ir.size[0]
        });
        out.extend(instruments::build_instrument_region_instances(
            ir, camera, is_hov, is_sel, is_active, &ctx.settings.theme,
        ));
    }

    // --- midi clips ---
    for (&id, mc) in ctx.midi_clips.iter() {
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
                        color: [0.20, 0.80, 0.60, 0.9],
                        border_radius: 2.0 / camera.zoom,
                    });
                }
            }
        }
    }

    // --- loop regions ---
    for (&id, lr) in ctx.loop_regions.iter() {
        let p = lr.position;
        let s = lr.size;
        let lr_right = p[0] + s[0];
        let lr_bottom = p[1] + s[1];
        if lr_right < world_left
            || p[0] > world_right
            || lr_bottom < world_top
            || p[1] > world_bottom
        {
            continue;
        }
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
            border_radius: 6.0 / camera.zoom,
        });

        let bw = if is_sel {
            2.5
        } else if is_hov {
            2.0
        } else {
            1.5
        } / camera.zoom;
        push_border(out, p, s, bw, border);

        let dash_h = 3.0 / camera.zoom;
        let dash_w = 20.0 / camera.zoom;
        let gap = 10.0 / camera.zoom;
        let dy = p[1] - dash_h - 2.0 / camera.zoom;
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
        let pill_y = p[1] + 4.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [pill_x, pill_y],
            size: [pill_w, pill_h],
            color: badge,
            border_radius: pill_h * 0.5,
        });

        if is_sel {
            let handle_sz = 8.0 / camera.zoom;
            for &hx in &[p[0] - handle_sz * 0.5, lr_right - handle_sz * 0.5] {
                for &hy in &[p[1] - handle_sz * 0.5, lr_bottom - handle_sz * 0.5] {
                    out.push(InstanceRaw {
                        position: [hx, hy],
                        size: [handle_sz, handle_sz],
                        color: [0.30, 0.60, 1.0, 0.9 * alpha_mul],
                        border_radius: 2.0 / camera.zoom,
                    });
                }
            }
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
        let wf_color = apply_intensity(wf.color);
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

    // --- edit mode dimming overlay ---
    if let Some(ec_id) = ctx.editing_component {
        if let Some(def) = ctx.components.get(&ec_id) {
            // Dim everything outside the component with 4 dark rectangles
            let dim_color = [0.0, 0.0, 0.0, 0.50];
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
            ctx.effect_regions,
            ctx.plugin_blocks,
            ctx.loop_regions,
            ctx.export_regions,
            ctx.components,
            ctx.component_instances,
            ctx.midi_clips,
            ctx.instrument_regions,
            ctx.text_notes,
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
            color: [0.30, 0.55, 1.0, 0.10],
            border_radius: 0.0,
        });
        let bw = 1.0 / camera.zoom;
        push_border(out, rp, rs, bw, [0.35, 0.65, 1.0, 0.5]);
    } else if let Some(sa) = ctx.select_area {
        let thin_threshold = 4.0 / camera.zoom;
        if sa.size[0] < thin_threshold {
            out.push(InstanceRaw {
                position: sa.position,
                size: sa.size,
                color: [0.40, 0.65, 1.0, 1.0],
                border_radius: 0.0,
            });
        } else {
            out.push(InstanceRaw {
                position: sa.position,
                size: sa.size,
                color: [0.30, 0.55, 1.0, 0.10],
                border_radius: 0.0,
            });
            let bw = 1.0 / camera.zoom;
            push_border(out, sa.position, sa.size, bw, [0.35, 0.65, 1.0, 0.5]);
        }
    }

    // --- MIDI note selection rectangle (inside editing clip) ---
    if let Some([rx, ry, rw, rh]) = ctx.midi_note_select_rect {
        if rw > 0.0 && rh > 0.0 {
            out.push(InstanceRaw {
                position: [rx, ry],
                size: [rw, rh],
                color: [0.30, 0.55, 1.0, 0.15],
                border_radius: 0.0,
            });
            let bw = 1.0 / camera.zoom;
            push_border(out, [rx, ry], [rw, rh], bw, [0.35, 0.65, 1.0, 0.6]);
        }
    }

    // --- playback cursor ---
    if let Some(px) = ctx.playhead_world_x {
        let line_w = 2.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [px - line_w * 0.5, world_top],
            size: [line_w, world_bottom - world_top],
            color: [1.0, 1.0, 1.0, 0.85],
            border_radius: 0.0,
        });
        let head_sz = 10.0 / camera.zoom;
        out.push(InstanceRaw {
            position: [px - head_sz * 0.5, world_top],
            size: [head_sz, head_sz],
            color: [1.0, 1.0, 1.0, 0.95],
            border_radius: 2.0 / camera.zoom,
        });
    }

    // --- remote user cursors ---
    for (_uid, remote) in ctx.remote_users {
        if !remote.online {
            continue;
        }
        if let Some(pos) = remote.cursor_world {
            // Cursor arrow body
            let arrow_sz = 12.0 / camera.zoom;
            out.push(InstanceRaw {
                position: [pos[0], pos[1]],
                size: [arrow_sz, arrow_sz],
                color: remote.user.color,
                border_radius: 2.0 / camera.zoom,
            });
            // Name tag background
            let tag_w = 60.0 / camera.zoom;
            let tag_h = 16.0 / camera.zoom;
            let tag_x = pos[0] + arrow_sz * 0.8;
            let tag_y = pos[1] + arrow_sz * 0.8;
            out.push(InstanceRaw {
                position: [tag_x, tag_y],
                size: [tag_w, tag_h],
                color: [remote.user.color[0], remote.user.color[1], remote.user.color[2], 0.85],
                border_radius: 3.0 / camera.zoom,
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

    // --- connection status dot (top-right corner) ---
    {
        use crate::network::NetworkMode;
        let dot_color = match ctx.network_mode {
            NetworkMode::Connected => Some([0.30, 0.85, 0.39, 0.90]),    // green
            NetworkMode::Connecting => Some([1.00, 0.84, 0.00, 0.90]),   // yellow
            NetworkMode::Disconnected => Some([1.00, 0.24, 0.19, 0.90]), // red
            NetworkMode::Offline => None,
        };
        if let Some(color) = dot_color {
            let dot_sz = 8.0 / camera.zoom;
            let margin = 12.0 / camera.zoom;
            out.push(InstanceRaw {
                position: [world_right - dot_sz - margin, world_top + margin],
                size: [dot_sz, dot_sz],
                color,
                border_radius: dot_sz / 2.0,
            });
        }
    }

    // --- file drop zone overlay ---
    if ctx.file_hovering {
        out.push(InstanceRaw {
            position: [world_left, world_top],
            size: [world_right - world_left, world_bottom - world_top],
            color: [0.25, 0.50, 1.0, 0.10],
            border_radius: 0.0,
        });
        let bw = 3.0 / camera.zoom;
        push_border(
            out,
            [world_left, world_top],
            [world_right - world_left, world_bottom - world_top],
            bw,
            [0.35, 0.65, 1.0, 0.7],
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
    effect_regions: &IndexMap<EntityId, effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, effects::PluginBlock>,
    loop_regions: &IndexMap<EntityId, LoopRegion>,
    export_regions: &IndexMap<EntityId, ExportRegion>,
    components: &IndexMap<EntityId, component::ComponentDef>,
    component_instances: &IndexMap<EntityId, component::ComponentInstance>,
    midi_clips: &IndexMap<EntityId, midi::MidiClip>,
    instrument_regions: &IndexMap<EntityId, instruments::InstrumentRegion>,
    text_notes: &IndexMap<EntityId, crate::text_note::TextNote>,
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
        HitTarget::EffectRegion(id) => {
            let e = effect_regions.get(id)?;
            Some((e.position, e.size))
        }
        HitTarget::PluginBlock(id) => {
            let p = plugin_blocks.get(id)?;
            Some((p.position, p.size))
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
        HitTarget::InstrumentRegion(id) => {
            let ir = instrument_regions.get(id)?;
            Some((ir.position, ir.size))
        }
        HitTarget::TextNote(id) => {
            let tn = text_notes.get(id)?;
            Some((tn.position, tn.size))
        }
    }
}

pub(crate) fn default_objects() -> IndexMap<EntityId, CanvasObject> {
    IndexMap::new()
}
