use crate::entity_id::EntityId;
use crate::{push_border, Camera, InstanceRaw, WaveformView};
use indexmap::IndexMap;

pub type ComponentId = EntityId;

use crate::theme::{
    COMPONENT_BORDER_COLOR, COMPONENT_FILL_COLOR, COMPONENT_BADGE_COLOR,
    INSTANCE_FILL_COLOR, INSTANCE_BORDER_COLOR, LOCK_ICON_COLOR,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComponentDef {
    pub id: ComponentId,
    pub name: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub waveform_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ComponentInstance {
    pub component_id: ComponentId,
    pub position: [f32; 2],
}

impl ComponentInstance {
    pub fn size_from_def(def: &ComponentDef) -> [f32; 2] {
        def.size
    }
}

pub fn build_component_def_instances(
    def: &ComponentDef,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let mut fill = COMPONENT_FILL_COLOR;
    if is_hovered || is_selected {
        fill[3] = (fill[3] + 0.04).min(1.0);
    }

    out.push(InstanceRaw {
        position: def.position,
        size: def.size,
        color: fill,
        border_radius: 6.0 / camera.zoom,
    });

    let bw = if is_selected { 2.5 } else { 1.5 } / camera.zoom;
    let mut bc = COMPONENT_BORDER_COLOR;
    if is_hovered && !is_selected {
        bc[3] = (bc[3] + 0.15).min(1.0);
    }
    push_border(&mut out, def.position, def.size, bw, bc);

    // Dashed top indicator
    let dash_h = 3.0 / camera.zoom;
    let dash_w = 20.0 / camera.zoom;
    let gap = 10.0 / camera.zoom;
    let y = def.position[1] - dash_h - 2.0 / camera.zoom;
    let mut x = def.position[0];
    while x < def.position[0] + def.size[0] {
        let w = dash_w.min(def.position[0] + def.size[0] - x);
        out.push(InstanceRaw {
            position: [x, y],
            size: [w, dash_h],
            color: [0.85, 0.55, 0.20, 0.40],
            border_radius: 1.0 / camera.zoom,
        });
        x += dash_w + gap;
    }

    // Component badge at top-left
    let badge_w = 20.0 / camera.zoom;
    let badge_h = 16.0 / camera.zoom;
    out.push(InstanceRaw {
        position: [
            def.position[0] + 4.0 / camera.zoom,
            def.position[1] + 4.0 / camera.zoom,
        ],
        size: [badge_w, badge_h],
        color: COMPONENT_BADGE_COLOR,
        border_radius: 3.0 / camera.zoom,
    });

    // Filled diamond inside badge (4 small triangles approximated as a rotated square)
    let diamond_sz = 7.0 / camera.zoom;
    let cx = def.position[0] + 4.0 / camera.zoom + badge_w * 0.5 - diamond_sz * 0.5;
    let cy = def.position[1] + 4.0 / camera.zoom + badge_h * 0.5 - diamond_sz * 0.5;
    out.push(InstanceRaw {
        position: [cx, cy],
        size: [diamond_sz, diamond_sz],
        color: [1.0, 1.0, 1.0, 0.90],
        border_radius: diamond_sz * 0.15,
    });

    // Resize handles at corners
    if is_selected {
        let handle_sz = 8.0 / camera.zoom;
        let handle_color = COMPONENT_BORDER_COLOR;
        for &hx in &[
            def.position[0] - handle_sz * 0.5,
            def.position[0] + def.size[0] - handle_sz * 0.5,
        ] {
            for &hy in &[
                def.position[1] - handle_sz * 0.5,
                def.position[1] + def.size[1] - handle_sz * 0.5,
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

pub fn build_component_instance_instances(
    inst: &ComponentInstance,
    def: &ComponentDef,
    waveforms: &IndexMap<EntityId, WaveformView>,
    camera: &Camera,
    world_left: f32,
    world_right: f32,
    is_hovered: bool,
    is_selected: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();
    let size = ComponentInstance::size_from_def(def);
    let offset = [
        inst.position[0] - def.position[0],
        inst.position[1] - def.position[1],
    ];

    // Instance fill
    let mut fill = INSTANCE_FILL_COLOR;
    if is_hovered || is_selected {
        fill[3] = (fill[3] + 0.04).min(1.0);
    }
    out.push(InstanceRaw {
        position: inst.position,
        size,
        color: fill,
        border_radius: 6.0 / camera.zoom,
    });

    // Instance border (dashed style via segments)
    let bw = if is_selected { 2.0 } else { 1.0 } / camera.zoom;
    let bc = if is_selected {
        let mut c = COMPONENT_BORDER_COLOR;
        c[3] = (c[3] + 0.2).min(1.0);
        c
    } else {
        INSTANCE_BORDER_COLOR
    };
    push_border(&mut out, inst.position, size, bw, bc);

    // Ghost waveforms: render each waveform belonging to the component at the offset position
    for &wf_id in &def.waveform_ids {
        let wf = match waveforms.get(&wf_id) {
            Some(wf) => wf,
            None => continue,
        };
        let ghost_pos = [wf.position[0] + offset[0], wf.position[1] + offset[1]];
        let ghost_right = ghost_pos[0] + wf.size[0];

        if ghost_right < world_left || ghost_pos[0] > world_right {
            continue;
        }

        // Background
        let bg_color = [
            wf.color[0] * 0.15,
            wf.color[1] * 0.15,
            wf.color[2] * 0.15,
            0.45,
        ];
        out.push(InstanceRaw {
            position: ghost_pos,
            size: wf.size,
            color: bg_color,
            border_radius: wf.border_radius,
        });

        // Waveform bars (simplified ghost rendering from samples)
        let bar_screen_px = 3.5;
        let bar_world = bar_screen_px / camera.zoom;
        let gap_world = 1.0 / camera.zoom;
        let step = bar_world + gap_world;
        let n_samples = wf.audio.left_samples.len();
        if n_samples == 0 || step <= 0.0 {
            continue;
        }
        let visible_left = world_left.max(ghost_pos[0]);
        let visible_right = world_right.min(ghost_pos[0] + wf.size[0]);
        let first = ((visible_left - ghost_pos[0]) / step).floor().max(0.0) as usize;
        let last = ((visible_right - ghost_pos[0]) / step).ceil().max(0.0) as usize;
        for bi in first..=last {
            let bx = ghost_pos[0] + bi as f32 * step;
            if bx + bar_world < visible_left || bx > visible_right {
                continue;
            }
            let t = if wf.size[0] > 0.0 {
                (bx - ghost_pos[0]) / wf.size[0]
            } else {
                0.0
            };
            let si = ((t * n_samples as f32) as usize).min(n_samples - 1);
            let window = (n_samples / 200).max(1);
            let s_start = si.saturating_sub(window / 2);
            let s_end = (si + window / 2).min(n_samples);
            let amp = wf.audio.left_samples[s_start..s_end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max)
                .clamp(0.0, 1.0);
            let bar_h = amp * wf.size[1] * 0.9;
            let by = ghost_pos[1] + (wf.size[1] - bar_h) * 0.5;
            let ghost_color = [wf.color[0], wf.color[1], wf.color[2], 0.35];
            out.push(InstanceRaw {
                position: [bx, by],
                size: [bar_world, bar_h.max(1.0 / camera.zoom)],
                color: ghost_color,
                border_radius: bar_world * 0.3,
            });
        }
    }

    // Lock icon (padlock shape) at top-right
    let lock_sz = 14.0 / camera.zoom;
    let lock_x = inst.position[0] + size[0] - lock_sz - 4.0 / camera.zoom;
    let lock_y = inst.position[1] + 4.0 / camera.zoom;

    // Lock body
    let body_w = lock_sz;
    let body_h = lock_sz * 0.6;
    let body_y = lock_y + lock_sz * 0.4;
    out.push(InstanceRaw {
        position: [lock_x, body_y],
        size: [body_w, body_h],
        color: LOCK_ICON_COLOR,
        border_radius: 2.0 / camera.zoom,
    });

    // Lock shackle (top arc approximated as a smaller rect)
    let shackle_w = lock_sz * 0.55;
    let shackle_h = lock_sz * 0.45;
    let shackle_x = lock_x + (body_w - shackle_w) * 0.5;
    out.push(InstanceRaw {
        position: [shackle_x, lock_y],
        size: [shackle_w, shackle_h],
        color: LOCK_ICON_COLOR,
        border_radius: shackle_w * 0.5,
    });
    // Hollow center of shackle
    let inner_w = shackle_w * 0.5;
    let inner_h = shackle_h * 0.6;
    let inner_x = shackle_x + (shackle_w - inner_w) * 0.5;
    let inner_y = lock_y + 1.5 / camera.zoom;
    out.push(InstanceRaw {
        position: [inner_x, inner_y],
        size: [inner_w, inner_h],
        color: INSTANCE_FILL_COLOR,
        border_radius: inner_w * 0.5,
    });

    out
}

pub fn bounding_box_of_waveforms(
    waveforms: &IndexMap<EntityId, WaveformView>,
    ids: &[EntityId],
) -> ([f32; 2], [f32; 2]) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for &id in ids {
        let wf = match waveforms.get(&id) {
            Some(wf) => wf,
            None => continue,
        };
        min_x = min_x.min(wf.position[0]);
        min_y = min_y.min(wf.position[1]);
        max_x = max_x.max(wf.position[0] + wf.size[0]);
        max_y = max_y.max(wf.position[1] + wf.size[1]);
    }
    let padding = 10.0;
    let pos = [min_x - padding, min_y - padding];
    let size = [
        (max_x - min_x) + padding * 2.0,
        (max_y - min_y) + padding * 2.0,
    ];
    (pos, size)
}
