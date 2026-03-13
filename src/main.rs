mod audio;
mod browser;
mod component;
mod effects;
mod gpu;
mod settings;
mod storage;
mod ui;

pub(crate) use gpu::{push_border, Camera, Gpu, InstanceRaw};
pub(crate) use ui::transport::{TransportPanel, TRANSPORT_WIDTH};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use audio::{load_audio_file, AudioClipData, AudioEngine, AudioRecorder, PIXELS_PER_SECOND};
use settings::GridMode;
use ui::context_menu::{ContextMenu, MenuContext};
use ui::palette::{CommandAction, CommandPalette, PaletteMode, PaletteRow, COMMANDS};
pub(crate) use ui::waveform::WaveformView;
use ui::waveform::{AudioData, WaveformPeaks, WaveformVertex};

use surrealdb::types::SurrealValue;

use muda::{MenuId, Submenu as MudaSubmenu};
use settings::{Settings, SettingsWindow, CATEGORIES};
use storage::{default_base_path, ProjectState, Storage};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    platform::macos::WindowAttributesExtMacOS,
    window::{CursorIcon, Window, WindowId},
};

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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HitTarget {
    Object(usize),
    Waveform(usize),
    EffectRegion(usize),
    LoopRegion(usize),
    ExportRegion(usize),
    ComponentDef(usize),
    ComponentInstance(usize),
}

const MAX_UNDO_HISTORY: usize = 50;

#[derive(Clone)]
struct EffectRegionSnapshot {
    position: [f32; 2],
    size: [f32; 2],
    plugin_ids: Vec<String>,
    plugin_names: Vec<String>,
    name: String,
}

#[derive(Clone)]
struct LoopRegionSnapshot {
    position: [f32; 2],
    size: [f32; 2],
    enabled: bool,
}

#[derive(Clone)]
struct ExportRegionSnapshot {
    position: [f32; 2],
    size: [f32; 2],
}

#[derive(Clone)]
struct Snapshot {
    objects: Vec<CanvasObject>,
    waveforms: Vec<WaveformView>,
    audio_clips: Vec<AudioClipData>,
    effect_regions: Vec<EffectRegionSnapshot>,
    loop_regions: Vec<LoopRegionSnapshot>,
    export_regions: Vec<ExportRegionSnapshot>,
    components: Vec<component::ComponentDef>,
    component_instances: Vec<component::ComponentInstance>,
}

#[derive(Clone)]
pub(crate) struct ExportRegion {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
}

impl ExportRegion {
    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(
            world_pos,
            [p[0] - border_thickness, p[1] - border_thickness],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0],
        ) {
            return false;
        }
        if point_in_rect(world_pos, [p[0], p[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0], p[1] + s[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0] + s[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        let pill_w = EXPORT_RENDER_PILL_W / camera.zoom;
        let pill_h = EXPORT_RENDER_PILL_H / camera.zoom;
        if point_in_rect(
            world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [pill_w, pill_h],
        ) {
            return true;
        }
        false
    }
}

struct SelectArea {
    position: [f32; 2],
    size: [f32; 2],
}

#[derive(Clone, Copy, PartialEq)]
enum ExportHover {
    None,
    RenderPill(usize),
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
}

const EXPORT_REGION_DEFAULT_WIDTH: f32 = 800.0;
const EXPORT_REGION_DEFAULT_HEIGHT: f32 = 300.0;
const EXPORT_FILL_COLOR: [f32; 4] = [0.15, 0.70, 0.55, 0.10];
const EXPORT_BORDER_COLOR: [f32; 4] = [0.20, 0.80, 0.60, 0.50];
const EXPORT_RENDER_PILL_COLOR: [f32; 4] = [0.15, 0.65, 0.50, 0.85];
pub(crate) const EXPORT_RENDER_PILL_W: f32 = 110.0;
pub(crate) const EXPORT_RENDER_PILL_H: f32 = 22.0;

#[derive(Clone)]
pub(crate) struct LoopRegion {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) enabled: bool,
}

impl LoopRegion {
    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(
            world_pos,
            [p[0] - border_thickness, p[1] - border_thickness],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0],
        ) {
            return false;
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
        // LOOP badge area
        let badge_w = LOOP_BADGE_W / camera.zoom;
        let badge_h = LOOP_BADGE_H / camera.zoom;
        if point_in_rect(
            world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [badge_w, badge_h],
        ) {
            return true;
        }
        false
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LoopHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
}

const LOOP_REGION_DEFAULT_WIDTH: f32 = 800.0;
const LOOP_REGION_DEFAULT_HEIGHT: f32 = 250.0;
const LOOP_FILL_COLOR: [f32; 4] = [0.25, 0.55, 0.95, 0.08];
const LOOP_BORDER_COLOR: [f32; 4] = [0.30, 0.60, 1.0, 0.50];
const LOOP_BADGE_COLOR: [f32; 4] = [0.20, 0.50, 0.95, 0.85];
pub(crate) const LOOP_BADGE_W: f32 = 70.0;
pub(crate) const LOOP_BADGE_H: f32 = 22.0;

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
    DraggingPlugin {
        plugin_id: String,
        plugin_name: String,
    },
    ResizingBrowser,
    ResizingExportRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    DraggingFade {
        waveform_idx: usize,
        is_fade_in: bool,
    },
    DraggingFadeCurve {
        waveform_idx: usize,
        is_fade_in: bool,
        start_mouse_y: f32,
        start_curve: f32,
    },
    ResizingComponentDef {
        comp_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingEffectRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingLoopRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum ComponentDefHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum EffectRegionHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
    PluginLabel(usize, usize),
    PluginClose(usize, usize),
}

#[derive(Clone)]
enum ClipboardItem {
    Object(CanvasObject),
    Waveform(WaveformView, Option<AudioClipData>),
    EffectRegion(effects::EffectRegion),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    ComponentDef(
        component::ComponentDef,
        Vec<(WaveformView, Option<AudioClipData>)>,
    ),
    ComponentInstance(component::ComponentInstance),
}

struct Clipboard {
    items: Vec<ClipboardItem>,
}

impl Clipboard {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(crate) const WAVEFORM_COLORS: &[[f32; 4]] = &[
    [1.00, 0.24, 0.19, 1.0], // red
    [1.00, 0.42, 0.24, 1.0], // orange-red
    [1.00, 0.58, 0.00, 1.0], // orange
    [1.00, 0.72, 0.00, 1.0], // amber
    [1.00, 0.84, 0.00, 1.0], // yellow
    [0.78, 0.90, 0.19, 1.0], // lime
    [0.30, 0.85, 0.39, 1.0], // green
    [0.19, 0.84, 0.55, 1.0], // mint
    [0.19, 0.78, 0.71, 1.0], // teal
    [0.19, 0.78, 0.90, 1.0], // cyan
    [0.35, 0.78, 0.98, 1.0], // sky blue
    [0.00, 0.48, 1.00, 1.0], // blue
    [0.35, 0.34, 0.84, 1.0], // indigo
    [0.69, 0.32, 0.87, 1.0], // violet
    [0.88, 0.25, 0.63, 1.0], // magenta
    [1.00, 0.18, 0.33, 1.0], // rose
];

const SEL_COLOR: [f32; 4] = [0.35, 0.65, 1.0, 0.8];

// Audio formats supported via symphonia: wav, mp3, ogg, flac, aac
const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aac", "m4a", "mp4"];

pub(crate) fn format_playback_time(secs: f64) -> String {
    let minutes = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{}:{:04.1}", minutes, s)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) const DEFAULT_BPM: f32 = 120.0;

fn pixels_per_beat(bpm: f32) -> f32 {
    audio::PIXELS_PER_SECOND * 60.0 / bpm
}

/// Musical subdivision levels in beats: 32, 16, 8, 4, 2, 1, 1/2, 1/4, 1/8, 1/16, 1/32
const BEAT_SUBDIVISIONS: &[f32] = &[
    32.0, 16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 0.0625, 0.03125,
];

/// Returns (minor_spacing_world, beats_per_bar) for adaptive grid.
/// Picks the subdivision where screen-px spacing is closest to the target.
fn musical_grid_spacing(zoom: f32, target_px: f32, triplet: bool, bpm: f32) -> f32 {
    let ppb = pixels_per_beat(bpm);
    let triplet_mul = if triplet { 2.0 / 3.0 } else { 1.0 };
    let mut best = BEAT_SUBDIVISIONS[0] * ppb * triplet_mul;
    let mut best_diff = f32::MAX;
    for &subdiv in BEAT_SUBDIVISIONS {
        let world_spacing = subdiv * ppb * triplet_mul;
        let screen_spacing = world_spacing * zoom;
        let diff = (screen_spacing - target_px).abs();
        if diff < best_diff {
            best_diff = diff;
            best = world_spacing;
        }
    }
    best
}

fn grid_spacing_for_settings(settings: &Settings, zoom: f32, bpm: f32) -> f32 {
    match settings.grid_mode {
        GridMode::Adaptive(size) => {
            musical_grid_spacing(zoom, size.target_px(), settings.triplet_grid, bpm)
        }
        GridMode::Fixed(fg) => {
            let ppb = pixels_per_beat(bpm);
            let triplet_mul = if settings.triplet_grid {
                2.0 / 3.0
            } else {
                1.0
            };
            fg.beats() * ppb * triplet_mul
        }
    }
}

/// Snap a world-X coordinate to the nearest grid line.
pub(crate) fn snap_to_grid(world_x: f32, settings: &Settings, zoom: f32, bpm: f32) -> f32 {
    if !settings.grid_enabled || !settings.snap_to_grid {
        return world_x;
    }
    let spacing = grid_spacing_for_settings(settings, zoom, bpm);
    (world_x / spacing).round() * spacing
}

pub(crate) fn point_in_rect(pos: [f32; 2], rect_pos: [f32; 2], rect_size: [f32; 2]) -> bool {
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

/// Returns (waveform_index, is_fade_in) if the cursor is over a fade handle.
fn hit_test_fade_handle(
    waveforms: &[WaveformView],
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<(usize, bool)> {
    let handle_sz = ui::waveform::FADE_HANDLE_SIZE / camera.zoom;
    let hit_margin = handle_sz * 0.8;
    for (i, wf) in waveforms.iter().enumerate().rev() {
        let fi_cx = wf.position[0] + wf.fade_in_px;
        let fi_cy = wf.position[1];
        if (world_pos[0] - fi_cx).abs() < hit_margin && (world_pos[1] - fi_cy).abs() < hit_margin {
            return Some((i, true));
        }

        let fo_cx = wf.position[0] + wf.size[0] - wf.fade_out_px;
        let fo_cy = wf.position[1];
        if (world_pos[0] - fo_cx).abs() < hit_margin && (world_pos[1] - fo_cy).abs() < hit_margin {
            return Some((i, false));
        }
    }
    None
}

/// Returns (waveform_index, is_fade_in) if the cursor is near the fade curve midpoint dot.
fn hit_test_fade_curve_dot(
    waveforms: &[WaveformView],
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<(usize, bool)> {
    let hit_radius = ui::waveform::FADE_HANDLE_SIZE / camera.zoom;
    for (i, wf) in waveforms.iter().enumerate().rev() {
        if wf.fade_in_px > 0.0 {
            let [dx, dy] = ui::waveform::fade_curve_dot_pos(wf, true);
            if (world_pos[0] - dx).abs() < hit_radius && (world_pos[1] - dy).abs() < hit_radius {
                return Some((i, true));
            }
        }
        if wf.fade_out_px > 0.0 {
            let [dx, dy] = ui::waveform::fade_curve_dot_pos(wf, false);
            if (world_pos[0] - dx).abs() < hit_radius && (world_pos[1] - dy).abs() < hit_radius {
                return Some((i, false));
            }
        }
    }
    None
}

fn hit_test(
    objects: &[CanvasObject],
    waveforms: &[WaveformView],
    effect_regions: &[effects::EffectRegion],
    loop_regions: &[LoopRegion],
    export_regions: &[ExportRegion],
    components: &[component::ComponentDef],
    component_instances: &[component::ComponentInstance],
    editing_component: Option<usize>,
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<HitTarget> {
    // When editing a component, only its waveforms are hittable
    if let Some(ec_idx) = editing_component {
        if let Some(def) = components.get(ec_idx) {
            for &wf_idx in def.waveform_indices.iter().rev() {
                if wf_idx < waveforms.len() {
                    if point_in_rect(
                        world_pos,
                        waveforms[wf_idx].position,
                        waveforms[wf_idx].size,
                    ) {
                        return Some(HitTarget::Waveform(wf_idx));
                    }
                }
            }
        }
        return None;
    }

    let wf_in_component: HashSet<usize> = components
        .iter()
        .flat_map(|c| c.waveform_indices.iter().copied())
        .collect();
    let comp_map: std::collections::HashMap<component::ComponentId, usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // Instances first (on top)
    for (i, inst) in component_instances.iter().enumerate().rev() {
        if let Some(&def_idx) = comp_map.get(&inst.component_id) {
            let def = &components[def_idx];
            if point_in_rect(world_pos, inst.position, def.size) {
                return Some(HitTarget::ComponentInstance(i));
            }
        }
    }
    for (i, wf) in waveforms.iter().enumerate().rev() {
        if wf_in_component.contains(&i) {
            continue;
        }
        if point_in_rect(world_pos, wf.position, wf.size) {
            return Some(HitTarget::Waveform(i));
        }
    }
    for (i, obj) in objects.iter().enumerate().rev() {
        if point_in_rect(world_pos, obj.position, obj.size) {
            return Some(HitTarget::Object(i));
        }
    }
    for (i, def) in components.iter().enumerate().rev() {
        if point_in_rect(world_pos, def.position, def.size) {
            return Some(HitTarget::ComponentDef(i));
        }
    }
    for (i, er) in effect_regions.iter().enumerate().rev() {
        if er.hit_test_border(world_pos, camera) {
            return Some(HitTarget::EffectRegion(i));
        }
    }
    for (i, lr) in loop_regions.iter().enumerate().rev() {
        if lr.hit_test_border(world_pos, camera) {
            return Some(HitTarget::LoopRegion(i));
        }
    }
    for (i, xr) in export_regions.iter().enumerate().rev() {
        if xr.hit_test_border(world_pos, camera) {
            return Some(HitTarget::ExportRegion(i));
        }
    }
    None
}

fn targets_in_rect(
    objects: &[CanvasObject],
    waveforms: &[WaveformView],
    effect_regions: &[effects::EffectRegion],
    loop_regions: &[LoopRegion],
    export_regions: &[ExportRegion],
    components: &[component::ComponentDef],
    component_instances: &[component::ComponentInstance],
    editing_component: Option<usize>,
    rect_pos: [f32; 2],
    rect_size: [f32; 2],
) -> Vec<HitTarget> {
    let mut result = Vec::new();

    // When editing a component, only its waveforms are selectable via rect
    if let Some(ec_idx) = editing_component {
        if let Some(def) = components.get(ec_idx) {
            for &wf_idx in &def.waveform_indices {
                if wf_idx < waveforms.len() {
                    if rects_overlap(
                        rect_pos,
                        rect_size,
                        waveforms[wf_idx].position,
                        waveforms[wf_idx].size,
                    ) {
                        result.push(HitTarget::Waveform(wf_idx));
                    }
                }
            }
        }
        return result;
    }

    let wf_in_component: HashSet<usize> = components
        .iter()
        .flat_map(|c| c.waveform_indices.iter().copied())
        .collect();
    let comp_map: std::collections::HashMap<component::ComponentId, usize> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    for (i, obj) in objects.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, obj.position, obj.size) {
            result.push(HitTarget::Object(i));
        }
    }
    for (i, wf) in waveforms.iter().enumerate() {
        if wf_in_component.contains(&i) {
            continue;
        }
        if rects_overlap(rect_pos, rect_size, wf.position, wf.size) {
            result.push(HitTarget::Waveform(i));
        }
    }
    for (i, er) in effect_regions.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, er.position, er.size) {
            result.push(HitTarget::EffectRegion(i));
        }
    }
    for (i, lr) in loop_regions.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, lr.position, lr.size) {
            result.push(HitTarget::LoopRegion(i));
        }
    }
    for (i, xr) in export_regions.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, xr.position, xr.size) {
            result.push(HitTarget::ExportRegion(i));
        }
    }
    for (i, def) in components.iter().enumerate() {
        if rects_overlap(rect_pos, rect_size, def.position, def.size) {
            result.push(HitTarget::ComponentDef(i));
        }
    }
    for (i, inst) in component_instances.iter().enumerate() {
        if let Some(&def_idx) = comp_map.get(&inst.component_id) {
            let def = &components[def_idx];
            if rects_overlap(rect_pos, rect_size, inst.position, def.size) {
                result.push(HitTarget::ComponentInstance(i));
            }
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
    waveforms: &'a [WaveformView],
    effect_regions: &'a [effects::EffectRegion],
    hovered: Option<HitTarget>,
    selected: &'a HashSet<HitTarget>,
    selection_rect: Option<([f32; 2], [f32; 2])>,
    select_area: Option<&'a SelectArea>,
    file_hovering: bool,
    playhead_world_x: Option<f32>,
    export_regions: &'a [ExportRegion],
    loop_regions: &'a [LoopRegion],
    components: &'a [component::ComponentDef],
    component_instances: &'a [component::ComponentInstance],
    editing_component: Option<usize>,
    settings: &'a Settings,
    component_map: &'a std::collections::HashMap<component::ComponentId, usize>,
    fade_curve_hovered: Option<(usize, bool)>,
    fade_curve_dragging: Option<(usize, bool)>,
    mouse_world: [f32; 2],
    bpm: f32,
}

fn build_instances(out: &mut Vec<InstanceRaw>, ctx: &RenderContext) {
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
        let minor_color = [1.0, 1.0, 1.0, grid_i * 0.20];
        let beat_color = [1.0, 1.0, 1.0, grid_i * 0.38];
        let bar_color = [1.0, 1.0, 1.0, grid_i * 0.60];

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
    for (i, er) in ctx.effect_regions.iter().enumerate() {
        let er_right = er.position[0] + er.size[0];
        let er_bottom = er.position[1] + er.size[1];
        if er_right < world_left
            || er.position[0] > world_right
            || er_bottom < world_top
            || er.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::EffectRegion(i));
        let is_hov = ctx.hovered == Some(HitTarget::EffectRegion(i));
        let is_active = ctx.playhead_world_x.map_or(false, |px| {
            px >= er.position[0] && px <= er.position[0] + er.size[0]
        });
        out.extend(effects::build_effect_region_instances(
            er, camera, is_hov, is_sel, is_active,
        ));
    }

    // --- export regions ---
    for (i, er) in ctx.export_regions.iter().enumerate() {
        let p = er.position;
        let s = er.size;
        let er_right = p[0] + s[0];
        let er_bottom = p[1] + s[1];
        if er_right < world_left || p[0] > world_right || er_bottom < world_top || p[1] > world_bottom {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::ExportRegion(i));
        let is_hov = ctx.hovered == Some(HitTarget::ExportRegion(i));

        out.push(InstanceRaw {
            position: p,
            size: s,
            color: EXPORT_FILL_COLOR,
            border_radius: 6.0 / camera.zoom,
        });

        let bw = if is_sel { 2.5 } else if is_hov { 2.0 } else { 1.5 } / camera.zoom;
        push_border(out, p, s, bw, EXPORT_BORDER_COLOR);

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
                color: EXPORT_BORDER_COLOR,
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
            color: EXPORT_RENDER_PILL_COLOR,
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
    for (i, lr) in ctx.loop_regions.iter().enumerate() {
        let p = lr.position;
        let s = lr.size;
        let lr_right = p[0] + s[0];
        let lr_bottom = p[1] + s[1];
        if lr_right < world_left || p[0] > world_right || lr_bottom < world_top || p[1] > world_bottom {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::LoopRegion(i));
        let is_hov = ctx.hovered == Some(HitTarget::LoopRegion(i));
        let alpha_mul = if lr.enabled { 1.0 } else { 0.25 };

        let fill = [LOOP_FILL_COLOR[0], LOOP_FILL_COLOR[1], LOOP_FILL_COLOR[2], LOOP_FILL_COLOR[3] * alpha_mul];
        let border = [LOOP_BORDER_COLOR[0], LOOP_BORDER_COLOR[1], LOOP_BORDER_COLOR[2], LOOP_BORDER_COLOR[3] * alpha_mul];
        let badge = [LOOP_BADGE_COLOR[0], LOOP_BADGE_COLOR[1], LOOP_BADGE_COLOR[2], LOOP_BADGE_COLOR[3] * alpha_mul];

        out.push(InstanceRaw {
            position: p,
            size: s,
            color: fill,
            border_radius: 6.0 / camera.zoom,
        });

        let bw = if is_sel { 2.5 } else if is_hov { 2.0 } else { 1.5 } / camera.zoom;
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
    for (i, obj) in ctx.objects.iter().enumerate() {
        let obj_right = obj.position[0] + obj.size[0];
        let obj_bottom = obj.position[1] + obj.size[1];
        if obj_right < world_left
            || obj.position[0] > world_right
            || obj_bottom < world_top
            || obj.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Object(i));
        let is_hov = ctx.hovered == Some(HitTarget::Object(i));
        let mut color = obj.color;
        if ci > 0.001 {
            let lum = 0.299 * color[0] + 0.587 * color[1] + 0.114 * color[2];
            let boost = 1.0 + ci * 2.0;
            color[0] = (lum + (color[0] - lum) * boost).clamp(0.0, 1.0);
            color[1] = (lum + (color[1] - lum) * boost).clamp(0.0, 1.0);
            color[2] = (lum + (color[2] - lum) * boost).clamp(0.0, 1.0);
        }
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
        let wf_right = wf.position[0] + wf.size[0];
        let wf_bottom = wf.position[1] + wf.size[1];
        if wf_right < world_left
            || wf.position[0] > world_right
            || wf_bottom < world_top
            || wf.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Waveform(i));
        let is_hov = ctx.hovered == Some(HitTarget::Waveform(i));
        out.extend(ui::waveform::build_waveform_instances(
            wf,
            camera,
            world_left,
            world_right,
            is_hov,
            is_sel,
        ));
    }

    // --- component definitions ---
    for (i, def) in ctx.components.iter().enumerate() {
        let def_right = def.position[0] + def.size[0];
        let def_bottom = def.position[1] + def.size[1];
        if def_right < world_left
            || def.position[0] > world_right
            || def_bottom < world_top
            || def.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::ComponentDef(i));
        let is_hov = ctx.hovered == Some(HitTarget::ComponentDef(i));
        let is_editing = ctx.editing_component == Some(i);
        out.extend(component::build_component_def_instances(
            def,
            camera,
            is_hov,
            is_sel || is_editing,
        ));
    }

    // --- component instances ---
    for (i, inst) in ctx.component_instances.iter().enumerate() {
        if let Some(&def_idx) = ctx.component_map.get(&inst.component_id) {
            let def = &ctx.components[def_idx];
            let inst_right = inst.position[0] + def.size[0];
            let inst_bottom = inst.position[1] + def.size[1];
            if inst_right < world_left
                || inst.position[0] > world_right
                || inst_bottom < world_top
                || inst.position[1] > world_bottom
            {
                continue;
            }
            let is_sel = ctx.selected.contains(&HitTarget::ComponentInstance(i));
            let is_hov = ctx.hovered == Some(HitTarget::ComponentInstance(i));
            out.extend(component::build_component_instance_instances(
                inst,
                def,
                ctx.waveforms,
                camera,
                world_left,
                world_right,
                is_hov,
                is_sel,
            ));
        }
    }

    // --- edit mode dimming overlay ---
    if let Some(ec_idx) = ctx.editing_component {
        if let Some(def) = ctx.components.get(ec_idx) {
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
        let (pos, size) = target_rect(
            ctx.objects,
            ctx.waveforms,
            ctx.effect_regions,
            ctx.loop_regions,
            ctx.export_regions,
            ctx.components,
            ctx.component_instances,
            ctx.component_map,
            target,
        );
        push_border(out, pos, size, sel_bw, SEL_COLOR);

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
        out.push(InstanceRaw {
            position: sa.position,
            size: sa.size,
            color: [0.30, 0.55, 1.0, 0.10],
            border_radius: 0.0,
        });
        let bw = 1.0 / camera.zoom;
        push_border(out, sa.position, sa.size, bw, [0.35, 0.65, 1.0, 0.5]);
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
            out,
            [world_left, world_top],
            [world_right - world_left, world_bottom - world_top],
            bw,
            [0.35, 0.65, 1.0, 0.7],
        );
    }
}

