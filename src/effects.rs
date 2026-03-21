use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{point_in_rect, push_border, rects_overlap, Camera, InstanceRaw};

// ---------------------------------------------------------------------------
// Plugin cache (native only)
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Serialize, Deserialize)]
struct CachedPluginInfo {
    name: String,
    manufacturer: String,
    subcategories: String,
    path: String,
    unique_id: String,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Serialize, Deserialize)]
struct PluginCache {
    version: u32,
    plugins: Vec<CachedPluginInfo>,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".layers")
        .join("vst_plugin_cache.json")
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn load_cache() -> Option<Vec<vst3_gui::ScannedPlugin>> {
    let data = std::fs::read_to_string(cache_path()).ok()?;
    let cache: PluginCache = serde_json::from_str(&data).ok()?;
    if cache.version != 2 {
        return None;
    }
    Some(
        cache
            .plugins
            .into_iter()
            .map(|c| vst3_gui::ScannedPlugin {
                name: c.name,
                vendor: c.manufacturer,
                uid: c.unique_id,
                path: PathBuf::from(c.path),
                subcategories: c.subcategories,
            })
            .collect(),
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn save_cache(plugins: &[vst3_gui::ScannedPlugin]) {
    let cache = PluginCache {
        version: 2,
        plugins: plugins
            .iter()
            .map(|p| CachedPluginInfo {
                name: p.name.clone(),
                manufacturer: p.vendor.clone(),
                subcategories: p.subcategories.clone(),
                path: p.path.to_string_lossy().to_string(),
                unique_id: p.uid.clone(),
            })
            .collect(),
    };
    if let Some(parent) = cache_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(cache_path(), json) {
                println!("  VST3 cache write error: {}", e);
            }
        }
        Err(e) => println!("  VST3 cache serialize error: {}", e),
    }
}

pub const EFFECT_REGION_DEFAULT_WIDTH: f32 = 600.0;
pub const EFFECT_REGION_DEFAULT_HEIGHT: f32 = 250.0;

// ---------------------------------------------------------------------------
// EffectRegion — spatial zone that controls when plugins sound
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EffectRegion {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub name: String,
}

