use super::*;

impl App {
    pub(crate) fn open_add_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.sample_browser.add_folder(folder);
            self.sample_browser.visible = true;
            self.save_browser_folders_to_settings();
            self.mark_dirty();
            self.request_redraw();
        }
    }

    pub(crate) fn save_browser_folders_to_settings(&self) {
        let mut settings = crate::settings::Settings::load();
        settings.sample_library_folders = self
            .sample_browser
            .root_folders
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        settings.save();
    }

    pub(crate) fn ensure_plugins_scanned(&mut self) {
        if self.plugin_registry.is_scanned() {
            return;
        }
        self.plugin_registry.ensure_scanned();

        let effects: Vec<ui::browser::PluginEntry> = self
            .plugin_registry
            .plugins
            .iter()
            .map(|e| ui::browser::PluginEntry {
                unique_id: e.info.unique_id.clone(),
                name: e.info.name.clone(),
                manufacturer: e.info.manufacturer.clone(),
                is_instrument: false,
            })
            .collect();
        let instruments: Vec<ui::browser::PluginEntry> = self
            .plugin_registry
            .instruments
            .iter()
            .map(|e| ui::browser::PluginEntry {
                unique_id: e.info.unique_id.clone(),
                name: e.info.name.clone(),
                manufacturer: e.info.manufacturer.clone(),
                is_instrument: true,
            })
            .collect();
        self.sample_browser.set_plugins(effects, instruments);

        // Reload any saved plugin blocks that were waiting for the scanner.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        for pb in self.plugin_blocks.values_mut() {
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
                        println!("  Reloaded plugin '{}'", pb.plugin_name);
                    }
                }
            }
        }
        // Reload any saved instrument regions that were waiting for the scanner
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        for ir in self.instrument_regions.values_mut() {
            if ir.plugin_id.is_empty() {
                continue;
            }
            let has_gui = ir.gui.lock().ok().map_or(false, |g| g.is_some());
            if !has_gui {
                if let Some(entry) = self.plugin_registry.instruments.iter().find(|e| e.info.unique_id == ir.plugin_id) {
                    ir.plugin_path = entry.info.path.clone();
                }
                let path = ir.plugin_path.to_string_lossy().to_string();
                if !path.is_empty() {
                    if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &ir.plugin_id, &ir.plugin_name) {
                        gui.hide();
                        gui.setup_processing(48000.0, 512);
                        if let Some(state) = &ir.pending_state {
                            gui.set_state(state);
                        }
                        if let Ok(mut g) = ir.gui.lock() {
                            *g = Some(gui);
                        }
                        println!("  Reloaded instrument '{}'", ir.plugin_name);
                    }
                }
            }
        }
        self.sync_audio_clips();
    }

    pub(crate) fn build_palette_plugin_entries(&self) -> Vec<ui::palette::PluginPickerEntry> {
        let mut entries = Vec::new();
        for e in &self.plugin_registry.instruments {
            entries.push(ui::palette::PluginPickerEntry {
                name: e.info.name.clone(),
                manufacturer: e.info.manufacturer.clone(),
                unique_id: e.info.unique_id.clone(),
                is_instrument: true,
            });
        }
        for e in &self.plugin_registry.plugins {
            entries.push(ui::palette::PluginPickerEntry {
                name: e.info.name.clone(),
                manufacturer: e.info.manufacturer.clone(),
                unique_id: e.info.unique_id.clone(),
                is_instrument: false,
            });
        }
        entries
    }

    pub(crate) fn add_plugin_block(&mut self, position: [f32; 2], plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let _sample_rate = 48000.0;
        let _block_size = 512;

        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let pb = effects::PluginBlock::new(
            position,
            plugin_id.to_string(),
            plugin_name.to_string(),
            plugin_path,
        );

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path = pb.plugin_path.to_string_lossy().to_string();
            if !path.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path, plugin_id, plugin_name) {
                    gui.setup_processing(_sample_rate, _block_size as i32);
                    println!("  Opened native GUI for '{}'", plugin_name);
                    if let Ok(mut g) = pb.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        self.plugin_blocks.insert(new_id(), pb);
        println!("  Added plugin block '{}'", plugin_name);
        self.sync_audio_clips();
    }

    pub(crate) fn add_plugin_to_selected_effect_region(&mut self, plugin_id: &str, plugin_name: &str) {
        let region_id = self.selected.iter().find_map(|t| {
            if let HitTarget::EffectRegion(id) = t {
                Some(*id)
            } else {
                None
            }
        });
        let Some(region_id) = region_id else {
            println!("  No effect region selected, cannot add plugin");
            return;
        };
        let Some(region) = self.effect_regions.get(&region_id) else {
            return;
        };

        let existing = effects::collect_plugins_for_region(region, &self.plugin_blocks);
        let position = if let Some(&last_id) = existing.last() {
            if let Some(last) = self.plugin_blocks.get(&last_id) {
                [
                    last.position[0] + last.size[0] + 10.0,
                    last.position[1],
                ]
            } else {
                [region.position[0] + 10.0, region.position[1] + 30.0]
            }
        } else {
            [region.position[0] + 10.0, region.position[1] + 30.0]
        };

        self.add_plugin_block(position, plugin_id, plugin_name);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn open_plugin_block_gui(&mut self, id: EntityId) {
        let Some(pb) = self.plugin_blocks.get(&id) else {
            return;
        };

        self.ensure_plugins_scanned();
        {
            let pb = self.plugin_blocks.get_mut(&id).unwrap();
            if pb.plugin_path.as_os_str().is_empty() {
                if let Some(entry) = self.plugin_registry.plugins.iter().find(|e| e.info.unique_id == pb.plugin_id) {
                    pb.plugin_path = entry.info.path.clone();
                }
            }
        }

        let pb = self.plugin_blocks.get_mut(&id).unwrap();
        let saved_state = pb.pending_state.take();
        let saved_params = pb.pending_params.take();
        let pb = self.plugin_blocks.get(&id).unwrap();
        let path = pb.plugin_path.to_string_lossy().to_string();
        let uid = pb.plugin_id.clone();
        let name = pb.plugin_name.clone();

        if !path.is_empty() {
            let has_gui = pb.gui.lock().ok().map_or(false, |g| g.is_some());
            if has_gui {
                let is_visible = pb.gui.lock()
                    .ok()
                    .map_or(false, |g| g.as_ref().map_or(false, |gui| gui.is_open()));
                if !is_visible {
                    if let Ok(g) = pb.gui.lock() {
                        if let Some(gui) = g.as_ref() {
                            gui.show();
                            println!("  Showed native GUI for '{}'", name);
                        }
                    }
                }
                return;
            }

            if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &uid, &name) {
                gui.setup_processing(48000.0, 512);
                if let Some(state) = saved_state {
                    if !state.is_empty() {
                        gui.set_state(&state);
                    }
                }
                if let Some(params) = saved_params {
                    gui.set_all_parameters(&params);
                    println!("  Restored {} GUI parameters", params.len());
                }
                println!("  Opened native GUI for '{}'", name);
                if let Ok(mut g) = pb.gui.lock() {
                    *g = Some(gui);
                }
                return;
            }
        }

        // Fallback: open parameter editor using gui instance
        let mut params = Vec::new();
        if let Ok(guard) = pb.gui.lock() {
            if let Some(gui) = guard.as_ref() {
                let count = gui.parameter_count();
                for param_idx in 0..count {
                    let val = gui.get_parameter(param_idx).unwrap_or(0.0);
                    params.push(ui::plugin_editor::ParamEntry {
                        name: format!("Param {}", param_idx),
                        unit: String::new(),
                        value: val as f32,
                        default: 0.0,
                    });
                }
            }
        }
        self.plugin_editor = Some(ui::plugin_editor::PluginEditorWindow::new(
            id, 0, name, params,
        ));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(crate) fn open_plugin_block_gui(&mut self, _id: EntityId) {
        // VST3 plugin GUIs are not available on this platform
    }

    pub(crate) fn add_instrument(&mut self, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();

        let padding = instruments::INSTRUMENT_REGION_PADDING;

        let ppb = grid::pixels_per_beat(self.bpm);
        let beats_per_bar = 4.0;
        let clip_w = ppb * beats_per_bar * midi::MIDI_CLIP_DEFAULT_BARS as f32;
        let clip_h = midi::MIDI_CLIP_DEFAULT_HEIGHT;

        let region_w = clip_w + padding * 2.0;
        let region_h = clip_h + padding * 2.0;

        let (sw, sh, _) = self.screen_info();
        let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
        let region_pos = [center[0] - region_w * 0.5, center[1] - region_h * 0.5];
        let clip_pos = [region_pos[0] + padding, region_pos[1] + padding];

        let mut ir = instruments::InstrumentRegion::new(region_pos, [region_w, region_h]);
        ir.plugin_id = plugin_id.to_string();
        ir.plugin_name = plugin_name.to_string();

        let plugin_path = self
            .plugin_registry
            .instruments
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();
        ir.plugin_path = plugin_path.clone();

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path_str = plugin_path.to_string_lossy().to_string();
            if !path_str.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path_str, plugin_id, plugin_name) {
                    if gui.setup_processing(48000.0, 512) {
                        println!("  Set up audio processing for instrument '{}'", plugin_name);
                    } else {
                        println!("  Warning: audio processing setup failed for '{}'", plugin_name);
                    }
                    println!("  Opened native GUI for instrument '{}'", plugin_name);
                    if let Ok(mut g) = ir.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        let ir_id = new_id();
        let ir_snap = instruments::InstrumentRegionSnapshot {
            position: ir.position, size: ir.size,
            name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
            plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
        };
        self.instrument_regions.insert(ir_id, ir);

        let mut clip = midi::MidiClip::new(clip_pos, &self.settings);
        clip.size = [clip_w, clip_h];
        clip.instrument_region_id = Some(ir_id);
        let clip_id = new_id();
        self.midi_clips.insert(clip_id, clip.clone());

        self.push_op(crate::operations::Operation::Batch(vec![
            crate::operations::Operation::CreateInstrumentRegion { id: ir_id, data: ir_snap },
            crate::operations::Operation::CreateMidiClip { id: clip_id, data: clip },
        ]));

        self.selected.clear();
        self.selected.push(HitTarget::InstrumentRegion(ir_id));
        self.editing_midi_clip = Some(clip_id);
        self.selected_midi_notes.clear();

        self.request_redraw();
        println!("  Added instrument '{}'", plugin_name);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn open_instrument_region_gui(&mut self, id: EntityId) {
        let Some(ir) = self.instrument_regions.get(&id) else {
            return;
        };
        if ir.plugin_id.is_empty() {
            return;
        }

        let path = ir.plugin_path.to_string_lossy().to_string();
        let uid = ir.plugin_id.clone();
        let name = ir.plugin_name.clone();

        if !path.is_empty() {
            let has_gui = ir.gui.lock().ok().map_or(false, |g| g.is_some());
            if has_gui {
                let is_visible = ir.gui.lock()
                    .ok()
                    .map_or(false, |g| g.as_ref().map_or(false, |gui| gui.is_open()));
                if !is_visible {
                    if let Ok(g) = ir.gui.lock() {
                        if let Some(gui) = g.as_ref() {
                            gui.show();
                        }
                    }
                }
                return;
            }

            if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &uid, &name) {
                println!("  Opened native GUI for instrument '{}'", name);
                if let Ok(mut g) = ir.gui.lock() {
                    *g = Some(gui);
                }
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(crate) fn open_instrument_region_gui(&mut self, _id: EntityId) {
        // VST3 instrument GUIs are not available on this platform
    }
}
