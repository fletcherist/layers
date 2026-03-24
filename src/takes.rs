use crate::entity_id::EntityId;

/// A group of alternative recordings (takes) for an audio clip.
/// Stored on the parent WaveformView. Child takes are separate WaveformViews
/// referenced by ID. The parent itself is take 0; children start at index 1.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TakeGroup {
    /// Child take WaveformView IDs, ordered by creation.
    /// Does NOT include the parent (which is implicitly take 0).
    pub take_ids: Vec<EntityId>,
    /// Active take index: 0 = parent, 1..N = children (index into take_ids is active_index - 1).
    pub active_index: usize,
    /// Whether take lanes are visually expanded below the parent.
    pub expanded: bool,
}

impl TakeGroup {
    /// Total number of takes including the parent.
    pub fn take_count(&self) -> usize {
        1 + self.take_ids.len()
    }

    /// Whether the given waveform ID is a child take in this group.
    pub fn contains(&self, wf_id: EntityId) -> bool {
        self.take_ids.contains(&wf_id)
    }
}
