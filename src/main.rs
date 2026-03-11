mod audio;
mod browser;
mod context_menu;
mod palette;
mod storage;
mod waveform;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use audio::{load_audio_file, AudioClipData, AudioEngine, AudioRecorder, PIXELS_PER_SECOND};
pub(crate) use waveform::WaveformObject;
use context_menu::{
    ContextMenu, ContextMenuEntry, CTX_MENU_ITEM_HEIGHT, CTX_MENU_PADDING,
    CTX_MENU_SEPARATOR_HEIGHT, CTX_MENU_WIDTH,
};
use palette::{
    CommandAction, CommandPalette, PaletteMode, PaletteRow, COMMANDS, PALETTE_INPUT_HEIGHT,
    PALETTE_ITEM_HEIGHT, PALETTE_PADDING, PALETTE_SECTION_HEIGHT, PALETTE_WIDTH,
};

use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer as TextBuffer, Color as TextColor, Family, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use surrealdb::types::SurrealValue;
use wgpu::util::DeviceExt;

use storage::{default_db_path, ProjectState, Storage};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, Window, WindowId},
};

// ---------------------------------------------------------------------------
// Shader (WGSL)
// ---------------------------------------------------------------------------

const SHADER_SRC: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) border_radius: f32,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) obj_pos: vec2<f32>,
    @location(2) obj_size: vec2<f32>,
    @location(3) obj_color: vec4<f32>,
    @location(4) radius: f32,
) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = obj_pos + position * obj_size;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.color = obj_color;
    out.local_pos = position * obj_size;
    out.rect_size = obj_size;
    out.border_radius = radius;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let r = min(in.border_radius, min(in.rect_size.x, in.rect_size.y) * 0.5);
    if (r < 0.01) {
        return in.color;
    }
    let center = in.rect_size * 0.5;
    let p = in.local_pos - center;
    let d = rounded_box_sdf(p, center, r);
    let fw = fwidth(d);
    let alpha = 1.0 - smoothstep(0.0, fw, d);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

// ---------------------------------------------------------------------------
// GPU data types
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
}

const QUAD_VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0],
    },
    Vertex {
        position: [0.0, 1.0],
    },
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct InstanceRaw {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

const MAX_INSTANCES: usize = 16384;

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

pub(crate) struct Camera {
    pub(crate) position: [f32; 2],
    pub(crate) zoom: f32,
}

impl Camera {
    fn new() -> Self {
        Self {
            position: [-100.0, -50.0],
            zoom: 1.0,
        }
    }

    fn view_proj(&self, width: f32, height: f32) -> [[f32; 4]; 4] {
        let z = self.zoom;
        let cx = self.position[0];
        let cy = self.position[1];
        [
            [2.0 * z / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 * z / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [
                -2.0 * z * cx / width - 1.0,
                2.0 * z * cy / height + 1.0,
                0.0,
                1.0,
            ],
        ]
    }

    fn screen_to_world(&self, screen: [f32; 2]) -> [f32; 2] {
        [
            screen[0] / self.zoom + self.position[0],
            screen[1] / self.zoom + self.position[1],
        ]
    }

    fn zoom_at(&mut self, screen_pos: [f32; 2], factor: f32) {
        let world = self.screen_to_world(screen_pos);
        self.zoom = (self.zoom * factor).clamp(0.05, 200.0);
        self.position[0] = world[0] - screen_pos[0] / self.zoom;
        self.position[1] = world[1] - screen_pos[1] / self.zoom;
    }
}

fn screen_ortho(width: f32, height: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}

// ---------------------------------------------------------------------------
// Canvas objects
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, SurrealValue)]
pub struct CanvasObject {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HitTarget {
    Object(usize),
    Waveform(usize),
}

const MAX_UNDO_HISTORY: usize = 50;

#[derive(Clone)]
struct Snapshot {
    objects: Vec<CanvasObject>,
    waveforms: Vec<WaveformObject>,
    audio_clips: Vec<AudioClipData>,
}

enum DragState {
    None,
    Panning {
        start_mouse: [f32; 2],
        start_camera: [f32; 2],
    },
    Selecting {
        start_world: [f32; 2],
    },
    MovingSelection {
        offsets: Vec<(HitTarget, [f32; 2])>,
    },
    DraggingFromBrowser {
        path: PathBuf,
        filename: String,
    },
    ResizingBrowser,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const WAVEFORM_COLORS: &[[f32; 4]] = &[
    [0.40, 0.72, 1.00, 1.0],
    [1.00, 0.55, 0.35, 1.0],
    [0.45, 0.92, 0.55, 1.0],
    [0.92, 0.45, 0.80, 1.0],
    [1.00, 0.85, 0.32, 1.0],
];

const SEL_COLOR: [f32; 4] = [0.35, 0.65, 1.0, 0.8];

// Audio formats supported via symphonia: wav, mp3, ogg, flac, aac
const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aac", "m4a", "mp4"];

// ---------------------------------------------------------------------------
// Transport Panel (bottom-center playback status)
// ---------------------------------------------------------------------------

const TRANSPORT_WIDTH: f32 = 210.0;
const TRANSPORT_HEIGHT: f32 = 36.0;
const TRANSPORT_BOTTOM_MARGIN: f32 = 32.0;

struct TransportPanel;

impl TransportPanel {
    fn panel_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = TRANSPORT_WIDTH * scale;
        let h = TRANSPORT_HEIGHT * scale;
        let x = (screen_w - w) * 0.5;
        let y = screen_h - h - TRANSPORT_BOTTOM_MARGIN * scale;
        ([x, y], [w, h])
    }

    fn record_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 24.0 * scale;
        let btn_x = pos[0] + size[0] - btn_size - 8.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    fn build_instances(
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        is_playing: bool,
        is_recording: bool,
    ) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);

        // background pill
        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.12, 0.12, 0.16, 0.85],
            border_radius: size[1] * 0.5,
        });

        let icon_x = pos[0] + 14.0 * scale;
        let icon_cy = pos[1] + size[1] * 0.5;

        if is_playing {
            let bar_w = 3.0 * scale;
            let bar_h = 12.0 * scale;
            let gap = 4.0 * scale;
            out.push(InstanceRaw {
                position: [icon_x, icon_cy - bar_h * 0.5],
                size: [bar_w, bar_h],
                color: [1.0, 1.0, 1.0, 0.9],
                border_radius: 1.0 * scale,
            });
            out.push(InstanceRaw {
                position: [icon_x + bar_w + gap, icon_cy - bar_h * 0.5],
                size: [bar_w, bar_h],
                color: [1.0, 1.0, 1.0, 0.9],
                border_radius: 1.0 * scale,
            });
        } else {
            let tri_w = 11.0 * scale;
            let tri_h = 13.0 * scale;
            let steps = 5;
            let step_h = tri_h / steps as f32;
            for i in 0..steps {
                let t_top = i as f32 / steps as f32;
                let t_bot = (i + 1) as f32 / steps as f32;
                let w_top = tri_w * (1.0 - (2.0 * t_top - 1.0).abs());
                let w_bot = tri_w * (1.0 - (2.0 * t_bot - 1.0).abs());
                let w = w_top.max(w_bot);
                let sy = icon_cy - tri_h * 0.5 + i as f32 * step_h;
                out.push(InstanceRaw {
                    position: [icon_x, sy],
                    size: [w, step_h],
                    color: [1.0, 1.0, 1.0, 0.9],
                    border_radius: 0.0,
                });
            }
        }

        // record button: red circle (brighter when recording)
        let (rbtn_pos, rbtn_size) = Self::record_button_rect(screen_w, screen_h, scale);
        let dot_diameter = 12.0 * scale;
        let dot_x = rbtn_pos[0] + (rbtn_size[0] - dot_diameter) * 0.5;
        let dot_y = rbtn_pos[1] + (rbtn_size[1] - dot_diameter) * 0.5;

        if is_recording {
            // stop icon: rounded red square
            let sq = 10.0 * scale;
            let sq_x = rbtn_pos[0] + (rbtn_size[0] - sq) * 0.5;
            let sq_y = rbtn_pos[1] + (rbtn_size[1] - sq) * 0.5;
            out.push(InstanceRaw {
                position: [sq_x, sq_y],
                size: [sq, sq],
                color: [0.95, 0.2, 0.2, 1.0],
                border_radius: 2.0 * scale,
            });
        } else {
            out.push(InstanceRaw {
                position: [dot_x, dot_y],
                size: [dot_diameter, dot_diameter],
                color: [0.85, 0.25, 0.25, 0.9],
                border_radius: dot_diameter * 0.5,
            });
        }

        out
    }

    fn contains(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::panel_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    fn hit_record_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::record_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }
}

