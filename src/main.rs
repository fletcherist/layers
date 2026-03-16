mod audio;
mod automation;
mod component;
mod effects;
mod events;
mod gpu;
mod grid;
mod history;
mod hit_testing;
mod instruments;
mod midi;
mod plugins;
mod regions;
mod rendering;
mod settings;
mod storage;
mod ui;

#[cfg(test)]
mod tests;

pub(crate) use gpu::{push_border, Camera, Gpu, InstanceRaw};
pub(crate) use ui::transport::{TransportPanel, TRANSPORT_WIDTH};

use grid::{grid_spacing_for_settings, snap_to_clip_grid, snap_to_grid, DEFAULT_BPM};
use hit_testing::{
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
use rendering::{build_instances, build_waveform_vertices, default_objects, RenderContext};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use audio::{load_audio_file, AudioClipData, AudioEngine, AudioRecorder, PIXELS_PER_SECOND};
use settings::GridMode;
use ui::context_menu::{ContextMenu, MenuContext};
use ui::palette::{
    CommandAction, CommandPalette, PaletteMode, PaletteRow, PluginPickerEntry, COMMANDS,
    PALETTE_ITEM_HEIGHT, db_to_gain, gain_to_db,
};
pub(crate) use ui::waveform::WaveformView;
use ui::waveform::{AudioData, WaveformPeaks, WaveformVertex};

use surrealdb::types::SurrealValue;

use muda::{MenuId, Submenu as MudaSubmenu};
use settings::{Settings, SettingsWindow, CATEGORIES};
use storage::{default_base_path, ProjectState, Storage};
use winit::{
    event_loop::EventLoop,
    keyboard::ModifiersState,
    window::CursorIcon,
};

// ---------------------------------------------------------------------------
// Canvas objects
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct CanvasObject {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HitTarget {
    Object(usize),
    Waveform(usize),
    EffectRegion(usize),
    PluginBlock(usize),
    LoopRegion(usize),
    ExportRegion(usize),
    ComponentDef(usize),
    ComponentInstance(usize),
    MidiClip(usize),
    InstrumentRegion(usize),
}

use automation::{AutomationData, AutomationParam};
use history::Snapshot;


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
    },
    DraggingFromBrowser {
        path: PathBuf,
        filename: String,
    },
    DraggingPlugin {
        plugin_id: String,
        plugin_name: String,
    },
    ResizingBrowser,
    ResizingExportRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    DraggingFade {
        waveform_idx: usize,
        is_fade_in: bool,
    },
    DraggingFadeCurve {
        waveform_idx: usize,
        is_fade_in: bool,
        start_mouse_y: f32,
        start_curve: f32,
    },
    ResizingComponentDef {
        comp_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingEffectRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingLoopRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingWaveform {
        waveform_idx: usize,
        is_left_edge: bool,
        initial_position_x: f32,
        initial_size_w: f32,
        initial_offset_px: f32,
    },
    DraggingAutomationPoint {
        waveform_idx: usize,
        param: AutomationParam,
        point_idx: usize,
        original_t: f32,
        original_value: f32,
    },
    ResizingInstrumentRegion {
        region_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    ResizingMidiClip {
        clip_idx: usize,
        anchor: [f32; 2],
        nwse: bool,
    },
    MovingMidiClip {
        clip_idx: usize,
        offset: [f32; 2],
    },
    MovingMidiNote {
        clip_idx: usize,
        note_indices: Vec<usize>,
        offsets: Vec<[f32; 2]>,
        start_world: [f32; 2],
    },
    ResizingMidiNote {
        clip_idx: usize,
        anchor_idx: usize,
        note_indices: Vec<usize>,
        original_durations: Vec<f32>,
    },
    ResizingMidiNoteLeft {
        clip_idx: usize,
        anchor_idx: usize,
        note_indices: Vec<usize>,
        original_starts: Vec<f32>,
        original_durations: Vec<f32>,
    },
    SelectingMidiNotes {
        clip_idx: usize,
        start_world: [f32; 2],
    },
    DraggingVelocity {
        clip_idx: usize,
        note_indices: Vec<usize>,
        original_velocities: Vec<u8>,
        start_world_y: f32,
    },
    ResizingVelocityLane {
        clip_idx: usize,
        start_world_y: f32,
        original_height: f32,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum ComponentDefHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum EffectRegionHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum InstrumentRegionHover {
    None,
    CornerNW(usize),
    CornerNE(usize),
    CornerSW(usize),
    CornerSE(usize),
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

pub(crate) const WAVEFORM_COLORS: &[[f32; 4]] = &[
    [1.00, 0.24, 0.19, 1.0], // red
    [1.00, 0.42, 0.24, 1.0], // orange-red
    [1.00, 0.58, 0.00, 1.0], // orange
    [1.00, 0.72, 0.00, 1.0], // amber
    [1.00, 0.84, 0.00, 1.0], // yellow
    [0.78, 0.90, 0.19, 1.0], // lime
    [0.30, 0.85, 0.39, 1.0], // green
    [0.19, 0.84, 0.55, 1.0], // mint
    [0.19, 0.78, 0.71, 1.0], // teal
    [0.19, 0.78, 0.90, 1.0], // cyan
    [0.35, 0.78, 0.98, 1.0], // sky blue
    [0.00, 0.48, 1.00, 1.0], // blue
    [0.35, 0.34, 0.84, 1.0], // indigo
    [0.69, 0.32, 0.87, 1.0], // violet
    [0.88, 0.25, 0.63, 1.0], // magenta
    [1.00, 0.18, 0.33, 1.0], // rose
];

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

struct App {
    gpu: Option<Gpu>,
    camera: Camera,
    objects: Vec<CanvasObject>,
    waveforms: Vec<WaveformView>,
    audio_clips: Vec<AudioClipData>,
    audio_engine: Option<AudioEngine>,
    recorder: Option<AudioRecorder>,
    recording_waveform_idx: Option<usize>,
    last_canvas_click_world: [f32; 2],
    selected: Vec<HitTarget>,
    drag: DragState,
    mouse_pos: [f32; 2],
    hovered: Option<HitTarget>,
    fade_handle_hovered: Option<(usize, bool)>,
    fade_curve_hovered: Option<(usize, bool)>,
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
    storage: Option<Storage>,
    has_saved_state: bool,
    project_dirty: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    current_project_name: String,
    effect_regions: Vec<effects::EffectRegion>,
    plugin_blocks: Vec<effects::PluginBlock>,
    components: Vec<component::ComponentDef>,
    component_instances: Vec<component::ComponentInstance>,
    next_component_id: component::ComponentId,
    plugin_registry: effects::PluginRegistry,
    export_regions: Vec<ExportRegion>,
    export_hover: ExportHover,
    loop_regions: Vec<LoopRegion>,
    loop_hover: LoopHover,
    select_area: Option<SelectArea>,
    component_def_hover: ComponentDefHover,
    effect_region_hover: EffectRegionHover,
    instrument_region_hover: InstrumentRegionHover,
    midi_clips: Vec<midi::MidiClip>,
    instrument_regions: Vec<instruments::InstrumentRegion>,
    editing_midi_clip: Option<usize>,
    selected_midi_notes: Vec<usize>,
    pending_midi_note_click: Option<usize>,
    midi_note_select_rect: Option<[f32; 4]>,
    cmd_velocity_hover_note: Option<(usize, usize)>,
    editing_component: Option<usize>,
    editing_effect_name: Option<(usize, String)>,
    editing_waveform_name: Option<(usize, String)>,
    bpm: f32,
    editing_bpm: Option<String>,
    dragging_bpm: Option<(f32, f32)>,
    last_click_time: std::time::Instant,
    last_click_world: [f32; 2],
    clipboard: Clipboard,
    settings: Settings,
    settings_window: Option<SettingsWindow>,
    plugin_editor: Option<ui::plugin_editor::PluginEditorWindow>,
    menu_state: Option<MenuState>,
    toast_manager: ui::toast::ToastManager,
    automation_mode: bool,
    active_automation_param: AutomationParam,
    cached_instances: Vec<InstanceRaw>,
    cached_wf_verts: Vec<WaveformVertex>,
    render_generation: u64,
    last_rendered_generation: u64,
    last_rendered_camera_pos: [f32; 2],
    last_rendered_camera_zoom: f32,
    last_rendered_hovered: Option<HitTarget>,
    last_rendered_selected_len: usize,
}

impl App {
    #[cfg(test)]
    pub(crate) fn new_headless() -> Self {
        Self {
            gpu: None,
            camera: Camera::new(),
            objects: Vec::new(),
            waveforms: Vec::new(),
            audio_clips: Vec::new(),
            audio_engine: None,
            recorder: None,
            recording_waveform_idx: None,
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
            storage: None,
            has_saved_state: false,
            project_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_project_name: "test".into(),
            effect_regions: Vec::new(),
            plugin_blocks: Vec::new(),
            components: Vec::new(),
            component_instances: Vec::new(),
            next_component_id: 1,
            plugin_registry: effects::PluginRegistry::new(),
            export_regions: Vec::new(),
            export_hover: ExportHover::None,
            loop_regions: Vec::new(),
            loop_hover: LoopHover::None,
            select_area: None,
            component_def_hover: ComponentDefHover::None,
            effect_region_hover: EffectRegionHover::None,
            instrument_region_hover: InstrumentRegionHover::None,
            midi_clips: Vec::new(),
            instrument_regions: Vec::new(),
            editing_midi_clip: None,
            selected_midi_notes: Vec::new(),
            pending_midi_note_click: None,
            midi_note_select_rect: None,
            cmd_velocity_hover_note: None,
            editing_component: None,
            editing_effect_name: None,
            editing_waveform_name: None,
            bpm: 120.0,
            editing_bpm: None,
            dragging_bpm: None,
            last_click_time: std::time::Instant::now(),
            last_click_world: [0.0; 2],
            clipboard: Clipboard::new(),
            settings: Settings::default(),
            settings_window: None,
            plugin_editor: None,
            menu_state: None,
            toast_manager: ui::toast::ToastManager::new(),
            automation_mode: false,
            active_automation_param: AutomationParam::Volume,
            cached_instances: Vec::new(),
            cached_wf_verts: Vec::new(),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
        }
    }

    fn mark_dirty(&mut self) {
        self.render_generation = self.render_generation.wrapping_add(1);
        self.project_dirty = true;
    }

    /// Tear down plugin GUIs and instances in the correct order before exit.
    /// GUIs must be destroyed before plugin instances they reference.
    fn shutdown_plugins(&mut self) {
        // Stop audio engine first so the audio thread releases plugin locks
        self.audio_engine = None;

        // Destroy instrument region GUIs (single instance handles both GUI + audio)
        for ir in &mut self.instrument_regions {
            if let Ok(mut g) = ir.gui.lock() {
                *g = None;
            }
        }

        // Destroy plugin block GUIs
        for pb in &mut self.plugin_blocks {
            if let Ok(mut g) = pb.gui.lock() {
                *g = None;
            }
        }
    }

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
                let bw = if state.browser_width > 0.0 {
                    state.browser_width
                } else {
                    260.0
                };
                let expanded: HashSet<PathBuf> =
                    state.browser_expanded.iter().map(PathBuf::from).collect();

                let num_waveforms = state.waveforms.len();
                let mut waveforms: Vec<WaveformView> = state
                    .waveforms
                    .into_iter()
                    .map(|sw| WaveformView {
                        audio: Arc::new(AudioData {
                            left_samples: Arc::new(Vec::new()),
                            right_samples: Arc::new(Vec::new()),
                            left_peaks: Arc::new(WaveformPeaks::empty()),
                            right_peaks: Arc::new(WaveformPeaks::empty()),
                            sample_rate: sw.sample_rate,
                            filename: sw.filename,
                        }),
                        position: sw.position,
                        size: sw.size,
                        color: sw.color,
                        border_radius: sw.border_radius,
                        fade_in_px: sw.fade_in_px,
                        fade_out_px: sw.fade_out_px,
                        fade_in_curve: sw.fade_in_curve,
                        fade_out_curve: sw.fade_out_curve,
                        volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                        disabled: sw.disabled,
                        sample_offset_px: sw.sample_offset_px,
                        automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
                    })
                    .collect();

                // Restore audio data and peaks from DB
                let mut audio_clips: Vec<AudioClipData> = Vec::new();
                if let Some(s) = &storage {
                    for i in 0..num_waveforms {
                        let mut left_samples = Arc::new(Vec::new());
                        let mut right_samples = Arc::new(Vec::new());
                        let mut sample_rate = waveforms[i].audio.sample_rate;
                        let mut left_peaks = waveforms[i].audio.left_peaks.clone();
                        let mut right_peaks = waveforms[i].audio.right_peaks.clone();

                        if let Some(audio) = s.load_audio(i as u64) {
                            left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                            right_samples =
                                Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                            let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                            sample_rate = audio.sample_rate;
                            audio_clips.push(AudioClipData {
                                samples: Arc::new(mono),
                                sample_rate: audio.sample_rate,
                                duration_secs: audio.duration_secs,
                            });
                        } else {
                            audio_clips.push(AudioClipData {
                                samples: Arc::new(Vec::new()),
                                sample_rate: 48000,
                                duration_secs: 0.0,
                            });
                        }
                        if let Some(peaks) = s.load_peaks(i as u64) {
                            let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                            let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                            left_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                            right_peaks =
                                Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                        }
                        waveforms[i].audio = Arc::new(AudioData {
                            left_samples,
                            right_samples,
                            left_peaks,
                            right_peaks,
                            sample_rate,
                            filename: waveforms[i].audio.filename.clone(),
                        });
                    }
                }

                (
                    cam,
                    state.objects,
                    waveforms,
                    name,
                    folders,
                    bw,
                    state.browser_visible,
                    Some(expanded),
                    state.effect_regions,
                    state.plugin_blocks,
                    state.loop_regions,
                    state.components,
                    state.component_instances,
                    audio_clips,
                    if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM },
                    state.midi_clips,
                    state.instrument_regions,
                )
            }
            None => {
                println!("  No saved project found, starting fresh");
                (
                    Camera::new(),
                    default_objects(),
                    Vec::new(),
                    "Untitled".to_string(),
                    Vec::new(),
                    260.0,
                    false,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    DEFAULT_BPM,
                    Vec::new(),
                    Vec::new(),
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
        } else {
            println!("  Warning: no audio output device found");
        }

        let recorder = AudioRecorder::new();
        if recorder.is_none() {
            println!("  Warning: no audio input device found");
        }

        let plugin_registry = effects::PluginRegistry::new();

        // Restore effect region geometry
        let restored_effect_regions: Vec<effects::EffectRegion> = stored_effect_regions
            .into_iter()
            .map(|ser| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                region
            })
            .collect();

        // Restore plugin blocks; instances will be loaded lazily on first scan
        let mut restored_plugin_blocks: Vec<effects::PluginBlock> = stored_plugin_blocks
            .into_iter()
            .map(|spb| {
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
                pb
            })
            .collect();

        // Migration: if old project had plugins in regions but no plugin_blocks, generate them
        if restored_plugin_blocks.is_empty() {
            // Read the raw stored regions before we stripped plugin data
            // We already have the effect_regions loaded; check the original stored data
            // by looking at the stored_effect_regions we consumed above.
            // Unfortunately they're consumed, so use a separate path: if load found
            // plugin_ids inside stored regions, we re-read from storage to migrate.
            if let Some(s) = &storage {
                if let Some(raw_state) = s.load_project_state() {
                    for (_ri, ser) in raw_state.effect_regions.iter().enumerate() {
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
                            restored_plugin_blocks.push(pb);
                            x_offset += effects::PLUGIN_BLOCK_DEFAULT_SIZE[0] + 10.0;
                        }
                    }
                    if !restored_plugin_blocks.is_empty() {
                        println!("  Migrated {} plugin blocks from old region format", restored_plugin_blocks.len());
                    }
                }
            }
        }

        let restored_loop_regions: Vec<LoopRegion> = stored_loop_regions
            .into_iter()
            .map(|slr| LoopRegion {
                position: slr.position,
                size: slr.size,
                enabled: slr.enabled,
            })
            .collect();

        let restored_components: Vec<component::ComponentDef> = stored_components
            .into_iter()
            .map(|sc| component::ComponentDef {
                id: sc.id,
                name: sc.name,
                position: sc.position,
                size: sc.size,
                waveform_indices: sc.waveform_indices.iter().map(|&i| i as usize).collect(),
            })
            .collect();
        let restored_instances: Vec<component::ComponentInstance> = stored_component_instances
            .into_iter()
            .map(|si| component::ComponentInstance {
                component_id: si.component_id,
                position: si.position,
            })
            .collect();
        let next_component_id = restored_components.iter().map(|c| c.id).max().unwrap_or(0) + 1;

        let restored_midi_clips: Vec<midi::MidiClip> = stored_midi_clips
            .into_iter()
            .map(|smc| midi::MidiClip {
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
            })
            .collect();

        let restored_instrument_regions: Vec<instruments::InstrumentRegion> = stored_instrument_regions
            .into_iter()
            .map(|sir| {
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
                ir
            })
            .collect();

        Self {
            gpu: None,
            camera,
            objects,
            waveforms,
            audio_clips,
            audio_engine,
            recorder,
            recording_waveform_idx: None,
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
            storage,
            has_saved_state,
            project_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_project_name: project_name,
            effect_regions: restored_effect_regions,
            plugin_blocks: restored_plugin_blocks,
            components: restored_components,
            component_instances: restored_instances,
            next_component_id,
            plugin_registry,
            export_regions: Vec::new(),
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
            editing_bpm: None,
            dragging_bpm: None,
            last_click_time: std::time::Instant::now(),
            last_click_world: [0.0; 2],
            clipboard: Clipboard::new(),
            settings,
            settings_window: None,
            plugin_editor: None,
            menu_state: None,
            toast_manager: ui::toast::ToastManager::new(),
            automation_mode: false,
            active_automation_param: AutomationParam::Volume,
            cached_instances: Vec::with_capacity(2048),
            cached_wf_verts: Vec::with_capacity(32768),
            render_generation: 1,
            last_rendered_generation: 0,
            last_rendered_camera_pos: [f32::NAN, f32::NAN],
            last_rendered_camera_zoom: f32::NAN,
            last_rendered_hovered: None,
            last_rendered_selected_len: 0,
        }
    }

    fn save_project_state(&mut self) {
        if let Some(storage) = &self.storage {
            let stored_regions: Vec<storage::StoredEffectRegion> = self
                .effect_regions
                .iter()
                .map(|er| storage::StoredEffectRegion {
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
                .map(|pb| storage::StoredPluginBlock {
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
                .map(|c| storage::StoredComponent {
                    id: c.id,
                    name: c.name.clone(),
                    position: c.position,
                    size: c.size,
                    waveform_indices: c.waveform_indices.iter().map(|&i| i as u64).collect(),
                })
                .collect();
            let stored_instances: Vec<storage::StoredComponentInstance> = self
                .component_instances
                .iter()
                .map(|inst| storage::StoredComponentInstance {
                    component_id: inst.component_id,
                    position: inst.position,
                })
                .collect();

            let stored_waveforms: Vec<storage::StoredWaveform> = self
                .waveforms
                .iter()
                .map(|wf| storage::StoredWaveform {
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
                    disabled: wf.disabled,
                    sample_offset_px: wf.sample_offset_px,
                    automation_volume: wf.automation.volume_lane().points.iter().map(|p| [p.t, p.value]).collect(),
                    automation_pan: wf.automation.pan_lane().points.iter().map(|p| [p.t, p.value]).collect(),
                })
                .collect();

            let state = ProjectState {
                name: self.current_project_name.clone(),
                camera_position: self.camera.position,
                camera_zoom: self.camera.zoom,
                objects: self.objects.clone(),
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
                    .map(|lr| storage::StoredLoopRegion {
                        position: lr.position,
                        size: lr.size,
                        enabled: lr.enabled,
                    })
                    .collect(),
                components: stored_components,
                component_instances: stored_instances,
                bpm: self.bpm,
                midi_clips: self.midi_clips.iter().map(|mc| {
                    let (grid_tag, grid_val) = storage::grid_mode_to_stored(mc.grid_mode);
                    storage::StoredMidiClip {
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
                    }
                }).collect(),
                instrument_regions: self.instrument_regions.iter().map(|ir| storage::StoredInstrumentRegion {
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
            };
            storage.save_project_state(state);

            // Update project name in index
            if let Some(path) = storage.current_project_path() {
                let path_str = path.to_string_lossy().to_string();
                storage.update_index_name(&path_str, &self.current_project_name);
            }

            // Save audio data and peaks for each waveform
            storage.clear_audio_and_peaks();
            for (i, wf) in self.waveforms.iter().enumerate() {
                let mono = if i < self.audio_clips.len() {
                    &self.audio_clips[i].samples
                } else {
                    continue;
                };
                let duration = if i < self.audio_clips.len() {
                    self.audio_clips[i].duration_secs
                } else {
                    0.0
                };
                storage.save_audio(
                    i as u64,
                    &wf.audio.left_samples,
                    &wf.audio.right_samples,
                    mono,
                    wf.audio.sample_rate,
                    duration,
                );
                storage.save_peaks(
                    i as u64,
                    wf.audio.left_peaks.block_size as u64,
                    &wf.audio.left_peaks.peaks,
                    &wf.audio.right_peaks.peaks,
                );
            }

            self.project_dirty = false;
            println!("Project '{}' saved", self.current_project_name);
        }
    }

    fn save_project(&mut self) {
        self.save_project_state();
        if let Some(storage) = &self.storage {
            if storage.is_temp_project() {
                self.save_project_as();
            }
        }
    }

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
            self.undo();
        } else if id == menu.redo {
            self.redo();
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
        self.objects = default_objects();
        self.waveforms.clear();
        self.audio_clips.clear();
        self.effect_regions.clear();
        self.plugin_blocks.clear();
        self.components.clear();
        self.component_instances.clear();
        self.next_component_id = 1;
        self.selected.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.camera = Camera::new();
        self.export_regions.clear();
        self.loop_regions.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm = None;
        self.dragging_bpm = None;
        self.command_palette = None;
        self.context_menu = None;

        if let Some(gpu) = &self.gpu {
            self.camera.zoom = gpu.window.scale_factor() as f32;
        }

        self.sync_audio_clips();
        self.save_project_state();
        println!("New project created");
    }

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
        self.objects = state.objects;
        let num_waveforms = state.waveforms.len();
        self.waveforms = state
            .waveforms
            .into_iter()
            .map(|sw| WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate: sw.sample_rate,
                    filename: sw.filename,
                }),
                position: sw.position,
                size: sw.size,
                color: sw.color,
                border_radius: sw.border_radius,
                fade_in_px: sw.fade_in_px,
                fade_out_px: sw.fade_out_px,
                fade_in_curve: sw.fade_in_curve,
                fade_out_curve: sw.fade_out_curve,
                volume: if sw.volume > 0.0 { sw.volume } else { 1.0 },
                disabled: sw.disabled,
                sample_offset_px: sw.sample_offset_px,
                automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
            })
            .collect();

        // Restore audio data and peaks from DB
        self.audio_clips.clear();
        if let Some(s) = &self.storage {
            for i in 0..num_waveforms {
                let mut left_samples = Arc::new(Vec::new());
                let mut right_samples = Arc::new(Vec::new());
                let mut sample_rate = self.waveforms[i].audio.sample_rate;
                let mut left_peaks = self.waveforms[i].audio.left_peaks.clone();
                let mut right_peaks = self.waveforms[i].audio.right_peaks.clone();

                if let Some(audio) = s.load_audio(i as u64) {
                    left_samples = Arc::new(storage::u8_slice_to_f32(&audio.left_samples));
                    right_samples = Arc::new(storage::u8_slice_to_f32(&audio.right_samples));
                    let mono = storage::u8_slice_to_f32(&audio.mono_samples);
                    sample_rate = audio.sample_rate;
                    self.audio_clips.push(AudioClipData {
                        samples: Arc::new(mono),
                        sample_rate: audio.sample_rate,
                        duration_secs: audio.duration_secs,
                    });
                } else {
                    self.audio_clips.push(AudioClipData {
                        samples: Arc::new(Vec::new()),
                        sample_rate: 48000,
                        duration_secs: 0.0,
                    });
                }
                if let Some(peaks) = s.load_peaks(i as u64) {
                    let lp = storage::u8_slice_to_f32(&peaks.left_peaks);
                    let rp = storage::u8_slice_to_f32(&peaks.right_peaks);
                    left_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, lp));
                    right_peaks = Arc::new(WaveformPeaks::from_raw(peaks.block_size as usize, rp));
                }
                self.waveforms[i].audio = Arc::new(AudioData {
                    left_samples,
                    right_samples,
                    left_peaks,
                    right_peaks,
                    sample_rate,
                    filename: self.waveforms[i].audio.filename.clone(),
                });
            }
        }

        let restored_regions: Vec<effects::EffectRegion> = state
            .effect_regions
            .into_iter()
            .map(|ser| {
                let mut region = effects::EffectRegion::new(ser.position, ser.size);
                region.name = ser.name;
                region
            })
            .collect();
        self.effect_regions = restored_regions;

        self.plugin_blocks = state
            .plugin_blocks
            .into_iter()
            .map(|spb| {
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
                pb
            })
            .collect();

        self.components = state
            .components
            .into_iter()
            .map(|sc| component::ComponentDef {
                id: sc.id,
                name: sc.name,
                position: sc.position,
                size: sc.size,
                waveform_indices: sc.waveform_indices.iter().map(|&i| i as usize).collect(),
            })
            .collect();
        self.component_instances = state
            .component_instances
            .into_iter()
            .map(|si| component::ComponentInstance {
                component_id: si.component_id,
                position: si.position,
            })
            .collect();
        self.next_component_id = self.components.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.bpm = if state.bpm > 0.0 { state.bpm } else { DEFAULT_BPM };

        self.sample_browser = if !state.browser_expanded.is_empty() {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let expanded: HashSet<PathBuf> =
                state.browser_expanded.iter().map(PathBuf::from).collect();
            let mut b =
                ui::browser::SampleBrowser::from_state(folders, expanded, state.browser_visible);
            b.width = if state.browser_width > 0.0 {
                state.browser_width
            } else {
                260.0
            };
            b
        } else {
            let folders: Vec<PathBuf> = state.browser_folders.iter().map(PathBuf::from).collect();
            let mut b = ui::browser::SampleBrowser::from_folders(folders);
            b.width = 260.0;
            b
        };

        self.selected.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.export_regions.clear();

        self.loop_regions = state
            .loop_regions
            .into_iter()
            .map(|slr| LoopRegion {
                position: slr.position,
                size: slr.size,
                enabled: slr.enabled,
            })
            .collect();

        self.midi_clips = state
            .midi_clips
            .into_iter()
            .map(|smc| midi::MidiClip {
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
            })
            .collect();

        self.instrument_regions = state
            .instrument_regions
            .into_iter()
            .map(|sir| {
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
                ir
            })
            .collect();

        self.editing_midi_clip = None;
        self.selected_midi_notes.clear();
        self.editing_component = None;
        self.editing_effect_name = None;
        self.editing_waveform_name = None;
        self.editing_bpm = None;
        self.dragging_bpm = None;
        self.command_palette = None;
        self.context_menu = None;

        // If plugins are already scanned, open vst3-gui instances for restored plugin blocks
        if self.plugin_registry.is_scanned() {
            for pb in &mut self.plugin_blocks {
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

    fn update_component_bounds(&mut self, comp_idx: usize) {
        if comp_idx >= self.components.len() {
            return;
        }
        let indices = self.components[comp_idx].waveform_indices.clone();
        if indices.is_empty() {
            return;
        }
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &indices);
        self.components[comp_idx].position = pos;
        self.components[comp_idx].size = size;
    }

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
            let mut sample_offsets: Vec<f32> = Vec::new();
            let mut vol_autos: Vec<Vec<(f32, f32)>> = Vec::new();
            let mut pan_autos: Vec<Vec<(f32, f32)>> = Vec::new();

            for (i, wf) in self.waveforms.iter().enumerate() {
                if wf.disabled || i >= self.audio_clips.len() {
                    continue;
                }
                positions.push(wf.position);
                sizes.push(wf.size);
                clips.push(&self.audio_clips[i]);
                fade_ins.push(wf.fade_in_px);
                fade_outs.push(wf.fade_out_px);
                fade_in_curves.push(wf.fade_in_curve);
                fade_out_curves.push(wf.fade_out_curve);
                volumes.push(wf.volume);
                sample_offsets.push(wf.sample_offset_px);
                vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
            }

            // Add virtual clips for each component instance
            let comp_map: std::collections::HashMap<component::ComponentId, usize> = self
                .components
                .iter()
                .enumerate()
                .map(|(i, c)| (c.id, i))
                .collect();
            for inst in &self.component_instances {
                if let Some(def) = comp_map
                    .get(&inst.component_id)
                    .map(|&i| &self.components[i])
                {
                    let offset = [
                        inst.position[0] - def.position[0],
                        inst.position[1] - def.position[1],
                    ];
                    for &wf_idx in &def.waveform_indices {
                        if wf_idx < self.waveforms.len() && wf_idx < self.audio_clips.len() && !self.waveforms[wf_idx].disabled {
                            let wf = &self.waveforms[wf_idx];
                            positions
                                .push([wf.position[0] + offset[0], wf.position[1] + offset[1]]);
                            sizes.push(wf.size);
                            clips.push(&self.audio_clips[wf_idx]);
                            fade_ins.push(wf.fade_in_px);
                            fade_outs.push(wf.fade_out_px);
                            fade_in_curves.push(wf.fade_in_curve);
                            fade_out_curves.push(wf.fade_out_curve);
                            volumes.push(wf.volume);
                            sample_offsets.push(wf.sample_offset_px);
                            vol_autos.push(wf.automation.volume_lane().points.iter().map(|p| (p.t, p.value)).collect());
                            pan_autos.push(wf.automation.pan_lane().points.iter().map(|p| (p.t, p.value)).collect());
                        }
                    }
                }
            }

            let owned_clips: Vec<AudioClipData> = clips.iter().map(|c| (*c).clone()).collect();
            engine.update_clips(&positions, &sizes, &owned_clips, &fade_ins, &fade_outs, &fade_in_curves, &fade_out_curves, &volumes, &sample_offsets, &vol_autos, &pan_autos);

            let regions: Vec<audio::AudioEffectRegion> = self
                .effect_regions
                .iter()
                .map(|er| {
                    let block_indices = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                    audio::AudioEffectRegion {
                        x_start_px: er.position[0],
                        x_end_px: er.position[0] + er.size[0],
                        y_start: er.position[1],
                        y_end: er.position[1] + er.size[1],
                        plugins: block_indices
                            .iter()
                            .map(|&i| self.plugin_blocks[i].gui.clone())
                            .collect(),
                    }
                })
                .collect();
            engine.update_effect_regions(regions);
        }
        self.sync_instrument_regions();
    }

    fn add_loop_area(&mut self) {
        self.push_undo();
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
        self.loop_regions.push(LoopRegion {
            position: pos,
            size,
            enabled: true,
        });
        let idx = self.loop_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::LoopRegion(idx));
        self.sync_loop_region();
        self.mark_dirty();
        self.request_redraw();
    }

    fn add_effect_area(&mut self) {
        self.push_undo();
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
        self.effect_regions
            .push(effects::EffectRegion::new(pos, size));
        let idx = self.effect_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::EffectRegion(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    fn add_render_area(&mut self) {
        self.push_undo();
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
        self.export_regions.push(ExportRegion {
            position: pos,
            size,
        });
        let idx = self.export_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::ExportRegion(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    #[cfg(test)]
    fn add_instrument_area(&mut self) {
        self.push_undo();
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
        self.instrument_regions
            .push(instruments::InstrumentRegion::new(pos, size));
        let idx = self.instrument_regions.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::InstrumentRegion(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    fn add_midi_clip(&mut self) {
        self.push_undo();
        let (sw, sh, _) = self.screen_info();
        let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
        let ppb = grid::pixels_per_beat(self.bpm);
        let beats_per_bar = 4.0;
        let width = ppb * beats_per_bar * midi::MIDI_CLIP_DEFAULT_BARS as f32;
        let height = midi::MIDI_CLIP_DEFAULT_HEIGHT;
        let pos = [center[0] - width * 0.5, center[1] - height * 0.5];
        let mut clip = midi::MidiClip::new(pos, &self.settings);
        clip.size = [width, height];
        self.midi_clips.push(clip);
        let idx = self.midi_clips.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::MidiClip(idx));
        self.mark_dirty();
        self.request_redraw();
    }

    fn sync_instrument_regions(&self) {
        if let Some(engine) = &self.audio_engine {
            let mut instrument_regions = Vec::new();
            for ir in &self.instrument_regions {
                if !ir.has_plugin() {
                    continue;
                }
                let mut midi_events = Vec::new();
                // Find MIDI clips that spatially overlap this region
                for mc in &self.midi_clips {
                    if !rects_overlap(ir.position, ir.size, mc.position, mc.size) {
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
    }

    fn sync_loop_region(&self) {
        if let Some(engine) = &self.audio_engine {
            let regions: Vec<(f64, f64)> = self
                .loop_regions
                .iter()
                .filter(|lr| lr.enabled)
                .map(|lr| {
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

    fn toggle_recording(&mut self) {
        if self.recorder.is_none() {
            return;
        }

        let is_rec = self.recorder.as_ref().unwrap().is_recording();

        if is_rec {
            let loaded = self.recorder.as_mut().unwrap().stop();
            if let Some(loaded) = loaded {
                if let Some(idx) = self.recording_waveform_idx.take() {
                    if idx < self.waveforms.len() {
                        self.waveforms[idx].size[0] = loaded.width;
                        self.waveforms[idx].audio = Arc::new(AudioData {
                            left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                            right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                            left_samples: loaded.left_samples.clone(),
                            right_samples: loaded.right_samples.clone(),
                            sample_rate: loaded.sample_rate,
                            filename: self.waveforms[idx].audio.filename.clone(),
                        });
                    }
                    if idx < self.audio_clips.len() {
                        self.audio_clips[idx] = AudioClipData {
                            samples: loaded.samples,
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        };
                    }
                    self.sync_audio_clips();
                }
            } else {
                if let Some(idx) = self.recording_waveform_idx.take() {
                    if idx < self.waveforms.len() {
                        self.waveforms.remove(idx);
                    }
                    if idx < self.audio_clips.len() {
                        self.audio_clips.remove(idx);
                    }
                }
            }
        } else {
            let world = self.last_canvas_click_world;
            let height = 150.0;
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let sample_rate = self.recorder.as_ref().unwrap().sample_rate();

            self.push_undo();
            let idx = self.waveforms.len();
            self.waveforms.push(WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: Arc::new(Vec::new()),
                    right_samples: Arc::new(Vec::new()),
                    left_peaks: Arc::new(WaveformPeaks::empty()),
                    right_peaks: Arc::new(WaveformPeaks::empty()),
                    sample_rate,
                    filename: "Recording".to_string(),
                }),
                position: [world[0], world[1] - height * 0.5],
                size: [0.0, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                fade_in_px: 0.0,
                fade_out_px: 0.0,
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            });
            self.audio_clips.push(AudioClipData {
                samples: Arc::new(Vec::new()),
                sample_rate,
                duration_secs: 0.0,
            });
            self.recording_waveform_idx = Some(idx);
            self.recorder.as_mut().unwrap().start();
        }
    }

    fn update_recording_waveform(&mut self) {
        let idx = match self.recording_waveform_idx {
            Some(i) => i,
            None => return,
        };
        let snapshot = self.recorder.as_ref().and_then(|r| r.current_snapshot());
        if let Some(loaded) = snapshot {
            if idx < self.waveforms.len() {
                self.waveforms[idx].size[0] = loaded.width;
                self.waveforms[idx].audio = Arc::new(AudioData {
                    left_peaks: Arc::new(WaveformPeaks::build(&loaded.left_samples)),
                    right_peaks: Arc::new(WaveformPeaks::build(&loaded.right_samples)),
                    left_samples: loaded.left_samples,
                    right_samples: loaded.right_samples,
                    sample_rate: loaded.sample_rate,
                    filename: self.waveforms[idx].audio.filename.clone(),
                });
                self.mark_dirty();
            }
        }
    }

    fn is_recording(&self) -> bool {
        self.recorder
            .as_ref()
            .map(|r| r.is_recording())
            .unwrap_or(false)
    }

    fn trigger_export_render(&mut self) {
        let er = match self.export_regions.first() {
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
            .zip(self.audio_clips.iter())
            .filter(|(wf, _)| !wf.disabled)
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
            })
            .collect();

        let effect_regions: Vec<audio::AudioEffectRegion> = self
            .effect_regions
            .iter()
            .map(|er| {
                let block_indices = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                audio::AudioEffectRegion {
                    x_start_px: er.position[0],
                    x_end_px: er.position[0] + er.size[0],
                    y_start: er.position[1],
                    y_end: er.position[1] + er.size[1],
                    plugins: block_indices
                        .iter()
                        .map(|&i| self.plugin_blocks[i].gui.clone())
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
        self.waveform_edge_hover = hit_test_waveform_edge(&self.waveforms, world, &self.camera);
        self.midi_note_edge_hover = if let Some(mc_idx) = self.editing_midi_clip {
            if mc_idx < self.midi_clips.len() {
                matches!(
                    midi::hit_test_midi_note_editing(&self.midi_clips[mc_idx], world, &self.camera, true),
                    Some((_, midi::MidiNoteHitZone::RightEdge | midi::MidiNoteHitZone::LeftEdge))
                )
            } else {
                false
            }
        } else {
            false
        };
        self.velocity_divider_hovered = if let Some(mc_idx) = self.editing_midi_clip {
            if mc_idx < self.midi_clips.len() {
                midi::hit_test_velocity_divider(&self.midi_clips[mc_idx], world, &self.camera)
            } else {
                false
            }
        } else {
            false
        };
        self.velocity_bar_hovered = if let Some(mc_idx) = self.editing_midi_clip {
            if mc_idx < self.midi_clips.len() && !self.velocity_divider_hovered {
                midi::hit_test_velocity_bar(&self.midi_clips[mc_idx], world, &self.camera).is_some()
            } else {
                false
            }
        } else {
            false
        };
        self.cmd_velocity_hover_note = if self.modifiers.super_key() {
            if let Some(mc_idx) = self.editing_midi_clip {
                if mc_idx < self.midi_clips.len() {
                    match midi::hit_test_midi_note_editing(&self.midi_clips[mc_idx], world, &self.camera, true) {
                        Some((note_idx, midi::MidiNoteHitZone::Body | midi::MidiNoteHitZone::LeftEdge | midi::MidiNoteHitZone::RightEdge)) => {
                            Some((mc_idx, note_idx))
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
        self.fade_handle_hovered = if self.waveform_edge_hover == WaveformEdgeHover::None {
            hit_test_fade_handle(&self.waveforms, world, &self.camera)
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
        for (ci, def) in self.components.iter().enumerate() {
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
        for (i, ir) in self.instrument_regions.iter().enumerate() {
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
        for (i, er) in self.effect_regions.iter().enumerate() {
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
        for (i, er) in self.export_regions.iter().enumerate() {
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
        for (i, lr) in self.loop_regions.iter().enumerate() {
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
            HitTarget::Object(i) => self.objects[*i].position = pos,
            HitTarget::Waveform(i) => self.waveforms[*i].position = pos,
            HitTarget::EffectRegion(i) => self.effect_regions[*i].position = pos,
            HitTarget::PluginBlock(i) => self.plugin_blocks[*i].position = pos,
            HitTarget::LoopRegion(i) => self.loop_regions[*i].position = pos,
            HitTarget::ExportRegion(i) => self.export_regions[*i].position = pos,
            HitTarget::ComponentDef(i) => {
                let old_pos = self.components[*i].position;
                let dx = pos[0] - old_pos[0];
                let dy = pos[1] - old_pos[1];
                self.components[*i].position = pos;
                for &wf_idx in &self.components[*i].waveform_indices.clone() {
                    if wf_idx < self.waveforms.len() {
                        self.waveforms[wf_idx].position[0] += dx;
                        self.waveforms[wf_idx].position[1] += dy;
                    }
                }
            }
            HitTarget::ComponentInstance(i) => self.component_instances[*i].position = pos,
            HitTarget::MidiClip(i) => self.midi_clips[*i].position = pos,
            HitTarget::InstrumentRegion(i) => self.instrument_regions[*i].position = pos,
        }
    }

    fn get_target_pos(&self, target: &HitTarget) -> [f32; 2] {
        match target {
            HitTarget::Object(i) => self.objects[*i].position,
            HitTarget::Waveform(i) => self.waveforms[*i].position,
            HitTarget::EffectRegion(i) => self.effect_regions[*i].position,
            HitTarget::PluginBlock(i) => self.plugin_blocks[*i].position,
            HitTarget::LoopRegion(i) => self.loop_regions[*i].position,
            HitTarget::ExportRegion(i) => self.export_regions[*i].position,
            HitTarget::ComponentDef(i) => self.components[*i].position,
            HitTarget::ComponentInstance(i) => self.component_instances[*i].position,
            HitTarget::MidiClip(i) => self.midi_clips[*i].position,
            HitTarget::InstrumentRegion(i) => self.instrument_regions[*i].position,
        }
    }

    fn is_snap_override_active(&self) -> bool {
        self.modifiers.super_key()
    }

    pub(crate) fn begin_move_selection(&mut self, world: [f32; 2], alt_copy: bool) {
        self.push_undo();

        if alt_copy {
            let mut new_selected: Vec<HitTarget> = Vec::new();
            for target in self.selected.clone() {
                match target {
                    HitTarget::Waveform(i) => {
                        if i < self.waveforms.len() {
                            let wf = self.waveforms[i].clone();
                            self.waveforms.push(wf);
                            let new_i = self.waveforms.len() - 1;
                            if i < self.audio_clips.len() {
                                let clip = self.audio_clips[i].clone();
                                self.audio_clips.push(clip);
                            }
                            new_selected.push(HitTarget::Waveform(new_i));
                        }
                    }
                    HitTarget::Object(i) => {
                        if i < self.objects.len() {
                            let obj = self.objects[i].clone();
                            self.objects.push(obj);
                            new_selected.push(HitTarget::Object(self.objects.len() - 1));
                        }
                    }
                    HitTarget::EffectRegion(i) => {
                        if i < self.effect_regions.len() {
                            let er = self.effect_regions[i].clone();
                            self.effect_regions.push(er);
                            new_selected
                                .push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                        }
                    }
                    HitTarget::PluginBlock(i) => {
                        if i < self.plugin_blocks.len() {
                            let pb = self.plugin_blocks[i].clone();
                            self.plugin_blocks.push(pb);
                            new_selected
                                .push(HitTarget::PluginBlock(self.plugin_blocks.len() - 1));
                        }
                    }
                    HitTarget::LoopRegion(i) => {
                        if i < self.loop_regions.len() {
                            let lr = self.loop_regions[i].clone();
                            self.loop_regions.push(lr);
                            new_selected
                                .push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                        }
                    }
                    HitTarget::ExportRegion(i) => {
                        if i < self.export_regions.len() {
                            let xr = self.export_regions[i].clone();
                            self.export_regions.push(xr);
                            new_selected
                                .push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                        }
                    }
                    HitTarget::ComponentInstance(i) => {
                        if i < self.component_instances.len() {
                            let inst = self.component_instances[i].clone();
                            self.component_instances.push(inst);
                            new_selected.push(HitTarget::ComponentInstance(
                                self.component_instances.len() - 1,
                            ));
                        }
                    }
                    HitTarget::MidiClip(i) => {
                        if i < self.midi_clips.len() {
                            let mc = self.midi_clips[i].clone();
                            self.midi_clips.push(mc);
                            new_selected.push(HitTarget::MidiClip(self.midi_clips.len() - 1));
                        }
                    }
                    HitTarget::InstrumentRegion(i) => {
                        if i < self.instrument_regions.len() {
                            let ir = self.instrument_regions[i].clone();
                            self.instrument_regions.push(ir);
                            new_selected.push(HitTarget::InstrumentRegion(self.instrument_regions.len() - 1));
                        }
                    }
                    HitTarget::ComponentDef(i) => {
                        if i < self.components.len() {
                            let src = &self.components[i];
                            let new_id = self.next_component_id;
                            self.next_component_id += 1;
                            let src_indices = src.waveform_indices.clone();
                            let new_comp = component::ComponentDef {
                                id: new_id,
                                name: format!("{} copy", src.name),
                                position: src.position,
                                size: src.size,
                                waveform_indices: Vec::new(),
                            };
                            let mut new_wf_indices = Vec::new();
                            for &wi in &src_indices {
                                if wi < self.waveforms.len() {
                                    let wf = self.waveforms[wi].clone();
                                    self.waveforms.push(wf);
                                    let new_wi = self.waveforms.len() - 1;
                                    new_wf_indices.push(new_wi);
                                    if wi < self.audio_clips.len() {
                                        let clip = self.audio_clips[wi].clone();
                                        self.audio_clips.push(clip);
                                    }
                                }
                            }
                            self.components.push(component::ComponentDef {
                                waveform_indices: new_wf_indices,
                                ..new_comp
                            });
                            new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                        }
                    }
                }
            }
            self.selected = new_selected;
        }

        let offsets: Vec<(HitTarget, [f32; 2])> = self
            .selected
            .iter()
            .map(|t| {
                let pos = self.get_target_pos(t);
                (*t, [world[0] - pos[0], world[1] - pos[1]])
            })
            .collect();
        self.drag = DragState::MovingSelection { offsets };
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
                for i in 0..self.objects.len() {
                    self.selected.push(HitTarget::Object(i));
                }
                for i in 0..self.waveforms.len() {
                    let in_component = self
                        .components
                        .iter()
                        .any(|c| c.waveform_indices.contains(&i));
                    if !in_component {
                        self.selected.push(HitTarget::Waveform(i));
                    }
                }
                for i in 0..self.effect_regions.len() {
                    self.selected.push(HitTarget::EffectRegion(i));
                }
                for i in 0..self.loop_regions.len() {
                    self.selected.push(HitTarget::LoopRegion(i));
                }
                for i in 0..self.components.len() {
                    self.selected.push(HitTarget::ComponentDef(i));
                }
                for i in 0..self.component_instances.len() {
                    self.selected.push(HitTarget::ComponentInstance(i));
                }
            }
            CommandAction::Undo => self.undo(),
            CommandAction::Redo => self.redo(),
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
                if self.sample_browser.visible {
                    self.ensure_plugins_scanned();
                }
            }
            CommandAction::AddFolderToBrowser => {
                self.open_add_folder_dialog();
            }
            CommandAction::SetMasterVolume => {
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
            CommandAction::SetSampleVolume => {
                let selected_wf = self.selected.iter().find_map(|t| {
                    if let HitTarget::Waveform(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() {
                        if let Some(p) = &mut self.command_palette {
                            p.mode = PaletteMode::SampleVolumeFader;
                            p.fader_value = self.waveforms[idx].volume;
                            p.fader_target_waveform = Some(idx);
                            p.search_text.clear();
                        }
                        self.request_redraw();
                        return;
                    }
                }
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
                self.settings_window = if self.settings_window.is_some() {
                    None
                } else {
                    Some(SettingsWindow::new())
                };
            }
            CommandAction::RenameEffectRegion => {
                let selected_er = self.selected.iter().find_map(|t| {
                    if let HitTarget::EffectRegion(i) = t {
                        Some(*i)
                    } else {
                        None
                    }
                });
                if let Some(idx) = selected_er {
                    if idx < self.effect_regions.len() {
                        let current = self.effect_regions[idx].name.clone();
                        self.editing_effect_name = Some((idx, current));
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
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() {
                        let current = self.waveforms[idx].audio.filename.clone();
                        self.editing_waveform_name = Some((idx, current));
                    }
                }
            }
            CommandAction::ToggleSnapToGrid => {
                self.settings.snap_to_grid = !self.settings.snap_to_grid;
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
                if let Some(idx) = self.editing_midi_clip {
                    if idx < self.midi_clips.len() {
                        self.midi_clips[idx].grid_mode = GridMode::Fixed(fg);
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::SetMidiClipGridAdaptive(size) => {
                if let Some(idx) = self.editing_midi_clip {
                    if idx < self.midi_clips.len() {
                        self.midi_clips[idx].grid_mode = GridMode::Adaptive(size);
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::ToggleMidiClipTripletGrid => {
                if let Some(idx) = self.editing_midi_clip {
                    if idx < self.midi_clips.len() {
                        self.midi_clips[idx].triplet_grid = !self.midi_clips[idx].triplet_grid;
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::NarrowMidiClipGrid => {
                if let Some(idx) = self.editing_midi_clip {
                    if idx < self.midi_clips.len() {
                        match self.midi_clips[idx].grid_mode {
                            GridMode::Adaptive(s) => {
                                self.midi_clips[idx].grid_mode = GridMode::Adaptive(s.narrower());
                            }
                            GridMode::Fixed(f) => {
                                self.midi_clips[idx].grid_mode = GridMode::Fixed(f.finer());
                            }
                        }
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::WidenMidiClipGrid => {
                if let Some(idx) = self.editing_midi_clip {
                    if idx < self.midi_clips.len() {
                        match self.midi_clips[idx].grid_mode {
                            GridMode::Adaptive(s) => {
                                self.midi_clips[idx].grid_mode = GridMode::Adaptive(s.wider());
                            }
                            GridMode::Fixed(f) => {
                                self.midi_clips[idx].grid_mode = GridMode::Fixed(f.coarser());
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
                if let Some(idx) = selected_wf {
                    if idx < self.waveforms.len() && idx < self.audio_clips.len() {
                        self.push_undo();

                        let mut mono = (*self.audio_clips[idx].samples).clone();
                        mono.reverse();
                        self.audio_clips[idx].samples = Arc::new(mono);

                        let old = &self.waveforms[idx].audio;
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
                        self.waveforms[idx].audio = new_audio;

                        self.sync_audio_clips();
                        self.mark_dirty();
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
            CommandAction::AddPlugin => {
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
            CommandAction::AddRenderArea => {
                self.add_render_area();
            }
            CommandAction::SetSampleColor(idx) => {
                if let Some(&color) = WAVEFORM_COLORS.get(idx) {
                    for target in self.selected.clone() {
                        if let HitTarget::Waveform(i) = target {
                            self.waveforms[i].color = color;
                        }
                    }
                }
            }
        }
        self.request_redraw();
    }

    fn split_sample_at_cursor(&mut self) {
        let world = self.camera.screen_to_world(self.mouse_pos);
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
        let wf_idx = match hit {
            Some(HitTarget::Waveform(i)) => i,
            _ => return,
        };
        if wf_idx >= self.waveforms.len() || wf_idx >= self.audio_clips.len() {
            return;
        }

        let pos = self.waveforms[wf_idx].position;
        let size = self.waveforms[wf_idx].size;
        let offset_px = self.waveforms[wf_idx].sample_offset_px;
        let split_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
        let t = ((split_x - pos[0]) / size[0]).clamp(0.01, 0.99);

        let audio = Arc::clone(&self.waveforms[wf_idx].audio);
        let mono_samples = Arc::clone(&self.audio_clips[wf_idx].samples);
        let total_mono = mono_samples.len();
        if total_mono == 0 {
            return;
        }

        let full_w = full_audio_width_px(&self.waveforms[wf_idx]);
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

        let orig_color = self.waveforms[wf_idx].color;
        let orig_border_radius = self.waveforms[wf_idx].border_radius;
        let orig_fade_in = self.waveforms[wf_idx].fade_in_px;
        let orig_fade_out = self.waveforms[wf_idx].fade_out_px;
        let orig_fade_in_curve = self.waveforms[wf_idx].fade_in_curve;
        let orig_fade_out_curve = self.waveforms[wf_idx].fade_out_curve;
        let orig_volume = self.waveforms[wf_idx].volume;

        self.push_undo();

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
            position: pos,
            size: [left_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: orig_fade_in,
            fade_out_px: 0.0,
            fade_in_curve: orig_fade_in_curve,
            fade_out_curve: 0.0,
            volume: orig_volume,
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
            filename,
        });
        let right_waveform = WaveformView {
            audio: right_audio,
            position: [pos[0] + left_width, pos[1]],
            size: [right_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: 0.0,
            fade_out_px: orig_fade_out,
            fade_in_curve: 0.0,
            fade_out_curve: orig_fade_out_curve,
            volume: orig_volume,
            disabled: false,
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
        };

        self.waveforms[wf_idx] = left_waveform;
        self.audio_clips[wf_idx] = left_clip;
        self.waveforms.insert(wf_idx + 1, right_waveform);
        self.audio_clips.insert(wf_idx + 1, right_clip);

        // Fix up indices in component waveform_indices
        for comp in &mut self.components {
            let mut new_indices = Vec::new();
            for &wi in &comp.waveform_indices {
                if wi == wf_idx {
                    new_indices.push(wi);
                    new_indices.push(wi + 1);
                } else if wi > wf_idx {
                    new_indices.push(wi + 1);
                } else {
                    new_indices.push(wi);
                }
            }
            comp.waveform_indices = new_indices;
        }

        // Fix up selected indices
        let mut new_selected: Vec<HitTarget> = Vec::new();
        for t in &self.selected {
            match t {
                HitTarget::Waveform(i) if *i > wf_idx => {
                    new_selected.push(HitTarget::Waveform(i + 1));
                }
                other => new_selected.push(*other),
            }
        }
        new_selected.push(HitTarget::Waveform(wf_idx + 1));
        self.selected = new_selected;

        self.sync_audio_clips();
        self.mark_dirty();
    }

    fn create_component_from_selection(&mut self) {
        let wf_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();
        if wf_indices.is_empty() {
            println!("No waveforms selected to create component");
            return;
        }
        self.push_undo();
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &wf_indices);
        let id = self.next_component_id;
        self.next_component_id += 1;
        let name = format!("Component {}", id);
        let def = component::ComponentDef {
            id,
            name: name.clone(),
            position: pos,
            size,
            waveform_indices: wf_indices,
        };
        self.components.push(def);
        let idx = self.components.len() - 1;
        self.selected.clear();
        self.selected.push(HitTarget::ComponentDef(idx));
        println!(
            "Created component '{}' with {} waveforms",
            name,
            self.components[idx].waveform_indices.len()
        );
    }

    fn create_instance_of_selected_component(&mut self) {
        let comp_idx = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentDef(i) => Some(*i),
            _ => None,
        });
        if let Some(ci) = comp_idx {
            if ci >= self.components.len() {
                return;
            }
            self.push_undo();
            let def = &self.components[ci];
            let offset_x = def.size[0] + 50.0;
            let inst = component::ComponentInstance {
                component_id: def.id,
                position: [def.position[0] + offset_x, def.position[1]],
            };
            self.component_instances.push(inst);
            let idx = self.component_instances.len() - 1;
            self.selected.clear();
            self.selected.push(HitTarget::ComponentInstance(idx));
            println!("Created instance of component {}", self.components[ci].name);
            self.sync_audio_clips();
        }
    }

    fn go_to_component_of_selected_instance(&mut self) {
        let inst_idx = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentInstance(i) => Some(*i),
            _ => None,
        });
        if let Some(ii) = inst_idx {
            if ii >= self.component_instances.len() {
                return;
            }
            let comp_id = self.component_instances[ii].component_id;
            if let Some((ci, def)) = self
                .components
                .iter()
                .enumerate()
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
        self.push_undo();
        let mut new_selected: Vec<HitTarget> = Vec::new();

        let selected_wf_indices: Vec<usize> = self
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

        let wf_group_shift = if selected_wf_indices.len() >= 2 {
            let min_start = selected_wf_indices
                .iter()
                .filter_map(|&i| self.waveforms.get(i))
                .map(|wf| wf.position[0])
                .fold(f32::INFINITY, f32::min);
            let max_end = selected_wf_indices
                .iter()
                .filter_map(|&i| self.waveforms.get(i))
                .map(|wf| wf.position[0] + wf.size[0])
                .fold(f32::NEG_INFINITY, f32::max);
            Some(max_end - min_start)
        } else {
            None
        };

        for target in self.selected.clone() {
            match target {
                HitTarget::ComponentInstance(i) => {
                    if i < self.component_instances.len() {
                        let src = &self.component_instances[i];
                        let def = self.components.iter().find(|c| c.id == src.component_id);
                        let shift = def.map(|d| d.size[0]).unwrap_or(100.0);
                        let inst = component::ComponentInstance {
                            component_id: src.component_id,
                            position: [src.position[0] + shift, src.position[1]],
                        };
                        self.component_instances.push(inst);
                        new_selected.push(HitTarget::ComponentInstance(
                            self.component_instances.len() - 1,
                        ));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if i < self.components.len() {
                        let src = &self.components[i];
                        let shift = src.size[0];
                        let new_id = self.next_component_id;
                        self.next_component_id += 1;
                        let new_comp = component::ComponentDef {
                            id: new_id,
                            name: format!("{} copy", src.name),
                            position: [src.position[0] + shift, src.position[1]],
                            size: src.size,
                            waveform_indices: Vec::new(),
                        };
                        let src_indices = src.waveform_indices.clone();
                        let mut new_wf_indices = Vec::new();
                        for &wi in &src_indices {
                            if wi < self.waveforms.len() {
                                let mut wf = self.waveforms[wi].clone();
                                wf.position[0] += shift;
                                self.waveforms.push(wf);
                                let new_wi = self.waveforms.len() - 1;
                                new_wf_indices.push(new_wi);
                                if wi < self.audio_clips.len() {
                                    let clip = self.audio_clips[wi].clone();
                                    self.audio_clips.push(clip);
                                }
                            }
                        }
                        self.components.push(component::ComponentDef {
                            waveform_indices: new_wf_indices,
                            ..new_comp
                        });
                        new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                    }
                }
                HitTarget::Waveform(i) => {
                    if i < self.waveforms.len() {
                        let mut wf = self.waveforms[i].clone();
                        let shift = wf_group_shift.unwrap_or(wf.size[0]);
                        wf.position[0] += shift;
                        self.waveforms.push(wf);
                        let new_i = self.waveforms.len() - 1;
                        if i < self.audio_clips.len() {
                            let clip = self.audio_clips[i].clone();
                            self.audio_clips.push(clip);
                        }
                        new_selected.push(HitTarget::Waveform(new_i));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if i < self.effect_regions.len() {
                        let mut er = self.effect_regions[i].clone();
                        er.position[0] += er.size[0];
                        self.effect_regions.push(er);
                        new_selected.push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if i < self.plugin_blocks.len() {
                        let mut pb = self.plugin_blocks[i].clone();
                        pb.position[0] += pb.size[0];
                        self.plugin_blocks.push(pb);
                        new_selected.push(HitTarget::PluginBlock(self.plugin_blocks.len() - 1));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if i < self.loop_regions.len() {
                        let mut lr = self.loop_regions[i].clone();
                        lr.position[0] += lr.size[0];
                        self.loop_regions.push(lr);
                        new_selected.push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if i < self.export_regions.len() {
                        let mut xr = self.export_regions[i].clone();
                        xr.position[0] += xr.size[0];
                        self.export_regions.push(xr);
                        new_selected.push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                    }
                }
                HitTarget::Object(i) => {
                    if i < self.objects.len() {
                        let mut obj = self.objects[i].clone();
                        obj.position[0] += obj.size[0];
                        self.objects.push(obj);
                        new_selected.push(HitTarget::Object(self.objects.len() - 1));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if i < self.midi_clips.len() {
                        let mut mc = self.midi_clips[i].clone();
                        mc.position[0] += mc.size[0];
                        self.midi_clips.push(mc);
                        new_selected.push(HitTarget::MidiClip(self.midi_clips.len() - 1));
                    }
                }
                HitTarget::InstrumentRegion(i) => {
                    if i < self.instrument_regions.len() {
                        let mut ir = self.instrument_regions[i].clone();
                        ir.position[0] += ir.size[0];
                        self.instrument_regions.push(ir);
                        new_selected.push(HitTarget::InstrumentRegion(self.instrument_regions.len() - 1));
                    }
                }
            }
        }

        self.selected = new_selected;
        self.sync_audio_clips();
    }

    fn copy_selected(&mut self) {
        self.clipboard.items.clear();
        // If editing a MIDI clip with selected notes, copy those instead
        if let Some(mc_idx) = self.editing_midi_clip {
            if mc_idx < self.midi_clips.len() && !self.selected_midi_notes.is_empty() {
                let notes = &self.midi_clips[mc_idx].notes;
                let min_start = self.selected_midi_notes.iter()
                    .filter(|&&ni| ni < notes.len())
                    .map(|&ni| notes[ni].start_px)
                    .fold(f32::INFINITY, f32::min);
                let mut copied: Vec<midi::MidiNote> = Vec::new();
                for &ni in &self.selected_midi_notes {
                    if ni < self.midi_clips[mc_idx].notes.len() {
                        let mut n = self.midi_clips[mc_idx].notes[ni].clone();
                        n.start_px -= min_start;
                        copied.push(n);
                    }
                }
                self.clipboard.items.push(ClipboardItem::MidiNotes(copied));
                return;
            }
        }
        for target in &self.selected {
            match target {
                HitTarget::Object(i) => {
                    if *i < self.objects.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::Object(self.objects[*i].clone()));
                    }
                }
                HitTarget::Waveform(i) => {
                    if *i < self.waveforms.len() {
                        let clip = if *i < self.audio_clips.len() {
                            Some(self.audio_clips[*i].clone())
                        } else {
                            None
                        };
                        self.clipboard
                            .items
                            .push(ClipboardItem::Waveform(self.waveforms[*i].clone(), clip));
                    }
                }
                HitTarget::EffectRegion(i) => {
                    if *i < self.effect_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::EffectRegion(self.effect_regions[*i].clone()));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if *i < self.plugin_blocks.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::PluginBlock(self.plugin_blocks[*i].clone()));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if *i < self.loop_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::LoopRegion(self.loop_regions[*i].clone()));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if *i < self.export_regions.len() {
                        self.clipboard
                            .items
                            .push(ClipboardItem::ExportRegion(self.export_regions[*i].clone()));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if *i < self.components.len() {
                        let def = &self.components[*i];
                        let wfs: Vec<(WaveformView, Option<AudioClipData>)> = def
                            .waveform_indices
                            .iter()
                            .filter_map(|&wi| {
                                if wi < self.waveforms.len() {
                                    let clip = if wi < self.audio_clips.len() {
                                        Some(self.audio_clips[wi].clone())
                                    } else {
                                        None
                                    };
                                    Some((self.waveforms[wi].clone(), clip))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        self.clipboard
                            .items
                            .push(ClipboardItem::ComponentDef(def.clone(), wfs));
                    }
                }
                HitTarget::ComponentInstance(i) => {
                    if *i < self.component_instances.len() {
                        self.clipboard.items.push(ClipboardItem::ComponentInstance(
                            self.component_instances[*i].clone(),
                        ));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if *i < self.midi_clips.len() {
                        self.clipboard.items.push(ClipboardItem::MidiClip(
                            self.midi_clips[*i].clone(),
                        ));
                    }
                }
                HitTarget::InstrumentRegion(i) => {
                    if *i < self.instrument_regions.len() {
                        let ir = &self.instrument_regions[*i];
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
        if let Some(mc_idx) = self.editing_midi_clip {
            let midi_notes = self.clipboard.items.iter().find_map(|item| {
                if let ClipboardItem::MidiNotes(notes) = item { Some(notes.clone()) } else { None }
            });
            if let Some(notes) = midi_notes {
                if mc_idx < self.midi_clips.len() {
                    self.push_undo();
                    let clip_x = self.midi_clips[mc_idx].position[0];
                    let paste_x = self.audio_engine.as_ref()
                        .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32)
                        .unwrap_or_else(|| self.camera.screen_to_world(self.mouse_pos)[0]);
                    let offset = (paste_x - clip_x).max(0.0);
                    let mut new_indices: Vec<usize> = Vec::new();
                    for n in &notes {
                        let mut pasted = n.clone();
                        pasted.start_px += offset;
                        self.midi_clips[mc_idx].notes.push(pasted);
                        new_indices.push(self.midi_clips[mc_idx].notes.len() - 1);
                    }
                    self.selected_midi_notes = self.midi_clips[mc_idx].resolve_note_overlaps(&new_indices);
                    self.sync_audio_clips();
                    self.mark_dirty();
                    return;
                }
            }
        }
        self.push_undo();
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
                    self.objects.push(o);
                    new_selected.push(HitTarget::Object(self.objects.len() - 1));
                }
                ClipboardItem::Waveform(mut w, clip) => {
                    w.position[0] += dx;
                    w.position[1] += dy;
                    self.waveforms.push(w);
                    let idx = self.waveforms.len() - 1;
                    if let Some(c) = clip {
                        while self.audio_clips.len() < idx {
                            self.audio_clips.push(AudioClipData {
                                samples: std::sync::Arc::new(Vec::new()),
                                sample_rate: 44100,
                                duration_secs: 0.0,
                            });
                        }
                        self.audio_clips.push(c);
                    }
                    new_selected.push(HitTarget::Waveform(idx));
                }
                ClipboardItem::EffectRegion(mut e) => {
                    e.position[0] += dx;
                    e.position[1] += dy;
                    self.effect_regions.push(e);
                    new_selected.push(HitTarget::EffectRegion(self.effect_regions.len() - 1));
                }
                ClipboardItem::PluginBlock(mut pb) => {
                    pb.position[0] += dx;
                    pb.position[1] += dy;
                    self.plugin_blocks.push(pb);
                    new_selected.push(HitTarget::PluginBlock(self.plugin_blocks.len() - 1));
                }
                ClipboardItem::LoopRegion(mut l) => {
                    l.position[0] += dx;
                    l.position[1] += dy;
                    self.loop_regions.push(l);
                    new_selected.push(HitTarget::LoopRegion(self.loop_regions.len() - 1));
                }
                ClipboardItem::ExportRegion(mut x) => {
                    x.position[0] += dx;
                    x.position[1] += dy;
                    self.export_regions.push(x);
                    new_selected.push(HitTarget::ExportRegion(self.export_regions.len() - 1));
                }
                ClipboardItem::ComponentDef(mut d, wfs) => {
                    let new_id = self.next_component_id;
                    self.next_component_id += 1;
                    d.id = new_id;
                    d.position[0] += dx;
                    d.position[1] += dy;
                    d.name = format!("{} copy", d.name);
                    let mut new_wf_indices = Vec::new();
                    for (mut wf, clip) in wfs {
                        wf.position[0] += dx;
                        wf.position[1] += dy;
                        self.waveforms.push(wf);
                        let wi = self.waveforms.len() - 1;
                        new_wf_indices.push(wi);
                        if let Some(c) = clip {
                            while self.audio_clips.len() < wi {
                                self.audio_clips.push(AudioClipData {
                                    samples: std::sync::Arc::new(Vec::new()),
                                    sample_rate: 44100,
                                    duration_secs: 0.0,
                                });
                            }
                            self.audio_clips.push(c);
                        }
                    }
                    d.waveform_indices = new_wf_indices;
                    self.components.push(d);
                    new_selected.push(HitTarget::ComponentDef(self.components.len() - 1));
                }
                ClipboardItem::ComponentInstance(mut ci) => {
                    ci.position[0] += dx;
                    ci.position[1] += dy;
                    self.component_instances.push(ci);
                    new_selected.push(HitTarget::ComponentInstance(
                        self.component_instances.len() - 1,
                    ));
                }
                ClipboardItem::MidiClip(mut mc) => {
                    mc.position[0] += dx;
                    mc.position[1] += dy;
                    self.midi_clips.push(mc);
                    new_selected.push(HitTarget::MidiClip(self.midi_clips.len() - 1));
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
                    self.instrument_regions.push(ir);
                    new_selected.push(HitTarget::InstrumentRegion(self.instrument_regions.len() - 1));
                }
            }
        }

        self.selected = new_selected;
        self.sync_audio_clips();
    }

    fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut obj_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Object(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut wf_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut er_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::EffectRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut pb_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::PluginBlock(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut lr_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::LoopRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut xr_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ExportRegion(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut comp_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ComponentDef(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut inst_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::ComponentInstance(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut mc_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::MidiClip(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut ir_indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::InstrumentRegion(i) => Some(*i),
                _ => None,
            })
            .collect();

        obj_indices.sort_unstable_by(|a, b| b.cmp(a));
        wf_indices.sort_unstable_by(|a, b| b.cmp(a));
        er_indices.sort_unstable_by(|a, b| b.cmp(a));
        pb_indices.sort_unstable_by(|a, b| b.cmp(a));
        lr_indices.sort_unstable_by(|a, b| b.cmp(a));
        xr_indices.sort_unstable_by(|a, b| b.cmp(a));
        comp_indices.sort_unstable_by(|a, b| b.cmp(a));
        inst_indices.sort_unstable_by(|a, b| b.cmp(a));
        mc_indices.sort_unstable_by(|a, b| b.cmp(a));
        ir_indices.sort_unstable_by(|a, b| b.cmp(a));

        // Delete instances first
        for &i in &inst_indices {
            if i < self.component_instances.len() {
                self.component_instances.remove(i);
            }
        }

        // Delete component defs (also removes their instances and releases waveforms)
        for &i in &comp_indices {
            if i < self.components.len() {
                let comp = self.components.remove(i);
                // Remove instances that reference this component
                self.component_instances
                    .retain(|inst| inst.component_id != comp.id);
                // Also delete the owned waveforms (sorted descending)
                let mut owned_wf: Vec<usize> = comp.waveform_indices;
                owned_wf.sort_unstable_by(|a, b| b.cmp(a));
                for &wi in &owned_wf {
                    if wi < self.waveforms.len() {
                        self.waveforms.remove(wi);
                    }
                    if wi < self.audio_clips.len() {
                        self.audio_clips.remove(wi);
                    }
                }
                // Fix waveform indices in remaining components
                for other in &mut self.components {
                    for idx in &mut other.waveform_indices {
                        for &removed in &owned_wf {
                            if *idx > removed {
                                *idx -= 1;
                            }
                        }
                    }
                }
            }
        }

        for &i in &obj_indices {
            if i < self.objects.len() {
                self.objects.remove(i);
            }
        }
        for &i in &wf_indices {
            if i < self.waveforms.len() {
                self.waveforms.remove(i);
            }
            if i < self.audio_clips.len() {
                self.audio_clips.remove(i);
            }
        }
        for &i in &er_indices {
            if i < self.effect_regions.len() {
                self.effect_regions.remove(i);
            }
        }
        for &i in &pb_indices {
            if i < self.plugin_blocks.len() {
                self.plugin_blocks.remove(i);
            }
        }
        for &i in &lr_indices {
            if i < self.loop_regions.len() {
                self.loop_regions.remove(i);
            }
        }
        for &i in &xr_indices {
            if i < self.export_regions.len() {
                self.export_regions.remove(i);
            }
        }
        for &i in &mc_indices {
            if i < self.midi_clips.len() {
                self.midi_clips.remove(i);
            }
        }
        for &i in &ir_indices {
            if i < self.instrument_regions.len() {
                self.instrument_regions.remove(i);
            }
        }

        self.selected.clear();
        self.sync_audio_clips();
        self.sync_loop_region();
        println!("Deleted selected items");
    }

    fn drop_audio_from_browser(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            return;
        }

        if let Some(loaded) = load_audio_file(path) {
            self.push_undo();
            let world = self.camera.screen_to_world(self.mouse_pos);
            let height = 150.0;
            let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            println!(
                "  Loaded: {} ({:.1}s, {} Hz, {} samples/ch)",
                filename,
                loaded.duration_secs,
                loaded.sample_rate,
                loaded.left_samples.len(),
            );
            let left_peaks = Arc::new(WaveformPeaks::build(&loaded.left_samples));
            let right_peaks = Arc::new(WaveformPeaks::build(&loaded.right_samples));
            self.waveforms.push(WaveformView {
                audio: Arc::new(AudioData {
                    left_samples: loaded.left_samples,
                    right_samples: loaded.right_samples,
                    left_peaks,
                    right_peaks,
                    sample_rate: loaded.sample_rate,
                    filename,
                }),
                position: [snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm), world[1] - height * 0.5],
                size: [loaded.width, height],
                color: WAVEFORM_COLORS[color_idx],
                border_radius: 8.0,
                fade_in_px: 0.0,
                fade_out_px: 0.0,
                fade_in_curve: 0.0,
                fade_out_curve: 0.0,
                volume: 1.0,
                disabled: false,
                sample_offset_px: 0.0,
                automation: AutomationData::new(),
            });
            self.audio_clips.push(AudioClipData {
                samples: loaded.samples,
                sample_rate: loaded.sample_rate,
                duration_secs: loaded.duration_secs,
            });
            self.sync_audio_clips();
        } else {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            self.toast_manager.push(
                format!(
                    "Cannot load '{}' \u{2014} unsupported or corrupted file",
                    filename
                ),
                ui::toast::ToastKind::Error,
            );
        }
    }

}

// ---------------------------------------------------------------------------
// Native macOS menu bar
// ---------------------------------------------------------------------------

fn build_app_menu(storage: Option<&Storage>) -> MenuState {
    use muda::{
        accelerator::{Accelerator, Code, Modifiers},
        Menu, MenuItem, PredefinedMenuItem, Submenu,
    };

    let menu = Menu::new();

    // -- App menu (Layers) --
    let app_menu = Submenu::new("Layers", true);
    let _ = app_menu.append(&PredefinedMenuItem::about(None, None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let settings_item = MenuItem::new(
        "Settings...",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
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
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
    );
    let _ = file_menu.append(&new_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let save_project_item = MenuItem::new(
        "Save Project",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyS)),
    );
    let _ = file_menu.append(&save_project_item);
    let _ = file_menu.append(&PredefinedMenuItem::separator());

    let open_project_item = MenuItem::new(
        "Open Project...",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyO)),
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
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyZ)),
    );
    let redo_item = MenuItem::new(
        "Redo",
        true,
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::KeyZ,
        )),
    );
    let copy_item = MenuItem::new(
        "Copy",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)),
    );
    let paste_item = MenuItem::new(
        "Paste",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)),
    );
    let select_all_item = MenuItem::new(
        "Select All",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyA)),
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

fn main() {
    env_logger::init();

    println!("╔════════════════════════════════════════════╗");
    println!("║              Layers                         ║");
    println!("╠════════════════════════════════════════════╣");
    println!("║  Space              →  Play / Pause        ║");
    println!("║  Click background   →  Seek playhead       ║");
    println!("║  Drop audio file    →  Add to canvas       ║");
    println!("║  Two-finger scroll  →  Pan canvas          ║");
    println!("║  Cmd + scroll       →  Zoom in/out         ║");
    println!("║  Pinch              →  Zoom in/out         ║");
    println!("║  Middle drag        →  Pan canvas          ║");
    println!("║  Left drag empty    →  Selection rectangle ║");
    println!("║  Left drag object   →  Move (+ selection)  ║");
    println!("║  Cmd + K / Right-click → Command palette   ║");
    println!("║  Backspace / Delete →  Delete selected     ║");
    println!("║  Cmd + Z / ⇧⌘Z     →  Undo / Redo         ║");
    println!("║  Cmd + S            →  Save project        ║");
    println!("║  Cmd + B            →  Toggle browser      ║");
    println!("║  Cmd + Shift + A    →  Add folder           ║");
    println!("╚════════════════════════════════════════════╝");

    let skip_load = std::env::args().any(|a| a == "--empty");

    let event_loop = EventLoop::new().unwrap();

    let mut app = App::new(skip_load);
    let menu_state = build_app_menu(app.storage.as_ref());
    app.menu_state = Some(menu_state);

    event_loop.run_app(&mut app).unwrap();
}
