//! Project I/O: save, load, menu building, and related types.
//!
//! This module is only compiled when the `native` feature is enabled.

use super::*;

use muda::{MenuId, Submenu as MudaSubmenu};
use storage::{ProjectState, Storage};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub(crate) struct MenuState {
    pub(crate) menu: muda::Menu,
    pub(crate) new_project: MenuId,
    pub(crate) save_project: MenuId,
    pub(crate) open_project: MenuId,
    pub(crate) settings: MenuId,
    pub(crate) undo: MenuId,
    pub(crate) redo: MenuId,
    pub(crate) copy: MenuId,
    pub(crate) paste: MenuId,
    pub(crate) select_all: MenuId,
    pub(crate) open_project_items: Vec<(MenuId, String)>,
    pub(crate) open_submenu: MudaSubmenu,
    pub(crate) initialized: bool,
}

/// Result of fetching audio from remote storage on a background thread.
pub(crate) struct PendingRemoteAudioFetch {
    pub(crate) wf_id: EntityId,
    pub(crate) audio: Arc<AudioData>,
    pub(crate) ac: AudioClipData,
}

// ---------------------------------------------------------------------------
// App methods
// ---------------------------------------------------------------------------

impl App {
    pub(crate) fn save_project_state(&mut self) {
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
                        instrument_id: mc.instrument_id.map(|id| id.to_string()).unwrap_or_default(),
                    }
                }).collect(),
                layer_tree: layers::tree_to_stored(&self.layer_tree),
                text_notes: storage::text_notes_to_stored(&self.text_notes),
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

    pub(crate) fn save_project(&mut self) {
        self.save_project_state();
        if let Some(storage) = &self.storage {
            if storage.is_temp_project() {
                self.save_project_as();
            }
        }
    }

    pub(crate) fn save_project_as(&mut self) {
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

    pub(crate) fn handle_menu_event(&mut self, id: MenuId) {
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

    pub(crate) fn new_project(&mut self) {
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

    pub(crate) fn load_project(&mut self, project_path: &str) {
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
                is_reversed: false,
                disabled: sw.disabled,
                sample_offset_px: sw.sample_offset_px,
                automation: AutomationData::from_stored(&sw.automation_volume, &sw.automation_pan),
                effect_chain_id: None,
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
                instrument_id: if smc.instrument_id.is_empty() { None } else { smc.instrument_id.parse().ok() },
                disabled: false,
            }))
            .collect();

        {
            let mut tree = layers::tree_from_stored(&state.layer_tree);
            layers::sync_tree(
                &mut tree,
                &self.instruments,
                &self.midi_clips,
                &self.waveforms,
                &self.effect_regions,
                &self.plugin_blocks,
            );
            self.layer_tree = tree;
        }

        self.text_notes = storage::text_notes_from_stored(state.text_notes);
        self.editing_text_note = None;

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
                            gui.setup_processing(48000.0, self.settings.buffer_size as i32);
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

    pub(crate) fn refresh_open_project_menu(&mut self) {
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
}

// ---------------------------------------------------------------------------
// Native macOS menu bar
// ---------------------------------------------------------------------------

pub(crate) fn build_app_menu(storage: Option<&Storage>) -> MenuState {
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