fn format_playback_time(secs: f64) -> String {
    let minutes = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{}:{:04.1}", minutes, s)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn grid_spacing(zoom: f32) -> f32 {
    let target_px = 80.0;
    let world = target_px / zoom;
    let mag = 10f32.powf(world.log10().floor());
    let norm = world / mag;
    if norm <= 1.0 {
        mag
    } else if norm <= 2.0 {
        2.0 * mag
    } else if norm <= 5.0 {
        5.0 * mag
    } else {
        10.0 * mag
    }
}

fn point_in_rect(pos: [f32; 2], rect_pos: [f32; 2], rect_size: [f32; 2]) -> bool {
    pos[0] >= rect_pos[0]
        && pos[0] <= rect_pos[0] + rect_size[0]
        && pos[1] >= rect_pos[1]
        && pos[1] <= rect_pos[1] + rect_size[1]
}

fn rects_overlap(a_pos: [f32; 2], a_size: [f32; 2], b_pos: [f32; 2], b_size: [f32; 2]) -> bool {
    a_pos[0] < b_pos[0] + b_size[0]
        && a_pos[0] + a_size[0] > b_pos[0]
        && a_pos[1] < b_pos[1] + b_size[1]
        && a_pos[1] + a_size[1] > b_pos[1]
}

fn canonical_rect(a: [f32; 2], b: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let w = (a[0] - b[0]).abs();
    let h = (a[1] - b[1]).abs();
    ([x, y], [w, h])
}

fn hit_test(
    objects: &[CanvasObject],
    waveforms: &[WaveformObject],
    world_pos: [f32; 2],
) -> Option<HitTarget> {
    for (i, wf) in waveforms.iter().enumerate().rev() {
        if point_in_rect(world_pos, wf.position, wf.size) {
            return Some(HitTarget::Waveform(i));
        }
    }
    for (i, obj) in objects.iter().enumerate().rev() {
        if point_in_rect(world_pos, obj.position, obj.size) {
            return Some(HitTarget::Object(i));
        }
    }
    None
}

fn targets_in_rect(
    objects: &[CanvasObject],
    waveforms: &[WaveformObject],
    rect_pos: [f32; 2],
    rect_size: [f32; 2],
) -> Vec<HitTarget> {
    let mut result = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, obj.position, obj.size) {
            result.push(HitTarget::Object(i));
        }
    }
    for (i, wf) in waveforms.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, wf.position, wf.size) {
            result.push(HitTarget::Waveform(i));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Instance building
// ---------------------------------------------------------------------------

struct RenderContext<'a> {
    camera: &'a Camera,
    screen_w: f32,
    screen_h: f32,
    objects: &'a [CanvasObject],
    waveforms: &'a [WaveformObject],
    hovered: Option<HitTarget>,
    selected: &'a [HitTarget],
    selection_rect: Option<([f32; 2], [f32; 2])>,
    file_hovering: bool,
    playhead_world_x: Option<f32>,
}

