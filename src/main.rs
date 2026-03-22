#[cfg(feature = "native")]
mod audio;
mod automation;
mod component;
mod effects;
mod entity_id;
mod events;
mod gpu;
mod grid;
mod history;
mod instruments;
mod layers;
mod midi;
mod midi_keyboard;
mod network;
mod operations;
#[cfg(feature = "native")]
mod surreal_client;
#[cfg(feature = "native")]
mod plugins;
mod regions;
mod settings;
pub mod theme;
mod storage;
mod ui;
mod user;

#[cfg(test)]
mod tests;

// Time compatibility: use web-time on WASM, std::time on native
#[cfg(target_arch = "wasm32")]
use web_time::Instant as TimeInstant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant as TimeInstant;

pub(crate) use gpu::{push_border, Camera, Gpu, InstanceRaw};
pub(crate) use ui::transport::{TransportPanel, TRANSPORT_WIDTH};

use grid::{grid_spacing_for_settings, snap_to_clip_grid, snap_to_grid, snap_to_vertical_grid, DEFAULT_BPM};
use ui::hit_testing::{
    canonical_rect, compute_resize, full_audio_width_px, hit_test, hit_test_corner_resize,
    hit_test_fade_curve_dot, hit_test_fade_handle, hit_test_waveform_edge,
    hit_test_automation_point, hit_test_automation_line,
    point_in_rect, rects_overlap, targets_in_rect, WaveformEdgeHover, WAVEFORM_MIN_WIDTH_PX,
};
use regions::{
    ExportHover, ExportRegion, LoopHover, LoopRegion, SelectArea,
    EXPORT_REGION_DEFAULT_HEIGHT, EXPORT_REGION_DEFAULT_WIDTH,
    EXPORT_RENDER_PILL_H, EXPORT_RENDER_PILL_W,
    LOOP_REGION_DEFAULT_HEIGHT, LOOP_REGION_DEFAULT_WIDTH,
};
use ui::rendering::{build_instances, build_waveform_vertices, default_objects, RenderContext};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use indexmap::IndexMap;
use entity_id::{EntityId, new_id};

#[cfg(feature = "native")]
use audio::{load_audio_file, AudioEngine, AudioRecorder};
use grid::PIXELS_PER_SECOND;
use ui::waveform::AudioClipData;
use settings::GridMode;
use ui::context_menu::{ContextMenu, MenuContext};
use ui::palette::{
    CommandAction, CommandPalette, PaletteMode, PaletteRow, PluginPickerEntry, COMMANDS,
    PALETTE_ITEM_HEIGHT,
};
pub(crate) use ui::waveform::WaveformView;
use ui::waveform::{AudioData, WaveformPeaks, WaveformVertex};

#[cfg(feature = "native")]
use muda::{MenuId, Submenu as MudaSubmenu};
use settings::Settings;
#[cfg(feature = "native")]
use ui::settings_window::{SettingsWindow, CATEGORIES};
#[cfg(feature = "native")]
use storage::{default_base_path, ProjectState, Storage};
use winit::{
    event_loop::EventLoop,
    keyboard::{KeyCode, ModifiersState},
    window::CursorIcon,
};

// ---------------------------------------------------------------------------
// Platform-conditional type aliases for App struct fields
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
type NativeAudioEngine = audio::AudioEngine;
#[cfg(not(feature = "native"))]
type NativeAudioEngine = ();

#[cfg(feature = "native")]
type NativeAudioRecorder = audio::AudioRecorder;
#[cfg(not(feature = "native"))]
type NativeAudioRecorder = ();

#[cfg(feature = "native")]
type NativeStorage = storage::Storage;
#[cfg(not(feature = "native"))]
type NativeStorage = ();

#[cfg(feature = "native")]
type NativeSettingsWindow = ui::settings_window::SettingsWindow;
#[cfg(not(feature = "native"))]
type NativeSettingsWindow = ();

#[cfg(feature = "native")]
type NativeMenuState = MenuState;
#[cfg(not(feature = "native"))]
type NativeMenuState = ();

#[cfg(feature = "native")]
type NativePendingRemoteAudioFetch = PendingRemoteAudioFetch;
#[cfg(not(feature = "native"))]
type NativePendingRemoteAudioFetch = ();

#[cfg(feature = "native")]
type NativeRemoteStorage = storage::RemoteStorage;
#[cfg(not(feature = "native"))]
type NativeRemoteStorage = ();

#[cfg(feature = "native")]
type NativeTokioRuntime = tokio::runtime::Runtime;
#[cfg(not(feature = "native"))]
type NativeTokioRuntime = ();

#[cfg(feature = "native")]
type NativeWelcomeReceiver = tokio::sync::oneshot::Receiver<user::User>;
#[cfg(not(feature = "native"))]
type NativeWelcomeReceiver = ();

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
    EffectRegion(EntityId),
    PluginBlock(EntityId),
    LoopRegion(EntityId),
    ExportRegion(EntityId),
    ComponentDef(EntityId),
    ComponentInstance(EntityId),
    MidiClip(EntityId),
    InstrumentRegion(EntityId),
}

use automation::{AutomationData, AutomationParam};