impl EffectRegion {
    pub fn new(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            position,
            size,
            name: "effects".to_string(),
        }
    }

    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let name_area_h = 30.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(world_pos, [p[0] - border_thickness, p[1] - border_thickness - name_area_h],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0 + name_area_h]) {
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
        // FX badge area
        let badge_w = 28.0 / camera.zoom;
        let badge_h = 16.0 / camera.zoom;
        if point_in_rect(world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [badge_w + 100.0 / camera.zoom, badge_h]) {
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// PluginBlock — first-class canvas object for a single plugin
// ---------------------------------------------------------------------------

pub const PLUGIN_BLOCK_DEFAULT_SIZE: [f32; 2] = [120.0, 40.0];
pub use crate::theme::PLUGIN_BLOCK_DEFAULT_COLOR;
pub const PLUGIN_BLOCK_BORDER_RADIUS: f32 = 6.0;

/// Native GUI handle type — vst3_gui::Vst3Gui on macOS, stub elsewhere.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub type PluginGuiHandle = vst3_gui::Vst3Gui;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub type PluginGuiHandle = PluginGuiStub;

/// Stub that provides the same API as vst3_gui::Vst3Gui but does nothing.
/// Used on platforms where VST3 is not available (WASM, Linux).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Clone)]
pub struct PluginGuiStub;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PluginGuiStub {
    pub fn is_open(&self) -> bool { false }
    pub fn hide(&self) {}
    pub fn show(&self) {}
    pub fn get_size(&self) -> Option<(f32, f32)> { None }
    pub fn parameter_count(&self) -> usize { 0 }
    pub fn get_parameter(&self, _index: usize) -> Option<f64> { None }
    pub fn set_parameter(&self, _index: usize, _value: f64) -> bool { false }
    pub fn get_state(&self) -> Option<Vec<u8>> { None }
    pub fn set_state(&self, _data: &[u8]) -> bool { false }
    pub fn get_all_parameters(&self) -> Vec<f64> { Vec::new() }
    pub fn set_all_parameters(&self, _values: &[f64]) {}
    pub fn setup_processing(&self, _sample_rate: f64, _block_size: i32) -> bool { false }
    pub fn process(&self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _num_frames: usize) -> bool { false }
    pub fn send_midi_note_on(&self, _note: u8, _velocity: u8, _channel: u8, _sample_offset: i32) {}
    pub fn send_midi_note_off(&self, _note: u8, _velocity: u8, _channel: u8, _sample_offset: i32) {}
    pub fn audio_input_channels(&self) -> usize { 0 }
    pub fn audio_output_channels(&self) -> usize { 0 }
}

#[derive(Clone)]
pub struct PluginBlock {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub bypass: bool,
    pub gui: Arc<Mutex<Option<PluginGuiHandle>>>,
    pub pending_state: Option<Vec<u8>>,
    pub pending_params: Option<Vec<f64>>,
}

impl PluginBlock {
    pub fn new(position: [f32; 2], plugin_id: String, plugin_name: String, plugin_path: PathBuf) -> Self {
        Self {
            position,
            size: PLUGIN_BLOCK_DEFAULT_SIZE,
            color: PLUGIN_BLOCK_DEFAULT_COLOR,
            plugin_id,
            plugin_name,
            plugin_path,
            bypass: false,
            gui: Arc::new(Mutex::new(None)),
            pending_state: None,
            pending_params: None,
        }
    }

    pub fn snapshot(&self) -> PluginBlockSnapshot {
        PluginBlockSnapshot {
            position: self.position,
            size: self.size,
            color: self.color,
            plugin_id: self.plugin_id.clone(),
            plugin_name: self.plugin_name.clone(),
            plugin_path: self.plugin_path.clone(),
            bypass: self.bypass,
        }
    }

    pub fn contains(&self, world_pos: [f32; 2]) -> bool {
        point_in_rect(world_pos, self.position, self.size)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PluginBlockSnapshot {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub bypass: bool,
}

/// Returns EntityIds of plugin_blocks that spatially overlap the given effect region,
/// sorted by X position (left-to-right chaining order).
pub fn collect_plugins_for_region(
    region: &EffectRegion,
    blocks: &indexmap::IndexMap<crate::entity_id::EntityId, PluginBlock>,
) -> Vec<crate::entity_id::EntityId> {
    let mut overlapping: Vec<(crate::entity_id::EntityId, f32)> = blocks
        .iter()
        .filter(|(_, b)| {
            !b.bypass
                && rects_overlap(region.position, region.size, b.position, b.size)
        })
        .map(|(&id, b)| (id, b.position[0]))
        .collect();
    overlapping.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
    });
    overlapping.into_iter().map(|(id, _)| id).collect()
}

pub fn build_plugin_block_instances(
    block: &PluginBlock,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    theme: &crate::theme::RuntimeTheme,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let mut color = block.color;
    if block.bypass {
        color[3] *= 0.4;
    }
    if is_hovered && !is_selected {
        color[3] = (color[3] + 0.10).min(1.0);
    }

    // Main block rectangle
    out.push(InstanceRaw {
        position: block.position,
        size: block.size,
        color,
        border_radius: PLUGIN_BLOCK_BORDER_RADIUS / camera.zoom,
    });

    // Selection border
    if is_selected {
        let bw = 2.0 / camera.zoom;
        push_border(&mut out, block.position, block.size, bw, [0.35, 0.65, 1.0, 0.8]);
    }

    out
}

// ---------------------------------------------------------------------------
// EffectRegion rendering (no more pill labels)
// ---------------------------------------------------------------------------

pub fn build_effect_region_instances(
    region: &EffectRegion,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    is_active: bool,
    theme: &crate::theme::RuntimeTheme,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let border_color = if is_active {
        theme.effect_active_border
    } else {
        theme.effect_border_color
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
            color: [0.25, 0.50, 0.90, 0.40],
            border_radius: 1.0 / camera.zoom,
        });
        x += dash_w + gap;
    }

    // "FX" badge at top-left
    let badge_w = 28.0 / camera.zoom;
    let badge_h = 16.0 / camera.zoom;
    out.push(InstanceRaw {
        position: [
            region.position[0] + 4.0 / camera.zoom,
            region.position[1] + 4.0 / camera.zoom,
        ],
        size: [badge_w, badge_h],
        color: [0.25, 0.50, 0.90, 0.70],
        border_radius: 3.0 / camera.zoom,
    });

    if is_selected {
        let handle_sz = 8.0 / camera.zoom;
        let handle_color = [0.25, 0.50, 0.90, 0.90];
        for &hx in &[region.position[0] - handle_sz * 0.5, region.position[0] + region.size[0] - handle_sz * 0.5] {
            for &hy in &[region.position[1] - handle_sz * 0.5, region.position[1] + region.size[1] - handle_sz * 0.5] {
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

// ---------------------------------------------------------------------------
// PluginRegistry (native only)
// ---------------------------------------------------------------------------

pub struct PluginRegistryEntry {
    pub info: PluginEntryInfo,
}

pub struct PluginEntryInfo {
    pub unique_id: String,
    pub name: String,
    pub manufacturer: String,
    pub path: PathBuf,
}

pub struct PluginRegistry {
    pub plugins: Vec<PluginRegistryEntry>,
    pub instruments: Vec<PluginRegistryEntry>,
    scanned: bool,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            instruments: Vec::new(),
            scanned: false,
        }
    }

    pub fn is_scanned(&self) -> bool {
        self.scanned
    }

    pub fn ensure_scanned(&mut self) {
        if !self.scanned {
            self.scan();
        }
    }

    pub fn scan(&mut self) {
        self.scanned = true;

        let split_plugins = |all: Vec<vst3_gui::ScannedPlugin>| -> (Vec<vst3_gui::ScannedPlugin>, Vec<vst3_gui::ScannedPlugin>) {
            let mut effects = Vec::new();
            let mut instruments = Vec::new();
            for p in all {
                if p.is_instrument() {
                    instruments.push(p);
                } else {
                    effects.push(p);
                }
            }
            (effects, instruments)
        };

        let to_entries = |plugins: Vec<vst3_gui::ScannedPlugin>| -> Vec<PluginRegistryEntry> {
            plugins
                .into_iter()
                .map(|p| PluginRegistryEntry {
                    info: PluginEntryInfo {
                        unique_id: p.uid,
                        name: p.name,
                        manufacturer: p.vendor,
                        path: p.path,
                    },
                })
                .collect()
        };

        if let Some(cached) = load_cache() {
            let (effects, instruments) = split_plugins(cached);
            println!("  VST3 (cached): found {} effect, {} instrument plugins", effects.len(), instruments.len());
            for p in &effects {
                println!("    - {} ({})", p.name, p.vendor);
            }
            for p in &instruments {
                println!("    - [INST] {} ({})", p.name, p.vendor);
            }
            self.plugins = to_entries(effects);
            self.instruments = to_entries(instruments);
            return;
        }

        let all = vst3_gui::scan_plugins();
        if all.is_empty() {
            println!("  VST3: no plugins found");
            return;
        }
        save_cache(&all);
        let (effects, instruments) = split_plugins(all);
        println!("  VST3: found {} effect, {} instrument plugins", effects.len(), instruments.len());
        for p in &effects {
            println!("    - {} ({})", p.name, p.vendor);
        }
        for p in &instruments {
            println!("    - [INST] {} ({})", p.name, p.vendor);
        }
        self.plugins = to_entries(effects);
        self.instruments = to_entries(instruments);
    }

    pub fn rescan(&mut self) {
        let _ = std::fs::remove_file(cache_path());
        self.scanned = false;
        self.plugins.clear();
        self.instruments.clear();
        self.scan();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            instruments: Vec::new(),
            scanned: false,
        }
    }

    pub fn is_scanned(&self) -> bool {
        self.scanned
    }

    pub fn ensure_scanned(&mut self) {
        self.scanned = true;
    }
}