fn build_instances(ctx: &RenderContext) -> Vec<InstanceRaw> {
    let mut out = Vec::with_capacity(1024);

    let camera = ctx.camera;
    let world_left = camera.position[0];
    let world_top = camera.position[1];
    let world_right = world_left + ctx.screen_w / camera.zoom;
    let world_bottom = world_top + ctx.screen_h / camera.zoom;

    // --- adaptive grid ---
    let spacing = grid_spacing(camera.zoom);
    let line_w = 1.0 / camera.zoom;
    let major_line_w = 2.0 / camera.zoom;
    let minor_color = [1.0, 1.0, 1.0, 0.06];
    let major_color = [1.0, 1.0, 1.0, 0.14];

    let first_xi = (world_left / spacing).floor() as i64;
    let last_xi = (world_right / spacing).ceil() as i64;
    for i in first_xi..=last_xi {
        let x = i as f32 * spacing;
        let is_major = i % 5 == 0;
        let w = if is_major { major_line_w } else { line_w };
        let c = if is_major { major_color } else { minor_color };
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
        let is_major = i % 5 == 0;
        let w = if is_major { major_line_w } else { line_w };
        let c = if is_major { major_color } else { minor_color };
        out.push(InstanceRaw {
            position: [world_left, y - w * 0.5],
            size: [world_right - world_left, w],
            color: c,
            border_radius: 0.0,
        });
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

    // --- canvas objects ---
    for (i, obj) in ctx.objects.iter().enumerate() {
        let is_sel = ctx.selected.contains(&HitTarget::Object(i));
        let is_hov = ctx.hovered == Some(HitTarget::Object(i));
        let mut color = obj.color;
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

    // --- waveforms ---
    for (i, wf) in ctx.waveforms.iter().enumerate() {
        let is_sel = ctx.selected.contains(&HitTarget::Waveform(i));
        let is_hov = ctx.hovered == Some(HitTarget::Waveform(i));
        out.extend(waveform::build_waveform_instances(
            wf, camera, world_left, world_right, is_hov, is_sel,
        ));
    }

    // --- selection highlights (rendered on top of everything) ---
    let sel_bw = 2.0 / camera.zoom;
    let handle_sz = 8.0 / camera.zoom;
    for target in ctx.selected {
        let (pos, size) = target_rect(ctx.objects, ctx.waveforms, target);
        push_border(&mut out, pos, size, sel_bw, SEL_COLOR);

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

    // --- selection rectangle ---
    if let Some((start, current)) = ctx.selection_rect {
        let (rp, rs) = canonical_rect(start, current);
        out.push(InstanceRaw {
            position: rp,
            size: rs,
            color: [0.30, 0.55, 1.0, 0.10],
            border_radius: 0.0,
        });
        let bw = 1.0 / camera.zoom;
        push_border(&mut out, rp, rs, bw, [0.35, 0.65, 1.0, 0.5]);
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
            &mut out,
            [world_left, world_top],
            [world_right - world_left, world_bottom - world_top],
            bw,
            [0.35, 0.65, 1.0, 0.7],
        );
    }

    out
}

pub(crate) fn push_border(
    out: &mut Vec<InstanceRaw>,
    pos: [f32; 2],
    size: [f32; 2],
    bw: f32,
    color: [f32; 4],
) {
    out.push(InstanceRaw {
        position: pos,
        size: [size[0], bw],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: [pos[0], pos[1] + size[1] - bw],
        size: [size[0], bw],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: pos,
        size: [bw, size[1]],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: [pos[0] + size[0] - bw, pos[1]],
        size: [bw, size[1]],
        color,
        border_radius: 0.0,
    });
}

fn target_rect(
    objects: &[CanvasObject],
    waveforms: &[WaveformObject],
    target: &HitTarget,
) -> ([f32; 2], [f32; 2]) {
    match target {
        HitTarget::Object(i) => (objects[*i].position, objects[*i].size),
        HitTarget::Waveform(i) => (waveforms[*i].position, waveforms[*i].size),
    }
}

fn default_objects() -> Vec<CanvasObject> {
    vec![
        CanvasObject {
            position: [80.0, 60.0],
            size: [220.0, 160.0],
            color: [0.95, 0.30, 0.30, 1.0],
            border_radius: 12.0,
        },
        CanvasObject {
            position: [400.0, 100.0],
            size: [180.0, 180.0],
            color: [0.20, 0.50, 0.95, 1.0],
            border_radius: 12.0,
        },
        CanvasObject {
            position: [200.0, 320.0],
            size: [260.0, 130.0],
            color: [0.20, 0.85, 0.50, 1.0],
            border_radius: 12.0,
        },
        CanvasObject {
            position: [550.0, 300.0],
            size: [160.0, 210.0],
            color: [0.95, 0.75, 0.20, 1.0],
            border_radius: 12.0,
        },
        CanvasObject {
            position: [30.0, 350.0],
            size: [150.0, 150.0],
            color: [0.70, 0.30, 0.90, 1.0],
            border_radius: 60.0,
        },
    ]
}

// ---------------------------------------------------------------------------
// GPU state
// ---------------------------------------------------------------------------

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    screen_camera_buffer: wgpu::Buffer,
    screen_camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,

    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: Viewport,
    scale_factor: f32,

    browser_text_buffers: Vec<TextBuffer>,
    browser_text_generation: u64,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No suitable GPU adapter found");

        log::info!("GPU adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let screen_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let screen_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[vertex_layout, instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad vertices"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance buffer"),
            size: (MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- glyphon text rendering ---
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let viewport = Viewport::new(&device, &cache);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            camera_buffer,
            camera_bind_group,
            screen_camera_buffer,
            screen_camera_bind_group,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            viewport,
            scale_factor,
            browser_text_buffers: Vec::new(),
            browser_text_generation: 0,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn render(
        &mut self,
        camera: &Camera,
        world_instances: &[InstanceRaw],
        command_palette: Option<&CommandPalette>,
        context_menu: Option<&ContextMenu>,
        sample_browser: Option<&browser::SampleBrowser>,
        browser_drag_ghost: Option<(&str, [f32; 2])>,
        is_playing: bool,
        is_recording: bool,
        playback_position: f64,
    ) {
        let w = self.config.width as f32;
        let h = self.config.height as f32;
        if w < 1.0 || h < 1.0 {
            return;
        }

        let cam_uniform = CameraUniform {
            view_proj: camera.view_proj(w, h),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[cam_uniform]));

        let screen_cam = CameraUniform {
            view_proj: screen_ortho(w, h),
        };
        self.queue.write_buffer(
            &self.screen_camera_buffer,
            0,
            bytemuck::cast_slice(&[screen_cam]),
        );

        let world_count = world_instances.len().min(MAX_INSTANCES);
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&world_instances[..world_count]),
        );

        // Build overlay instances: browser panel + drag ghost + command palette
        let mut overlay_instances: Vec<InstanceRaw> = Vec::new();

        if let Some(br) = sample_browser {
            overlay_instances.extend(br.build_instances(w, h, self.scale_factor));
        }

        if let Some((_, pos)) = browser_drag_ghost {
            overlay_instances.push(InstanceRaw {
                position: [pos[0] - 4.0, pos[1] - 4.0],
                size: [160.0 * self.scale_factor, 24.0 * self.scale_factor],
                color: [0.20, 0.20, 0.28, 0.90],
                border_radius: 4.0 * self.scale_factor,
            });
        }

        if let Some(p) = command_palette {
            overlay_instances.extend(p.build_instances(w, h, self.scale_factor));
        }

        if let Some(cm) = context_menu {
            overlay_instances.extend(cm.build_instances(w, h, self.scale_factor));
        }

        overlay_instances.extend(TransportPanel::build_instances(
            w,
            h,
            self.scale_factor,
            is_playing,
            is_recording,
        ));

        let overlay_count = overlay_instances.len().min(MAX_INSTANCES - world_count);
        if overlay_count > 0 {
            let offset = (world_count * std::mem::size_of::<InstanceRaw>()) as u64;
            self.queue.write_buffer(
                &self.instance_buffer,
                offset,
                bytemuck::cast_slice(&overlay_instances[..overlay_count]),
            );
        }

        // --- prepare text ---
        let scale = self.scale_factor;
        let mut text_buffers: Vec<TextBuffer> = Vec::new();
        let mut text_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();

        let full_bounds = TextBounds {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };

        // Browser text: shape ALL entries once, positions computed each frame
        if let Some(br) = sample_browser {
            if br.text_generation != self.browser_text_generation {
                self.browser_text_buffers.clear();
                for te in &br.cached_text {
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(te.font_size, te.line_height),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(te.max_width),
                        Some(te.line_height),
                    );
                    let attrs = Attrs::new()
                        .family(Family::Name(".AppleSystemUIFont"))
                        .weight(glyphon::Weight(te.weight));
                    buf.set_text(&mut self.font_system, &te.text, attrs, Shaping::Advanced);
                    buf.shape_until_scroll(&mut self.font_system, false);
                    self.browser_text_buffers.push(buf);
                }
                self.browser_text_generation = br.text_generation;
            }
        } else if !self.browser_text_buffers.is_empty() {
            self.browser_text_buffers.clear();
        }

        // Drag ghost text
        if let Some((label, pos)) = browser_drag_ghost {
            let font_sz = 12.0 * scale;
            let line_h = 16.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(font_sz, line_h));
            buf.set_size(&mut self.font_system, Some(150.0 * scale), Some(line_h));
            buf.set_text(
                &mut self.font_system,
                label,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                pos[0] + 4.0 * scale,
                pos[1] - 4.0 + (24.0 * scale - line_h) * 0.5,
                TextColor::rgb(220, 220, 230),
                full_bounds,
            ));
        }

        if let Some(palette) = command_palette {
            let (ppos, _psize) = palette.palette_rect(w, h, scale);
            let margin = PALETTE_PADDING * scale;
            let list_top = ppos[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;

            // Search input text (or placeholder)
            let (display_text, search_color) = match palette.mode {
                PaletteMode::VolumeFader => {
                    ("Master Volume", TextColor::rgb(235, 235, 240))
                }
                _ if palette.search_text.is_empty() => {
                    ("Search", TextColor::rgba(140, 140, 150, 160))
                }
                _ => {
                    (palette.search_text.as_str(), TextColor::rgb(235, 235, 240))
                }
            };
            let sfont = 15.0 * scale;
            let sline = 22.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(sfont, sline));
            buf.set_size(
                &mut self.font_system,
                Some(PALETTE_WIDTH * scale - 60.0 * scale),
                Some(PALETTE_INPUT_HEIGHT * scale),
            );
            buf.set_text(
                &mut self.font_system,
                display_text,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                ppos[0] + 36.0 * scale,
                ppos[1] + (PALETTE_INPUT_HEIGHT * scale - sline) * 0.5,
                search_color,
                full_bounds,
            ));

            match palette.mode {
                PaletteMode::VolumeFader => {
                    let pad = 16.0 * scale;
                    let track_y = list_top + 36.0 * scale;
                    let track_h = 6.0 * scale;
                    let rms_y = track_y + track_h + 22.0 * scale;

                    let pct = (palette.fader_value * 100.0) as u32;
                    let vol_text = format!("{}%", pct);
                    let label_font = 13.0 * scale;
                    let label_line = 18.0 * scale;
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(label_font, label_line),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(PALETTE_WIDTH * scale - margin * 2.0),
                        Some(20.0 * scale),
                    );
                    buf.set_text(
                        &mut self.font_system,
                        &vol_text,
                        Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        ppos[0] + margin + pad,
                        list_top + 14.0 * scale,
                        TextColor::rgba(200, 200, 210, 220),
                        full_bounds,
                    ));

                    let db_val = if palette.fader_rms > 0.0001 {
                        20.0 * palette.fader_rms.log10()
                    } else {
                        -60.0
                    };
                    let rms_text = format!("RMS: {:.1} dB", db_val);
                    let small_font = 11.0 * scale;
                    let small_line = 15.0 * scale;
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(small_font, small_line),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(PALETTE_WIDTH * scale - margin * 2.0),
                        Some(16.0 * scale),
                    );
                    buf.set_text(
                        &mut self.font_system,
                        &rms_text,
                        Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        ppos[0] + margin + pad,
                        rms_y + 8.0 * scale,
                        TextColor::rgba(140, 140, 150, 180),
                        full_bounds,
                    ));
                }
                PaletteMode::Commands => {
                    let sect_font = 11.0 * scale;
                    let sect_line = 16.0 * scale;
                    let ifont = 13.5 * scale;
                    let iline = 20.0 * scale;
                    let shortcut_font = 12.0 * scale;
                    let shortcut_line = 17.0 * scale;

                    let mut y = list_top;
                    for row in palette.visible_rows() {
                        match row {
                            PaletteRow::Section(label) => {
                                let mut buf = TextBuffer::new(
                                    &mut self.font_system,
                                    Metrics::new(sect_font, sect_line),
                                );
                                buf.set_size(
                                    &mut self.font_system,
                                    Some(PALETTE_WIDTH * scale - margin * 4.0),
                                    Some(PALETTE_SECTION_HEIGHT * scale),
                                );
                                buf.set_text(
                                    &mut self.font_system,
                                    label,
                                    Attrs::new().family(Family::SansSerif),
                                    Shaping::Advanced,
                                );
                                buf.shape_until_scroll(&mut self.font_system, false);
                                text_buffers.push(buf);
                                text_meta.push((
                                    ppos[0] + margin + 12.0 * scale,
                                    y + (PALETTE_SECTION_HEIGHT * scale - sect_line) * 0.5
                                        + 2.0 * scale,
                                    TextColor::rgba(120, 140, 170, 200),
                                    full_bounds,
                                ));
                                y += PALETTE_SECTION_HEIGHT * scale;
                            }
                            PaletteRow::Command(ci) => {
                                let cmd = &COMMANDS[*ci];

                                let mut buf = TextBuffer::new(
                                    &mut self.font_system,
                                    Metrics::new(ifont, iline),
                                );
                                buf.set_size(
                                    &mut self.font_system,
                                    Some(PALETTE_WIDTH * scale * 0.65),
                                    Some(PALETTE_ITEM_HEIGHT * scale),
                                );
                                buf.set_text(
                                    &mut self.font_system,
                                    cmd.name,
                                    Attrs::new().family(Family::SansSerif),
                                    Shaping::Advanced,
                                );
                                buf.shape_until_scroll(&mut self.font_system, false);
                                text_buffers.push(buf);
                                text_meta.push((
                                    ppos[0] + margin + 12.0 * scale,
                                    y + (PALETTE_ITEM_HEIGHT * scale - iline) * 0.5,
                                    TextColor::rgb(215, 215, 222),
                                    full_bounds,
                                ));

                                if !cmd.shortcut.is_empty() {
                                    let mut buf = TextBuffer::new(
                                        &mut self.font_system,
                                        Metrics::new(shortcut_font, shortcut_line),
                                    );
                                    buf.set_size(
                                        &mut self.font_system,
                                        Some(80.0 * scale),
                                        Some(PALETTE_ITEM_HEIGHT * scale),
                                    );
                                    buf.set_text(
                                        &mut self.font_system,
                                        cmd.shortcut,
                                        Attrs::new().family(Family::SansSerif),
                                        Shaping::Advanced,
                                    );
                                    buf.shape_until_scroll(&mut self.font_system, false);
                                    text_buffers.push(buf);
                                    text_meta.push((
                                        ppos[0] + PALETTE_WIDTH * scale - margin - 70.0 * scale,
                                        y + (PALETTE_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                                        TextColor::rgba(120, 120, 135, 180),
                                        full_bounds,
                                    ));
                                }

                                y += PALETTE_ITEM_HEIGHT * scale;
                            }
                        }
                    }
                }
            }
        }

        if let Some(cm) = context_menu {
            let (mpos, _msize) = cm.menu_rect(w, h, scale);
            let pad = CTX_MENU_PADDING * scale;
            let label_font = 13.0 * scale;
            let label_line = 18.0 * scale;
            let shortcut_font = 12.0 * scale;
            let shortcut_line = 17.0 * scale;

            let mut y = mpos[1] + pad;
            for entry in &cm.entries {
                match entry {
                    ContextMenuEntry::Item(item) => {
                        let mut buf = TextBuffer::new(
                            &mut self.font_system,
                            Metrics::new(label_font, label_line),
                        );
                        buf.set_size(
                            &mut self.font_system,
                            Some(CTX_MENU_WIDTH * scale * 0.65),
                            Some(CTX_MENU_ITEM_HEIGHT * scale),
                        );
                        buf.set_text(
                            &mut self.font_system,
                            item.label,
                            Attrs::new().family(Family::SansSerif),
                            Shaping::Advanced,
                        );
                        buf.shape_until_scroll(&mut self.font_system, false);
                        text_buffers.push(buf);
                        text_meta.push((
                            mpos[0] + pad + 10.0 * scale,
                            y + (CTX_MENU_ITEM_HEIGHT * scale - label_line) * 0.5,
                            TextColor::rgb(220, 220, 228),
                            full_bounds,
                        ));

                        if !item.shortcut.is_empty() {
                            let mut buf = TextBuffer::new(
                                &mut self.font_system,
                                Metrics::new(shortcut_font, shortcut_line),
                            );
                            buf.set_size(
                                &mut self.font_system,
                                Some(60.0 * scale),
                                Some(CTX_MENU_ITEM_HEIGHT * scale),
                            );
                            buf.set_text(
                                &mut self.font_system,
                                item.shortcut,
                                Attrs::new().family(Family::SansSerif),
                                Shaping::Advanced,
                            );
                            buf.shape_until_scroll(&mut self.font_system, false);
                            text_buffers.push(buf);
                            text_meta.push((
                                mpos[0] + CTX_MENU_WIDTH * scale - pad - 50.0 * scale,
                                y + (CTX_MENU_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                                TextColor::rgba(120, 120, 135, 180),
                                full_bounds,
                            ));
                        }

                        y += CTX_MENU_ITEM_HEIGHT * scale;
                    }
                    ContextMenuEntry::Separator => {
                        y += CTX_MENU_SEPARATOR_HEIGHT * scale;
                    }
                }
            }
        }

        // Transport panel time text
        {
            let (tp_pos, tp_size) = TransportPanel::panel_rect(w, h, scale);
            let time_str = format_playback_time(playback_position);
            let tfont = 13.0 * scale;
            let tline = 18.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(tfont, tline));
            buf.set_size(
                &mut self.font_system,
                Some(TRANSPORT_WIDTH * scale * 0.6),
                Some(tline),
            );
            buf.set_text(
                &mut self.font_system,
                &time_str,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                tp_pos[0] + 38.0 * scale,
                tp_pos[1] + (tp_size[1] - tline) * 0.5,
                TextColor::rgba(220, 220, 230, 220),
                full_bounds,
            ));
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );

        let mut browser_text_areas: Vec<TextArea> = Vec::new();
        if let Some(br) = sample_browser {
            let panel_w = br.panel_width(scale);
            let header_h = browser::HEADER_HEIGHT * scale;
            for (idx, te) in br.cached_text.iter().enumerate() {
                if idx >= self.browser_text_buffers.len() {
                    break;
                }
                let actual_y = if te.is_header {
                    te.base_y
                } else {
                    te.base_y - br.scroll_offset
                };
                if !te.is_header && (actual_y + te.line_height < header_h || actual_y > h) {
                    continue;
                }
                let clip_top = if actual_y < header_h {
                    header_h
                } else {
                    actual_y
                };
                browser_text_areas.push(TextArea {
                    buffer: &self.browser_text_buffers[idx],
                    left: te.x,
                    top: actual_y,
                    scale: 1.0,
                    default_color: TextColor::rgba(
                        te.color[0],
                        te.color[1],
                        te.color[2],
                        te.color[3],
                    ),
                    bounds: TextBounds {
                        left: 0,
                        top: clip_top as i32,
                        right: (panel_w - 8.0 * scale) as i32,
                        bottom: (actual_y + te.line_height) as i32,
                    },
                    custom_glyphs: &[],
                });
            }
        }

        let other_areas = text_buffers.iter().zip(text_meta.iter()).map(
            |(buffer, &(left, top, color, bounds))| TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            },
        );

        let text_areas: Vec<TextArea> = browser_text_areas.into_iter().chain(other_areas).collect();

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();

        // --- render pass ---
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.09,
                            g: 0.09,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..QUAD_INDICES.len() as u32, 0, 0..world_count as u32);

            if overlay_count > 0 {
                pass.set_bind_group(0, &self.screen_camera_bind_group, &[]);
                pass.draw_indexed(
                    0..QUAD_INDICES.len() as u32,
                    0,
                    world_count as u32..(world_count + overlay_count) as u32,
                );
            }

            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut pass)
                .unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