enum DragState {
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
    ResizingEffectRegion {
        region_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: effects::EffectRegion,
    },
    ResizingLoopRegion {
        region_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
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
    ResizingInstrumentRegion {
        region_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: instruments::InstrumentRegionSnapshot,
    },
    ResizingMidiClip {
        clip_id: EntityId,
        anchor: [f32; 2],
        nwse: bool,
        before: midi::MidiClip,
    },
    MovingMidiClip {
        clip_id: EntityId,
        offset: [f32; 2],
        before: midi::MidiClip,
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
}

/// Captures before-state of an entity for drag operations.
#[derive(Clone)]
pub(crate) enum EntityBeforeState {
    Object(CanvasObject),
    Waveform(WaveformView),
    EffectRegion(effects::EffectRegion),
    PluginBlock(effects::PluginBlockSnapshot),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    ComponentDef(component::ComponentDef),
    ComponentInstance(component::ComponentInstance),
    MidiClip(midi::MidiClip),
    InstrumentRegion(instruments::InstrumentRegionSnapshot),
}

#[derive(Clone, Copy, PartialEq)]
enum ComponentDefHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

#[derive(Clone, Copy, PartialEq)]
enum EffectRegionHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

#[derive(Clone, Copy, PartialEq)]
enum InstrumentRegionHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}


#[derive(Clone)]
enum ClipboardItem {
    Object(CanvasObject),
    Waveform(WaveformView, Option<AudioClipData>),
    EffectRegion(effects::EffectRegion),
    PluginBlock(effects::PluginBlock),
    LoopRegion(LoopRegion),
    ExportRegion(ExportRegion),
    ComponentDef(
        component::ComponentDef,
        Vec<(WaveformView, Option<AudioClipData>)>,
    ),
    ComponentInstance(component::ComponentInstance),
    MidiClip(midi::MidiClip),
    MidiNotes(Vec<midi::MidiNote>),
    InstrumentRegion(instruments::InstrumentRegionSnapshot),
}

struct Clipboard {
    items: Vec<ClipboardItem>,
}

impl Clipboard {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(crate) use crate::theme::WAVEFORM_COLORS;

// Audio formats supported via symphonia: wav, mp3, ogg, flac, aac
const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aac", "m4a", "mp4"];

pub(crate) const MIDI_AUTO_EDIT_ZOOM_THRESHOLD: f32 = 2.0;

pub(crate) fn format_playback_time(secs: f64) -> String {
    let minutes = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{}:{:04.1}", minutes, s)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------














// (rendering functions moved to src/rendering.rs)

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
struct MenuState {
    menu: muda::Menu,
    new_project: MenuId,
    save_project: MenuId,
    open_project: MenuId,
    settings: MenuId,
    undo: MenuId,
    redo: MenuId,
    copy: MenuId,
    paste: MenuId,
    select_all: MenuId,
    open_project_items: Vec<(MenuId, String)>,
    open_submenu: MudaSubmenu,
    initialized: bool,
}

/// Result of fetching audio from remote storage on a background thread.
#[cfg(feature = "native")]
struct PendingRemoteAudioFetch {
    wf_id: EntityId,
    audio: Arc<AudioData>,
    ac: AudioClipData,
}

/// Result of decoding an audio file on a background thread.
/// The wf_id refers to a placeholder waveform already visible on canvas.
enum PendingAudioLoad {
    /// Audio decoded — update local display immediately.
    Decoded {
        wf_id: EntityId,
        wf_data: WaveformView,
        ac_data: AudioClipData,
    },
    /// Remote storage save finished — safe to push op to network now.
    /// Carries the decoded audio data so it can be applied at this point.
    SyncReady {
        wf_id: EntityId,
        wf_data: WaveformView,
        ac_data: AudioClipData,
    },
    /// Load failed — remove placeholder.
    Failed { wf_id: EntityId },
}

struct App {
    gpu: Option<Gpu>,
    /// Shared slot for async GPU init on WASM — `spawn_local` writes here, `about_to_wait` reads.
    pending_gpu: Arc<std::sync::Mutex<Option<Gpu>>>,
    /// Window ref kept separately so we can request_redraw before GPU is ready.
    window: Option<Arc<winit::window::Window>>,
    camera: Camera,
    objects: IndexMap<EntityId, CanvasObject>,
    waveforms: IndexMap<EntityId, WaveformView>,
    audio_clips: IndexMap<EntityId, AudioClipData>,
    audio_engine: Option<NativeAudioEngine>,
    recorder: Option<NativeAudioRecorder>,
    recording_waveform_id: Option<EntityId>,
    last_canvas_click_world: [f32; 2],
    selected: Vec<HitTarget>,
    drag: DragState,
    mouse_pos: [f32; 2],
    hovered: Option<HitTarget>,
    fade_handle_hovered: Option<(EntityId, bool)>,
    fade_curve_hovered: Option<(EntityId, bool)>,
    waveform_edge_hover: WaveformEdgeHover,
    midi_note_edge_hover: bool,
    velocity_bar_hovered: bool,
    velocity_divider_hovered: bool,
    file_hovering: bool,
    modifiers: ModifiersState,
    command_palette: Option<CommandPalette>,
    context_menu: Option<ContextMenu>,
    browser_context_path: Option<std::path::PathBuf>,
    sample_browser: ui::browser::SampleBrowser,
    layer_tree: Vec<layers::LayerNode>,
    storage: Option<NativeStorage>,
    has_saved_state: bool,
    project_dirty: bool,
    op_undo_stack: Vec<operations::CommittedOp>,
    op_redo_stack: Vec<operations::CommittedOp>,
    arrow_nudge_before: Option<Vec<(HitTarget, EntityBeforeState)>>,
    arrow_nudge_last: Option<TimeInstant>,
    arrow_nudge_overlap_snapshots: IndexMap<EntityId, WaveformView>,
    arrow_nudge_overlap_temp_splits: Vec<EntityId>,
    current_project_name: String,
    effect_regions: IndexMap<EntityId, effects::EffectRegion>,
    plugin_blocks: IndexMap<EntityId, effects::PluginBlock>,
    components: IndexMap<EntityId, component::ComponentDef>,
    component_instances: IndexMap<EntityId, component::ComponentInstance>,
    next_component_id: component::ComponentId,
    plugin_registry: effects::PluginRegistry,
    export_regions: IndexMap<EntityId, ExportRegion>,
    export_hover: ExportHover,
    loop_regions: IndexMap<EntityId, LoopRegion>,
    loop_hover: LoopHover,
    select_area: Option<SelectArea>,
    component_def_hover: ComponentDefHover,
    effect_region_hover: EffectRegionHover,
    instrument_region_hover: InstrumentRegionHover,
    midi_clips: IndexMap<EntityId, midi::MidiClip>,
    instrument_regions: IndexMap<EntityId, instruments::InstrumentRegion>,
    editing_midi_clip: Option<EntityId>,
    selected_midi_notes: Vec<usize>,
    pending_midi_note_click: Option<usize>,
    midi_note_select_rect: Option<[f32; 4]>,
    cmd_velocity_hover_note: Option<(EntityId, usize)>,
    editing_component: Option<EntityId>,
    editing_effect_name: Option<(EntityId, String)>,
    editing_waveform_name: Option<(EntityId, String)>,
    bpm: f32,
    editing_bpm: ui::value_entry::ValueEntry,
    dragging_bpm: Option<(f32, f32)>,
    bpm_drag_overlap_snapshots: IndexMap<EntityId, WaveformView>,
    bpm_drag_overlap_temp_splits: Vec<EntityId>,
    last_click_time: TimeInstant,
    last_vol_text_click_time: TimeInstant,
    last_vol_knob_click_time: TimeInstant,
    last_pan_knob_click_time: TimeInstant,
    last_sample_bpm_text_click_time: TimeInstant,
    last_pitch_text_click_time: TimeInstant,
    last_click_world: [f32; 2],
    last_cursor_send: TimeInstant,
    clipboard: Clipboard,
    settings: Settings,
    settings_window: Option<NativeSettingsWindow>,
    plugin_editor: Option<ui::plugin_editor::PluginEditorWindow>,
    menu_state: Option<NativeMenuState>,
    toast_manager: ui::toast::ToastManager,
    tooltip: ui::tooltip::TooltipState,
    automation_mode: bool,
    active_automation_param: AutomationParam,
    right_window: Option<ui::right_window::RightWindow>,
    // Background audio loading
    pending_audio_tx: mpsc::Sender<PendingAudioLoad>,
    pending_audio_rx: mpsc::Receiver<PendingAudioLoad>,
    pending_remote_audio_tx: mpsc::Sender<NativePendingRemoteAudioFetch>,
    pending_remote_audio_rx: mpsc::Receiver<NativePendingRemoteAudioFetch>,
    pending_audio_loads_count: usize,
    // Collaboration
    remote_storage: Option<Arc<NativeRemoteStorage>>,
    local_user: user::User,
    remote_users: std::collections::HashMap<user::UserId, user::RemoteUserState>,
    applied_remote_seqs: std::collections::HashSet<(user::UserId, u64)>,
    network: network::NetworkManager,
    ws_runtime: Option<NativeTokioRuntime>,
    connect_url: Option<String>,
    connect_project_id: Option<String>,
    pending_welcome: Option<NativeWelcomeReceiver>,
    reconnect_attempt: u32,
    last_reconnect_time: Option<TimeInstant>,
    cached_instances: Vec<InstanceRaw>,
    cached_wf_verts: Vec<WaveformVertex>,
    render_generation: u64,
    last_rendered_generation: u64,
    last_rendered_camera_pos: [f32; 2],
    last_rendered_camera_zoom: f32,
    last_rendered_hovered: Option<HitTarget>,
    last_rendered_selected_len: usize,
    /// Computer keyboard → instrument preview (native audio only).
    pub(crate) computer_keyboard_armed: bool,
    pub(crate) computer_keyboard_octave_offset: i8,
    pub(crate) computer_keyboard_velocity: u8,
    pub(crate) keyboard_instrument_id: Option<EntityId>,
    pub(crate) midi_keyboard_held: HashMap<KeyCode, (EntityId, u8)>,
}

impl App {
    /// Minimal constructor for headless/web use — no storage, audio, or native GUI.
    fn new_minimal(project_name: &str) -> Self {
        let (pending_audio_tx, pending_audio_rx) = mpsc::channel();
        let (pending_remote_audio_tx, pending_remote_audio_rx) = mpsc::channel();
        Self {
            gpu: None,
            pending_gpu: Arc::new(std::sync::Mutex::new(None)),
            window: None,
            camera: Camera::new(),
            objects: IndexMap::new(),
            waveforms: IndexMap::new(),
            audio_clips: IndexMap::new(),
            audio_engine: None,
            recorder: None,
            recording_waveform_id: None,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            fade_handle_hovered: None,
            fade_curve_hovered: None,
            waveform_edge_hover: WaveformEdgeHover::None,
            midi_note_edge_hover: false,
            velocity_bar_hovered: false,
            velocity_divider_hovered: false,
            file_hovering: false,
            modifiers: ModifiersState::empty(),
            command_palette: None,
            context_menu: None,
            browser_context_path: None,
            sample_browser: ui::browser::SampleBrowser::new(),
            layer_tree: Vec::new(),
            storage: None,
            has_saved_state: false,
            project_dirty: false,
            op_undo_stack: Vec::new(),
            op_redo_stack: Vec::new(),
            arrow_nudge_before: None,
            arrow_nudge_last: None,
            arrow_nudge_overlap_snapshots: IndexMap::new(),
            arrow_nudge_overlap_temp_splits: Vec::new(),
            current_project_name: project_name.into(),
            effect_regions: IndexMap::new(),
            plugin_blocks: IndexMap::new(),
            components: IndexMap::new(),
            component_instances: IndexMap::new(),
            next_component_id: new_id(),
            plugin_registry: effects::PluginRegistry::new(),
            export_regions: IndexMap::new(),
            export_hover: ExportHover::None,
            loop_regions: IndexMap::new(),
            loop_hover: LoopHover::None,
            select_area: None,
            component_def_hover: ComponentDefHover::None,
            effect_region_hover: EffectRegionHover::None,
            instrument_region_hover: InstrumentRegionHover::None,
            midi_clips: IndexMap::new(),
            instrument_regions: IndexMap::new(),
            editing_midi_clip: None,
            selected_midi_notes: Vec::new(),
            pending_midi_note_click: None,
            midi_note_select_rect: None,
            cmd_velocity_hover_note: None,
            editing_component: None,
            editing_effect_name: None,
            editing_waveform_name: None,
            bpm: 120.0,
            editing_bpm: ui::value_entry::ValueEntry::new(),
            dragging_bpm: None,
            bpm_drag_overlap_snapshots: IndexMap::new(),
            bpm_drag_overlap_temp_splits: Vec::new(),
            last_click_time: TimeInstant::now(),
            last_vol_text_click_time: TimeInstant::now(),
            last_vol_knob_click_time: TimeInstant::now(),
            last_pan_knob_click_time: TimeInstant::now(),
            last_sample_bpm_text_click_time: TimeInstant::now(),
            last_pitch_text_click_time: TimeInstant::now(),
            last_click_world: [0.0; 2],
            last_cursor_send: TimeInstant::now(),
            clipboard: Clipboard::new(),
            settings: Settings::default(),
            settings_window: None,
            plugin_editor: None,
            menu_state: None,
            toast_manager: ui::toast::ToastManager::new(),
            tooltip: ui::tooltip::TooltipState::new(),
            automation_mode: false,
            active_automation_param: AutomationParam::Volume,
            right_window: None,
            pending_audio_tx,
            pending_audio_rx,
            pending_remote_audio_tx,
            pending_remote_audio_rx,
            pending_audio_loads_count: 0,
            remote_storage: None,
            local_user: user::User {
                id: entity_id::new_id(),
                name: "Local".to_string(),
                color: user::USER_COLORS[0],
            },
            remote_users: std::collections::HashMap::new(),
            applied_remote_seqs: std::collections::HashSet::new(),
            network: network::NetworkManager::new_offline(),
            ws_runtime: None,
            connect_url: None,
            connect_project_id: None,
            pending_welcome: None,
            reconnect_attempt: 0,
            last_reconnect_time: None,
            cached_instances: Vec::new(),
            cached_wf_verts: Vec::new(),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
            computer_keyboard_armed: false,
            computer_keyboard_octave_offset: 0,
            computer_keyboard_velocity: midi_keyboard::DEFAULT_VELOCITY,
            keyboard_instrument_id: None,
            midi_keyboard_held: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_headless() -> Self {
        Self::new_minimal("test")
    }

    /// Constructor for the web build.
    #[cfg(not(feature = "native"))]
    pub fn new_web() -> Self {
        Self::new_minimal("Untitled")
    }

    fn mark_dirty(&mut self) {
        self.render_generation = self.render_generation.wrapping_add(1);
        self.project_dirty = true;
        if self.sample_browser.visible {
            self.refresh_project_browser_entries();
        }
    }

    /// Sync the layer tree with current entities and refresh the Layers browser tab.
    pub(crate) fn refresh_project_browser_entries(&mut self) {
        layers::sync_tree(
            &mut self.layer_tree,
            &self.instrument_regions,
            &self.midi_clips,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
        );
        let rows = layers::flatten_tree(
            &self.layer_tree,
            &self.instrument_regions,
            &self.midi_clips,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
        );
        self.sample_browser.layer_rows = rows;
        if self.sample_browser.active_category == ui::browser::BrowserCategory::Layers {
            self.sample_browser.rebuild_entries();
        }
    }

    /// Center the camera on an instrument region and select it for computer-keyboard preview.
    pub(crate) fn focus_instrument_region(&mut self, id: EntityId) {
        let Some(ir) = self.instrument_regions.get(&id) else {
            return;
        };
        let (sw, sh, _) = self.screen_info();
        let cx = ir.position[0] + ir.size[0] * 0.5;
        let cy = ir.position[1] + ir.size[1] * 0.5;
        self.camera.position = [
            cx - sw * 0.5 / self.camera.zoom,
            cy - sh * 0.5 / self.camera.zoom,
        ];
        self.selected.clear();
        self.selected.push(HitTarget::InstrumentRegion(id));
        self.keyboard_instrument_id = Some(id);
        self.update_right_window();
        #[cfg(feature = "native")]
        self.sync_computer_keyboard_to_engine();
        self.mark_dirty();
    }

    #[cfg(feature = "native")]
    pub(crate) fn release_computer_keyboard_notes(&mut self) {
        if let Some(engine) = &self.audio_engine {
            for (_, (target, note)) in self.midi_keyboard_held.drain() {
                engine.keyboard_preview_note_off(target, note);
            }
        } else {
            self.midi_keyboard_held.clear();
        }
    }

    #[cfg(not(feature = "native"))]
    pub(crate) fn release_computer_keyboard_notes(&mut self) {
        self.midi_keyboard_held.clear();
    }

    /// Returns the standard clip height for the current BPM.
    pub(crate) fn clip_height(&self) -> f32 {
        grid::clip_height(self.bpm)
    }

    /// Rescale all time-based positions and widths by `scale` so that every
    /// clip/region stays locked to the same bar/beat grid after a BPM change.
    /// Call this before updating `self.bpm` so that `scale = old_bpm / new_bpm`.
    /// Both axes are scaled: X for horizontal beat alignment, Y for vertical
    /// grid alignment. Waveform height is also scaled so clips always span
    /// the same number of grid beats.
    pub(crate) fn rescale_clip_positions(&mut self, scale: f32) {
        for wf in self.waveforms.values_mut() {
            wf.position[0] *= scale;
            wf.position[1] *= scale;
            wf.size[1] *= scale;
            // size[0] intentionally NOT scaled: audio duration is fixed in seconds.
        }
        for mc in self.midi_clips.values_mut() {
            mc.position[0] *= scale;
            mc.position[1] *= scale;
            mc.size[0] *= scale;
            for note in &mut mc.notes {
                note.start_px *= scale;
                note.duration_px *= scale;
            }
        }
        for lr in self.loop_regions.values_mut() {
            lr.position[0] *= scale;
            lr.position[1] *= scale;
            lr.size[0] *= scale;
        }
        for er in self.export_regions.values_mut() {
            er.position[0] *= scale;
            er.position[1] *= scale;
            er.size[0] *= scale;
        }
        for ir in self.instrument_regions.values_mut() {
            ir.position[0] *= scale;
            ir.position[1] *= scale;
            ir.size[0] *= scale;
        }
        for efr in self.effect_regions.values_mut() {
            efr.position[0] *= scale;
            efr.position[1] *= scale;
            efr.size[0] *= scale;
        }
        // Keep overlap snapshots in sync so live restore uses the correct scale.
        for snap in self.bpm_drag_overlap_snapshots.values_mut() {
            snap.position[0] *= scale;
            snap.position[1] *= scale;
            snap.size[1] *= scale;
        }
    }

    /// Resize all warped waveforms (RePitch and Semitone) based on current project BPM / pitch.
    pub(crate) fn resize_warped_clips(&mut self) {
        for (&wf_id, wf) in self.waveforms.iter_mut() {
            match wf.warp_mode {
                ui::waveform::WarpMode::RePitch => {
                    if let Some(clip) = self.audio_clips.get(&wf_id) {
                        let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                        wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                    }
                }
                ui::waveform::WarpMode::Semitone => {
                    if let Some(clip) = self.audio_clips.get(&wf_id) {
                        let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                        wf.size[0] = original_duration_px / 2.0_f32.powf(wf.pitch_semitones / 12.0);
                    }
                }
                ui::waveform::WarpMode::Off => {}
            }
        }
    }

    /// Adjust camera position so the screen center stays anchored to the same
    /// world content after all object positions have been scaled by `scale`.
    pub(crate) fn rescale_camera_for_bpm(&mut self, scale: f32) {
        let (sw, sh, _) = self.screen_info();
        let cx = sw / 2.0;
        let cy = sh / 2.0;
        let world_center = self.camera.screen_to_world([cx, cy]);
        self.camera.position[0] = world_center[0] * scale - cx / self.camera.zoom;
        self.camera.position[1] = world_center[1] * scale - cy / self.camera.zoom;
    }

    /// Tear down plugin GUIs and instances in the correct order before exit.
    /// GUIs must be destroyed before plugin instances they reference.
    #[cfg(feature = "native")]
    fn shutdown_plugins(&mut self) {
        // Stop audio engine first so the audio thread releases plugin locks
        self.audio_engine = None;

        // Destroy instrument region GUIs (single instance handles both GUI + audio)
        for ir in self.instrument_regions.values_mut() {
            if let Ok(mut g) = ir.gui.lock() {
                *g = None;
            }
        }

        // Destroy plugin block GUIs
        for pb in self.plugin_blocks.values_mut() {
            if let Ok(mut g) = pb.gui.lock() {
                *g = None;
            }
        }
    }

    #[cfg(feature = "native")]
    fn new(skip_load: bool) -> Self {
        let base_path = default_base_path();
        println!("  Storage: {}", base_path.display());

        let mut storage = Storage::open(&base_path);

        let mut opened_project = false;
        if let Some(s) = &mut storage {
            if skip_load {
                if s.create_temp_project().is_some() {
                    opened_project = true;
                }
                println!("  Starting with empty project (--empty)");
            } else {
                let projects = s.list_projects();
                if !projects.is_empty() {
                    println!("  Projects:");
                    for p in &projects {
                        println!("    - {} ({})", p.name, p.path);
                    }
                    let best = projects.iter().max_by_key(|p| p.updated_at).unwrap();
                    let path = PathBuf::from(&best.path);
                    if path.exists() && s.open_project(&path) {
                        opened_project = true;
                    }
                }
                if !opened_project {
                    if s.create_temp_project().is_some() {
                        opened_project = true;
                    }
                }
            }
        }

        let loaded = if opened_project && !skip_load {
            storage.as_ref().and_then(|s| s.load_project_state())
        } else {
            None
        };
        let has_saved_state = loaded.is_some();

        // Load audio + peaks from project DB if available
        let (
            camera,
            objects,
            waveforms,
            project_name,
            browser_folders,
            browser_width,
            browser_visible,
            browser_expanded,
            stored_effect_regions,
            stored_plugin_blocks,
            stored_loop_regions,
            stored_components,
            stored_component_instances,
            audio_clips,
            loaded_bpm,
            stored_midi_clips,
            stored_instrument_regions,
            stored_layer_tree,
        ) = match loaded {
            Some(state) => {
                println!(
                    "  Loaded project '{}' ({} objects, {} waveforms, {} effect regions)",
                    state.name,
                    state.objects.len(),
                    state.waveforms.len(),
                    state.effect_regions.len(),
                );
                let cam = Camera {
                    position: state.camera_position,
                    zoom: state.camera_zoom,
                };
                let name = storage
                    .as_ref()
                    .and_then(|s| s.current_project_path())
                    .and_then(|p| storage::Storage::read_project_json(p))
                    .map(|m| m.name)
                    .unwrap_or_else(|| state.name.clone());
                let folders: Vec<PathBuf> =
                    state.browser_folders.iter().map(PathBuf::from).collect();
                let bw = if state.browser_width >= 480.0 {
                    state.browser_width
                } else {
                    480.0
                };
                let expanded: HashSet<PathBuf> =
                    state.browser_expanded.iter().map(PathBuf::from).collect();

                let wf_pairs = storage::waveforms_from_stored(state.waveforms);
                let mut waveforms: IndexMap<EntityId, WaveformView> = wf_pairs
                    .into_iter()
                    .map(|(id, sw)| (id, WaveformView {
                        audio: Arc::new(AudioData {
                            left_samples: Arc::new(Vec::new()),
                            right_samples: Arc::new(Vec::new()),
                            left_peaks: Arc::new(WaveformPeaks::empty()),
                            right_peaks: Arc::new(WaveformPeaks::empty()),
                            sample_rate: sw.sample_rate,
                            filename: sw.filename.clone(),
                        }),
                        filename: sw.filename,
                        position: sw.position,
                        size: sw.size,
                        color: sw.color,
                        border_radius: sw.border_radius,
                        fade_in_px: sw.fade_in_px,
                        fade_out_px: sw.fade_out_px,
                        fade_in_curve: sw.fade_in_curve,
                        fade_out_curve: sw.fade_out_curve,
                        volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                        pan: sw.pan,
                        warp_mode: ui::waveform::WarpMode::Off,
                        sample_bpm: if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
                        pitch_semitones: 0.0,
                        disabled: sw.disabled,
                        sample_offset_px: sw.sample_offset_px,
                        automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
                    }))
                    .collect();

                // Restore audio data and peaks from DB
                let mut audio_clips: IndexMap<EntityId, AudioClipData> = IndexMap::new();
                if let Some(s) = &storage {
                    let wf_ids: Vec<EntityId> = waveforms.keys().cloned().collect();
                    for wf_id in &wf_ids {
                        let id_str = wf_id.to_string();
                        let wf = waveforms.get(wf_id).unwrap();
                        let mut left_samples = Arc::new(Vec::new());
                        let mut right_samples = Arc::new(Vec::new());
                        let mut sample_rate = wf.audio.sample_rate;
                        let mut left_peaks = wf.audio.left_peaks.clone();
                        let mut right_peaks = wf.audio.right_peaks.clone();

                        if let Some(audio) = s.load_audio(&id_str) {
                            left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                            right_samples =
                                Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                            let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                            sample_rate = audio.sample_rate;
                            audio_clips.insert(*wf_id, AudioClipData {
                                samples: Arc::new(mono),
                                sample_rate: audio.sample_rate,
                                duration_secs: audio.duration_secs,
                            });
                        } else {
                            audio_clips.insert(*wf_id, AudioClipData {
                                samples: Arc::new(Vec::new()),
                                sample_rate: 48000,
                                duration_secs: 0.0,
                            });
                        }
                        if let Some(peaks) = s.load_peaks(&id_str) {
                            let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                            let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                            left_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                            right_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                        }
                        let filename = wf.audio.filename.clone();
                        waveforms.get_mut(wf_id).unwrap().audio = Arc::new(AudioData {
                            left_samples,
                            right_samples,
                            left_peaks,
                            right_peaks,
                            sample_rate,
                            filename,
                        });
                    }
                }

                (
                    cam,
                    storage::objects_from_stored(state.objects),
                    waveforms,
                    name,
                    folders,
                    bw,
                    state.browser_visible,
                    Some(expanded),
                    storage::effect_regions_from_stored(state.effect_regions),
                    storage::plugin_blocks_from_stored(state.plugin_blocks),
                    storage::loop_regions_from_stored(state.loop_regions),
                    storage::components_from_stored(state.components),
                    storage::component_instances_from_stored(state.component_instances),
                    audio_clips,
                    if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
                    storage::midi_clips_from_stored(state.midi_clips),
                    storage::instrument_regions_from_stored(state.instrument_regions),
                    state.layer_tree,
                )
            }
            None => {
                println!("  No saved project found, starting fresh");
                (
                    Camera::new(),
                    IndexMap::new(),
                    IndexMap::new(),
                    "Untitled".to_string(),
                    Vec::new(),
                    260.0,
                    false,
                    None,
                    Vec::new(),  // stored_effect_regions
                    Vec::new(),  // stored_plugin_blocks
                    Vec::new(),  // stored_loop_regions
                    Vec::new(),  // stored_components
                    Vec::new(),  // stored_component_instances
                    IndexMap::new(),  // audio_clips
                    DEFAULT_BPM,
                    Vec::new(),  // stored_midi_clips
                    Vec::new(),  // stored_instrument_regions
                    Vec::new(),  // stored_layer_tree
                )
            }
        };

        let settings = Settings::load();

        // Sample library folders are stored globally in settings so they
        // persist across restarts regardless of project save state.
        // Merge: use settings folders as the authoritative source, but keep
        // any project-specific folders that aren't already in settings.
        let global_folders: Vec<PathBuf> = settings
            .sample_library_folders
            .iter()
            .map(PathBuf::from)
            .collect();
        let mut merged_folders = global_folders.clone();
        for f in &browser_folders {
            if !merged_folders.contains(f) {
                merged_folders.push(f.clone());
            }
        }
        let use_global = !settings.sample_library_folders.is_empty();
        let mut sample_browser = if use_global {
            // Rebuild expanded set: keep project expanded state, add any new global folders as expanded
            let mut expanded = browser_expanded.unwrap_or_default();
            for f in &global_folders {
                if !browser_folders.contains(f) {
                    expanded.insert(f.clone());
                }
            }
            ui::browser::SampleBrowser::from_state(
                merged_folders,
                expanded,
                browser_visible || !global_folders.is_empty(),
            )
        } else if let Some(expanded) = browser_expanded {
            ui::browser::SampleBrowser::from_state(browser_folders, expanded, browser_visible)
        } else {
            ui::browser::SampleBrowser::from_folders(browser_folders)
        };
        sample_browser.width = browser_width;

        let mut settings = settings;

        let device_name = if settings.audio_output_device == "No Device" {
            None
        } else {
            Some(settings.audio_output_device.as_str())
        };
        let audio_engine = AudioEngine::new_with_device(device_name);
        if let Some(ref engine) = audio_engine {
            let actual = engine.device_name();
            if settings.audio_output_device != actual {
                println!(
                    "  Correcting stale output device setting: '{}' -> '{}'",
                    settings.audio_output_device, actual
                );
                settings.audio_output_device = actual.to_string();
                settings.save();
            }
            engine.set_bpm(loaded_bpm);
            engine.set_metronome_enabled(settings.metronome_enabled);
        } else {
            println!("  Warning: no audio output device found");
        }

        let recorder = AudioRecorder::new();
        if recorder.is_none() {
            println!("  Warning: no audio input device found");
        }

        let plugin_registry = effects::PluginRegistry::new();

        // Restore effect region geometry
        let restored_effect_regions: IndexMap<EntityId, effects::EffectRegion> = stored_effect_regions
            .into_iter()
            .map(|(id, ser)| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                (id, region)
            })
            .collect();

        // Restore plugin blocks; instances will be loaded lazily on first scan
        let mut restored_plugin_blocks: IndexMap<EntityId, effects::PluginBlock> = stored_plugin_blocks
            .into_iter()
            .map(|(id, spb)| {
                let mut pb = effects::PluginBlock::new(
                    spb.position,
                    spb.plugin_id,
                    spb.plugin_name,
                    std::path::PathBuf::new(),
                );
                pb.size = spb.size;
                pb.color = spb.color;
                pb.bypass = spb.bypass;
                if !spb.state.is_empty() {
                    pb.pending_state = Some(spb.state);
                }
                if !spb.params.is_empty() && spb.params.len() % 8 == 0 {
                    pb.pending_params = Some(spb.params.chunks_exact(8)
                        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                        .collect());
                }
                (id, pb)
            })
            .collect();

        // Migration: if old project had plugins in regions but no plugin_blocks, generate them
        if restored_plugin_blocks.is_empty() {
            if let Some(s) = &storage {
                if let Some(raw_state) = s.load_project_state() {
                    for ser in raw_state.effect_regions.iter() {
                        if ser.plugin_ids.is_empty() {
                            continue;
                        }
                        let region_pos = ser.position;
                        let mut x_offset = 10.0;
                        for (pid, pname) in ser.plugin_ids.iter().zip(ser.plugin_names.iter()) {
                            let pos = [region_pos[0] + x_offset, region_pos[1] + 10.0];
                            let pb = effects::PluginBlock::new(
                                pos,
                                pid.clone(),
                                pname.clone(),
                                std::path::PathBuf::new(),
                            );
                            restored_plugin_blocks.insert(entity_id::new_id(), pb);
                            x_offset += effects::PLUGIN_BLOCK_DEFAULT_SIZE[0] + 10.0;
                        }
                    }
                    if !restored_plugin_blocks.is_empty() {
                        println!("  Migrated {} plugin blocks from old region format", restored_plugin_blocks.len());
                    }
                }
            }
        }

        let restored_loop_regions: IndexMap<EntityId, LoopRegion> = stored_loop_regions
            .into_iter()
            .map(|(id, slr)| (id, LoopRegion {
                position: slr.position,
                size: slr.size,
                enabled: slr.enabled,
            }))
            .collect();

        let restored_components: IndexMap<EntityId, component::ComponentDef> = stored_components
            .into_iter()
            .map(|(id, sc)| {
                let waveform_ids = sc.waveform_ids.iter()
                    .map(|s| s.parse::<EntityId>().unwrap_or_else(|_| entity_id::new_id()))
                    .collect();
                (id, component::ComponentDef {
                    id,
                    name: sc.name,
                    position: sc.position,
                    size: sc.size,
                    waveform_ids,
                })
            })
            .collect();
        let restored_instances: IndexMap<EntityId, component::ComponentInstance> = stored_component_instances
            .into_iter()
            .map(|(id, si)| {
                let component_id = si.component_id.parse::<EntityId>().unwrap_or_else(|_| entity_id::new_id());
                (id, component::ComponentInstance {
                    component_id,
                    position: si.position,
                })
            })
            .collect();
        let next_component_id = entity_id::new_id();

        let restored_midi_clips: IndexMap<EntityId, midi::MidiClip> = stored_midi_clips
            .into_iter()
            .map(|(id, smc)| (id, midi::MidiClip {
                position: smc.position,
                size: smc.size,
                color: smc.color,
                notes: smc.notes.into_iter().map(|n| midi::MidiNote {
                    pitch: n.pitch as u8,
                    start_px: n.start_px,
                    duration_px: n.duration_px,
                    velocity: n.velocity as u8,
                }).collect(),
                pitch_range: (smc.pitch_low as u8, smc.pitch_high as u8),
                grid_mode: storage::grid_mode_from_stored(&smc.grid_mode_tag, &smc.grid_mode_value),
                triplet_grid: smc.triplet_grid,
                velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
                instrument_region_id: if smc.instrument_region_id.is_empty() { None } else { smc.instrument_region_id.parse().ok() },
            }))
            .collect();

        let restored_instrument_regions: IndexMap<EntityId, instruments::InstrumentRegion> = stored_instrument_regions
            .into_iter()
            .map(|(id, sir)| {
                let mut ir = instruments::InstrumentRegion::new(sir.position, sir.size);
                ir.name = sir.name;
                ir.plugin_id = sir.plugin_id;
                ir.plugin_name = sir.plugin_name;
                if !sir.state.is_empty() {
                    ir.pending_state = Some(sir.state);
                }
                if !sir.params.is_empty() {
                    ir.pending_params = Some(sir.params.chunks(8).map(|chunk| {
                        let mut bytes = [0u8; 8];
                        bytes[..chunk.len()].copy_from_slice(chunk);
                        f64::from_le_bytes(bytes)
                    }).collect());
                }
                (id, ir)
            })
            .collect();

        let restored_layer_tree = {
            let mut tree = layers::tree_from_stored(&stored_layer_tree);
            layers::sync_tree(
                &mut tree,
                &restored_instrument_regions,
                &restored_midi_clips,
                &waveforms,
                &restored_effect_regions,
                &restored_plugin_blocks,
            );
            tree
        };

        let (pending_audio_tx, pending_audio_rx) = mpsc::channel();
        let (pending_remote_audio_tx, pending_remote_audio_rx) = mpsc::channel();

        Self {
            gpu: None,
            pending_gpu: Arc::new(std::sync::Mutex::new(None)),
            window: None,
            camera,
            objects,
            waveforms,
            audio_clips,
            audio_engine,
            recorder,
            recording_waveform_id: None,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            fade_handle_hovered: None,
            fade_curve_hovered: None,
            waveform_edge_hover: WaveformEdgeHover::None,
            midi_note_edge_hover: false,
            velocity_bar_hovered: false,
            velocity_divider_hovered: false,
            file_hovering: false,
            modifiers: ModifiersState::empty(),
            command_palette: None,
            context_menu: None,
            browser_context_path: None,
            sample_browser,
            layer_tree: restored_layer_tree,
            storage,
            has_saved_state,
            project_dirty: false,
            op_undo_stack: Vec::new(),
            op_redo_stack: Vec::new(),
            arrow_nudge_before: None,
            arrow_nudge_last: None,
            arrow_nudge_overlap_snapshots: IndexMap::new(),
            arrow_nudge_overlap_temp_splits: Vec::new(),
            current_project_name: project_name,
            effect_regions: restored_effect_regions,
            plugin_blocks: restored_plugin_blocks,
            components: restored_components,
            component_instances: restored_instances,
            next_component_id,
            plugin_registry,
            export_regions: IndexMap::new(),
            export_hover: ExportHover::None,
            loop_regions: restored_loop_regions,
            loop_hover: LoopHover::None,
            select_area: None,
            component_def_hover: ComponentDefHover::None,
            effect_region_hover: EffectRegionHover::None,
            instrument_region_hover: InstrumentRegionHover::None,
            midi_clips: restored_midi_clips,
            instrument_regions: restored_instrument_regions,
            editing_midi_clip: None,
            selected_midi_notes: Vec::new(),
            pending_midi_note_click: None,
            midi_note_select_rect: None,
            cmd_velocity_hover_note: None,
            editing_component: None,
            editing_effect_name: None,
            editing_waveform_name: None,
            bpm: loaded_bpm,
            editing_bpm: ui::value_entry::ValueEntry::new(),
            dragging_bpm: None,
            bpm_drag_overlap_snapshots: IndexMap::new(),
            bpm_drag_overlap_temp_splits: Vec::new(),
            last_click_time: TimeInstant::now(),
            last_vol_text_click_time: TimeInstant::now(),
            last_vol_knob_click_time: TimeInstant::now(),
            last_pan_knob_click_time: TimeInstant::now(),
            last_sample_bpm_text_click_time: TimeInstant::now(),
            last_pitch_text_click_time: TimeInstant::now(),
            last_click_world: [0.0; 2],
            last_cursor_send: TimeInstant::now(),
            clipboard: Clipboard::new(),
            settings,
            settings_window: None,
            plugin_editor: None,
            menu_state: None,
            toast_manager: ui::toast::ToastManager::new(),
            tooltip: ui::tooltip::TooltipState::new(),
            automation_mode: false,
            active_automation_param: AutomationParam::Volume,
            right_window: None,
            pending_audio_tx,
            pending_audio_rx,
            pending_remote_audio_tx,
            pending_remote_audio_rx,
            pending_audio_loads_count: 0,
            remote_storage: None,
            local_user: user::User {
                id: entity_id::new_id(),
                name: "Local".to_string(),
                color: user::USER_COLORS[0],
            },
            remote_users: std::collections::HashMap::new(),
            applied_remote_seqs: std::collections::HashSet::new(),
            network: network::NetworkManager::new_offline(),
            ws_runtime: None,
            connect_url: None,
            connect_project_id: None,
            pending_welcome: None,
            reconnect_attempt: 0,
            last_reconnect_time: None,
            cached_instances: Vec::with_capacity(2048),
            cached_wf_verts: Vec::with_capacity(32768),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
            computer_keyboard_armed: false,
            computer_keyboard_octave_offset: 0,
            computer_keyboard_velocity: midi_keyboard::DEFAULT_VELOCITY,
            keyboard_instrument_id: None,
            midi_keyboard_held: HashMap::new(),
        }
    }

    /// Returns true if the app is allowed to mutate state.
    /// Blocked when the user intended to connect but is currently disconnected.
    fn can_mutate(&self) -> bool {
        match self.network.mode() {
            network::NetworkMode::Offline => true,
            network::NetworkMode::Connected => true,
            _ => false, // Connecting or Disconnected
        }
    }

    /// Broadcast cursor world position to remote users (throttled to ~20/sec).
    /// Call this after any event that changes the world-space cursor position:
    /// mouse movement, camera panning, zooming, etc.
    fn broadcast_cursor_if_connected(&mut self) {
        #[cfg(not(feature = "native"))]
        return;
        #[cfg(feature = "native")]
        if self.network.is_connected() {
            let now = TimeInstant::now();
            if now.duration_since(self.last_cursor_send).as_millis() >= 25 {
                let world_pos = self.camera.screen_to_world(self.mouse_pos);
                self.network.send_ephemeral(crate::user::EphemeralMessage::CursorMove {
                    user_id: self.local_user.id,
                    position: world_pos,
                });
                self.last_cursor_send = now;
            }
        }
    }

    #[cfg(feature = "native")]
    fn connect_to_server(&mut self, url: &str, project_id: &str) {
        // Reuse existing runtime or create one
        if self.ws_runtime.is_none() {
            self.ws_runtime = Some(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime for networking"),
            );
        }
        let rt = self.ws_runtime.as_ref().unwrap();

        let (mgr, remote_op_tx, remote_op_rx, remote_eph_tx, remote_eph_rx) =
            network::NetworkManager::new_connected();

        let (welcome_tx, welcome_rx) = tokio::sync::oneshot::channel();
        let conn_state = mgr.connection_state.clone();

        let _handle = surreal_client::spawn_surreal_client(
            url.to_string(),
            project_id.to_string(),
            remote_op_tx,
            remote_op_rx,
            remote_eph_tx,
            remote_eph_rx,
            welcome_tx,
            conn_state,
            rt,
        );

        self.network = mgr;
        self.connect_url = Some(url.to_string());
        self.connect_project_id = Some(project_id.to_string());
        self.pending_welcome = Some(welcome_rx);
        log::info!("Connecting to SurrealDB at {}", url);
    }

    #[cfg(feature = "native")]
    fn save_project_state(&mut self) {
        if let Some(storage) = &self.storage {
            let stored_regions: Vec<storage::StoredEffectRegion> = self
                .effect_regions
                .iter()
                .map(|(id, er)| storage::StoredEffectRegion {
                    id: id.to_string(),
                    position: er.position,
                    size: er.size,
                    plugin_ids: Vec::new(),
                    plugin_names: Vec::new(),
                    name: er.name.clone(),
                })
                .collect();

            let stored_plugin_blocks: Vec<storage::StoredPluginBlock> = self
                .plugin_blocks
                .iter()
                .map(|(id, pb)| storage::StoredPluginBlock {
                    id: id.to_string(),
                    position: pb.position,
                    size: pb.size,
                    color: pb.color,
                    plugin_id: pb.plugin_id.clone(),
                    plugin_name: pb.plugin_name.clone(),
                    bypass: pb.bypass,
                    state: pb.gui.lock().ok()
                        .and_then(|g| g.as_ref().and_then(|gui| gui.get_state()))
                        .unwrap_or_default(),
                    params: {
                        let vals = pb.gui.lock().ok()
                            .and_then(|g| g.as_ref().map(|gui| gui.get_all_parameters()))
                            .unwrap_or_default();
                        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
                    },
                })
                .collect();

            let stored_components: Vec<storage::StoredComponent> = self
                .components
                .iter()
                .map(|(id, c)| storage::StoredComponent {
                    id: id.to_string(),
                    name: c.name.clone(),
                    position: c.position,
                    size: c.size,
                    waveform_ids: c.waveform_ids.iter().map(|wid| wid.to_string()).collect(),
                })
                .collect();
            let stored_instances: Vec<storage::StoredComponentInstance> = self
                .component_instances
                .iter()
                .map(|(id, inst)| storage::StoredComponentInstance {
                    id: id.to_string(),
                    component_id: inst.component_id.to_string(),
                    position: inst.position,
                })
                .collect();

            let stored_waveforms: Vec<storage::StoredWaveform> = self
                .waveforms
                .iter()
                .map(|(id, wf)| storage::StoredWaveform {
                    id: id.to_string(),
                    position: wf.position,
                    size: wf.size,
                    color: wf.color,
                    border_radius: wf.border_radius,
                    filename: wf.audio.filename.clone(),
                    fade_in_px: wf.fade_in_px,
                    fade_out_px: wf.fade_out_px,
                    fade_in_curve: wf.fade_in_curve,
                    fade_out_curve: wf.fade_out_curve,
                    sample_rate: wf.audio.sample_rate,
                    volume: wf.volume,
                    pan: wf.pan,
                    disabled: wf.disabled,
                    sample_offset_px: wf.sample_offset_px,
                    automation_volume: wf.automation.volume_lane().points.iter().map(|p| [p.t, p.value]).collect(),
                    automation_pan: wf.automation.pan_lane().points.iter().map(|p| [p.t, p.value]).collect(),
                })
                .collect();

            let state = ProjectState {
                version: 2,
                name: self.current_project_name.clone(),
                camera_position: self.camera.position,
                camera_zoom: self.camera.zoom,
                objects: storage::objects_to_stored(&self.objects),
                waveforms: stored_waveforms,
                browser_folders: self
                    .sample_browser
                    .root_folders
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                browser_width: self.sample_browser.width,
                browser_visible: self.sample_browser.visible,
                browser_expanded: self
                    .sample_browser
                    .expanded
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                effect_regions: stored_regions,
                plugin_blocks: stored_plugin_blocks,
                loop_regions: self
                    .loop_regions
                    .iter()
                    .map(|(id, lr)| storage::StoredLoopRegion {
                        id: id.to_string(),
                        position: lr.position,
                        size: lr.size,
                        enabled: lr.enabled,
                    })
                    .collect(),
                components: stored_components,
                component_instances: stored_instances,
                bpm: self.bpm,
                midi_clips: self.midi_clips.iter().map(|(id, mc)| {
                    let (grid_tag, grid_val) = storage::grid_mode_to_stored(mc.grid_mode);
                    storage::StoredMidiClip {
                        id: id.to_string(),
                        position: mc.position,
                        size: mc.size,
                        color: mc.color,
                        notes: mc.notes.iter().map(|n| storage::StoredMidiNote {
                            pitch: n.pitch as u32,
                            start_px: n.start_px,
                            duration_px: n.duration_px,
                            velocity: n.velocity as u32,
                        }).collect(),
                        pitch_low: mc.pitch_range.0 as u32,
                        pitch_high: mc.pitch_range.1 as u32,
                        grid_mode_tag: grid_tag,
                        grid_mode_value: grid_val,
                        triplet_grid: mc.triplet_grid,
                        instrument_region_id: mc.instrument_region_id.map(|id| id.to_string()).unwrap_or_default(),
                    }
                }).collect(),
                instrument_regions: self.instrument_regions.iter().map(|(id, ir)| storage::StoredInstrumentRegion {
                    id: id.to_string(),
                    position: ir.position,
                    size: ir.size,
                    name: ir.name.clone(),
                    plugin_id: ir.plugin_id.clone(),
                    plugin_name: ir.plugin_name.clone(),
                    state: ir.gui.lock().ok()
                        .and_then(|g| g.as_ref().and_then(|gui| gui.get_state()))
                        .unwrap_or_default(),
                    params: {
                        let vals = ir.gui.lock().ok()
                            .and_then(|g| g.as_ref().map(|gui| gui.get_all_parameters()))
                            .unwrap_or_default();
                        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
                    },
                }).collect(),
                layer_tree: layers::tree_to_stored(&self.layer_tree),
            };
            storage.save_project_state(state);

            // Update project name in index
            if let Some(path) = storage.current_project_path() {
                let path_str = path.to_string_lossy().to_string();
                storage.update_index_name(&path_str, &self.current_project_name);
            }

            // Save audio data and peaks for each waveform
            storage.clear_audio_and_peaks();
            for (wf_id, wf) in self.waveforms.iter() {
                let id_str = wf_id.to_string();
                let (mono, duration) = if let Some(clip) = self.audio_clips.get(wf_id) {
                    (&clip.samples, clip.duration_secs)
                } else {
                    continue;
                };
                storage.save_audio(
                    &id_str,
                    &wf.audio.left_samples,
                    &wf.audio.right_samples,
                    mono,
                    wf.audio.sample_rate,
                    duration,
                );
                storage.save_peaks(
                    &id_str,
                    wf.audio.left_peaks.block_size as u64,
                    &wf.audio.left_peaks.peaks,
                    &wf.audio.right_peaks.peaks,
                );
            }

            self.project_dirty = false;
            println!("Project '{}' saved", self.current_project_name);
        }
    }

    #[cfg(feature = "native")]
    fn save_project(&mut self) {
        self.save_project_state();
        if let Some(storage) = &self.storage {
            if storage.is_temp_project() {
                self.save_project_as();
            }
        }
    }

    #[cfg(feature = "native")]
    fn save_project_as(&mut self) {
        println!("Showing Save Project dialog...");
        let dest = rfd::FileDialog::new()
            .set_title("Save Project")
            .pick_folder();
        if let Some(dest) = dest {
            if let Some(storage) = &mut self.storage {
                if storage.save_project_to(&dest) {
                    if let Some(folder_name) = dest.file_name() {
                        self.current_project_name = folder_name.to_string_lossy().to_string();
                    }
                    self.save_project_state();
                    println!("Project saved to {:?}", dest);
                } else {
                    println!("Failed to save project to {:?}", dest);
                }
            }
        }
    }

    #[cfg(feature = "native")]
    fn handle_menu_event(&mut self, id: MenuId) {
        let menu = match &self.menu_state {
            Some(m) => m,
            None => return,
        };

        if id == menu.new_project {
            self.new_project();
            self.refresh_open_project_menu();
            self.request_redraw();
        } else if id == menu.save_project {
            self.save_project();
            self.refresh_open_project_menu();
        } else if id == menu.open_project {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Open Project")
                .pick_folder()
            {
                let path = folder.to_string_lossy().to_string();
                self.load_project(&path);
                self.refresh_open_project_menu();
                self.request_redraw();
            }
        } else if id == menu.settings {
            self.command_palette = None;
            self.context_menu = None;
            self.settings_window = if self.settings_window.is_some() {
                None
            } else {
                Some(SettingsWindow::new())
            };
            self.request_redraw();
        } else if id == menu.undo {
            self.undo_op();
        } else if id == menu.redo {
            self.redo_op();
        } else if id == menu.copy {
            self.copy_selected();
            self.request_redraw();
        } else if id == menu.paste {
            self.paste_clipboard();
            self.request_redraw();
        } else if id == menu.select_all {
            self.execute_command(CommandAction::SelectAll);
            self.request_redraw();
        } else if let Some(project_path) = menu
            .open_project_items
            .iter()
            .find(|(mid, _)| *mid == id)
            .map(|(_, p)| p.clone())
        {
            self.load_project(&project_path);
            self.refresh_open_project_menu();
            self.request_redraw();
        }
    }

    #[cfg(feature = "native")]
    fn new_project(&mut self) {
        self.save_project_state();

        // Create a new temp project folder
        if let Some(storage) = &mut self.storage {
            if storage.create_temp_project().is_none() {
                println!("Failed to create temp project");
                return;
            }
        }

        self.current_project_name = "Untitled".to_string();
        self.objects = IndexMap::new();
        self.waveforms.clear();
        self.audio_clips.clear();
        self.effect_regions.clear();
        self.plugin_blocks.clear();
        self.components.clear();
        self.component_instances.clear();
        self.next_component_id = entity_id::new_id();
        self.selected.clear();
        self.op_undo_stack.clear();
        self.op_redo_stack.clear();
        self.camera = Camera::new();
        self.export_regions.clear();
        self.loop_regions.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm.cancel();
        self.dragging_bpm = None;
        self.bpm_drag_overlap_snapshots.clear();
        for id in self.bpm_drag_overlap_temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }
        self.command_palette = None;
        self.context_menu = None;

        if let Some(gpu) = &self.gpu {
            self.camera.zoom = gpu.window.scale_factor() as f32;
        }

        self.sync_audio_clips();
        self.save_project_state();
        println!("New project created");
    }

    #[cfg(feature = "native")]
    fn load_project(&mut self, project_path: &str) {
        // Check if same project
        if let Some(s) = &self.storage {
            if let Some(cur) = s.current_project_path() {
                if cur.to_string_lossy() == project_path {
                    return;
                }
            }
        }
        self.save_project_state();

        // Open the project DB
        let path = PathBuf::from(project_path);
        if let Some(s) = &mut self.storage {
            if !s.open_project(&path) {
                println!("Failed to open project at '{project_path}'");
                return;
            }
        }

        let state = match self.storage.as_ref().and_then(|s| s.load_project_state()) {
            Some(s) => s,
            None => {
                println!("Failed to load project state from '{project_path}'");
                return;
            }
        };

        println!(
            "Loading project '{}' ({} objects, {} waveforms)",
            state.name,
            state.objects.len(),
            state.waveforms.len(),
        );

        self.current_project_name = if let Some(meta) = storage::Storage::read_project_json(&path) {
            meta.name
        } else {
            state.name.clone()
        };
        self.camera = Camera {
            position: state.camera_position,
            zoom: state.camera_zoom,
        };
        self.objects = storage::objects_from_stored(state.objects);
        let wf_pairs = storage::waveforms_from_stored(state.waveforms);
        self.waveforms = wf_pairs
            .into_iter()
            .map(|(id, sw)| (id, WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate: sw.sample_rate,
                    filename: sw.filename.clone(),
                }),
                filename: sw.filename,
                position: sw.position,
                size: sw.size,
                color: sw.color,
                border_radius: sw.border_radius,
                fade_in_px: sw.fade_in_px,
                fade_out_px: sw.fade_out_px,
                fade_in_curve: sw.fade_in_curve,
                fade_out_curve: sw.fade_out_curve,
                volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                pan: sw.pan,
                warp_mode: ui::waveform::WarpMode::Off,
                sample_bpm: self.bpm,
                pitch_semitones: 0.0,
                disabled: sw.disabled,
                sample_offset_px: sw.sample_offset_px,
                automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
            }))
            .collect();

