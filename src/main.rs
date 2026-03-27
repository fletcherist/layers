#[cfg(feature = "native")]
mod project;
#[cfg(feature = "native")]
mod audio;
#[cfg(feature = "native")]
mod export;
mod automation;
mod clipboard;
mod component;
mod effects;
mod entity_id;
mod events;
mod gpu;
mod grid;
mod group;
mod icons;
mod history;
mod instruments;
mod layers;
mod master;
mod midi;
mod midi_keyboard;
mod network;
mod operations;
mod overlap;
#[cfg(feature = "native")]
mod paulstretch;
#[cfg(feature = "native")]
mod warp;
#[cfg(feature = "native")]
mod surreal_client;
#[cfg(feature = "native")]
mod plugins;
mod regions;
mod settings;
mod share_id;
mod takes;
mod text_note;
pub mod theme;
mod storage;
mod types;
mod ui;
mod user;

pub(crate) use types::*;

#[cfg(test)]
mod tests;

// Time compatibility: use web-time on WASM, std::time on native
#[cfg(target_arch = "wasm32")]
use web_time::Instant as TimeInstant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant as TimeInstant;

pub(crate) use gpu::{push_border, Camera, Gpu, InstanceRaw};
pub(crate) use ui::transport::TransportPanel;

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
use ui::rendering::{build_instances, build_waveform_vertices, RenderContext};

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

use settings::Settings;
#[cfg(feature = "native")]
use ui::settings_window::{SettingsWindow, CATEGORIES};
#[cfg(feature = "native")]
use storage::{default_base_path, ProjectStore, Storage};
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
type NativeMenuState = project::MenuState;
#[cfg(not(feature = "native"))]
type NativeMenuState = ();

#[cfg(feature = "native")]
type NativePendingRemoteAudioFetch = project::PendingRemoteAudioFetch;
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

#[cfg(feature = "native")]
type NativeSyncCompleteReceiver = tokio::sync::oneshot::Receiver<()>;
#[cfg(not(feature = "native"))]
type NativeSyncCompleteReceiver = ();

use automation::{AutomationData, AutomationParam};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(crate) use crate::theme::WAVEFORM_COLORS;

// Audio formats supported via symphonia: wav, mp3, ogg, flac, aac
const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "aac", "m4a", "mp4"];
const SESSION_DB_WS_URL: &str = "ws://db.layers.audio";
const SESSION_DB_HOST: &str = "db.layers.audio";

/// Strip known URL prefixes from a session address, return the bare project ID.
pub(crate) fn parse_session_id(input: &str) -> String {
    let s = input.trim();
    for prefix in &[
        "https://layers.audio/projects/",
        "http://layers.audio/projects/",
        "layers.audio/projects/",
        "wss://db.layers.audio:8000/",
        "https://db.layers.audio/",
        "http://db.layers.audio/",
        "db.layers.audio/",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            // base62 IDs are case-sensitive; legacy IDs use lowercase
            let rest = rest.trim_end_matches('/');
            if rest.chars().all(|c| c.is_ascii_alphanumeric()) && rest.len() == 22 {
                // Looks like a base62 share ID — preserve case
                return rest.to_string();
            }
            return rest.to_lowercase();
        }
    }
    s.to_lowercase()
}

pub(crate) const MIDI_AUTO_EDIT_ZOOM_THRESHOLD: f32 = 2.0;

pub(crate) fn share_button_rect(screen_w: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
    let w = 72.0 * scale;
    let h = 28.0 * scale;
    let x = screen_w - w - 12.0 * scale;
    let y = 10.0 * scale;
    ([x, y], [w, h])
}

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


/// Result of decoding an audio file on a background thread.
/// The wf_id refers to a placeholder waveform already visible on canvas.
enum PendingAudioLoad {
    /// Audio decoded — update local display immediately.
    Decoded {
        wf_id: EntityId,
        wf_data: WaveformView,
        ac_data: AudioClipData,
        source_file: (Vec<u8>, String),
    },
    /// Remote storage save finished — safe to push op to network now.
    /// Carries the decoded audio data so it can be applied at this point.
    SyncReady {
        wf_id: EntityId,
        wf_data: WaveformView,
        ac_data: AudioClipData,
        source_file: (Vec<u8>, String),
    },
    /// Load failed — remove placeholder.
    Failed { wf_id: EntityId },
    /// Browser preview sample loaded.
    PreviewLoaded {
        path: std::path::PathBuf,
        audio: std::sync::Arc<ui::waveform::AudioData>,
        left_samples: std::sync::Arc<Vec<f32>>,
        right_samples: std::sync::Arc<Vec<f32>>,
        sample_rate: u32,
    },
}

#[cfg(feature = "native")]
struct PaulStretchCache {
    original_audio: Arc<ui::waveform::AudioData>,
    original_clip: ui::waveform::AudioClipData,
    original_width: f32,
    factor: f32,
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
    /// Original encoded audio file bytes + extension per waveform (for saving).
    source_audio_files: IndexMap<EntityId, (Vec<u8>, String)>,
    audio_engine: Option<NativeAudioEngine>,
    recorder: Option<NativeAudioRecorder>,
    recording_waveform_id: Option<EntityId>,
    /// If recording into a take group, this is the parent waveform ID.
    recording_take_parent_id: Option<EntityId>,
    input_monitoring: bool,
    last_canvas_click_world: [f32; 2],
    selected: Vec<HitTarget>,
    drag: DragState,
    mouse_pos: [f32; 2],
    hovered: Option<HitTarget>,
    fade_handle_hovered: Option<(EntityId, bool)>,
    fade_curve_hovered: Option<(EntityId, bool)>,
    waveform_edge_hover: WaveformEdgeHover,
    midi_note_edge_hover: bool,
    midi_clip_edge_hover_ew: bool,
    midi_clip_edge_hover_ns: bool,
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
    effect_chains: IndexMap<EntityId, effects::EffectChain>,
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
    pub(crate) master: master::Master,
    groups: IndexMap<EntityId, group::Group>,
    group_hover: GroupHover,
    text_notes: IndexMap<EntityId, text_note::TextNote>,
    text_note_hover: TextNoteHover,
    editing_text_note: Option<text_note::TextNoteEditState>,
    midi_clips: IndexMap<EntityId, midi::MidiClip>,
    instruments: IndexMap<EntityId, instruments::Instrument>,
    editing_midi_clip: Option<EntityId>,
    selected_midi_notes: Vec<usize>,
    pending_midi_note_click: Option<usize>,
    midi_note_select_rect: Option<[f32; 4]>,
    cmd_velocity_hover_note: Option<(EntityId, usize)>,
    editing_component: Option<EntityId>,
    editing_group: Option<EntityId>,
    editing_waveform_name: Option<(EntityId, String)>,
    bpm: f32,
    editing_bpm: ui::value_entry::ValueEntry,
    dragging_bpm: Option<(f32, f32)>,
    bpm_drag_overlap_snapshots: IndexMap<EntityId, WaveformView>,
    bpm_drag_overlap_temp_splits: Vec<EntityId>,
    last_click_time: TimeInstant,
    last_browser_click_time: TimeInstant,
    last_browser_click_idx: Option<usize>,
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
    export_window: Option<ui::export_window::ExportWindow>,
    share_window: Option<ui::share_window::ShareWindow>,
    share_button_hovered: bool,
    /// Base62 share ID for this project — generated on first Share click.
    share_id: Option<String>,
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
    connect_password: Option<String>,
    pending_welcome: Option<NativeWelcomeReceiver>,
    pending_sync_complete: Option<NativeSyncCompleteReceiver>,
    reconnect_attempt: u32,
    last_reconnect_time: Option<TimeInstant>,
    cached_instances: Vec<InstanceRaw>,
    cached_wf_verts: Vec<WaveformVertex>,
    cached_preview_wf_verts: Vec<WaveformVertex>,
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
    /// Transient solo state — not persisted, not in undo history.
    pub(crate) solo_ids: std::collections::HashSet<EntityId>,
    /// Follow mode: when set, camera and playback sync to this remote user.
    following_user: Option<user::UserId>,
    /// Track which plugin GUIs were open last frame for close detection.
    open_plugin_guis: std::collections::HashSet<(entity_id::EntityId, usize)>,
    /// Track which instrument GUIs were open last frame for close detection.
    open_instrument_guis: std::collections::HashSet<entity_id::EntityId>,
    /// Cache for PaulStretch: stores original audio data so we can restore when switching modes.
    #[cfg(feature = "native")]
    paulstretch_cache: IndexMap<EntityId, PaulStretchCache>,
}

