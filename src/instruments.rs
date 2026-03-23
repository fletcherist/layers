use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::effects::PluginGuiHandle;

// ---------------------------------------------------------------------------
// Instrument — lightweight non-spatial plugin holder
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Instrument {
    pub name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub gui: Arc<Mutex<Option<PluginGuiHandle>>>,
    pub pending_state: Option<Vec<u8>>,
    pub pending_params: Option<Vec<f64>>,
}

impl Instrument {
    pub fn new() -> Self {
        Self {
            name: "instrument".to_string(),
            plugin_id: String::new(),
            plugin_name: String::new(),
            plugin_path: PathBuf::new(),
            gui: Arc::new(Mutex::new(None)),
            pending_state: None,
            pending_params: None,
        }
    }

    pub fn has_plugin(&self) -> bool {
        !self.plugin_id.is_empty()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InstrumentSnapshot {
    pub name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Legacy snapshot (kept for backward-compat deserialization of old undo stacks)
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