struct App {
    gpu: Option<Gpu>,
    camera: Camera,
    objects: Vec<CanvasObject>,
    waveforms: Vec<WaveformObject>,
    audio_clips: Vec<AudioClipData>,
    audio_engine: Option<AudioEngine>,
    recorder: Option<AudioRecorder>,
    recording_waveform_idx: Option<usize>,
    last_canvas_click_world: [f32; 2],
    selected: Vec<HitTarget>,
    drag: DragState,
    mouse_pos: [f32; 2],
    hovered: Option<HitTarget>,
    file_hovering: bool,
    modifiers: ModifiersState,
    command_palette: Option<CommandPalette>,
    context_menu: Option<ContextMenu>,
    sample_browser: browser::SampleBrowser,
    storage: Option<Storage>,
    has_saved_state: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    current_project_id: String,
    current_project_name: String,
}

impl App {
    fn new() -> Self {
        let project_id = "default".to_string();
        let db_path = default_db_path();
        println!("  Database: {}", db_path.display());

        let storage = Storage::open(&db_path);

        if let Some(s) = &storage {
            let projects = s.list_projects();
            if !projects.is_empty() {
                println!("  Projects:");
                for p in &projects {
                    let marker = if p.project_id == project_id { " *" } else { "" };
                    println!("    - {} ({}){}", p.name, p.project_id, marker);
                }
            }
        }

        let loaded = storage.as_ref().and_then(|s| s.load(&project_id));
        let has_saved_state = loaded.is_some();
        let (
            camera,
            objects,
            waveforms,
            project_name,
            browser_folders,
            browser_width,
            browser_visible,
            browser_expanded,
        ) = match loaded {
            Some(state) => {
                println!(
                    "  Loaded project '{}' ({} objects, {} waveforms)",
                    state.name,
                    state.objects.len(),
                    state.waveforms.len()
                );
                let cam = Camera {
                    position: state.camera_position,
                    zoom: state.camera_zoom,
                };
                let name = state.name.clone();
                let folders: Vec<PathBuf> =
                    state.browser_folders.iter().map(PathBuf::from).collect();
                let bw = if state.browser_width > 0.0 {
                    state.browser_width
                } else {
                    260.0
                };
                let expanded: HashSet<PathBuf> =
                    state.browser_expanded.iter().map(PathBuf::from).collect();
                (
                    cam,
                    state.objects,
                    state.waveforms,
                    name,
                    folders,
                    bw,
                    state.browser_visible,
                    Some(expanded),
                )
            }
            None => {
                println!("  No saved project found, starting fresh");
                (
                    Camera::new(),
                    default_objects(),
                    Vec::new(),
                    "Untitled".to_string(),
                    Vec::new(),
                    260.0,
                    false,
                    None,
                )
            }
        };

        let mut sample_browser = if let Some(expanded) = browser_expanded {
            browser::SampleBrowser::from_state(browser_folders, expanded, browser_visible)
        } else {
            browser::SampleBrowser::from_folders(browser_folders)
        };
        sample_browser.width = browser_width;

        let audio_engine = AudioEngine::new();
        if audio_engine.is_none() {
            println!("  Warning: no audio output device found");
        }

        let recorder = AudioRecorder::new();
        if recorder.is_none() {
            println!("  Warning: no audio input device found");
        }

        Self {
            gpu: None,
            camera,
            objects,
            waveforms,
            audio_clips: Vec::new(),
            audio_engine,
            recorder,
            recording_waveform_idx: None,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            file_hovering: false,
            modifiers: ModifiersState::empty(),
            command_palette: None,
            context_menu: None,
            sample_browser,
            storage,
            has_saved_state,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_project_id: project_id,
            current_project_name: project_name,
        }
    }