fn build_waveform_vertices(verts: &mut Vec<WaveformVertex>, ctx: &RenderContext) {
    verts.clear();
    let camera = ctx.camera;
    let world_left = camera.position[0];
    let world_right = world_left + ctx.screen_w / camera.zoom;
    let world_top = camera.position[1];
    let world_bottom = world_top + ctx.screen_h / camera.zoom;
    for (i, wf) in ctx.waveforms.iter().enumerate() {
        let wf_right = wf.position[0] + wf.size[0];
        let wf_bottom = wf.position[1] + wf.size[1];
        if wf_right < world_left
            || wf.position[0] > world_right
            || wf_bottom < world_top
            || wf.position[1] > world_bottom
        {
            continue;
        }
        let is_sel = ctx.selected.contains(&HitTarget::Waveform(i));
        let is_hov = ctx.hovered == Some(HitTarget::Waveform(i));
        verts.extend(ui::waveform::build_waveform_triangles(
            wf,
            camera,
            world_left,
            world_right,
            is_hov,
            is_sel,
        ));
        // Fade curve lines as smooth triangles (line only when cursor is near)
        let mx = ctx.mouse_world[0];
        let my = ctx.mouse_world[1];
        let in_wf_y = my >= wf.position[1] && my <= wf.position[1] + wf.size[1];
        let mouse_in_fi = wf.fade_in_px > 0.0 && in_wf_y
            && mx >= wf.position[0] && mx <= wf.position[0] + wf.fade_in_px;
        let mouse_in_fo = wf.fade_out_px > 0.0 && in_wf_y
            && mx >= wf.position[0] + wf.size[0] - wf.fade_out_px && mx <= wf.position[0] + wf.size[0];
        let show_fi_line = mouse_in_fi
            || matches!(ctx.fade_curve_hovered, Some((idx, true)) if idx == i)
            || matches!(ctx.fade_curve_dragging, Some((idx, true)) if idx == i);
        let show_fo_line = mouse_in_fo
            || matches!(ctx.fade_curve_hovered, Some((idx, false)) if idx == i)
            || matches!(ctx.fade_curve_dragging, Some((idx, false)) if idx == i);
        verts.extend(ui::waveform::build_fade_curve_triangles(wf, camera, show_fi_line, show_fo_line));
    }
}

fn target_rect(
    objects: &[CanvasObject],
    waveforms: &[WaveformView],
    effect_regions: &[effects::EffectRegion],
    loop_regions: &[LoopRegion],
    export_regions: &[ExportRegion],
    components: &[component::ComponentDef],
    component_instances: &[component::ComponentInstance],
    component_map: &std::collections::HashMap<component::ComponentId, usize>,
    target: &HitTarget,
) -> ([f32; 2], [f32; 2]) {
    match target {
        HitTarget::Object(i) => (objects[*i].position, objects[*i].size),
        HitTarget::Waveform(i) => (waveforms[*i].position, waveforms[*i].size),
        HitTarget::EffectRegion(i) => (effect_regions[*i].position, effect_regions[*i].size),
        HitTarget::LoopRegion(i) => (loop_regions[*i].position, loop_regions[*i].size),
        HitTarget::ExportRegion(i) => (export_regions[*i].position, export_regions[*i].size),
        HitTarget::ComponentDef(i) => (components[*i].position, components[*i].size),
        HitTarget::ComponentInstance(i) => {
            let inst = &component_instances[*i];
            let def = component_map
                .get(&inst.component_id)
                .map(|&idx| &components[idx]);
            match def {
                Some(d) => (inst.position, d.size),
                None => (inst.position, [100.0, 100.0]),
            }
        }
    }
}

