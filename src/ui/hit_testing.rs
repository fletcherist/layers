use std::collections::HashSet;

use indexmap::IndexMap;

use crate::automation::AutomationParam;
use crate::component;
use crate::effects;
use crate::entity_id::EntityId;
use crate::grid::snap_to_grid;
use crate::instruments;
use crate::midi;
use crate::regions::{ExportRegion, LoopRegion};
use crate::settings::Settings;
use crate::ui;
use crate::{Camera, CanvasObject, HitTarget, WaveformView};

pub(crate) fn point_in_rect(pos: [f32; 2], rect_pos: [f32; 2], rect_size: [f32; 2]) -> bool {
    pos[0] >= rect_pos[0]
        && pos[0] <= rect_pos[0] + rect_size[0]
        && pos[1] >= rect_pos[1]
        && pos[1] <= rect_pos[1] + rect_size[1]
}

pub(crate) fn rects_overlap(a_pos: [f32; 2], a_size: [f32; 2], b_pos: [f32; 2], b_size: [f32; 2]) -> bool {
    a_pos[0] < b_pos[0] + b_size[0]
        && a_pos[0] + a_size[0] > b_pos[0]
        && a_pos[1] < b_pos[1] + b_size[1]
        && a_pos[1] + a_size[1] > b_pos[1]
}

pub(crate) fn hit_test_corner_resize(
    position: [f32; 2],
    size: [f32; 2],
    world_pos: [f32; 2],
    zoom: f32,
) -> Option<([f32; 2], bool)> {
    let handle_sz = 24.0 / zoom;
    let p = position;
    let s = size;
    let corners: [([f32; 2], [f32; 2], bool); 4] = [
        ([p[0], p[1]], [p[0] + s[0], p[1] + s[1]], true),
        ([p[0] + s[0], p[1]], [p[0], p[1] + s[1]], false),
        ([p[0], p[1] + s[1]], [p[0] + s[0], p[1]], false),
        ([p[0] + s[0], p[1] + s[1]], [p[0], p[1]], true),
    ];
    for (corner, anchor, is_nwse) in &corners {
        let hx = corner[0] - handle_sz * 0.5;
        let hy = corner[1] - handle_sz * 0.5;
        if point_in_rect(world_pos, [hx, hy], [handle_sz, handle_sz]) {
            return Some((*anchor, *is_nwse));
        }
    }
    None
}

pub(crate) fn compute_resize(
    anchor: [f32; 2],
    world: [f32; 2],
    min_size: f32,
    snap_x: bool,
    settings: &Settings,
    zoom: f32,
    bpm: f32,
) -> ([f32; 2], [f32; 2]) {
    let (wx, ax) = if snap_x {
        (
            snap_to_grid(world[0], settings, zoom, bpm),
            snap_to_grid(anchor[0], settings, zoom, bpm),
        )
    } else {
        (world[0], anchor[0])
    };
    let x0 = ax.min(wx);
    let y0 = anchor[1].min(world[1]);
    let x1 = ax.max(wx);
    let y1 = anchor[1].max(world[1]);
    ([x0, y0], [(x1 - x0).max(min_size), (y1 - y0).max(min_size)])
}

pub(crate) const WAVEFORM_MIN_WIDTH_PX: f32 = 10.0;

pub(crate) fn full_audio_width_px(wf: &WaveformView) -> f32 {
    let total_samples = wf.audio.left_samples.len().max(wf.audio.right_samples.len());
    total_samples as f32 / (wf.audio.sample_rate as f32 / crate::grid::PIXELS_PER_SECOND)
}

pub(crate) fn canonical_rect(a: [f32; 2], b: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let w = (a[0] - b[0]).abs();
    let h = (a[1] - b[1]).abs();
    ([x, y], [w, h])
}

pub(crate) const WAVEFORM_EDGE_HIT_PX: f32 = 20.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum WaveformEdgeHover {
    None,
    LeftEdge(EntityId),
    RightEdge(EntityId),
}

