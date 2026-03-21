#[cfg(feature = "native")]
use surrealdb::types::SurrealValue;

// ---------------------------------------------------------------------------
// Macro to conditionally derive SurrealValue
// ---------------------------------------------------------------------------

macro_rules! surreal_derive {
    (
        $(#[$outer:meta])*
        pub struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                pub $field:ident : $ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$outer])*
        #[cfg_attr(feature = "native", derive(SurrealValue))]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: $ty,
            )*
        }
    };
}

// ---------------------------------------------------------------------------
// Per-project schema
// ---------------------------------------------------------------------------

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredCanvasObject {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub color: [f32; 4],
        pub border_radius: f32,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredWaveform {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub color: [f32; 4],
        pub border_radius: f32,
        pub filename: String,
        pub fade_in_px: f32,
        pub fade_out_px: f32,
        pub fade_in_curve: f32,
        pub fade_out_curve: f32,
        pub sample_rate: u32,
        pub volume: f32,
        pub pan: f32,
        pub disabled: bool,
        pub sample_offset_px: f32,
        pub automation_volume: Vec<[f32; 2]>,
        pub automation_pan: Vec<[f32; 2]>,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredEffectRegion {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub plugin_ids: Vec<String>,
        pub plugin_names: Vec<String>,
        pub name: String,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredPluginBlock {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub color: [f32; 4],
        pub plugin_id: String,
        pub plugin_name: String,
        pub bypass: bool,
        pub state: Vec<u8>,
        pub params: Vec<u8>,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredLoopRegion {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub enabled: bool,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredComponent {
        pub id: String,
        pub name: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub waveform_ids: Vec<String>,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredComponentInstance {
        pub id: String,
        pub component_id: String,
        pub position: [f32; 2],
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredMidiNote {
        pub pitch: u32,
        pub start_px: f32,
        pub duration_px: f32,
        pub velocity: u32,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredMidiClip {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub color: [f32; 4],
        pub notes: Vec<StoredMidiNote>,
        pub pitch_low: u32,
        pub pitch_high: u32,
        pub grid_mode_tag: String,
        pub grid_mode_value: String,
        pub triplet_grid: bool,
        pub instrument_region_id: String,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredInstrumentRegion {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub name: String,
        pub plugin_id: String,
        pub plugin_name: String,
        pub state: Vec<u8>,
        pub params: Vec<u8>,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredTextNote {
        pub id: String,
        pub position: [f32; 2],
        pub size: [f32; 2],
        pub color: [f32; 4],
        pub border_radius: f32,
        pub text: String,
        pub font_size: f32,
        pub text_color: [f32; 4],
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredLayerNode {
        pub entity_id: String,
        pub kind_tag: String,
        pub parent_entity_id: String,
        pub expanded: bool,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct ProjectState {
        pub version: u32,
        pub name: String,
        pub camera_position: [f32; 2],
        pub camera_zoom: f32,
        pub objects: Vec<StoredCanvasObject>,
        pub waveforms: Vec<StoredWaveform>,
        pub browser_folders: Vec<String>,
        pub browser_width: f32,
        pub browser_visible: bool,
        pub browser_expanded: Vec<String>,
        pub effect_regions: Vec<StoredEffectRegion>,
        pub plugin_blocks: Vec<StoredPluginBlock>,
        pub loop_regions: Vec<StoredLoopRegion>,
        pub components: Vec<StoredComponent>,
        pub component_instances: Vec<StoredComponentInstance>,
        pub bpm: f32,
        pub midi_clips: Vec<StoredMidiClip>,
        pub instrument_regions: Vec<StoredInstrumentRegion>,
        pub layer_tree: Vec<StoredLayerNode>,
        pub text_notes: Vec<StoredTextNote>,
    }
}

// ---------------------------------------------------------------------------
// Audio / peaks stored per waveform
// ---------------------------------------------------------------------------

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredAudioData {
        pub waveform_id: String,
        pub left_samples: Vec<u8>,
        pub right_samples: Vec<u8>,
        pub mono_samples: Vec<u8>,
        pub sample_rate: u32,
        pub duration_secs: f32,
    }
}

surreal_derive! {
    #[derive(Clone)]
    pub struct StoredPeaks {
        pub waveform_id: String,
        pub block_size: u64,
        pub left_peaks: Vec<u8>,
        pub right_peaks: Vec<u8>,
    }
}

// ---------------------------------------------------------------------------
// Project metadata (project.json in project folder)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProjectMeta {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Global index DB schema
// ---------------------------------------------------------------------------

surreal_derive! {
    #[derive(Clone, Debug)]
    pub struct ProjectIndexEntry {
        pub name: String,
        pub path: String,
        pub is_temp: bool,
        pub created_at: u64,
        pub updated_at: u64,
    }
}
