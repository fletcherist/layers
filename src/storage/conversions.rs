use indexmap::IndexMap;

use crate::entity_id::{EntityId, new_id};
use crate::settings::{AdaptiveGridSize, FixedGrid, GridMode};
use crate::CanvasObject;

use super::models::*;

// ---------------------------------------------------------------------------
// Grid mode serialisation helpers
// ---------------------------------------------------------------------------

pub fn grid_mode_to_stored(mode: GridMode) -> (String, String) {
    match mode {
        GridMode::Fixed(fg) => ("fixed".to_string(), fg.label().to_string()),
        GridMode::Adaptive(ag) => ("adaptive".to_string(), ag.label().to_string()),
    }
}

pub fn grid_mode_from_stored(tag: &str, value: &str) -> GridMode {
    match tag {
        "fixed" => {
            let fg = match value {
                "8 Bars" => FixedGrid::Bars8,
                "4 Bars" => FixedGrid::Bars4,
                "2 Bars" => FixedGrid::Bars2,
                "1 Bar" => FixedGrid::Bar1,
                "1/2" => FixedGrid::Half,
                "1/4" => FixedGrid::Quarter,
                "1/8" => FixedGrid::Eighth,
                "1/16" => FixedGrid::Sixteenth,
                "1/32" => FixedGrid::ThirtySecond,
                _ => FixedGrid::Quarter,
            };
            GridMode::Fixed(fg)
        }
        "adaptive" => {
            let ag = match value {
                "Widest" => AdaptiveGridSize::Widest,
                "Wide" => AdaptiveGridSize::Wide,
                "Medium" => AdaptiveGridSize::Medium,
                "Narrow" => AdaptiveGridSize::Narrow,
                "Narrowest" => AdaptiveGridSize::Narrowest,
                _ => AdaptiveGridSize::Medium,
            };
            GridMode::Adaptive(ag)
        }
        _ => GridMode::default(),
    }
}

// ---------------------------------------------------------------------------
// EntityId <-> String helpers for SurrealDB serialisation
// ---------------------------------------------------------------------------

fn entity_id_to_string(id: EntityId) -> String {
    id.to_string()
}

fn entity_id_from_string(s: &str) -> EntityId {
    s.parse::<EntityId>().unwrap_or_else(|_| new_id())
}

// ---------------------------------------------------------------------------
// Conversion: IndexMap <-> Vec<Stored*> for save/load
// ---------------------------------------------------------------------------

/// Convert an IndexMap of CanvasObjects to stored format.
pub fn objects_to_stored(map: &IndexMap<EntityId, CanvasObject>) -> Vec<StoredCanvasObject> {
    map.iter()
        .map(|(id, obj)| StoredCanvasObject {
            id: entity_id_to_string(*id),
            position: obj.position,
            size: obj.size,
            color: obj.color,
            border_radius: obj.border_radius,
        })
        .collect()
}

/// Convert stored objects back to an IndexMap. Old projects without `id` get new UUIDs.
pub fn objects_from_stored(stored: Vec<StoredCanvasObject>) -> IndexMap<EntityId, CanvasObject> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            let obj = CanvasObject {
                position: s.position,
                size: s.size,
                color: s.color,
                border_radius: s.border_radius,
            };
            (id, obj)
        })
        .collect()
}

/// Extract the EntityId and stored data for a waveform vec.
/// Returns (EntityId, StoredWaveform) pairs for downstream processing.
pub fn waveforms_from_stored(stored: Vec<StoredWaveform>) -> Vec<(EntityId, StoredWaveform)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn effect_regions_from_stored(
    stored: Vec<StoredEffectRegion>,
) -> Vec<(EntityId, StoredEffectRegion)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn plugin_blocks_from_stored(
    stored: Vec<StoredPluginBlock>,
) -> Vec<(EntityId, StoredPluginBlock)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn loop_regions_from_stored(
    stored: Vec<StoredLoopRegion>,
) -> Vec<(EntityId, StoredLoopRegion)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn components_from_stored(
    stored: Vec<StoredComponent>,
) -> Vec<(EntityId, StoredComponent)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn component_instances_from_stored(
    stored: Vec<StoredComponentInstance>,
) -> Vec<(EntityId, StoredComponentInstance)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn midi_clips_from_stored(
    stored: Vec<StoredMidiClip>,
) -> Vec<(EntityId, StoredMidiClip)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn instrument_regions_from_stored(
    stored: Vec<StoredInstrumentRegion>,
) -> Vec<(EntityId, StoredInstrumentRegion)> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            (id, s)
        })
        .collect()
}

pub fn text_notes_to_stored(
    map: &IndexMap<EntityId, crate::text_note::TextNote>,
) -> Vec<StoredTextNote> {
    map.iter()
        .map(|(id, tn)| StoredTextNote {
            id: entity_id_to_string(*id),
            position: tn.position,
            size: tn.size,
            color: tn.color,
            border_radius: tn.border_radius,
            text: tn.text.clone(),
            font_size: tn.font_size,
            text_color: tn.text_color,
        })
        .collect()
}

pub fn text_notes_from_stored(
    stored: Vec<StoredTextNote>,
) -> IndexMap<EntityId, crate::text_note::TextNote> {
    stored
        .into_iter()
        .map(|s| {
            let id = if s.id.is_empty() {
                new_id()
            } else {
                entity_id_from_string(&s.id)
            };
            let tn = crate::text_note::TextNote {
                position: s.position,
                size: s.size,
                color: s.color,
                border_radius: s.border_radius,
                text: s.text,
                font_size: s.font_size,
                text_color: s.text_color,
            };
            (id, tn)
        })
        .collect()
}