        // Restore audio data and peaks from DB
        self.audio_clips.clear();
        if let Some(s) = &self.storage {
            let wf_ids: Vec<EntityId> = self.waveforms.keys().cloned().collect();
            for wf_id in &wf_ids {
                let id_str = wf_id.to_string();
                let wf = self.waveforms.get(wf_id).unwrap();
                let mut left_samples = Arc::new(Vec::new());
                let mut right_samples = Arc::new(Vec::new());
                let mut sample_rate = wf.audio.sample_rate;
                let mut left_peaks = wf.audio.left_peaks.clone();
                let mut right_peaks = wf.audio.right_peaks.clone();

                if let Some(audio) = s.load_audio(&id_str) {
                    left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                    right_samples = Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                    let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                    sample_rate = audio.sample_rate;
                    self.audio_clips.insert(*wf_id, AudioClipData {
                        samples: Arc::new(mono),
                        sample_rate: audio.sample_rate,
                        duration_secs: audio.duration_secs,
                    });
                } else {
                    self.audio_clips.insert(*wf_id, AudioClipData {
                        samples: Arc::new(Vec::new()),
                        sample_rate: 48000,
                        duration_secs: 0.0,
                    });
                }
                if let Some(peaks) = s.load_peaks(&id_str) {
                    let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                    let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                    left_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                    right_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                }
                let filename = wf.audio.filename.clone();
                self.waveforms.get_mut(wf_id).unwrap().audio = Arc::new(AudioData {
                    left_samples,
                    right_samples,
                    left_peaks,
                    right_peaks,
                    sample_rate,
                    filename,
                });
            }
        }

        self.effect_regions = storage::effect_regions_from_stored(state.effect_regions)
            .into_iter()
            .map(|(id, ser)| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                (id, region)
            })
            .collect();

        self.plugin_blocks = storage::plugin_blocks_from_stored(state.plugin_blocks)
            .into_iter()
            .map(|(id, spb)| {
                let mut pb = effects::PluginBlock::new(
                    spb.position,
                    spb.plugin_id,
                    spb.plugin_name,
                    std::path::PathBuf::new(),
                );
                pb.size = spb.size;
                pb.color = spb.color;
                pb.bypass = spb.bypass;
                if !spb.state.is_empty() {
                    pb.pending_state = Some(spb.state);
                }
                if !spb.params.is_empty() && spb.params.len() % 8 == 0 {
                    pb.pending_params = Some(spb.params.chunks_exact(8)
                        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                        .collect());
                }
                (id, pb)
            })
            .collect();

