use std::path::PathBuf;

use indexmap::IndexMap;

use crate::automation::AutomationParam;
use crate::component;
use crate::entity_id::EntityId;
use crate::instruments::InstrumentSnapshot;
use crate::midi;
use crate::regions::{ExportRegion, LoopRegion};
use crate::text_note;
use crate::ui::waveform::{AudioClipData, WaveformView};

// ---------------------------------------------------------------------------
// Canvas objects
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanvasObject {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum HitTarget {
    Object(EntityId),
    Waveform(EntityId),
    LoopRegion(EntityId),
    ExportRegion(EntityId),
    ComponentDef(EntityId),
    ComponentInstance(EntityId),
    MidiClip(EntityId),
    TextNote(EntityId),
    Group(EntityId),
    Instrument(EntityId),
}

#[derive(Clone, Copy)]
pub(crate) enum PitchRangeEdge { Top, Bottom }

pub(crate) enum DragState {
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
        anchor_idx: usize,
        before_states: Vec<(HitTarget, EntityBeforeState)>,
        overlap_snapshots: IndexMap<EntityId, WaveformView>,
        overlap_temp_splits: Vec<EntityId>,
    },
    PendingBrowserDrag {
        path: PathBuf,
        filename: String,
        start_mouse: [f32; 2],
    },
    DraggingFromBrowser {
        path: PathBuf,
        filename: String,
    },
    DraggingPlugin {
        plugin_id: String,
        plugin_name: String,
        is_instrument: bool,
    },
    ResizingBrowser,
    ResizingExportRegion {
        region_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: ExportRegion,
    },
    DraggingFade {
        waveform_id: EntityId,
        is_fade_in: bool,
        before: WaveformView,
    },
    DraggingFadeCurve {
        waveform_id: EntityId,
        is_fade_in: bool,
        start_mouse_y: f32,
        start_curve: f32,
        before: WaveformView,
    },
    ResizingComponentDef {
        comp_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: component::ComponentDef,
    },
    ResizingLoopRegion {
        region_id: EntityId,
        is_left_edge: bool,
        initial_position_x: f32,
        initial_size_w: f32,
        before: LoopRegion,
    },
    ResizingWaveform {
        waveform_id: EntityId,
        is_left_edge: bool,
        initial_position_x: f32,
        initial_size_w: f32,
        initial_offset_px: f32,
        before: WaveformView,
        overlap_snapshots: IndexMap<EntityId, WaveformView>,
        overlap_temp_splits: Vec<EntityId>,
    },
    DraggingAutomationPoint {
        waveform_id: EntityId,
        param: AutomationParam,
        point_idx: usize,
        original_t: f32,
        original_value: f32,
        before: WaveformView,
    },
    ResizingMidiClip {
        clip_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: midi::MidiClip,
    },
    ResizingMidiPitchRange {
        clip_id: EntityId,
        edge: PitchRangeEdge,
        start_y: f32,
        before: midi::MidiClip,
    },
    ResizingMidiClipEdge {
        clip_id: EntityId,
        is_left: bool,
        before: midi::MidiClip,
        overlap_snapshots: IndexMap<EntityId, midi::MidiClip>,
        overlap_temp_splits: Vec<EntityId>,
    },
    MovingMidiClip {
        clip_id: EntityId,
        offset: [f32; 2],
        before: midi::MidiClip,
        overlap_snapshots: IndexMap<EntityId, midi::MidiClip>,
        overlap_temp_splits: Vec<EntityId>,
    },
    MovingMidiNote {
        clip_id: EntityId,
        note_indices: Vec<usize>,
        offsets: Vec<[f32; 2]>,
        start_world: [f32; 2],
        before_notes: Vec<midi::MidiNote>,
    },
    ResizingMidiNote {
        clip_id: EntityId,
        anchor_idx: usize,
        note_indices: Vec<usize>,
        original_durations: Vec<f32>,
        before_notes: Vec<midi::MidiNote>,
    },
    ResizingMidiNoteLeft {
        clip_id: EntityId,
        anchor_idx: usize,
        note_indices: Vec<usize>,
        original_starts: Vec<f32>,
        original_durations: Vec<f32>,
        before_notes: Vec<midi::MidiNote>,
    },
    SelectingMidiNotes {
        clip_id: EntityId,
        start_world: [f32; 2],
    },
    DraggingVelocity {
        clip_id: EntityId,
        note_indices: Vec<usize>,
        original_velocities: Vec<u8>,
        start_world_y: f32,
        before_notes: Vec<midi::MidiNote>,
    },
    ResizingVelocityLane {
        clip_id: EntityId,
        start_world_y: f32,
        original_height: f32,
    },
    ResizingTextNote {
        note_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: text_note::TextNote,
    },
    DraggingEffectSlot {
        chain_id: EntityId,
        slot_idx: usize,
        start_y: f32,
    },
    ReorderingLayerNode {
        entity_id: EntityId,
        kind: crate::layers::LayerNodeKind,
        start_y: f32,
        start_flat_index: usize,
        drag_active: bool,
        drop_target: Option<crate::layers::DropTarget>,
        source_group_before: Option<(EntityId, crate::group::Group)>,
        #[cfg(not(target_arch = "wasm32"))]
        hover_expand_target: Option<(EntityId, std::time::Instant)>,
        #[cfg(target_arch = "wasm32")]
        hover_expand_target: Option<(EntityId, web_time::Instant)>,
    },
}

/// Captures before-state of an entity for drag operations.
#[derive(Clone)]
pub(crate) enum EntityBeforeState {
    Object(CanvasObject),
    Waveform(WaveformView),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    ComponentDef(component::ComponentDef),
    ComponentInstance(component::ComponentInstance),
    MidiClip(midi::MidiClip),
    TextNote(text_note::TextNote),
    Group(crate::group::Group),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ComponentDefHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TextNoteHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum GroupHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

/// Snapshot of a single group member entity, used when copying groups to clipboard.
/// Uses InstrumentSnapshot (not Instrument) to avoid sharing Arc<Mutex<...>> state.
#[derive(Clone)]
pub(crate) enum GroupMemberSnapshot {
    Object(CanvasObject),
    Waveform(WaveformView, Option<AudioClipData>),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    MidiClip(midi::MidiClip),
    TextNote(text_note::TextNote),
    ComponentInstance(component::ComponentInstance),
    Instrument(InstrumentSnapshot),
}

#[derive(Clone)]
pub(crate) enum ClipboardItem {
    Object(CanvasObject),
    Waveform(WaveformView, Option<AudioClipData>),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    ComponentDef(
        component::ComponentDef,
        Vec<(WaveformView, Option<AudioClipData>)>,
    ),
    ComponentInstance(component::ComponentInstance),
    MidiClip(midi::MidiClip),
    MidiNotes(Vec<midi::MidiNote>),
    TextNote(text_note::TextNote),
    Group(crate::group::Group, Vec<(EntityId, GroupMemberSnapshot)>),
}

pub(crate) struct Clipboard {
    pub(crate) items: Vec<ClipboardItem>,
}

impl Clipboard {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }
}
