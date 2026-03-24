use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};


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
    pub fn get_latency_samples(&self) -> u32 { 0 }
    pub fn latency_changed(&self) -> bool { false }
}

// ---------------------------------------------------------------------------
// EffectChain — ordered list of VST3 effect plugins attached to a waveform
// ---------------------------------------------------------------------------

/// A shared, ordered effect chain that can be referenced by multiple waveforms.
/// When a waveform is duplicated, the copy shares the same chain (by ID).
#[derive(Clone)]
pub struct EffectChain {
    pub slots: Vec<EffectChainSlot>,
}

/// A single slot in an effect chain — one VST3 plugin instance.
#[derive(Clone)]
pub struct EffectChainSlot {
    pub id: crate::entity_id::EntityId,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub bypass: bool,
    pub gui: Arc<Mutex<Option<PluginGuiHandle>>>,
    pub pending_state: Option<Vec<u8>>,
    pub pending_params: Option<Vec<f64>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EffectChainSlotSnapshot {
    pub id: crate::entity_id::EntityId,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub bypass: bool,
}

impl EffectChainSlot {
    pub fn new(plugin_id: String, plugin_name: String, plugin_path: PathBuf) -> Self {
        Self {
            id: crate::entity_id::new_id(),
            plugin_id,
            plugin_name,
            plugin_path,
            bypass: false,
            gui: Arc::new(Mutex::new(None)),
            pending_state: None,
            pending_params: None,
        }
    }

    pub fn snapshot(&self) -> EffectChainSlotSnapshot {
        EffectChainSlotSnapshot {
            id: self.id,
            plugin_id: self.plugin_id.clone(),
            plugin_name: self.plugin_name.clone(),
            plugin_path: self.plugin_path.clone(),
            bypass: self.bypass,
        }
    }
}

impl EffectChain {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// How many waveforms reference this chain (computed externally).
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
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
