use super::*;

impl App {
    /// Get the actual audio device sample rate, falling back to 48000 if no engine.
    #[cfg(feature = "native")]
    pub(crate) fn plugin_sample_rate(&self) -> f64 {
        self.audio_engine
            .as_ref()
            .map(|e| e.sample_rate() as f64)
            .unwrap_or(48000.0)
    }

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
        let (reload_sr, reload_bs) = (self.plugin_sample_rate(), self.settings.buffer_size as i32);
        // Reload lightweight instruments waiting for the scanner
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        for inst in self.instruments.values_mut() {
            if inst.plugin_id.is_empty() {
                continue;
            }
            let has_gui = inst.gui.lock().ok().map_or(false, |g| g.is_some());
            if !has_gui {
                if let Some(entry) = self.plugin_registry.instruments.iter().find(|e| e.info.unique_id == inst.plugin_id) {
                    inst.plugin_path = entry.info.path.clone();
                }
                let path = inst.plugin_path.to_string_lossy().to_string();
                if !path.is_empty() {
                    if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &inst.plugin_id, &inst.plugin_name) {
                        gui.hide();
                        gui.setup_processing(reload_sr, reload_bs);
                        if let Some(state) = &inst.pending_state {
                            gui.set_state(state);
                        }
                        if let Ok(mut g) = inst.gui.lock() {
                            *g = Some(gui);
                        }
                        println!("  Reloaded instrument '{}'", inst.plugin_name);
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

    /// Add a VST3 effect plugin to a waveform's shared effect chain.
    /// Creates a new chain if the waveform doesn't have one yet.
    pub(crate) fn add_plugin_to_waveform_chain(&mut self, wf_id: EntityId, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let _block_size = self.settings.buffer_size;

        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let mut slot = effects::EffectChainSlot::new(
            plugin_id.to_string(),
            plugin_name.to_string(),
            plugin_path,
        );

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path = slot.plugin_path.to_string_lossy().to_string();
            if !path.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path, plugin_id, plugin_name) {
                    gui.setup_processing(self.plugin_sample_rate(), _block_size as i32);
                    println!("  Opened effect chain plugin '{}'", plugin_name);
                    if let Ok(mut g) = slot.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        // Get or create effect chain for this waveform
        let chain_id = if let Some(wf) = self.waveforms.get(&wf_id) {
            wf.effect_chain_id
        } else {
            return;
        };

        let chain_id = match chain_id {
            Some(id) => id,
            None => {
                let id = new_id();
                self.effect_chains.insert(id, effects::EffectChain::new());
                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                    wf.effect_chain_id = Some(id);
                }
                id
            }
        };

        if let Some(chain) = self.effect_chains.get_mut(&chain_id) {
            chain.slots.push(slot);
        }

        // Open the right window for this waveform
        self.open_right_window_for(wf_id);
        self.selected.clear();
        self.selected.push(HitTarget::Waveform(wf_id));
        self.sync_audio_clips();
        println!("  Added '{}' to waveform effect chain", plugin_name);
        self.request_redraw();
    }

    pub(crate) fn add_plugin_to_instrument_chain(&mut self, inst_id: EntityId, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let _block_size = self.settings.buffer_size;

        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let mut slot = effects::EffectChainSlot::new(
            plugin_id.to_string(),
            plugin_name.to_string(),
            plugin_path,
        );

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path = slot.plugin_path.to_string_lossy().to_string();
            if !path.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path, plugin_id, plugin_name) {
                    gui.setup_processing(self.plugin_sample_rate(), _block_size as i32);
                    if let Ok(mut g) = slot.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        // Get or create effect chain for this instrument
        let chain_id = if let Some(inst) = self.instruments.get(&inst_id) {
            inst.effect_chain_id
        } else {
            return;
        };

        let chain_id = match chain_id {
            Some(id) => id,
            None => {
                let id = new_id();
                self.effect_chains.insert(id, effects::EffectChain::new());
                if let Some(inst) = self.instruments.get_mut(&inst_id) {
                    inst.effect_chain_id = Some(id);
                }
                id
            }
        };

        if let Some(chain) = self.effect_chains.get_mut(&chain_id) {
            chain.slots.push(slot);
        }

        self.update_right_window_for_instrument(inst_id);
        self.sync_audio_clips();
        self.request_redraw();
    }

    pub(crate) fn add_plugin_to_group_chain(&mut self, group_id: EntityId, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let _block_size = self.settings.buffer_size;

        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let mut slot = effects::EffectChainSlot::new(
            plugin_id.to_string(),
            plugin_name.to_string(),
            plugin_path,
        );

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path = slot.plugin_path.to_string_lossy().to_string();
            if !path.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path, plugin_id, plugin_name) {
                    gui.setup_processing(self.plugin_sample_rate(), _block_size as i32);
                    if let Ok(mut g) = slot.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        // Get or create effect chain for this group
        let chain_id = if let Some(g) = self.groups.get(&group_id) {
            g.effect_chain_id
        } else {
            return;
        };

        let chain_id = match chain_id {
            Some(id) => id,
            None => {
                let id = new_id();
                self.effect_chains.insert(id, effects::EffectChain::new());
                if let Some(g) = self.groups.get_mut(&group_id) {
                    g.effect_chain_id = Some(id);
                }
                id
            }
        };

        if let Some(chain) = self.effect_chains.get_mut(&chain_id) {
            chain.slots.push(slot);
        }

        self.sync_audio_clips();
        self.request_redraw();
        println!("  Added '{}' to group effect chain", plugin_name);
    }

    pub(crate) fn add_plugin_to_master_chain(&mut self, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();
        let _block_size = self.settings.buffer_size;

        let plugin_path = self
            .plugin_registry
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let mut slot = effects::EffectChainSlot::new(
            plugin_id.to_string(),
            plugin_name.to_string(),
            plugin_path,
        );

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path = slot.plugin_path.to_string_lossy().to_string();
            if !path.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path, plugin_id, plugin_name) {
                    gui.setup_processing(self.plugin_sample_rate(), _block_size as i32);
                    if let Ok(mut g) = slot.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        let chain_id = match self.master.effect_chain_id {
            Some(id) => id,
            None => {
                let id = new_id();
                self.effect_chains.insert(id, effects::EffectChain::new());
                self.master.effect_chain_id = Some(id);
                id
            }
        };

        if let Some(chain) = self.effect_chains.get_mut(&chain_id) {
            chain.slots.push(slot);
        }

        self.sync_audio_clips();
        self.request_redraw();
    }

    pub(crate) fn add_plugin_to_selected_effect_region(&mut self, _plugin_id: &str, _plugin_name: &str) {}

    /// Open the VST3 GUI for a specific slot in an effect chain.
    pub(crate) fn open_effect_chain_slot_gui(&mut self, chain_id: EntityId, slot_idx: usize) {
        let Some(chain) = self.effect_chains.get(&chain_id) else { return; };
        let Some(slot) = chain.slots.get(slot_idx) else { return; };
        let gui_arc = slot.gui.clone();
        drop(chain);

        if let Ok(guard) = gui_arc.lock() {
            if let Some(gui) = guard.as_ref() {
                gui.show();
            }
        };
    }

    pub(crate) fn add_instrument(&mut self, plugin_id: &str, plugin_name: &str) {
        self.ensure_plugins_scanned();

        let ppb = grid::pixels_per_beat(self.bpm);
        let beats_per_bar = 4.0;
        let clip_w = ppb * beats_per_bar * midi::MIDI_CLIP_DEFAULT_BARS as f32;
        let clip_h = midi::MIDI_CLIP_DEFAULT_HEIGHT;

        let (sw, sh, _) = self.screen_info();
        let center = self.camera.screen_to_world([sw * 0.5, sh * 0.5]);
        let clip_pos = [center[0] - clip_w * 0.5, center[1] - clip_h * 0.5];

        let plugin_path = self
            .plugin_registry
            .instruments
            .iter()
            .find(|e| e.info.unique_id == plugin_id)
            .map(|e| e.info.path.clone())
            .unwrap_or_default();

        let inst_id = new_id();
        let mut inst = instruments::Instrument::new();
        inst.plugin_id = plugin_id.to_string();
        inst.plugin_name = plugin_name.to_string();
        inst.plugin_path = plugin_path.clone();

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let path_str = plugin_path.to_string_lossy().to_string();
            if !path_str.is_empty() {
                if let Some(gui) = vst3_gui::Vst3Gui::open(&path_str, plugin_id, plugin_name) {
                    if gui.setup_processing(self.plugin_sample_rate(), self.settings.buffer_size as i32) {
                        println!("  Set up audio processing for instrument '{}'", plugin_name);
                    } else {
                        println!("  Warning: audio processing setup failed for '{}'", plugin_name);
                    }
                    println!("  Opened native GUI for instrument '{}'", plugin_name);
                    if let Ok(mut g) = inst.gui.lock() {
                        *g = Some(gui);
                    }
                }
            }
        }

        let inst_snap = instruments::InstrumentSnapshot {
            name: inst.name.clone(),
            plugin_id: inst.plugin_id.clone(),
            plugin_name: inst.plugin_name.clone(),
            plugin_path: inst.plugin_path.clone(),
            volume: inst.volume,
            pan: inst.pan,
            effect_chain_id: inst.effect_chain_id,
            disabled: inst.disabled,
        };
        self.instruments.insert(inst_id, inst);

        let mut clip = midi::MidiClip::new(clip_pos, &self.settings);
        clip.size = [clip_w, clip_h];
        clip.instrument_id = Some(inst_id);
        let clip_id = new_id();
        self.midi_clips.insert(clip_id, clip.clone());

        self.push_op(crate::operations::Operation::Batch(vec![
            crate::operations::Operation::CreateInstrument { id: inst_id, data: inst_snap },
            crate::operations::Operation::CreateMidiClip { id: clip_id, data: clip },
        ]));

        self.selected.clear();
        self.selected.push(HitTarget::MidiClip(clip_id));
        self.keyboard_instrument_id = Some(inst_id);

        self.sync_audio_clips();
        self.request_redraw();
        println!("  Added instrument '{}'", plugin_name);
    }

    pub(crate) fn open_instrument_region_gui(&mut self, id: EntityId) {
        // InstrumentRegion is gone; delegate to Instrument GUI
        self.open_instrument_gui(id);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn open_instrument_gui(&mut self, id: EntityId) {
        let Some(inst) = self.instruments.get(&id) else {
            return;
        };
        if inst.plugin_id.is_empty() {
            return;
        }

        let path = inst.plugin_path.to_string_lossy().to_string();
        let uid = inst.plugin_id.clone();
        let name = inst.plugin_name.clone();

        if !path.is_empty() {
            let has_gui = inst.gui.lock().ok().map_or(false, |g| g.is_some());
            if has_gui {
                let is_visible = inst.gui.lock()
                    .ok()
                    .map_or(false, |g| g.as_ref().map_or(false, |gui| gui.is_open()));
                if !is_visible {
                    if let Ok(g) = inst.gui.lock() {
                        if let Some(gui) = g.as_ref() {
                            gui.show();
                        }
                    }
                }
                return;
            }

            if let Some(gui) = vst3_gui::Vst3Gui::open(&path, &uid, &name) {
                println!("  Opened native GUI for instrument '{}'", name);
                if let Ok(mut g) = inst.gui.lock() {
                    *g = Some(gui);
                }
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(crate) fn open_instrument_gui(&mut self, _id: EntityId) {
        // VST3 instrument GUIs are not available on this platform
    }
}