fn default_objects() -> Vec<CanvasObject> {
    vec![]
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

struct MenuState {
    menu: muda::Menu,
    new_project: MenuId,
    save_project: MenuId,
    settings: MenuId,
    open_project_items: Vec<(MenuId, String)>,
    open_submenu: MudaSubmenu,
    initialized: bool,
}

struct App {
    gpu: Option<Gpu>,
    camera: Camera,
    objects: Vec<CanvasObject>,
    waveforms: Vec<WaveformView>,
    audio_clips: Vec<AudioClipData>,
    audio_engine: Option<AudioEngine>,
    recorder: Option<AudioRecorder>,
    recording_waveform_idx: Option<usize>,
    last_canvas_click_world: [f32; 2],
    selected: Vec<HitTarget>,
    drag: DragState,
    mouse_pos: [f32; 2],
    hovered: Option<HitTarget>,
    fade_handle_hovered: Option<(usize, bool)>,
    fade_curve_hovered: Option<(usize, bool)>,
    file_hovering: bool,
    modifiers: ModifiersState,
    command_palette: Option<CommandPalette>,
    context_menu: Option<ContextMenu>,
    browser_context_path: Option<std::path::PathBuf>,
    sample_browser: browser::SampleBrowser,
    storage: Option<Storage>,
    has_saved_state: bool,
    project_dirty: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    current_project_name: String,
    effect_regions: Vec<effects::EffectRegion>,
    components: Vec<component::ComponentDef>,
    component_instances: Vec<component::ComponentInstance>,
    next_component_id: component::ComponentId,
    plugin_registry: effects::PluginRegistry,
    plugin_browser: browser::PluginBrowserSection,
    export_regions: Vec<ExportRegion>,
    export_hover: ExportHover,
    loop_regions: Vec<LoopRegion>,
    loop_hover: LoopHover,
    select_area: Option<SelectArea>,
    component_def_hover: ComponentDefHover,
    effect_region_hover: EffectRegionHover,
    editing_component: Option<usize>,
    editing_effect_name: Option<(usize, String)>,
    editing_waveform_name: Option<(usize, String)>,
    bpm: f32,
    editing_bpm: Option<String>,
    dragging_bpm: Option<(f32, f32)>,
    last_click_time: std::time::Instant,
    last_click_world: [f32; 2],
    clipboard: Clipboard,
    settings: Settings,
    settings_window: Option<SettingsWindow>,
    plugin_editor: Option<ui::plugin_editor::PluginEditorWindow>,
    menu_state: Option<MenuState>,
    toast_manager: ui::toast::ToastManager,
    cached_instances: Vec<InstanceRaw>,
    cached_wf_verts: Vec<WaveformVertex>,
    render_generation: u64,
    last_rendered_generation: u64,
    last_rendered_camera_pos: [f32; 2],
    last_rendered_camera_zoom: f32,
    last_rendered_hovered: Option<HitTarget>,
    last_rendered_selected_len: usize,
}

impl App {
    fn mark_dirty(&mut self) {
        self.render_generation = self.render_generation.wrapping_add(1);
        self.project_dirty = true;
    }

    fn new(skip_load: bool) -> Self {
        let base_path = default_base_path();
        println!("  Storage: {}", base_path.display());

        let mut storage = Storage::open(&base_path);

        let mut opened_project = false;
        if let Some(s) = &mut storage {
            if skip_load {
                if s.create_temp_project().is_some() {
                    opened_project = true;
                }
                println!("  Starting with empty project (--empty)");
            } else {
                let projects = s.list_projects();
                if !projects.is_empty() {
                    println!("  Projects:");
                    for p in &projects {
                        println!("    - {} ({})", p.name, p.path);
                    }
                    let best = projects.iter().max_by_key(|p| p.updated_at).unwrap();
                    let path = PathBuf::from(&best.path);
                    if path.exists() && s.open_project(&path) {
                        opened_project = true;
                    }
                }
                if !opened_project {
                    if s.create_temp_project().is_some() {
                        opened_project = true;
                    }
                }
            }
        }

        let loaded = if opened_project && !skip_load {
            storage.as_ref().and_then(|s| s.load_project_state())
        } else {
            None
        };
        let has_saved_state = loaded.is_some();

        // Load audio + peaks from project DB if available
        let (
            camera,
            objects,
            waveforms,
            project_name,
            browser_folders,
            browser_width,
            browser_visible,
            browser_expanded,
            stored_effect_regions,
            stored_loop_regions,
            stored_components,
            stored_component_instances,
            audio_clips,
            loaded_bpm,
        ) = match loaded {
            Some(state) => {
                println!(
                    "  Loaded project '{}' ({} objects, {} waveforms, {} effect regions)",
                    state.name,
                    state.objects.len(),
                    state.waveforms.len(),
                    state.effect_regions.len(),
                );
                let cam = Camera {
                    position: state.camera_position,
                    zoom: state.camera_zoom,
                };
                let name = storage
                    .as_ref()
                    .and_then(|s| s.current_project_path())
                    .and_then(|p| storage::Storage::read_project_json(p))
                    .map(|m| m.name)
                    .unwrap_or_else(|| state.name.clone());
                let folders: Vec<PathBuf> =
                    state.browser_folders.iter().map(PathBuf::from).collect();
                let bw = if state.browser_width > 0.0 {
                    state.browser_width
                } else {
                    260.0
                };
                let expanded: HashSet<PathBuf> =
                    state.browser_expanded.iter().map(PathBuf::from).collect();

                let num_waveforms = state.waveforms.len();
                let mut waveforms: Vec<WaveformView> = state
                    .waveforms
                    .into_iter()
                    .map(|sw| WaveformView {
                        audio: Arc::new(AudioData {
                            left_samples: Arc::new(Vec::new()),
                            right_samples: Arc::new(Vec::new()),
                            left_peaks: Arc::new(WaveformPeaks::empty()),
                            right_peaks: Arc::new(WaveformPeaks::empty()),
                            sample_rate: sw.sample_rate,
                            filename: sw.filename,
                        }),
                        position: sw.position,
                        size: sw.size,
                        color: sw.color,
                        border_radius: sw.border_radius,
                        fade_in_px: sw.fade_in_px,
                        fade_out_px: sw.fade_out_px,
                        fade_in_curve: sw.fade_in_curve,
                        fade_out_curve: sw.fade_out_curve,
                        volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                        disabled: sw.disabled,
                    })
                    .collect();

                // Restore audio data and peaks from DB
                let mut audio_clips: Vec<AudioClipData> = Vec::new();
                if let Some(s) = &storage {
                    for i in 0..num_waveforms {
                        let mut left_samples = Arc::new(Vec::new());
                        let mut right_samples = Arc::new(Vec::new());
                        let mut sample_rate = waveforms[i].audio.sample_rate;
                        let mut left_peaks = waveforms[i].audio.left_peaks.clone();
                        let mut right_peaks = waveforms[i].audio.right_peaks.clone();

                        if let Some(audio) = s.load_audio(i as u64) {
                            left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                            right_samples =
                                Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                            let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                            sample_rate = audio.sample_rate;
                            audio_clips.push(AudioClipData {
                                samples: Arc::new(mono),
                                sample_rate: audio.sample_rate,
                                duration_secs: audio.duration_secs,
                            });
                        } else {
                            audio_clips.push(AudioClipData {
                                samples: Arc::new(Vec::new()),
                                sample_rate: 48000,
                                duration_secs: 0.0,
                            });
                        }
                        if let Some(peaks) = s.load_peaks(i as u64) {
                            let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                            let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                            left_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                            right_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                        }
                        waveforms[i].audio = Arc::new(AudioData {
                            left_samples,
                            right_samples,
                            left_peaks,
                            right_peaks,
                            sample_rate,
                            filename: waveforms[i].audio.filename.clone(),
                        });
                    }
                }

                (
                    cam,
                    state.objects,
                    waveforms,
                    name,
                    folders,
                    bw,
                    state.browser_visible,
                    Some(expanded),
                    state.effect_regions,
                    state.loop_regions,
                    state.components,
                    state.component_instances,
                    audio_clips,
                    if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
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
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    DEFAULT_BPM,
                )
            }
        };

        let mut sample_browser = if let Some(expanded) = browser_expanded {
            browser::SampleBrowser::from_state(browser_folders, expanded, browser_visible)
        } else {
            browser::SampleBrowser::from_folders(browser_folders)
        };
        sample_browser.width = browser_width;

        let mut settings = Settings::load();

        let device_name = if settings.audio_output_device == "No Device" {
            None
        } else {
            Some(settings.audio_output_device.as_str())
        };
        let audio_engine = AudioEngine::new_with_device(device_name);
        if let Some(ref engine) = audio_engine {
            let actual = engine.device_name();
            if settings.audio_output_device != actual {
                println!(
                    "  Correcting stale output device setting: '{}' -> '{}'",
                    settings.audio_output_device, actual
                );
                settings.audio_output_device = actual.to_string();
                settings.save();
            }
        } else {
            println!("  Warning: no audio output device found");
        }

        let recorder = AudioRecorder::new();
        if recorder.is_none() {
            println!("  Warning: no audio input device found");
        }

        let plugin_registry = effects::PluginRegistry::new();

        // Restore effect region geometry; plugins will be loaded lazily on first scan
        let restored_effect_regions: Vec<effects::EffectRegion> = stored_effect_regions
            .into_iter()
            .map(|ser| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                for (pid, pname) in ser.plugin_ids.iter().zip(ser.plugin_names.iter()) {
                    region.chain.push(effects::PluginSlot {
                        plugin_id: pid.clone(),
                        plugin_name: pname.clone(),
                        plugin_path: std::path::PathBuf::new(),
                        bypass: false,
                        instance: Arc::new(std::sync::Mutex::new(None)),
                        gui: Arc::new(std::sync::Mutex::new(None)),
                    });
                }
                region
            })
            .collect();

        let plugin_browser = browser::PluginBrowserSection::new();

        let restored_loop_regions: Vec<LoopRegion> = stored_loop_regions
            .into_iter()
            .map(|slr| LoopRegion {
                position: slr.position,
                size: slr.size,
                enabled: slr.enabled,
            })
            .collect();

        let restored_components: Vec<component::ComponentDef> = stored_components
            .into_iter()
            .map(|sc| component::ComponentDef {
                id: sc.id,
                name: sc.name,
                position: sc.position,
                size: sc.size,
                waveform_indices: sc.waveform_indices.iter().map(|&i| i as usize).collect(),
            })
            .collect();
        let restored_instances: Vec<component::ComponentInstance> = stored_component_instances
            .into_iter()
            .map(|si| component::ComponentInstance {
                component_id: si.component_id,
                position: si.position,
            })
            .collect();
        let next_component_id = restored_components.iter().map(|c| c.id).max().unwrap_or(0) + 1;

        Self {
            gpu: None,
            camera,
            objects,
            waveforms,
            audio_clips,
            audio_engine,
            recorder,
            recording_waveform_idx: None,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            fade_handle_hovered: None,
            fade_curve_hovered: None,
            file_hovering: false,
            modifiers: ModifiersState::empty(),
            command_palette: None,
            context_menu: None,
            browser_context_path: None,
            sample_browser,
            storage,
            has_saved_state,
            project_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_project_name: project_name,
            effect_regions: restored_effect_regions,
            components: restored_components,
            component_instances: restored_instances,
            next_component_id,
            plugin_registry,
            plugin_browser,
            export_regions: Vec::new(),
            export_hover: ExportHover::None,
            loop_regions: restored_loop_regions,
            loop_hover: LoopHover::None,
            select_area: None,
            component_def_hover: ComponentDefHover::None,
            effect_region_hover: EffectRegionHover::None,
            editing_component: None,
            editing_effect_name: None,
            editing_waveform_name: None,
            bpm: loaded_bpm,
            editing_bpm: None,
            dragging_bpm: None,
            last_click_time: std::time::Instant::now(),
            last_click_world: [0.0; 2],
            clipboard: Clipboard::new(),
            settings,
            settings_window: None,
            plugin_editor: None,
            menu_state: None,
            toast_manager: ui::toast::ToastManager::new(),
            cached_instances: Vec::with_capacity(2048),
            cached_wf_verts: Vec::with_capacity(32768),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
        }
    }

    fn save_project_state(&mut self) {
        if let Some(storage) = &self.storage {
            let stored_regions: Vec<storage::StoredEffectRegion> = self
                .effect_regions
                .iter()
                .map(|er| storage::StoredEffectRegion {
                    position: er.position,
                    size: er.size,
                    plugin_ids: er.chain.iter().map(|s| s.plugin_id.clone()).collect(),
                    plugin_names: er.chain.iter().map(|s| s.plugin_name.clone()).collect(),
                    name: er.name.clone(),
                })
                .collect();

            let stored_components: Vec<storage::StoredComponent> = self
                .components
                .iter()
                .map(|c| storage::StoredComponent {
                    id: c.id,
                    name: c.name.clone(),
                    position: c.position,
                    size: c.size,
                    waveform_indices: c.waveform_indices.iter().map(|&i| i as u64).collect(),
                })
                .collect();
            let stored_instances: Vec<storage::StoredComponentInstance> = self
                .component_instances
                .iter()
                .map(|inst| storage::StoredComponentInstance {
                    component_id: inst.component_id,
                    position: inst.position,
                })
                .collect();

            let stored_waveforms: Vec<storage::StoredWaveform> = self
                .waveforms
                .iter()
                .map(|wf| storage::StoredWaveform {
                    position: wf.position,
                    size: wf.size,
                    color: wf.color,
                    border_radius: wf.border_radius,
                    filename: wf.audio.filename.clone(),
                    fade_in_px: wf.fade_in_px,
                    fade_out_px: wf.fade_out_px,
                    fade_in_curve: wf.fade_in_curve,
                    fade_out_curve: wf.fade_out_curve,
                    sample_rate: wf.audio.sample_rate,
                    volume: wf.volume,
                    disabled: wf.disabled,
                })
                .collect();

            let state = ProjectState {
                name: self.current_project_name.clone(),
                camera_position: self.camera.position,
                camera_zoom: self.camera.zoom,
                objects: self.objects.clone(),
                waveforms: stored_waveforms,
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
                effect_regions: stored_regions,
                loop_regions: self
                    .loop_regions
                    .iter()
                    .map(|lr| storage::StoredLoopRegion {
                        position: lr.position,
                        size: lr.size,
                        enabled: lr.enabled,
                    })
                    .collect(),
                components: stored_components,
                component_instances: stored_instances,
                bpm: self.bpm,
            };
            storage.save_project_state(state);

            // Update project name in index
            if let Some(path) = storage.current_project_path() {
                let path_str = path.to_string_lossy().to_string();
                storage.update_index_name(&path_str, &self.current_project_name);
            }

            // Save audio data and peaks for each waveform
            storage.clear_audio_and_peaks();
            for (i, wf) in self.waveforms.iter().enumerate() {
                let mono = if i < self.audio_clips.len() {
                    &self.audio_clips[i].samples
                } else {
                    continue;
                };
                let duration = if i < self.audio_clips.len() {
                    self.audio_clips[i].duration_secs
                } else {
                    0.0
                };
                storage.save_audio(
                    i as u64,
                    &wf.audio.left_samples,
                    &wf.audio.right_samples,
                    mono,
                    wf.audio.sample_rate,
                    duration,
                );
                storage.save_peaks(
                    i as u64,
                    wf.audio.left_peaks.block_size as u64,
                    &wf.audio.left_peaks.peaks,
                    &wf.audio.right_peaks.peaks,
                );
            }

            self.project_dirty = false;
            println!("Project '{}' saved", self.current_project_name);
        }
    }

    fn save_project(&mut self) {
        self.save_project_state();
        if let Some(storage) = &self.storage {
            if storage.is_temp_project() {
                self.save_project_as();
            }
        }
    }

    fn save_project_as(&mut self) {
        println!("Showing Save Project dialog...");
        let dest = rfd::FileDialog::new()
            .set_title("Save Project")
            .pick_folder();
        if let Some(dest) = dest {
            if let Some(storage) = &mut self.storage {
                if storage.save_project_to(&dest) {
                    if let Some(folder_name) = dest.file_name() {
                        self.current_project_name = folder_name.to_string_lossy().to_string();
                    }
                    self.save_project_state();
                    println!("Project saved to {:?}", dest);
                } else {
                    println!("Failed to save project to {:?}", dest);
                }
            }
        }
    }

    fn handle_menu_event(&mut self, id: MenuId) {
        let menu = match &self.menu_state {
            Some(m) => m,
            None => return,
        };

        if id == menu.new_project {
            self.new_project();
            self.refresh_open_project_menu();
            self.request_redraw();
        } else if id == menu.save_project {
            self.save_project();
            self.refresh_open_project_menu();
        } else if id == menu.settings {
            self.command_palette = None;
            self.context_menu = None;
            self.settings_window = if self.settings_window.is_some() {
                None
            } else {
                Some(SettingsWindow::new())
            };
            self.request_redraw();
        } else if let Some(project_path) = menu
            .open_project_items
            .iter()
            .find(|(mid, _)| *mid == id)
            .map(|(_, p)| p.clone())
        {
            self.load_project(&project_path);
            self.refresh_open_project_menu();
            self.request_redraw();
        }
    }

    fn new_project(&mut self) {
        self.save_project_state();

        // Create a new temp project folder
        if let Some(storage) = &mut self.storage {
            if storage.create_temp_project().is_none() {
                println!("Failed to create temp project");
                return;
            }
        }

        self.current_project_name = "Untitled".to_string();
        self.objects = default_objects();
        self.waveforms.clear();
        self.audio_clips.clear();
        self.effect_regions.clear();
        self.components.clear();
        self.component_instances.clear();
        self.next_component_id = 1;
        self.selected.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.camera = Camera::new();
        self.export_regions.clear();
        self.loop_regions.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm = None;
        self.dragging_bpm = None;
        self.command_palette = None;
        self.context_menu = None;

        if let Some(gpu) = &self.gpu {
            self.camera.zoom = gpu.window.scale_factor() as f32;
        }

        self.sync_audio_clips();
        self.save_project_state();
        println!("New project created");
    }

    fn load_project(&mut self, project_path: &str) {
        // Check if same project
        if let Some(s) = &self.storage {
            if let Some(cur) = s.current_project_path() {
                if cur.to_string_lossy() == project_path {
                    return;
                }
            }
        }
        self.save_project_state();

        // Open the project DB
        let path = PathBuf::from(project_path);
        if let Some(s) = &mut self.storage {
            if !s.open_project(&path) {
                println!("Failed to open project at '{project_path}'");
                return;
            }
        }

        let state = match self.storage.as_ref().and_then(|s| s.load_project_state()) {
            Some(s) => s,
            None => {
                println!("Failed to load project state from '{project_path}'");
                return;
            }
        };

        println!(
            "Loading project '{}' ({} objects, {} waveforms)",
            state.name,
            state.objects.len(),
            state.waveforms.len(),
        );

        self.current_project_name = if let Some(meta) = storage::Storage::read_project_json(&path) {
            meta.name
        } else {
            state.name.clone()
        };
        self.camera = Camera {
            position: state.camera_position,
            zoom: state.camera_zoom,
        };
        self.objects = state.objects;
        let num_waveforms = state.waveforms.len();
        self.waveforms = state
            .waveforms
            .into_iter()
            .map(|sw| WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate: sw.sample_rate,
                    filename: sw.filename,
                }),
                position: sw.position,
                size: sw.size,
                color: sw.color,
                border_radius: sw.border_radius,
                fade_in_px: sw.fade_in_px,
                fade_out_px: sw.fade_out_px,
                fade_in_curve: sw.fade_in_curve,
                fade_out_curve: sw.fade_out_curve,
                volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                disabled: sw.disabled,
            })
            .collect();

        // Restore audio data and peaks from DB
        self.audio_clips.clear();
        if let Some(s) = &self.storage {
            for i in 0..num_waveforms {
                let mut left_samples = Arc::new(Vec::new());
                let mut right_samples = Arc::new(Vec::new());
                let mut sample_rate = self.waveforms[i].audio.sample_rate;
                let mut left_peaks = self.waveforms[i].audio.left_peaks.clone();
                let mut right_peaks = self.waveforms[i].audio.right_peaks.clone();

                if let Some(audio) = s.load_audio(i as u64) {
                    left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                    right_samples = Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                    let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                    sample_rate = audio.sample_rate;
                    self.audio_clips.push(AudioClipData {
                        samples: Arc::new(mono),
                        sample_rate: audio.sample_rate,
                        duration_secs: audio.duration_secs,
                    });
                } else {
                    self.audio_clips.push(AudioClipData {
                        samples: Arc::new(Vec::new()),
                        sample_rate: 48000,
                        duration_secs: 0.0,
                    });
                }
                if let Some(peaks) = s.load_peaks(i as u64) {
                    let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                    let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                    left_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                    right_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                }
                self.waveforms[i].audio = Arc::new(AudioData {
                    left_samples,
                    right_samples,
                    left_peaks,
                    right_peaks,
                    sample_rate,
                    filename: self.waveforms[i].audio.filename.clone(),
                });
            }
        }

        let restored_regions: Vec<effects::EffectRegion> = state
            .effect_regions
            .into_iter()
            .map(|ser| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                for (pid, pname) in ser.plugin_ids.iter().zip(ser.plugin_names.iter()) {
                    region.chain.push(effects::PluginSlot {
                        plugin_id: pid.clone(),
                        plugin_name: pname.clone(),
                        plugin_path: std::path::PathBuf::new(),
                        bypass: false,
                        instance: Arc::new(std::sync::Mutex::new(None)),
                        gui: Arc::new(std::sync::Mutex::new(None)),
                    });
                }
                region
            })
            .collect();
        self.effect_regions = restored_regions;

        self.components = state
            .components
            .into_iter()
            .map(|sc| component::ComponentDef {
                id: sc.id,
                name: sc.name,
                position: sc.position,
                size: sc.size,
                waveform_indices: sc.waveform_indices.iter().map(|&i| i as usize).collect(),
            })
            .collect();
        self.component_instances = state
            .component_instances
            .into_iter()
            .map(|si| component::ComponentInstance {
                component_id: si.component_id,
                position: si.position,
            })
            .collect();
        self.next_component_id = self.components.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.bpm = if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM };

        self.sample_browser = if !state.browser_expanded.is_empty() {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let expanded: HashSet<PathBuf> =
                state.browser_expanded.iter().map(PathBuf::from).collect();
            let mut b =
                browser::SampleBrowser::from_state(folders, expanded, state.browser_visible);
            b.width = if state.browser_width > 0.0 {
                state.browser_width
            } else {
                260.0
            };
            b
        } else {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let mut b = browser::SampleBrowser::from_folders(folders);
            b.width = 260.0;
            b
        };

        self.selected.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.export_regions.clear();
        self.loop_regions.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm = None;
        self.dragging_bpm = None;
        self.command_palette = None;
        self.context_menu = None;

        self.sync_audio_clips();
        println!("Project '{}' loaded", self.current_project_name);
    }

    fn refresh_open_project_menu(&mut self) {
        let menu = match &mut self.menu_state {
            Some(m) => m,
            None => return,
        };

        while menu.open_submenu.remove_at(0).is_some() {}

        let mut new_items: Vec<(MenuId, String)> = Vec::new();
        if let Some(s) = &self.storage {
            for entry in s.list_projects() {
                let item = muda::MenuItem::new(&entry.name, true, None);
                new_items.push((item.id().clone(), entry.path.clone()));
                let _ = menu.open_submenu.append(&item);
            }
        }
        if new_items.is_empty() {
            let _ = menu
                .open_submenu
                .append(&muda::MenuItem::new("No Projects", false, None));
        }
        menu.open_project_items = new_items;
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            objects: self.objects.clone(),
            waveforms: self.waveforms.clone(),
            audio_clips: self.audio_clips.clone(),
            effect_regions: self
                .effect_regions
                .iter()
                .map(|er| EffectRegionSnapshot {
                    position: er.position,
                    size: er.size,
                    plugin_ids: er.chain.iter().map(|s| s.plugin_id.clone()).collect(),
                    plugin_names: er.chain.iter().map(|s| s.plugin_name.clone()).collect(),
                    name: er.name.clone(),
                })
                .collect(),
            loop_regions: self
                .loop_regions
                .iter()
                .map(|lr| LoopRegionSnapshot {
                    position: lr.position,
                    size: lr.size,
                    enabled: lr.enabled,
                })
                .collect(),
            export_regions: self
                .export_regions
                .iter()
                .map(|xr| ExportRegionSnapshot {
                    position: xr.position,
                    size: xr.size,
                })
                .collect(),
            components: self.components.clone(),
            component_instances: self.component_instances.clone(),
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.mark_dirty();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.objects = prev.objects;
            self.waveforms = prev.waveforms;
            self.audio_clips = prev.audio_clips;
            self.restore_effect_regions(prev.effect_regions);
            self.restore_loop_regions(prev.loop_regions);
            self.restore_export_regions(prev.export_regions);
            self.components = prev.components;
            self.component_instances = prev.component_instances;
            self.selected.clear();
            self.mark_dirty();
            self.sync_audio_clips();
            self.sync_loop_region();
            self.request_redraw();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.objects = next.objects;
            self.waveforms = next.waveforms;
            self.audio_clips = next.audio_clips;
            self.restore_effect_regions(next.effect_regions);
            self.restore_loop_regions(next.loop_regions);
            self.restore_export_regions(next.export_regions);
            self.components = next.components;
            self.component_instances = next.component_instances;
            self.selected.clear();
            self.mark_dirty();
            self.sync_audio_clips();
            self.sync_loop_region();
            self.request_redraw();
        }
    }

    fn restore_effect_regions(&mut self, snapshots: Vec<EffectRegionSnapshot>) {
        self.effect_regions = snapshots
            .into_iter()
            .map(|snap| {
                let mut region = effects::EffectRegion::new(snap.position, snap.size);
                region.name = snap.name;
                for (pid, pname) in snap.plugin_ids.iter().zip(snap.plugin_names.iter()) {
                    let instance = if self.plugin_registry.is_scanned() {
                        self.plugin_registry.load_plugin(pid, 48000.0, 512)
                    } else {
                        None
                    };
                    region.chain.push(effects::PluginSlot {
                        plugin_id: pid.clone(),
                        plugin_name: pname.clone(),
                        plugin_path: std::path::PathBuf::new(),
                        bypass: false,
                        instance: Arc::new(std::sync::Mutex::new(instance)),
                        gui: Arc::new(std::sync::Mutex::new(None)),
                    });
                }
                region
            })
            .collect();
    }

    fn restore_loop_regions(&mut self, snapshots: Vec<LoopRegionSnapshot>) {
        self.loop_regions = snapshots
            .into_iter()
            .map(|snap| LoopRegion {
                position: snap.position,
                size: snap.size,
                enabled: snap.enabled,
            })
            .collect();
    }

    fn restore_export_regions(&mut self, snapshots: Vec<ExportRegionSnapshot>) {
        self.export_regions = snapshots
            .into_iter()
            .map(|snap| ExportRegion {
                position: snap.position,
                size: snap.size,
            })
            .collect();
    }

    fn update_component_bounds(&mut self, comp_idx: usize) {
        if comp_idx >= self.components.len() {
            return;
        }
        let indices = self.components[comp_idx].waveform_indices.clone();
        if indices.is_empty() {
            return;
        }
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &indices);
        self.components[comp_idx].position = pos;
        self.components[comp_idx].size = size;
    }

    fn sync_audio_clips(&self) {
        if let Some(engine) = &self.audio_engine {
            let mut positions: Vec<[f32; 2]> = Vec::new();
            let mut sizes: Vec<[f32; 2]> = Vec::new();
            let mut clips: Vec<&AudioClipData> = Vec::new();
            let mut fade_ins: Vec<f32> = Vec::new();
            let mut fade_outs: Vec<f32> = Vec::new();
            let mut fade_in_curves: Vec<f32> = Vec::new();
            let mut fade_out_curves: Vec<f32> = Vec::new();
            let mut volumes: Vec<f32> = Vec::new();

            for (i, wf) in self.waveforms.iter().enumerate() {
                if wf.disabled || i >= self.audio_clips.len() {
                    continue;
                }
                positions.push(wf.position);
                sizes.push(wf.size);
                clips.push(&self.audio_clips[i]);
                fade_ins.push(wf.fade_in_px);
                fade_outs.push(wf.fade_out_px);
                fade_in_curves.push(wf.fade_in_curve);
                fade_out_curves.push(wf.fade_out_curve);
                volumes.push(wf.volume);
            }

            // Add virtual clips for each component instance
            let comp_map: std::collections::HashMap<component::ComponentId, usize> = self
                .components
                .iter()
                .enumerate()
                .map(|(i, c)| (c.id, i))
                .collect();
            for inst in &self.component_instances {
                if let Some(def) = comp_map
                    .get(&inst.component_id)
                    .map(|&i| &self.components[i])
                {
                    let offset = [
                        inst.position[0] - def.position[0],
                        inst.position[1] - def.position[1],
                    ];
                    for &wf_idx in &def.waveform_indices {
                        if wf_idx < self.waveforms.len() && wf_idx < self.audio_clips.len() && !self.waveforms[wf_idx].disabled {
                            let wf = &self.waveforms[wf_idx];
                            positions
                                .push([wf.position[0] + offset[0], wf.position[1] + offset[1]]);
                            sizes.push(wf.size);
                            clips.push(&self.audio_clips[wf_idx]);
                            fade_ins.push(wf.fade_in_px);
                            fade_outs.push(wf.fade_out_px);
                            fade_in_curves.push(wf.fade_in_curve);
                            fade_out_curves.push(wf.fade_out_curve);
                            volumes.push(wf.volume);
                        }
                    }
                }
            }

            let owned_clips: Vec<AudioClipData> = clips.iter().map(|c| (*c).clone()).collect();
            engine.update_clips(&positions, &sizes, &owned_clips, &fade_ins, &fade_outs, &fade_in_curves, &fade_out_curves, &volumes);

            let regions: Vec<audio::AudioEffectRegion> = self
                .effect_regions
                .iter()
                .map(|er| audio::AudioEffectRegion {
                    x_start_px: er.position[0],
                    x_end_px: er.position[0] + er.size[0],
                    y_start: er.position[1],
                    y_end: er.position[1] + er.size[1],
                    plugins: er.chain.iter().map(|slot| slot.instance.clone()).collect(),
                })
                .collect();
            engine.update_effect_regions(regions);
        }
    }

    fn add_loop_area(&mut self) {
        self.push_undo();
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(sa.position[0] + sa.size[0], &self.settings, self.camera.zoom, self.bpm);
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = LOOP_REGION_DEFAULT_WIDTH;
            let h = LOOP_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        self.loop_regions.push(LoopRegion {
            position: pos,
            size,
            enabled: true,
        });
        let idx = self.loop_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::LoopRegion(idx));
        self.sync_loop_region();
        self.mark_dirty();
        self.request_redraw();
    }

    fn add_effect_area(&mut self) {
        self.push_undo();
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(
                sa.position[0] + sa.size[0],
                &self.settings,
                self.camera.zoom,
                self.bpm,
            );
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = effects::EFFECT_REGION_DEFAULT_WIDTH;
            let h = effects::EFFECT_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        self.effect_regions
            .push(effects::EffectRegion::new(pos, size));
        let idx = self.effect_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::EffectRegion(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    fn add_render_area(&mut self) {
        self.push_undo();
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(
                sa.position[0] + sa.size[0],
                &self.settings,
                self.camera.zoom,
                self.bpm,
            );
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = EXPORT_REGION_DEFAULT_WIDTH;
            let h = EXPORT_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        self.export_regions.push(ExportRegion {
            position: pos,
            size,
        });
        let idx = self.export_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::ExportRegion(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    fn sync_loop_region(&self) {
        if let Some(engine) = &self.audio_engine {
            let regions: Vec<(f64, f64)> = self
                .loop_regions
                .iter()
                .filter(|lr| lr.enabled)
                .map(|lr| {
                    let start = lr.position[0] as f64 / audio::PIXELS_PER_SECOND as f64;
                    let end = (lr.position[0] + lr.size[0]) as f64 / audio::PIXELS_PER_SECOND as f64;
                    (start, end)
                })
                .collect();
            if let Some(&(start, end)) = regions.first() {
                engine.set_loop_region(start, end);
                engine.set_loop_enabled(true);
            } else {
                engine.set_loop_enabled(false);
            }
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
                        self.waveforms[idx].audio = Arc::new(AudioData {
                            left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                            right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                            left_samples: loaded.left_samples.clone(),
                            right_samples: loaded.right_samples.clone(),
                            sample_rate: loaded.sample_rate,
                            filename: self.waveforms[idx].audio.filename.clone(),
                        });
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
            self.waveforms.push(WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate,
                    filename: "Recording".to_string(),
                }),
                position: [world[0], world[1] - height * 0.5],
                size: [0.0, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                fade_in_px: 0.0,
                fade_out_px: 0.0,
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                disabled: false,
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
                self.waveforms[idx].audio = Arc::new(AudioData {
                    left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                    right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                    left_samples: loaded.left_samples,
                    right_samples: loaded.right_samples,
                    sample_rate: loaded.sample_rate,
                    filename: self.waveforms[idx].audio.filename.clone(),
                });
                self.mark_dirty();
            }
        }
    }

    fn is_recording(&self) -> bool {
        self.recorder
            .as_ref()
            .map(|r| r.is_recording())
            .unwrap_or(false)
    }

    fn trigger_export_render(&mut self) {
        let er = match self.export_regions.first() {
            Some(er) => er,
            None => return,
        };

        let start_secs = er.position[0] as f64 / audio::PIXELS_PER_SECOND as f64;
        let end_secs = (er.position[0] + er.size[0]) as f64 / audio::PIXELS_PER_SECOND as f64;
        let y_start = er.position[1];
        let y_end = er.position[1] + er.size[1];

        if end_secs <= start_secs {
            println!("  Export region has zero or negative duration");
            return;
        }

        let path = rfd::FileDialog::new()
            .set_file_name("export.wav")
            .add_filter("WAV", &["wav"])
            .save_file();

        let path = match path {
            Some(p) => p,
            None => return,
        };

        let clips: Vec<audio::ExportClip> = self
            .waveforms
            .iter()
            .zip(self.audio_clips.iter())
            .filter(|(wf, _)| !wf.disabled)
            .map(|(wf, clip)| audio::ExportClip {
                buffer: clip.samples.clone(),
                source_sample_rate: clip.sample_rate,
                start_time_secs: wf.position[0] as f64 / audio::PIXELS_PER_SECOND as f64,
                duration_secs: clip.duration_secs as f64,
                position_y: wf.position[1],
                height: wf.size[1],
                fade_in_secs: (wf.fade_in_px / audio::PIXELS_PER_SECOND) as f64,
                fade_out_secs: (wf.fade_out_px / audio::PIXELS_PER_SECOND) as f64,
                fade_in_curve: wf.fade_in_curve,
                fade_out_curve: wf.fade_out_curve,
                volume: wf.volume,
            })
            .collect();

        let effect_regions: Vec<audio::AudioEffectRegion> = self
            .effect_regions
            .iter()
            .map(|er| audio::AudioEffectRegion {
                x_start_px: er.position[0],
                x_end_px: er.position[0] + er.size[0],
                y_start: er.position[1],
                y_end: er.position[1] + er.size[1],
                plugins: er.chain.iter().map(|slot| slot.instance.clone()).collect(),
            })
            .collect();

        match audio::render_to_wav(
            &path,
            start_secs,
            end_secs,
            y_start,
            y_end,
            &clips,
            &effect_regions,
        ) {
            Ok(()) => println!("  Exported WAV to {}", path.display()),
            Err(e) => println!("  Export failed: {}", e),
        }
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn update_cursor(&self) {
        if let Some(gpu) = &self.gpu {
            let icon =
                match &self.drag {
                    DragState::Panning { .. } => CursorIcon::Grabbing,
                    DragState::MovingSelection { .. } => CursorIcon::Grabbing,
                    DragState::Selecting { .. } => CursorIcon::Default,
                    DragState::DraggingFromBrowser { .. } => CursorIcon::Grabbing,
                    DragState::DraggingPlugin { .. } => CursorIcon::Grabbing,
                    DragState::ResizingBrowser => CursorIcon::EwResize,
                    DragState::ResizingExportRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::DraggingFade { .. } => CursorIcon::EwResize,
                    DragState::DraggingFadeCurve { .. } => CursorIcon::NsResize,
                    DragState::ResizingComponentDef { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::ResizingEffectRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::ResizingLoopRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::None => {
                        if self.sample_browser.visible && self.sample_browser.resize_hovered {
                            CursorIcon::EwResize
                        } else if self.fade_handle_hovered.is_some() {
                            CursorIcon::EwResize
                        } else if self.fade_curve_hovered.is_some() {
                            CursorIcon::NsResize
                        } else if self.command_palette.is_some() {
                            CursorIcon::Default
                        } else if {
                            let (sw, sh, sc) = self.screen_info();
                            TransportPanel::hit_bpm(self.mouse_pos, sw, sh, sc)
                        } {
                            CursorIcon::Default
                        } else {
                            match self.component_def_hover {
                                ComponentDefHover::CornerNW(_) | ComponentDefHover::CornerSE(_) => {
                                    CursorIcon::NwseResize
                                }
                                ComponentDefHover::CornerNE(_) | ComponentDefHover::CornerSW(_) => {
                                    CursorIcon::NeswResize
                                }
                                ComponentDefHover::None => match self.effect_region_hover {
                                    EffectRegionHover::CornerNW(_)
                                    | EffectRegionHover::CornerSE(_) => CursorIcon::NwseResize,
                                    EffectRegionHover::CornerNE(_)
                                    | EffectRegionHover::CornerSW(_) => CursorIcon::NeswResize,
                                    EffectRegionHover::PluginLabel(_, _)
                                    | EffectRegionHover::PluginClose(_, _) => CursorIcon::Pointer,
                                    EffectRegionHover::None => match self.export_hover {
                                        ExportHover::CornerNW(_) | ExportHover::CornerSE(_) => {
                                            CursorIcon::NwseResize
                                        }
                                        ExportHover::CornerNE(_) | ExportHover::CornerSW(_) => {
                                            CursorIcon::NeswResize
                                        }
                                        ExportHover::RenderPill(_) => CursorIcon::Pointer,
                                        ExportHover::None => match self.loop_hover {
                                            LoopHover::CornerNW(_) | LoopHover::CornerSE(_) => {
                                                CursorIcon::NwseResize
                                            }
                                            LoopHover::CornerNE(_) | LoopHover::CornerSW(_) => {
                                                CursorIcon::NeswResize
                                            }
                                            LoopHover::None => {
                                                if self.hovered.is_some() {
                                                    CursorIcon::Grab
                                                } else {
                                                    CursorIcon::Default
                                                }
                                            }
                                        },
                                    },
                                },
                            }
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
        self.fade_handle_hovered = hit_test_fade_handle(&self.waveforms, world, &self.camera);
        self.fade_curve_hovered = if self.fade_handle_hovered.is_none() {
            hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
        } else {
            None
        };
        self.hovered = hit_test(
            &self.objects,
            &self.waveforms,
            &self.effect_regions,
            &self.loop_regions,
            &self.export_regions,
            &self.components,
            &self.component_instances,
            self.editing_component,
            world,
            &self.camera,
        );

        self.component_def_hover = ComponentDefHover::None;
        for (ci, def) in self.components.iter().enumerate() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = def.position;
            let s = def.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerNW(ci);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerNE(ci);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerSW(ci);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.component_def_hover = ComponentDefHover::CornerSE(ci);
                break;
            }
        }

        self.effect_region_hover = EffectRegionHover::None;
        'er_hover: for (i, er) in self.effect_regions.iter().enumerate() {
            let labels = effects::plugin_label_rects(er, &self.camera);
            // Check close buttons before pill body
            for rect in &labels {
                if point_in_rect(world, rect.close_position, rect.close_size) {
                    self.effect_region_hover = EffectRegionHover::PluginClose(i, rect.slot_idx);
                    break 'er_hover;
                }
            }
            for rect in &labels {
                if point_in_rect(world, rect.position, rect.size) {
                    self.effect_region_hover = EffectRegionHover::PluginLabel(i, rect.slot_idx);
                    break 'er_hover;
                }
            }

            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = er.position;
            let s = er.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerSW(i);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.effect_region_hover = EffectRegionHover::CornerSE(i);
                break;
            }
        }

        self.export_hover = ExportHover::None;
        for (i, er) in self.export_regions.iter().enumerate() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = er.position;
            let s = er.size;

            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerSW(i);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.export_hover = ExportHover::CornerSE(i);
                break;
            } else {
                let pill_w = EXPORT_RENDER_PILL_W / self.camera.zoom;
                let pill_h = EXPORT_RENDER_PILL_H / self.camera.zoom;
                let pill_x = p[0] + 4.0 / self.camera.zoom;
                let pill_y = p[1] + 4.0 / self.camera.zoom;
                if point_in_rect(world, [pill_x, pill_y], [pill_w, pill_h]) {
                    self.export_hover = ExportHover::RenderPill(i);
                    break;
                }
            }
        }

        self.loop_hover = LoopHover::None;
        for (i, lr) in self.loop_regions.iter().enumerate() {
            if !lr.enabled {
                continue;
            }
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = lr.position;
            let s = lr.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerSW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerSE(i);
                break;
            }
        }

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
            HitTarget::EffectRegion(i) => self.effect_regions[*i].position = pos,
            HitTarget::LoopRegion(i) => self.loop_regions[*i].position = pos,
            HitTarget::ExportRegion(i) => self.export_regions[*i].position = pos,
            HitTarget::ComponentDef(i) => {
                let old_pos = self.components[*i].position;
                let dx = pos[0] - old_pos[0];
                let dy = pos[1] - old_pos[1];
                self.components[*i].position = pos;
                for &wf_idx in &self.components[*i].waveform_indices.clone() {
                    if wf_idx < self.waveforms.len() {
                        self.waveforms[wf_idx].position[0] += dx;
                        self.waveforms[wf_idx].position[1] += dy;
                    }
                }
            }
            HitTarget::ComponentInstance(i) => self.component_instances[*i].position = pos,
        }
    }

    fn get_target_pos(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects[*i].position,
            HitTarget::Waveform(i) => self.waveforms[*i].position,
            HitTarget::EffectRegion(i) => self.effect_regions[*i].position,
            HitTarget::LoopRegion(i) => self.loop_regions[*i].position,
            HitTarget::ExportRegion(i) => self.export_regions[*i].position,
            HitTarget::ComponentDef(i) => self.components[*i].position,
            HitTarget::ComponentInstance(i) => self.component_instances[*i].position,
        }
    }

    fn begin_move_selection(&mut self, world: [f32; 2], alt_copy: bool) {
        self.push_undo();

        if alt_copy {
            let mut new_selected: Vec<HitTarget> = Vec::new();
            for target in self.selected.clone() {
                match target {
                    HitTarget::Waveform(i) => {
                        if i < self.waveforms.len() {
                            let wf = self.waveforms[i].clone();
                            self.waveforms.push(wf);
                            let new_i = self.waveforms.len() - 1;
                            if i < self.audio_clips.len() {
                                let clip = self.audio_clips[i].clone();
                                self.audio_clips.push(clip);
                            }
                            new_selected.push(HitTarget::Waveform(new_i));
                        }
                    }
                    HitTarget::Object(i) => {
                        if i < self.objects.len() {
                            let obj = self.objects[i].clone();
                            self.objects.push(obj);
                            new_selected.push(HitTarget::Object(self.objects.len() - 1));
                        }
                    }
                    HitTarget::EffectRegion(i) => {
                        if i < self.effect_regions.len() {
                            let er = self.effect_regions[i].clone();
                            self.effect_regions.push(er);
                            new_selected
                                .push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                        }
                    }
                    HitTarget::LoopRegion(i) => {
                        if i < self.loop_regions.len() {
                            let lr = self.loop_regions[i].clone();
                            self.loop_regions.push(lr);
                            new_selected
                                .push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                        }
                    }
                    HitTarget::ExportRegion(i) => {
                        if i < self.export_regions.len() {
                            let xr = self.export_regions[i].clone();
                            self.export_regions.push(xr);
                            new_selected
                                .push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                        }
                    }
                    HitTarget::ComponentInstance(i) => {
                        if i < self.component_instances.len() {
                            let inst = self.component_instances[i].clone();
                            self.component_instances.push(inst);
                            new_selected.push(HitTarget::ComponentInstance(
                                self.component_instances.len() - 1,
                            ));
                        }
                    }
                    HitTarget::ComponentDef(i) => {
                        if i < self.components.len() {
                            let src = &self.components[i];
                            let new_id = self.next_component_id;
                            self.next_component_id += 1;
                            let src_indices = src.waveform_indices.clone();
                            let new_comp = component::ComponentDef {
                                id: new_id,
                                name: format!("{} copy", src.name),
                                position: src.position,
                                size: src.size,
                                waveform_indices: Vec::new(),
                            };
                            let mut new_wf_indices = Vec::new();
                            for &wi in &src_indices {
                                if wi < self.waveforms.len() {
                                    let wf = self.waveforms[wi].clone();
                                    self.waveforms.push(wf);
                                    let new_wi = self.waveforms.len() - 1;
                                    new_wf_indices.push(new_wi);
                                    if wi < self.audio_clips.len() {
                                        let clip = self.audio_clips[wi].clone();
                                        self.audio_clips.push(clip);
                                    }
                                }
                            }
                            self.components.push(component::ComponentDef {
                                waveform_indices: new_wf_indices,
                                ..new_comp
                            });
                            new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                        }
                    }
                }
            }
            self.selected = new_selected;
        }

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
                self.copy_selected();
            }
            CommandAction::Paste => {
                self.paste_clipboard();
            }
            CommandAction::Duplicate => {
                self.duplicate_selected();
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
                    let in_component = self
                        .components
                        .iter()
                        .any(|c| c.waveform_indices.contains(&i));
                    if !in_component {
                        self.selected.push(HitTarget::Waveform(i));
                    }
                }
                for i in 0..self.effect_regions.len() {
                    self.selected.push(HitTarget::EffectRegion(i));
                }
                for i in 0..self.loop_regions.len() {
                    self.selected.push(HitTarget::LoopRegion(i));
                }
                for i in 0..self.components.len() {
                    self.selected.push(HitTarget::ComponentDef(i));
                }
                for i in 0..self.component_instances.len() {
                    self.selected.push(HitTarget::ComponentInstance(i));
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
                if self.sample_browser.visible {
                    self.ensure_plugins_scanned();
                }
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
            CommandAction::SetSampleVolume => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() {
                        if let Some(p) = &mut self.command_palette {
                            p.mode = PaletteMode::SampleVolumeFader;
                            p.fader_value = self.waveforms[idx].volume;
                            p.fader_target_waveform = Some(idx);
                            p.search_text.clear();
                        }
                        self.request_redraw();
                        return;
                    }
                }
            }
            CommandAction::CreateComponent => {
                self.create_component_from_selection();
            }
            CommandAction::CreateInstance => {
                self.create_instance_of_selected_component();
            }
            CommandAction::GoToComponent => {
                self.go_to_component_of_selected_instance();
            }
            CommandAction::OpenSettings => {
                self.settings_window = if self.settings_window.is_some() {
                    None
                } else {
                    Some(SettingsWindow::new())
                };
            }
            CommandAction::RenameEffectRegion => {
                let selected_er = self.selected.iter().find_map(|t| {
                    if let HitTarget::EffectRegion(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_er {
                    if idx < self.effect_regions.len() {
                        let current = self.effect_regions[idx].name.clone();
                        self.editing_effect_name = Some((idx, current));
                    }
                }
            }
            CommandAction::RenameSample => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() {
                        let current = self.waveforms[idx].audio.filename.clone();
                        self.editing_waveform_name = Some((idx, current));
                    }
                }
            }
            CommandAction::ToggleSnapToGrid => {
                self.settings.snap_to_grid = !self.settings.snap_to_grid;
                self.settings.save();
            }
            CommandAction::ToggleGrid => {
                self.settings.grid_enabled = !self.settings.grid_enabled;
                self.settings.save();
                self.mark_dirty();
            }
            CommandAction::SetGridAdaptive(size) => {
                self.settings.grid_mode = GridMode::Adaptive(size);
                self.settings.save();
            }
            CommandAction::SetGridFixed(fg) => {
                self.settings.grid_mode = GridMode::Fixed(fg);
                self.settings.save();
            }
            CommandAction::NarrowGrid => {
                match self.settings.grid_mode {
                    GridMode::Adaptive(s) => {
                        self.settings.grid_mode = GridMode::Adaptive(s.narrower());
                    }
                    GridMode::Fixed(f) => {
                        self.settings.grid_mode = GridMode::Fixed(f.finer());
                    }
                }
                self.settings.save();
            }
            CommandAction::WidenGrid => {
                match self.settings.grid_mode {
                    GridMode::Adaptive(s) => {
                        self.settings.grid_mode = GridMode::Adaptive(s.wider());
                    }
                    GridMode::Fixed(f) => {
                        self.settings.grid_mode = GridMode::Fixed(f.coarser());
                    }
                }
                self.settings.save();
            }
            CommandAction::ToggleTripletGrid => {
                self.settings.triplet_grid = !self.settings.triplet_grid;
                self.settings.save();
            }
            CommandAction::TestToast => {
                self.toast_manager
                    .push("This is an error toast", ui::toast::ToastKind::Error);
                self.toast_manager
                    .push("This is an info toast", ui::toast::ToastKind::Info);
                self.toast_manager
                    .push("This is a success toast", ui::toast::ToastKind::Success);
            }
            CommandAction::RevealInFinder => {
                if let Some(path) = self.browser_context_path.take() {
                    std::process::Command::new("open")
                        .arg("-R")
                        .arg(&path)
                        .spawn()
                        .ok();
                }
            }
            CommandAction::ReverseSample => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() && idx < self.audio_clips.len() {
                        self.push_undo();

                        let mut mono = (*self.audio_clips[idx].samples).clone();
                        mono.reverse();
                        self.audio_clips[idx].samples = Arc::new(mono);

                        let old = &self.waveforms[idx].audio;
                        let mut left = (*old.left_samples).clone();
                        let mut right = (*old.right_samples).clone();
                        left.reverse();
                        right.reverse();
                        let left_peaks = Arc::new(WaveformPeaks::build(&left));
                        let right_peaks = Arc::new(WaveformPeaks::build(&right));
                        let new_audio = Arc::new(AudioData {
                            left_samples: Arc::new(left),
                            right_samples: Arc::new(right),
                            left_peaks,
                            right_peaks,
                            sample_rate: old.sample_rate,
                            filename: old.filename.clone(),
                        });
                        self.waveforms[idx].audio = new_audio;

                        self.sync_audio_clips();
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::SplitSample => {
                self.split_sample_at_cursor();
            }
            CommandAction::AddLoopArea => {
                self.add_loop_area();
            }
            CommandAction::AddEffectsArea => {
                self.add_effect_area();
            }
            CommandAction::AddRenderArea => {
                self.add_render_area();
            }
            CommandAction::SetSampleColor(idx) => {
                if let Some(&color) = WAVEFORM_COLORS.get(idx) {
                    for target in self.selected.clone() {
                        if let HitTarget::Waveform(i) = target {
                            self.waveforms[i].color = color;
                        }
                    }
                }
            }
        }
        self.request_redraw();
    }

    fn split_sample_at_cursor(&mut self) {
        let world = self.camera.screen_to_world(self.mouse_pos);
        let hit = hit_test(
            &self.objects,
            &self.waveforms,
            &self.effect_regions,
            &self.loop_regions,
            &self.export_regions,
            &self.components,
            &self.component_instances,
            self.editing_component,
            world,
            &self.camera,
        );
        let wf_idx = match hit {
            Some(HitTarget::Waveform(i)) => i,
            _ => return,
        };
        if wf_idx >= self.waveforms.len() || wf_idx >= self.audio_clips.len() {
            return;
        }

        let pos = self.waveforms[wf_idx].position;
        let size = self.waveforms[wf_idx].size;
        let split_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
        let t = ((split_x - pos[0]) / size[0]).clamp(0.01, 0.99);

        let audio = Arc::clone(&self.waveforms[wf_idx].audio);
        let mono_samples = Arc::clone(&self.audio_clips[wf_idx].samples);
        let total_mono = mono_samples.len();
        if total_mono == 0 {
            return;
        }

        let split_mono = (t * total_mono as f32) as usize;
        let split_left = (t * audio.left_samples.len() as f32) as usize;
        let split_right = (t * audio.right_samples.len() as f32) as usize;

        let orig_color = self.waveforms[wf_idx].color;
        let orig_border_radius = self.waveforms[wf_idx].border_radius;
        let orig_fade_in = self.waveforms[wf_idx].fade_in_px;
        let orig_fade_out = self.waveforms[wf_idx].fade_out_px;
        let orig_fade_in_curve = self.waveforms[wf_idx].fade_in_curve;
        let orig_fade_out_curve = self.waveforms[wf_idx].fade_out_curve;
        let orig_volume = self.waveforms[wf_idx].volume;

        self.push_undo();

        let sample_rate = audio.sample_rate;
        let filename = audio.filename.clone();

        let left_mono: Vec<f32> = mono_samples[..split_mono].to_vec();
        let right_mono: Vec<f32> = mono_samples[split_mono..].to_vec();
        let left_l: Vec<f32> = audio.left_samples[..split_left].to_vec();
        let left_r: Vec<f32> = audio.right_samples[..split_right].to_vec();
        let right_l: Vec<f32> = audio.left_samples[split_left..].to_vec();
        let right_r: Vec<f32> = audio.right_samples[split_right..].to_vec();

        let left_duration = left_mono.len() as f32 / sample_rate as f32;
        let right_duration = right_mono.len() as f32 / sample_rate as f32;
        let left_width = left_duration * PIXELS_PER_SECOND;
        let right_width = right_duration * PIXELS_PER_SECOND;

        let left_clip = AudioClipData {
            samples: Arc::new(left_mono.clone()),
            sample_rate,
            duration_secs: left_duration,
        };
        let left_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&left_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&left_r)),
            left_samples: Arc::new(left_l),
            right_samples: Arc::new(left_r),
            sample_rate,
            filename: filename.clone(),
        });
        let left_waveform = WaveformView {
            audio: left_audio,
            position: pos,
            size: [left_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: orig_fade_in,
            fade_out_px: 0.0,
            fade_in_curve: orig_fade_in_curve,
            fade_out_curve: 0.0,
            volume: orig_volume,
            disabled: false,
        };

        let right_clip = AudioClipData {
            samples: Arc::new(right_mono.clone()),
            sample_rate,
            duration_secs: right_duration,
        };
        let right_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&right_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&right_r)),
            left_samples: Arc::new(right_l),
            right_samples: Arc::new(right_r),
            sample_rate,
            filename,
        });
        let right_waveform = WaveformView {
            audio: right_audio,
            position: [pos[0] + left_width, pos[1]],
            size: [right_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: 0.0,
            fade_out_px: orig_fade_out,
            fade_in_curve: 0.0,
            fade_out_curve: orig_fade_out_curve,
            volume: orig_volume,
            disabled: false,
        };

        self.waveforms[wf_idx] = left_waveform;
        self.audio_clips[wf_idx] = left_clip;
        self.waveforms.insert(wf_idx + 1, right_waveform);
        self.audio_clips.insert(wf_idx + 1, right_clip);

        // Fix up indices in component waveform_indices
        for comp in &mut self.components {
            let mut new_indices = Vec::new();
            for &wi in &comp.waveform_indices {
                if wi == wf_idx {
                    new_indices.push(wi);
                    new_indices.push(wi + 1);
                } else if wi > wf_idx {
                    new_indices.push(wi + 1);
                } else {
                    new_indices.push(wi);
                }
            }
            comp.waveform_indices = new_indices;
        }

        // Fix up selected indices
        let mut new_selected: Vec<HitTarget> = Vec::new();
        for t in &self.selected {
            match t {
                HitTarget::Waveform(i) if *i > wf_idx => {
                    new_selected.push(HitTarget::Waveform(i + 1));
                }
                other => new_selected.push(*other),
            }
        }
        new_selected.push(HitTarget::Waveform(wf_idx + 1));
        self.selected = new_selected;

        self.sync_audio_clips();
        self.mark_dirty();
    }

    fn create_component_from_selection(&mut self) {
        let wf_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();
        if wf_indices.is_empty() {
            println!("No waveforms selected to create component");
            return;
        }
        self.push_undo();
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &wf_indices);
        let id = self.next_component_id;
        self.next_component_id += 1;
        let name = format!("Component {}", id);
        let def = component::ComponentDef {
            id,
            name: name.clone(),
            position: pos,
            size,
            waveform_indices: wf_indices,
        };
        self.components.push(def);
        let idx = self.components.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::ComponentDef(idx));
        println!(
            "Created component '{}' with {} waveforms",
            name,
            self.components[idx].waveform_indices.len()
        );
    }

    fn create_instance_of_selected_component(&mut self) {
        let comp_idx = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentDef(i) => Some(*i),
            _ => None,
        });
        if let Some(ci) = comp_idx {
            if ci >= self.components.len() {
                return;
            }
            self.push_undo();
            let def = &self.components[ci];
            let offset_x = def.size[0] + 50.0;
            let inst = component::ComponentInstance {
                component_id: def.id,
                position: [def.position[0] + offset_x, def.position[1]],
            };
            self.component_instances.push(inst);
            let idx = self.component_instances.len() - 1;
            self.selected.clear();
            self.selected.push(HitTarget::ComponentInstance(idx));
            println!("Created instance of component {}", self.components[ci].name);
            self.sync_audio_clips();
        }
    }

    fn go_to_component_of_selected_instance(&mut self) {
        let inst_idx = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentInstance(i) => Some(*i),
            _ => None,
        });
        if let Some(ii) = inst_idx {
            if ii >= self.component_instances.len() {
                return;
            }
            let comp_id = self.component_instances[ii].component_id;
            if let Some((ci, def)) = self
                .components
                .iter()
                .enumerate()
                .find(|(_, c)| c.id == comp_id)
            {
                let (sw, sh, _) = self.screen_info();
                self.camera.position = [
                    def.position[0] + def.size[0] * 0.5 - sw * 0.5 / self.camera.zoom,
                    def.position[1] + def.size[1] * 0.5 - sh * 0.5 / self.camera.zoom,
                ];
                self.selected.clear();
                self.selected.push(HitTarget::ComponentDef(ci));
                println!("Navigated to component '{}'", def.name);
            }
        }
    }

    fn duplicate_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut new_selected: Vec<HitTarget> = Vec::new();

        let selected_wf_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| {
                if let HitTarget::Waveform(i) = t {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();

        let wf_group_shift = if selected_wf_indices.len() >= 2 {
            let min_start = selected_wf_indices
                .iter()
                .filter_map(|&i| self.waveforms.get(i))
                .map(|wf| wf.position[0])
                .fold(f32::INFINITY, f32::min);
            let max_end = selected_wf_indices
                .iter()
                .filter_map(|&i| self.waveforms.get(i))
                .map(|wf| wf.position[0] + wf.size[0])
                .fold(f32::NEG_INFINITY, f32::max);
            Some(max_end - min_start)
        } else {
            None
        };

        for target in self.selected.clone() {
            match target {
                HitTarget::ComponentInstance(i) => {
                    if i < self.component_instances.len() {
                        let src = &self.component_instances[i];
                        let def = self.components.iter().find(|c| c.id == src.component_id);
                        let shift = def.map(|d| d.size[0]).unwrap_or(100.0);
                        let inst = component::ComponentInstance {
                            component_id: src.component_id,
                            position: [src.position[0] + shift, src.position[1]],
                        };
                        self.component_instances.push(inst);
                        new_selected.push(HitTarget::ComponentInstance(
                            self.component_instances.len() - 1,
                        ));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if i < self.components.len() {
                        let src = &self.components[i];
                        let shift = src.size[0];
                        let new_id = self.next_component_id;
                        self.next_component_id += 1;
                        let new_comp = component::ComponentDef {
                            id: new_id,
                            name: format!("{} copy", src.name),
                            position: [src.position[0] + shift, src.position[1]],
                            size: src.size,
                            waveform_indices: Vec::new(),
                        };
                        let src_indices = src.waveform_indices.clone();
                        let mut new_wf_indices = Vec::new();
                        for &wi in &src_indices {
                            if wi < self.waveforms.len() {
                                let mut wf = self.waveforms[wi].clone();
                                wf.position[0] += shift;
                                self.waveforms.push(wf);
                                let new_wi = self.waveforms.len() - 1;
                                new_wf_indices.push(new_wi);
                                if wi < self.audio_clips.len() {
                                    let clip = self.audio_clips[wi].clone();
                                    self.audio_clips.push(clip);
                                }
                            }
                        }
                        self.components.push(component::ComponentDef {
                            waveform_indices: new_wf_indices,
                            ..new_comp
                        });
                        new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                    }
                }
                HitTarget::Waveform(i) => {
                    if i < self.waveforms.len() {
                        let mut wf = self.waveforms[i].clone();
                        let shift = wf_group_shift.unwrap_or(wf.size[0]);
                        wf.position[0] += shift;
                        self.waveforms.push(wf);
                        let new_i = self.waveforms.len() - 1;
                        if i < self.audio_clips.len() {
                            let clip = self.audio_clips[i].clone();
                            self.audio_clips.push(clip);
                        }
                        new_selected.push(HitTarget::Waveform(new_i));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if i < self.effect_regions.len() {
                        let mut er = self.effect_regions[i].clone();
                        er.position[0] += er.size[0];
                        self.effect_regions.push(er);
                        new_selected.push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if i < self.loop_regions.len() {
                        let mut lr = self.loop_regions[i].clone();
                        lr.position[0] += lr.size[0];
                        self.loop_regions.push(lr);
                        new_selected.push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if i < self.export_regions.len() {
                        let mut xr = self.export_regions[i].clone();
                        xr.position[0] += xr.size[0];
                        self.export_regions.push(xr);
                        new_selected.push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                    }
                }
                HitTarget::Object(i) => {
                    if i < self.objects.len() {
                        let mut obj = self.objects[i].clone();
                        obj.position[0] += obj.size[0];
                        self.objects.push(obj);
                        new_selected.push(HitTarget::Object(self.objects.len() - 1));
                    }
                }
            }
        }

        self.selected = new_selected;
        self.sync_audio_clips();
    }

    fn copy_selected(&mut self) {
        self.clipboard.items.clear();
        for target in &self.selected {
            match target {
                HitTarget::Object(i) => {
                    if *i < self.objects.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::Object(self.objects[*i].clone()));
                    }
                }
                HitTarget::Waveform(i) => {
                    if *i < self.waveforms.len() {
                        let clip = if *i < self.audio_clips.len() {
                            Some(self.audio_clips[*i].clone())
                        } else {
                            None
                        };
                        self.clipboard
                            .items
                            .push(ClipboardItem::Waveform(self.waveforms[*i].clone(), clip));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if *i < self.effect_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::EffectRegion(self.effect_regions[*i].clone()));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if *i < self.loop_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::LoopRegion(self.loop_regions[*i].clone()));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if *i < self.export_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::ExportRegion(self.export_regions[*i].clone()));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if *i < self.components.len() {
                        let def = &self.components[*i];
                        let wfs: Vec<(WaveformView, Option<AudioClipData>)> = def
                            .waveform_indices
                            .iter()
                            .filter_map(|&wi| {
                                if wi < self.waveforms.len() {
                                    let clip = if wi < self.audio_clips.len() {
                                        Some(self.audio_clips[wi].clone())
                                    } else {
                                        None
                                    };
                                    Some((self.waveforms[wi].clone(), clip))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        self.clipboard
                            .items
                            .push(ClipboardItem::ComponentDef(def.clone(), wfs));
                    }
                }
                HitTarget::ComponentInstance(i) => {
                    if *i < self.component_instances.len() {
                        self.clipboard.items.push(ClipboardItem::ComponentInstance(
                            self.component_instances[*i].clone(),
                        ));
                    }
                }
            }
        }
    }

    fn paste_clipboard(&mut self) {
        if self.clipboard.items.is_empty() {
            return;
        }
        self.push_undo();
        let world = self.camera.screen_to_world(self.mouse_pos);

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for item in &self.clipboard.items {
            let pos = match item {
                ClipboardItem::Object(o) => o.position,
                ClipboardItem::Waveform(w, _) => w.position,
                ClipboardItem::EffectRegion(e) => e.position,
                ClipboardItem::LoopRegion(l) => l.position,
                ClipboardItem::ExportRegion(x) => x.position,
                ClipboardItem::ComponentDef(d, _) => d.position,
                ClipboardItem::ComponentInstance(ci) => ci.position,
            };
            if pos[0] < min_x {
                min_x = pos[0];
            }
            if pos[1] < min_y {
                min_y = pos[1];
            }
        }

        let dx = world[0] - min_x;
        let dy = world[1] - min_y;
        let mut new_selected: Vec<HitTarget> = Vec::new();

        for item in self.clipboard.items.clone() {
            match item {
                ClipboardItem::Object(mut o) => {
                    o.position[0] += dx;
                    o.position[1] += dy;
                    self.objects.push(o);
                    new_selected.push(HitTarget::Object(self.objects.len() - 1));
                }
                ClipboardItem::Waveform(mut w, clip) => {
                    w.position[0] += dx;
                    w.position[1] += dy;
                    self.waveforms.push(w);
                    let idx = self.waveforms.len() - 1;
                    if let Some(c) = clip {
                        while self.audio_clips.len() < idx {
                            self.audio_clips.push(AudioClipData {
                                samples: std::sync::Arc::new(Vec::new()),
                                sample_rate: 44100,
                                duration_secs: 0.0,
                            });
                        }
                        self.audio_clips.push(c);
                    }
                    new_selected.push(HitTarget::Waveform(idx));
                }
                ClipboardItem::EffectRegion(mut e) => {
                    e.position[0] += dx;
                    e.position[1] += dy;
                    self.effect_regions.push(e);
                    new_selected.push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                }
                ClipboardItem::LoopRegion(mut l) => {
                    l.position[0] += dx;
                    l.position[1] += dy;
                    self.loop_regions.push(l);
                    new_selected.push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                }
                ClipboardItem::ExportRegion(mut x) => {
                    x.position[0] += dx;
                    x.position[1] += dy;
                    self.export_regions.push(x);
                    new_selected.push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                }
                ClipboardItem::ComponentDef(mut d, wfs) => {
                    let new_id = self.next_component_id;
                    self.next_component_id += 1;
                    d.id = new_id;
                    d.position[0] += dx;
                    d.position[1] += dy;
                    d.name = format!("{} copy", d.name);
                    let mut new_wf_indices = Vec::new();
                    for (mut wf, clip) in wfs {
                        wf.position[0] += dx;
                        wf.position[1] += dy;
                        self.waveforms.push(wf);
                        let wi = self.waveforms.len() - 1;
                        new_wf_indices.push(wi);
                        if let Some(c) = clip {
                            while self.audio_clips.len() < wi {
                                self.audio_clips.push(AudioClipData {
                                    samples: std::sync::Arc::new(Vec::new()),
                                    sample_rate: 44100,
                                    duration_secs: 0.0,
                                });
                            }
                            self.audio_clips.push(c);
                        }
                    }
                    d.waveform_indices = new_wf_indices;
                    self.components.push(d);
                    new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                }
                ClipboardItem::ComponentInstance(mut ci) => {
                    ci.position[0] += dx;
                    ci.position[1] += dy;
                    self.component_instances.push(ci);
                    new_selected.push(HitTarget::ComponentInstance(
                        self.component_instances.len() - 1,
                    ));
                }
            }
        }

        self.selected = new_selected;
        self.sync_audio_clips();
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
        let mut er_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::EffectRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut lr_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::LoopRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut xr_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ExportRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut comp_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ComponentDef(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut inst_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ComponentInstance(i) => Some(*i),
                _ => None,
            })
            .collect();

        obj_indices.sort_unstable_by(|a, b| b.cmp(a));
        wf_indices.sort_unstable_by(|a, b| b.cmp(a));
        er_indices.sort_unstable_by(|a, b| b.cmp(a));
        lr_indices.sort_unstable_by(|a, b| b.cmp(a));
        xr_indices.sort_unstable_by(|a, b| b.cmp(a));
        comp_indices.sort_unstable_by(|a, b| b.cmp(a));
        inst_indices.sort_unstable_by(|a, b| b.cmp(a));

        // Delete instances first
        for &i in &inst_indices {
            if i < self.component_instances.len() {
                self.component_instances.remove(i);
            }
        }

        // Delete component defs (also removes their instances and releases waveforms)
        for &i in &comp_indices {
            if i < self.components.len() {
                let comp = self.components.remove(i);
                // Remove instances that reference this component
                self.component_instances
                    .retain(|inst| inst.component_id != comp.id);
                // Also delete the owned waveforms (sorted descending)
                let mut owned_wf: Vec<usize> = comp.waveform_indices;
                owned_wf.sort_unstable_by(|a, b| b.cmp(a));
                for &wi in &owned_wf {
                    if wi < self.waveforms.len() {
                        self.waveforms.remove(wi);
                    }
                    if wi < self.audio_clips.len() {
                        self.audio_clips.remove(wi);
                    }
                }
                // Fix waveform indices in remaining components
                for other in &mut self.components {
                    for idx in &mut other.waveform_indices {
                        for &removed in &owned_wf {
                            if *idx > removed {
                                *idx -= 1;
                            }
                        }
                    }
                }
            }
        }

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
        for &i in &er_indices {
            if i < self.effect_regions.len() {
                self.effect_regions.remove(i);
            }
        }
        for &i in &lr_indices {
            if i < self.loop_regions.len() {
                self.loop_regions.remove(i);
            }
        }
        for &i in &xr_indices {
            if i < self.export_regions.len() {
                self.export_regions.remove(i);
            }
        }

        self.selected.clear();
        self.sync_audio_clips();
        self.sync_loop_region();
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
                "  Loaded: {} ({:.1}s, {} Hz, {} samples/ch)",
                filename,
                loaded.duration_secs,
                loaded.sample_rate,
                loaded.left_samples.len(),
            );
            let left_peaks = Arc::new(WaveformPeaks::build(&loaded.left_samples));
            let right_peaks = Arc::new(WaveformPeaks::build(&loaded.right_samples));
            self.waveforms.push(WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: loaded.left_samples,
                    right_samples: loaded.right_samples,
                    left_peaks,
                    right_peaks,
                    sample_rate: loaded.sample_rate,
                    filename,
                }),
                position: [snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm), world[1] - height * 0.5],
                size: [loaded.width, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                fade_in_px: 0.0,
                fade_out_px: 0.0,
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                disabled: false,
            });
            self.audio_clips.push(AudioClipData {
                samples: loaded.samples,
                sample_rate: loaded.sample_rate,
                duration_secs: loaded.duration_secs,
            });
            self.sync_audio_clips();
        } else {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            self.toast_manager.push(
                format!(
                    "Cannot load '{}' \u{2014} unsupported or corrupted file",
                    filename
                ),
                ui::toast::ToastKind::Error,
            );
        }
    }

    fn open_add_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.sample_browser.add_folder(folder);
            self.sample_browser.visible = true;
            self.request_redraw();
        }
    }

    fn plugin_section_y_offset(&self, _screen_h: f32, scale: f32) -> f32 {
        let header_h = browser::HEADER_HEIGHT * scale;
        let item_h = browser::ITEM_HEIGHT * scale;
        let total_items = self.sample_browser.entries.len() as f32;
        header_h + total_items * item_h - self.sample_browser.scroll_offset
    }

    fn ensure_plugins_scanned(&mut self) {
        if self.plugin_registry.is_scanned() {
            return;
        }
        self.plugin_registry.ensure_scanned();

        let entries: Vec<browser::PluginEntry> = self
            .plugin_registry
            .plugins
            .iter()
            .map(|e| browser::PluginEntry {
                unique_id: e.info.unique_id.clone(),
                name: e.info.name.clone(),
                manufacturer: e.info.manufacturer.clone(),
            })
            .collect();
        self.plugin_browser.set_plugins(entries);

        // Reload any saved plugins that were waiting for the scanner
        for region in &mut self.effect_regions {
            for slot in &mut region.chain {
                let has_instance = slot.instance.lock().ok().map_or(false, |g| g.is_some());
                if !has_instance {
                    if let Some(instance) =
                        self.plugin_registry
                            .load_plugin(&slot.plugin_id, 48000.0, 512)
                    {
                        *slot.instance.lock().unwrap() = Some(instance);
                        println!("  Reloaded plugin '{}'", slot.plugin_name);
                    }
                }
            }
        }
        self.sync_audio_clips();
    }

    fn add_plugin_to_region(&mut self, region_idx: usize, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let sample_rate = 48000.0;
        let block_size = 512;

        // Look up plugin path from registry
        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        if let Some(instance) = self
            .plugin_registry
            .load_plugin(plugin_id, sample_rate, block_size)
        {
            let slot = effects::PluginSlot {
                plugin_id: plugin_id.to_string(),
                plugin_name: plugin_name.to_string(),
                plugin_path,
                bypass: false,
                instance: Arc::new(std::sync::Mutex::new(Some(instance))),
                gui: Arc::new(std::sync::Mutex::new(None)),
            };
            if region_idx < self.effect_regions.len() {
                self.effect_regions[region_idx].chain.push(slot);
                println!(
                    "  Added plugin '{}' to effect region {}",
                    plugin_name, region_idx
                );
                self.sync_audio_clips();
            }
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
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800))
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        if !self.has_saved_state {
            self.camera.zoom = window.scale_factor() as f32;
        }

        self.gpu = Some(pollster::block_on(Gpu::new(window)));

        if let Some(ms) = &mut self.menu_state {
            if !ms.initialized {
                ms.menu.init_for_nsapp();
                ms.initialized = true;
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());

        if self.sample_browser.visible && self.sample_browser.tick_scroll() {
            self.request_redraw();
        }

        if is_playing || self.is_recording() {
            self.request_redraw();
        }

        if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            self.handle_menu_event(event.id);
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
                if !self.project_dirty {
                    event_loop.exit();
                    return;
                }

                let is_temp = self
                    .storage
                    .as_ref()
                    .map(|s| s.is_temp_project())
                    .unwrap_or(false);

                let result = rfd::MessageDialog::new()
                    .set_title("Save Changes?")
                    .set_description(
                        "Your project has unsaved changes. Would you like to save before closing?",
                    )
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();

                match result {
                    rfd::MessageDialogResult::Yes => {
                        if is_temp {
                            self.save_project();
                        } else {
                            self.save_project_state();
                        }
                        event_loop.exit();
                    }
                    rfd::MessageDialogResult::No => {
                        if is_temp && !self.waveforms.is_empty() {
                            if let Some(storage) = &mut self.storage {
                                if let Some(path) = storage
                                    .current_project_path()
                                    .map(|p| p.to_string_lossy().to_string())
                                {
                                    storage.delete_project(&path);
                                }
                            }
                        }
                        event_loop.exit();
                    }
                    _ => {}
                }
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
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    if let Some(loaded) = load_audio_file(&path) {
                        self.push_undo();
                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let height = 150.0;
                        let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
                        println!(
                            "  Loaded: {} ({:.1}s, {} Hz, {} samples/ch)",
                            filename,
                            loaded.duration_secs,
                            loaded.sample_rate,
                            loaded.left_samples.len(),
                        );
                        let left_peaks = Arc::new(WaveformPeaks::build(&loaded.left_samples));
                        let right_peaks = Arc::new(WaveformPeaks::build(&loaded.right_samples));
                        self.waveforms.push(WaveformView {
                            audio: Arc::new(AudioData {
                                left_samples: loaded.left_samples,
                                right_samples: loaded.right_samples,
                                left_peaks,
                                right_peaks,
                                sample_rate: loaded.sample_rate,
                                filename: filename.clone(),
                            }),
                            position: [world[0] - loaded.width * 0.5, world[1] - height * 0.5],
                            size: [loaded.width, height],
                            color: WAVEFORM_COLORS[color_idx],
                            border_radius: 8.0,
                            fade_in_px: 0.0,
                            fade_out_px: 0.0,
                            fade_in_curve: 0.0,
                            fade_out_curve: 0.0,
                            volume: 1.0,
                            disabled: false,
                        });
                        self.audio_clips.push(AudioClipData {
                            samples: loaded.samples,
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        });
                        self.sync_audio_clips();
                    } else {
                        self.toast_manager.push(
                            format!(
                                "Cannot load '{}' \u{2014} unsupported or corrupted file",
                                filename
                            ),
                            ui::toast::ToastKind::Error,
                        );
                    }
                } else {
                    self.toast_manager.push(
                        format!(
                            "Cannot load '{}' \u{2014} not a supported audio format",
                            filename
                        ),
                        ui::toast::ToastKind::Error,
                    );
                }
                self.request_redraw();
            }

            // --- cursor ---
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = [position.x as f32, position.y as f32];

                // Plugin editor: slider drag
                {
                    let is_dragging_pe = self
                        .plugin_editor
                        .as_ref()
                        .map_or(false, |pe| pe.dragging_slider.is_some());
                    if is_dragging_pe {
                        let (scr_w, scr_h, scale) = self.screen_info();
                        let mx = self.mouse_pos[0];
                        if let Some(pe) = &mut self.plugin_editor {
                            let idx = pe.dragging_slider.unwrap();
                            let new_val = pe.slider_drag(idx, mx, scr_w, scr_h, scale);
                            let ri = pe.region_idx;
                            let si = pe.slot_idx;
                            if let Some(slot) =
                                self.effect_regions.get(ri).and_then(|er| er.chain.get(si))
                            {
                                if let Ok(mut guard) = slot.instance.lock() {
                                    if let Some(inst) = guard.as_mut() {
                                        let _ = inst.set_parameter(idx, new_val);
                                    }
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // Settings window: slider drag + hover
                {
                    let is_dragging_settings = self
                        .settings_window
                        .as_ref()
                        .map_or(false, |sw| sw.dragging_slider.is_some());
                    if is_dragging_settings {
                        let (scr_w, scr_h, scale) = self.screen_info();
                        let mx = self.mouse_pos[0];
                        if let Some(sw) = &self.settings_window {
                            let idx = sw.dragging_slider.unwrap();
                            sw.slider_drag(idx, mx, &mut self.settings, scr_w, scr_h, scale);
                        }
                        self.mark_dirty();
                        self.request_redraw();
                        return;
                    }
                    if self.settings_window.is_some() {
                        let (scr_w, scr_h, scale) = self.screen_info();
                        let pos = self.mouse_pos;
                        if let Some(sw) = &mut self.settings_window {
                            sw.update_hover(pos, scr_w, scr_h, scale);
                        }
                    }
                }

                if let Some((initial_bpm, initial_y)) = self.dragging_bpm {
                    let dy = initial_y - self.mouse_pos[1];
                    let new_bpm = (initial_bpm + dy * 0.5).clamp(20.0, 999.0);
                    self.bpm = new_bpm;
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                if self.context_menu.is_some() {
                    let (sw, sh, scale) = self.screen_info();
                    if let Some(cm) = self.context_menu.as_mut() {
                        cm.update_hover(self.mouse_pos, sw, sh, scale);
                    }
                    self.request_redraw();
                    return;
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
                            match p.mode {
                                PaletteMode::SampleVolumeFader => {
                                    if let Some(idx) = p.fader_target_waveform {
                                        if idx < self.waveforms.len() {
                                            self.waveforms[idx].volume = p.fader_value;
                                            self.sync_audio_clips();
                                        }
                                    }
                                }
                                _ => {
                                    if let Some(engine) = &self.audio_engine {
                                        engine.set_master_volume(p.fader_value);
                                    }
                                }
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
                        let plugin_y = self.plugin_section_y_offset(sh, scale);
                        let header_h = browser::HEADER_HEIGHT * scale;
                        let local_plugin_y = self.mouse_pos[1] - plugin_y;
                        if local_plugin_y >= 0.0
                            && plugin_y >= header_h
                            && !self.plugin_browser.plugins.is_empty()
                        {
                            self.plugin_browser.update_hover(local_plugin_y, scale);
                            self.sample_browser.hovered_entry = None;
                        } else {
                            self.plugin_browser.hovered_entry = None;
                            self.sample_browser.update_hover(self.mouse_pos, sh, scale);
                        }
                    } else {
                        self.sample_browser.hovered_entry = None;
                        self.sample_browser.add_button_hovered = false;
                        self.sample_browser.resize_hovered = false;
                        self.plugin_browser.hovered_entry = None;
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

                // If dragging from browser or plugin, just request redraw for ghost
                if matches!(
                    self.drag,
                    DragState::DraggingFromBrowser { .. } | DragState::DraggingPlugin { .. }
                ) {
                    self.request_redraw();
                    return;
                }

                // Resizing component def
                if let DragState::ResizingComponentDef {
                    comp_idx, anchor, ..
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if comp_idx < self.components.len() {
                        let min_size = 40.0;
                        let x0 = anchor[0].min(world[0]);
                        let y0 = anchor[1].min(world[1]);
                        let x1 = anchor[0].max(world[0]);
                        let y1 = anchor[1].max(world[1]);
                        self.components[comp_idx].position = [x0, y0];
                        self.components[comp_idx].size =
                            [(x1 - x0).max(min_size), (y1 - y0).max(min_size)];
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing export region
                if let DragState::ResizingExportRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.export_regions.len() {
                        let min_size = 40.0;
                        let snapped_wx = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
                        let snapped_ax = snap_to_grid(anchor[0], &self.settings, self.camera.zoom, self.bpm);
                        let x0 = snapped_ax.min(snapped_wx);
                        let y0 = anchor[1].min(world[1]);
                        let x1 = snapped_ax.max(snapped_wx);
                        let y1 = anchor[1].max(world[1]);
                        self.export_regions[region_idx].position = [x0, y0];
                        self.export_regions[region_idx].size = [(x1 - x0).max(min_size), (y1 - y0).max(min_size)];
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing effect region
                if let DragState::ResizingEffectRegion {
                    region_idx, anchor, ..
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.effect_regions.len() {
                        let min_size = 40.0;
                        let snapped_wx = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
                        let snapped_ax = snap_to_grid(anchor[0], &self.settings, self.camera.zoom, self.bpm);
                        let x0 = snapped_ax.min(snapped_wx);
                        let y0 = anchor[1].min(world[1]);
                        let x1 = snapped_ax.max(snapped_wx);
                        let y1 = anchor[1].max(world[1]);
                        self.effect_regions[region_idx].position = [x0, y0];
                        self.effect_regions[region_idx].size =
                            [(x1 - x0).max(min_size), (y1 - y0).max(min_size)];
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing loop region
                if let DragState::ResizingLoopRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.loop_regions.len() {
                        let min_size = 40.0;
                        let snapped_wx = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
                        let snapped_ax = snap_to_grid(anchor[0], &self.settings, self.camera.zoom, self.bpm);
                        let x0 = snapped_ax.min(snapped_wx);
                        let y0 = anchor[1].min(world[1]);
                        let x1 = snapped_ax.max(snapped_wx);
                        let y1 = anchor[1].max(world[1]);
                        self.loop_regions[region_idx].position = [x0, y0];
                        self.loop_regions[region_idx].size = [(x1 - x0).max(min_size), (y1 - y0).max(min_size)];
                    }
                    self.sync_loop_region();
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Dragging fade handle
                if let DragState::DraggingFade {
                    waveform_idx,
                    is_fade_in,
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if let Some(wf) = self.waveforms.get_mut(waveform_idx) {
                        let max_fade = wf.size[0] * 0.5;
                        if is_fade_in {
                            let new_val = (world[0] - wf.position[0]).clamp(0.0, max_fade);
                            wf.fade_in_px = new_val;
                        } else {
                            let new_val =
                                (wf.position[0] + wf.size[0] - world[0]).clamp(0.0, max_fade);
                            wf.fade_out_px = new_val;
                        }
                    }
                    self.mark_dirty();
                    self.sync_audio_clips();
                    self.request_redraw();
                    return;
                }

                // Dragging fade curve shape
                if let DragState::DraggingFadeCurve {
                    waveform_idx,
                    is_fade_in,
                    start_mouse_y,
                    start_curve,
                } = self.drag
                {
                    let dy = self.mouse_pos[1] - start_mouse_y;
                    let sensitivity = 0.005;
                    let new_curve = (start_curve - dy * sensitivity).clamp(-1.0, 1.0);
                    if let Some(wf) = self.waveforms.get_mut(waveform_idx) {
                        if is_fade_in {
                            wf.fade_in_curve = new_curve;
                        } else {
                            wf.fade_out_curve = new_curve;
                        }
                    }
                    self.mark_dirty();
                    self.sync_audio_clips();
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
                        let mut needs_sync = false;
                        for (target, offset) in &offsets {
                            let raw_x = world[0] - offset[0];
                            let snapped_x = snap_to_grid(raw_x, &self.settings, self.camera.zoom, self.bpm);
                            self.set_target_pos(target, [snapped_x, world[1] - offset[1]]);
                            if matches!(
                                target,
                                HitTarget::Waveform(_)
                                    | HitTarget::EffectRegion(_)
                                    | HitTarget::LoopRegion(_)
                                    | HitTarget::ExportRegion(_)
                                    | HitTarget::ComponentDef(_)
                                    | HitTarget::ComponentInstance(_)
                            ) {
                                needs_sync = true;
                            }
                        }
                        if let Some(ec_idx) = self.editing_component {
                            self.update_component_bounds(ec_idx);
                        }
                        if needs_sync {
                            self.sync_audio_clips();
                            self.sync_loop_region();
                        }
                        self.mark_dirty();
                    }
                    Action::Other => {
                        if let DragState::Selecting { start_world } = &self.drag {
                            let start = *start_world;
                            let current = self.camera.screen_to_world(self.mouse_pos);
                            let (rp, rs) = canonical_rect(start, current);
                            let min_sz = 5.0 / self.camera.zoom;
                            if rs[0] >= min_sz || rs[1] >= min_sz {
                                self.selected = targets_in_rect(
                                    &self.objects,
                                    &self.waveforms,
                                    &self.effect_regions,
                                    &self.loop_regions,
                                    &self.export_regions,
                                    &self.components,
                                    &self.component_instances,
                                    self.editing_component,
                                    rp,
                                    rs,
                                );
                            }
                        }
                    }
                }

                self.update_hover();
                self.request_redraw();
            }

            // --- mouse buttons ---
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Middle => match state {
                    ElementState::Pressed => {
                        if self.context_menu.is_some() {
                            return;
                        }
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

                        if self.sample_browser.visible {
                            let (_, sh, scale) = self.screen_info();
                            if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                                if let Some(idx) =
                                    self.sample_browser.item_at(self.mouse_pos, sh, scale)
                                {
                                    let entry = &self.sample_browser.entries[idx];
                                    self.browser_context_path = Some(entry.path.clone());
                                    self.context_menu = Some(ContextMenu::new(
                                        self.mouse_pos,
                                        MenuContext::BrowserEntry,
                                        &self.settings,
                                    ));
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let hit = hit_test(
                            &self.objects,
                            &self.waveforms,
                            &self.effect_regions,
                            &self.loop_regions,
                            &self.export_regions,
                            &self.components,
                            &self.component_instances,
                            self.editing_component,
                            world,
                            &self.camera,
                        );
                        let menu_ctx = match hit {
                            Some(HitTarget::ComponentInstance(_)) => {
                                if !self.selected.contains(&hit.unwrap()) {
                                    self.selected.clear();
                                    self.selected.push(hit.unwrap());
                                }
                                MenuContext::ComponentInstance
                            }
                            Some(HitTarget::ComponentDef(_)) => {
                                if !self.selected.contains(&hit.unwrap()) {
                                    self.selected.clear();
                                    self.selected.push(hit.unwrap());
                                }
                                MenuContext::ComponentDef
                            }
                            Some(target) => {
                                if !self.selected.contains(&target) {
                                    self.selected.clear();
                                    self.selected.push(target);
                                }
                                let has_waveforms = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::Waveform(_)));
                                let has_effect_region = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::EffectRegion(_)));
                                let current_waveform_color = self
                                    .selected
                                    .iter()
                                    .find_map(|t| match t {
                                        HitTarget::Waveform(i) => Some(self.waveforms[*i].color),
                                        _ => None,
                                    });
                                MenuContext::Selection {
                                    has_waveforms,
                                    has_effect_region,
                                    current_waveform_color,
                                }
                            }
                            None => {
                                self.selected.clear();
                                MenuContext::Grid
                            }
                        };
                        self.context_menu =
                            Some(ContextMenu::new(self.mouse_pos, menu_ctx, &self.settings));
                        self.request_redraw();
                    }
                }

                MouseButton::Left => match state {
                    ElementState::Pressed => {
                        if self.editing_bpm.is_some() {
                            let (sw, sh, scale) = self.screen_info();
                            if !TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                                self.editing_bpm = None;
                                self.request_redraw();
                            }
                        }

                        // Plugin editor click
                        if self.plugin_editor.is_some() {
                            let (scr_w, scr_h, scale) = self.screen_info();
                            let inside = self.plugin_editor.as_ref().map_or(false, |pe| {
                                pe.contains(self.mouse_pos, scr_w, scr_h, scale)
                            });
                            if inside {
                                let slider_hit = self.plugin_editor.as_ref().and_then(|pe| {
                                    pe.slider_hit_test(self.mouse_pos, scr_w, scr_h, scale)
                                });
                                if let Some(idx) = slider_hit {
                                    if let Some(pe) = &mut self.plugin_editor {
                                        pe.dragging_slider = Some(idx);
                                        let new_val = pe.slider_drag(
                                            idx,
                                            self.mouse_pos[0],
                                            scr_w,
                                            scr_h,
                                            scale,
                                        );
                                        let ri = pe.region_idx;
                                        let si = pe.slot_idx;
                                        if let Some(slot) = self
                                            .effect_regions
                                            .get(ri)
                                            .and_then(|er| er.chain.get(si))
                                        {
                                            if let Ok(mut guard) = slot.instance.lock() {
                                                if let Some(inst) = guard.as_mut() {
                                                    let _ = inst.set_parameter(idx, new_val);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                self.plugin_editor = None;
                            }
                            self.request_redraw();
                            return;
                        }

                        // Settings window click
                        if self.settings_window.is_some() {
                            let (scr_w, scr_h, scale) = self.screen_info();
                            let inside = self.settings_window.as_ref().map_or(false, |sw| {
                                sw.contains(self.mouse_pos, scr_w, scr_h, scale)
                            });
                            if inside {
                                // Try audio dropdown interaction first
                                let prev_output_device = self.settings.audio_output_device.clone();
                                let audio_consumed =
                                    self.settings_window.as_mut().map_or(false, |sw| {
                                        sw.handle_audio_click(
                                            self.mouse_pos,
                                            &mut self.settings,
                                            scr_w,
                                            scr_h,
                                            scale,
                                        )
                                    });
                                if audio_consumed {
                                    self.settings.save();

                                    if self.settings.audio_output_device != prev_output_device {
                                        println!(
                                            "[audio] Output device changed: '{}' -> '{}'",
                                            prev_output_device, self.settings.audio_output_device
                                        );

                                        let old_pos = self
                                            .audio_engine
                                            .as_ref()
                                            .map(|e| e.position_seconds());
                                        let old_vol =
                                            self.audio_engine.as_ref().map(|e| e.master_volume());
                                        let was_playing = self
                                            .audio_engine
                                            .as_ref()
                                            .map_or(false, |e| e.is_playing());

                                        let device_name =
                                            if self.settings.audio_output_device == "No Device" {
                                                None
                                            } else {
                                                Some(self.settings.audio_output_device.as_str())
                                            };
                                        self.audio_engine =
                                            AudioEngine::new_with_device(device_name);

                                        if let Some(ref engine) = self.audio_engine {
                                            let actual = engine.device_name().to_string();
                                            if self.settings.audio_output_device != actual {
                                                println!(
                                                    "[audio] Device '{}' not available, using '{}'",
                                                    self.settings.audio_output_device, actual
                                                );
                                                self.settings.audio_output_device = actual;
                                                self.settings.save();
                                            }
                                            if let Some(pos) = old_pos {
                                                engine.seek_to_seconds(pos);
                                            }
                                            if let Some(vol) = old_vol {
                                                engine.set_master_volume(vol);
                                            }
                                        } else {
                                            println!("[audio] Warning: failed to create audio engine for device");
                                        }

                                        self.sync_audio_clips();
                                        if was_playing {
                                            if let Some(engine) = &self.audio_engine {
                                                engine.toggle_playback();
                                            }
                                        }
                                    }

                                    self.request_redraw();
                                    return;
                                }

                                // Try developer dropdown interaction
                                let dev_consumed =
                                    self.settings_window.as_mut().map_or(false, |sw| {
                                        sw.handle_developer_click(
                                            self.mouse_pos,
                                            &mut self.settings,
                                            scr_w,
                                            scr_h,
                                            scale,
                                        )
                                    });
                                if dev_consumed {
                                    self.settings.save();
                                    self.request_redraw();
                                    return;
                                }

                                let slider_hit = self.settings_window.as_ref().and_then(|sw| {
                                    sw.slider_hit_test(
                                        self.mouse_pos,
                                        &self.settings,
                                        scr_w,
                                        scr_h,
                                        scale,
                                    )
                                });
                                if let Some(idx) = slider_hit {
                                    if let Some(sw) = &mut self.settings_window {
                                        sw.dragging_slider = Some(idx);
                                    }
                                    if let Some(sw) = &self.settings_window {
                                        sw.slider_drag(
                                            idx,
                                            self.mouse_pos[0],
                                            &mut self.settings,
                                            scr_w,
                                            scr_h,
                                            scale,
                                        );
                                    }
                                } else if let Some(cat_idx) =
                                    self.settings_window.as_ref().and_then(|sw| {
                                        sw.category_at(self.mouse_pos, scr_w, scr_h, scale)
                                    })
                                {
                                    if let Some(sw) = &mut self.settings_window {
                                        sw.active_category = CATEGORIES[cat_idx];
                                        sw.open_dropdown = None;
                                    }
                                }
                            } else {
                                self.settings_window = None;
                            }
                            self.request_redraw();
                            return;
                        }

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
                                .map_or(false, |p| matches!(p.mode, PaletteMode::VolumeFader | PaletteMode::SampleVolumeFader));

                            if is_fader {
                                if inside {
                                    let hit = self.command_palette.as_ref().map_or(false, |p| {
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
                                if matches!(action, CommandAction::SetMasterVolume | CommandAction::SetSampleVolume) {
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
                                } else {
                                    let plugin_section_y = self.plugin_section_y_offset(sh, scale);
                                    let header_h = browser::HEADER_HEIGHT * scale;
                                    let local_plugin_y = self.mouse_pos[1] - plugin_section_y;

                                    if local_plugin_y >= 0.0 && plugin_section_y >= header_h {
                                        if self.plugin_browser.hit_header(local_plugin_y, scale) {
                                            self.plugin_browser.expanded =
                                                !self.plugin_browser.expanded;
                                            self.plugin_browser.text_dirty = true;
                                            self.sample_browser.extra_content_height =
                                                self.plugin_browser.section_height(scale);
                                        } else if let Some(idx) =
                                            self.plugin_browser.item_at(local_plugin_y, scale)
                                        {
                                            let plugin = self.plugin_browser.plugins[idx].clone();
                                            self.drag = DragState::DraggingPlugin {
                                                plugin_id: plugin.unique_id,
                                                plugin_name: plugin.name,
                                            };
                                        }
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
                                } else if TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                                    let now = std::time::Instant::now();
                                    let elapsed = now.duration_since(self.last_click_time);
                                    let is_dbl = elapsed.as_millis() < 400;
                                    self.last_click_time = now;
                                    if is_dbl {
                                        self.editing_bpm = Some(String::new());
                                        self.dragging_bpm = None;
                                    } else {
                                        self.dragging_bpm = Some((self.bpm, self.mouse_pos[1]));
                                        self.editing_bpm = None;
                                    }
                                } else if let Some(engine) = &self.audio_engine {
                                    engine.toggle_playback();
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        let world = self.camera.screen_to_world(self.mouse_pos);
                        self.last_canvas_click_world = world;

                        // --- component def corner resize ---
                        {
                            let handle_sz = 24.0 / self.camera.zoom;
                            let mut corner_hit: Option<(usize, [f32; 2], bool)> = None;
                            for (ci, def) in self.components.iter().enumerate() {
                                let p = def.position;
                                let s = def.size;
                                let corners: [([f32; 2], [f32; 2], bool); 4] = [
                                    ([p[0], p[1]], [p[0] + s[0], p[1] + s[1]], true),
                                    ([p[0] + s[0], p[1]], [p[0], p[1] + s[1]], false),
                                    ([p[0], p[1] + s[1]], [p[0] + s[0], p[1]], false),
                                    ([p[0] + s[0], p[1] + s[1]], [p[0], p[1]], true),
                                ];
                                for (corner, anchor, is_nwse) in &corners {
                                    let hx = corner[0] - handle_sz * 0.5;
                                    let hy = corner[1] - handle_sz * 0.5;
                                    if point_in_rect(world, [hx, hy], [handle_sz, handle_sz]) {
                                        corner_hit = Some((ci, *anchor, *is_nwse));
                                        break;
                                    }
                                }
                                if corner_hit.is_some() {
                                    break;
                                }
                            }
                            if let Some((ci, anchor, nwse)) = corner_hit {
                                self.push_undo();
                                self.drag = DragState::ResizingComponentDef {
                                    comp_idx: ci,
                                    anchor,
                                    nwse,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- plugin label click (opens native GUI or parameter editor) ---
                        if let EffectRegionHover::PluginLabel(ri, si) = self.effect_region_hover {
                            if ri < self.effect_regions.len()
                                && si < self.effect_regions[ri].chain.len()
                            {
                                let slot = &self.effect_regions[ri].chain[si];
                                let name = slot.plugin_name.clone();
                                let uid = slot.plugin_id.clone();
                                let path = slot.plugin_path.to_string_lossy().to_string();

                                // Try to open native VST3 GUI first
                                if !path.is_empty() {
                                    // Check if GUI is already open
                                    let already_open = slot.gui.lock()
                                        .ok()
                                        .map_or(false, |g| g.as_ref().map_or(false, |gui| gui.is_open()));

                                    if !already_open {
                                        if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &uid, &name) {
                                            println!("  Opened native GUI for '{}'", name);
                                            if let Ok(mut g) = slot.gui.lock() {
                                                *g = Some(gui);
                                            }
                                            self.request_redraw();
                                            return;
                                        }
                                    } else {
                                        // GUI already open, do nothing
                                        return;
                                    }
                                }

                                // Fallback: open parameter editor
                                let mut params = Vec::new();
                                if let Ok(guard) = slot.instance.lock() {
                                    if let Some(inst) = guard.as_ref() {
                                        let count = inst.parameter_count();
                                        for idx in 0..count {
                                            let info = inst.parameter_info(idx);
                                            let val = inst.get_parameter(idx).unwrap_or(0.0);
                                            let (pname, unit, default) = match info {
                                                Ok(pi) => (pi.name, pi.unit, pi.default),
                                                Err(_) => {
                                                    (format!("Param {}", idx), String::new(), 0.0)
                                                }
                                            };
                                            params.push(ui::plugin_editor::ParamEntry {
                                                name: pname,
                                                unit,
                                                value: val,
                                                default,
                                            });
                                        }
                                    }
                                }
                                self.plugin_editor =
                                    Some(ui::plugin_editor::PluginEditorWindow::new(
                                        ri, si, name, params,
                                    ));
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- effect region corner resize ---
                        {
                            let handle_sz = 24.0 / self.camera.zoom;
                            let mut corner_hit: Option<(usize, [f32; 2], bool)> = None;
                            for (i, er) in self.effect_regions.iter().enumerate() {
                                let p = er.position;
                                let s = er.size;
                                let corners: [([f32; 2], [f32; 2], bool); 4] = [
                                    ([p[0], p[1]], [p[0] + s[0], p[1] + s[1]], true),
                                    ([p[0] + s[0], p[1]], [p[0], p[1] + s[1]], false),
                                    ([p[0], p[1] + s[1]], [p[0] + s[0], p[1]], false),
                                    ([p[0] + s[0], p[1] + s[1]], [p[0], p[1]], true),
                                ];
                                for (corner, anchor, is_nwse) in &corners {
                                    let hx = corner[0] - handle_sz * 0.5;
                                    let hy = corner[1] - handle_sz * 0.5;
                                    if point_in_rect(world, [hx, hy], [handle_sz, handle_sz]) {
                                        corner_hit = Some((i, *anchor, *is_nwse));
                                        break;
                                    }
                                }
                                if corner_hit.is_some() {
                                    break;
                                }
                            }
                            if let Some((idx, anchor, nwse)) = corner_hit {
                                self.push_undo();
                                self.drag = DragState::ResizingEffectRegion {
                                    region_idx: idx,
                                    anchor,
                                    nwse,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- export region corner resize ---
                        {
                            let handle_sz = 24.0 / self.camera.zoom;
                            let mut corner_hit: Option<(usize, [f32; 2], bool)> = None;
                            for (i, er) in self.export_regions.iter().enumerate() {
                                let p = er.position;
                                let s = er.size;
                                let corners: [([f32; 2], [f32; 2], bool); 4] = [
                                    ([p[0], p[1]], [p[0] + s[0], p[1] + s[1]], true),
                                    ([p[0] + s[0], p[1]], [p[0], p[1] + s[1]], false),
                                    ([p[0], p[1] + s[1]], [p[0] + s[0], p[1]], false),
                                    ([p[0] + s[0], p[1] + s[1]], [p[0], p[1]], true),
                                ];
                                for (corner, anchor, is_nwse) in &corners {
                                    let hx = corner[0] - handle_sz * 0.5;
                                    let hy = corner[1] - handle_sz * 0.5;
                                    if point_in_rect(world, [hx, hy], [handle_sz, handle_sz]) {
                                        corner_hit = Some((i, *anchor, *is_nwse));
                                        break;
                                    }
                                }
                                if corner_hit.is_some() {
                                    break;
                                }
                            }
                            if let Some((idx, anchor, nwse)) = corner_hit {
                                self.push_undo();
                                self.drag = DragState::ResizingExportRegion {
                                    region_idx: idx,
                                    anchor,
                                    nwse,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- export region render pill click ---
                        for er in &self.export_regions {
                            let pill_w = EXPORT_RENDER_PILL_W / self.camera.zoom;
                            let pill_h = EXPORT_RENDER_PILL_H / self.camera.zoom;
                            let pill_x = er.position[0] + 4.0 / self.camera.zoom;
                            let pill_y = er.position[1] + 4.0 / self.camera.zoom;
                            if point_in_rect(world, [pill_x, pill_y], [pill_w, pill_h]) {
                                self.trigger_export_render();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- loop region corner resize ---
                        {
                            let handle_sz = 24.0 / self.camera.zoom;
                            let mut corner_hit: Option<(usize, [f32; 2], bool)> = None;
                            for (i, lr) in self.loop_regions.iter().enumerate() {
                                if !lr.enabled {
                                    continue;
                                }
                                let p = lr.position;
                                let s = lr.size;
                                let corners: [([f32; 2], [f32; 2], bool); 4] = [
                                    ([p[0], p[1]], [p[0] + s[0], p[1] + s[1]], true),
                                    ([p[0] + s[0], p[1]], [p[0], p[1] + s[1]], false),
                                    ([p[0], p[1] + s[1]], [p[0] + s[0], p[1]], false),
                                    ([p[0] + s[0], p[1] + s[1]], [p[0], p[1]], true),
                                ];
                                for (corner, anchor, is_nwse) in &corners {
                                    let hx = corner[0] - handle_sz * 0.5;
                                    let hy = corner[1] - handle_sz * 0.5;
                                    if point_in_rect(world, [hx, hy], [handle_sz, handle_sz]) {
                                        corner_hit = Some((i, *anchor, *is_nwse));
                                        break;
                                    }
                                }
                                if corner_hit.is_some() {
                                    break;
                                }
                            }
                            if let Some((idx, anchor, nwse)) = corner_hit {
                                self.push_undo();
                                self.drag = DragState::ResizingLoopRegion {
                                    region_idx: idx,
                                    anchor,
                                    nwse,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- fade handle drag ---
                        if let Some((wf_idx, is_fade_in)) =
                            hit_test_fade_handle(&self.waveforms, world, &self.camera)
                        {
                            self.push_undo();
                            self.drag = DragState::DraggingFade {
                                waveform_idx: wf_idx,
                                is_fade_in,
                            };
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- fade curve shape drag ---
                        if let Some((wf_idx, is_fade_in)) =
                            hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
                        {
                            let wf = &self.waveforms[wf_idx];
                            let start_curve = if is_fade_in { wf.fade_in_curve } else { wf.fade_out_curve };
                            self.push_undo();
                            self.drag = DragState::DraggingFadeCurve {
                                waveform_idx: wf_idx,
                                is_fade_in,
                                start_mouse_y: self.mouse_pos[1],
                                start_curve,
                            };
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        let hit = hit_test(
                            &self.objects,
                            &self.waveforms,
                            &self.effect_regions,
                            &self.loop_regions,
                            &self.export_regions,
                            &self.components,
                            &self.component_instances,
                            self.editing_component,
                            world,
                            &self.camera,
                        );

                        // Double-click detection: enter component edit mode
                        let now = std::time::Instant::now();
                        let elapsed = now.duration_since(self.last_click_time);
                        let dist = ((world[0] - self.last_click_world[0]).powi(2)
                            + (world[1] - self.last_click_world[1]).powi(2))
                        .sqrt();
                        let is_double_click =
                            elapsed.as_millis() < 400 && dist < 10.0 / self.camera.zoom;
                        self.last_click_time = now;
                        self.last_click_world = world;

                        if is_double_click {
                            if let Some(HitTarget::ComponentDef(ci)) = hit {
                                self.editing_component = Some(ci);
                                self.selected.clear();
                                println!(
                                    "Entered component edit mode: {}",
                                    self.components[ci].name
                                );
                                self.request_redraw();
                                return;
                            }
                        }

                        // Click outside the editing component exits edit mode
                        if let Some(ec_idx) = self.editing_component {
                            if let Some(def) = self.components.get(ec_idx) {
                                if !point_in_rect(world, def.position, def.size) {
                                    self.editing_component = None;
                                    self.selected.clear();
                                    println!("Exited component edit mode");
                                    // Re-do hit test without edit mode
                                    let hit2 = hit_test(
                                        &self.objects,
                                        &self.waveforms,
                                        &self.effect_regions,
                                        &self.loop_regions,
                                        &self.export_regions,
                                        &self.components,
                                        &self.component_instances,
                                        None,
                                        world,
                                        &self.camera,
                                    );
                                    if let Some(target) = hit2 {
                                        self.selected.push(target);
                                        self.begin_move_selection(world, self.modifiers.alt_key());
                                    } else {
                                        self.drag = DragState::Selecting { start_world: world };
                                    }
                                    self.update_cursor();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        match hit {
                            Some(target) => {
                                self.select_area = None;
                                if self.selected.contains(&target) {
                                    // Already selected -> drag whole selection
                                } else {
                                    self.selected.clear();
                                    self.selected.push(target);
                                }
                                self.begin_move_selection(world, self.modifiers.alt_key());
                            }
                            None => {
                                self.drag = DragState::Selecting { start_world: world };
                            }
                        }

                        self.update_cursor();
                        self.request_redraw();
                    }

                    ElementState::Released => {
                        // Finish plugin editor slider drag
                        if let Some(pe) = &mut self.plugin_editor {
                            if pe.dragging_slider.is_some() {
                                pe.dragging_slider = None;
                                self.request_redraw();
                                return;
                            }
                        }

                        // Finish settings slider drag
                        if let Some(sw) = &mut self.settings_window {
                            if sw.dragging_slider.is_some() {
                                sw.dragging_slider = None;
                                self.settings.save();
                                self.request_redraw();
                                return;
                            }
                        }

                        if self.dragging_bpm.is_some() {
                            self.dragging_bpm = None;
                            self.bpm = self.bpm.round();
                            self.mark_dirty();
                            self.request_redraw();
                            return;
                        }

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

                        // --- finish resizing component def ---
                        if matches!(self.drag, DragState::ResizingComponentDef { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing effect region ---
                        if matches!(self.drag, DragState::ResizingEffectRegion { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing export region ---
                        if matches!(self.drag, DragState::ResizingExportRegion { .. }) {
                            self.drag = DragState::None;
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing loop region ---
                        if matches!(self.drag, DragState::ResizingLoopRegion { .. }) {
                            self.drag = DragState::None;
                            self.sync_loop_region();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish fade handle drag ---
                        if matches!(self.drag, DragState::DraggingFade { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish fade curve drag ---
                        if matches!(self.drag, DragState::DraggingFadeCurve { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
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

                        // --- drop plugin from browser to canvas/effect region ---
                        if let DragState::DraggingPlugin {
                            ref plugin_id,
                            ref plugin_name,
                        } = self.drag
                        {
                            let plugin_id = plugin_id.clone();
                            let plugin_name = plugin_name.clone();
                            let (_, sh, scale) = self.screen_info();
                            let in_browser = self.sample_browser.visible
                                && self.sample_browser.contains(self.mouse_pos, sh, scale);
                            if !in_browser {
                                let world = self.camera.screen_to_world(self.mouse_pos);
                                let hit_er = self
                                    .effect_regions
                                    .iter()
                                    .enumerate()
                                    .rev()
                                    .find(|(_, er)| point_in_rect(world, er.position, er.size))
                                    .map(|(i, _)| i);

                                if let Some(er_idx) = hit_er {
                                    self.add_plugin_to_region(er_idx, &plugin_id, &plugin_name);
                                } else {
                                    self.push_undo();
                                    let w = effects::EFFECT_REGION_DEFAULT_WIDTH;
                                    let h = effects::EFFECT_REGION_DEFAULT_HEIGHT;
                                    self.effect_regions.push(effects::EffectRegion::new(
                                        [world[0] - w * 0.5, world[1] - h * 0.5],
                                        [w, h],
                                    ));
                                    let idx = self.effect_regions.len() - 1;
                                    self.add_plugin_to_region(idx, &plugin_id, &plugin_name);
                                    self.selected.clear();
                                    self.selected.push(HitTarget::EffectRegion(idx));
                                }
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
                                let snapped_x = snap_to_grid(current[0], &self.settings, self.camera.zoom, self.bpm);
                                if let Some(engine) = &self.audio_engine {
                                    let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                    engine.seek_to_seconds(secs);
                                }
                                let (_, sh, _) = self.screen_info();
                                let world_top = self.camera.screen_to_world([0.0, 0.0])[1];
                                let world_bottom = self.camera.screen_to_world([0.0, sh])[1];
                                let line_w = 2.0 / self.camera.zoom;
                                self.select_area = Some(SelectArea {
                                    position: [snapped_x, world_top],
                                    size: [line_w, world_bottom - world_top],
                                });
                            } else {
                                self.selected = targets_in_rect(
                                    &self.objects,
                                    &self.waveforms,
                                    &self.effect_regions,
                                    &self.loop_regions,
                                    &self.export_regions,
                                    &self.components,
                                    &self.component_instances,
                                    self.editing_component,
                                    rp,
                                    rs,
                                );
                                self.select_area = Some(SelectArea { position: rp, size: rs });
                            }
                        }

                        self.drag = DragState::None;
                        self.sync_audio_clips();
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
                    if self.plugin_editor.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.plugin_editor = None;
                            self.request_redraw();
                            return;
                        }
                        return;
                    }

                    if self.settings_window.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.settings_window = None;
                            self.request_redraw();
                            return;
                        }
                        // Block other keyboard input while settings is open
                        if !self.modifiers.super_key() {
                            return;
                        }
                    }

                    if self.context_menu.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.context_menu = None;
                            self.request_redraw();
                            return;
                        }
                    }

                    if self.editing_component.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.editing_component = None;
                            self.selected.clear();
                            println!("Exited component edit mode");
                            self.request_redraw();
                            return;
                        }
                    }

                    // --- BPM editing input ---
                    if self.editing_bpm.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_bpm = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some(text) = self.editing_bpm.take() {
                                    if let Ok(val) = text.parse::<f32>() {
                                        self.bpm = val.clamp(20.0, 999.0);
                                        self.mark_dirty();
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(ref mut text) = self.editing_bpm {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
                                    if let Some(ref mut text) = self.editing_bpm {
                                        text.push_str(s);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- effect region name editing input ---
                    if self.editing_effect_name.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_effect_name = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((idx, text)) = self.editing_effect_name.take() {
                                    if idx < self.effect_regions.len() {
                                        self.push_undo();
                                        let name = if text.trim().is_empty() {
                                            "effects".to_string()
                                        } else {
                                            text
                                        };
                                        self.effect_regions[idx].name = name;
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.push(' ');
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.push_str(ch.as_ref());
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- waveform name editing input ---
                    if self.editing_waveform_name.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_waveform_name = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((idx, text)) = self.editing_waveform_name.take() {
                                    if idx < self.waveforms.len() {
                                        self.push_undo();
                                        let name = if text.trim().is_empty() {
                                            self.waveforms[idx].audio.filename.clone()
                                        } else {
                                            text
                                        };
                                        let mut new_audio = (*self.waveforms[idx].audio).clone();
                                        new_audio.filename = name;
                                        self.waveforms[idx].audio = Arc::new(new_audio);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.push(' ');
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.push_str(ch.as_ref());
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
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
                                    if matches!(a, CommandAction::SetMasterVolume | CommandAction::SetSampleVolume) {
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
                                    p.update_filter(self.settings.dev_mode);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.push_str(ch.as_ref());
                                    p.update_filter(self.settings.dev_mode);
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- Enter on selected effect region: show plugin info ---
                    if matches!(event.logical_key, Key::Named(NamedKey::Enter)) {
                        if let Some(HitTarget::EffectRegion(idx)) = self.selected.first().copied() {
                            if idx < self.effect_regions.len() {
                                let er = &self.effect_regions[idx];
                                if er.chain.is_empty() {
                                    println!("  Effect region {} has no plugins", idx);
                                } else {
                                    println!("  Effect region {} plugin chain:", idx);
                                    for (j, slot) in er.chain.iter().enumerate() {
                                        let param_count = slot
                                            .instance
                                            .lock()
                                            .ok()
                                            .and_then(|g| g.as_ref().map(|p| p.parameter_count()))
                                            .unwrap_or(0);
                                        println!(
                                            "    [{}] {} ({} params)",
                                            j, slot.plugin_name, param_count
                                        );
                                    }
                                }
                            }
                            self.request_redraw();
                        }
                    }

                    // --- global shortcuts ---
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.selected.clear();
                            self.select_area = None;
                            self.request_redraw();
                        }
                        Key::Named(NamedKey::Space) => {
                            if self.is_recording() {
                                self.toggle_recording();
                                self.request_redraw();
                            } else if let Some(engine) = &self.audio_engine {
                                if !engine.is_playing() {
                                    if let Some(sa) = &self.select_area {
                                        let secs = sa.position[0] as f64 / PIXELS_PER_SECOND as f64;
                                        engine.seek_to_seconds(secs);
                                    }
                                }
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
                        Key::Character(ch) if !self.modifiers.super_key() => match ch.as_ref() {
                            "0" => {
                                let wf_indices: Vec<usize> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::Waveform(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                let lr_indices: Vec<usize> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::LoopRegion(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                if !wf_indices.is_empty() || !lr_indices.is_empty() {
                                    self.push_undo();
                                    if !wf_indices.is_empty() {
                                        let any_enabled = wf_indices.iter().any(|&i| i < self.waveforms.len() && !self.waveforms[i].disabled);
                                        let new_disabled = any_enabled;
                                        for &i in &wf_indices {
                                            if i < self.waveforms.len() {
                                                self.waveforms[i].disabled = new_disabled;
                                            }
                                        }
                                    }
                                    if !lr_indices.is_empty() {
                                        let any_enabled = lr_indices.iter().any(|&i| i < self.loop_regions.len() && self.loop_regions[i].enabled);
                                        let new_enabled = !any_enabled;
                                        for &i in &lr_indices {
                                            if i < self.loop_regions.len() {
                                                self.loop_regions[i].enabled = new_enabled;
                                            }
                                        }
                                        self.sync_loop_region();
                                    }
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                }
                            }
                            _ => {}
                        },
                        Key::Character(ch) if self.modifiers.super_key() => match ch.as_ref() {
                            "," => {
                                self.command_palette = None;
                                self.context_menu = None;
                                self.settings_window = if self.settings_window.is_some() {
                                    None
                                } else {
                                    Some(SettingsWindow::new())
                                };
                                self.request_redraw();
                            }
                            "k" | "t" => {
                                self.context_menu = None;
                                self.settings_window = None;
                                self.command_palette = if self.command_palette.is_some() {
                                    None
                                } else {
                                    Some(CommandPalette::new(self.settings.dev_mode))
                                };
                                self.request_redraw();
                            }
                            "b" => {
                                self.sample_browser.visible = !self.sample_browser.visible;
                                if self.sample_browser.visible {
                                    self.ensure_plugins_scanned();
                                }
                                self.request_redraw();
                            }
                            "a" if self.modifiers.shift_key() => {
                                self.open_add_folder_dialog();
                            }
                            "r" => {
                                let has_er = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::EffectRegion(_)));
                                let has_wf = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::Waveform(_)));
                                if has_er {
                                    self.execute_command(CommandAction::RenameEffectRegion);
                                } else if has_wf {
                                    self.execute_command(CommandAction::RenameSample);
                                } else {
                                    self.toggle_recording();
                                }
                                self.request_redraw();
                            }
                            "c" => {
                                self.copy_selected();
                                self.request_redraw();
                            }
                            "v" => {
                                self.paste_clipboard();
                                self.request_redraw();
                            }
                            "d" => {
                                self.duplicate_selected();
                                self.request_redraw();
                            }
                            "e" => {
                                self.execute_command(CommandAction::SplitSample);
                            }
                            "l" => {
                                self.execute_command(CommandAction::AddLoopArea);
                            }
                            "s" => self.save_project(),
                            "z" => {
                                if self.modifiers.shift_key() {
                                    self.redo();
                                } else {
                                    self.undo();
                                }
                            }
                            "1" => {
                                self.execute_command(CommandAction::NarrowGrid);
                            }
                            "2" => {
                                self.execute_command(CommandAction::WidenGrid);
                            }
                            "3" => {
                                self.execute_command(CommandAction::ToggleTripletGrid);
                            }
                            "4" => {
                                self.execute_command(CommandAction::ToggleSnapToGrid);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }

            // --- scroll = pan, Cmd+scroll = zoom, pinch = zoom ---
            WindowEvent::MouseWheel { delta, .. } => {
                if self.command_palette.is_some() || self.context_menu.is_some() {
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
                if self.command_palette.is_some() || self.context_menu.is_some() {
                    return;
                }
                let factor = (1.0 + delta as f32).clamp(0.5, 2.0);
                self.camera.zoom_at(self.mouse_pos, factor);
                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.toast_manager.tick();
                self.update_recording_waveform();
                let (_pre_w, pre_h, pre_scale) = self.screen_info();
                self.sample_browser.extra_content_height =
                    self.plugin_browser.section_height(pre_scale);
                let plugin_section_y = self.plugin_section_y_offset(pre_h, pre_scale);
                let plugin_panel_w = self.sample_browser.panel_width(pre_scale);
                let clip_top = browser::HEADER_HEIGHT * pre_scale;
                if self.sample_browser.visible && !self.plugin_browser.plugins.is_empty() {
                    self.plugin_browser.get_text_entries(
                        plugin_panel_w,
                        plugin_section_y,
                        pre_scale,
                        clip_top,
                        pre_h,
                    );
                }
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

                    let camera_moved = self.camera.position != self.last_rendered_camera_pos
                        || self.camera.zoom != self.last_rendered_camera_zoom;
                    let hover_changed = self.hovered != self.last_rendered_hovered;
                    let sel_changed = self.selected.len() != self.last_rendered_selected_len;
                    let gen_changed = self.render_generation != self.last_rendered_generation;
                    let needs_rebuild = camera_moved
                        || hover_changed
                        || sel_changed
                        || gen_changed
                        || playhead_world_x.is_some()
                        || sel_rect.is_some()
                        || self.file_hovering;

                    if needs_rebuild {
                        let selected_set: HashSet<HitTarget> =
                            self.selected.iter().copied().collect();
                        let component_map: std::collections::HashMap<
                            component::ComponentId,
                            usize,
                        > = self
                            .components
                            .iter()
                            .enumerate()
                            .map(|(i, c)| (c.id, i))
                            .collect();
                        let render_ctx = RenderContext {
                            camera: &self.camera,
                            screen_w: w,
                            screen_h: h,
                            objects: &self.objects,
                            waveforms: &self.waveforms,
                            effect_regions: &self.effect_regions,
                            hovered: self.hovered,
                            selected: &selected_set,
                            selection_rect: sel_rect,
                            select_area: self.select_area.as_ref(),
                            file_hovering: self.file_hovering,
                            playhead_world_x,
                            export_regions: &self.export_regions,
                            loop_regions: &self.loop_regions,
                            components: &self.components,
                            component_instances: &self.component_instances,
                            editing_component: self.editing_component,
                            settings: &self.settings,
                            component_map: &component_map,
                            fade_curve_hovered: self.fade_curve_hovered,
                            fade_curve_dragging: if let DragState::DraggingFadeCurve { waveform_idx, is_fade_in, .. } = self.drag {
                                Some((waveform_idx, is_fade_in))
                            } else {
                                None
                            },
                            mouse_world: self.camera.screen_to_world(self.mouse_pos),
                            bpm: self.bpm,
                        };
                        build_instances(&mut self.cached_instances, &render_ctx);
                        build_waveform_vertices(&mut self.cached_wf_verts, &render_ctx);

                        self.last_rendered_generation = self.render_generation;
                        self.last_rendered_camera_pos = self.camera.position;
                        self.last_rendered_camera_zoom = self.camera.zoom;
                        self.last_rendered_hovered = self.hovered;
                        self.last_rendered_selected_len = self.selected.len();
                    }

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
                        } else if let DragState::DraggingPlugin {
                            ref plugin_name, ..
                        } = self.drag
                        {
                            Some((plugin_name.as_str(), self.mouse_pos))
                        } else {
                            None
                        };

                    if let Some(p) = &mut self.command_palette {
                        if p.mode == PaletteMode::VolumeFader {
                            p.fader_rms = self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak());
                        }
                    }

                    let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
                    let playback_pos = self
                        .audio_engine
                        .as_ref()
                        .map_or(0.0, |e| e.position_seconds());
                    let is_recording = self.recorder.as_ref().map_or(false, |r| r.is_recording());

                    let plugin_browser_ref =
                        if self.sample_browser.visible && !self.plugin_browser.plugins.is_empty() {
                            Some((&self.plugin_browser, plugin_section_y))
                        } else {
                            None
                        };

                    gpu.render(
                        &self.camera,
                        &self.cached_instances,
                        &self.cached_wf_verts,
                        self.command_palette.as_ref(),
                        self.context_menu.as_ref(),
                        browser_ref,
                        plugin_browser_ref,
                        drag_ghost,
                        is_playing,
                        is_recording,
                        playback_pos,
                        &self.export_regions,
                        &self.effect_regions,
                        self.editing_effect_name
                            .as_ref()
                            .map(|(idx, s)| (*idx, s.as_str())),
                        &self.waveforms,
                        self.editing_waveform_name
                            .as_ref()
                            .map(|(idx, s)| (*idx, s.as_str())),
                        self.plugin_editor.as_ref(),
                        self.settings_window.as_ref(),
                        &self.settings,
                        &self.toast_manager,
                        self.bpm,
                        self.editing_bpm.as_deref(),
                    );
                }
                if self.toast_manager.has_active() {
                    self.request_redraw();
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Native macOS menu bar
// ---------------------------------------------------------------------------

fn build_app_menu(storage: Option<&Storage>) -> MenuState {
    use muda::{
        accelerator::{Accelerator, Code, Modifiers},
        Menu, MenuItem, PredefinedMenuItem, Submenu,
    };

    let menu = Menu::new();

    // -- App menu (Layers) --
    let app_menu = Submenu::new("Layers", true);
    let _ = app_menu.append(&PredefinedMenuItem::about(None, None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let settings_item = MenuItem::new(
        "Settings...",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
    );
    let _ = app_menu.append(&settings_item);
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let _ = app_menu.append(&PredefinedMenuItem::quit(None));
    let _ = menu.append(&app_menu);

    // -- File menu --
    let file_menu = Submenu::new("File", true);
    let new_project_item = MenuItem::new(
        "New Project",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
    );
    let _ = file_menu.append(&new_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let save_project_item = MenuItem::new(
        "Save Project",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyS)),
    );
    let _ = file_menu.append(&save_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());

    let open_submenu = Submenu::new("Open Project", true);
    let mut open_items: Vec<(MenuId, String)> = Vec::new();
    if let Some(s) = storage {
        for entry in s.list_projects() {
            let item = MenuItem::new(&entry.name, true, None);
            open_items.push((item.id().clone(), entry.path.clone()));
            let _ = open_submenu.append(&item);
        }
    }
    if open_items.is_empty() {
        let _ = open_submenu.append(&MenuItem::new("No Projects", false, None));
    }
    let _ = file_menu.append(&open_submenu);
    let _ = menu.append(&file_menu);

    // -- Edit menu --
    let edit_menu = Submenu::new("Edit", true);
    let _ = edit_menu.append(&PredefinedMenuItem::undo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::redo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&PredefinedMenuItem::copy(None));
    let _ = edit_menu.append(&PredefinedMenuItem::paste(None));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&PredefinedMenuItem::select_all(None));
    let _ = menu.append(&edit_menu);

    MenuState {
        menu,
        new_project: new_project_item.id().clone(),
        save_project: save_project_item.id().clone(),
        settings: settings_item.id().clone(),
        open_project_items: open_items,
        open_submenu,
        initialized: false,
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

    let skip_load = std::env::args().any(|a| a == "--empty");

    let event_loop = EventLoop::new().unwrap();

    let mut app = App::new(skip_load);
    let menu_state = build_app_menu(app.storage.as_ref());
    app.menu_state = Some(menu_state);

    event_loop.run_app(&mut app).unwrap();
}
