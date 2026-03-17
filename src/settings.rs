#[cfg(feature = "native")]
use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Grid types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptiveGridSize {
    Widest,
    Wide,
    Medium,
    Narrow,
    Narrowest,
}

impl AdaptiveGridSize {
    pub fn target_px(self) -> f32 {
        match self {
            Self::Widest => 200.0,
            Self::Wide => 140.0,
            Self::Medium => 100.0,
            Self::Narrow => 60.0,
            Self::Narrowest => 35.0,
        }
    }

    pub fn narrower(self) -> Self {
        match self {
            Self::Widest => Self::Wide,
            Self::Wide => Self::Medium,
            Self::Medium => Self::Narrow,
            Self::Narrow => Self::Narrowest,
            Self::Narrowest => Self::Narrowest,
        }
    }

    pub fn wider(self) -> Self {
        match self {
            Self::Widest => Self::Widest,
            Self::Wide => Self::Widest,
            Self::Medium => Self::Wide,
            Self::Narrow => Self::Medium,
            Self::Narrowest => Self::Narrow,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Widest => "Widest",
            Self::Wide => "Wide",
            Self::Medium => "Medium",
            Self::Narrow => "Narrow",
            Self::Narrowest => "Narrowest",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixedGrid {
    Bars8,
    Bars4,
    Bars2,
    Bar1,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl FixedGrid {
    pub fn beats(self) -> f32 {
        match self {
            Self::Bars8 => 32.0,
            Self::Bars4 => 16.0,
            Self::Bars2 => 8.0,
            Self::Bar1 => 4.0,
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bars8 => "8 Bars",
            Self::Bars4 => "4 Bars",
            Self::Bars2 => "2 Bars",
            Self::Bar1 => "1 Bar",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
        }
    }

    pub fn finer(self) -> Self {
        match self {
            Self::Bars8 => Self::Bars4,
            Self::Bars4 => Self::Bars2,
            Self::Bars2 => Self::Bar1,
            Self::Bar1 => Self::Half,
            Self::Half => Self::Quarter,
            Self::Quarter => Self::Eighth,
            Self::Eighth => Self::Sixteenth,
            Self::Sixteenth => Self::ThirtySecond,
            Self::ThirtySecond => Self::ThirtySecond,
        }
    }

    pub fn coarser(self) -> Self {
        match self {
            Self::Bars8 => Self::Bars8,
            Self::Bars4 => Self::Bars8,
            Self::Bars2 => Self::Bars4,
            Self::Bar1 => Self::Bars2,
            Self::Half => Self::Bar1,
            Self::Quarter => Self::Half,
            Self::Eighth => Self::Quarter,
            Self::Sixteenth => Self::Eighth,
            Self::ThirtySecond => Self::Sixteenth,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum GridMode {
    Adaptive(AdaptiveGridSize),
    Fixed(FixedGrid),
}

impl Default for GridMode {
    fn default() -> Self {
        Self::Fixed(FixedGrid::Quarter)
    }
}

// ---------------------------------------------------------------------------
// Persisted settings
// ---------------------------------------------------------------------------

fn default_grid_enabled() -> bool { true }
fn default_snap_to_grid() -> bool { true }
fn default_grid_mode() -> GridMode { GridMode::default() }
fn default_triplet_grid() -> bool { false }

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub grid_line_intensity: f32,
    pub brightness: f32,
    pub color_intensity: f32,
    #[serde(default = "default_driver_type")]
    pub audio_driver_type: String,
    #[serde(default = "default_input_device")]
    pub audio_input_device: String,
    #[serde(default = "default_output_device")]
    pub audio_output_device: String,
    #[serde(default = "default_grid_enabled")]
    pub grid_enabled: bool,
    #[serde(default = "default_snap_to_grid")]
    pub snap_to_grid: bool,
    #[serde(default = "default_grid_mode")]
    pub grid_mode: GridMode,
    #[serde(default = "default_triplet_grid")]
    pub triplet_grid: bool,
    #[serde(default)]
    pub dev_mode: bool,
    #[serde(default)]
    pub sample_library_folders: Vec<String>,
}

#[cfg(feature = "native")]
fn default_driver_type() -> String {
    cpal::default_host().id().name().to_string()
}

#[cfg(not(feature = "native"))]
fn default_driver_type() -> String {
    "Web Audio".to_string()
}

fn default_no_device() -> String {
    "No Device".to_string()
}

#[cfg(feature = "native")]
fn default_output_device() -> String {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(default_no_device)
}

#[cfg(not(feature = "native"))]
fn default_output_device() -> String {
    default_no_device()
}

#[cfg(feature = "native")]
fn default_input_device() -> String {
    cpal::default_host()
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(default_no_device)
}

#[cfg(not(feature = "native"))]
fn default_input_device() -> String {
    default_no_device()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            grid_line_intensity: 0.26,
            brightness: 1.0,
            color_intensity: 0.0,
            audio_driver_type: default_driver_type(),
            audio_input_device: default_input_device(),
            audio_output_device: default_output_device(),
            grid_enabled: true,
            snap_to_grid: true,
            grid_mode: GridMode::default(),
            triplet_grid: false,
            dev_mode: false,
            sample_library_folders: Vec::new(),
        }
    }
}

#[cfg(feature = "native")]
fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".layers")
        .join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        #[cfg(feature = "native")]
        {
            let path = settings_path();
            match std::fs::read_to_string(&path) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        }
        #[cfg(not(feature = "native"))]
        {
            Self::default()
        }
    }

    pub fn save(&self) {
        #[cfg(feature = "native")]
        {
            let path = settings_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(&path, json);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Audio device enumeration (native only)
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
pub fn available_driver_types() -> Vec<String> {
    cpal::available_hosts()
        .into_iter()
        .map(|id| id.name().to_string())
        .collect()
}

#[cfg(feature = "native")]
pub fn available_input_devices() -> Vec<String> {
    let mut names = vec!["No Device".to_string()];
    let host = cpal::default_host();
    // Ensure the default input device is always present
    if let Some(default) = host.default_input_device() {
        if let Ok(name) = default.name() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    // Fallback: try all devices and check if they support input
    if let Ok(devices) = host.devices() {
        for d in devices {
            if d.default_input_config().is_ok() {
                if let Ok(name) = d.name() {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }
    names
}

#[cfg(feature = "native")]
pub fn available_output_devices() -> Vec<String> {
    let mut names = vec!["No Device".to_string()];
    let host = cpal::default_host();
    // Ensure the default output device is always present
    if let Some(default) = host.default_output_device() {
        if let Ok(name) = default.name() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    // Fallback: try all devices and check if they support output
    if let Ok(devices) = host.devices() {
        for d in devices {
            if d.default_output_config().is_ok() {
                if let Ok(name) = d.name() {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }
    names
}