        self.components = storage::components_from_stored(state.components)
            .into_iter()
            .map(|(id, sc)| {
                let waveform_ids = sc.waveform_ids.iter()
                    .map(|s| s.parse::<EntityId>().unwrap_or_else(|_| entity_id::new_id()))
                    .collect();
                (id, component::ComponentDef {
                    id,
                    name: sc.name,
                    position: sc.position,
                    size: sc.size,
                    waveform_ids,
                })
            })
            .collect();
        self.component_instances = storage::component_instances_from_stored(state.component_instances)
            .into_iter()
            .map(|(id, si)| {
                let component_id = si.component_id.parse::<EntityId>().unwrap_or_else(|_| entity_id::new_id());
                (id, component::ComponentInstance {
                    component_id,
                    position: si.position,
                })
            })
            .collect();
        self.next_component_id = entity_id::new_id();
        self.bpm = if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM };
        #[cfg(feature = "native")]
        if let Some(engine) = &self.audio_engine {
            engine.set_bpm(self.bpm);
        }

        self.sample_browser = if !state.browser_expanded.is_empty() {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let expanded: HashSet<PathBuf> =
                state.browser_expanded.iter().map(PathBuf::from).collect();
            let mut b =
                ui::browser::SampleBrowser::from_state(folders, expanded, state.browser_visible);
            b.width = if state.browser_width >= 480.0 {
                state.browser_width
            } else {
                480.0
            };
            b
        } else {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let mut b = ui::browser::SampleBrowser::from_folders(folders);
            b.width = 260.0;
            b
        };

        self.selected.clear();
        self.op_undo_stack.clear();
        self.op_redo_stack.clear();
        self.export_regions.clear();

        self.loop_regions = storage::loop_regions_from_stored(state.loop_regions)
            .into_iter()
            .map(|(id, slr)| (id, LoopRegion {
                position: slr.position,
                size: slr.size,
                enabled: slr.enabled,
            }))
            .collect();

        self.midi_clips = storage::midi_clips_from_stored(state.midi_clips)
            .into_iter()
            .map(|(id, smc)| (id, midi::MidiClip {
                position: smc.position,
                size: smc.size,
                color: smc.color,
                notes: smc.notes.into_iter().map(|n| midi::MidiNote {
                    pitch: n.pitch as u8,
                    start_px: n.start_px,
                    duration_px: n.duration_px,
                    velocity: n.velocity as u8,
                }).collect(),
                pitch_range: (smc.pitch_low as u8, smc.pitch_high as u8),
                grid_mode: storage::grid_mode_from_stored(&smc.grid_mode_tag, &smc.grid_mode_value),
                triplet_grid: smc.triplet_grid,
                velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
                instrument_region_id: if smc.instrument_region_id.is_empty() { None } else { smc.instrument_region_id.parse().ok() },
            }))
            .collect();

        self.instrument_regions = storage::instrument_regions_from_stored(state.instrument_regions)
            .into_iter()
            .map(|(id, sir)| {
                let mut ir = instruments::InstrumentRegion::new(sir.position, sir.size);
                ir.name = sir.name;
                ir.plugin_id = sir.plugin_id;
                ir.plugin_name = sir.plugin_name;
                if !sir.state.is_empty() {
                    ir.pending_state = Some(sir.state);
                }
                if !sir.params.is_empty() {
                    ir.pending_params = Some(sir.params.chunks(8).map(|chunk| {
                        let mut bytes = [0u8; 8];
                        bytes[..chunk.len()].copy_from_slice(chunk);
                        f64::from_le_bytes(bytes)
                    }).collect());
                }
                (id, ir)
            })
            .collect();

        {
            let mut tree = layers::tree_from_stored(&state.layer_tree);
            layers::sync_tree(
                &mut tree,
                &self.instrument_regions,
                &self.midi_clips,
                &self.waveforms,
                &self.effect_regions,
                &self.plugin_blocks,
            );
            self.layer_tree = tree;
        }

        self.editing_midi_clip = None;
        self.selected_midi_notes.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm.cancel();
        self.dragging_bpm = None;
        self.bpm_drag_overlap_snapshots.clear();
        for id in self.bpm_drag_overlap_temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }
        self.command_palette = None;
        self.context_menu = None;

        // If plugins are already scanned, open vst3-gui instances for restored plugin blocks
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if self.plugin_registry.is_scanned() {
            for (_pb_id, pb) in &mut self.plugin_blocks {
                let has_gui = pb.gui.lock().ok().map_or(false, |g| g.is_some());
                if !has_gui {
                    if let Some(entry) = self.plugin_registry.plugins.iter().find(|e| e.info.unique_id == pb.plugin_id) {
                        pb.plugin_path = entry.info.path.clone();
                    }
                    let path = pb.plugin_path.to_string_lossy().to_string();
                    if !path.is_empty() {
                        if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &pb.plugin_id, &pb.plugin_name) {
                            gui.hide();
                            gui.setup_processing(48000.0, 512);
                            if let Some(state) = &pb.pending_state {
                                gui.set_state(state);
                                println!("  Restored plugin state ({} bytes)", state.len());
                            }
                            if let Some(params) = &pb.pending_params {
                                gui.set_all_parameters(params);
                                println!("  Restored {} plugin parameters", params.len());
                            }
                            if let Ok(mut g) = pb.gui.lock() {
                                *g = Some(gui);
                            }
                            println!("  Loaded plugin '{}', path='{}'", pb.plugin_name, pb.plugin_path.display());
                        }
                    }
                }
            }
        }

        self.sync_audio_clips();
        println!("Project '{}' loaded", self.current_project_name);
    }

    #[cfg(feature = "native")]
    fn refresh_open_project_menu(&mut self) {
        let menu = match &mut self.menu_state {
            Some(m) => m,
            None => return,
        };

        while menu.open_submenu.remove_at(0).is_some() {}

        let mut new_items: Vec<(MenuId, String)> = Vec::new();
        if let Some(s) = &self.storage {
            for entry in s.list_projects() {
                if entry.is_temp {
                    continue;
                }
                let exists = std::path::Path::new(&entry.path).exists();
                let item = muda::MenuItem::new(&entry.name, exists, None);
                if exists {
                    new_items.push((item.id().clone(), entry.path.clone()));
                }
                let _ = menu.open_submenu.append(&item);
            }
        }
        if new_items.is_empty() {
            let _ = menu
                .open_submenu
                .append(&muda::MenuItem::new("No Projects", false, None));
        }
        menu.open_project_items = new_items;
    }

    fn update_component_bounds(&mut self, comp_id: EntityId) {
        let indices = if let Some(comp) = self.components.get(&comp_id) {
            comp.waveform_ids.clone()
        } else {
            return;
        };
        if indices.is_empty() {
            return;
        }
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &indices);
        if let Some(comp) = self.components.get_mut(&comp_id) {
            comp.position = pos;
            comp.size = size;
        }
    }

    /// Resolve overlapping audio waveforms, analogous to `MidiClip::resolve_note_overlaps`.
    /// `active_ids` are the waveforms that "win" — other waveforms on the same track
    /// (Y-overlap) that collide horizontally get cropped or deleted.
    /// Returns ops describing all mutations (for undo support).
    fn resolve_waveform_overlaps(&mut self, active_ids: &[EntityId]) -> Vec<operations::Operation> {
        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut to_delete: HashSet<EntityId> = HashSet::new();
        let mut updates: Vec<(EntityId, ui::waveform::WaveformView, ui::waveform::WaveformView)> = Vec::new();
        let mut to_create: Vec<(EntityId, ui::waveform::WaveformView, Option<AudioClipData>)> = Vec::new();

        for &aid in active_ids {
            let (a_pos, a_size) = match self.waveforms.get(&aid) {
                Some(wf) => (wf.position, wf.size),
                None => continue,
            };
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id) && !to_delete.contains(id))
                .copied()
                .collect();

            for bid in other_ids {
                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) => wf,
                    None => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) {
                    continue;
                }

                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];

                // Case 1: B fully covered by A
                if b_start >= a_start && b_end <= a_end {
                    to_delete.insert(bid);
                    continue;
                }

                // Case 4: A fully inside B — split B into left and right portions
                if b_start < a_start && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;

                    if left_width >= WAVEFORM_MIN_WIDTH_PX {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = left_width;
                        wf.fade_out_px = 0.0;
                        if wf.fade_in_px > left_width * 0.5 {
                            wf.fade_in_px = left_width * 0.5;
                        }
                        updates.push((bid, before.clone(), wf.clone()));
                    } else {
                        to_delete.insert(bid);
                    }

                    if right_width >= WAVEFORM_MIN_WIDTH_PX {
                        let right_id = new_id();
                        let mut right_wf = before.clone();
                        right_wf.position[0] = a_end;
                        right_wf.size[0] = right_width;
                        right_wf.sample_offset_px += a_end - b_start;
                        right_wf.fade_in_px = 0.0;
                        if right_wf.fade_out_px > right_width * 0.5 {
                            right_wf.fade_out_px = right_width * 0.5;
                        }
                        let ac = self.audio_clips.get(&bid).cloned();
                        self.waveforms.insert(right_id, right_wf.clone());
                        if let Some(clip) = ac.clone() {
                            self.audio_clips.insert(right_id, clip);
                        }
                        to_create.push((right_id, right_wf, ac));

                        for comp in self.components.values_mut() {
                            if comp.waveform_ids.contains(&bid) {
                                let mut new_ids = Vec::new();
                                for &wi in &comp.waveform_ids {
                                    new_ids.push(wi);
                                    if wi == bid {
                                        new_ids.push(right_id);
                                    }
                                }
                                comp.waveform_ids = new_ids;
                            }
                        }
                    }
                    continue;
                }

                // Case 2: B's tail overlaps A's start (B starts before A, ends inside A)
                if b_start < a_start && b_end > a_start {
                    let before = self.waveforms[&bid].clone();
                    let new_width = a_start - b_start;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = new_width;
                        if wf.fade_out_px > new_width * 0.5 {
                            wf.fade_out_px = new_width * 0.5;
                        }
                        updates.push((bid, before, wf.clone()));
                    }
                }

                // Case 3: B's head overlaps A's end (B starts inside A, ends after A)
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.position[0] = a_end;
                        wf.size[0] = new_width;
                        wf.sample_offset_px += crop_amount;
                        if wf.fade_in_px > new_width * 0.5 {
                            wf.fade_in_px = new_width * 0.5;
                        }
                        updates.push((bid, before, wf.clone()));
                    }
                }
            }
        }

        let mut ops: Vec<operations::Operation> = Vec::new();
        for (id, before, after) in updates {
            if !to_delete.contains(&id) {
                ops.push(operations::Operation::UpdateWaveform { id, before, after });
            }
        }
        for (id, wf_data, ac) in to_create {
            ops.push(operations::Operation::CreateWaveform {
                id,
                data: wf_data,
                audio_clip: ac.map(|c| (id, c)),
            });
        }
        for &id in &to_delete {
            if let Some(data) = self.waveforms.shift_remove(&id) {
                let ac = self.audio_clips.shift_remove(&id);
                ops.push(operations::Operation::DeleteWaveform {
                    id,
                    data,
                    audio_clip: ac.map(|c| (id, c)),
                });
            }
        }
        ops
    }

    /// Resolve mutual overlaps among ALL waveforms on the same track.
    /// Rightmost waveform wins; waveforms to its left get cropped/deleted.
    /// Used after BPM changes where every waveform's position shifts.
    fn resolve_all_waveform_overlaps(&mut self) -> Vec<operations::Operation> {
        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut all_ops = Vec::new();
        let mut processed: HashSet<EntityId> = HashSet::new();

        for (id, _) in &sorted {
            if !self.waveforms.contains_key(id) { continue; }
            processed.insert(*id);
            let ops = self.resolve_waveform_overlaps(&[*id]);
            all_ops.extend(ops);
        }
        all_ops
    }

    /// Same as `resolve_all_waveform_overlaps` but live (uses snapshots for restore).
    fn resolve_all_waveform_overlaps_live(
        &mut self,
        snapshots: &mut IndexMap<EntityId, WaveformView>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }

        for (id, original) in snapshots.iter() {
            if let Some(wf) = self.waveforms.get_mut(id) {
                *wf = original.clone();
            } else {
                self.waveforms.insert(*id, original.clone());
            }
        }

        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut new_snapshots: IndexMap<EntityId, WaveformView> = IndexMap::new();

        for (active_id, _) in &sorted {
            if !self.waveforms.contains_key(active_id) { continue; }
            let a_pos = self.waveforms[active_id].position;
            let a_size = self.waveforms[active_id].size;
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| *id != active_id)
                .copied()
                .collect();

            for bid in other_ids {
                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) if !wf.disabled => wf,
                    _ => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];
                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                if !snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, bwf.clone());
                } else if snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                if b_start >= a_start && b_end <= a_end {
                    self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    continue;
                }
                if b_start < a_start && b_end > a_end {
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;
                    if left_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = left_width;
                        wf.fade_out_px = 0.0;
                        if wf.fade_in_px > left_width * 0.5 { wf.fade_in_px = left_width * 0.5; }
                    }
                    if right_width >= WAVEFORM_MIN_WIDTH_PX {
                        let orig = new_snapshots.get(&bid)
                            .or_else(|| snapshots.get(&bid))
                            .unwrap();
                        let mut right_wf = orig.clone();
                        right_wf.position[0] = a_end;
                        right_wf.size[0] = right_width;
                        right_wf.sample_offset_px += a_end - b_start;
                        right_wf.fade_in_px = 0.0;
                        if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                        let right_id = new_id();
                        if let Some(clip) = self.audio_clips.get(&bid).cloned() {
                            self.audio_clips.insert(right_id, clip);
                        }
                        self.waveforms.insert(right_id, right_wf);
                        temp_splits.push(right_id);
                    }
                    continue;
                }
                if b_start < a_start && b_end > a_start {
                    let new_width = a_start - b_start;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = new_width;
                        if wf.fade_out_px > new_width * 0.5 { wf.fade_out_px = new_width * 0.5; }
                    }
                }
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.position[0] = a_end;
                        wf.size[0] = new_width;
                        wf.sample_offset_px += crop_amount;
                        if wf.fade_in_px > new_width * 0.5 { wf.fade_in_px = new_width * 0.5; }
                    }
                }
            }
        }

        let prev_keys: Vec<EntityId> = snapshots.keys().copied().collect();
        for id in prev_keys {
            if !new_snapshots.contains_key(&id) {
                snapshots.shift_remove(&id);
            }
        }
        for (id, original) in new_snapshots {
            snapshots.entry(id).or_insert(original);
        }
    }

    /// Live overlap resolution during drag. Restores previously-affected waveforms
    /// from `snapshots`, then re-resolves. Mutates `snapshots` to track affected waveforms.
    /// Deleted waveforms are hidden (set disabled=true) rather than removed, so they
    /// can be restored if the user drags away.
    /// `temp_splits` tracks temporary waveforms created by split (Case 4) so they can
    /// be cleaned up on subsequent calls or on commit.
    fn resolve_waveform_overlaps_live(
        &mut self,
        active_ids: &[EntityId],
        snapshots: &mut IndexMap<EntityId, WaveformView>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        // 0. Remove temporary split waveforms from previous frame
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }

        // 1. Restore all previously-affected waveforms to their original state
        for (id, original) in snapshots.iter() {
            if let Some(wf) = self.waveforms.get_mut(id) {
                *wf = original.clone();
            } else {
                self.waveforms.insert(*id, original.clone());
            }
        }

        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut new_snapshots: IndexMap<EntityId, WaveformView> = IndexMap::new();

        for &aid in active_ids {
            let (a_pos, a_size) = match self.waveforms.get(&aid) {
                Some(wf) => (wf.position, wf.size),
                None => continue,
            };
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id))
                .copied()
                .collect();

            for bid in other_ids {
                if new_snapshots.contains_key(&bid) {
                    let bwf = match self.waveforms.get(&bid) {
                        Some(wf) => wf,
                        None => continue,
                    };
                    if bwf.disabled { continue; }
                    let b_y0 = bwf.position[1];
                    let b_y1 = b_y0 + bwf.size[1];
                    if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                    let b_start = bwf.position[0];
                    let b_end = b_start + bwf.size[0];

                    if b_start >= a_start && b_end <= a_end {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                        continue;
                    }
                    if b_start < a_start && b_end > a_end {
                        let left_width = a_start - b_start;
                        let right_width = b_end - a_end;
                        if left_width < WAVEFORM_MIN_WIDTH_PX {
                            self.waveforms.get_mut(&bid).unwrap().disabled = true;
                        } else {
                            let wf = self.waveforms.get_mut(&bid).unwrap();
                            wf.size[0] = left_width;
                            wf.fade_out_px = 0.0;
                            if wf.fade_in_px > left_width * 0.5 { wf.fade_in_px = left_width * 0.5; }
                        }
                        if right_width >= WAVEFORM_MIN_WIDTH_PX {
                            let orig = new_snapshots.get(&bid).unwrap_or(&self.waveforms[&bid]);
                            let mut right_wf = orig.clone();
                            right_wf.position[0] = a_end;
                            right_wf.size[0] = right_width;
                            right_wf.sample_offset_px += a_end - orig.position[0];
                            right_wf.fade_in_px = 0.0;
                            if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                            let right_id = new_id();
                            if let Some(clip) = self.audio_clips.get(&bid).cloned() {
                                self.audio_clips.insert(right_id, clip);
                            }
                            self.waveforms.insert(right_id, right_wf);
                            temp_splits.push(right_id);
                        }
                        continue;
                    }
                    if b_start < a_start && b_end > a_start {
                        let new_width = a_start - b_start;
                        if new_width < WAVEFORM_MIN_WIDTH_PX {
                            self.waveforms.get_mut(&bid).unwrap().disabled = true;
                        } else {
                            let wf = self.waveforms.get_mut(&bid).unwrap();
                            wf.size[0] = new_width;
                            if wf.fade_out_px > new_width * 0.5 { wf.fade_out_px = new_width * 0.5; }
                        }
                    }
                    if b_start >= a_start && b_start < a_end && b_end > a_end {
                        let crop_amount = a_end - b_start;
                        let new_width = b_end - a_end;
                        if new_width < WAVEFORM_MIN_WIDTH_PX {
                            self.waveforms.get_mut(&bid).unwrap().disabled = true;
                        } else {
                            let wf = self.waveforms.get_mut(&bid).unwrap();
                            wf.position[0] = a_end;
                            wf.size[0] = new_width;
                            wf.sample_offset_px += crop_amount;
                            if wf.fade_in_px > new_width * 0.5 { wf.fade_in_px = new_width * 0.5; }
                        }
                    }
                    continue;
                }

                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) => wf,
                    None => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];

                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                if !snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, bwf.clone());
                } else if !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                if b_start >= a_start && b_end <= a_end {
                    self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    continue;
                }
                if b_start < a_start && b_end > a_end {
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;
                    if left_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = left_width;
                        wf.fade_out_px = 0.0;
                        if wf.fade_in_px > left_width * 0.5 { wf.fade_in_px = left_width * 0.5; }
                    }
                    if right_width >= WAVEFORM_MIN_WIDTH_PX {
                        let orig = new_snapshots.get(&bid)
                            .or_else(|| snapshots.get(&bid))
                            .unwrap_or(&self.waveforms[&bid]);
                        let mut right_wf = orig.clone();
                        right_wf.position[0] = a_end;
                        right_wf.size[0] = right_width;
                        right_wf.sample_offset_px += a_end - b_start;
                        right_wf.fade_in_px = 0.0;
                        if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                        let right_id = new_id();
                        if let Some(clip) = self.audio_clips.get(&bid).cloned() {
                            self.audio_clips.insert(right_id, clip);
                        }
                        self.waveforms.insert(right_id, right_wf);
                        temp_splits.push(right_id);
                    }
                    continue;
                }
                if b_start < a_start && b_end > a_start {
                    let new_width = a_start - b_start;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = new_width;
                        if wf.fade_out_px > new_width * 0.5 { wf.fade_out_px = new_width * 0.5; }
                    }
                }
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.position[0] = a_end;
                        wf.size[0] = new_width;
                        wf.sample_offset_px += crop_amount;
                        if wf.fade_in_px > new_width * 0.5 { wf.fade_in_px = new_width * 0.5; }
                    }
                }
            }
        }

        let prev_keys: Vec<EntityId> = snapshots.keys().copied().collect();
        for id in prev_keys {
            if !new_snapshots.contains_key(&id) {
                snapshots.shift_remove(&id);
            }
        }
        for (id, original) in new_snapshots {
            snapshots.entry(id).or_insert(original);
        }
    }

    pub(crate) fn update_right_window(&mut self) {
        if let Some(HitTarget::Waveform(id)) = self.selected.first().copied() {
            if let Some(wf) = self.waveforms.get(&id) {
                // Preserve vol_entry when updating the same waveform so that
                // click-to-edit isn't reset by the unconditional update_right_window
                // call at the end of the mouse-released handler.
                let (vol_entry, sample_bpm_entry, pitch_entry, vol_fader_focused, pan_knob_focused, pitch_focused, sample_bpm_focused) = if self.right_window.as_ref().map_or(false, |rw| rw.waveform_id == id) {
                    let rw = self.right_window.take().unwrap();
                    (rw.vol_entry, rw.sample_bpm_entry, rw.pitch_entry, rw.vol_fader_focused, rw.pan_knob_focused, rw.pitch_focused, rw.sample_bpm_focused)
                } else {
                    (ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), false, false, false, false)
                };
                self.right_window = Some(ui::right_window::RightWindow {
                    waveform_id: id,
                    volume: wf.volume,
                    pan: wf.pan,
                    warp_mode: wf.warp_mode,
                    sample_bpm: wf.sample_bpm,
                    pitch_semitones: wf.pitch_semitones,
                    vol_dragging: false,
                    pan_dragging: false,
                    sample_bpm_dragging: false,
                    pitch_dragging: false,
                    drag_start_y: 0.0,
                    drag_start_value: 0.0,
                    vol_entry,
                    sample_bpm_entry,
                    pitch_entry,
                    vol_fader_focused,
                    pan_knob_focused,
                    pitch_focused,
                    sample_bpm_focused,
                });
                return;
            }
        }
        self.right_window = None;
    }

    #[cfg(feature = "native")]
    fn sync_audio_clips(&self) {
        if let Some(engine) = &self.audio_engine {
            let mut positions: Vec<[f32; 2]> = Vec::new();
            let mut sizes: Vec<[f32; 2]> = Vec::new();
            let mut clips: Vec<&AudioClipData> = Vec::new();
            let mut fade_ins: Vec<f32> = Vec::new();
            let mut fade_outs: Vec<f32> = Vec::new();
            let mut fade_in_curves: Vec<f32> = Vec::new();
            let mut fade_out_curves: Vec<f32> = Vec::new();
            let mut volumes: Vec<f32> = Vec::new();
            let mut pans: Vec<f32> = Vec::new();
            let mut sample_offsets: Vec<f32> = Vec::new();
            let mut vol_autos: Vec<Vec<(f32, f32)>> = Vec::new();
            let mut pan_autos: Vec<Vec<(f32, f32)>> = Vec::new();
            let mut warp_modes: Vec<u8> = Vec::new();
            let mut sample_bpms: Vec<f32> = Vec::new();
            let mut pitch_semitones_vec: Vec<f32> = Vec::new();

            for (&wf_id, wf) in self.waveforms.iter() {
                if wf.disabled {
                    continue;
                }
                let clip = match self.audio_clips.get(&wf_id) {
                    Some(c) => c,
                    None => continue,
                };
                positions.push(wf.position);
                sizes.push(wf.size);
                clips.push(clip);
                fade_ins.push(wf.fade_in_px);
                fade_outs.push(wf.fade_out_px);
                fade_in_curves.push(wf.fade_in_curve);
                fade_out_curves.push(wf.fade_out_curve);
                volumes.push(wf.volume);
                pans.push(wf.pan);
                sample_offsets.push(wf.sample_offset_px);
                vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
                warp_modes.push(match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, ui::waveform::WarpMode::Semitone => 2, _ => 0 });
                sample_bpms.push(wf.sample_bpm);
                pitch_semitones_vec.push(wf.pitch_semitones);
            }

            // Add virtual clips for each component instance
            for inst in self.component_instances.values() {
                if let Some(def) = self.components.values().find(|c| c.id == inst.component_id) {
                    let offset = [
                        inst.position[0] - def.position[0],
                        inst.position[1] - def.position[1],
                    ];
                    for &wf_id in &def.waveform_ids {
                        if let (Some(wf), Some(clip)) = (self.waveforms.get(&wf_id), self.audio_clips.get(&wf_id)) {
                            if !wf.disabled {
                                positions.push([wf.position[0] + offset[0], wf.position[1] + offset[1]]);
                                sizes.push(wf.size);
                                clips.push(clip);
                                fade_ins.push(wf.fade_in_px);
                                fade_outs.push(wf.fade_out_px);
                                fade_in_curves.push(wf.fade_in_curve);
                                fade_out_curves.push(wf.fade_out_curve);
                                volumes.push(wf.volume);
                                pans.push(wf.pan);
                                sample_offsets.push(wf.sample_offset_px);
                                vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                                pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
                                warp_modes.push(match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, _ => 0 });
                                sample_bpms.push(wf.sample_bpm);
                            }
                        }
                    }
                }
            }

            let owned_clips: Vec<AudioClipData> = clips.iter().map(|c| (*c).clone()).collect();
            engine.update_clips(&positions, &sizes, &owned_clips, &fade_ins, &fade_outs, &fade_in_curves, &fade_out_curves, &volumes, &pans, &sample_offsets, &vol_autos, &pan_autos, &warp_modes, &sample_bpms, self.bpm, &pitch_semitones_vec);

            let regions: Vec<audio::AudioEffectRegion> = self
                .effect_regions
                .values()
                .map(|er| {
                    let block_ids = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                    audio::AudioEffectRegion {
                        x_start_px: er.position[0],
                        x_end_px: er.position[0] + er.size[0],
                        y_start: er.position[1],
                        y_end: er.position[1] + er.size[1],
                        plugins: block_ids
                            .iter()
                            .filter_map(|id| self.plugin_blocks.get(id))
                            .map(|pb| pb.gui.clone())
                            .collect(),
                    }
                })
                .collect();
            engine.update_effect_regions(regions);
        }
        self.sync_instrument_regions();
    }

    fn add_loop_area(&mut self) {
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(sa.position[0] + sa.size[0], &self.settings, self.camera.zoom, self.bpm);
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = LOOP_REGION_DEFAULT_WIDTH;
            let h = LOOP_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        let id = new_id();
        let data = LoopRegion { position: pos, size, enabled: true };
        self.loop_regions.insert(id, data.clone());
        self.push_op(operations::Operation::CreateLoopRegion { id, data });
        self.selected.clear();
        self.selected.push(HitTarget::LoopRegion(id));
        self.sync_loop_region();
        self.request_redraw();
    }

    fn add_effect_area(&mut self) {
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(
                sa.position[0] + sa.size[0],
                &self.settings,
                self.camera.zoom,
                self.bpm,
            );
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = effects::EFFECT_REGION_DEFAULT_WIDTH;
            let h = effects::EFFECT_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        let id = new_id();
        let er = effects::EffectRegion::new(pos, size);
        self.effect_regions.insert(id, er.clone());
        self.push_op(operations::Operation::CreateEffectRegion { id, data: er });
        self.selected.clear();
        self.selected.push(HitTarget::EffectRegion(id));
        self.request_redraw();
    }

    fn add_render_area(&mut self) {
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(
                sa.position[0] + sa.size[0],
                &self.settings,
                self.camera.zoom,
                self.bpm,
            );
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = EXPORT_REGION_DEFAULT_WIDTH;
            let h = EXPORT_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        let id = new_id();
        let data = ExportRegion { position: pos, size };
        self.export_regions.insert(id, data.clone());
        self.push_op(operations::Operation::CreateExportRegion { id, data });
        self.selected.clear();
        self.selected.push(HitTarget::ExportRegion(id));
        self.request_redraw();
    }

    #[cfg(test)]
    fn add_instrument_area(&mut self) {
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(
                sa.position[0] + sa.size[0],
                &self.settings,
                self.camera.zoom,
                self.bpm,
            );
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else {
            let (sw, sh, _) = self.screen_info();
            let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
            let w = instruments::INSTRUMENT_REGION_DEFAULT_WIDTH;
            let h = instruments::INSTRUMENT_REGION_DEFAULT_HEIGHT;
            ([center[0] - w * 0.5, center[1] - h * 0.5], [w, h])
        };
        let id = new_id();
        let ir = instruments::InstrumentRegion::new(pos, size);
        let snap = instruments::InstrumentRegionSnapshot {
            position: ir.position, size: ir.size,
            name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
            plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
        };
        self.instrument_regions.insert(id, ir);
        self.push_op(operations::Operation::CreateInstrumentRegion { id, data: snap });
        self.selected.clear();
        self.selected.push(HitTarget::InstrumentRegion(id));
        self.request_redraw();
    }

    fn add_midi_clip(&mut self) {
        let (sw, sh, _) = self.screen_info();
        let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
        let ppb = grid::pixels_per_beat(self.bpm);
        let beats_per_bar = 4.0;
        let width = ppb * beats_per_bar * midi::MIDI_CLIP_DEFAULT_BARS as f32;
        let height = midi::MIDI_CLIP_DEFAULT_HEIGHT;
        let pos = [center[0] - width * 0.5, center[1] - height * 0.5];
        let mut clip = midi::MidiClip::new(pos, &self.settings);
        clip.size = [width, height];
        clip.instrument_region_id = self.find_containing_instrument(pos, [width, height]);
        let id = new_id();
        self.midi_clips.insert(id, clip.clone());
        self.push_op(operations::Operation::CreateMidiClip { id, data: clip });
        self.selected.clear();
        self.selected.push(HitTarget::MidiClip(id));
        self.request_redraw();
    }

    /// Find the instrument region whose rectangle contains the given rect's center,
    /// falling back to the first instrument if none overlap.
    fn find_containing_instrument(&self, pos: [f32; 2], size: [f32; 2]) -> Option<EntityId> {
        if self.instrument_regions.is_empty() {
            return None;
        }
        let cx = pos[0] + size[0] * 0.5;
        let cy = pos[1] + size[1] * 0.5;
        for (&id, ir) in &self.instrument_regions {
            if cx >= ir.position[0] && cx <= ir.position[0] + ir.size[0]
                && cy >= ir.position[1] && cy <= ir.position[1] + ir.size[1]
            {
                return Some(id);
            }
        }
        Some(*self.instrument_regions.keys().next().unwrap())
    }

    #[cfg(feature = "native")]
    fn sync_instrument_regions(&self) {
        if let Some(engine) = &self.audio_engine {
            let mut instrument_regions = Vec::new();
            for (&id, ir) in self.instrument_regions.iter() {
                if !ir.has_plugin() {
                    continue;
                }
                let mut midi_events = Vec::new();
                for mc in self.midi_clips.values() {
                    let belongs = mc.instrument_region_id == Some(id)
                        || (mc.instrument_region_id.is_none() && rects_overlap(ir.position, ir.size, mc.position, mc.size));
                    if !belongs {
                        continue;
                    }
                    for note in &mc.notes {
                        let note_on_time = (mc.position[0] + note.start_px) as f64
                            / PIXELS_PER_SECOND as f64;
                        let note_off_time = note_on_time
                            + note.duration_px as f64 / PIXELS_PER_SECOND as f64;
                        midi_events.push(audio::TimedMidiEvent {
                            time_secs: note_on_time,
                            note: note.pitch,
                            velocity: note.velocity,
                            is_note_on: true,
                        });
                        midi_events.push(audio::TimedMidiEvent {
                            time_secs: note_off_time,
                            note: note.pitch,
                            velocity: 0,
                            is_note_on: false,
                        });
                    }
                }
                midi_events.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap());
                instrument_regions.push(audio::AudioInstrumentRegion {
                    id,
                    x_start_px: ir.position[0],
                    x_end_px: ir.position[0] + ir.size[0],
                    y_start: ir.position[1],
                    y_end: ir.position[1] + ir.size[1],
                    gui: ir.gui.clone(),
                    midi_events,
                });
            }
            engine.update_instrument_regions(instrument_regions);
        }
        self.sync_computer_keyboard_to_engine();
    }

    #[cfg(feature = "native")]
    pub(crate) fn sync_keyboard_instrument_from_selection(&mut self) {
        let irs: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::InstrumentRegion(id) => Some(*id),
                _ => None,
            })
            .collect();
        self.keyboard_instrument_id = if irs.len() == 1 && self.instrument_regions.contains_key(&irs[0]) {
            Some(irs[0])
        } else {
            None
        };
    }

    #[cfg(feature = "native")]
    pub(crate) fn sync_computer_keyboard_to_engine(&self) {
        let Some(engine) = &self.audio_engine else {
            return;
        };
        if !self.computer_keyboard_armed {
            engine.set_keyboard_preview_target(None);
            return;
        }
        let Some(id) = self.keyboard_instrument_id else {
            engine.set_keyboard_preview_target(None);
            return;
        };
        if self
            .instrument_regions
            .get(&id)
            .map_or(false, |ir| ir.has_plugin())
        {
            engine.set_keyboard_preview_target(Some(id));
        } else {
            engine.set_keyboard_preview_target(None);
        }
    }

    #[cfg(feature = "native")]
    fn sync_loop_region(&self) {
        if let Some(engine) = &self.audio_engine {
            let regions: Vec<(f64, f64)> = self
                .loop_regions
                .iter()
                .filter(|(_, lr)| lr.enabled)
                .map(|(_, lr)| {
                    let start = lr.position[0] as f64 / audio::PIXELS_PER_SECOND as f64;
                    let end = (lr.position[0] + lr.size[0]) as f64 / audio::PIXELS_PER_SECOND as f64;
                    (start, end)
                })
                .collect();
            if let Some(&(start, end)) = regions.first() {
                engine.set_loop_region(start, end);
                engine.set_loop_enabled(true);
            } else {
                engine.set_loop_enabled(false);
            }
        }
    }

    #[cfg(feature = "native")]
    fn toggle_recording(&mut self) {
        if self.recorder.is_none() {
            return;
        }

        let is_rec = self.recorder.as_ref().unwrap().is_recording();

        if is_rec {
            let loaded = self.recorder.as_mut().unwrap().stop();
            if let Some(loaded) = loaded {
                if let Some(wf_id) = self.recording_waveform_id.take() {
                    if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                        let filename = wf.audio.filename.clone();
                        wf.size[0] = loaded.width;
                        wf.audio = Arc::new(AudioData {
                            left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                            right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                            left_samples: loaded.left_samples.clone(),
                            right_samples: loaded.right_samples.clone(),
                            sample_rate: loaded.sample_rate,
                            filename,
                        });
                    }
                    if let Some(clip) = self.audio_clips.get_mut(&wf_id) {
                        *clip = AudioClipData {
                            samples: loaded.samples.clone(),
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        };
                    }
                    if let Some(rs) = &self.remote_storage {
                        let wf_id_str = wf_id.to_string();
                        // Encode recorded PCM as WAV bytes for remote storage
                        let wav_bytes = audio::encode_wav_bytes(
                            &loaded.left_samples,
                            &loaded.right_samples,
                            loaded.sample_rate,
                        );
                        rs.save_audio(&wf_id_str, &wav_bytes, "wav");
                    }
                    self.sync_audio_clips();
                }
            } else {
                if let Some(wf_id) = self.recording_waveform_id.take() {
                    self.waveforms.shift_remove(&wf_id);
                    self.audio_clips.shift_remove(&wf_id);
                }
            }

            if let Some(engine) = &self.audio_engine {
                if engine.is_playing() {
                    engine.toggle_playback();
                }
            }
        } else {
            let world = self.last_canvas_click_world;
            let height = grid::clip_height(self.bpm);
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let sample_rate = self.recorder.as_ref().unwrap().sample_rate();

            let wf_id = new_id();
            let wf_data = WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate,
                    filename: "Recording".to_string(),
                }),
                filename: "Recording".to_string(),
                position: [world[0], world[1] - height * 0.5],
                size: [0.0, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                fade_in_px: if self.settings.auto_clip_fades { ui::waveform::DEFAULT_AUTO_FADE_PX } else { 0.0 },
                fade_out_px: if self.settings.auto_clip_fades { ui::waveform::DEFAULT_AUTO_FADE_PX } else { 0.0 },
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                pan: 0.5,
                warp_mode: ui::waveform::WarpMode::Off,
                sample_bpm: self.bpm,
                pitch_semitones: 0.0,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            };
            let ac_data = AudioClipData {
                samples: Arc::new(Vec::new()),
                sample_rate,
                duration_secs: 0.0,
            };
            self.waveforms.insert(wf_id, wf_data.clone());
            self.audio_clips.insert(wf_id, ac_data.clone());
            self.push_op(operations::Operation::CreateWaveform { id: wf_id, data: wf_data, audio_clip: Some((wf_id, ac_data)) });
            self.recording_waveform_id = Some(wf_id);
            self.recorder.as_mut().unwrap().start();

            if let Some(engine) = &self.audio_engine {
                let secs = world[0] as f64 / PIXELS_PER_SECOND as f64;
                engine.seek_to_seconds(secs);
                if !engine.is_playing() {
                    engine.toggle_playback();
                }
            }
        }
    }

    #[cfg(feature = "native")]
    fn update_recording_waveform(&mut self) {
        let wf_id = match self.recording_waveform_id {
            Some(id) => id,
            None => return,
        };
        let snapshot = self.recorder.as_ref().and_then(|r| r.current_snapshot());
        if let Some(loaded) = snapshot {
            if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                let filename = wf.audio.filename.clone();
                wf.size[0] = loaded.width;
                wf.audio = Arc::new(AudioData {
                    left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                    right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                    left_samples: loaded.left_samples,
                    right_samples: loaded.right_samples,
                    sample_rate: loaded.sample_rate,
                    filename,
                });
                self.mark_dirty();
            }
        }
    }

    #[cfg(feature = "native")]
    fn is_recording(&self) -> bool {
        self.recorder
            .as_ref()
            .map(|r| r.is_recording())
            .unwrap_or(false)
    }
    #[cfg(not(feature = "native"))]
    fn is_recording(&self) -> bool {
        false
    }

    // No-op stubs for web builds — these methods are native-only but called from many places
    #[cfg(not(feature = "native"))]
    fn sync_audio_clips(&self) {}
    #[cfg(not(feature = "native"))]
    fn sync_loop_region(&self) {}
    #[cfg(not(feature = "native"))]
    fn sync_instrument_regions(&self) {}
    #[cfg(not(feature = "native"))]
    fn save_project_state(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn save_project(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn toggle_recording(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn update_recording_waveform(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn drop_audio_from_browser(&mut self, _path: &std::path::Path) {}
    #[cfg(not(feature = "native"))]
    fn poll_pending_audio_loads(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn ensure_plugins_scanned(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn open_add_folder_dialog(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn save_browser_folders_to_settings(&self) {}
    #[cfg(not(feature = "native"))]
    fn add_plugin_to_selected_effect_region(&mut self, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn add_instrument(&mut self, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn add_plugin_block(&mut self, _position: [f32; 2], _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn build_palette_plugin_entries(&self) -> Vec<PluginPickerEntry> { Vec::new() }
    #[cfg(not(feature = "native"))]
    fn open_plugin_block_gui(&mut self, _id: EntityId) {}
    #[cfg(not(feature = "native"))]
    fn open_instrument_region_gui(&mut self, _id: EntityId) {}
    #[cfg(not(feature = "native"))]
    fn shutdown_plugins(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn new_project(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn load_project(&mut self, _project_path: &str) {}
    #[cfg(not(feature = "native"))]
    fn refresh_open_project_menu(&mut self) {}
    #[cfg(not(feature = "native"))]
    fn trigger_export_render(&mut self) {}

    #[cfg(feature = "native")]
    fn trigger_export_render(&mut self) {
        let er = match self.export_regions.values().next() {
            Some(er) => er,
            None => return,
        };

        let start_secs = er.position[0] as f64 / audio::PIXELS_PER_SECOND as f64;
        let end_secs = (er.position[0] + er.size[0]) as f64 / audio::PIXELS_PER_SECOND as f64;
        let y_start = er.position[1];
        let y_end = er.position[1] + er.size[1];

        if end_secs <= start_secs {
            println!("  Export region has zero or negative duration");
            return;
        }

        let path = rfd::FileDialog::new()
            .set_file_name("export.wav")
            .add_filter("WAV", &["wav"])
            .save_file();

        let path = match path {
            Some(p) => p,
            None => return,
        };

        let clips: Vec<audio::ExportClip> = self
            .waveforms
            .iter()
            .filter_map(|(wf_id, wf)| {
                if wf.disabled { return None; }
                self.audio_clips.get(wf_id).map(|clip| (wf, clip))
            })
            .map(|(wf, clip)| audio::ExportClip {
                buffer: clip.samples.clone(),
                source_sample_rate: clip.sample_rate,
                start_time_secs: wf.position[0] as f64 / audio::PIXELS_PER_SECOND as f64,
                duration_secs: wf.size[0] as f64 / audio::PIXELS_PER_SECOND as f64,
                position_y: wf.position[1],
                height: wf.size[1],
                fade_in_secs: (wf.fade_in_px / audio::PIXELS_PER_SECOND) as f64,
                fade_out_secs: (wf.fade_out_px / audio::PIXELS_PER_SECOND) as f64,
                fade_in_curve: wf.fade_in_curve,
                fade_out_curve: wf.fade_out_curve,
                volume: wf.volume,
                buffer_offset_secs: wf.sample_offset_px as f64 / audio::PIXELS_PER_SECOND as f64,
                warp_mode: match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, ui::waveform::WarpMode::Semitone => 2, _ => 0 },
                sample_bpm: wf.sample_bpm,
                project_bpm: self.bpm,
                pitch_semitones: wf.pitch_semitones,
            })
            .collect();

        let effect_regions: Vec<audio::AudioEffectRegion> = self
            .effect_regions
            .values()
            .map(|er| {
                let block_ids = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                audio::AudioEffectRegion {
                    x_start_px: er.position[0],
                    x_end_px: er.position[0] + er.size[0],
                    y_start: er.position[1],
                    y_end: er.position[1] + er.size[1],
                    plugins: block_ids
                        .iter()
                        .filter_map(|id| self.plugin_blocks.get(id))
                        .map(|pb| pb.gui.clone())
                        .collect(),
                }
            })
            .collect();

        match audio::render_to_wav(
            &path,
            start_secs,
            end_secs,
            y_start,
            y_end,
            &clips,
            &effect_regions,
        ) {
            Ok(()) => println!("  Exported WAV to {}", path.display()),
            Err(e) => println!("  Export failed: {}", e),
        }
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        } else if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_cursor(&self) {
        if let Some(gpu) = &self.gpu {
            let icon =
                match &self.drag {
                    DragState::Panning { .. } => CursorIcon::Grabbing,
                    DragState::MovingSelection { .. } => CursorIcon::Grabbing,
                    DragState::Selecting { .. } => CursorIcon::Default,
                    DragState::DraggingFromBrowser { .. } => CursorIcon::Grabbing,
                    DragState::DraggingPlugin { .. } => CursorIcon::Grabbing,
                    DragState::ResizingBrowser => CursorIcon::EwResize,
                    DragState::ResizingExportRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::DraggingFade { .. } => CursorIcon::EwResize,
                    DragState::ResizingWaveform { .. } => CursorIcon::EwResize,
                    DragState::DraggingFadeCurve { .. } => CursorIcon::NsResize,
                    DragState::DraggingAutomationPoint { .. } => CursorIcon::Grabbing,
                    DragState::MovingMidiNote { .. } => CursorIcon::Default,
                    DragState::ResizingMidiNote { .. } => CursorIcon::EwResize,
                    DragState::ResizingMidiNoteLeft { .. } => CursorIcon::EwResize,
                    DragState::ResizingMidiClip { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::MovingMidiClip { .. } => CursorIcon::Grabbing,
                    DragState::SelectingMidiNotes { .. } => CursorIcon::Default,
                    DragState::DraggingVelocity { .. } => CursorIcon::NsResize,
                    DragState::ResizingVelocityLane { .. } => CursorIcon::NsResize,
                    DragState::ResizingComponentDef { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::ResizingEffectRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::ResizingLoopRegion { nwse, .. } => {
                        if *nwse {
                            CursorIcon::NwseResize
                        } else {
                            CursorIcon::NeswResize
                        }
                    }
                    DragState::None => {
                        if self.cmd_velocity_hover_note.is_some() {
                            CursorIcon::NsResize
                        } else if self.sample_browser.visible && self.sample_browser.resize_hovered {
                            CursorIcon::EwResize
                        } else if self.waveform_edge_hover != WaveformEdgeHover::None {
                            CursorIcon::EwResize
                        } else if self.midi_note_edge_hover {
                            CursorIcon::EwResize
                        } else if self.velocity_divider_hovered {
                            CursorIcon::NsResize
                        } else if self.velocity_bar_hovered {
                            CursorIcon::NsResize
                        } else if self.fade_handle_hovered.is_some() {
                            CursorIcon::EwResize
                        } else if self.fade_curve_hovered.is_some() {
                            CursorIcon::NsResize
                        } else if self.command_palette.is_some() {
                            CursorIcon::Default
                        } else if {
                            let (sw, sh, sc) = self.screen_info();
                            TransportPanel::hit_bpm(self.mouse_pos, sw, sh, sc)
                        } {
                            CursorIcon::Default
                        } else {
                            match self.component_def_hover {
                                ComponentDefHover::CornerNW(_) | ComponentDefHover::CornerSE(_) => {
                                    CursorIcon::NwseResize
                                }
                                ComponentDefHover::CornerNE(_) | ComponentDefHover::CornerSW(_) => {
                                    CursorIcon::NeswResize
                                }
                                ComponentDefHover::None => match self.instrument_region_hover {
                                    InstrumentRegionHover::CornerNW(_)
                                    | InstrumentRegionHover::CornerSE(_) => CursorIcon::NwseResize,
                                    InstrumentRegionHover::CornerNE(_)
                                    | InstrumentRegionHover::CornerSW(_) => CursorIcon::NeswResize,
                                    InstrumentRegionHover::None => match self.effect_region_hover {
                                    EffectRegionHover::CornerNW(_)
                                    | EffectRegionHover::CornerSE(_) => CursorIcon::NwseResize,
                                    EffectRegionHover::CornerNE(_)
                                    | EffectRegionHover::CornerSW(_) => CursorIcon::NeswResize,
                                    EffectRegionHover::None => match self.export_hover {
                                        ExportHover::CornerNW(_) | ExportHover::CornerSE(_) => {
                                            CursorIcon::NwseResize
                                        }
                                        ExportHover::CornerNE(_) | ExportHover::CornerSW(_) => {
                                            CursorIcon::NeswResize
                                        }
                                        ExportHover::RenderPill(_) => CursorIcon::Pointer,
                                        ExportHover::None => match self.loop_hover {
                                            LoopHover::CornerNW(_) | LoopHover::CornerSE(_) => {
                                                CursorIcon::NwseResize
                                            }
                                            LoopHover::CornerNE(_) | LoopHover::CornerSW(_) => {
                                                CursorIcon::NeswResize
                                            }
                                            LoopHover::None => {
                                                if matches!(self.hovered, Some(HitTarget::MidiClip(i)) if self.editing_midi_clip == Some(i)) {
                                                    CursorIcon::Default
                                                } else if self.hovered.is_some() {
                                                    CursorIcon::Grab
                                                } else {
                                                    CursorIcon::Default
                                                }
                                            }
                                        },
                                    },
                                },
                                },
                            }
                        }
                    }
                    _ => CursorIcon::Default,
                };
            gpu.window.set_cursor(icon);
        }
    }

    fn update_hover(&mut self) {
        let (sw, sh, scale) = self.screen_info();
        if let Some(palette) = &mut self.command_palette {
            if let Some(idx) = palette.item_at(self.mouse_pos, sw, sh, scale) {
                if matches!(palette.mode, PaletteMode::PluginPicker | PaletteMode::InstrumentPicker) {
                    palette.plugin_selected_index = idx;
                } else {
                    palette.selected_index = idx;
                }
            }
        }
        let world = self.camera.screen_to_world(self.mouse_pos);
        self.fade_handle_hovered = hit_test_fade_handle(&self.waveforms, world, &self.camera);
        self.waveform_edge_hover = if self.fade_handle_hovered.is_none() {
            hit_test_waveform_edge(&self.waveforms, world, &self.camera)
        } else {
            WaveformEdgeHover::None
        };
        self.midi_note_edge_hover = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                matches!(
                    midi::hit_test_midi_note_editing(mc, world, &self.camera, true),
                    Some((_, midi::MidiNoteHitZone::RightEdge | midi::MidiNoteHitZone::LeftEdge))
                )
            } else {
                false
            }
        } else {
            false
        };
        self.velocity_divider_hovered = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                midi::hit_test_velocity_divider(mc, world, &self.camera)
            } else {
                false
            }
        } else {
            false
        };
        self.velocity_bar_hovered = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                !self.velocity_divider_hovered && midi::hit_test_velocity_bar(mc, world, &self.camera).is_some()
            } else {
                false
            }
        } else {
            false
        };
        self.cmd_velocity_hover_note = if self.cmd_held() {
            if let Some(mc_id) = self.editing_midi_clip {
                if let Some(mc) = self.midi_clips.get(&mc_id) {
                    match midi::hit_test_midi_note_editing(mc, world, &self.camera, true) {
                        Some((note_idx, midi::MidiNoteHitZone::Body | midi::MidiNoteHitZone::LeftEdge | midi::MidiNoteHitZone::RightEdge)) => {
                            Some((mc_id, note_idx))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        self.fade_curve_hovered = if self.fade_handle_hovered.is_none() && self.waveform_edge_hover == WaveformEdgeHover::None {
            hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
        } else {
            None
        };
        self.hovered = hit_test(
            &self.objects,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
            &self.loop_regions,
            &self.export_regions,
            &self.components,
            &self.component_instances,
            &self.midi_clips,
            &self.instrument_regions,
            self.editing_component,
            world,
            &self.camera,
        );

        self.component_def_hover = ComponentDefHover::None;
        for (&ci, def) in self.components.iter() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = def.position;
            let s = def.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerNW(ci);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerNE(ci);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.component_def_hover = ComponentDefHover::CornerSW(ci);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.component_def_hover = ComponentDefHover::CornerSE(ci);
                break;
            }
        }

        self.instrument_region_hover = InstrumentRegionHover::None;
        for (&i, ir) in self.instrument_regions.iter() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = ir.position;
            let s = ir.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.instrument_region_hover = InstrumentRegionHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.instrument_region_hover = InstrumentRegionHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.instrument_region_hover = InstrumentRegionHover::CornerSW(i);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.instrument_region_hover = InstrumentRegionHover::CornerSE(i);
                break;
            }
        }

        self.effect_region_hover = EffectRegionHover::None;
        for (&i, er) in self.effect_regions.iter() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = er.position;
            let s = er.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.effect_region_hover = EffectRegionHover::CornerSW(i);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.effect_region_hover = EffectRegionHover::CornerSE(i);
                break;
            }
        }

        self.export_hover = ExportHover::None;
        for (&i, er) in self.export_regions.iter() {
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = er.position;
            let s = er.size;

            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.export_hover = ExportHover::CornerSW(i);
                break;
            } else if point_in_rect(
                world,
                [p[0] + s[0] - hs, p[1] + s[1] - hs],
                [handle_sz, handle_sz],
            ) {
                self.export_hover = ExportHover::CornerSE(i);
                break;
            } else {
                let pill_w = EXPORT_RENDER_PILL_W / self.camera.zoom;
                let pill_h = EXPORT_RENDER_PILL_H / self.camera.zoom;
                let pill_x = p[0] + 4.0 / self.camera.zoom;
                let pill_y = p[1] + 4.0 / self.camera.zoom;
                if point_in_rect(world, [pill_x, pill_y], [pill_w, pill_h]) {
                    self.export_hover = ExportHover::RenderPill(i);
                    break;
                }
            }
        }

        self.loop_hover = LoopHover::None;
        for (&i, lr) in self.loop_regions.iter() {
            if !lr.enabled {
                continue;
            }
            let handle_sz = 24.0 / self.camera.zoom;
            let hs = handle_sz * 0.5;
            let p = lr.position;
            let s = lr.size;
            if point_in_rect(world, [p[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerNW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerNE(i);
                break;
            } else if point_in_rect(world, [p[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerSW(i);
                break;
            } else if point_in_rect(world, [p[0] + s[0] - hs, p[1] + s[1] - hs], [handle_sz, handle_sz]) {
                self.loop_hover = LoopHover::CornerSE(i);
                break;
            }
        }

        self.update_cursor();
    }

    fn screen_info(&self) -> (f32, f32, f32) {
        match &self.gpu {
            Some(g) => (
                g.config.width as f32,
                g.config.height as f32,
                g.scale_factor,
            ),
            None => (1280.0, 800.0, 1.0),
        }
    }

    fn set_target_pos(&mut self, target: &HitTarget, pos: [f32; 2]) {
        match target {
            HitTarget::Object(i) => { if let Some(o) = self.objects.get_mut(i) { o.position = pos; } }
            HitTarget::Waveform(i) => { if let Some(w) = self.waveforms.get_mut(i) { w.position = pos; } }
            HitTarget::EffectRegion(i) => { if let Some(e) = self.effect_regions.get_mut(i) { e.position = pos; } }
            HitTarget::PluginBlock(i) => { if let Some(p) = self.plugin_blocks.get_mut(i) { p.position = pos; } }
            HitTarget::LoopRegion(i) => { if let Some(l) = self.loop_regions.get_mut(i) { l.position = pos; } }
            HitTarget::ExportRegion(i) => { if let Some(e) = self.export_regions.get_mut(i) { e.position = pos; } }
            HitTarget::ComponentDef(i) => {
                let wf_ids_and_delta = if let Some(comp) = self.components.get(i) {
                    let dx = pos[0] - comp.position[0];
                    let dy = pos[1] - comp.position[1];
                    Some((comp.waveform_ids.clone(), dx, dy))
                } else {
                    None
                };
                if let Some((wf_ids, dx, dy)) = wf_ids_and_delta {
                    if let Some(comp) = self.components.get_mut(i) {
                        comp.position = pos;
                    }
                    for wf_id in &wf_ids {
                        if let Some(wf) = self.waveforms.get_mut(wf_id) {
                            wf.position[0] += dx;
                            wf.position[1] += dy;
                        }
                    }
                }
            }
            HitTarget::ComponentInstance(i) => { if let Some(c) = self.component_instances.get_mut(i) { c.position = pos; } }
            HitTarget::MidiClip(i) => { if let Some(m) = self.midi_clips.get_mut(i) { m.position = pos; } }
            HitTarget::InstrumentRegion(i) => { if let Some(r) = self.instrument_regions.get_mut(i) { r.position = pos; } }
        }
    }

    fn get_target_pos(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects.get(i).map(|o| o.position).unwrap_or([0.0; 2]),
            HitTarget::Waveform(i) => self.waveforms.get(i).map(|w| w.position).unwrap_or([0.0; 2]),
            HitTarget::EffectRegion(i) => self.effect_regions.get(i).map(|e| e.position).unwrap_or([0.0; 2]),
            HitTarget::PluginBlock(i) => self.plugin_blocks.get(i).map(|p| p.position).unwrap_or([0.0; 2]),
            HitTarget::LoopRegion(i) => self.loop_regions.get(i).map(|l| l.position).unwrap_or([0.0; 2]),
            HitTarget::ExportRegion(i) => self.export_regions.get(i).map(|e| e.position).unwrap_or([0.0; 2]),
            HitTarget::ComponentDef(i) => self.components.get(i).map(|c| c.position).unwrap_or([0.0; 2]),
            HitTarget::ComponentInstance(i) => self.component_instances.get(i).map(|c| c.position).unwrap_or([0.0; 2]),
            HitTarget::MidiClip(i) => self.midi_clips.get(i).map(|m| m.position).unwrap_or([0.0; 2]),
            HitTarget::InstrumentRegion(i) => self.instrument_regions.get(i).map(|r| r.position).unwrap_or([0.0; 2]),
        }
    }

    fn get_target_size(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects.get(i).map(|o| o.size).unwrap_or([50.0; 2]),
            HitTarget::Waveform(i) => self.waveforms.get(i).map(|w| w.size).unwrap_or([50.0; 2]),
            HitTarget::EffectRegion(i) => self.effect_regions.get(i).map(|e| e.size).unwrap_or([50.0; 2]),
            HitTarget::PluginBlock(i) => self.plugin_blocks.get(i).map(|p| p.size).unwrap_or([50.0; 2]),
            HitTarget::LoopRegion(i) => self.loop_regions.get(i).map(|l| l.size).unwrap_or([50.0; 2]),
            HitTarget::ExportRegion(i) => self.export_regions.get(i).map(|e| e.size).unwrap_or([50.0; 2]),
            HitTarget::ComponentDef(i) => self.components.get(i).map(|c| c.size).unwrap_or([50.0; 2]),
            HitTarget::ComponentInstance(i) => {
                self.component_instances.get(i)
                    .and_then(|ci| self.components.get(&ci.component_id))
                    .map(|c| c.size)
                    .unwrap_or([50.0; 2])
            }
            HitTarget::MidiClip(i) => self.midi_clips.get(i).map(|m| m.size).unwrap_or([50.0; 2]),
            HitTarget::InstrumentRegion(i) => self.instrument_regions.get(i).map(|r| r.size).unwrap_or([50.0; 2]),
        }
    }

    /// Broadcast a drag preview to remote users (not throttled — called alongside cursor broadcast).
    fn broadcast_drag_preview(&self, preview: crate::user::DragPreview) {
        if self.network.is_connected() {
            self.network.send_ephemeral(crate::user::EphemeralMessage::DragUpdate {
                user_id: self.local_user.id,
                preview,
            });
        }
    }

    /// Broadcast drag end to remote users.
    fn broadcast_drag_end(&self) {
        if self.network.is_connected() {
            self.network.send_ephemeral(crate::user::EphemeralMessage::DragEnd {
                user_id: self.local_user.id,
            });
        }
    }

    /// Returns true when the platform's primary shortcut modifier is held.
    /// Cmd on macOS, Ctrl on Windows/Linux.
    fn cmd_held(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.modifiers.super_key()
        } else {
            self.modifiers.control_key()
        }
    }

    fn is_snap_override_active(&self) -> bool {
        self.cmd_held()
    }

    pub(crate) fn begin_move_selection(&mut self, world: [f32; 2], alt_copy: bool, clicked_target: Option<HitTarget>) {
        if alt_copy {
            let mut new_selected: Vec<HitTarget> = Vec::new();
            let mut copy_ops: Vec<operations::Operation> = Vec::new();
            for target in self.selected.clone() {
                match target {
                    HitTarget::Waveform(i) => {
                        if let Some(wf) = self.waveforms.get(&i).cloned() {
                            let nid = new_id();
                            let ac = self.audio_clips.get(&i).cloned();
                            self.waveforms.insert(nid, wf.clone());
                            if let Some(clip) = &ac {
                                self.audio_clips.insert(nid, clip.clone());
                            }
                            copy_ops.push(operations::Operation::CreateWaveform { id: nid, data: wf, audio_clip: ac.map(|c| (nid, c)) });
                            new_selected.push(HitTarget::Waveform(nid));
                        }
                    }
                    HitTarget::Object(i) => {
                        if let Some(obj) = self.objects.get(&i).cloned() {
                            let nid = new_id();
                            self.objects.insert(nid, obj.clone());
                            copy_ops.push(operations::Operation::CreateObject { id: nid, data: obj });
                            new_selected.push(HitTarget::Object(nid));
                        }
                    }
                    HitTarget::EffectRegion(i) => {
                        if let Some(er) = self.effect_regions.get(&i).cloned() {
                            let nid = new_id();
                            self.effect_regions.insert(nid, er.clone());
                            copy_ops.push(operations::Operation::CreateEffectRegion { id: nid, data: er });
                            new_selected.push(HitTarget::EffectRegion(nid));
                        }
                    }
                    HitTarget::PluginBlock(i) => {
                        if let Some(pb) = self.plugin_blocks.get(&i).cloned() {
                            let nid = new_id();
                            let snap = pb.snapshot();
                            self.plugin_blocks.insert(nid, pb);
                            copy_ops.push(operations::Operation::CreatePluginBlock { id: nid, data: snap });
                            new_selected.push(HitTarget::PluginBlock(nid));
                        }
                    }
                    HitTarget::LoopRegion(i) => {
                        if let Some(lr) = self.loop_regions.get(&i).cloned() {
                            let nid = new_id();
                            self.loop_regions.insert(nid, lr.clone());
                            copy_ops.push(operations::Operation::CreateLoopRegion { id: nid, data: lr });
                            new_selected.push(HitTarget::LoopRegion(nid));
                        }
                    }
                    HitTarget::ExportRegion(i) => {
                        if let Some(xr) = self.export_regions.get(&i).cloned() {
                            let nid = new_id();
                            self.export_regions.insert(nid, xr.clone());
                            copy_ops.push(operations::Operation::CreateExportRegion { id: nid, data: xr });
                            new_selected.push(HitTarget::ExportRegion(nid));
                        }
                    }
                    HitTarget::ComponentInstance(i) => {
                        if let Some(inst) = self.component_instances.get(&i).cloned() {
                            let nid = new_id();
                            self.component_instances.insert(nid, inst.clone());
                            copy_ops.push(operations::Operation::CreateComponentInstance { id: nid, data: inst });
                            new_selected.push(HitTarget::ComponentInstance(nid));
                        }
                    }
                    HitTarget::MidiClip(i) => {
                        if let Some(mc) = self.midi_clips.get(&i).cloned() {
                            let nid = new_id();
                            self.midi_clips.insert(nid, mc.clone());
                            copy_ops.push(operations::Operation::CreateMidiClip { id: nid, data: mc });
                            new_selected.push(HitTarget::MidiClip(nid));
                        }
                    }
                    HitTarget::InstrumentRegion(i) => {
                        if let Some(ir) = self.instrument_regions.get(&i).cloned() {
                            let nid = new_id();
                            let snap = instruments::InstrumentRegionSnapshot {
                                position: ir.position, size: ir.size,
                                name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                            };
                            self.instrument_regions.insert(nid, ir);
                            copy_ops.push(operations::Operation::CreateInstrumentRegion { id: nid, data: snap });
                            new_selected.push(HitTarget::InstrumentRegion(nid));
                        }
                    }
                    HitTarget::ComponentDef(i) => {
                        if let Some(src) = self.components.get(&i).cloned() {
                            let comp_nid = new_id();
                            self.next_component_id = new_id();
                            let src_wf_ids = src.waveform_ids.clone();
                            let mut new_wf_ids = Vec::new();
                            for &wi in &src_wf_ids {
                                if let Some(wf) = self.waveforms.get(&wi).cloned() {
                                    let wf_nid = new_id();
                                    let ac = self.audio_clips.get(&wi).cloned();
                                    self.waveforms.insert(wf_nid, wf.clone());
                                    new_wf_ids.push(wf_nid);
                                    if let Some(clip) = &ac {
                                        self.audio_clips.insert(wf_nid, clip.clone());
                                    }
                                    copy_ops.push(operations::Operation::CreateWaveform { id: wf_nid, data: wf, audio_clip: ac.map(|c| (wf_nid, c)) });
                                }
                            }
                            let def = component::ComponentDef {
                                id: comp_nid,
                                name: format!("{} copy", src.name),
                                position: src.position,
                                size: src.size,
                                waveform_ids: new_wf_ids,
                            };
                            self.components.insert(comp_nid, def.clone());
                            copy_ops.push(operations::Operation::CreateComponent { id: comp_nid, data: def });
                            new_selected.push(HitTarget::ComponentDef(comp_nid));
                        }
                    }
                }
            }
            self.selected = new_selected;
            if !copy_ops.is_empty() {
                self.push_op(operations::Operation::Batch(copy_ops));
            }
        }

        // Capture before states for all selected entities
        let before_states: Vec<(HitTarget, EntityBeforeState)> = self.selected.iter().filter_map(|t| {
            match t {
                HitTarget::Object(id) => self.objects.get(id).map(|o| (*t, EntityBeforeState::Object(o.clone()))),
                HitTarget::Waveform(id) => self.waveforms.get(id).map(|w| (*t, EntityBeforeState::Waveform(w.clone()))),
                HitTarget::EffectRegion(id) => self.effect_regions.get(id).map(|e| (*t, EntityBeforeState::EffectRegion(e.clone()))),
                HitTarget::PluginBlock(id) => self.plugin_blocks.get(id).map(|p| (*t, EntityBeforeState::PluginBlock(p.snapshot()))),
                HitTarget::LoopRegion(id) => self.loop_regions.get(id).map(|l| (*t, EntityBeforeState::LoopRegion(l.clone()))),
                HitTarget::ExportRegion(id) => self.export_regions.get(id).map(|x| (*t, EntityBeforeState::ExportRegion(x.clone()))),
                HitTarget::ComponentDef(id) => self.components.get(id).map(|c| (*t, EntityBeforeState::ComponentDef(c.clone()))),
                HitTarget::ComponentInstance(id) => self.component_instances.get(id).map(|c| (*t, EntityBeforeState::ComponentInstance(c.clone()))),
                HitTarget::MidiClip(id) => self.midi_clips.get(id).map(|m| (*t, EntityBeforeState::MidiClip(m.clone()))),
                HitTarget::InstrumentRegion(id) => self.instrument_regions.get(id).map(|r| {
                    let snap = instruments::InstrumentRegionSnapshot {
                        position: r.position, size: r.size,
                        name: r.name.clone(), plugin_id: r.plugin_id.clone(),
                        plugin_name: r.plugin_name.clone(), plugin_path: r.plugin_path.clone(),
                    };
                    (*t, EntityBeforeState::InstrumentRegion(snap))
                }),
            }
        }).collect();

        let offsets: Vec<(HitTarget, [f32; 2])> = self
            .selected
            .iter()
            .map(|t| {
                let pos = self.get_target_pos(t);
                (*t, [world[0] - pos[0], world[1] - pos[1]])
            })
            .collect();
        let anchor_idx = clicked_target
            .and_then(|ct| offsets.iter().position(|(t, _)| *t == ct))
            .unwrap_or(0);
        self.drag = DragState::MovingSelection { offsets, anchor_idx, before_states, overlap_snapshots: IndexMap::new(), overlap_temp_splits: Vec::new() };
    }

    /// Flush any pending coalesced arrow-nudge into the undo stack.
    pub(crate) fn commit_arrow_nudge(&mut self) {
        if let Some(before_states) = self.arrow_nudge_before.take() {
            let mut ops = Vec::new();
            for (target, bs) in before_states {
                match (target, bs) {
                    (HitTarget::Object(id), EntityBeforeState::Object(before)) => {
                        if let Some(after) = self.objects.get(&id) {
                            ops.push(crate::operations::Operation::UpdateObject { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::Waveform(id), EntityBeforeState::Waveform(before)) => {
                        if let Some(after) = self.waveforms.get(&id) {
                            ops.push(crate::operations::Operation::UpdateWaveform { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::EffectRegion(id), EntityBeforeState::EffectRegion(before)) => {
                        if let Some(after) = self.effect_regions.get(&id) {
                            ops.push(crate::operations::Operation::UpdateEffectRegion { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::PluginBlock(id), EntityBeforeState::PluginBlock(before)) => {
                        if let Some(after) = self.plugin_blocks.get(&id) {
                            ops.push(crate::operations::Operation::DeletePluginBlock { id, data: before });
                            ops.push(crate::operations::Operation::CreatePluginBlock { id, data: after.snapshot() });
                        }
                    }
                    (HitTarget::LoopRegion(id), EntityBeforeState::LoopRegion(before)) => {
                        if let Some(after) = self.loop_regions.get(&id) {
                            ops.push(crate::operations::Operation::UpdateLoopRegion { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::ExportRegion(id), EntityBeforeState::ExportRegion(before)) => {
                        if let Some(after) = self.export_regions.get(&id) {
                            ops.push(crate::operations::Operation::UpdateExportRegion { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::ComponentDef(id), EntityBeforeState::ComponentDef(before)) => {
                        if let Some(after) = self.components.get(&id) {
                            ops.push(crate::operations::Operation::UpdateComponent { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::ComponentInstance(id), EntityBeforeState::ComponentInstance(before)) => {
                        if let Some(after) = self.component_instances.get(&id) {
                            ops.push(crate::operations::Operation::UpdateComponentInstance { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::MidiClip(id), EntityBeforeState::MidiClip(before)) => {
                        if let Some(after) = self.midi_clips.get(&id) {
                            ops.push(crate::operations::Operation::UpdateMidiClip { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::InstrumentRegion(id), EntityBeforeState::InstrumentRegion(before)) => {
                        if let Some(ir) = self.instrument_regions.get(&id) {
                            let after = crate::instruments::InstrumentRegionSnapshot {
                                position: ir.position, size: ir.size,
                                name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                            };
                            ops.push(crate::operations::Operation::UpdateInstrumentRegion { id, before, after });
                        }
                    }
                    _ => {}
                }
            }
            // Commit overlap changes from live resolution
            let overlap_snaps = std::mem::take(&mut self.arrow_nudge_overlap_snapshots);
            for (id, original) in overlap_snaps {
                if let Some(wf) = self.waveforms.get(&id) {
                    if wf.disabled {
                        self.waveforms.shift_remove(&id);
                        let ac = self.audio_clips.shift_remove(&id);
                        ops.push(crate::operations::Operation::DeleteWaveform {
                            id, data: original, audio_clip: ac.map(|c| (id, c)),
                        });
                    } else {
                        ops.push(crate::operations::Operation::UpdateWaveform {
                            id, before: original, after: wf.clone(),
                        });
                    }
                }
            }
            for id in self.arrow_nudge_overlap_temp_splits.drain(..) {
                if let Some(wf_data) = self.waveforms.get(&id).cloned() {
                    let ac = self.audio_clips.get(&id).cloned();
                    ops.push(crate::operations::Operation::CreateWaveform {
                        id, data: wf_data, audio_clip: ac.map(|c| (id, c)),
                    });
                }
            }
            if !ops.is_empty() {
                self.push_op(crate::operations::Operation::Batch(ops));
            }
            self.sync_audio_clips();
            self.arrow_nudge_last = None;
        }
    }

    /// Move all selected entities by (dx, dy) pixels. Rapid calls within 500ms coalesce into one undo step.
    pub(crate) fn nudge_selection(&mut self, dx: f32, dy: f32) {
        if self.selected.is_empty() {
            return;
        }

        let should_coalesce = self.arrow_nudge_before.is_some()
            && self.arrow_nudge_last.map_or(false, |t| t.elapsed().as_millis() < 500);

        if !should_coalesce {
            // Flush any stale pending nudge
            self.commit_arrow_nudge();
            // Capture fresh before-states
            let before_states: Vec<(HitTarget, EntityBeforeState)> = self.selected.iter().filter_map(|t| {
                match t {
                    HitTarget::Object(id) => self.objects.get(id).map(|o| (*t, EntityBeforeState::Object(o.clone()))),
                    HitTarget::Waveform(id) => self.waveforms.get(id).map(|w| (*t, EntityBeforeState::Waveform(w.clone()))),
                    HitTarget::EffectRegion(id) => self.effect_regions.get(id).map(|e| (*t, EntityBeforeState::EffectRegion(e.clone()))),
                    HitTarget::PluginBlock(id) => self.plugin_blocks.get(id).map(|p| (*t, EntityBeforeState::PluginBlock(p.snapshot()))),
                    HitTarget::LoopRegion(id) => self.loop_regions.get(id).map(|l| (*t, EntityBeforeState::LoopRegion(l.clone()))),
                    HitTarget::ExportRegion(id) => self.export_regions.get(id).map(|x| (*t, EntityBeforeState::ExportRegion(x.clone()))),
                    HitTarget::ComponentDef(id) => self.components.get(id).map(|c| (*t, EntityBeforeState::ComponentDef(c.clone()))),
                    HitTarget::ComponentInstance(id) => self.component_instances.get(id).map(|c| (*t, EntityBeforeState::ComponentInstance(c.clone()))),
                    HitTarget::MidiClip(id) => self.midi_clips.get(id).map(|m| (*t, EntityBeforeState::MidiClip(m.clone()))),
                    HitTarget::InstrumentRegion(id) => self.instrument_regions.get(id).map(|r| {
                        let snap = instruments::InstrumentRegionSnapshot {
                            position: r.position, size: r.size,
                            name: r.name.clone(), plugin_id: r.plugin_id.clone(),
                            plugin_name: r.plugin_name.clone(), plugin_path: r.plugin_path.clone(),
                        };
                        (*t, EntityBeforeState::InstrumentRegion(snap))
                    }),
                }
            }).collect();
            self.arrow_nudge_before = Some(before_states);
        }

        // Move all selected entities as a group: snap the anchor and apply the same delta to all.
        // Only snap the axis that is actually being nudged (dx != 0 or dy != 0).
        let targets: Vec<HitTarget> = self.selected.clone();
        let anchor_pos = self.get_target_pos(&targets[0]);
        let actual_dx = if dx != 0.0 {
            let raw_x = anchor_pos[0] + dx;
            let snapped_x = if self.is_snap_override_active() {
                raw_x
            } else {
                crate::grid::snap_to_grid(raw_x, &self.settings, self.camera.zoom, self.bpm)
            };
            snapped_x - anchor_pos[0]
        } else {
            0.0
        };
        let actual_dy = if dy != 0.0 {
            let raw_y = anchor_pos[1] + dy;
            let snapped_y = if self.is_snap_override_active() {
                raw_y
            } else {
                crate::grid::snap_to_vertical_grid(raw_y, &self.settings, self.camera.zoom, self.bpm)
            };
            snapped_y - anchor_pos[1]
        } else {
            0.0
        };
        for t in &targets {
            let pos = self.get_target_pos(t);
            self.set_target_pos(t, [pos[0] + actual_dx, pos[1] + actual_dy]);
        }

        // Live waveform overlap resolution (same as mouse drag)
        let moved_wf_ids: Vec<EntityId> = targets.iter()
            .filter_map(|t| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
            .collect();
        if !moved_wf_ids.is_empty() {
            let mut snaps = std::mem::take(&mut self.arrow_nudge_overlap_snapshots);
            let mut tsplits = std::mem::take(&mut self.arrow_nudge_overlap_temp_splits);
            self.resolve_waveform_overlaps_live(&moved_wf_ids, &mut snaps, &mut tsplits);
            self.arrow_nudge_overlap_snapshots = snaps;
            self.arrow_nudge_overlap_temp_splits = tsplits;
        }

        self.sync_audio_clips();
        self.sync_loop_region();
        self.arrow_nudge_last = Some(TimeInstant::now());
        self.mark_dirty();
        self.request_redraw();
    }

    fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::Copy => {
                self.copy_selected();
            }
            CommandAction::Paste => {
                self.paste_clipboard();
            }
            CommandAction::Duplicate => {
                self.duplicate_selected();
            }
            CommandAction::Delete => {
                self.delete_selected();
            }
            CommandAction::SelectAll => {
                self.selected.clear();
                for &id in self.objects.keys() {
                    self.selected.push(HitTarget::Object(id));
                }
                for &id in self.waveforms.keys() {
                    let in_component = self
                        .components
                        .values()
                        .any(|c| c.waveform_ids.contains(&id));
                    if !in_component {
                        self.selected.push(HitTarget::Waveform(id));
                    }
                }
                for &id in self.effect_regions.keys() {
                    self.selected.push(HitTarget::EffectRegion(id));
                }
                for &id in self.loop_regions.keys() {
                    self.selected.push(HitTarget::LoopRegion(id));
                }
                for &id in self.components.keys() {
                    self.selected.push(HitTarget::ComponentDef(id));
                }
                for &id in self.component_instances.keys() {
                    self.selected.push(HitTarget::ComponentInstance(id));
                }
            }
            CommandAction::Undo => { self.undo_op(); }
            CommandAction::Redo => { self.redo_op(); }
            CommandAction::SaveProject => self.save_project(),
            CommandAction::ZoomIn => {
                let (sw, sh, _) = self.screen_info();
                self.camera.zoom_at([sw * 0.5, sh * 0.5], 1.25);
            }
            CommandAction::ZoomOut => {
                let (sw, sh, _) = self.screen_info();
                self.camera.zoom_at([sw * 0.5, sh * 0.5], 0.8);
            }
            CommandAction::ResetZoom => {
                let (_, _, scale) = self.screen_info();
                self.camera.zoom = scale;
            }
            CommandAction::ToggleBrowser => {
                self.sample_browser.visible = !self.sample_browser.visible;
                #[cfg(feature = "native")]
                if self.sample_browser.visible {
                    self.refresh_project_browser_entries();
                    self.ensure_plugins_scanned();
                }
            }
            CommandAction::AddFolderToBrowser => {
                #[cfg(feature = "native")]
                self.open_add_folder_dialog();
            }
            CommandAction::SetMasterVolume => {
                #[cfg(feature = "native")]
                if let Some(p) = &mut self.command_palette {
                    p.mode = PaletteMode::VolumeFader;
                    p.fader_value = self
                        .audio_engine
                        .as_ref()
                        .map_or(1.0, |e| e.master_volume());
                    p.search_text.clear();
                }
                self.request_redraw();
                return;
            }
            CommandAction::CreateComponent => {
                self.create_component_from_selection();
            }
            CommandAction::CreateInstance => {
                self.create_instance_of_selected_component();
            }
            CommandAction::GoToComponent => {
                self.go_to_component_of_selected_instance();
            }
            CommandAction::OpenSettings => {
                #[cfg(feature = "native")]
                {
                    self.settings_window = if self.settings_window.is_some() {
                        None
                    } else {
                        Some(SettingsWindow::new())
                    };
                }
            }
            CommandAction::RenameEffectRegion => {
                let selected_er = self.selected.iter().find_map(|t| {
                    if let HitTarget::EffectRegion(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(er_id) = selected_er {
                    if let Some(er) = self.effect_regions.get(&er_id) {
                        let current = er.name.clone();
                        self.editing_effect_name = Some((er_id, current));
                    }
                }
            }
            CommandAction::RenameSample => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(wf_id) = selected_wf {
                    if let Some(wf) = self.waveforms.get(&wf_id) {
                        let current = wf.audio.filename.clone();
                        self.editing_waveform_name = Some((wf_id, current));
                    }
                }
            }
            CommandAction::ToggleSnapToGrid => {
                self.settings.snap_to_grid = !self.settings.snap_to_grid;
                self.settings.save();
            }
            CommandAction::ToggleVerticalSnap => {
                self.settings.snap_to_vertical_grid = !self.settings.snap_to_vertical_grid;
                self.settings.save();
            }
            CommandAction::ToggleGrid => {
                self.settings.grid_enabled = !self.settings.grid_enabled;
                self.settings.save();
                self.mark_dirty();
            }
            CommandAction::SetGridAdaptive(size) => {
                self.settings.grid_mode = GridMode::Adaptive(size);
                self.settings.save();
            }
            CommandAction::SetGridFixed(fg) => {
                self.settings.grid_mode = GridMode::Fixed(fg);
                self.settings.save();
            }
            CommandAction::NarrowGrid => {
                match self.settings.grid_mode {
                    GridMode::Adaptive(s) => {
                        self.settings.grid_mode = GridMode::Adaptive(s.narrower());
                    }
                    GridMode::Fixed(f) => {
                        self.settings.grid_mode = GridMode::Fixed(f.finer());
                    }
                }
                self.settings.save();
            }
            CommandAction::WidenGrid => {
                match self.settings.grid_mode {
                    GridMode::Adaptive(s) => {
                        self.settings.grid_mode = GridMode::Adaptive(s.wider());
                    }
                    GridMode::Fixed(f) => {
                        self.settings.grid_mode = GridMode::Fixed(f.coarser());
                    }
                }
                self.settings.save();
            }
            CommandAction::ToggleTripletGrid => {
                self.settings.triplet_grid = !self.settings.triplet_grid;
                self.settings.save();
            }
            CommandAction::SetMidiClipGridFixed(fg) => {
                if let Some(mc_id) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        mc.grid_mode = GridMode::Fixed(fg);
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::SetMidiClipGridAdaptive(size) => {
                if let Some(mc_id) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        mc.grid_mode = GridMode::Adaptive(size);
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::ToggleMidiClipTripletGrid => {
                if let Some(mc_id) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        mc.triplet_grid = !mc.triplet_grid;
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::NarrowMidiClipGrid => {
                if let Some(mc_id) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        match mc.grid_mode {
                            GridMode::Adaptive(s) => {
                                mc.grid_mode = GridMode::Adaptive(s.narrower());
                            }
                            GridMode::Fixed(f) => {
                                mc.grid_mode = GridMode::Fixed(f.finer());
                            }
                        }
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::WidenMidiClipGrid => {
                if let Some(mc_id) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        match mc.grid_mode {
                            GridMode::Adaptive(s) => {
                                mc.grid_mode = GridMode::Adaptive(s.wider());
                            }
                            GridMode::Fixed(f) => {
                                mc.grid_mode = GridMode::Fixed(f.coarser());
                            }
                        }
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::ToggleAutomation => {
                self.automation_mode = !self.automation_mode;
                if self.automation_mode {
                    self.active_automation_param = crate::automation::AutomationParam::Volume;
                }
                self.mark_dirty();
            }
            CommandAction::AddVolumeAutomation => {
                self.automation_mode = true;
                self.active_automation_param = crate::automation::AutomationParam::Volume;
                self.mark_dirty();
            }
            CommandAction::AddPanAutomation => {
                self.automation_mode = true;
                self.active_automation_param = crate::automation::AutomationParam::Pan;
                self.mark_dirty();
            }
            CommandAction::TestToast => {
                self.toast_manager
                    .push("This is an error toast", ui::toast::ToastKind::Error);
                self.toast_manager
                    .push("This is an info toast", ui::toast::ToastKind::Info);
                self.toast_manager
                    .push("This is a success toast", ui::toast::ToastKind::Success);
            }
            CommandAction::RevealInFinder => {
                if let Some(path) = self.browser_context_path.take() {
                    std::process::Command::new("open")
                        .arg("-R")
                        .arg(&path)
                        .spawn()
                        .ok();
                }
            }
            CommandAction::ReverseSample => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(wf_id) = selected_wf {
                    if self.waveforms.contains_key(&wf_id) && self.audio_clips.contains_key(&wf_id) {
                        let before = self.waveforms[&wf_id].clone();

                        let mut mono = (*self.audio_clips[&wf_id].samples).clone();
                        mono.reverse();
                        self.audio_clips.get_mut(&wf_id).unwrap().samples = Arc::new(mono);

                        let old = &self.waveforms[&wf_id].audio;
                        let mut left = (*old.left_samples).clone();
                        let mut right = (*old.right_samples).clone();
                        left.reverse();
                        right.reverse();
                        let left_peaks = Arc::new(WaveformPeaks::build(&left));
                        let right_peaks = Arc::new(WaveformPeaks::build(&right));
                        let new_audio = Arc::new(AudioData {
                            left_samples: Arc::new(left),
                            right_samples: Arc::new(right),
                            left_peaks,
                            right_peaks,
                            sample_rate: old.sample_rate,
                            filename: old.filename.clone(),
                        });
                        self.waveforms.get_mut(&wf_id).unwrap().audio = new_audio;

                        let after = self.waveforms[&wf_id].clone();
                        self.push_op(operations::Operation::UpdateWaveform { id: wf_id, before, after });
                        #[cfg(feature = "native")]
                        self.sync_audio_clips();
                    }
                }
            }
            CommandAction::SplitSample => {
                self.split_sample_at_cursor();
            }
            CommandAction::AddLoopArea => {
                self.add_loop_area();
            }
            CommandAction::AddEffectsArea => {
                self.add_effect_area();
            }
            CommandAction::AddMidiClip => {
                self.add_midi_clip();
            }
            CommandAction::AddInstrument => {
                #[cfg(feature = "native")]
                {
                    self.ensure_plugins_scanned();
                    let entries: Vec<PluginPickerEntry> = self
                        .plugin_registry
                        .instruments
                        .iter()
                        .map(|e| PluginPickerEntry {
                            name: e.info.name.clone(),
                            manufacturer: e.info.manufacturer.clone(),
                            unique_id: e.info.unique_id.clone(),
                            is_instrument: true,
                        })
                        .collect();
                    if let Some(p) = &mut self.command_palette {
                        p.mode = PaletteMode::InstrumentPicker;
                        p.search_text.clear();
                        p.set_plugin_entries(entries);
                    }
                    self.request_redraw();
                    return;
                }
            }
            CommandAction::AddPlugin => {
                #[cfg(feature = "native")]
                {
                    self.ensure_plugins_scanned();
                    let entries: Vec<PluginPickerEntry> = self
                        .plugin_registry
                        .plugins
                        .iter()
                        .map(|e| PluginPickerEntry {
                            name: e.info.name.clone(),
                            manufacturer: e.info.manufacturer.clone(),
                            unique_id: e.info.unique_id.clone(),
                            is_instrument: false,
                        })
                        .collect();
                    if let Some(p) = &mut self.command_palette {
                        p.mode = PaletteMode::PluginPicker;
                        p.search_text.clear();
                        p.set_plugin_entries(entries);
                    }
                    self.request_redraw();
                    return;
                }
            }
            CommandAction::AddRenderArea => {
                self.add_render_area();
            }
            CommandAction::SetSampleColor(idx) => {
                if let Some(&color) = WAVEFORM_COLORS.get(idx) {
                    for target in self.selected.clone() {
                        if let HitTarget::Waveform(i) = target {
                            if let Some(wf) = self.waveforms.get_mut(&i) { wf.color = color; }
                        }
                    }
                }
            }
            CommandAction::MoveLayerUp => {
                if let Some(target) = self.selected.first() {
                    let id = match target {
                        HitTarget::InstrumentRegion(id) | HitTarget::Waveform(id) |
                        HitTarget::EffectRegion(id) | HitTarget::MidiClip(id) |
                        HitTarget::PluginBlock(id) => Some(*id),
                        _ => None,
                    };
                    if let Some(id) = id {
                        if layers::move_node_up(&mut self.layer_tree, id) {
                            self.refresh_project_browser_entries();
                            self.mark_dirty();
                        }
                    }
                }
            }
            CommandAction::MoveLayerDown => {
                if let Some(target) = self.selected.first() {
                    let id = match target {
                        HitTarget::InstrumentRegion(id) | HitTarget::Waveform(id) |
                        HitTarget::EffectRegion(id) | HitTarget::MidiClip(id) |
                        HitTarget::PluginBlock(id) => Some(*id),
                        _ => None,
                    };
                    if let Some(id) = id {
                        if layers::move_node_down(&mut self.layer_tree, id) {
                            self.refresh_project_browser_entries();
                            self.mark_dirty();
                        }
                    }
                }
            }
            CommandAction::SetWarpOff | CommandAction::SetWarpRePitch | CommandAction::SetWarpSemitone => {
                let new_mode = match action {
                    CommandAction::SetWarpRePitch => ui::waveform::WarpMode::RePitch,
                    CommandAction::SetWarpSemitone => ui::waveform::WarpMode::Semitone,
                    _ => ui::waveform::WarpMode::Off,
                };
                if let Some(rw) = &self.right_window {
                    let wf_id = rw.waveform_id;
                    if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.warp_mode = new_mode;
                            if new_mode == ui::waveform::WarpMode::RePitch {
                                if let Some(clip) = self.audio_clips.get(&wf_id) {
                                    let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                    wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                                }
                            }
                        }
                        if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                            self.push_op(crate::operations::Operation::UpdateWaveform {
                                id: wf_id, before, after,
                            });
                        }
                    }
                }
                self.update_right_window();
                self.mark_dirty();
                #[cfg(feature = "native")]
                self.sync_audio_clips();
            }
        }
        self.request_redraw();
    }

    fn split_sample_at_cursor(&mut self) {
        let screen_pos = self
            .context_menu
            .as_ref()
            .map(|m| m.position)
            .unwrap_or(self.mouse_pos);
        let world = self.camera.screen_to_world(screen_pos);
        let hit = hit_test(
            &self.objects,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
            &self.loop_regions,
            &self.export_regions,
            &self.components,
            &self.component_instances,
            &self.midi_clips,
            &self.instrument_regions,
            self.editing_component,
            world,
            &self.camera,
        );
        let wf_id = match hit {
            Some(HitTarget::Waveform(i)) => i,
            _ => return,
        };
        let wf = match self.waveforms.get(&wf_id) {
            Some(w) => w,
            None => return,
        };
        let clip = match self.audio_clips.get(&wf_id) {
            Some(c) => c,
            None => return,
        };

        let pos = wf.position;
        let size = wf.size;
        let offset_px = wf.sample_offset_px;
        let split_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
        let t = ((split_x - pos[0]) / size[0]).clamp(0.01, 0.99);

        let audio = Arc::clone(&wf.audio);
        let mono_samples = Arc::clone(&clip.samples);
        let total_mono = mono_samples.len();
        if total_mono == 0 {
            return;
        }

        let full_w = full_audio_width_px(wf);
        let vis_start_frac = if full_w > 0.0 { offset_px / full_w } else { 0.0 };
        let vis_end_frac = if full_w > 0.0 { (offset_px + size[0]) / full_w } else { 1.0 };
        let split_frac = vis_start_frac + t * (vis_end_frac - vis_start_frac);

        let vis_start_mono = (vis_start_frac * total_mono as f32) as usize;
        let vis_end_mono = (vis_end_frac * total_mono as f32).min(total_mono as f32) as usize;
        let split_mono = (split_frac * total_mono as f32) as usize;

        let vis_start_left = (vis_start_frac * audio.left_samples.len() as f32) as usize;
        let vis_end_left = (vis_end_frac * audio.left_samples.len() as f32).min(audio.left_samples.len() as f32) as usize;
        let split_left = (split_frac * audio.left_samples.len() as f32) as usize;

        let vis_start_right = (vis_start_frac * audio.right_samples.len() as f32) as usize;
        let vis_end_right = (vis_end_frac * audio.right_samples.len() as f32).min(audio.right_samples.len() as f32) as usize;
        let split_right = (split_frac * audio.right_samples.len() as f32) as usize;

        let orig_color = wf.color;
        let orig_border_radius = wf.border_radius;
        let orig_fade_in = wf.fade_in_px;
        let orig_fade_out = wf.fade_out_px;
        let orig_fade_in_curve = wf.fade_in_curve;
        let orig_fade_out_curve = wf.fade_out_curve;
        let orig_volume = wf.volume;

        let before_wf = self.waveforms[&wf_id].clone();

        let sample_rate = audio.sample_rate;
        let filename = audio.filename.clone();

        let left_mono: Vec<f32> = mono_samples[vis_start_mono..split_mono].to_vec();
        let right_mono: Vec<f32> = mono_samples[split_mono..vis_end_mono].to_vec();
        let left_l: Vec<f32> = audio.left_samples[vis_start_left..split_left].to_vec();
        let left_r: Vec<f32> = audio.right_samples[vis_start_right..split_right].to_vec();
        let right_l: Vec<f32> = audio.left_samples[split_left..vis_end_left].to_vec();
        let right_r: Vec<f32> = audio.right_samples[split_right..vis_end_right].to_vec();

        let left_duration = left_mono.len() as f32 / sample_rate as f32;
        let right_duration = right_mono.len() as f32 / sample_rate as f32;
        let left_width = left_duration * PIXELS_PER_SECOND;
        let right_width = right_duration * PIXELS_PER_SECOND;

        let left_clip = AudioClipData {
            samples: Arc::new(left_mono.clone()),
            sample_rate,
            duration_secs: left_duration,
        };
        let left_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&left_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&left_r)),
            left_samples: Arc::new(left_l),
            right_samples: Arc::new(left_r),
            sample_rate,
            filename: filename.clone(),
        });
        let left_waveform = WaveformView {
            audio: left_audio,
            filename: filename.clone(),
            position: pos,
            size: [left_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: orig_fade_in,
            fade_out_px: 0.0,
            fade_in_curve: orig_fade_in_curve,
            fade_out_curve: 0.0,
            volume: orig_volume,
            pan: 0.5,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: self.bpm,
            pitch_semitones: 0.0,
            disabled: false,
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
        };

        let right_clip = AudioClipData {
            samples: Arc::new(right_mono.clone()),
            sample_rate,
            duration_secs: right_duration,
        };
        let right_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&right_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&right_r)),
            left_samples: Arc::new(right_l),
            right_samples: Arc::new(right_r),
            sample_rate,
            filename: filename.clone(),
        });
        let right_waveform = WaveformView {
            audio: right_audio,
            filename,
            position: [pos[0] + left_width, pos[1]],
            size: [right_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: 0.0,
            fade_out_px: orig_fade_out,
            fade_in_curve: 0.0,
            fade_out_curve: orig_fade_out_curve,
            volume: orig_volume,
            pan: 0.5,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: self.bpm,
            pitch_semitones: 0.0,
            disabled: false,
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
        };

        // Replace original with left half
        *self.waveforms.get_mut(&wf_id).unwrap() = left_waveform;
        *self.audio_clips.get_mut(&wf_id).unwrap() = left_clip;

        // Insert right half as new entity
        let right_id = new_id();
        self.waveforms.insert(right_id, right_waveform);
        self.audio_clips.insert(right_id, right_clip);

        // Fix up waveform_ids in component defs
        for comp in self.components.values_mut() {
            let mut new_ids = Vec::new();
            for &wi in &comp.waveform_ids {
                new_ids.push(wi);
                if wi == wf_id {
                    new_ids.push(right_id);
                }
            }
            comp.waveform_ids = new_ids;
        }

        // Add right half to selection
        self.selected.push(HitTarget::Waveform(right_id));

        let after_wf = self.waveforms[&wf_id].clone();
        let right_wf_data = self.waveforms[&right_id].clone();
        let right_ac_data = self.audio_clips.get(&right_id).cloned();
        let mut split_ops = vec![
            operations::Operation::UpdateWaveform { id: wf_id, before: before_wf, after: after_wf },
            operations::Operation::CreateWaveform { id: right_id, data: right_wf_data, audio_clip: right_ac_data.map(|c| (right_id, c)) },
        ];
        let overlap_ops = self.resolve_waveform_overlaps(&[wf_id, right_id]);
        split_ops.extend(overlap_ops);
        self.push_op(operations::Operation::Batch(split_ops));
        self.sync_audio_clips();
    }

    fn create_component_from_selection(&mut self) {
        let wf_ids: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();
        if wf_ids.is_empty() {
            println!("No waveforms selected to create component");
            return;
        }
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &wf_ids);
        let comp_id = new_id();
        self.next_component_id = new_id();
        let name = format!("Component {}", &comp_id.to_string()[..8]);
        let wf_count = wf_ids.len();
        let def = component::ComponentDef {
            id: comp_id,
            name: name.clone(),
            position: pos,
            size,
            waveform_ids: wf_ids,
        };
        self.components.insert(comp_id, def.clone());
        self.push_op(operations::Operation::CreateComponent { id: comp_id, data: def });
        self.selected.clear();
        self.selected.push(HitTarget::ComponentDef(comp_id));
        println!(
            "Created component '{}' with {} waveforms",
            name,
            wf_count
        );
    }

    fn create_instance_of_selected_component(&mut self) {
        let comp_id = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentDef(i) => Some(*i),
            _ => None,
        });
        if let Some(ci) = comp_id {
            let (comp_id_val, offset_x, def_name, inst_pos) = match self.components.get(&ci) {
                Some(d) => (d.id, d.size[0] + 50.0, d.name.clone(), [d.position[0] + d.size[0] + 50.0, d.position[1]]),
                None => return,
            };
            let inst = component::ComponentInstance {
                component_id: comp_id_val,
                position: inst_pos,
            };
            let inst_id = new_id();
            self.component_instances.insert(inst_id, inst.clone());
            self.push_op(operations::Operation::CreateComponentInstance { id: inst_id, data: inst });
            self.selected.clear();
            self.selected.push(HitTarget::ComponentInstance(inst_id));
            println!("Created instance of component {}", def_name);
            self.sync_audio_clips();
        }
    }

    fn go_to_component_of_selected_instance(&mut self) {
        let inst_id = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentInstance(i) => Some(*i),
            _ => None,
        });
        if let Some(ii) = inst_id {
            let comp_id = match self.component_instances.get(&ii) {
                Some(inst) => inst.component_id,
                None => return,
            };
            if let Some((&ci, def)) = self
                .components
                .iter()
                .find(|(_, c)| c.id == comp_id)
            {
                let (sw, sh, _) = self.screen_info();
                self.camera.position = [
                    def.position[0] + def.size[0] * 0.5 - sw * 0.5 / self.camera.zoom,
                    def.position[1] + def.size[1] * 0.5 - sh * 0.5 / self.camera.zoom,
                ];
                self.selected.clear();
                self.selected.push(HitTarget::ComponentDef(ci));
                println!("Navigated to component '{}'", def.name);
            }
        }
    }

    fn duplicate_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut new_selected: Vec<HitTarget> = Vec::new();
        let mut dup_ops: Vec<operations::Operation> = Vec::new();

        let selected_wf_ids: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| {
                if let HitTarget::Waveform(i) = t {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();

        let wf_group_shift = if selected_wf_ids.len() >= 2 {
            let min_start = selected_wf_ids
                .iter()
                .filter_map(|i| self.waveforms.get(i))
                .map(|wf| wf.position[0])
                .fold(f32::INFINITY, f32::min);
            let max_end = selected_wf_ids
                .iter()
                .filter_map(|i| self.waveforms.get(i))
                .map(|wf| wf.position[0] + wf.size[0])
                .fold(f32::NEG_INFINITY, f32::max);
            Some(max_end - min_start)
        } else {
            None
        };

        for target in self.selected.clone() {
            match target {
                HitTarget::ComponentInstance(i) => {
                    if let Some(src) = self.component_instances.get(&i).cloned() {
                        let def = self.components.values().find(|c| c.id == src.component_id);
                        let shift = def.map(|d| d.size[0]).unwrap_or(100.0);
                        let inst = component::ComponentInstance {
                            component_id: src.component_id,
                            position: [src.position[0] + shift, src.position[1]],
                        };
                        let nid = new_id();
                        self.component_instances.insert(nid, inst);
                        new_selected.push(HitTarget::ComponentInstance(nid));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if let Some(src) = self.components.get(&i).cloned() {
                        let shift = src.size[0];
                        let comp_nid = new_id();
                        self.next_component_id = new_id();
                        let src_wf_ids = src.waveform_ids.clone();
                        let mut new_wf_ids = Vec::new();
                        for &wi in &src_wf_ids {
                            if let Some(wf) = self.waveforms.get(&wi).cloned() {
                                let mut wf = wf;
                                wf.position[0] += shift;
                                let wf_nid = new_id();
                                self.waveforms.insert(wf_nid, wf);
                                new_wf_ids.push(wf_nid);
                                if let Some(clip) = self.audio_clips.get(&wi).cloned() {
                                    self.audio_clips.insert(wf_nid, clip);
                                }
                            }
                        }
                        self.components.insert(comp_nid, component::ComponentDef {
                            id: comp_nid,
                            name: format!("{} copy", src.name),
                            position: [src.position[0] + shift, src.position[1]],
                            size: src.size,
                            waveform_ids: new_wf_ids,
                        });
                        new_selected.push(HitTarget::ComponentDef(comp_nid));
                    }
                }
                HitTarget::Waveform(i) => {
                    if let Some(wf) = self.waveforms.get(&i).cloned() {
                        let mut wf = wf;
                        let shift = wf_group_shift.unwrap_or(wf.size[0]);
                        wf.position[0] += shift;
                        let nid = new_id();
                        self.waveforms.insert(nid, wf);
                        if let Some(clip) = self.audio_clips.get(&i).cloned() {
                            self.audio_clips.insert(nid, clip);
                        }
                        new_selected.push(HitTarget::Waveform(nid));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if let Some(er) = self.effect_regions.get(&i).cloned() {
                        let mut er = er;
                        er.position[0] += er.size[0];
                        let nid = new_id();
                        self.effect_regions.insert(nid, er);
                        new_selected.push(HitTarget::EffectRegion(nid));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if let Some(pb) = self.plugin_blocks.get(&i).cloned() {
                        let mut pb = pb;
                        pb.position[0] += pb.size[0];
                        let nid = new_id();
                        self.plugin_blocks.insert(nid, pb);
                        new_selected.push(HitTarget::PluginBlock(nid));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if let Some(lr) = self.loop_regions.get(&i).cloned() {
                        let mut lr = lr;
                        lr.position[0] += lr.size[0];
                        let nid = new_id();
                        self.loop_regions.insert(nid, lr);
                        new_selected.push(HitTarget::LoopRegion(nid));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if let Some(xr) = self.export_regions.get(&i).cloned() {
                        let mut xr = xr;
                        xr.position[0] += xr.size[0];
                        let nid = new_id();
                        self.export_regions.insert(nid, xr);
                        new_selected.push(HitTarget::ExportRegion(nid));
                    }
                }
                HitTarget::Object(i) => {
                    if let Some(obj) = self.objects.get(&i).cloned() {
                        let mut obj = obj;
                        obj.position[0] += obj.size[0];
                        let nid = new_id();
                        self.objects.insert(nid, obj);
                        new_selected.push(HitTarget::Object(nid));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if let Some(mc) = self.midi_clips.get(&i).cloned() {
                        let mut mc = mc;
                        mc.position[0] += mc.size[0];
                        let nid = new_id();
                        self.midi_clips.insert(nid, mc);
                        new_selected.push(HitTarget::MidiClip(nid));
                    }
                }
                HitTarget::InstrumentRegion(i) => {
                    if let Some(ir) = self.instrument_regions.get(&i).cloned() {
                        let mut ir = ir;
                        ir.position[0] += ir.size[0];
                        let nid = new_id();
                        self.instrument_regions.insert(nid, ir);
                        new_selected.push(HitTarget::InstrumentRegion(nid));
                    }
                }
            }
        }

        // Build ops from all duplicated entities
        for t in &new_selected {
            match t {
                HitTarget::Object(id) => { if let Some(d) = self.objects.get(id) { dup_ops.push(operations::Operation::CreateObject { id: *id, data: d.clone() }); } }
                HitTarget::Waveform(id) => { if let Some(d) = self.waveforms.get(id) { let ac = self.audio_clips.get(id).cloned(); dup_ops.push(operations::Operation::CreateWaveform { id: *id, data: d.clone(), audio_clip: ac.map(|c| (*id, c)) }); } }
                HitTarget::EffectRegion(id) => { if let Some(d) = self.effect_regions.get(id) { dup_ops.push(operations::Operation::CreateEffectRegion { id: *id, data: d.clone() }); } }
                HitTarget::PluginBlock(id) => { if let Some(d) = self.plugin_blocks.get(id) { dup_ops.push(operations::Operation::CreatePluginBlock { id: *id, data: d.snapshot() }); } }
                HitTarget::LoopRegion(id) => { if let Some(d) = self.loop_regions.get(id) { dup_ops.push(operations::Operation::CreateLoopRegion { id: *id, data: d.clone() }); } }
                HitTarget::ExportRegion(id) => { if let Some(d) = self.export_regions.get(id) { dup_ops.push(operations::Operation::CreateExportRegion { id: *id, data: d.clone() }); } }
                HitTarget::ComponentDef(id) => { if let Some(d) = self.components.get(id) { dup_ops.push(operations::Operation::CreateComponent { id: *id, data: d.clone() }); } }
                HitTarget::ComponentInstance(id) => { if let Some(d) = self.component_instances.get(id) { dup_ops.push(operations::Operation::CreateComponentInstance { id: *id, data: d.clone() }); } }
                HitTarget::MidiClip(id) => { if let Some(d) = self.midi_clips.get(id) { dup_ops.push(operations::Operation::CreateMidiClip { id: *id, data: d.clone() }); } }
                HitTarget::InstrumentRegion(id) => { if let Some(ir) = self.instrument_regions.get(id) { let snap = instruments::InstrumentRegionSnapshot { position: ir.position, size: ir.size, name: ir.name.clone(), plugin_id: ir.plugin_id.clone(), plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone() }; dup_ops.push(operations::Operation::CreateInstrumentRegion { id: *id, data: snap }); } }
            }
        }
        let dup_wf_ids: Vec<EntityId> = new_selected.iter()
            .filter_map(|t| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
            .collect();
        if !dup_wf_ids.is_empty() {
            let overlap_ops = self.resolve_waveform_overlaps(&dup_wf_ids);
            dup_ops.extend(overlap_ops);
        }
        if !dup_ops.is_empty() {
            self.push_op(operations::Operation::Batch(dup_ops));
        }
        self.selected = new_selected;
        self.sync_audio_clips();
    }

    fn copy_selected(&mut self) {
        self.clipboard.items.clear();
        // If editing a MIDI clip with selected notes, copy those instead
        if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                if !self.selected_midi_notes.is_empty() {
                    let notes = &mc.notes;
                    let min_start = self.selected_midi_notes.iter()
                        .filter(|&&ni| ni < notes.len())
                        .map(|&ni| notes[ni].start_px)
                        .fold(f32::INFINITY, f32::min);
                    let mut copied: Vec<midi::MidiNote> = Vec::new();
                    for &ni in &self.selected_midi_notes {
                        if ni < notes.len() {
                            let mut n = notes[ni].clone();
                            n.start_px -= min_start;
                            copied.push(n);
                        }
                    }
                    self.clipboard.items.push(ClipboardItem::MidiNotes(copied));
                    return;
                }
            }
        }
        for target in &self.selected {
            match target {
                HitTarget::Object(i) => {
                    if let Some(obj) = self.objects.get(i) {
                        self.clipboard.items.push(ClipboardItem::Object(obj.clone()));
                    }
                }
                HitTarget::Waveform(i) => {
                    if let Some(wf) = self.waveforms.get(i) {
                        let clip = self.audio_clips.get(i).cloned();
                        self.clipboard.items.push(ClipboardItem::Waveform(wf.clone(), clip));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if let Some(er) = self.effect_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::EffectRegion(er.clone()));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if let Some(pb) = self.plugin_blocks.get(i) {
                        self.clipboard.items.push(ClipboardItem::PluginBlock(pb.clone()));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if let Some(lr) = self.loop_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::LoopRegion(lr.clone()));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if let Some(xr) = self.export_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::ExportRegion(xr.clone()));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if let Some(def) = self.components.get(i) {
                        let wfs: Vec<(WaveformView, Option<AudioClipData>)> = def
                            .waveform_ids
                            .iter()
                            .filter_map(|wi| {
                                if let Some(wf) = self.waveforms.get(wi) {
                                    let clip = self.audio_clips.get(wi).cloned();
                                    Some((wf.clone(), clip))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        self.clipboard.items.push(ClipboardItem::ComponentDef(def.clone(), wfs));
                    }
                }
                HitTarget::ComponentInstance(i) => {
                    if let Some(inst) = self.component_instances.get(i) {
                        self.clipboard.items.push(ClipboardItem::ComponentInstance(inst.clone()));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if let Some(mc) = self.midi_clips.get(i) {
                        self.clipboard.items.push(ClipboardItem::MidiClip(mc.clone()));
                    }
                }
                HitTarget::InstrumentRegion(i) => {
                    if let Some(ir) = self.instrument_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::InstrumentRegion(
                            instruments::InstrumentRegionSnapshot {
                                position: ir.position,
                                size: ir.size,
                                name: ir.name.clone(),
                                plugin_id: ir.plugin_id.clone(),
                                plugin_name: ir.plugin_name.clone(),
                                plugin_path: ir.plugin_path.clone(),
                            },
                        ));
                    }
                }
            }
        }
    }

    fn paste_clipboard(&mut self) {
        if self.clipboard.items.is_empty() {
            return;
        }
        // If editing a MIDI clip and clipboard has MIDI notes, paste them
        if let Some(mc_id) = self.editing_midi_clip {
            let midi_notes = self.clipboard.items.iter().find_map(|item| {
                if let ClipboardItem::MidiNotes(notes) = item { Some(notes.clone()) } else { None }
            });
            if let Some(notes) = midi_notes {
                let clip_x = self.midi_clips.get(&mc_id).map(|mc| mc.position[0]);
                if let Some(clip_x) = clip_x {
                    let before_notes = self.midi_clips[&mc_id].notes.clone();
                    let paste_x = {
                        #[cfg(feature = "native")]
                        { self.audio_engine.as_ref()
                            .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32)
                            .unwrap_or_else(|| self.camera.screen_to_world(self.mouse_pos)[0]) }
                        #[cfg(not(feature = "native"))]
                        { self.camera.screen_to_world(self.mouse_pos)[0] }
                    };
                    let offset = (paste_x - clip_x).max(0.0);
                    let new_indices = if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        let mut indices: Vec<usize> = Vec::new();
                        for n in &notes {
                            let mut pasted = n.clone();
                            pasted.start_px += offset;
                            mc.notes.push(pasted);
                            indices.push(mc.notes.len() - 1);
                        }
                        Some(indices)
                    } else {
                        None
                    };
                    if let Some(indices) = new_indices {
                        if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                            self.selected_midi_notes = mc.resolve_note_overlaps(&indices);
                        }
                    }
                    let after_notes = self.midi_clips[&mc_id].notes.clone();
                    self.push_op(operations::Operation::UpdateMidiNotes { clip_id: mc_id, before: before_notes, after: after_notes });
                    self.sync_audio_clips();
                    return;
                }
            }
        }
        let world = self.camera.screen_to_world(self.mouse_pos);

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for item in &self.clipboard.items {
            let pos = match item {
                ClipboardItem::Object(o) => o.position,
                ClipboardItem::Waveform(w, _) => w.position,
                ClipboardItem::EffectRegion(e) => e.position,
                ClipboardItem::PluginBlock(pb) => pb.position,
                ClipboardItem::LoopRegion(l) => l.position,
                ClipboardItem::ExportRegion(x) => x.position,
                ClipboardItem::ComponentDef(d, _) => d.position,
                ClipboardItem::ComponentInstance(ci) => ci.position,
                ClipboardItem::MidiClip(mc) => mc.position,
                ClipboardItem::MidiNotes(_) => continue,
                ClipboardItem::InstrumentRegion(ir) => ir.position,
            };
            if pos[0] < min_x {
                min_x = pos[0];
            }
            if pos[1] < min_y {
                min_y = pos[1];
            }
        }

        let dx = world[0] - min_x;
        let dy = world[1] - min_y;
        let mut new_selected: Vec<HitTarget> = Vec::new();

        for item in self.clipboard.items.clone() {
            match item {
                ClipboardItem::Object(mut o) => {
                    o.position[0] += dx;
                    o.position[1] += dy;
                    let nid = new_id();
                    self.objects.insert(nid, o);
                    new_selected.push(HitTarget::Object(nid));
                }
                ClipboardItem::Waveform(mut w, clip) => {
                    w.position[0] += dx;
                    w.position[1] += dy;
                    let nid = new_id();
                    self.waveforms.insert(nid, w);
                    if let Some(c) = clip {
                        self.audio_clips.insert(nid, c);
                    }
                    new_selected.push(HitTarget::Waveform(nid));
                }
                ClipboardItem::EffectRegion(mut e) => {
                    e.position[0] += dx;
                    e.position[1] += dy;
                    let nid = new_id();
                    self.effect_regions.insert(nid, e);
                    new_selected.push(HitTarget::EffectRegion(nid));
                }
                ClipboardItem::PluginBlock(mut pb) => {
                    pb.position[0] += dx;
                    pb.position[1] += dy;
                    let nid = new_id();
                    self.plugin_blocks.insert(nid, pb);
                    new_selected.push(HitTarget::PluginBlock(nid));
                }
                ClipboardItem::LoopRegion(mut l) => {
                    l.position[0] += dx;
                    l.position[1] += dy;
                    let nid = new_id();
                    self.loop_regions.insert(nid, l);
                    new_selected.push(HitTarget::LoopRegion(nid));
                }
                ClipboardItem::ExportRegion(mut x) => {
                    x.position[0] += dx;
                    x.position[1] += dy;
                    let nid = new_id();
                    self.export_regions.insert(nid, x);
                    new_selected.push(HitTarget::ExportRegion(nid));
                }
                ClipboardItem::ComponentDef(mut d, wfs) => {
                    let comp_nid = new_id();
                    self.next_component_id = new_id();
                    d.id = comp_nid;
                    d.position[0] += dx;
                    d.position[1] += dy;
                    d.name = format!("{} copy", d.name);
                    let mut new_wf_ids = Vec::new();
                    for (mut wf, clip) in wfs {
                        wf.position[0] += dx;
                        wf.position[1] += dy;
                        let wf_nid = new_id();
                        self.waveforms.insert(wf_nid, wf);
                        new_wf_ids.push(wf_nid);
                        if let Some(c) = clip {
                            self.audio_clips.insert(wf_nid, c);
                        }
                    }
                    d.waveform_ids = new_wf_ids;
                    self.components.insert(comp_nid, d);
                    new_selected.push(HitTarget::ComponentDef(comp_nid));
                }
                ClipboardItem::ComponentInstance(mut ci) => {
                    ci.position[0] += dx;
                    ci.position[1] += dy;
                    let nid = new_id();
                    self.component_instances.insert(nid, ci);
                    new_selected.push(HitTarget::ComponentInstance(nid));
                }
                ClipboardItem::MidiClip(mut mc) => {
                    mc.position[0] += dx;
                    mc.position[1] += dy;
                    let nid = new_id();
                    self.midi_clips.insert(nid, mc);
                    new_selected.push(HitTarget::MidiClip(nid));
                }
                ClipboardItem::MidiNotes(_) => {
                    // Handled in MIDI editing mode (events.rs), skip in global paste
                }
                ClipboardItem::InstrumentRegion(snap) => {
                    let mut ir = instruments::InstrumentRegion::new(snap.position, snap.size);
                    ir.position[0] += dx;
                    ir.position[1] += dy;
                    ir.name = snap.name;
                    ir.plugin_id = snap.plugin_id;
                    ir.plugin_name = snap.plugin_name;
                    ir.plugin_path = snap.plugin_path;
                    let nid = new_id();
                    self.instrument_regions.insert(nid, ir);
                    new_selected.push(HitTarget::InstrumentRegion(nid));
                }
            }
        }

        // Build ops from pasted entities
        let mut paste_ops: Vec<operations::Operation> = Vec::new();
        for t in &new_selected {
            match t {
                HitTarget::Object(id) => { if let Some(d) = self.objects.get(id) { paste_ops.push(operations::Operation::CreateObject { id: *id, data: d.clone() }); } }
                HitTarget::Waveform(id) => { if let Some(d) = self.waveforms.get(id) { let ac = self.audio_clips.get(id).cloned(); paste_ops.push(operations::Operation::CreateWaveform { id: *id, data: d.clone(), audio_clip: ac.map(|c| (*id, c)) }); } }
                HitTarget::EffectRegion(id) => { if let Some(d) = self.effect_regions.get(id) { paste_ops.push(operations::Operation::CreateEffectRegion { id: *id, data: d.clone() }); } }
                HitTarget::PluginBlock(id) => { if let Some(d) = self.plugin_blocks.get(id) { paste_ops.push(operations::Operation::CreatePluginBlock { id: *id, data: d.snapshot() }); } }
                HitTarget::LoopRegion(id) => { if let Some(d) = self.loop_regions.get(id) { paste_ops.push(operations::Operation::CreateLoopRegion { id: *id, data: d.clone() }); } }
                HitTarget::ExportRegion(id) => { if let Some(d) = self.export_regions.get(id) { paste_ops.push(operations::Operation::CreateExportRegion { id: *id, data: d.clone() }); } }
                HitTarget::ComponentDef(id) => { if let Some(d) = self.components.get(id) { paste_ops.push(operations::Operation::CreateComponent { id: *id, data: d.clone() }); } }
                HitTarget::ComponentInstance(id) => { if let Some(d) = self.component_instances.get(id) { paste_ops.push(operations::Operation::CreateComponentInstance { id: *id, data: d.clone() }); } }
                HitTarget::MidiClip(id) => { if let Some(d) = self.midi_clips.get(id) { paste_ops.push(operations::Operation::CreateMidiClip { id: *id, data: d.clone() }); } }
                HitTarget::InstrumentRegion(id) => { if let Some(ir) = self.instrument_regions.get(id) { let snap = instruments::InstrumentRegionSnapshot { position: ir.position, size: ir.size, name: ir.name.clone(), plugin_id: ir.plugin_id.clone(), plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone() }; paste_ops.push(operations::Operation::CreateInstrumentRegion { id: *id, data: snap }); } }
            }
        }
        let pasted_wf_ids: Vec<EntityId> = new_selected.iter()
            .filter_map(|t| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
            .collect();
        if !pasted_wf_ids.is_empty() {
            let overlap_ops = self.resolve_waveform_overlaps(&pasted_wf_ids);
            paste_ops.extend(overlap_ops);
        }
        if !paste_ops.is_empty() {
            self.push_op(operations::Operation::Batch(paste_ops));
        }
        self.selected = new_selected;
        self.sync_audio_clips();
    }

    fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut del_ops: Vec<operations::Operation> = Vec::new();
        let obj_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::Object(i) => Some(*i), _ => None }).collect();
        let wf_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::Waveform(i) => Some(*i), _ => None }).collect();
        let er_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::EffectRegion(i) => Some(*i), _ => None }).collect();
        let pb_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::PluginBlock(i) => Some(*i), _ => None }).collect();
        let lr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::LoopRegion(i) => Some(*i), _ => None }).collect();
        let xr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ExportRegion(i) => Some(*i), _ => None }).collect();
        let comp_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentDef(i) => Some(*i), _ => None }).collect();
        let inst_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentInstance(i) => Some(*i), _ => None }).collect();
        let mc_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::MidiClip(i) => Some(*i), _ => None }).collect();
        let ir_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::InstrumentRegion(i) => Some(*i), _ => None }).collect();

        #[cfg(feature = "native")]
        if !ir_ids.is_empty() {
            let removed_ids: HashSet<_> = ir_ids.iter().copied().collect();
            if let Some(engine) = &self.audio_engine {
                self.midi_keyboard_held.retain(|_, (target, note)| {
                    if removed_ids.contains(target) {
                        engine.keyboard_preview_note_off(*target, *note);
                        false
                    } else {
                        true
                    }
                });
            } else {
                self.midi_keyboard_held
                    .retain(|_, (target, _)| !removed_ids.contains(target));
            }
        }

        // Capture before removing
        for &id in &inst_ids {
            if let Some(d) = self.component_instances.get(&id) { del_ops.push(operations::Operation::DeleteComponentInstance { id, data: d.clone() }); }
            self.component_instances.shift_remove(&id);
        }
        for &id in &comp_ids {
            if let Some(comp) = self.components.shift_remove(&id) {
                del_ops.push(operations::Operation::DeleteComponent { id, data: comp.clone() });
                self.component_instances.retain(|_, inst| inst.component_id != comp.id);
                for &wi in &comp.waveform_ids {
                    if let Some(wf) = self.waveforms.get(&wi) {
                        let ac = self.audio_clips.get(&wi).cloned();
                        del_ops.push(operations::Operation::DeleteWaveform { id: wi, data: wf.clone(), audio_clip: ac.map(|c| (wi, c)) });
                    }
                    self.waveforms.shift_remove(&wi);
                    self.audio_clips.shift_remove(&wi);
                }
            }
        }
        for &id in &obj_ids { if let Some(d) = self.objects.get(&id) { del_ops.push(operations::Operation::DeleteObject { id, data: d.clone() }); } self.objects.shift_remove(&id); }
        for &id in &wf_ids {
            if let Some(d) = self.waveforms.get(&id) { let ac = self.audio_clips.get(&id).cloned(); del_ops.push(operations::Operation::DeleteWaveform { id, data: d.clone(), audio_clip: ac.map(|c| (id, c)) }); }
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }
        for &id in &er_ids { if let Some(d) = self.effect_regions.get(&id) { del_ops.push(operations::Operation::DeleteEffectRegion { id, data: d.clone() }); } self.effect_regions.shift_remove(&id); }
        for &id in &pb_ids { if let Some(d) = self.plugin_blocks.get(&id) { del_ops.push(operations::Operation::DeletePluginBlock { id, data: d.snapshot() }); } self.plugin_blocks.shift_remove(&id); }
        for &id in &lr_ids { if let Some(d) = self.loop_regions.get(&id) { del_ops.push(operations::Operation::DeleteLoopRegion { id, data: d.clone() }); } self.loop_regions.shift_remove(&id); }
        for &id in &xr_ids { if let Some(d) = self.export_regions.get(&id) { del_ops.push(operations::Operation::DeleteExportRegion { id, data: d.clone() }); } self.export_regions.shift_remove(&id); }
        for &id in &mc_ids { if let Some(d) = self.midi_clips.get(&id) { del_ops.push(operations::Operation::DeleteMidiClip { id, data: d.clone() }); } self.midi_clips.shift_remove(&id); }
        // Cascade-delete child MIDI clips when deleting instrument regions
        if !ir_ids.is_empty() {
            let ir_set: std::collections::HashSet<EntityId> = ir_ids.iter().copied().collect();
            let child_mc_ids: Vec<EntityId> = self.midi_clips.iter()
                .filter(|(_, mc)| mc.instrument_region_id.map_or(false, |iid| ir_set.contains(&iid)))
                .map(|(&id, _)| id)
                .collect();
            for id in child_mc_ids {
                if let Some(d) = self.midi_clips.get(&id) { del_ops.push(operations::Operation::DeleteMidiClip { id, data: d.clone() }); }
                self.midi_clips.shift_remove(&id);
            }
        }
        for &id in &ir_ids { if let Some(ir) = self.instrument_regions.get(&id) { let snap = instruments::InstrumentRegionSnapshot { position: ir.position, size: ir.size, name: ir.name.clone(), plugin_id: ir.plugin_id.clone(), plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone() }; del_ops.push(operations::Operation::DeleteInstrumentRegion { id, data: snap }); } self.instrument_regions.shift_remove(&id); }
        if !del_ops.is_empty() {
            self.push_op(operations::Operation::Batch(del_ops));
        }

        self.selected.clear();
        #[cfg(feature = "native")]
        {
            self.sync_keyboard_instrument_from_selection();
            self.sync_computer_keyboard_to_engine();
            self.sync_audio_clips();
            self.sync_loop_region();
        }
        println!("Deleted selected items");
    }

    /// Spawn audio loading on a background thread. A placeholder waveform
    /// (empty audio) is placed on the canvas immediately so the user sees
    /// feedback. When decoding finishes the placeholder is filled in by
    /// `poll_pending_audio_loads`.
    #[cfg(feature = "native")]
    fn drop_audio_from_browser(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            return;
        }

        let world = self.camera.screen_to_world(self.mouse_pos);
        let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
        let color = WAVEFORM_COLORS[color_idx];
        let snap_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
        let wf_id = new_id();
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let height = grid::clip_height(self.bpm);
        // Probe header for duration so placeholder has correct width.
        let placeholder_width = audio::probe_audio_duration(&path)
            .map(|(_, w)| w)
            .unwrap_or(200.0);

        // Insert an empty placeholder waveform on the canvas immediately.
        let placeholder = WaveformView {
            audio: Arc::new(AudioData {
                left_samples: Arc::new(Vec::new()),
                right_samples: Arc::new(Vec::new()),
                left_peaks: Arc::new(WaveformPeaks::empty()),
                right_peaks: Arc::new(WaveformPeaks::empty()),
                sample_rate: 0,
                filename: filename.clone(),
            }),
            filename: filename.clone(),
            position: [snap_x, world[1] - height * 0.5],
            size: [placeholder_width, height],
            color,
            border_radius: 8.0,
            fade_in_px: 0.0,
            fade_out_px: 0.0,
            fade_in_curve: 0.0,
            fade_out_curve: 0.0,
            volume: 1.0,
            pan: 0.5,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: self.bpm,
            pitch_semitones: 0.0,
            disabled: true, // disabled until loaded
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
        };
        self.waveforms.insert(wf_id, placeholder);
        self.pending_audio_loads_count += 1;
        self.mark_dirty();

        let auto_fade_px = if self.settings.auto_clip_fades { ui::waveform::DEFAULT_AUTO_FADE_PX } else { 0.0 };
        let project_bpm = self.bpm;
        let path = path.to_owned();
        let tx = self.pending_audio_tx.clone();
        let rs = self.remote_storage.clone();

        std::thread::spawn(move || {
            let Some(loaded) = load_audio_file(&path) else {
                eprintln!("Failed to load audio: {}", path.display());
                let _ = tx.send(PendingAudioLoad::Failed { wf_id });
                return;
            };

            println!(
                "  Loaded: {} ({:.1}s, {} Hz, {} samples/ch)",
                filename, loaded.duration_secs, loaded.sample_rate, loaded.left_samples.len(),
            );

            let left_peaks = Arc::new(WaveformPeaks::build(&loaded.left_samples));
            let right_peaks = Arc::new(WaveformPeaks::build(&loaded.right_samples));

            let wf_data = WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: loaded.left_samples.clone(),
                    right_samples: loaded.right_samples.clone(),
                    left_peaks: left_peaks.clone(),
                    right_peaks: right_peaks.clone(),
                    sample_rate: loaded.sample_rate,
                    filename: filename.clone(),
                }),
                filename,
                position: [snap_x, world[1] - height * 0.5],
                size: [loaded.width, height],
                color,
                border_radius: 8.0,
                fade_in_px: auto_fade_px,
                fade_out_px: auto_fade_px,
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                pan: 0.5,
                warp_mode: ui::waveform::WarpMode::Off,
                sample_bpm: project_bpm,
                pitch_semitones: 0.0,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            };
            let ac_data = AudioClipData {
                samples: loaded.samples.clone(),
                sample_rate: loaded.sample_rate,
                duration_secs: loaded.duration_secs,
            };

            if let Some(rs) = &rs {
                // Remote storage mode: defer waveform display until upload completes.
                // Do NOT send Decoded — keep the placeholder visible with "uploading..." label.
                let wf_id_str = wf_id.to_string();
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("wav")
                    .to_string();
                let save_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Ok(file_bytes) = std::fs::read(&path) {
                        rs.save_audio(&wf_id_str, &file_bytes, &ext);
                    } else {
                        eprintln!("[BgAudioLoad] Failed to re-read file for remote save: {}", path.display());
                    }
                }));
                match save_result {
                    Ok(()) => {
                        println!("[BgAudioLoad] Remote save done for {wf_id}, sending SyncReady");
                        let _ = tx.send(PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data });
                    }
                    Err(e) => {
                        eprintln!("[BgAudioLoad] Remote save PANICKED for {wf_id}: {e:?}");
                        // Still send SyncReady so the op gets pushed (data may be missing on remote)
                        let _ = tx.send(PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data });
                    }
                }
            } else {
                // Local-only mode: show waveform immediately after decode.
                let _ = tx.send(PendingAudioLoad::Decoded {
                    wf_id,
                    wf_data,
                    ac_data,
                });
            }
        });
    }

    /// Called each frame to finalize any background audio loads.
    /// Replaces placeholder waveforms with the fully-decoded version.
    #[cfg(feature = "native")]
    fn poll_pending_audio_loads(&mut self) {
        let mut any = false;
        while let Ok(load) = self.pending_audio_rx.try_recv() {
            match load {
                PendingAudioLoad::Decoded { wf_id, wf_data, ac_data } => {
                    self.waveforms.insert(wf_id, wf_data.clone());
                    self.audio_clips.insert(wf_id, ac_data.clone());
                    let mut ops = vec![operations::Operation::CreateWaveform {
                        id: wf_id,
                        data: wf_data,
                        audio_clip: Some((wf_id, ac_data)),
                    }];
                    let overlap_ops = self.resolve_waveform_overlaps(&[wf_id]);
                    ops.extend(overlap_ops);
                    self.push_op(operations::Operation::Batch(ops));
                    self.pending_audio_loads_count = self.pending_audio_loads_count.saturating_sub(1);
                }
                PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data } => {
                    self.waveforms.insert(wf_id, wf_data.clone());
                    self.audio_clips.insert(wf_id, ac_data.clone());
                    let mut ops = vec![operations::Operation::CreateWaveform {
                        id: wf_id,
                        data: wf_data,
                        audio_clip: Some((wf_id, ac_data)),
                    }];
                    let overlap_ops = self.resolve_waveform_overlaps(&[wf_id]);
                    ops.extend(overlap_ops);
                    self.push_op(operations::Operation::Batch(ops));
                    self.pending_audio_loads_count = self.pending_audio_loads_count.saturating_sub(1);
                }
                PendingAudioLoad::Failed { wf_id } => {
                    // Load failed — remove the placeholder.
                    self.waveforms.swap_remove(&wf_id);
                    self.toast_manager.push(
                        "Failed to load audio file".to_string(),
                        ui::toast::ToastKind::Error,
                    );
                    self.pending_audio_loads_count = self.pending_audio_loads_count.saturating_sub(1);
                }
            }
            any = true;
        }
        while let Ok(fetch) = self.pending_remote_audio_rx.try_recv() {
            if let Some(wf) = self.waveforms.get_mut(&fetch.wf_id) {
                wf.audio = fetch.audio;
            }
            self.audio_clips.insert(fetch.wf_id, fetch.ac);
            any = true;
            log::info!("[SYNC] Applied remote audio fetch for waveform {}", fetch.wf_id);
        }
        if any {
            self.sync_audio_clips();
            self.mark_dirty();
            self.request_redraw();
        }
    }

}

