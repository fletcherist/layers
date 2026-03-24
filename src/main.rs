#[cfg(feature = "native")]
mod project;
#[cfg(feature = "native")]
mod audio;
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
mod midi;
mod midi_keyboard;
mod network;
mod operations;
mod overlap;
#[cfg(feature = "native")]
mod surreal_client;
#[cfg(feature = "native")]
mod plugins;
mod regions;
mod settings;
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
use storage::{default_base_path, Storage};
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

use automation::{AutomationData, AutomationParam};

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
    editing_effect_name: Option<(EntityId, String)>,
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
            editing_effect_name: None,
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
            &self.instruments,
            &self.midi_clips,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
            &self.groups,
        );
        let rows = layers::flatten_tree(
            &self.layer_tree,
            &self.instruments,
            &self.midi_clips,
            &self.waveforms,
            &self.effect_regions,
            &self.plugin_blocks,
            &self.groups,
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
            stored_layer_tree,
            restored_text_notes,
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
                        is_reversed: false,
                        disabled: sw.disabled,
                        sample_offset_px: sw.sample_offset_px,
                        automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
                        effect_chain_id: None,
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
                    Vec::new(),  // stored_layer_tree
                    IndexMap::new(),  // text_notes
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
                &restored_effect_regions,
                &restored_plugin_blocks,
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
            audio_engine,
            recorder,
            recording_waveform_id: None,
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
            editing_effect_name: None,
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

        // Clear stale state from previous session before installing new network
        self.remote_users.clear();
        self.applied_remote_seqs.clear();

        self.network = mgr;
        self.connect_url = Some(url.to_string());
        self.connect_project_id = Some(project_id.to_string());
        self.pending_welcome = Some(welcome_rx);
        log::info!("Connecting to SurrealDB at {}", url);
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

    pub(crate) fn update_right_window(&mut self) {
        if let Some(HitTarget::Waveform(id)) = self.selected.first().copied() {
            if let Some(wf) = self.waveforms.get(&id) {
                // Preserve vol_entry when updating the same waveform so that
                // click-to-edit isn't reset by the unconditional update_right_window
                // call at the end of the mouse-released handler.
                let (vol_entry, sample_bpm_entry, pitch_entry, vol_fader_focused, pan_knob_focused, pitch_focused, sample_bpm_focused) = if self.right_window.as_ref().map_or(false, |rw| rw.target_id() == id) {
                    let rw = self.right_window.take().unwrap();
                    (rw.vol_entry, rw.sample_bpm_entry, rw.pitch_entry, rw.vol_fader_focused, rw.pan_knob_focused, rw.pitch_focused, rw.sample_bpm_focused)
                } else {
                    (ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), ui::value_entry::ValueEntry::new(), false, false, false, false)
                };
                self.right_window = Some(ui::right_window::RightWindow {
                    target: ui::right_window::RightWindowTarget::Waveform(id),
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
            });
        }
    }

    /// Detach a waveform's effect chain — clone the shared chain into a new independent one.
    pub(crate) fn detach_effect_chain(&mut self, wf_id: EntityId) {
        let chain_id = match self.waveforms.get(&wf_id).and_then(|w| w.effect_chain_id) {
            Some(id) => id,
            None => return,
        };
        let ref_count = ui::right_window::RightWindow::chain_ref_count(chain_id, &self.waveforms);
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
        let ref_count = ui::right_window::RightWindow::chain_ref_count_all(chain_id, &self.waveforms, &self.instruments);
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
        self.sync_monitor_effects();
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
    fn sync_instrument_regions(&self) {
        if let Some(engine) = &self.audio_engine {
            let mut audio_instruments = Vec::new();

            // Build from lightweight instruments (new path)
            for (&inst_id, inst) in self.instruments.iter() {
                if !inst.has_plugin() {
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
                });
            }

            engine.update_instruments(audio_instruments);
        }
        self.sync_computer_keyboard_to_engine();
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

        let mut plugins = Vec::new();
        for er in self.effect_regions.values() {
            let ey = er.position[1];
            let ey_end = ey + er.size[1];
            // Check vertical overlap
            if wf_y < ey_end && wf_y_end > ey {
                let block_ids = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                for id in &block_ids {
                    if let Some(pb) = self.plugin_blocks.get(id) {
                        plugins.push(pb.gui.clone());
                    }
                }
            }
        }
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
                is_reversed: false,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            effect_chain_id: None,
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
    fn add_plugin_to_waveform_chain(&mut self, _wf_id: EntityId, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn add_plugin_to_instrument_chain(&mut self, _inst_id: EntityId, _plugin_id: &str, _plugin_name: &str) {}
    #[cfg(not(feature = "native"))]
    fn open_effect_chain_slot_gui(&mut self, _chain_id: EntityId, _slot_idx: usize) {}
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
            HitTarget::Group(i) => { if let Some(g) = self.groups.get_mut(i) { g.position = pos; } }
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
            HitTarget::TextNote(i) => self.text_notes.get(i).map(|t| t.position).unwrap_or([0.0; 2]),
            HitTarget::Group(i) => self.groups.get(i).map(|g| g.position).unwrap_or([0.0; 2]),
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
            HitTarget::TextNote(i) => self.text_notes.get(i).map(|t| t.size).unwrap_or([50.0; 2]),
            HitTarget::Group(i) => self.groups.get(i).map(|g| g.size).unwrap_or([50.0; 2]),
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
                            let nid = new_id();
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
                HitTarget::TextNote(id) => self.text_notes.get(id).map(|tn| (*t, EntityBeforeState::TextNote(tn.clone()))),
                HitTarget::Group(id) => self.groups.get(id).map(|g| (*t, EntityBeforeState::Group(g.clone()))),
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
                    (HitTarget::TextNote(id), EntityBeforeState::TextNote(before)) => {
                        if let Some(after) = self.text_notes.get(&id) {
                            ops.push(crate::operations::Operation::UpdateTextNote { id, before, after: after.clone() });
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
                    HitTarget::TextNote(id) => self.text_notes.get(id).map(|tn| (*t, EntityBeforeState::TextNote(tn.clone()))),
                    HitTarget::Group(id) => self.groups.get(id).map(|g| (*t, EntityBeforeState::Group(g.clone()))),
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
            is_reversed: false,
            disabled: true, // disabled until loaded
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
        effect_chain_id: None,
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
                is_reversed: false,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            effect_chain_id: None,
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