    fn save_project(&self) {
        if let Some(storage) = &self.storage {
            let state = ProjectState {
                name: self.current_project_name.clone(),
                camera_position: self.camera.position,
                camera_zoom: self.camera.zoom,
                objects: self.objects.clone(),
                waveforms: self.waveforms.clone(),
                browser_folders: self
                    .sample_browser
                    .root_folders
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                browser_width: self.sample_browser.width,
                browser_visible: self.sample_browser.visible,
                browser_expanded: self
                    .sample_browser
                    .expanded
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            };
            storage.save(&self.current_project_id, state);
            println!("Project '{}' saved", self.current_project_name);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            objects: self.objects.clone(),
            waveforms: self.waveforms.clone(),
            audio_clips: self.audio_clips.clone(),
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.objects = prev.objects;
            self.waveforms = prev.waveforms;
            self.audio_clips = prev.audio_clips;
            self.selected.clear();
            self.sync_audio_clips();
            self.request_redraw();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.objects = next.objects;
            self.waveforms = next.waveforms;
            self.audio_clips = next.audio_clips;
            self.selected.clear();
            self.sync_audio_clips();
            self.request_redraw();
        }
    }

    fn sync_audio_clips(&self) {
        if let Some(engine) = &self.audio_engine {
            let positions: Vec<[f32; 2]> = self.waveforms.iter().map(|wf| wf.position).collect();
            engine.update_clips(&positions, &self.audio_clips);
        }
    }

