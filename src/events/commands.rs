use super::*;

impl App {
    pub(crate) fn execute_command(&mut self, action: CommandAction) {
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
                        let wf_mut = self.waveforms.get_mut(&wf_id).unwrap();
                        wf_mut.audio = new_audio;
                        wf_mut.is_reversed = !wf_mut.is_reversed;

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
            CommandAction::AddTextNote => {
                self.add_text_note();
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
                    self.mark_dirty();
                }
            }
            CommandAction::MoveLayerUp => {
                if let Some(target) = self.selected.first() {
                    let id = match target {
                        HitTarget::Waveform(id) |
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
                        HitTarget::Waveform(id) |
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
            CommandAction::OpenInstrumentGui => {
                if let Some(id) = self.keyboard_instrument_id {
                    self.open_instrument_gui(id);
                }
            }
        }
        self.request_redraw();
    }
}