/// Whether an entity should be audible, considering solo/mute and group membership.
/// Shared logic used by both audio routing (`App::should_play`) and rendering (`is_dimmed_by_solo_mute`).
/// Mute is now handled via entity `disabled` fields, so this only checks solo logic.
pub(crate) fn is_entity_audible(
    id: EntityId,
    solo_ids: &std::collections::HashSet<EntityId>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) -> bool {
    // Check group disabled — members of a disabled group should not play
    for group in groups.values() {
        if group.member_ids.contains(&id) && group.disabled {
            return false;
        }
    }
    // If no solos active, play everything
    if solo_ids.is_empty() {
        return true;
    }
    // Check direct solo
    if solo_ids.contains(&id) {
        return true;
    }
    // Check group solo — members of a soloed group should play
    for group in groups.values() {
        if group.member_ids.contains(&id) && solo_ids.contains(&group.id) {
            return true;
        }
    }
    false
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
            source_audio_files: IndexMap::new(),
            audio_engine: None,
            recorder: None,
            recording_waveform_id: None,
            recording_take_parent_id: None,
            input_monitoring: false,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            fade_handle_hovered: None,
            fade_curve_hovered: None,
            waveform_edge_hover: WaveformEdgeHover::None,
            midi_note_edge_hover: false,
            midi_clip_edge_hover_ew: false,
            midi_clip_edge_hover_ns: false,
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
            effect_chains: IndexMap::new(),
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
            master: master::Master::default(),
            groups: IndexMap::new(),
            group_hover: GroupHover::None,
            text_notes: IndexMap::new(),
            text_note_hover: TextNoteHover::None,
            editing_text_note: None,
            midi_clips: IndexMap::new(),
            instruments: IndexMap::new(),
            editing_midi_clip: None,
            selected_midi_notes: Vec::new(),
            pending_midi_note_click: None,
            midi_note_select_rect: None,
            cmd_velocity_hover_note: None,
            editing_component: None,
            editing_group: None,
            editing_waveform_name: None,
            bpm: 120.0,
            editing_bpm: ui::value_entry::ValueEntry::new(),
            dragging_bpm: None,
            bpm_drag_overlap_snapshots: IndexMap::new(),
            bpm_drag_overlap_temp_splits: Vec::new(),
            last_click_time: TimeInstant::now(),
            last_browser_click_time: TimeInstant::now(),
            last_browser_click_idx: None,
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
            export_window: None,
            share_window: None,
            share_button_hovered: false,
            share_id: None,
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
            connect_password: None,
            pending_welcome: None,
            pending_sync_complete: None,
            reconnect_attempt: 0,
            last_reconnect_time: None,
            cached_instances: Vec::new(),
            cached_wf_verts: Vec::new(),
            cached_preview_wf_verts: Vec::new(),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
            computer_keyboard_armed: true,
            computer_keyboard_octave_offset: 0,
            computer_keyboard_velocity: midi_keyboard::DEFAULT_VELOCITY,
            keyboard_instrument_id: None,
            midi_keyboard_held: HashMap::new(),
            solo_ids: std::collections::HashSet::new(),
            following_user: None,
            open_plugin_guis: std::collections::HashSet::new(),
            open_instrument_guis: std::collections::HashSet::new(),
            #[cfg(feature = "native")]
            paulstretch_cache: IndexMap::new(),
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

    /// Returns true if `entity_id` belongs to a group that is NOT currently entered.
    fn is_in_non_entered_group(&self, entity_id: &EntityId) -> bool {
        for (gid, group) in &self.groups {
            if group.member_ids.contains(entity_id) {
                return self.editing_group != Some(*gid);
            }
        }
        false
    }

    /// If `target` is a member of a non-entered group, return the group's HitTarget instead.
    fn redirect_to_group(&self, target: HitTarget) -> HitTarget {
        let entity_id = match target {
            HitTarget::Waveform(id) | HitTarget::MidiClip(id)
            | HitTarget::TextNote(id) | HitTarget::Object(id) | HitTarget::LoopRegion(id)
            | HitTarget::ExportRegion(id) | HitTarget::ComponentDef(id)
            | HitTarget::ComponentInstance(id) => id,
            HitTarget::Group(_) | HitTarget::Instrument(_) => return target,
        };
        for (gid, group) in &self.groups {
            if group.member_ids.contains(&entity_id) && self.editing_group != Some(*gid) {
                return HitTarget::Group(*gid);
            }
        }
        target
    }

    /// If shift is held, toggle `target` in selection; otherwise clear and set.
    /// Returns whether the target is now selected.
    pub(crate) fn select_with_shift(&mut self, target: HitTarget, shift: bool) -> bool {
        if shift {
            if let Some(pos) = self.selected.iter().position(|t| *t == target) {
                self.selected.remove(pos);
                false
            } else {
                self.selected.push(target);
                true
            }
        } else {
            if !self.selected.contains(&target) {
                self.selected.clear();
                self.selected.push(target);
            }
            true
        }
    }

    /// Enrich a selection with instruments whose paired MIDI clips are already selected.
    pub(crate) fn include_paired_instruments(&mut self) {
        let mut to_add = Vec::new();
        for t in self.selected.iter() {
            if let HitTarget::MidiClip(mc_id) = t {
                if let Some(mc) = self.midi_clips.get(mc_id) {
                    if let Some(inst_id) = mc.instrument_id {
                        let inst_target = HitTarget::Instrument(inst_id);
                        if !self.selected.contains(&inst_target) && !to_add.contains(&inst_target) {
                            to_add.push(inst_target);
                        }
                    }
                }
            }
        }
        self.selected.extend(to_add);
    }

    /// Map each target through `redirect_to_group` and deduplicate, preserving order.
    fn normalize_group_selection(&self, targets: Vec<HitTarget>) -> Vec<HitTarget> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for t in targets {
            let redirected = self.redirect_to_group(t);
            if seen.insert(redirected) {
                out.push(redirected);
            }
        }
        out
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
            &self.instruments,
            &self.midi_clips,
            &self.waveforms,
            &self.groups,
        );
        let rows = layers::flatten_tree(
            &self.layer_tree,
            &self.instruments,
            &self.midi_clips,
            &self.waveforms,
            &self.groups,
            &self.solo_ids,
        );
        self.sample_browser.layer_rows = rows;
        if self.sample_browser.active_category == ui::browser::BrowserCategory::Layers {
            self.sample_browser.rebuild_entries();
        }
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
        for group in self.groups.values_mut() {
            group.position[0] *= scale;
            group.position[1] *= scale;
            group.size[0] *= scale;
            group.size[1] *= scale;
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
                ui::waveform::WarpMode::Complex => {
                    if let Some(clip) = self.audio_clips.get(&wf_id) {
                        let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                        wf.size[0] = original_duration_px * (wf.sample_bpm / self.bpm);
                    }
                }
                ui::waveform::WarpMode::Off => {}
                ui::waveform::WarpMode::PaulStretch => {}
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
            stored_loop_regions,
            stored_components,
            stored_component_instances,
            audio_clips,
            source_audio_files,
            loaded_bpm,
            stored_midi_clips,
            stored_layer_tree,
            restored_text_notes,
        ) = match loaded {
            Some(state) => {
                println!(
                    "  Loaded project '{}' ({} objects, {} waveforms)",
                    state.name,
                    state.objects.len(),
                    state.waveforms.len(),
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
                let bw = state.browser_width;
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
                        warp_mode: match sw.warp_mode {
                            1 => ui::waveform::WarpMode::RePitch,
                            2 => ui::waveform::WarpMode::Semitone,
                            3 => ui::waveform::WarpMode::PaulStretch,
                            4 => ui::waveform::WarpMode::Complex,
                            _ => ui::waveform::WarpMode::Off,
                        },
                        sample_bpm: if sw.sample_bpm > 0.0 { sw.sample_bpm } else if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
                        pitch_semitones: sw.pitch_semitones,
                        paulstretch_factor: if sw.paulstretch_factor > 0.0 { sw.paulstretch_factor } else { 8.0 },
                        is_reversed: false,
                        disabled: sw.disabled,
                        sample_offset_px: sw.sample_offset_px,
                        automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
                        effect_chain_id: None,
                        take_group: if sw.take_group_json.is_empty() {
                            None
                        } else {
                            serde_json::from_str(&sw.take_group_json).ok()
                        },
                    }))
                    .collect();

                // Restore audio data and peaks from DB
                let mut audio_clips: IndexMap<EntityId, AudioClipData> = IndexMap::new();
                let mut source_audio_files: IndexMap<EntityId, (Vec<u8>, String)> = IndexMap::new();
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

                        if let Some((file_bytes, ext)) = s.load_audio(&id_str) {
                            if let Some(loaded) = crate::audio::load_audio_from_bytes(&file_bytes, &ext) {
                                left_samples = loaded.left_samples;
                                right_samples = loaded.right_samples;
                                sample_rate = loaded.sample_rate;
                                audio_clips.insert(*wf_id, AudioClipData {
                                    samples: loaded.samples,
                                    sample_rate: loaded.sample_rate,
                                    duration_secs: loaded.duration_secs,
                                });
                            } else {
                                audio_clips.insert(*wf_id, AudioClipData {
                                    samples: Arc::new(Vec::new()),
                                    sample_rate: 48000,
                                    duration_secs: 0.0,
                                });
                            }
                            source_audio_files.insert(*wf_id, (file_bytes, ext));
                        } else {
                            audio_clips.insert(*wf_id, AudioClipData {
                                samples: Arc::new(Vec::new()),
                                sample_rate: 48000,
                                duration_secs: 0.0,
                            });
                        }
                        if let Some((block_size, lp, rp)) = s.load_peaks(&id_str) {
                            left_peaks =
                                Arc::new(WaveformPeaks::from_raw(block_size as usize, lp));
                            right_peaks =
                                Arc::new(WaveformPeaks::from_raw(block_size as usize, rp));
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
                    storage::loop_regions_from_stored(state.loop_regions),
                    storage::components_from_stored(state.components),
                    storage::component_instances_from_stored(state.component_instances),
                    audio_clips,
                    source_audio_files,
                    if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
                    storage::midi_clips_from_stored(state.midi_clips),
                    state.layer_tree,
                    storage::text_notes_from_stored(state.text_notes),
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
                    0.0,
                    true,
                    None,
                    Vec::new(),  // stored_loop_regions
                    Vec::new(),  // stored_components
                    Vec::new(),  // stored_component_instances
                    IndexMap::new(),  // audio_clips
                    IndexMap::new(),  // source_audio_files
                    DEFAULT_BPM,
                    Vec::new(),  // stored_midi_clips
                    Vec::new(),  // stored_layer_tree
                    IndexMap::new(),  // text_notes
                )
            }
        };

        let settings = Settings::load();

        // Sample library folders are authoritative in global settings only.
        // Migration: if settings has no folders but the project file has some,
        // copy them into settings so they aren't lost.
        let mut settings = settings;
        if settings.sample_library_folders.is_empty() && !browser_folders.is_empty() {
            settings.sample_library_folders = browser_folders
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            settings.save();
        }

        let global_folders: Vec<PathBuf> = settings
            .sample_library_folders
            .iter()
            .map(PathBuf::from)
            .collect();

        // Use project expanded state; expand any newly-seen settings folders
        let mut expanded = browser_expanded.unwrap_or_default();
        for f in &global_folders {
            if !browser_folders.contains(f) {
                expanded.insert(f.clone());
            }
        }
        let visible = browser_visible || !global_folders.is_empty();
        let mut sample_browser = ui::browser::SampleBrowser::from_state(global_folders, expanded, visible);
        sample_browser.restore_width(browser_width);

        let device_name = if settings.audio_output_device == "No Device" {
            None
        } else {
            Some(settings.audio_output_device.as_str())
        };
        let audio_engine = AudioEngine::new_with_device(device_name, settings.buffer_size as usize);
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

        let mut recorder = AudioRecorder::new();
        if recorder.is_none() {
            println!("  Warning: no audio input device found");
        }

        // Wire monitoring ring buffer between recorder and engine
        if let (Some(ref mut rec), Some(ref eng)) = (&mut recorder, &audio_engine) {
            rec.set_monitor_ring(
                eng.monitor_ring(),
                eng.monitoring_enabled_flag(),
                eng.monitor_input_channels_flag(),
                eng.monitor_input_sample_rate_flag(),
            );
        }

        let plugin_registry = effects::PluginRegistry::new();

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
                instrument_id: if smc.instrument_id.is_empty() { None } else { smc.instrument_id.parse().ok() },
                disabled: false,
            }))
            .collect();

        let restored_layer_tree = {
            let mut tree = layers::tree_from_stored(&stored_layer_tree);
            layers::sync_tree(
                &mut tree,
                &IndexMap::new(), // no instruments in old format yet
                &restored_midi_clips,
                &waveforms,
                &IndexMap::new(), // no groups in old format
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
            source_audio_files,
            audio_engine,
            recorder,
            recording_waveform_id: None,
            recording_take_parent_id: None,
            input_monitoring: false,
            last_canvas_click_world: [0.0; 2],
            selected: Vec::new(),
            drag: DragState::None,
            mouse_pos: [0.0; 2],
            hovered: None,
            fade_handle_hovered: None,
            fade_curve_hovered: None,
            waveform_edge_hover: WaveformEdgeHover::None,
            midi_note_edge_hover: false,
            midi_clip_edge_hover_ew: false,
            midi_clip_edge_hover_ns: false,
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
            effect_chains: IndexMap::new(),
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
            master: master::Master::default(),
            groups: IndexMap::new(),
            group_hover: GroupHover::None,
            text_notes: restored_text_notes,
            text_note_hover: TextNoteHover::None,
            editing_text_note: None,
            midi_clips: restored_midi_clips,
            instruments: IndexMap::new(),
            editing_midi_clip: None,
            selected_midi_notes: Vec::new(),
            pending_midi_note_click: None,
            midi_note_select_rect: None,
            cmd_velocity_hover_note: None,
            editing_component: None,
            editing_group: None,
            editing_waveform_name: None,
            bpm: loaded_bpm,
            editing_bpm: ui::value_entry::ValueEntry::new(),
            dragging_bpm: None,
            bpm_drag_overlap_snapshots: IndexMap::new(),
            bpm_drag_overlap_temp_splits: Vec::new(),
            last_click_time: TimeInstant::now(),
            last_browser_click_time: TimeInstant::now(),
            last_browser_click_idx: None,
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
            export_window: None,
            share_window: None,
            share_button_hovered: false,
            share_id: None,
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
            connect_password: None,
            pending_welcome: None,
            pending_sync_complete: None,
            reconnect_attempt: 0,
            last_reconnect_time: None,
            cached_instances: Vec::with_capacity(2048),
            cached_wf_verts: Vec::with_capacity(32768),
            cached_preview_wf_verts: Vec::new(),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
            computer_keyboard_armed: true,
            computer_keyboard_octave_offset: 0,
            computer_keyboard_velocity: midi_keyboard::DEFAULT_VELOCITY,
            keyboard_instrument_id: None,
            midi_keyboard_held: HashMap::new(),
            solo_ids: std::collections::HashSet::new(),
            following_user: None,
            open_plugin_guis: std::collections::HashSet::new(),
            open_instrument_guis: std::collections::HashSet::new(),
            paulstretch_cache: IndexMap::new(),
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
                self.network.send_ephemeral(crate::user::EphemeralMessage::ViewportUpdate {
                    user_id: self.local_user.id,
                    position: self.camera.position,
                    zoom: self.camera.zoom,
                });
                self.last_cursor_send = now;
            }
        }
    }

    /// Broadcast playback state to remote users. Call after any local playback change.
    fn broadcast_playback_if_connected(&mut self) {
        #[cfg(not(feature = "native"))]
        return;
        #[cfg(feature = "native")]
        if self.network.is_connected() {
            if let Some(engine) = &self.audio_engine {
                self.network.send_ephemeral(crate::user::EphemeralMessage::PlaybackUpdate {
                    user_id: self.local_user.id,
                    is_playing: engine.is_playing(),
                    position_seconds: engine.position_seconds(),
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                });
            }
        }
    }

    /// Hit-test avatar circles in the top-right corner (screen-space).
    /// Returns the UserId of the clicked avatar, if any.
    fn hit_test_avatar_circles(&self) -> Option<user::UserId> {
        let r = 14.0_f32;
        let margin = 12.0_f32;
        let gap = 6.0_f32;
        let (sw, _sh, _scale) = self.screen_info();
        let mut x = sw - margin - r;
        let y = margin + r + 28.0; // offset for macOS titlebar

        let mut sorted_users: Vec<_> = self.remote_users.iter()
            .filter(|(_, s)| s.online)
            .collect();
        sorted_users.sort_by_key(|(uid, _)| **uid);

        for (uid, _remote) in &sorted_users {
            let dx = self.mouse_pos[0] - x;
            let dy = self.mouse_pos[1] - y;
            if dx * dx + dy * dy <= r * r {
                return Some(**uid);
            }
            x -= r * 2.0 + gap;
        }
        None
    }

    #[cfg(feature = "native")]
    fn connect_to_server(&mut self, url: &str, project_id: &str, password: Option<&str>) {
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
        let (sync_complete_tx, sync_complete_rx) = tokio::sync::oneshot::channel();
        let conn_state = mgr.connection_state.clone();

        let _handle = surreal_client::spawn_surreal_client(
            url.to_string(),
            project_id.to_string(),
            password.map(|s| s.to_string()),
            remote_op_tx,
            remote_op_rx,
            remote_eph_tx,
            remote_eph_rx,
            welcome_tx,
            sync_complete_tx,
            conn_state,
            rt,
        );

        // Clear stale state from previous session before installing new network
        self.remote_users.clear();
        self.applied_remote_seqs.clear();

        self.network = mgr;
        self.connect_url = Some(url.to_string());
        self.connect_project_id = Some(project_id.to_string());
        self.connect_password = password.map(|s| s.to_string());
        self.pending_welcome = Some(welcome_rx);
        self.pending_sync_complete = Some(sync_complete_rx);
        log::info!("Connecting to SurrealDB at {}", url);
    }

    /// Reset all entity state to empty for a clean-slate full sync.
    /// Called on reconnect before replaying all ops from the server.
    fn clear_entity_state(&mut self) {
        log::info!("[SYNC] Clearing entity state for full sync");

        // Abort any active recording
        self.recording_waveform_id = None;
        self.recording_take_parent_id = None;

        // Clear all entity maps
        self.objects.clear();
        self.waveforms.clear();
        self.audio_clips.clear();
        self.source_audio_files.clear();
        self.midi_clips.clear();
        self.instruments.clear();
        self.effect_chains.clear();
        self.components.clear();
        self.component_instances.clear();
        self.export_regions.clear();
        self.loop_regions.clear();
        self.groups.clear();
        self.text_notes.clear();

        // Clear undo/redo — stale ops reference entities that no longer exist
        self.op_undo_stack.clear();
        self.op_redo_stack.clear();

        // Clear selection and editing state referencing old entity IDs
        self.selected.clear();
        self.hovered = None;
        self.editing_midi_clip = None;
        self.selected_midi_notes.clear();
        self.editing_component = None;
        self.editing_group = None;
        self.editing_text_note = None;
        self.editing_waveform_name = None;
        self.solo_ids.clear();
        self.following_user = None;

        // Clear deduplication — we will replay all ops fresh
        self.applied_remote_seqs.clear();

        // Reset misc
        self.pending_audio_loads_count = 0;
        self.bpm = 120.0;

        self.sync_audio_clips();
        self.mark_dirty();
    }

    #[cfg(feature = "native")]
    fn connect_remote_session(&mut self, project_id: &str) {
        let url = SESSION_DB_WS_URL;

        // RemoteStorage needs its own Arc<Runtime> for audio assets
        let rs_rt = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio rt for remote storage"),
        );
        if let Some(rs) = storage::RemoteStorage::connect(url, None, rs_rt) {
            rs.use_project(project_id);
            self.remote_storage = Some(std::sync::Arc::new(rs));
        }

        // Real-time op sync
        self.connect_to_server(url, project_id, None);

        self.toast_manager.push(
            format!("Connecting to {SESSION_DB_HOST}/{project_id}…"),
            ui::toast::ToastKind::Info,
        );
    }

    #[cfg(feature = "native")]
    fn submit_session(&mut self, is_share: bool, input: &str) {
        let project_id = parse_session_id(input);
        if project_id.is_empty() {
            return;
        }

        self.connect_remote_session(&project_id);

        if is_share {
            let url = format!("{SESSION_DB_HOST}/{project_id}");
            #[cfg(target_os = "macos")]
            {
                use std::io::Write;
                if let Ok(mut child) = std::process::Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(url.as_bytes());
                    }
                    let _ = child.wait();
                }
            }
            self.toast_manager.push(
                format!("Session link copied: {url}"),
                ui::toast::ToastKind::Success,
            );
        }
    }

    #[cfg(not(feature = "native"))]
    fn connect_remote_session(&mut self, _project_id: &str) {}

    #[cfg(not(feature = "native"))]
    fn submit_session(&mut self, _is_share: bool, _input: &str) {}

    fn open_share_window(&mut self) {
        // Generate share_id if this project hasn't been shared yet
        if self.share_id.is_none() {
            self.share_id = Some(share_id::generate());
        }
        let id = self.share_id.as_ref().unwrap().clone();

        // Auto-connect to the session if not already connected to it
        #[cfg(feature = "native")]
        if self.connect_project_id.as_deref() != Some(id.as_str()) {
            self.connect_remote_session(&id);
        }

        let url = format!("https://layers.audio/projects/{id}");
        self.share_window = Some(ui::share_window::ShareWindow::new(url));
    }

    #[cfg(feature = "native")]
    fn copy_share_url_to_clipboard(&self, url: &str) {
        #[cfg(target_os = "macos")]
        {
            use std::io::Write;
            if let Ok(mut child) = std::process::Command::new("pbcopy")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(url.as_bytes());
                }
                let _ = child.wait();
            }
        }
    }

    #[cfg(not(feature = "native"))]
    fn copy_share_url_to_clipboard(&self, _url: &str) {}

    fn disconnect_session(&mut self) {
        if self.network.mode() == network::NetworkMode::Offline {
            return;
        }

        // Replace network manager with offline — drops channels, surreal_client task exits
        self.network = network::NetworkManager::new_offline();

        // Clear connection params — prevents auto-reconnect
        self.connect_url = None;
        self.connect_project_id = None;
        self.connect_password = None;
        self.pending_welcome = None;
        self.pending_sync_complete = None;
        self.reconnect_attempt = 0;
        self.last_reconnect_time = None;

        // Clear remote collaboration state
        self.remote_users.clear();
        self.applied_remote_seqs.clear();
        self.following_user = None;
        self.remote_storage = None;

        // Drop tokio runtime — cleans up all spawned async tasks
        self.ws_runtime = None;

        // Dismiss any lingering reconnect toast
        self.toast_manager.dismiss_by_id("reconnecting");

        self.toast_manager.push(
            "Disconnected from session",
            ui::toast::ToastKind::Info,
        );
        log::info!("[SYNC] Manually disconnected from session");
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

    /// Recompute a group's bounding box from its member entities.
    pub(crate) fn update_group_bounds(&mut self, group_id: EntityId) {
        let member_ids = if let Some(g) = self.groups.get(&group_id) {
            g.member_ids.clone()
        } else {
            return;
        };
        if member_ids.is_empty() {
            return;
        }
        // Build HitTargets from member IDs by checking entity maps
        let targets: Vec<HitTarget> = member_ids.iter().flat_map(|id| {
            if self.waveforms.contains_key(id) { vec![HitTarget::Waveform(*id)] }
            else if self.midi_clips.contains_key(id) { vec![HitTarget::MidiClip(*id)] }
            else if self.text_notes.contains_key(id) { vec![HitTarget::TextNote(*id)] }
            else if self.objects.contains_key(id) { vec![HitTarget::Object(*id)] }
            else if self.loop_regions.contains_key(id) { vec![HitTarget::LoopRegion(*id)] }
            else if self.export_regions.contains_key(id) { vec![HitTarget::ExportRegion(*id)] }
            else if self.components.contains_key(id) { vec![HitTarget::ComponentDef(*id)] }
            else if self.instruments.contains_key(id) {
                // Instrument has no position/size — use its child MIDI clips instead
                self.midi_clips.iter()
                    .filter(|(_, mc)| mc.instrument_id == Some(*id))
                    .map(|(mc_id, _)| HitTarget::MidiClip(*mc_id))
                    .collect()
            }
            else { vec![] }
        }).collect();
        if let Some((pos, size)) = group::bounding_box_of_selection(
            &targets,
            &self.waveforms, &self.midi_clips,
            &self.text_notes, &self.objects, &self.loop_regions,
            &self.export_regions, &self.components, &self.component_instances,
        ) {
            if let Some(g) = self.groups.get_mut(&group_id) {
                g.position = pos;
                g.size = size;
            }
        }
    }

    /// Update bounds for all groups that contain the given entity.
    fn update_groups_containing(&mut self, entity_id: EntityId) {
        let group_ids: Vec<EntityId> = self.groups.iter()
            .filter(|(_, g)| g.member_ids.contains(&entity_id))
            .map(|(gid, _)| *gid)
            .collect();
        for gid in group_ids {
            self.update_group_bounds(gid);
        }
    }

    pub(crate) fn update_right_window(&mut self) {
        // Don't clobber a deliberately-opened Master right window when nothing else is selected
        if self.right_window.as_ref().map_or(false, |rw| rw.is_master()) && self.selected.is_empty() {
            return;
        }
        // Collect all selected waveform IDs
        let wf_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| {
            if let HitTarget::Waveform(id) = t { Some(*id) } else { None }
        }).collect();

        if !wf_ids.is_empty() {
            let first_id = wf_ids[0];
            if let Some(wf) = self.waveforms.get(&first_id) {
                // Preserve vol_entry when updating the same waveform so that
                // click-to-edit isn't reset by the unconditional update_right_window
                // call at the end of the mouse-released handler.
                let (vol_entry, sample_bpm_entry, pitch_entry, paulstretch_factor_entry, vol_fader_focused, pan_knob_focused, pitch_focused, sample_bpm_focused) = if self.right_window.as_ref().map_or(false, |rw| rw.target_id() == first_id) {
                    let rw = self.right_window.take().unwrap();
                    (rw.vol_entry, rw.sample_bpm_entry, rw.pitch_entry, rw.paulstretch_factor_entry, rw.vol_fader_focused, rw.pan_knob_focused, rw.pitch_focused, rw.sample_bpm_focused)
                } else {
                    (ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), false, false, false, false)
                };
                self.right_window = Some(ui::right_window::RightWindow {
                    target: ui::right_window::RightWindowTarget::Waveform(first_id),
                    volume: wf.volume,
                    pan: wf.pan,
                    warp_mode: wf.warp_mode,
                    sample_bpm: wf.sample_bpm,
                    pitch_semitones: wf.pitch_semitones,
                    is_reversed: wf.is_reversed,
                    vol_dragging: false,
                    pan_dragging: false,
                    sample_bpm_dragging: false,
                    pitch_dragging: false,
                    paulstretch_factor: wf.paulstretch_factor,
                    paulstretch_factor_dragging: false,
                    paulstretch_factor_entry: paulstretch_factor_entry,
                    paulstretch_factor_focused: false,
                    drag_start_y: 0.0,
                    drag_start_value: 0.0,
                    vol_entry,
                    sample_bpm_entry,
                    pitch_entry,
                    vol_fader_focused,
                    pan_knob_focused,
                    pitch_focused,
                    sample_bpm_focused,
                    add_effect_hovered: false,
                    export_button_hovered: false,
                    reverse_button_hovered: false,
                    warp_button_hovered: false,
                    warp_selector_hovered: false,
                    paulstretch_render_button_hovered: false,
                    group_name: String::new(),
                    group_member_count: 0,
                    multi_target_ids: wf_ids,
                    drag_start_snapshots: Vec::new(),
                    is_soloed: self.solo_ids.contains(&first_id),
                    is_muted: wf.disabled,
                    meter_rms: 0.0,
                    meter_peak: 0.0,
                    peak_hold_timer: 0.0,
                });
                return;
            }
        }
        // If a MIDI clip is selected, open the right window for its parent instrument
        if let Some(HitTarget::MidiClip(mc_id)) = self.selected.first().copied() {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                if let Some(inst_id) = mc.instrument_id {
                    self.update_right_window_for_instrument(inst_id);
                    return;
                }
            }
        }
        // If a group is selected, open the right window for it
        if let Some(HitTarget::Group(group_id)) = self.selected.first().copied() {
            if let Some(g) = self.groups.get(&group_id) {
                let name = g.name.clone();
                let member_count = g.member_ids.len();
                let (vol_entry, vol_fader_focused, pan_knob_focused) =
                    if self.right_window.as_ref().map_or(false, |rw| rw.target_id() == group_id && rw.is_group()) {
                        let rw = self.right_window.take().unwrap();
                        (rw.vol_entry, rw.vol_fader_focused, rw.pan_knob_focused)
                    } else {
                        (ui::value_entry::ValueEntry::new(), false, false)
                    };
                self.right_window = Some(ui::right_window::RightWindow {
                    target: ui::right_window::RightWindowTarget::Group(group_id),
                    volume: g.volume,
                    pan: g.pan,
                    warp_mode: ui::waveform::WarpMode::Off,
                    sample_bpm: 120.0,
                    pitch_semitones: 0.0,
                    is_reversed: false,
                    vol_dragging: false,
                    pan_dragging: false,
                    sample_bpm_dragging: false,
                    pitch_dragging: false,
                    paulstretch_factor: 8.0,
                    paulstretch_factor_dragging: false,
                    paulstretch_factor_entry: ui::value_entry::ValueEntry::new(),
                    paulstretch_factor_focused: false,
                    drag_start_y: 0.0,
                    drag_start_value: 0.0,
                    vol_entry,
                    sample_bpm_entry: ui::value_entry::ValueEntry::new(),
                    pitch_entry: ui::value_entry::ValueEntry::new(),
                    vol_fader_focused,
                    pan_knob_focused,
                    pitch_focused: false,
                    sample_bpm_focused: false,
                    add_effect_hovered: false,
                    export_button_hovered: false,
                    reverse_button_hovered: false,
                    warp_button_hovered: false,
                    warp_selector_hovered: false,
                    paulstretch_render_button_hovered: false,
                    group_name: name,
                    group_member_count: member_count,
                    multi_target_ids: Vec::new(),
                    drag_start_snapshots: Vec::new(),
                    is_soloed: self.solo_ids.contains(&group_id),
                    is_muted: g.disabled,
                    meter_rms: 0.0,
                    meter_peak: 0.0,
                    peak_hold_timer: 0.0,
                });
                return;
            }
        }
        self.right_window = None;
    }

    /// Open the right window inspector for a specific waveform (used when adding effects).
    pub(crate) fn open_right_window_for(&mut self, wf_id: EntityId) {
        if let Some(wf) = self.waveforms.get(&wf_id) {
            self.right_window = Some(ui::right_window::RightWindow {
                target: ui::right_window::RightWindowTarget::Waveform(wf_id),
                volume: wf.volume,
                pan: wf.pan,
                warp_mode: wf.warp_mode,
                sample_bpm: wf.sample_bpm,
                pitch_semitones: wf.pitch_semitones,
                is_reversed: wf.is_reversed,
                vol_dragging: false,
                pan_dragging: false,
                sample_bpm_dragging: false,
                pitch_dragging: false,
                paulstretch_factor: wf.paulstretch_factor,
                paulstretch_factor_dragging: false,
                paulstretch_factor_entry: ui::value_entry::ValueEntry::new(),
                paulstretch_factor_focused: false,
                drag_start_y: 0.0,
                drag_start_value: 0.0,
                vol_entry: ui::value_entry::ValueEntry::new(),
                sample_bpm_entry: ui::value_entry::ValueEntry::new(),
                pitch_entry: ui::value_entry::ValueEntry::new(),
                vol_fader_focused: false,
                pan_knob_focused: false,
                pitch_focused: false,
                sample_bpm_focused: false,
                add_effect_hovered: false,
                export_button_hovered: false,
                reverse_button_hovered: false,
                    warp_button_hovered: false,
                    warp_selector_hovered: false,
                    paulstretch_render_button_hovered: false,
                group_name: String::new(),
                group_member_count: 0,
                multi_target_ids: vec![wf_id],
                drag_start_snapshots: Vec::new(),
                is_soloed: self.solo_ids.contains(&wf_id),
                is_muted: wf.disabled,
                meter_rms: 0.0,
                meter_peak: 0.0,
                peak_hold_timer: 0.0,
            });
        }
    }

    /// Open the right window inspector for an instrument.
    pub(crate) fn update_right_window_for_instrument(&mut self, inst_id: EntityId) {
        if let Some(inst) = self.instruments.get(&inst_id) {
            let (vol_entry, vol_fader_focused, pan_knob_focused) =
                if self.right_window.as_ref().map_or(false, |rw| rw.target_id() == inst_id && rw.is_instrument()) {
                    let rw = self.right_window.take().unwrap();
                    (rw.vol_entry, rw.vol_fader_focused, rw.pan_knob_focused)
                } else {
                    (ui::value_entry::ValueEntry::new(), false, false)
                };
            self.right_window = Some(ui::right_window::RightWindow {
                target: ui::right_window::RightWindowTarget::Instrument(inst_id),
                volume: inst.volume,
                pan: inst.pan,
                warp_mode: ui::waveform::WarpMode::Off,
                sample_bpm: 120.0,
                pitch_semitones: 0.0,
                is_reversed: false,
                vol_dragging: false,
                pan_dragging: false,
                sample_bpm_dragging: false,
                pitch_dragging: false,
                paulstretch_factor: 8.0,
                paulstretch_factor_dragging: false,
                paulstretch_factor_entry: ui::value_entry::ValueEntry::new(),
                paulstretch_factor_focused: false,
                drag_start_y: 0.0,
                drag_start_value: 0.0,
                vol_entry,
                sample_bpm_entry: ui::value_entry::ValueEntry::new(),
                pitch_entry: ui::value_entry::ValueEntry::new(),
                vol_fader_focused,
                pan_knob_focused,
                pitch_focused: false,
                sample_bpm_focused: false,
                add_effect_hovered: false,
                export_button_hovered: false,
                reverse_button_hovered: false,
                    warp_button_hovered: false,
                    warp_selector_hovered: false,
                    paulstretch_render_button_hovered: false,
                group_name: String::new(),
                group_member_count: 0,
                multi_target_ids: Vec::new(),
                drag_start_snapshots: Vec::new(),
                is_soloed: self.solo_ids.contains(&inst_id),
                is_muted: inst.disabled,
                meter_rms: 0.0,
                meter_peak: 0.0,
                peak_hold_timer: 0.0,
            });
        }
    }

    pub(crate) fn open_right_window_for_master(&mut self) {
        let (vol_entry, vol_fader_focused, pan_knob_focused) =
            if self.right_window.as_ref().map_or(false, |rw| rw.is_master()) {
                let rw = self.right_window.take().unwrap();
                (rw.vol_entry, rw.vol_fader_focused, rw.pan_knob_focused)
            } else {
                (ui::value_entry::ValueEntry::new(), false, false)
            };
        let member_count = self.waveforms.len() + self.instruments.len();
        self.right_window = Some(ui::right_window::RightWindow {
            target: ui::right_window::RightWindowTarget::Master,
            volume: self.master.volume,
            pan: self.master.pan,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: 120.0,
            pitch_semitones: 0.0,
            is_reversed: false,
            vol_dragging: false,
            pan_dragging: false,
            sample_bpm_dragging: false,
            pitch_dragging: false,
            paulstretch_factor: 8.0,
            paulstretch_factor_dragging: false,
            paulstretch_factor_entry: ui::value_entry::ValueEntry::new(),
            paulstretch_factor_focused: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
            vol_entry,
            sample_bpm_entry: ui::value_entry::ValueEntry::new(),
            pitch_entry: ui::value_entry::ValueEntry::new(),
            vol_fader_focused,
            pan_knob_focused,
            pitch_focused: false,
            sample_bpm_focused: false,
            add_effect_hovered: false,
            export_button_hovered: false,
            reverse_button_hovered: false,
                    warp_button_hovered: false,
                    warp_selector_hovered: false,
                    paulstretch_render_button_hovered: false,
            group_name: "Main".to_string(),
            group_member_count: member_count,
            multi_target_ids: Vec::new(),
            drag_start_snapshots: Vec::new(),
            is_soloed: false,
            is_muted: false,
            meter_rms: 0.0,
            meter_peak: 0.0,
            peak_hold_timer: 0.0,
        });
    }

    /// Detach a waveform's effect chain — clone the shared chain into a new independent one.
    pub(crate) fn detach_effect_chain(&mut self, wf_id: EntityId) {
        let chain_id = match self.waveforms.get(&wf_id).and_then(|w| w.effect_chain_id) {
            Some(id) => id,
            None => return,
        };
        let ref_count = ui::right_window::RightWindow::chain_ref_count_all(chain_id, &self.waveforms, &self.instruments, &self.groups);
        if ref_count <= 1 {
            return; // Already unique
        }
        let Some(chain) = self.effect_chains.get(&chain_id) else { return; };
        let mut new_chain = effects::EffectChain::new();
        for slot in &chain.slots {
            let mut new_slot = effects::EffectChainSlot::new(
                slot.plugin_id.clone(),
                slot.plugin_name.clone(),
                slot.plugin_path.clone(),
            );
            new_slot.bypass = slot.bypass;
            // Note: plugin GUI instances are not cloned — new instances would need to be opened
            new_chain.slots.push(new_slot);
        }
        let new_chain_id = new_id();
        self.effect_chains.insert(new_chain_id, new_chain);
        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
            wf.effect_chain_id = Some(new_chain_id);
        }
        self.request_redraw();
    }

    /// Detach an instrument's effect chain — clone the shared chain into a new independent one.
    pub(crate) fn detach_instrument_effect_chain(&mut self, inst_id: EntityId) {
        let chain_id = match self.instruments.get(&inst_id).and_then(|i| i.effect_chain_id) {
            Some(id) => id,
            None => return,
        };
        let ref_count = ui::right_window::RightWindow::chain_ref_count_all(chain_id, &self.waveforms, &self.instruments, &self.groups);
        if ref_count <= 1 {
            return;
        }
        let Some(chain) = self.effect_chains.get(&chain_id) else { return; };
        let mut new_chain = effects::EffectChain::new();
        for slot in &chain.slots {
            let mut new_slot = effects::EffectChainSlot::new(
                slot.plugin_id.clone(),
                slot.plugin_name.clone(),
                slot.plugin_path.clone(),
            );
            new_slot.bypass = slot.bypass;
            new_chain.slots.push(new_slot);
        }
        let new_chain_id = new_id();
        self.effect_chains.insert(new_chain_id, new_chain);
        if let Some(inst) = self.instruments.get_mut(&inst_id) {
            inst.effect_chain_id = Some(new_chain_id);
        }
        self.request_redraw();
    }

    /// Detach a group's effect chain — clone the shared chain into a new independent one.
    pub(crate) fn detach_group_effect_chain(&mut self, group_id: EntityId) {
        let chain_id = match self.groups.get(&group_id).and_then(|g| g.effect_chain_id) {
            Some(id) => id,
            None => return,
        };
        let ref_count = ui::right_window::RightWindow::chain_ref_count_all(chain_id, &self.waveforms, &self.instruments, &self.groups);
        if ref_count <= 1 {
            return;
        }
        let Some(chain) = self.effect_chains.get(&chain_id) else { return; };
        let mut new_chain = effects::EffectChain::new();
        for slot in &chain.slots {
            let mut new_slot = effects::EffectChainSlot::new(
                slot.plugin_id.clone(),
                slot.plugin_name.clone(),
                slot.plugin_path.clone(),
            );
            new_slot.bypass = slot.bypass;
            new_chain.slots.push(new_slot);
        }
        let new_chain_id = new_id();
        self.effect_chains.insert(new_chain_id, new_chain);
        if let Some(g) = self.groups.get_mut(&group_id) {
            g.effect_chain_id = Some(new_chain_id);
        }
        self.request_redraw();
    }

    /// Toggle solo on an entity. Click = exclusive, shift = additive.
    pub(crate) fn toggle_solo(&mut self, id: EntityId, shift: bool) {
        if shift {
            // Additive: toggle this one
            if !self.solo_ids.remove(&id) {
                self.solo_ids.insert(id);
            }
        } else {
            // Exclusive: if already the only solo, clear; otherwise set as only solo
            if self.solo_ids.len() == 1 && self.solo_ids.contains(&id) {
                self.solo_ids.clear();
            } else {
                self.solo_ids.clear();
                self.solo_ids.insert(id);
            }
        }
    }

    /// Toggle mute (disabled) on a single entity, creating an undoable operation.
    pub(crate) fn toggle_mute_disabled(&mut self, id: EntityId) {
        if self.waveforms.contains_key(&id) {
            let before = self.waveforms[&id].clone();
            self.waveforms.get_mut(&id).unwrap().disabled = !before.disabled;
            let after = self.waveforms[&id].clone();
            self.push_op(crate::operations::Operation::UpdateWaveform { id, before, after });
        } else if self.instruments.contains_key(&id) {
            let inst = &self.instruments[&id];
            let before = crate::instruments::InstrumentSnapshot {
                name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
                plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
                volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id, disabled: inst.disabled,
            };
            let new_disabled = !inst.disabled;
            self.instruments.get_mut(&id).unwrap().disabled = new_disabled;
            let after = crate::instruments::InstrumentSnapshot { disabled: new_disabled, ..before.clone() };
            self.push_op(crate::operations::Operation::UpdateInstrument { id, before, after });
        } else if let Some(before) = self.groups.get(&id).cloned() {
            if let Some(g) = self.groups.get_mut(&id) {
                g.disabled = !g.disabled;
            }
            if let Some(after) = self.groups.get(&id).cloned() {
                self.push_op(crate::operations::Operation::UpdateGroup { id, before, after });
            }
        }
    }

    /// Whether an entity should produce audio, considering solo and group membership.
    pub(crate) fn should_play(&self, id: EntityId) -> bool {
        // Check disabled on the entity itself
        if self.waveforms.get(&id).map_or(false, |wf| wf.disabled) {
            return false;
        }
        if self.instruments.get(&id).map_or(false, |inst| inst.disabled) {
            return false;
        }
        if self.groups.get(&id).map_or(false, |g| g.disabled) {
            return false;
        }
        is_entity_audible(id, &self.solo_ids, &self.groups)
    }

    /// Whether a group bus should be active (not disabled, and passes solo check).
    #[cfg(feature = "native")]
    fn should_play_group(&self, gid: EntityId) -> bool {
        if self.groups.get(&gid).map_or(false, |g| g.disabled) {
            return false;
        }
        if self.solo_ids.is_empty() {
            return true;
        }
        // Group is soloed, or any member is soloed
        if self.solo_ids.contains(&gid) {
            return true;
        }
        if let Some(group) = self.groups.get(&gid) {
            for mid in &group.member_ids {
                if self.solo_ids.contains(mid) {
                    return true;
                }
            }
        }
        false
    }

    /// Build a member → group lookup map.
    #[cfg(feature = "native")]
    fn build_group_membership(&self) -> std::collections::HashMap<EntityId, EntityId> {
        let mut map = std::collections::HashMap::new();
        for (&gid, group) in &self.groups {
            for &mid in &group.member_ids {
                map.insert(mid, gid);
            }
        }
        map
    }

    /// Collect non-bypassed effect chain plugin handles for an entity,
    /// including its own chain and its parent group's chain (if any).
    /// Used for instruments where group FX are still inlined (v1 scope).
    #[cfg(feature = "native")]
    fn collect_chain_latency(
        chain_plugins: &[std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>],
    ) -> u32 {
        chain_plugins.iter().filter_map(|p| {
            p.lock().ok().and_then(|g| g.as_ref().map(|gui| gui.get_latency_samples()))
        }).sum()
    }

    fn collect_chain_plugins(
        &self,
        entity_id: EntityId,
        own_chain_id: Option<EntityId>,
        group_of: &std::collections::HashMap<EntityId, EntityId>,
    ) -> Vec<std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>> {
        let mut out = Vec::new();
        if let Some(chain_id) = own_chain_id {
            if let Some(chain) = self.effect_chains.get(&chain_id) {
                for slot in &chain.slots {
                    if !slot.bypass {
                        out.push(slot.gui.clone());
                    }
                }
            }
        }
        if let Some(&group_id) = group_of.get(&entity_id) {
            if let Some(group) = self.groups.get(&group_id) {
                if let Some(chain_id) = group.effect_chain_id {
                    if let Some(chain) = self.effect_chains.get(&chain_id) {
                        for slot in &chain.slots {
                            if !slot.bypass {
                                out.push(slot.gui.clone());
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Collect only the entity's own effect chain plugins (no group chain).
    /// Used for waveform clips where group FX are processed on a separate bus.
    pub(crate) fn collect_clip_chain_plugins(
        &self,
        own_chain_id: Option<EntityId>,
    ) -> Vec<std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>> {
        let mut out = Vec::new();
        if let Some(chain_id) = own_chain_id {
            if let Some(chain) = self.effect_chains.get(&chain_id) {
                for slot in &chain.slots {
                    if !slot.bypass {
                        out.push(slot.gui.clone());
                    }
                }
            }
        }
        out
    }

    /// Collect non-bypassed plugin handles for a group's own effect chain.
    fn collect_group_chain_plugins(
        &self,
        group_id: EntityId,
    ) -> Vec<std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>> {
        let mut out = Vec::new();
        if let Some(group) = self.groups.get(&group_id) {
            if let Some(chain_id) = group.effect_chain_id {
                if let Some(chain) = self.effect_chains.get(&chain_id) {
                    for slot in &chain.slots {
                        if !slot.bypass {
                            out.push(slot.gui.clone());
                        }
                    }
                }
            }
        }
        out
    }

    fn collect_master_chain_plugins(
        &self,
    ) -> Vec<std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>> {
        let mut out = Vec::new();
        if let Some(chain_id) = self.master.effect_chain_id {
            if let Some(chain) = self.effect_chains.get(&chain_id) {
                for slot in &chain.slots {
                    if !slot.bypass {
                        out.push(slot.gui.clone());
                    }
                }
            }
        }
        out
    }

    #[cfg(feature = "native")]
    fn apply_paulstretch(&mut self, wf_id: EntityId) {
        let factor = match self.waveforms.get(&wf_id) {
            Some(wf) => wf.paulstretch_factor,
            None => return,
        };

        // If already cached with same factor, skip reprocessing
        if let Some(cache) = self.paulstretch_cache.get(&wf_id) {
            if cache.factor == factor { return; }
        }

        // Get originals from cache (if re-processing) or from current state
        let (original_audio, original_clip, original_width) = if let Some(cache) = self.paulstretch_cache.get(&wf_id) {
            (cache.original_audio.clone(), cache.original_clip.clone(), cache.original_width)
        } else {
            let wf = self.waveforms.get(&wf_id).unwrap();
            let clip = match self.audio_clips.get(&wf_id) {
                Some(c) => c.clone(),
                None => return,
            };
            (wf.audio.clone(), clip, wf.size[0])
        };

        if original_audio.left_samples.is_empty() {
            return;
        }

        let sr = original_audio.sample_rate;
        let (stretched_left, stretched_right) = paulstretch::paulstretch_stereo(
            &original_audio.left_samples,
            &original_audio.right_samples,
            sr,
            factor,
            0.25,
        );

        // Build mono downmix for the audio engine
        let mono: Vec<f32> = stretched_left.iter()
            .zip(stretched_right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        let stretched_duration = mono.len() as f32 / sr as f32;
        let stretched_width = stretched_duration * grid::PIXELS_PER_SECOND;

        let left_peaks = Arc::new(ui::waveform::WaveformPeaks::build(&stretched_left));
        let right_peaks = Arc::new(ui::waveform::WaveformPeaks::build(&stretched_right));

        let new_audio = Arc::new(ui::waveform::AudioData {
            left_samples: Arc::new(stretched_left),
            right_samples: Arc::new(stretched_right),
            left_peaks,
            right_peaks,
            sample_rate: sr,
            filename: original_audio.filename.clone(),
        });

        let new_clip = ui::waveform::AudioClipData {
            samples: Arc::new(mono),
            sample_rate: sr,
            duration_secs: stretched_duration,
        };

        // Store cache with originals
        self.paulstretch_cache.insert(wf_id, PaulStretchCache {
            original_audio,
            original_clip,
            original_width,
            factor,
        });

        // Replace the active audio data
        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
            wf.audio = new_audio;
            wf.size[0] = stretched_width;
        }
        self.audio_clips.insert(wf_id, new_clip);
    }

    #[cfg(feature = "native")]
    fn restore_paulstretch(&mut self, wf_id: EntityId) {
        if let Some(cache) = self.paulstretch_cache.remove(&wf_id) {
            if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                wf.audio = cache.original_audio;
                wf.size[0] = cache.original_width;
            }
            self.audio_clips.insert(wf_id, cache.original_clip);
        }
    }

    #[cfg(feature = "native")]
    fn sync_audio_clips(&self) {
        if let Some(engine) = &self.audio_engine {
            let group_of = self.build_group_membership();

            // Build group → bus_index mapping and collect group bus data.
            // Allocate a bus when the group has FX plugins OR has audio members
            // (so group volume/pan always applies).
            let mut group_id_to_bus_idx: std::collections::HashMap<EntityId, usize> = std::collections::HashMap::new();
            let mut group_buses: Vec<audio::GroupBus> = Vec::new();
            for (&gid, group) in &self.groups {
                if !self.should_play_group(gid) {
                    continue;
                }
                let plugins = self.collect_group_chain_plugins(gid);
                let has_audio_member = group.member_ids.iter().any(|mid| {
                    self.waveforms.get(mid).map_or(false, |wf| !wf.disabled && self.audio_clips.contains_key(mid))
                });
                let has_instrument_member = group.member_ids.iter().any(|mid| {
                    self.instruments.get(mid).map_or(false, |inst| inst.has_plugin())
                });
                if plugins.is_empty() && !has_audio_member && !has_instrument_member {
                    continue;
                }
                let latency = Self::collect_chain_latency(&plugins);
                let bus_idx = group_buses.len();
                group_id_to_bus_idx.insert(gid, bus_idx);
                group_buses.push(audio::GroupBus { entity_id: gid, plugins, latency_samples: latency, volume: group.volume, pan: group.pan });
            }

            let mut positions: Vec<[f32; 2]> = Vec::new();
            let mut sizes: Vec<[f32; 2]> = Vec::new();
            let mut clips: Vec<&AudioClipData> = Vec::new();
            let mut entity_ids: Vec<EntityId> = Vec::new();
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
            let mut left_buffers: Vec<Option<std::sync::Arc<Vec<f32>>>> = Vec::new();
            let mut right_buffers: Vec<Option<std::sync::Arc<Vec<f32>>>> = Vec::new();
            let mut chain_plugins_per_clip: Vec<Vec<std::sync::Arc<std::sync::Mutex<Option<effects::PluginGuiHandle>>>>> = Vec::new();
            let mut chain_latencies: Vec<u32> = Vec::new();
            let mut group_bus_indices: Vec<Option<usize>> = Vec::new();
            let mut chain_bus_indices: Vec<Option<usize>> = Vec::new();

            // Count references per effect_chain_id to detect shared chains
            let mut chain_ref_counts: std::collections::HashMap<EntityId, usize> = std::collections::HashMap::new();
            for wf in self.waveforms.values() {
                if wf.disabled { continue; }
                if let Some(cid) = wf.effect_chain_id {
                    *chain_ref_counts.entry(cid).or_insert(0) += 1;
                }
            }
            // Also count from component instances
            for inst in self.component_instances.values() {
                if let Some(def) = self.components.values().find(|c| c.id == inst.component_id) {
                    for &wf_id in &def.waveform_ids {
                        if let Some(wf) = self.waveforms.get(&wf_id) {
                            if !wf.disabled && self.audio_clips.contains_key(&wf_id) {
                                if let Some(cid) = wf.effect_chain_id {
                                    *chain_ref_counts.entry(cid).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                }
            }

            // Build ChainBus entries for shared chains (ref_count > 1)
            let mut chain_id_to_bus_idx: std::collections::HashMap<EntityId, usize> = std::collections::HashMap::new();
            let mut chain_buses: Vec<audio::ChainBus> = Vec::new();
            for (&cid, &count) in &chain_ref_counts {
                if count <= 1 { continue; }
                let plugins = self.collect_clip_chain_plugins(Some(cid));
                let latency = Self::collect_chain_latency(&plugins);
                let idx = chain_buses.len();
                chain_id_to_bus_idx.insert(cid, idx);
                chain_buses.push(audio::ChainBus { plugins, latency_samples: latency });
            }

            for (&wf_id, wf) in self.waveforms.iter() {
                if wf.disabled {
                    continue;
                }
                if !self.should_play(wf_id) {
                    continue;
                }
                let clip = match self.audio_clips.get(&wf_id) {
                    Some(c) => c,
                    None => continue,
                };
                positions.push(wf.position);
                sizes.push(wf.size);
                clips.push(clip);
                entity_ids.push(wf_id);
                fade_ins.push(wf.fade_in_px);
                fade_outs.push(wf.fade_out_px);
                fade_in_curves.push(wf.fade_in_curve);
                fade_out_curves.push(wf.fade_out_curve);
                volumes.push(wf.volume);
                pans.push(wf.pan);
                sample_offsets.push(wf.sample_offset_px);
                vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
                warp_modes.push(match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, ui::waveform::WarpMode::Semitone => 2, ui::waveform::WarpMode::PaulStretch => 3, ui::waveform::WarpMode::Complex => 4, _ => 0 });
                sample_bpms.push(wf.sample_bpm);
                pitch_semitones_vec.push(wf.pitch_semitones);

                // Stereo buffers for Complex warp mode
                if wf.warp_mode == ui::waveform::WarpMode::Complex {
                    left_buffers.push(Some(wf.audio.left_samples.clone()));
                    right_buffers.push(Some(wf.audio.right_samples.clone()));
                } else {
                    left_buffers.push(None);
                    right_buffers.push(None);
                }

                let is_shared = wf.effect_chain_id
                    .map(|cid| chain_id_to_bus_idx.contains_key(&cid))
                    .unwrap_or(false);

                let bus_idx = group_of.get(&wf_id)
                    .and_then(|gid| group_id_to_bus_idx.get(gid).copied());
                let group_latency = bus_idx
                    .map(|idx| group_buses[idx].latency_samples)
                    .unwrap_or(0);

                if is_shared {
                    let chain_bus_idx = wf.effect_chain_id
                        .and_then(|cid| chain_id_to_bus_idx.get(&cid).copied());
                    let chain_latency = chain_bus_idx
                        .map(|idx| chain_buses[idx].latency_samples)
                        .unwrap_or(0);
                    chain_latencies.push(chain_latency + group_latency);
                    chain_plugins_per_clip.push(Vec::new());
                    chain_bus_indices.push(chain_bus_idx);
                } else {
                    let clip_plugins = self.collect_clip_chain_plugins(wf.effect_chain_id);
                    let clip_latency = Self::collect_chain_latency(&clip_plugins);
                    chain_latencies.push(clip_latency + group_latency);
                    chain_plugins_per_clip.push(clip_plugins);
                    chain_bus_indices.push(None);
                }
                group_bus_indices.push(bus_idx);
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
                                entity_ids.push(wf_id);
                                fade_ins.push(wf.fade_in_px);
                                fade_outs.push(wf.fade_out_px);
                                fade_in_curves.push(wf.fade_in_curve);
                                fade_out_curves.push(wf.fade_out_curve);
                                volumes.push(wf.volume);
                                pans.push(wf.pan);
                                sample_offsets.push(wf.sample_offset_px);
                                vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                                pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
                                warp_modes.push(match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, ui::waveform::WarpMode::Semitone => 2, ui::waveform::WarpMode::PaulStretch => 3, ui::waveform::WarpMode::Complex => 4, _ => 0 });
                                sample_bpms.push(wf.sample_bpm);

                                if wf.warp_mode == ui::waveform::WarpMode::Complex {
                                    left_buffers.push(Some(wf.audio.left_samples.clone()));
                                    right_buffers.push(Some(wf.audio.right_samples.clone()));
                                } else {
                                    left_buffers.push(None);
                                    right_buffers.push(None);
                                }

                                let is_shared = wf.effect_chain_id
                                    .map(|cid| chain_id_to_bus_idx.contains_key(&cid))
                                    .unwrap_or(false);

                                let bus_idx = group_of.get(&wf_id)
                                    .and_then(|gid| group_id_to_bus_idx.get(gid).copied());
                                let group_latency = bus_idx
                                    .map(|idx| group_buses[idx].latency_samples)
                                    .unwrap_or(0);

                                if is_shared {
                                    let chain_bus_idx = wf.effect_chain_id
                                        .and_then(|cid| chain_id_to_bus_idx.get(&cid).copied());
                                    let chain_latency = chain_bus_idx
                                        .map(|idx| chain_buses[idx].latency_samples)
                                        .unwrap_or(0);
                                    chain_latencies.push(chain_latency + group_latency);
                                    chain_plugins_per_clip.push(Vec::new());
                                    chain_bus_indices.push(chain_bus_idx);
                                } else {
                                    let clip_plugins = self.collect_clip_chain_plugins(wf.effect_chain_id);
                                    let clip_latency = Self::collect_chain_latency(&clip_plugins);
                                    chain_latencies.push(clip_latency + group_latency);
                                    chain_plugins_per_clip.push(clip_plugins);
                                    chain_bus_indices.push(None);
                                }
                                group_bus_indices.push(bus_idx);
                            }
                        }
                    }
                }
            }

            let owned_clips: Vec<AudioClipData> = clips.iter().map(|c| (*c).clone()).collect();
            engine.update_clips(&positions, &sizes, &owned_clips, &fade_ins, &fade_outs, &fade_in_curves, &fade_out_curves, &volumes, &pans, &sample_offsets, &vol_autos, &pan_autos, &warp_modes, &sample_bpms, self.bpm, &pitch_semitones_vec, &chain_plugins_per_clip, &chain_latencies, &group_bus_indices, &chain_bus_indices, &entity_ids, &left_buffers, &right_buffers);
            self.sync_instrument_regions(&group_id_to_bus_idx, &group_buses);
            engine.update_group_buses(group_buses);
            engine.update_chain_buses(chain_buses);

            // Update master bus (Main Layer vol/pan/effects)
            let master_plugins = self.collect_master_chain_plugins();
            engine.update_master_bus(master_plugins, self.master.volume, self.master.pan);

            let regions: Vec<audio::AudioEffectRegion> = Vec::new();
            engine.update_effect_regions(regions);
        }
        self.sync_monitor_effects();
    }

    fn add_loop_area(&mut self) {
        let (pos, size) = if let Some(sa) = self.select_area.take() {
            let x0 = snap_to_grid(sa.position[0], &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(sa.position[0] + sa.size[0], &self.settings, self.camera.zoom, self.bpm);
            ([x0, sa.position[1]], [x1 - x0, sa.size[1]])
        } else if !self.selected.is_empty()
            && self.selected.iter().any(|t| !matches!(t, HitTarget::LoopRegion(_)))
        {
            // From selected entities — use bounding box x/width
            let mut min_x = f32::MAX;
            let mut max_x = f32::MIN;
            for target in self.selected.iter().filter(|t| !matches!(t, HitTarget::LoopRegion(_))) {
                let p = self.get_target_pos(target);
                let s = self.get_target_size(target);
                min_x = min_x.min(p[0]);
                max_x = max_x.max(p[0] + s[0]);
            }
            let x0 = snap_to_grid(min_x, &self.settings, self.camera.zoom, self.bpm);
            let x1 = snap_to_grid(max_x, &self.settings, self.camera.zoom, self.bpm);
            let (_, sh, _) = self.screen_info();
            let h = LOOP_REGION_DEFAULT_HEIGHT;
            let center_y = self.camera.screen_to_world([0.0, sh * 0.5])[1];
            ([x0, center_y - h * 0.5], [x1 - x0, h])
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
        let name = format!("Group {}", self.groups.len() + 1);
        let group = crate::group::Group::new(id, name, pos, size, vec![]);
        self.groups.insert(id, group.clone());
        self.push_op(operations::Operation::CreateGroup { id, data: group });
        self.selected.clear();
        self.selected.push(HitTarget::Group(id));
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

    fn add_text_note(&mut self) {
        let (sw, sh, _) = self.screen_info();
        let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
        let w = text_note::DEFAULT_SIZE[0];
        let h = text_note::DEFAULT_SIZE[1];
        let pos = [center[0] - w * 0.5, center[1] - h * 0.5];
        let id = new_id();
        let data = text_note::TextNote::new(pos, &self.settings.theme);
        self.text_notes.insert(id, data.clone());
        self.push_op(operations::Operation::CreateTextNote { id, data });
        self.selected.clear();
        self.selected.push(HitTarget::TextNote(id));
        self.render_generation += 1;
        self.request_redraw();
    }

    pub(crate) fn commit_text_note_edit(&mut self) {
        if let Some(edit) = self.editing_text_note.take() {
            if let Some(tn) = self.text_notes.get(&edit.note_id) {
                if tn.text != edit.before_text {
                    let mut before = tn.clone();
                    before.text = edit.before_text;
                    let after = tn.clone();
                    self.push_op(operations::Operation::UpdateTextNote {
                        id: edit.note_id,
                        before,
                        after,
                    });
                }
            }
            self.render_generation += 1;
        }
    }

    pub(crate) fn enter_text_note_edit(&mut self, note_id: EntityId) {
        // Commit any existing edit first
        self.commit_text_note_edit();
        if let Some(tn) = self.text_notes.get(&note_id) {
            let text = tn.text.clone();
            let cursor = text.len();
            self.editing_text_note = Some(text_note::TextNoteEditState {
                note_id,
                text: text.clone(),
                before_text: text,
                cursor,
            });
            self.render_generation += 1;
            self.request_redraw();
        }
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
        let color_idx = self.midi_clips.len() % crate::theme::WAVEFORM_COLORS.len();
        clip.color = crate::theme::WAVEFORM_COLORS[color_idx];
        // Standalone MIDI clip — no instrument assigned
        let id = new_id();
        self.midi_clips.insert(id, clip.clone());
        self.push_op(operations::Operation::CreateMidiClip { id, data: clip });
        self.selected.clear();
        self.selected.push(HitTarget::MidiClip(id));
        self.request_redraw();
    }

    /// Find the first instrument, if any.
    fn find_containing_instrument(&self, _pos: [f32; 2], _size: [f32; 2]) -> Option<EntityId> {
        self.instruments.keys().next().copied()
    }

    #[cfg(feature = "native")]
    fn sync_instrument_regions(
        &self,
        group_id_to_bus_idx: &std::collections::HashMap<EntityId, usize>,
        group_buses: &[audio::GroupBus],
    ) {
        if let Some(engine) = &self.audio_engine {
            let group_of = self.build_group_membership();
            let mut audio_instruments = Vec::new();

            // Build from lightweight instruments (new path)
            for (&inst_id, inst) in self.instruments.iter() {
                if !inst.has_plugin() || inst.disabled {
                    continue;
                }
                if !self.should_play(inst_id) {
                    continue;
                }
                let mut midi_events = Vec::new();
                let mut x_min = f32::MAX;
                let mut x_max = f32::MIN;
                for mc in self.midi_clips.values() {
                    if mc.instrument_id != Some(inst_id) {
                        continue;
                    }
                    x_min = x_min.min(mc.position[0]);
                    x_max = x_max.max(mc.position[0] + mc.size[0]);
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
                if x_min > x_max {
                    x_min = 0.0;
                    x_max = 0.0;
                }
                midi_events.sort_by(|a, b| a.time_secs.partial_cmp(&b.time_secs).unwrap());
                // Only collect the instrument's own FX chain — group FX are
                // applied via the group bus to avoid double-processing the same
                // plugin instance.
                let inst_chain_plugins = self.collect_clip_chain_plugins(inst.effect_chain_id);
                let synth_latency = inst.gui.lock().ok()
                    .and_then(|g| g.as_ref().map(|gui| gui.get_latency_samples()))
                    .unwrap_or(0);
                let chain_latency = Self::collect_chain_latency(&inst_chain_plugins);
                let bus_idx = group_of.get(&inst_id)
                    .and_then(|gid| group_id_to_bus_idx.get(gid).copied());
                let group_latency = bus_idx
                    .map(|idx| group_buses[idx].latency_samples)
                    .unwrap_or(0);
                audio_instruments.push(audio::AudioInstrument {
                    id: inst_id,
                    x_start_px: x_min,
                    x_end_px: x_max,
                    y_start: 0.0,
                    y_end: 0.0,
                    gui: inst.gui.clone(),
                    midi_events,
                    volume: inst.volume,
                    pan: inst.pan,
                    chain_plugins: inst_chain_plugins,
                    total_latency_samples: synth_latency + chain_latency + group_latency,
                    group_bus_index: bus_idx,
                });
            }

            engine.update_instruments(audio_instruments);
        }
        self.sync_computer_keyboard_to_engine();
    }

    /// Build group bus mapping for instrument sync. Used by callers that don't
    /// already have group bus data (e.g. right-panel instrument edits).
    #[cfg(feature = "native")]
    fn build_group_bus_data(&self) -> (std::collections::HashMap<EntityId, usize>, Vec<audio::GroupBus>) {
        let mut group_id_to_bus_idx = std::collections::HashMap::new();
        let mut group_buses = Vec::new();
        for (&gid, group) in &self.groups {
            if !self.should_play_group(gid) {
                continue;
            }
            let plugins = self.collect_group_chain_plugins(gid);
            let has_audio_member = group.member_ids.iter().any(|mid| {
                self.waveforms.get(mid).map_or(false, |wf| !wf.disabled && self.audio_clips.contains_key(mid))
            });
            let has_instrument_member = group.member_ids.iter().any(|mid| {
                self.instruments.get(mid).map_or(false, |inst| inst.has_plugin())
            });
            if plugins.is_empty() && !has_audio_member && !has_instrument_member {
                continue;
            }
            let latency = Self::collect_chain_latency(&plugins);
            let bus_idx = group_buses.len();
            group_id_to_bus_idx.insert(gid, bus_idx);
            group_buses.push(audio::GroupBus { entity_id: gid, plugins, latency_samples: latency, volume: group.volume, pan: group.pan });
        }
        (group_id_to_bus_idx, group_buses)
    }

    /// Convenience: sync instruments using freshly built group bus data.
    #[cfg(feature = "native")]
    fn sync_instrument_regions_auto(&self) {
        let (map, buses) = self.build_group_bus_data();
        self.sync_instrument_regions(&map, &buses);
    }

    #[cfg(feature = "native")]
    pub(crate) fn sync_keyboard_instrument_from_selection(&mut self) {
        // Try to find instrument from selected MidiClip's instrument_id
        let clip_insts: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::MidiClip(id) => {
                    self.midi_clips.get(id).and_then(|mc| mc.instrument_id)
                }
                _ => None,
            })
            .collect();
        if clip_insts.len() == 1 && self.instruments.contains_key(&clip_insts[0]) {
            self.keyboard_instrument_id = Some(clip_insts[0]);
            return;
        }
        self.keyboard_instrument_id = None;
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
        let has_plugin = self.instruments.get(&id).map_or(false, |inst| inst.has_plugin());
        if has_plugin {
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
    fn toggle_monitoring(&mut self) {
        self.input_monitoring = !self.input_monitoring;

        // Set engine flag first so the input callback sees it when the stream starts
        if let Some(ref engine) = self.audio_engine {
            engine.set_monitoring_enabled(self.input_monitoring);
        }

        if let Some(ref mut recorder) = self.recorder {
            recorder.set_monitoring(self.input_monitoring);
        }

        self.sync_monitor_effects();
        self.request_redraw();
    }

    /// Find the parent waveform ID that owns a given child take.
    pub(crate) fn find_take_parent(&self, child_id: EntityId) -> Option<EntityId> {
        for (&parent_id, wf) in &self.waveforms {
            if let Some(tg) = &wf.take_group {
                if tg.contains(child_id) {
                    return Some(parent_id);
                }
            }
        }
        None
    }

    /// Switch the active take in a take group. `parent_id` is the parent waveform,
    /// `new_active_index` is the take index (0 = parent, 1+ = children).
    pub(crate) fn switch_active_take(&mut self, parent_id: EntityId, new_active_index: usize) {
        // Clone parent data upfront to avoid borrow conflicts
        let before = match self.waveforms.get(&parent_id).cloned() {
            Some(p) => p,
            None => return,
        };
        let tg = match &before.take_group {
            Some(tg) => tg.clone(),
            None => return,
        };
        if new_active_index >= tg.take_count() || new_active_index == tg.active_index {
            return;
        }

        let old_active = tg.active_index;

        // Disable old active take
        if old_active > 0 {
            if let Some(old_id) = tg.take_ids.get(old_active - 1) {
                if let Some(wf) = self.waveforms.get_mut(old_id) {
                    wf.disabled = true;
                }
            }
        }

        // Enable new active take
        if new_active_index > 0 {
            if let Some(new_id) = tg.take_ids.get(new_active_index - 1) {
                if let Some(wf) = self.waveforms.get_mut(new_id) {
                    wf.disabled = false;
                }
            }
        }

        // Update parent
        let mut after = before.clone();
        after.take_group.as_mut().unwrap().active_index = new_active_index;
        after.disabled = new_active_index != 0;
        if let Some(p) = self.waveforms.get_mut(&parent_id) {
            *p = after.clone();
        }

        self.push_op(operations::Operation::UpdateWaveform {
            id: parent_id,
            before,
            after,
        });
        self.sync_audio_clips();
        self.mark_dirty();
        self.request_redraw();
    }

    /// Toggle expand/collapse of a take group.
    pub(crate) fn toggle_take_expanded(&mut self, parent_id: EntityId) {
        let parent = match self.waveforms.get(&parent_id) {
            Some(p) => p,
            None => return,
        };
        if parent.take_group.is_none() {
            return;
        }

        let before = parent.clone();
        let mut after = before.clone();
        let tg = after.take_group.as_mut().unwrap();
        tg.expanded = !tg.expanded;

        if let Some(p) = self.waveforms.get_mut(&parent_id) {
            *p = after.clone();
        }
        self.push_op(operations::Operation::UpdateWaveform {
            id: parent_id,
            before,
            after,
        });
        self.mark_dirty();
        self.request_redraw();
    }

    #[cfg(not(feature = "native"))]
    fn toggle_monitoring(&mut self) {}

    #[cfg(feature = "native")]
    fn sync_monitor_effects(&self) {
        let engine = match &self.audio_engine {
            Some(e) => e,
            None => return,
        };

        if !self.input_monitoring {
            engine.update_monitor_effects(vec![]);
            return;
        }

        // Find recording waveform position to check spatial overlap with effect regions
        let wf = match self.recording_waveform_id.and_then(|id| self.waveforms.get(&id)) {
            Some(w) => w,
            None => {
                engine.update_monitor_effects(vec![]);
                return;
            }
        };

        let wf_y = wf.position[1];
        let wf_y_end = wf_y + wf.size[1];

        let plugins = Vec::new();
        engine.update_monitor_effects(plugins);
    }

    #[cfg(not(feature = "native"))]
    fn sync_monitor_effects(&self) {}

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
                    // Capture before-state for UpdateWaveform op sent after upload
                    let wf_before = self.waveforms.get(&wf_id).cloned();
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
                    // Encode recorded PCM as WAV bytes for storage
                    let wav_bytes = audio::encode_wav_bytes(
                        &loaded.left_samples,
                        &loaded.right_samples,
                        loaded.sample_rate,
                    );
                    if let Some(rs) = &self.remote_storage {
                        let wf_id_str = wf_id.to_string();
                        rs.save_audio(&wf_id_str, &wav_bytes, "wav");
                    }
                    self.source_audio_files.insert(wf_id, (wav_bytes, "wav".to_string()));
                    // Notify remote peers that recording is finalized and audio is available
                    if let Some(before) = wf_before {
                        if let Some(after) = self.waveforms.get(&wf_id) {
                            self.push_op(operations::Operation::UpdateWaveform {
                                id: wf_id,
                                before,
                                after: after.clone(),
                            });
                        }
                    }
                    // Finalize take group if recording into a selected waveform
                    if let Some(parent_id) = self.recording_take_parent_id.take() {
                        if let Some(parent) = self.waveforms.get(&parent_id) {
                            let before = parent.clone();
                            let mut after = before.clone();

                            if after.take_group.is_none() {
                                // First time: create take group on parent
                                after.take_group = Some(takes::TakeGroup {
                                    take_ids: vec![wf_id],
                                    active_index: 1, // new recording is active
                                    expanded: true,
                                });
                                // Disable the parent (it's now take 0, inactive)
                                after.disabled = true;
                            } else {
                                // Already has takes: append new take
                                let tg = after.take_group.as_mut().unwrap();
                                tg.take_ids.push(wf_id);
                                tg.active_index = tg.take_count() - 1; // new take is active

                                // Disable all other takes
                                let previously_active = before.take_group.as_ref().unwrap().active_index;
                                if previously_active == 0 {
                                    after.disabled = true;
                                } else if let Some(prev_id) = before.take_group.as_ref()
                                    .and_then(|tg| tg.take_ids.get(previously_active - 1))
                                {
                                    if let Some(prev_wf) = self.waveforms.get_mut(prev_id) {
                                        prev_wf.disabled = true;
                                    }
                                }
                            }

                            // Apply parent update
                            if let Some(p) = self.waveforms.get_mut(&parent_id) {
                                *p = after.clone();
                            }
                            self.push_op(operations::Operation::UpdateWaveform {
                                id: parent_id,
                                before,
                                after,
                            });
                        }
                    }

                    self.sync_audio_clips();
                }
            } else {
                if let Some(wf_id) = self.recording_waveform_id.take() {
                    self.waveforms.shift_remove(&wf_id);
                    self.audio_clips.shift_remove(&wf_id);
                }
                self.recording_take_parent_id = None;
            }

            if let Some(engine) = &self.audio_engine {
                if engine.is_playing() {
                    engine.toggle_playback();
                }
            }
        } else {
            let height = grid::clip_height(self.bpm);
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let sample_rate = self.recorder.as_ref().unwrap().sample_rate();

            // Check if a waveform is selected — if so, record as a new take
            // If a child take is selected, resolve to the parent waveform
            let selected_wf_id = self.selected.first().and_then(|t| match t {
                HitTarget::Waveform(id) => {
                    // If this waveform is a child take, use its parent instead
                    Some(self.find_take_parent(*id).unwrap_or(*id))
                },
                _ => None,
            });

            // Determine recording position: if recording into a take, position below parent
            let (rec_x, rec_y) = if let Some(parent_id) = selected_wf_id {
                if let Some(parent) = self.waveforms.get(&parent_id) {
                    let parent_h = parent.size[1];
                    let half_h = parent_h * 0.5;
                    // First child starts below parent at full height, subsequent at half-height each
                    let num_children = parent.take_group.as_ref()
                        .map(|tg| tg.take_ids.len())
                        .unwrap_or(0);
                    let child_y = parent.position[1] + parent_h + half_h * num_children as f32;
                    (parent.position[0], child_y)
                } else {
                    let world = self.last_canvas_click_world;
                    (world[0], world[1] - height * 0.5)
                }
            } else {
                let world = self.last_canvas_click_world;
                (world[0], world[1] - height * 0.5)
            };

            // Determine filename: "Take N" when recording into a take, "Recording" otherwise
            let rec_filename = if let Some(pid) = selected_wf_id {
                let num_children = self.waveforms.get(&pid)
                    .and_then(|wf| wf.take_group.as_ref())
                    .map(|tg| tg.take_ids.len())
                    .unwrap_or(0);
                format!("Take {}", num_children + 1)
            } else {
                format!("Recording {}", self.waveforms.len() + 1)
            };

            let wf_id = new_id();
            let wf_data = WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate,
                    filename: rec_filename.clone(),
                }),
                filename: rec_filename,
                position: [rec_x, rec_y],
                size: [0.0, selected_wf_id.and_then(|pid| self.waveforms.get(&pid)).map(|wf| wf.size[1] * 0.5).unwrap_or(height)],
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
                paulstretch_factor: 8.0,
                is_reversed: false,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
                effect_chain_id: None,
                take_group: None,
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
            self.recording_take_parent_id = selected_wf_id;
            self.recorder.as_mut().unwrap().start();

            if let Some(engine) = &self.audio_engine {
                let secs = rec_x as f64 / PIXELS_PER_SECOND as f64;
                engine.seek_to_seconds(secs);
                if !engine.is_playing() {
                    engine.toggle_playback();
                }
            }
            self.sync_monitor_effects();
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
    fn sync_instrument_regions(&self, _group_id_to_bus_idx: &std::collections::HashMap<EntityId, usize>, _group_buses: &[audio::GroupBus]) {}
    #[cfg(not(feature = "native"))]
    fn build_group_bus_data(&self) -> (std::collections::HashMap<EntityId, usize>, Vec<audio::GroupBus>) { (std::collections::HashMap::new(), Vec::new()) }
    #[cfg(not(feature = "native"))]
    fn sync_instrument_regions_auto(&self) {}
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
    fn load_browser_preview(&mut self, _path: &std::path::Path) {}
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
    fn add_plugin_to_waveform_chain(&mut self, _wf_id: EntityId, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn add_plugin_to_instrument_chain(&mut self, _inst_id: EntityId, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn open_effect_chain_slot_gui(&mut self, _chain_id: EntityId, _slot_idx: usize) {}
    #[cfg(not(feature = "native"))]
    fn add_instrument(&mut self, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn build_palette_plugin_entries(&self) -> Vec<PluginPickerEntry> { Vec::new() }
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
                warp_mode: match wf.warp_mode { ui::waveform::WarpMode::RePitch => 1, ui::waveform::WarpMode::Semitone => 2, ui::waveform::WarpMode::PaulStretch => 3, ui::waveform::WarpMode::Complex => 4, _ => 0 },
                sample_bpm: wf.sample_bpm,
                project_bpm: self.bpm,
                pitch_semitones: wf.pitch_semitones,
            })
            .collect();

        let effect_regions: Vec<audio::AudioEffectRegion> = Vec::new();

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

    pub(crate) fn set_target_pos(&mut self, target: &HitTarget, pos: [f32; 2]) {
        match target {
            HitTarget::Object(i) => { if let Some(o) = self.objects.get_mut(i) { o.position = pos; } }
            HitTarget::Waveform(i) => {
                // If this waveform is a take parent, move all child takes by the same delta
                let take_delta = self.waveforms.get(i).and_then(|w| {
                    let dx = pos[0] - w.position[0];
                    let dy = pos[1] - w.position[1];
                    w.take_group.as_ref().map(|tg| (tg.take_ids.clone(), dx, dy))
                });
                if let Some(w) = self.waveforms.get_mut(i) { w.position = pos; }
                if let Some((ids, dx, dy)) = take_delta {
                    for cid in &ids {
                        if let Some(cw) = self.waveforms.get_mut(cid) {
                            cw.position[0] += dx;
                            cw.position[1] += dy;
                        }
                    }
                }
            }
            HitTarget::LoopRegion(i) => { if let Some(l) = self.loop_regions.get_mut(i) { l.position[0] = pos[0]; } }
            HitTarget::ExportRegion(i) => { if let Some(e) = self.export_regions.get_mut(i) { e.position = pos; } }
            HitTarget::TextNote(i) => { if let Some(tn) = self.text_notes.get_mut(i) { tn.position = pos; } }
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
            HitTarget::Group(i) => {
                let (member_ids, dx, dy) = {
                    if let Some(g) = self.groups.get_mut(i) {
                        let dx = pos[0] - g.position[0];
                        let dy = pos[1] - g.position[1];
                        g.position = pos;
                        (g.member_ids.clone(), dx, dy)
                    } else { return; }
                };
                for mid in &member_ids {
                    if let Some(wf) = self.waveforms.get_mut(mid) { wf.position[0] += dx; wf.position[1] += dy; }
                    else if let Some(mc) = self.midi_clips.get_mut(mid) { mc.position[0] += dx; mc.position[1] += dy; }
                    else if let Some(tn) = self.text_notes.get_mut(mid) { tn.position[0] += dx; tn.position[1] += dy; }
                    else if let Some(obj) = self.objects.get_mut(mid) { obj.position[0] += dx; obj.position[1] += dy; }
                    else if let Some(lr) = self.loop_regions.get_mut(mid) { lr.position[0] += dx; }
                    else if let Some(xr) = self.export_regions.get_mut(mid) { xr.position[0] += dx; xr.position[1] += dy; }
                    else if let Some(c) = self.components.get_mut(mid) { c.position[0] += dx; c.position[1] += dy; }
                }
            }
            HitTarget::Instrument(_) => {}
        }
    }

    fn get_target_pos(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects.get(i).map(|o| o.position).unwrap_or([0.0; 2]),
            HitTarget::Waveform(i) => self.waveforms.get(i).map(|w| w.position).unwrap_or([0.0; 2]),
            HitTarget::LoopRegion(i) => self.loop_regions.get(i).map(|l| l.position).unwrap_or([0.0; 2]),
            HitTarget::ExportRegion(i) => self.export_regions.get(i).map(|e| e.position).unwrap_or([0.0; 2]),
            HitTarget::ComponentDef(i) => self.components.get(i).map(|c| c.position).unwrap_or([0.0; 2]),
            HitTarget::ComponentInstance(i) => self.component_instances.get(i).map(|c| c.position).unwrap_or([0.0; 2]),
            HitTarget::MidiClip(i) => self.midi_clips.get(i).map(|m| m.position).unwrap_or([0.0; 2]),
            HitTarget::TextNote(i) => self.text_notes.get(i).map(|t| t.position).unwrap_or([0.0; 2]),
            HitTarget::Group(i) => self.groups.get(i).map(|g| g.position).unwrap_or([0.0; 2]),
            HitTarget::Instrument(_) => [0.0; 2],
        }
    }

    fn get_target_size(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects.get(i).map(|o| o.size).unwrap_or([50.0; 2]),
            HitTarget::Waveform(i) => self.waveforms.get(i).map(|w| w.size).unwrap_or([50.0; 2]),
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
            HitTarget::TextNote(i) => self.text_notes.get(i).map(|t| t.size).unwrap_or([50.0; 2]),
            HitTarget::Group(i) => self.groups.get(i).map(|g| g.size).unwrap_or([50.0; 2]),
            HitTarget::Instrument(_) => [0.0; 2],
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
                    HitTarget::TextNote(i) => {
                        if let Some(tn) = self.text_notes.get(&i).cloned() {
                            let nid = new_id();
                            self.text_notes.insert(nid, tn.clone());
                            copy_ops.push(operations::Operation::CreateTextNote { id: nid, data: tn });
                            new_selected.push(HitTarget::TextNote(nid));
                        }
                    }
                    HitTarget::Group(i) => {
                        if let Some(g) = self.groups.get(&i).cloned() {
                            let mut g = g;
                            let old_member_ids = g.member_ids.clone();
                            let mut new_member_ids = Vec::new();
                            for mid in old_member_ids {
                                let new_mid = self.clone_entity(mid).unwrap_or(mid);
                                new_member_ids.push(new_mid);
                            }
                            g.member_ids = new_member_ids;
                            let nid = new_id();
                            // Emit create ops for cloned members so undo removes them
                            for mid in &g.member_ids {
                                if let Some(w) = self.waveforms.get(mid) {
                                    let ac = self.audio_clips.get(mid).cloned();
                                    copy_ops.push(operations::Operation::CreateWaveform { id: *mid, data: w.clone(), audio_clip: ac.map(|c| (*mid, c)) });
                                } else if let Some(mc) = self.midi_clips.get(mid) {
                                    copy_ops.push(operations::Operation::CreateMidiClip { id: *mid, data: mc.clone() });
                                } else if let Some(obj) = self.objects.get(mid) {
                                    copy_ops.push(operations::Operation::CreateObject { id: *mid, data: obj.clone() });
                                } else if let Some(tn) = self.text_notes.get(mid) {
                                    copy_ops.push(operations::Operation::CreateTextNote { id: *mid, data: tn.clone() });
                                } else if let Some(lr) = self.loop_regions.get(mid) {
                                    copy_ops.push(operations::Operation::CreateLoopRegion { id: *mid, data: lr.clone() });
                                } else if let Some(xr) = self.export_regions.get(mid) {
                                    copy_ops.push(operations::Operation::CreateExportRegion { id: *mid, data: xr.clone() });
                                } else if let Some(ci) = self.component_instances.get(mid) {
                                    copy_ops.push(operations::Operation::CreateComponentInstance { id: *mid, data: ci.clone() });
                                }
                            }
                            self.groups.insert(nid, g.clone());
                            copy_ops.push(operations::Operation::CreateGroup { id: nid, data: g });
                            new_selected.push(HitTarget::Group(nid));
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
                    HitTarget::Instrument(_) => {}
                }
            }
            self.selected = new_selected;
            if !copy_ops.is_empty() {
                self.push_op(operations::Operation::Batch(copy_ops));
            }
        }

        // Capture before states for all selected entities
        let mut before_states: Vec<(HitTarget, EntityBeforeState)> = self.selected.iter().filter_map(|t| {
            match t {
                HitTarget::Object(id) => self.objects.get(id).map(|o| (*t, EntityBeforeState::Object(o.clone()))),
                HitTarget::Waveform(id) => self.waveforms.get(id).map(|w| (*t, EntityBeforeState::Waveform(w.clone()))),
                HitTarget::LoopRegion(id) => self.loop_regions.get(id).map(|l| (*t, EntityBeforeState::LoopRegion(l.clone()))),
                HitTarget::ExportRegion(id) => self.export_regions.get(id).map(|x| (*t, EntityBeforeState::ExportRegion(x.clone()))),
                HitTarget::ComponentDef(id) => self.components.get(id).map(|c| (*t, EntityBeforeState::ComponentDef(c.clone()))),
                HitTarget::ComponentInstance(id) => self.component_instances.get(id).map(|c| (*t, EntityBeforeState::ComponentInstance(c.clone()))),
                HitTarget::MidiClip(id) => self.midi_clips.get(id).map(|m| (*t, EntityBeforeState::MidiClip(m.clone()))),
                HitTarget::TextNote(id) => self.text_notes.get(id).map(|tn| (*t, EntityBeforeState::TextNote(tn.clone()))),
                HitTarget::Group(id) => self.groups.get(id).map(|g| (*t, EntityBeforeState::Group(g.clone()))),
                HitTarget::Instrument(_) => None,
            }
        }).collect();

        // Also capture before-states for group members so undo/redo works
        let member_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| {
            if let HitTarget::Group(id) = t {
                self.groups.get(id).map(|g| g.member_ids.clone())
            } else {
                None
            }
        }).flatten().collect();
        let existing_ids: HashSet<HitTarget> = before_states.iter().map(|(t, _)| *t).collect();
        for mid in &member_ids {
            if let Some(wf) = self.waveforms.get(mid) {
                let t = HitTarget::Waveform(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::Waveform(wf.clone()))); }
            } else if let Some(mc) = self.midi_clips.get(mid) {
                let t = HitTarget::MidiClip(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::MidiClip(mc.clone()))); }
            } else if let Some(tn) = self.text_notes.get(mid) {
                let t = HitTarget::TextNote(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::TextNote(tn.clone()))); }
            } else if let Some(obj) = self.objects.get(mid) {
                let t = HitTarget::Object(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::Object(obj.clone()))); }
            } else if let Some(lr) = self.loop_regions.get(mid) {
                let t = HitTarget::LoopRegion(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::LoopRegion(lr.clone()))); }
            } else if let Some(xr) = self.export_regions.get(mid) {
                let t = HitTarget::ExportRegion(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::ExportRegion(xr.clone()))); }
            } else if let Some(c) = self.components.get(mid) {
                let t = HitTarget::ComponentDef(*mid);
                if !existing_ids.contains(&t) { before_states.push((t, EntityBeforeState::ComponentDef(c.clone()))); }
            }
        }

        // Also capture before-states for take children so undo/redo works
        let take_child_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| {
            if let HitTarget::Waveform(id) = t {
                self.waveforms.get(id).and_then(|wf| wf.take_group.as_ref())
                    .map(|tg| tg.take_ids.clone())
            } else { None }
        }).flatten().collect();
        let existing_ids2: HashSet<HitTarget> = before_states.iter().map(|(t, _)| *t).collect();
        for cid in &take_child_ids {
            if let Some(wf) = self.waveforms.get(cid) {
                let t = HitTarget::Waveform(*cid);
                if !existing_ids2.contains(&t) { before_states.push((t, EntityBeforeState::Waveform(wf.clone()))); }
            }
        }

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
                    (HitTarget::TextNote(id), EntityBeforeState::TextNote(before)) => {
                        if let Some(after) = self.text_notes.get(&id) {
                            ops.push(crate::operations::Operation::UpdateTextNote { id, before, after: after.clone() });
                        }
                    }
                    (HitTarget::Group(id), EntityBeforeState::Group(before)) => {
                        if let Some(after) = self.groups.get(&id) {
                            ops.push(crate::operations::Operation::UpdateGroup { id, before, after: after.clone() });
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
            // Update group bounds for any nudged members
            let nudged_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t {
                HitTarget::Waveform(id) | HitTarget::MidiClip(id)
                | HitTarget::TextNote(id) | HitTarget::Object(id) => Some(*id),
                _ => None,
            }).collect();
            for id in nudged_ids {
                self.update_groups_containing(id);
            }
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
                    HitTarget::LoopRegion(id) => self.loop_regions.get(id).map(|l| (*t, EntityBeforeState::LoopRegion(l.clone()))),
                    HitTarget::ExportRegion(id) => self.export_regions.get(id).map(|x| (*t, EntityBeforeState::ExportRegion(x.clone()))),
                    HitTarget::ComponentDef(id) => self.components.get(id).map(|c| (*t, EntityBeforeState::ComponentDef(c.clone()))),
                    HitTarget::ComponentInstance(id) => self.component_instances.get(id).map(|c| (*t, EntityBeforeState::ComponentInstance(c.clone()))),
                    HitTarget::MidiClip(id) => self.midi_clips.get(id).map(|m| (*t, EntityBeforeState::MidiClip(m.clone()))),
                    HitTarget::TextNote(id) => self.text_notes.get(id).map(|tn| (*t, EntityBeforeState::TextNote(tn.clone()))),
                    HitTarget::Group(id) => self.groups.get(id).map(|g| (*t, EntityBeforeState::Group(g.clone()))),
                    HitTarget::Instrument(_) => None,
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

    /// Spawn audio loading on a background thread. A placeholder waveform
    /// (empty audio) is placed on the canvas immediately so the user sees
    /// feedback. When decoding finishes the placeholder is filled in by
    #[cfg(feature = "native")]
    fn load_browser_preview(&mut self, path: &std::path::Path) {
        // Stop any currently playing preview
        if let Some(engine) = &self.audio_engine {
            engine.stop_preview();
        }

        let path_owned = path.to_owned();
        self.sample_browser.preview_path = Some(path_owned.clone());
        self.sample_browser.preview_audio = None;
        self.sample_browser.text_dirty = true;

        let tx = self.pending_audio_tx.clone();
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        std::thread::spawn(move || {
            let Some(loaded) = load_audio_file(&path_owned) else {
                return;
            };
            let left_peaks = std::sync::Arc::new(WaveformPeaks::build(&loaded.left_samples));
            let right_peaks = std::sync::Arc::new(WaveformPeaks::build(&loaded.right_samples));
            let audio = std::sync::Arc::new(AudioData {
                left_samples: loaded.left_samples.clone(),
                right_samples: loaded.right_samples.clone(),
                left_peaks,
                right_peaks,
                sample_rate: loaded.sample_rate,
                filename,
            });
            let _ = tx.send(PendingAudioLoad::PreviewLoaded {
                path: path_owned,
                audio,
                left_samples: loaded.left_samples,
                right_samples: loaded.right_samples,
                sample_rate: loaded.sample_rate,
            });
        });
    }

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
            paulstretch_factor: 8.0,
            is_reversed: false,
            disabled: true, // disabled until loaded
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
            effect_chain_id: None,
            take_group: None,
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
                paulstretch_factor: 8.0,
                is_reversed: false,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
                effect_chain_id: None,
                take_group: None,
            };
            let ac_data = AudioClipData {
                samples: loaded.samples.clone(),
                sample_rate: loaded.sample_rate,
                duration_secs: loaded.duration_secs,
            };

            // Read original file bytes for storage
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("wav")
                .to_string();
            let source_file = match std::fs::read(&path) {
                Ok(bytes) => (bytes, ext.clone()),
                Err(e) => {
                    eprintln!("[BgAudioLoad] Failed to read file bytes: {e}");
                    (Vec::new(), ext.clone())
                }
            };

            if let Some(rs) = &rs {
                // Remote storage mode: defer waveform display until upload completes.
                // Do NOT send Decoded — keep the placeholder visible with "uploading..." label.
                let wf_id_str = wf_id.to_string();
                let save_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rs.save_audio(&wf_id_str, &source_file.0, &ext);
                }));
                match save_result {
                    Ok(()) => {
                        println!("[BgAudioLoad] Remote save done for {wf_id}, sending SyncReady");
                        let _ = tx.send(PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data, source_file });
                    }
                    Err(e) => {
                        eprintln!("[BgAudioLoad] Remote save PANICKED for {wf_id}: {e:?}");
                        let _ = tx.send(PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data, source_file });
                    }
                }
            } else {
                // Local-only mode: show waveform immediately after decode.
                let _ = tx.send(PendingAudioLoad::Decoded {
                    wf_id,
                    wf_data,
                    ac_data,
                    source_file,
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
                PendingAudioLoad::Decoded { wf_id, wf_data, ac_data, source_file } => {
                    self.waveforms.insert(wf_id, wf_data.clone());
                    self.audio_clips.insert(wf_id, ac_data.clone());
                    if !source_file.0.is_empty() {
                        self.source_audio_files.insert(wf_id, source_file);
                    }
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
                PendingAudioLoad::SyncReady { wf_id, wf_data, ac_data, source_file } => {
                    self.waveforms.insert(wf_id, wf_data.clone());
                    self.audio_clips.insert(wf_id, ac_data.clone());
                    if !source_file.0.is_empty() {
                        self.source_audio_files.insert(wf_id, source_file);
                    }
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
                PendingAudioLoad::PreviewLoaded { path, audio, left_samples, right_samples, sample_rate } => {
                    // Only apply if this is still the requested preview
                    if self.sample_browser.preview_path.as_ref() == Some(&path) {
                        self.sample_browser.preview_audio = Some(audio);
                        self.sample_browser.text_dirty = true;
                        if self.sample_browser.auto_preview {
                            #[cfg(feature = "native")]
                            if let Some(engine) = &self.audio_engine {
                                engine.play_preview(left_samples, right_samples, sample_rate);
                            }
                        }
                    }
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

    let db_password = std::env::args()
        .position(|a| a == "--db-password")
        .and_then(|i| std::env::args().nth(i + 1));

    let event_loop = EventLoop::new().unwrap();

    let mut app = App::new(skip_load);
    let menu_state = project::build_app_menu(app.storage.as_ref());
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
        if let Some(rs) = storage::RemoteStorage::connect(url, db_password.as_deref(), rt) {
            rs.use_project(pid);
            println!("[RemoteStorage] Connected to {url}, project '{pid}'");
            app.remote_storage = Some(Arc::new(rs));
        } else {
            eprintln!("[RemoteStorage] Failed to connect to {url}");
        }

        // Real-time sync via SurrealDB live queries
        app.connect_to_server(url, pid, db_password.as_deref());
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
