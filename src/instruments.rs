use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::effects::PluginGuiHandle;
use crate::entity_id::EntityId;

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
    pub volume: f32,
    pub pan: f32,
    pub effect_chain_id: Option<EntityId>,
    pub disabled: bool,
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
            volume: 1.0,
            pan: 0.5,
            effect_chain_id: None,
            disabled: false,
        }
    }

    pub fn has_plugin(&self) -> bool {
        !self.plugin_id.is_empty()
    }
}

fn default_volume() -> f32 { 1.0 }
fn default_pan() -> f32 { 0.5 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InstrumentSnapshot {
    pub name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_pan")]
    pub pan: f32,
    #[serde(default)]
    pub effect_chain_id: Option<EntityId>,
    #[serde(default)]
    pub disabled: bool,
}