pub(crate) fn hit_test_waveform_edge(
    waveforms: &IndexMap<EntityId, WaveformView>,
    world_pos: [f32; 2],
    camera: &Camera,
) -> WaveformEdgeHover {
    let margin = WAVEFORM_EDGE_HIT_PX / camera.zoom;
    // Collect all candidates: (dist, cursor_inside_clip, result)
    // Prefer candidates where cursor is inside the clip body, then by distance.
    let mut best: Option<(f32, bool, WaveformEdgeHover)> = None;

    for (&id, wf) in waveforms.iter().rev() {
        if world_pos[1] < wf.position[1] || world_pos[1] > wf.position[1] + wf.size[1] {
            continue;
        }
        let left_edge = wf.position[0];
        let right_edge = wf.position[0] + wf.size[0];
        let cursor_inside = world_pos[0] >= left_edge && world_pos[0] <= right_edge;

        let left_dist = (world_pos[0] - left_edge).abs();
        let right_dist = (world_pos[0] - right_edge).abs();

        for (dist, hover) in [
            (left_dist, WaveformEdgeHover::LeftEdge(id)),
            (right_dist, WaveformEdgeHover::RightEdge(id)),
        ] {
            if dist >= margin {
                continue;
            }
            let better = match best {
                None => true,
                Some((bd, bi, _)) => (cursor_inside && !bi) || (cursor_inside == bi && dist < bd),
            };
            if better {
                best = Some((dist, cursor_inside, hover));
            }
        }
    }

    best.map(|(_, _, result)| result).unwrap_or(WaveformEdgeHover::None)
}