// ---------------------------------------------------------------------------
// Native macOS menu bar
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
fn build_app_menu(storage: Option<&Storage>) -> MenuState {
    use muda::{
        accelerator::{Accelerator, Code, Modifiers},
        Menu, MenuItem, PredefinedMenuItem, Submenu,
    };

    // Cmd on macOS, Ctrl on Windows/Linux
    let cmd = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };

    let menu = Menu::new();

    // -- App menu (Layers) --
    let app_menu = Submenu::new("Layers", true);
    let _ = app_menu.append(&PredefinedMenuItem::about(None, None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let settings_item = MenuItem::new(
        "Settings...",
        true,
        Some(Accelerator::new(Some(cmd), Code::Comma)),
    );
    let _ = app_menu.append(&settings_item);
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let _ = app_menu.append(&PredefinedMenuItem::quit(None));
    let _ = menu.append(&app_menu);

    // -- File menu --
    let file_menu = Submenu::new("File", true);
    let new_project_item = MenuItem::new(
        "New Project",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyN)),
    );
    let _ = file_menu.append(&new_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let save_project_item = MenuItem::new(
        "Save Project",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyS)),
    );
    let _ = file_menu.append(&save_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());

    let open_project_item = MenuItem::new(
        "Open Project...",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyO)),
    );
    let _ = file_menu.append(&open_project_item);

    let open_submenu = Submenu::new("Open Recent", true);
    let mut open_items: Vec<(MenuId, String)> = Vec::new();
    if let Some(s) = storage {
        for entry in s.list_projects() {
            if entry.is_temp {
                continue;
            }
            let exists = std::path::Path::new(&entry.path).exists();
            let item = MenuItem::new(&entry.name, exists, None);
            if exists {
                open_items.push((item.id().clone(), entry.path.clone()));
            }
            let _ = open_submenu.append(&item);
        }
    }
    if open_items.is_empty() {
        let _ = open_submenu.append(&MenuItem::new("No Projects", false, None));
    }
    let _ = file_menu.append(&open_submenu);
    let _ = menu.append(&file_menu);

    // -- Edit menu --
    let edit_menu = Submenu::new("Edit", true);
    let undo_item = MenuItem::new(
        "Undo",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyZ)),
    );
    let redo_item = MenuItem::new(
        "Redo",
        true,
        Some(Accelerator::new(
            Some(cmd | Modifiers::SHIFT),
            Code::KeyZ,
        )),
    );
    let copy_item = MenuItem::new(
        "Copy",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyC)),
    );
    let paste_item = MenuItem::new(
        "Paste",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyV)),
    );
    let select_all_item = MenuItem::new(
        "Select All",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyA)),
    );
    let _ = edit_menu.append(&undo_item);
    let _ = edit_menu.append(&redo_item);
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&copy_item);
    let _ = edit_menu.append(&paste_item);
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&select_all_item);
    let _ = menu.append(&edit_menu);

    MenuState {
        menu,
        new_project: new_project_item.id().clone(),
        save_project: save_project_item.id().clone(),
        open_project: open_project_item.id().clone(),
        settings: settings_item.id().clone(),
        undo: undo_item.id().clone(),
        redo: redo_item.id().clone(),
        copy: copy_item.id().clone(),
        paste: paste_item.id().clone(),
        select_all: select_all_item.id().clone(),
        open_project_items: open_items,
        open_submenu,
        initialized: false,
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
fn main() {
    env_logger::init();

    let m = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
    println!("╔════════════════════════════════════════════╗");
    println!("║              Layers                         ║");
    println!("╠════════════════════════════════════════════╣");
    println!("║  Space              →  Play / Pause        ║");
    println!("║  Click background   →  Seek playhead       ║");
    println!("║  Drop audio file    →  Add to canvas       ║");
    println!("║  Two-finger scroll  →  Pan canvas          ║");
    println!("║  {} + scroll       →  Zoom in/out         ║", m);
    println!("║  Pinch              →  Zoom in/out         ║");
    println!("║  Middle drag        →  Pan canvas          ║");
    println!("║  Left drag empty    →  Selection rectangle ║");
    println!("║  Left drag object   →  Move (+ selection)  ║");
    println!("║  {} + K / Right-click → Command palette   ║", m);
    println!("║  Backspace / Delete →  Delete selected     ║");
    println!("║  {} + Z / Shift+Z  →  Undo / Redo         ║", m);
    println!("║  {} + S            →  Save project        ║", m);
    println!("║  {} + B            →  Toggle browser      ║", m);
    println!("║  {} + Shift + A    →  Add folder           ║", m);
    println!("╚════════════════════════════════════════════╝");

    let skip_load = std::env::args().any(|a| a == "--empty");

    let db_url = std::env::args()
        .position(|a| a == "--db-url")
        .and_then(|i| std::env::args().nth(i + 1));

    let project_id = std::env::args()
        .position(|a| a == "--project")
        .and_then(|i| std::env::args().nth(i + 1));

    let event_loop = EventLoop::new().unwrap();

    let mut app = App::new(skip_load);
    let menu_state = build_app_menu(app.storage.as_ref());
    app.menu_state = Some(menu_state);

    if let Some(url) = &db_url {
        let pid = project_id.as_deref().unwrap_or("default");

        // Remote storage connection (separate SurrealDB connection for audio)
        let rt = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for remote storage"),
        );
        if let Some(rs) = storage::RemoteStorage::connect(url, rt) {
            rs.use_project(pid);
            println!("[RemoteStorage] Connected to {url}, project '{pid}'");
            app.remote_storage = Some(Arc::new(rs));
        } else {
            eprintln!("[RemoteStorage] Failed to connect to {url}");
        }

        // Real-time sync via SurrealDB live queries
        app.connect_to_server(url, pid);
    }

    event_loop.run_app(&mut app).unwrap();
}

#[cfg(not(feature = "native"))]
fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        console_log::init_with_level(log::Level::Info).ok();
        log::info!("Layers WASM starting...");

        let event_loop = EventLoop::new().unwrap();
        let app = App::new_web();

        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("This binary requires the 'native' feature. For web, build with --target wasm32-unknown-unknown.");
    }
}
