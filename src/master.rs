use crate::entity_id::EntityId;

fn default_volume() -> f32 { 1.0 }
fn default_pan() -> f32 { 0.5 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Master {
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_pan")]
    pub pan: f32,
    #[serde(default)]
    pub effect_chain_id: Option<EntityId>,
}

impl Default for Master {
    fn default() -> Self {
        Self { volume: 1.0, pan: 0.5, effect_chain_id: None }
    }
}