/// Returns (waveform_id, is_fade_in) if the cursor is over a fade handle.
pub(crate) fn hit_test_fade_handle(
    waveforms: &IndexMap<EntityId, WaveformView>,
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<(EntityId, bool)> {
    let hit_margin = 30.0;
    for (&id, wf) in waveforms.iter().rev() {
        let fi_cx = wf.position[0] + wf.fade_in_px;
        let fi_cy = wf.position[1];
        if (world_pos[0] - fi_cx).abs() < hit_margin && (world_pos[1] - fi_cy).abs() < hit_margin {
            return Some((id, true));
        }

        let fo_cx = wf.position[0] + wf.size[0] - wf.fade_out_px;
        let fo_cy = wf.position[1];
        if (world_pos[0] - fo_cx).abs() < hit_margin && (world_pos[1] - fo_cy).abs() < hit_margin {
            return Some((id, false));
        }
    }
    None
}

/// Returns (waveform_id, is_fade_in) if the cursor is near the fade curve midpoint dot.
pub(crate) fn hit_test_fade_curve_dot(
    waveforms: &IndexMap<EntityId, WaveformView>,
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<(EntityId, bool)> {
    let hit_radius = ui::waveform::FADE_HANDLE_SIZE / camera.zoom;
    for (&id, wf) in waveforms.iter().rev() {
        if wf.fade_in_px > 0.0 {
            let [dx, dy] = ui::waveform::fade_curve_dot_pos(wf, true);
            if (world_pos[0] - dx).abs() < hit_radius && (world_pos[1] - dy).abs() < hit_radius {
                return Some((id, true));
            }
        }
        if wf.fade_out_px > 0.0 {
            let [dx, dy] = ui::waveform::fade_curve_dot_pos(wf, false);
            if (world_pos[0] - dx).abs() < hit_radius && (world_pos[1] - dy).abs() < hit_radius {
                return Some((id, false));
            }
        }
    }
    None
}

pub(crate) fn hit_test(
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
    editing_component: Option<EntityId>,
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<HitTarget> {
    // When editing a component, only its waveforms are hittable
    if let Some(ec_id) = editing_component {
        if let Some(def) = components.get(&ec_id) {
            for &wf_id in def.waveform_ids.iter().rev() {
                if let Some(wf) = waveforms.get(&wf_id) {
                    if point_in_rect(world_pos, wf.position, wf.size) {
                        return Some(HitTarget::Waveform(wf_id));
                    }
                }
            }
        }
        return None;
    }

    let wf_in_component: HashSet<EntityId> = components
        .values()
        .flat_map(|c| c.waveform_ids.iter().copied())
        .collect();

    // Instances first (on top)
    for (&id, inst) in component_instances.iter().rev() {
        if let Some(def) = components.get(&inst.component_id) {
            if point_in_rect(world_pos, inst.position, def.size) {
                return Some(HitTarget::ComponentInstance(id));
            }
        }
    }
    for (&id, wf) in waveforms.iter().rev() {
        if wf_in_component.contains(&id) {
            continue;
        }
        if point_in_rect(world_pos, wf.position, wf.size) {
            return Some(HitTarget::Waveform(id));
        }
    }
    for (&id, obj) in objects.iter().rev() {
        if point_in_rect(world_pos, obj.position, obj.size) {
            return Some(HitTarget::Object(id));
        }
    }
    for (&id, tn) in text_notes.iter().rev() {
        if point_in_rect(world_pos, tn.position, tn.size) {
            return Some(HitTarget::TextNote(id));
        }
    }
    for (&id, def) in components.iter().rev() {
        if point_in_rect(world_pos, def.position, def.size) {
            return Some(HitTarget::ComponentDef(id));
        }
    }
    for (&id, pb) in plugin_blocks.iter().rev() {
        if pb.contains(world_pos) {
            return Some(HitTarget::PluginBlock(id));
        }
    }
    for (&id, mc) in midi_clips.iter().rev() {
        if mc.contains(world_pos) {
            return Some(HitTarget::MidiClip(id));
        }
    }
    for (&id, er) in effect_regions.iter().rev() {
        if er.hit_test_border(world_pos, camera) {
            return Some(HitTarget::EffectRegion(id));
        }
    }
    for (&id, ir) in instrument_regions.iter().rev() {
        if ir.hit_test_border(world_pos, camera) {
            return Some(HitTarget::InstrumentRegion(id));
        }
    }
    for (&id, lr) in loop_regions.iter().rev() {
        if lr.hit_test_border(world_pos, camera) {
            return Some(HitTarget::LoopRegion(id));
        }
    }
    for (&id, xr) in export_regions.iter().rev() {
        if xr.hit_test_border(world_pos, camera) {
            return Some(HitTarget::ExportRegion(id));
        }
    }
    None
}

pub(crate) fn targets_in_rect(
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
    editing_component: Option<EntityId>,
    rect_pos: [f32; 2],
    rect_size: [f32; 2],
) -> Vec<HitTarget> {
    let mut result = Vec::new();

    // When editing a component, only its waveforms are selectable via rect
    if let Some(ec_id) = editing_component {
        if let Some(def) = components.get(&ec_id) {
            for &wf_id in &def.waveform_ids {
                if let Some(wf) = waveforms.get(&wf_id) {
                    if rects_overlap(rect_pos, rect_size, wf.position, wf.size) {
                        result.push(HitTarget::Waveform(wf_id));
                    }
                }
            }
        }
        return result;
    }

    let wf_in_component: HashSet<EntityId> = components
        .values()
        .flat_map(|c| c.waveform_ids.iter().copied())
        .collect();

    for (&id, obj) in objects.iter() {
        if rects_overlap(rect_pos, rect_size, obj.position, obj.size) {
            result.push(HitTarget::Object(id));
        }
    }
    for (&id, wf) in waveforms.iter() {
        if wf_in_component.contains(&id) {
            continue;
        }
        if rects_overlap(rect_pos, rect_size, wf.position, wf.size) {
            result.push(HitTarget::Waveform(id));
        }
    }
    for (&id, er) in effect_regions.iter() {
        if rects_overlap(rect_pos, rect_size, er.position, er.size) {
            result.push(HitTarget::EffectRegion(id));
        }
    }
    for (&id, pb) in plugin_blocks.iter() {
        if rects_overlap(rect_pos, rect_size, pb.position, pb.size) {
            result.push(HitTarget::PluginBlock(id));
        }
    }
    for (&id, lr) in loop_regions.iter() {
        if rects_overlap(rect_pos, rect_size, lr.position, lr.size) {
            result.push(HitTarget::LoopRegion(id));
        }
    }
    for (&id, xr) in export_regions.iter() {
        if rects_overlap(rect_pos, rect_size, xr.position, xr.size) {
            result.push(HitTarget::ExportRegion(id));
        }
    }
    for (&id, def) in components.iter() {
        if rects_overlap(rect_pos, rect_size, def.position, def.size) {
            result.push(HitTarget::ComponentDef(id));
        }
    }
    for (&id, inst) in component_instances.iter() {
        if let Some(def) = components.get(&inst.component_id) {
            if rects_overlap(rect_pos, rect_size, inst.position, def.size) {
                result.push(HitTarget::ComponentInstance(id));
            }
        }
    }
    for (&id, mc) in midi_clips.iter() {
        if rects_overlap(rect_pos, rect_size, mc.position, mc.size) {
            result.push(HitTarget::MidiClip(id));
        }
    }
    for (&id, ir) in instrument_regions.iter() {
        if rects_overlap(rect_pos, rect_size, ir.position, ir.size) {
            result.push(HitTarget::InstrumentRegion(id));
        }
    }
    for (&id, tn) in text_notes.iter() {
        if rects_overlap(rect_pos, rect_size, tn.position, tn.size) {
            result.push(HitTarget::TextNote(id));
        }
    }
    result
}

/// Returns (waveform_id, point_idx) if cursor is near an automation breakpoint.
pub(crate) fn hit_test_automation_point(
    waveforms: &IndexMap<EntityId, WaveformView>,
    world_pos: [f32; 2],
    camera: &Camera,
    param: AutomationParam,
) -> Option<(EntityId, usize)> {
    let hit_radius = 8.0 / camera.zoom;
    for (&id, wf) in waveforms.iter().rev() {
        let lane = wf.automation.lane_for(param);
        let y_top = wf.position[1];
        let y_bot = wf.position[1] + wf.size[1];
        for (pi, p) in lane.points.iter().enumerate() {
            let px = wf.position[0] + p.t * wf.size[0];
            let py = y_bot + (y_top - y_bot) * p.value;
            if (world_pos[0] - px).abs() < hit_radius && (world_pos[1] - py).abs() < hit_radius {
                return Some((id, pi));
            }
        }
    }
    None
}

/// Returns (waveform_id, t, value) if cursor is near an automation line segment (for inserting).
pub(crate) fn hit_test_automation_line(
    waveforms: &IndexMap<EntityId, WaveformView>,
    world_pos: [f32; 2],
    camera: &Camera,
    param: AutomationParam,
) -> Option<(EntityId, f32, f32)> {
    let threshold = 4.0 / camera.zoom;
    for (&id, wf) in waveforms.iter().rev() {
        // Check if point is inside waveform rect first
        if !point_in_rect(world_pos, wf.position, wf.size) {
            continue;
        }
        let lane = wf.automation.lane_for(param);
        if lane.points.len() < 2 {
            continue;
        }
        let y_top = wf.position[1];
        let y_bot = wf.position[1] + wf.size[1];

        // Check each segment
        for seg in 0..lane.points.len() - 1 {
            let a = &lane.points[seg];
            let b = &lane.points[seg + 1];
            let ax = wf.position[0] + a.t * wf.size[0];
            let ay = y_bot + (y_top - y_bot) * a.value;
            let bx = wf.position[0] + b.t * wf.size[0];
            let by = y_bot + (y_top - y_bot) * b.value;

            // Check if world_pos.x is between ax and bx
            if world_pos[0] < ax.min(bx) - threshold || world_pos[0] > ax.max(bx) + threshold {
                continue;
            }

            // Distance from point to line segment
            let dx = bx - ax;
            let dy = by - ay;
            let len_sq = dx * dx + dy * dy;
            if len_sq < 1e-6 {
                continue;
            }
            let t_proj = ((world_pos[0] - ax) * dx + (world_pos[1] - ay) * dy) / len_sq;
            let t_proj = t_proj.clamp(0.0, 1.0);
            let proj_x = ax + t_proj * dx;
            let proj_y = ay + t_proj * dy;
            let dist = ((world_pos[0] - proj_x).powi(2) + (world_pos[1] - proj_y).powi(2)).sqrt();

            if dist < threshold {
                // Convert world_pos to (t, value) in automation space
                let t = ((world_pos[0] - wf.position[0]) / wf.size[0]).clamp(0.0, 1.0);
                let value = ((world_pos[1] - y_bot) / (y_top - y_bot)).clamp(0.0, 1.0);
                return Some((id, t, value));
            }
        }
    }
    None
}
