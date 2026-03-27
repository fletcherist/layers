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
                for &id in self.loop_regions.keys() {
                    self.selected.push(HitTarget::LoopRegion(id));
                }
                for &id in self.components.keys() {
                    self.selected.push(HitTarget::ComponentDef(id));
                }
                for &id in self.component_instances.keys() {
                    self.selected.push(HitTarget::ComponentInstance(id));
                }
                for &id in self.instruments.keys() {
                    self.selected.push(HitTarget::Instrument(id));
                }
                for &id in self.midi_clips.keys() {
                    self.selected.push(HitTarget::MidiClip(id));
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
            CommandAction::SetMidiClipColor(idx) => {
                if let Some(&color) = WAVEFORM_COLORS.get(idx) {
                    for target in self.selected.clone() {
                        if let HitTarget::MidiClip(i) = target {
                            if let Some(mc) = self.midi_clips.get_mut(&i) { mc.color = color; }
                        }
                    }
                    self.mark_dirty();
                }
            }
            CommandAction::MoveLayerUp => {
                if let Some(target) = self.selected.first() {
                    let id = match target {
                        HitTarget::Waveform(id) |
                        HitTarget::MidiClip(id) => Some(*id),
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
                        HitTarget::MidiClip(id) => Some(*id),
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
            CommandAction::SetWarpOff | CommandAction::SetWarpRePitch | CommandAction::SetWarpSemitone | CommandAction::SetWarpPaulStretch | CommandAction::SetWarpComplex => {
                let new_mode = match action {
                    CommandAction::SetWarpRePitch => ui::waveform::WarpMode::RePitch,
                    CommandAction::SetWarpSemitone => ui::waveform::WarpMode::Semitone,
                    CommandAction::SetWarpPaulStretch => ui::waveform::WarpMode::PaulStretch,
                    CommandAction::SetWarpComplex => ui::waveform::WarpMode::Complex,
                    _ => ui::waveform::WarpMode::Off,
                };
                if let Some(rw) = &self.right_window {
                    let wf_id = rw.target_id();
                    let old_mode = self.waveforms.get(&wf_id).map(|w| w.warp_mode);
                    if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.warp_mode = new_mode;
                            if new_mode == ui::waveform::WarpMode::RePitch {
                                if let Some(clip) = self.audio_clips.get(&wf_id) {
                                    let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                    wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                                }
                            }
                            if new_mode == ui::waveform::WarpMode::Complex {
                                if let Some(clip) = self.audio_clips.get(&wf_id) {
                                    let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                    wf.size[0] = original_duration_px * (wf.sample_bpm / self.bpm);
                                }
                            }
                        }
                        // Restore original audio when switching away from PaulStretch
                        #[cfg(feature = "native")]
                        if old_mode == Some(ui::waveform::WarpMode::PaulStretch) && new_mode != ui::waveform::WarpMode::PaulStretch {
                            self.restore_paulstretch(wf_id);
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
            CommandAction::CreateGroup => {
                if self.selected.len() >= 1 {
                    let targets: Vec<HitTarget> = self.selected.clone();
                    if let Some((pos, size)) = crate::group::bounding_box_of_selection(
                        &targets,
                        &self.waveforms,
                        &self.midi_clips,
                        &self.text_notes,
                        &self.objects,
                        &self.loop_regions,
                        &self.export_regions,
                        &self.components,
                        &self.component_instances,
                    ) {
                        let mut member_ids: Vec<crate::entity_id::EntityId> = Vec::new();
                        let mut seen = std::collections::HashSet::new();
                        for t in &targets {
                            let id = match t {
                                // Resolve MIDI clips to their parent instrument
                                HitTarget::MidiClip(id) => {
                                    if let Some(inst_id) = self.midi_clips.get(id).and_then(|mc| mc.instrument_id) {
                                        inst_id
                                    } else {
                                        *id
                                    }
                                }
                                HitTarget::Object(id)
                                | HitTarget::Waveform(id)
                                | HitTarget::LoopRegion(id)
                                | HitTarget::ExportRegion(id)
                                | HitTarget::ComponentDef(id)
                                | HitTarget::ComponentInstance(id)
                                | HitTarget::TextNote(id)
                                | HitTarget::Group(id)
                                | HitTarget::Instrument(id) => *id,
                            };
                            if seen.insert(id) {
                                member_ids.push(id);
                            }
                        }
                        let group_id = crate::entity_id::new_id();
                        let group_name = format!("Group {}", self.groups.len() + 1);
                        let group = crate::group::Group::new(group_id, group_name, pos, size, member_ids);
                        self.groups.insert(group_id, group.clone());
                        self.push_op(operations::Operation::CreateGroup { id: group_id, data: group });
                        self.selected.clear();
                        self.selected.push(HitTarget::Group(group_id));
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::UngroupSelected => {
                let group_target = self.selected.iter().find_map(|t| {
                    if let HitTarget::Group(id) = t { Some(*id) } else { None }
                });
                if let Some(group_id) = group_target {
                    if let Some(group) = self.groups.shift_remove(&group_id) {
                        let member_ids = group.member_ids.clone();
                        self.push_op(operations::Operation::DeleteGroup { id: group_id, data: group });
                        self.selected.clear();
                        for mid in &member_ids {
                            // Try to figure out the HitTarget type for each member
                            if self.objects.contains_key(mid) {
                                self.selected.push(HitTarget::Object(*mid));
                            } else if self.waveforms.contains_key(mid) {
                                self.selected.push(HitTarget::Waveform(*mid));
                            } else if self.midi_clips.contains_key(mid) {
                                self.selected.push(HitTarget::MidiClip(*mid));
                            } else if self.text_notes.contains_key(mid) {
                                self.selected.push(HitTarget::TextNote(*mid));
                            } else if self.components.contains_key(mid) {
                                self.selected.push(HitTarget::ComponentDef(*mid));
                            } else if self.component_instances.contains_key(mid) {
                                self.selected.push(HitTarget::ComponentInstance(*mid));
                            } else if self.loop_regions.contains_key(mid) {
                                self.selected.push(HitTarget::LoopRegion(*mid));
                            } else if self.export_regions.contains_key(mid) {
                                self.selected.push(HitTarget::ExportRegion(*mid));
                            } else if self.groups.contains_key(mid) {
                                self.selected.push(HitTarget::Group(*mid));
                            } else if self.instruments.contains_key(mid) {
                                self.selected.push(HitTarget::Instrument(*mid));
                            }
                        }
                        self.mark_dirty();
                    }
                }
            }
            CommandAction::RemovePlaceFromSidebar(idx) => {
                self.sample_browser.remove_folder(idx);
                let mut s = crate::settings::Settings::load();
                s.sample_library_folders = self.sample_browser.root_folders
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                s.save();
                self.sample_browser.rebuild_entries();
                self.request_redraw();
            }
            CommandAction::ShowPlaceInFinder(idx) => {
                if let Some(path) = self.sample_browser.root_folders.get(idx) {
                    std::process::Command::new("open")
                        .arg(path)
                        .spawn()
                        .ok();
                }
            }
            CommandAction::ShareSession => {
                if let Some(p) = &mut self.command_palette {
                    p.mode = PaletteMode::ShareSession;
                    p.session_input.clear();
                    p.search_text.clear();
                }
                self.request_redraw();
                return;
            }
            CommandAction::JoinSession => {
                if let Some(p) = &mut self.command_palette {
                    p.mode = PaletteMode::JoinSession;
                    p.session_input.clear();
                    p.search_text.clear();
                }
                self.request_redraw();
                return;
            }
            CommandAction::DisconnectSession => {
                self.disconnect_session();
            }
        }
        self.request_redraw();
    }
}