    fn toggle_recording(&mut self) {
        if self.recorder.is_none() {
            return;
        }

        let is_rec = self.recorder.as_ref().unwrap().is_recording();

        if is_rec {
            let loaded = self.recorder.as_mut().unwrap().stop();
            if let Some(loaded) = loaded {
                if let Some(idx) = self.recording_waveform_idx.take() {
                    if idx < self.waveforms.len() {
                        self.waveforms[idx].size[0] = loaded.width;
                        self.waveforms[idx].peaks = loaded.peaks;
                    }
                    if idx < self.audio_clips.len() {
                        self.audio_clips[idx] = AudioClipData {
                            samples: loaded.samples,
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        };
                    }
                    self.sync_audio_clips();
                }
            } else {
                if let Some(idx) = self.recording_waveform_idx.take() {
                    if idx < self.waveforms.len() {
                        self.waveforms.remove(idx);
                    }
                    if idx < self.audio_clips.len() {
                        self.audio_clips.remove(idx);
                    }
                }
            }
        } else {
            let world = self.last_canvas_click_world;
            let height = 150.0;
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let sample_rate = self.recorder.as_ref().unwrap().sample_rate();

            self.push_undo();
            let idx = self.waveforms.len();
            self.waveforms.push(WaveformObject {
                position: [world[0], world[1] - height * 0.5],
                size: [0.0, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                peaks: Vec::new(),
                filename: "Recording".to_string(),
            });
            self.audio_clips.push(AudioClipData {
                samples: Arc::new(Vec::new()),
                sample_rate,
                duration_secs: 0.0,
            });
            self.recording_waveform_idx = Some(idx);
            self.recorder.as_mut().unwrap().start();
        }
    }

    fn update_recording_waveform(&mut self) {
        let idx = match self.recording_waveform_idx {
            Some(i) => i,
            None => return,
        };
        let snapshot = self.recorder.as_ref().and_then(|r| r.current_snapshot());
        if let Some(loaded) = snapshot {
            if idx < self.waveforms.len() {
                self.waveforms[idx].size[0] = loaded.width;
                self.waveforms[idx].peaks = loaded.peaks;
            }
        }
    }

    fn is_recording(&self) -> bool {
        self.recorder
            .as_ref()
            .map(|r| r.is_recording())
            .unwrap_or(false)
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn update_cursor(&self) {
        if let Some(gpu) = &self.gpu {
            let icon = match &self.drag {
                DragState::Panning { .. } => CursorIcon::Grabbing,
                DragState::MovingSelection { .. } => CursorIcon::Grabbing,
                DragState::Selecting { .. } => CursorIcon::Crosshair,
                DragState::DraggingFromBrowser { .. } => CursorIcon::Grabbing,
                DragState::ResizingBrowser => CursorIcon::EwResize,
                DragState::None => {
                    if self.sample_browser.visible && self.sample_browser.resize_hovered {
                        CursorIcon::EwResize
                    } else if self.command_palette.is_some() {
                        CursorIcon::Default
                    } else if self.hovered.is_some() {
                        CursorIcon::Grab
                    } else {
                        CursorIcon::Default
                    }
                }
            };
            gpu.window.set_cursor(icon);
        }
    }

    fn update_hover(&mut self) {
        let (sw, sh, scale) = self.screen_info();
        if let Some(palette) = &mut self.command_palette {
            if let Some(idx) = palette.item_at(self.mouse_pos, sw, sh, scale) {
                palette.selected_index = idx;
            }
        }
        let world = self.camera.screen_to_world(self.mouse_pos);
        self.hovered = hit_test(&self.objects, &self.waveforms, world);
        self.update_cursor();
    }

    fn screen_info(&self) -> (f32, f32, f32) {
        match &self.gpu {
            Some(g) => (
                g.config.width as f32,
                g.config.height as f32,
                g.scale_factor,
            ),
            None => (1280.0, 800.0, 1.0),
        }
    }

    fn set_target_pos(&mut self, target: &HitTarget, pos: [f32; 2]) {
        match target {
            HitTarget::Object(i) => self.objects[*i].position = pos,
            HitTarget::Waveform(i) => self.waveforms[*i].position = pos,
        }
    }

    fn get_target_pos(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects[*i].position,
            HitTarget::Waveform(i) => self.waveforms[*i].position,
        }
    }

    fn begin_move_selection(&mut self, world: [f32; 2]) {
        self.push_undo();
        let offsets: Vec<(HitTarget, [f32; 2])> = self
            .selected
            .iter()
            .map(|t| {
                let pos = self.get_target_pos(t);
                (*t, [world[0] - pos[0], world[1] - pos[1]])
            })
            .collect();
        self.drag = DragState::MovingSelection { offsets };
    }

    fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::Copy => {
                println!("Copy: {} item(s) selected", self.selected.len());
            }
            CommandAction::Paste => {
                println!(
                    "Paste at ({:.0}, {:.0})",
                    self.mouse_pos[0], self.mouse_pos[1]
                );
            }
            CommandAction::Duplicate => {
                println!("Duplicate: {} item(s) selected", self.selected.len());
            }
            CommandAction::Delete => {
                self.delete_selected();
            }
            CommandAction::SelectAll => {
                self.selected.clear();
                for i in 0..self.objects.len() {
                    self.selected.push(HitTarget::Object(i));
                }
                for i in 0..self.waveforms.len() {
                    self.selected.push(HitTarget::Waveform(i));
                }
            }
            CommandAction::Undo => self.undo(),
            CommandAction::Redo => self.redo(),
            CommandAction::SaveProject => self.save_project(),
            CommandAction::ZoomIn => {
                let (sw, sh, _) = self.screen_info();
                self.camera.zoom_at([sw * 0.5, sh * 0.5], 1.25);
            }
            CommandAction::ZoomOut => {
                let (sw, sh, _) = self.screen_info();
                self.camera.zoom_at([sw * 0.5, sh * 0.5], 0.8);
            }
            CommandAction::ResetZoom => {
                let (_, _, scale) = self.screen_info();
                self.camera.zoom = scale;
            }
            CommandAction::ToggleBrowser => {
                self.sample_browser.visible = !self.sample_browser.visible;
            }
            CommandAction::AddFolderToBrowser => {
                self.open_add_folder_dialog();
            }
            CommandAction::SetMasterVolume => {
                if let Some(p) = &mut self.command_palette {
                    p.mode = PaletteMode::VolumeFader;
                    p.fader_value = self
                        .audio_engine
                        .as_ref()
                        .map_or(1.0, |e| e.master_volume());
                    p.search_text.clear();
                }
                self.request_redraw();
                return;
            }
        }
        self.request_redraw();
    }

    fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut obj_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Object(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut wf_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();

        obj_indices.sort_unstable_by(|a, b| b.cmp(a));
        wf_indices.sort_unstable_by(|a, b| b.cmp(a));

        for &i in &obj_indices {
            if i < self.objects.len() {
                self.objects.remove(i);
            }
        }
        for &i in &wf_indices {
            if i < self.waveforms.len() {
                self.waveforms.remove(i);
            }
            if i < self.audio_clips.len() {
                self.audio_clips.remove(i);
            }
        }

        self.selected.clear();
        self.sync_audio_clips();
        println!("Deleted selected items");
    }

    fn drop_audio_from_browser(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            return;
        }

        if let Some(loaded) = load_audio_file(path) {
            self.push_undo();
            let world = self.camera.screen_to_world(self.mouse_pos);
            let height = 150.0;
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            println!(
                "  Loaded: {} ({:.1}s, {} Hz, {} peaks)",
                filename,
                loaded.duration_secs,
                loaded.sample_rate,
                loaded.peaks.len(),
            );
            self.waveforms.push(WaveformObject {
                position: [world[0] - loaded.width * 0.5, world[1] - height * 0.5],
                size: [loaded.width, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                peaks: loaded.peaks,
                filename,
            });
            self.audio_clips.push(AudioClipData {
                samples: loaded.samples,
                sample_rate: loaded.sample_rate,
                duration_secs: loaded.duration_secs,
            });
            self.sync_audio_clips();
        }
    }

    fn open_add_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.sample_browser.add_folder(folder);
            self.sample_browser.visible = true;
            self.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Layers")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        if !self.has_saved_state {
            self.camera.zoom = window.scale_factor() as f32;
        }

        self.gpu = Some(pollster::block_on(Gpu::new(window)));
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());

        if self.sample_browser.visible && self.sample_browser.tick_scroll() {
            self.request_redraw();
        }

        if is_playing || self.is_recording() {
            self.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_project();
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size);
                    self.request_redraw();
                }
            }

            // --- drag & drop files ---
            WindowEvent::HoveredFile(_) => {
                self.file_hovering = true;
                self.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                self.file_hovering = false;
                self.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                self.file_hovering = false;
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    if let Some(loaded) = load_audio_file(&path) {
                        self.push_undo();
                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let height = 150.0;
                        let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
                        let filename = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        println!(
                            "  Loaded: {} ({:.1}s, {} Hz, {} peaks)",
                            filename,
                            loaded.duration_secs,
                            loaded.sample_rate,
                            loaded.peaks.len(),
                        );
                        self.waveforms.push(WaveformObject {
                            position: [world[0] - loaded.width * 0.5, world[1] - height * 0.5],
                            size: [loaded.width, height],
                            color: WAVEFORM_COLORS[color_idx],
                            border_radius: 8.0,
                            peaks: loaded.peaks,
                            filename,
                        });
                        self.audio_clips.push(AudioClipData {
                            samples: loaded.samples,
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        });
                        self.sync_audio_clips();
                    }
                }
                self.request_redraw();
            }

            // --- cursor ---
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = [position.x as f32, position.y as f32];

                if self.context_menu.is_some() {
                    let (sw, sh, scale) = self.screen_info();
                    if let Some(cm) = self.context_menu.as_mut() {
                        cm.update_hover(self.mouse_pos, sw, sh, scale);
                    }
                    self.request_redraw();
                }

                {
                    let is_dragging_fader = self
                        .command_palette
                        .as_ref()
                        .map_or(false, |p| p.fader_dragging);
                    if is_dragging_fader {
                        let (sw, sh, scale) = self.screen_info();
                        let mx = self.mouse_pos[0];
                        if let Some(p) = &mut self.command_palette {
                            p.fader_drag(mx, sw, sh, scale);
                            if let Some(engine) = &self.audio_engine {
                                engine.set_master_volume(p.fader_value);
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // Update browser hover state
                if self.sample_browser.visible && !matches!(self.drag, DragState::ResizingBrowser) {
                    let (_, sh, scale) = self.screen_info();
                    if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                        self.sample_browser.update_hover(self.mouse_pos, sh, scale);
                    } else {
                        self.sample_browser.hovered_entry = None;
                        self.sample_browser.add_button_hovered = false;
                        self.sample_browser.resize_hovered = false;
                    }
                    self.update_cursor();
                }

                // If resizing browser panel, update width
                if matches!(self.drag, DragState::ResizingBrowser) {
                    let (_, _, scale) = self.screen_info();
                    self.sample_browser
                        .set_width_from_screen(self.mouse_pos[0], scale);
                    self.request_redraw();
                    return;
                }

                // If dragging from browser, just request redraw for ghost
                if matches!(self.drag, DragState::DraggingFromBrowser { .. }) {
                    self.request_redraw();
                    return;
                }

                enum Action {
                    Pan([f32; 2], [f32; 2]),
                    MoveSelection(Vec<(HitTarget, [f32; 2])>),
                    Other,
                }
                let action = match &self.drag {
                    DragState::Panning {
                        start_mouse,
                        start_camera,
                    } => Action::Pan(*start_mouse, *start_camera),
                    DragState::MovingSelection { offsets } => {
                        Action::MoveSelection(offsets.clone())
                    }
                    _ => Action::Other,
                };

                match action {
                    Action::Pan(sm, sc) => {
                        self.camera.position[0] =
                            sc[0] - (self.mouse_pos[0] - sm[0]) / self.camera.zoom;
                        self.camera.position[1] =
                            sc[1] - (self.mouse_pos[1] - sm[1]) / self.camera.zoom;
                    }
                    Action::MoveSelection(offsets) => {
                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let mut waveform_moved = false;
                        for (target, offset) in &offsets {
                            self.set_target_pos(
                                target,
                                [world[0] - offset[0], world[1] - offset[1]],
                            );
                            if matches!(target, HitTarget::Waveform(_)) {
                                waveform_moved = true;
                            }
                        }
                        if waveform_moved {
                            self.sync_audio_clips();
                        }
                    }
                    Action::Other => {}
                }

                self.update_hover();
                self.request_redraw();
            }

            // --- mouse buttons ---
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Middle => match state {
                    ElementState::Pressed => {
                        self.command_palette = None;
                        self.drag = DragState::Panning {
                            start_mouse: self.mouse_pos,
                            start_camera: self.camera.position,
                        };
                        self.update_cursor();
                        self.request_redraw();
                    }
                    ElementState::Released => {
                        self.drag = DragState::None;
                        self.update_cursor();
                        self.request_redraw();
                    }
                },

                MouseButton::Right => {
                    if state == ElementState::Pressed {
                        self.command_palette = None;
                        self.context_menu = Some(ContextMenu::new(self.mouse_pos));
                        self.request_redraw();
                    }
                }

                MouseButton::Left => match state {
                    ElementState::Pressed => {
                        if self.context_menu.is_some() {
                            let (sw, sh, scale) = self.screen_info();
                            let inside = self
                                .context_menu
                                .as_ref()
                                .map_or(false, |cm| cm.contains(self.mouse_pos, sw, sh, scale));
                            let clicked_action = self.context_menu.as_ref().and_then(|cm| {
                                let idx = cm.item_at(self.mouse_pos, sw, sh, scale)?;
                                cm.action_at(idx)
                            });

                            if let Some(action) = clicked_action {
                                self.context_menu = None;
                                self.execute_command(action);
                            } else {
                                self.context_menu = None;
                            }
                            self.request_redraw();
                            if inside {
                                return;
                            }
                        }

                        if self.command_palette.is_some() {
                            let (sw, sh, scale) = self.screen_info();
                            let inside = self
                                .command_palette
                                .as_ref()
                                .map_or(false, |p| p.contains(self.mouse_pos, sw, sh, scale));

                            let is_fader = self
                                .command_palette
                                .as_ref()
                                .map_or(false, |p| p.mode == PaletteMode::VolumeFader);

                            if is_fader {
                                if inside {
                                    let hit = self
                                        .command_palette
                                        .as_ref()
                                        .map_or(false, |p| {
                                            p.fader_hit_test(self.mouse_pos, sw, sh, scale)
                                        });
                                    if hit {
                                        if let Some(p) = &mut self.command_palette {
                                            p.fader_dragging = true;
                                        }
                                    }
                                } else {
                                    self.command_palette = None;
                                }
                                self.request_redraw();
                                return;
                            }

                            let clicked_action = self.command_palette.as_ref().and_then(|p| {
                                let idx = p.item_at(self.mouse_pos, sw, sh, scale)?;
                                let mut cmd_i = 0;
                                for row in p.visible_rows() {
                                    if let PaletteRow::Command(ci) = row {
                                        if cmd_i == idx {
                                            return Some(COMMANDS[*ci].action);
                                        }
                                        cmd_i += 1;
                                    }
                                }
                                None
                            });

                            if let Some(action) = clicked_action {
                                if action == CommandAction::SetMasterVolume {
                                    self.execute_command(action);
                                } else {
                                    self.command_palette = None;
                                    self.execute_command(action);
                                }
                            } else if !inside {
                                self.command_palette = None;
                            }
                            self.request_redraw();
                            return;
                        }

                        // --- sample browser click ---
                        if self.sample_browser.visible {
                            let (_, sh, scale) = self.screen_info();
                            if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                                if self.sample_browser.hit_resize_handle(self.mouse_pos, scale) {
                                    self.drag = DragState::ResizingBrowser;
                                    self.update_cursor();
                                    self.request_redraw();
                                    return;
                                } else if self.sample_browser.hit_add_button(self.mouse_pos, scale)
                                {
                                    self.open_add_folder_dialog();
                                } else if let Some(idx) =
                                    self.sample_browser.item_at(self.mouse_pos, sh, scale)
                                {
                                    let entry = self.sample_browser.entries[idx].clone();
                                    if entry.is_dir {
                                        self.sample_browser.toggle_expand(idx);
                                    } else {
                                        let ext = entry
                                            .path
                                            .extension()
                                            .map(|e| e.to_string_lossy().to_lowercase())
                                            .unwrap_or_default();
                                        if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                                            self.drag = DragState::DraggingFromBrowser {
                                                path: entry.path.clone(),
                                                filename: entry.name.clone(),
                                            };
                                        }
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- transport panel click ---
                        {
                            let (sw, sh, scale) = self.screen_info();
                            if TransportPanel::contains(self.mouse_pos, sw, sh, scale) {
                                if TransportPanel::hit_record_button(self.mouse_pos, sw, sh, scale)
                                {
                                    self.toggle_recording();
                                } else if let Some(engine) = &self.audio_engine {
                                    engine.toggle_playback();
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        let world = self.camera.screen_to_world(self.mouse_pos);
                        self.last_canvas_click_world = world;
                        let hit = hit_test(&self.objects, &self.waveforms, world);

                        match hit {
                            Some(target) => {
                                if self.selected.contains(&target) {
                                    // Already selected -> drag whole selection
                                } else {
                                    self.selected.clear();
                                    self.selected.push(target);
                                }
                                self.begin_move_selection(world);
                            }
                            None => {
                                self.drag = DragState::Selecting { start_world: world };
                            }
                        }

                        self.update_cursor();
                        self.request_redraw();
                    }

                    ElementState::Released => {
                        if let Some(p) = &mut self.command_palette {
                            if p.fader_dragging {
                                p.fader_dragging = false;
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- finish browser resize ---
                        if matches!(self.drag, DragState::ResizingBrowser) {
                            self.drag = DragState::None;
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- drop from browser to canvas ---
                        if let DragState::DraggingFromBrowser { ref path, .. } = self.drag {
                            let (_, sh, scale) = self.screen_info();
                            let in_browser = self.sample_browser.visible
                                && self.sample_browser.contains(self.mouse_pos, sh, scale);
                            if !in_browser {
                                let path = path.clone();
                                self.drop_audio_from_browser(&path);
                            }
                            self.drag = DragState::None;
                            self.update_hover();
                            self.request_redraw();
                            return;
                        }

                        if let DragState::Selecting { start_world } = &self.drag {
                            let start = *start_world;
                            let current = self.camera.screen_to_world(self.mouse_pos);
                            let (rp, rs) = canonical_rect(start, current);

                            let min_sz = 5.0 / self.camera.zoom;
                            if rs[0] < min_sz && rs[1] < min_sz {
                                self.selected.clear();
                                if let Some(engine) = &self.audio_engine {
                                    let secs = current[0] as f64 / PIXELS_PER_SECOND as f64;
                                    engine.seek_to_seconds(secs);
                                }
                            } else {
                                self.selected =
                                    targets_in_rect(&self.objects, &self.waveforms, rp, rs);
                            }
                        }

                        self.drag = DragState::None;
                        self.update_hover();
                        self.request_redraw();
                    }
                },
                _ => {}
            },

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if self.context_menu.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.context_menu = None;
                            self.request_redraw();
                            return;
                        }
                    }

                    // --- command palette input ---
                    if self.command_palette.is_some() {
                        let is_fader = self
                            .command_palette
                            .as_ref()
                            .map_or(false, |p| p.mode == PaletteMode::VolumeFader);

                        if is_fader {
                            match &event.logical_key {
                                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                                    self.command_palette = None;
                                    self.request_redraw();
                                    return;
                                }
                                _ => {
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.command_palette = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.move_selection(-1);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.move_selection(1);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let action = self
                                    .command_palette
                                    .as_ref()
                                    .and_then(|p| p.selected_action());
                                if let Some(a) = action {
                                    if a == CommandAction::SetMasterVolume {
                                        self.execute_command(a);
                                    } else {
                                        self.command_palette = None;
                                        self.execute_command(a);
                                    }
                                } else {
                                    self.command_palette = None;
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.pop();
                                    p.update_filter();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.push_str(ch.as_ref());
                                    p.update_filter();
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- global shortcuts ---
                    match &event.logical_key {
                        Key::Named(NamedKey::Space) => {
                            if let Some(engine) = &self.audio_engine {
                                engine.toggle_playback();
                                self.request_redraw();
                            }
                        }
                        Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete) => {
                            if !self.selected.is_empty() {
                                self.delete_selected();
                                self.request_redraw();
                            }
                        }
                        Key::Character(ch) if self.modifiers.super_key() => match ch.as_ref() {
                            "k" | "t" => {
                                self.context_menu = None;
                                self.command_palette = if self.command_palette.is_some() {
                                    None
                                } else {
                                    Some(CommandPalette::new())
                                };
                                self.request_redraw();
                            }
                            "b" => {
                                self.sample_browser.visible = !self.sample_browser.visible;
                                self.request_redraw();
                            }
                            "a" if self.modifiers.shift_key() => {
                                self.open_add_folder_dialog();
                            }
                            "r" => {
                                self.toggle_recording();
                                self.request_redraw();
                            }
                            "s" => self.save_project(),
                            "z" => {
                                if self.modifiers.shift_key() {
                                    self.redo();
                                } else {
                                    self.undo();
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }

            // --- scroll = pan, Cmd+scroll = zoom, pinch = zoom ---
            WindowEvent::MouseWheel { delta, .. } => {
                if self.command_palette.is_some() {
                    return;
                }
                let is_pixel_delta = matches!(delta, MouseScrollDelta::PixelDelta(_));
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 50.0, y * 50.0),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };

                if self.sample_browser.visible {
                    let (_, sh, scale) = self.screen_info();
                    if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                        if is_pixel_delta {
                            self.sample_browser.scroll_direct(dy, sh, scale);
                        } else {
                            self.sample_browser.scroll(dy, sh, scale);
                        }
                        self.sample_browser.update_hover(self.mouse_pos, sh, scale);
                        self.request_redraw();
                        return;
                    }
                }

                if self.modifiers.super_key() {
                    let zoom_sensitivity = 0.005;
                    let factor = (1.0 + dy * zoom_sensitivity).clamp(0.5, 2.0);
                    self.camera.zoom_at(self.mouse_pos, factor);
                } else {
                    self.camera.position[0] -= dx / self.camera.zoom;
                    self.camera.position[1] -= dy / self.camera.zoom;
                }

                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::PinchGesture { delta, .. } => {
                if self.command_palette.is_some() {
                    return;
                }
                let factor = (1.0 + delta as f32).clamp(0.5, 2.0);
                self.camera.zoom_at(self.mouse_pos, factor);
                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.update_recording_waveform();
                if let Some(gpu) = &mut self.gpu {
                    let w = gpu.config.width as f32;
                    let h = gpu.config.height as f32;

                    let sel_rect = if let DragState::Selecting { start_world } = &self.drag {
                        Some((*start_world, self.camera.screen_to_world(self.mouse_pos)))
                    } else {
                        None
                    };

                    let playhead_world_x = self
                        .audio_engine
                        .as_ref()
                        .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32);

                    let instances = build_instances(&RenderContext {
                        camera: &self.camera,
                        screen_w: w,
                        screen_h: h,
                        objects: &self.objects,
                        waveforms: &self.waveforms,
                        hovered: self.hovered,
                        selected: &self.selected,
                        selection_rect: sel_rect,
                        file_hovering: self.file_hovering,
                        playhead_world_x,
                    });

                    if self.sample_browser.visible {
                        self.sample_browser.get_text_entries(h, gpu.scale_factor);
                    }
                    let browser_ref = if self.sample_browser.visible {
                        Some(&self.sample_browser)
                    } else {
                        None
                    };

                    let drag_ghost =
                        if let DragState::DraggingFromBrowser { ref filename, .. } = self.drag {
                            Some((filename.as_str(), self.mouse_pos))
                        } else {
                            None
                        };

                    if let Some(p) = &mut self.command_palette {
                        if p.mode == PaletteMode::VolumeFader {
                            p.fader_rms =
                                self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak());
                        }
                    }

                    let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
                    let playback_pos = self
                        .audio_engine
                        .as_ref()
                        .map_or(0.0, |e| e.position_seconds());
                    let is_recording = self.recorder.as_ref().map_or(false, |r| r.is_recording());

                    gpu.render(
                        &self.camera,
                        &instances,
                        self.command_palette.as_ref(),
                        self.context_menu.as_ref(),
                        browser_ref,
                        drag_ghost,
                        is_playing,
                        is_recording,
                        playback_pos,
                    );
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    env_logger::init();

    println!("╔════════════════════════════════════════════╗");
    println!("║              Layers                         ║");
    println!("╠════════════════════════════════════════════╣");
    println!("║  Space              →  Play / Pause        ║");
    println!("║  Click background   →  Seek playhead       ║");
    println!("║  Drop audio file    →  Add to canvas       ║");
    println!("║  Two-finger scroll  →  Pan canvas          ║");
    println!("║  Cmd + scroll       →  Zoom in/out         ║");
    println!("║  Pinch              →  Zoom in/out         ║");
    println!("║  Middle drag        →  Pan canvas          ║");
    println!("║  Left drag empty    →  Selection rectangle ║");
    println!("║  Left drag object   →  Move (+ selection)  ║");
    println!("║  Cmd + K / Right-click → Command palette   ║");
    println!("║  Backspace / Delete →  Delete selected     ║");
    println!("║  Cmd + Z / ⇧⌘Z     →  Undo / Redo         ║");
    println!("║  Cmd + S            →  Save project        ║");
    println!("║  Cmd + B            →  Toggle browser      ║");
    println!("║  Cmd + Shift + A    →  Add folder           ║");
    println!("╚════════════════════════════════════════════╝");

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
