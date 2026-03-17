use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::ui::hit_testing::point_in_rect;
use crate::{push_border, Camera, InstanceRaw};

// ---------------------------------------------------------------------------
// InstrumentRegion — spatial zone holding one VST3 instrument
// ---------------------------------------------------------------------------

pub const INSTRUMENT_REGION_DEFAULT_WIDTH: f32 = 600.0;
pub const INSTRUMENT_REGION_DEFAULT_HEIGHT: f32 = 250.0;
pub const INSTRUMENT_REGION_PADDING: f32 = 40.0;
const INSTRUMENT_BORDER_COLOR: [f32; 4] = [0.60, 0.30, 0.90, 0.50];
const INSTRUMENT_ACTIVE_BORDER: [f32; 4] = [0.70, 0.40, 1.00, 0.70];

#[derive(Clone)]
pub struct InstrumentRegion {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub gui: Arc<Mutex<Option<vst3_gui::Vst3Gui>>>,
    pub pending_state: Option<Vec<u8>>,
    pub pending_params: Option<Vec<f64>>,
}

impl InstrumentRegion {
    pub fn new(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            position,
            size,
            name: "instrument".to_string(),
            plugin_id: String::new(),
            plugin_name: String::new(),
            plugin_path: PathBuf::new(),
            gui: Arc::new(Mutex::new(None)),
            pending_state: None,
            pending_params: None,
        }
    }

    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let name_area_h = 30.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(
            world_pos,
            [p[0] - border_thickness, p[1] - border_thickness - name_area_h],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0 + name_area_h],
        ) {
            return false;
        }
        // Name label area above the region
        if point_in_rect(world_pos, [p[0], p[1] - name_area_h], [s[0], name_area_h]) {
            return true;
        }
        // Top edge
        if point_in_rect(world_pos, [p[0], p[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        // Bottom edge
        if point_in_rect(world_pos, [p[0], p[1] + s[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        // Left edge
        if point_in_rect(world_pos, [p[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        // Right edge
        if point_in_rect(world_pos, [p[0] + s[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        // INST badge area
        let badge_w = 36.0 / camera.zoom;
        let badge_h = 16.0 / camera.zoom;
        if point_in_rect(
            world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [badge_w + 100.0 / camera.zoom, badge_h],
        ) {
            return true;
        }
        false
    }

    pub fn has_plugin(&self) -> bool {
        !self.plugin_id.is_empty()
    }
}

/// Grow `region` so that the rectangle `clip_pos`/`clip_size` fits inside with `padding` on each side.
/// Only grows — never shrinks the region.
pub fn ensure_region_contains_clip(
    region: &mut InstrumentRegion,
    clip_pos: [f32; 2],
    clip_size: [f32; 2],
    padding: f32,
) {
    let needed_x0 = clip_pos[0] - padding;
    let needed_y0 = clip_pos[1] - padding;
    let needed_x1 = clip_pos[0] + clip_size[0] + padding;
    let needed_y1 = clip_pos[1] + clip_size[1] + padding;

    let cur_x1 = region.position[0] + region.size[0];
    let cur_y1 = region.position[1] + region.size[1];

    if needed_x0 < region.position[0] {
        region.size[0] += region.position[0] - needed_x0;
        region.position[0] = needed_x0;
    }
    if needed_y0 < region.position[1] {
        region.size[1] += region.position[1] - needed_y0;
        region.position[1] = needed_y0;
    }
    if needed_x1 > cur_x1 {
        region.size[0] = needed_x1 - region.position[0];
    }
    if needed_y1 > cur_y1 {
        region.size[1] = needed_y1 - region.position[1];
    }
}

// ---------------------------------------------------------------------------
// Snapshot (for undo/redo — no Arc fields)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InstrumentRegionSnapshot {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn build_instrument_region_instances(
    region: &InstrumentRegion,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    is_active: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let border_color = if is_active {
        INSTRUMENT_ACTIVE_BORDER
    } else {
        INSTRUMENT_BORDER_COLOR
    };

    let bw = if is_selected { 2.5 } else { 1.5 } / camera.zoom;
    let mut bc = border_color;
    if is_hovered && !is_selected {
        bc[3] = (bc[3] + 0.15).min(1.0);
    }
    push_border(&mut out, region.position, region.size, bw, bc);

    // Dashed top indicator
    let dash_h = 3.0 / camera.zoom;
    let dash_w = 20.0 / camera.zoom;
    let gap = 10.0 / camera.zoom;
    let y = region.position[1] - dash_h - 2.0 / camera.zoom;
    let mut x = region.position[0];
    while x < region.position[0] + region.size[0] {
        let w = dash_w.min(region.position[0] + region.size[0] - x);
        out.push(InstanceRaw {
            position: [x, y],
            size: [w, dash_h],
            color: [0.60, 0.30, 0.90, 0.40],
            border_radius: 1.0 / camera.zoom,
        });
        x += dash_w + gap;
    }

    // "INST" badge at top-left
    let badge_w = 36.0 / camera.zoom;
    let badge_h = 16.0 / camera.zoom;
    out.push(InstanceRaw {
        position: [
            region.position[0] + 4.0 / camera.zoom,
            region.position[1] + 4.0 / camera.zoom,
        ],
        size: [badge_w, badge_h],
        color: [0.60, 0.30, 0.90, 0.70],
        border_radius: 3.0 / camera.zoom,
    });

    // Corner resize handles when selected
    if is_selected {
        let handle_sz = 8.0 / camera.zoom;
        let handle_color = [0.60, 0.30, 0.90, 0.90];
        for &hx in &[
            region.position[0] - handle_sz * 0.5,
            region.position[0] + region.size[0] - handle_sz * 0.5,
        ] {
            for &hy in &[
                region.position[1] - handle_sz * 0.5,
                region.position[1] + region.size[1] - handle_sz * 0.5,
            ] {
                out.push(InstanceRaw {
                    position: [hx, hy],
                    size: [handle_sz, handle_sz],
                    color: handle_color,
                    border_radius: 2.0 / camera.zoom,
                });
            }
        }
    }

    out
}
