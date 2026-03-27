use super::*;

use winit::event::{ElementState, MouseButton};

impl App {
    pub(crate) fn handle_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        match button {
        MouseButton::Middle => match state {
            ElementState::Pressed => {
                if self.context_menu.is_some() || self.settings_window.is_some() || self.export_window.is_some() || self.share_window.is_some() {
                    return;
                }
                self.command_palette = None;
                self.following_user = None;
                self.drag = DragState::Panning {
                    start_mouse: self.mouse_pos,
                    start_camera: self.camera.position,
                };
                self.update_cursor();
                self.request_redraw();
            }
            ElementState::Released => {
                self.drag = DragState::None;
                self.update_cursor();
                self.request_redraw();
            }
        },

        MouseButton::Right => {
            if state == ElementState::Pressed {
                self.command_palette = None;

                // Right-click to delete automation point
                if self.automation_mode {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    let param = self.active_automation_param;
                    if let Some((wf_idx, pt_idx)) =
                        hit_test_automation_point(&self.waveforms, world, &self.camera, param)
                    {
                        let before = self.waveforms[&wf_idx].clone();
                        if let Some(wf) = self.waveforms.get_mut(&wf_idx) {
                            wf.automation
                                .lane_for_mut(param)
                                .remove_point(pt_idx);
                        }
                        let after = self.waveforms[&wf_idx].clone();
                        self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_idx, before, after });
                        self.request_redraw();
                        return;
                    }
                }

                if self.sample_browser.visible {
                    let (_, sh, scale) = self.screen_info();
                    if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                        // Right-click on a place row in the sidebar
                        if let Some(place_idx) = self.sample_browser.hit_place_row(self.mouse_pos, scale) {
                            self.context_menu = Some(ContextMenu::new(
                                self.mouse_pos,
                                MenuContext::BrowserPlace { folder_index: place_idx },
                                &self.settings,
                            ));
                            self.request_redraw();
                            return;
                        }

                        if let Some(idx) =
                            self.sample_browser.item_at(self.mouse_pos, sh, scale)
                        {
                            let entry = &self.sample_browser.entries[idx];
                            let menu_ctx = match &entry.kind {
                                ui::browser::EntryKind::LayerNode { id, kind, .. } => {
                                    self.selected.clear();
                                    let target = match kind {
                                        crate::layers::LayerNodeKind::Waveform => Some(HitTarget::Waveform(*id)),
                                        crate::layers::LayerNodeKind::Instrument => {
                                            self.keyboard_instrument_id = Some(*id);
                                            Some(HitTarget::Instrument(*id))
                                        },
                                        crate::layers::LayerNodeKind::MidiClip => Some(HitTarget::MidiClip(*id)),
                                        crate::layers::LayerNodeKind::TextNote => Some(HitTarget::TextNote(*id)),
                                        crate::layers::LayerNodeKind::Group => Some(HitTarget::Group(*id)),
                                    };
                                    if let Some(target) = target {
                                        self.selected.push(target);
                                    }
                                    MenuContext::LayerNode { kind: *kind }
                                }
                                ui::browser::EntryKind::Master => {
                                    // No context menu for Main Layer
                                    self.request_redraw();
                                    return;
                                }
                                _ => {
                                    self.browser_context_path = Some(entry.path.clone());
                                    MenuContext::BrowserEntry
                                }
                            };
                            self.context_menu = Some(ContextMenu::new(
                                self.mouse_pos,
                                menu_ctx,
                                &self.settings,
                            ));
                            self.request_redraw();
                            return;
                        }

                        // Right-click on empty space in browser — do nothing
                        return;
                    }
                }

                // Right-click on UI panels — do nothing
                {
                    let (sw, sh, scale) = self.screen_info();
                    if self.right_window.is_some() {
                        let (pp, ps) = ui::right_window::RightWindow::panel_rect(sw, sh, scale);
                        let in_rw = self.mouse_pos[0] >= pp[0]
                            && self.mouse_pos[0] <= pp[0] + ps[0]
                            && self.mouse_pos[1] >= pp[1]
                            && self.mouse_pos[1] <= pp[1] + ps[1];
                        if in_rw {
                            return;
                        }
                    }
                    if self.settings_window.as_ref().map_or(false, |sw_win| sw_win.contains(self.mouse_pos, sw, sh, scale)) {
                        return;
                    }
                    if self.export_window.as_ref().map_or(false, |ew| ew.contains(self.mouse_pos, sw, sh, scale)) {
                        return;
                    }
                    if ui::transport::TransportPanel::contains(self.mouse_pos, sw, sh, scale) {
                        return;
                    }
                    if self.command_palette.as_ref().map_or(false, |cp| cp.contains(self.mouse_pos, sw, sh, scale)) {
                        return;
                    }
                }

                let world = self.camera.screen_to_world(self.mouse_pos);

                if let Some(mc_idx) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get(&mc_idx) {
                        if mc.contains(world) {
                            let menu_ctx = MenuContext::MidiClipEdit {
                                grid_mode: mc.grid_mode,
                                triplet_grid: mc.triplet_grid,
                            };
                            self.context_menu =
                                Some(ContextMenu::new(self.mouse_pos, menu_ctx, &self.settings));
                            self.request_redraw();
                            return;
                        }
                    }
                }

                let (sw, sh, _) = self.screen_info();
                let hit = hit_test(
                    &self.objects,
                    &self.waveforms,
                    &self.loop_regions,
                    &self.export_regions,
                    &self.components,
                    &self.component_instances,
                    &self.midi_clips,
                    &self.text_notes,
                    &self.groups,
                    self.editing_component,
                    world,
                    &self.camera,
                    self.editing_group,
                    sw,
                    sh,
                );
                let hit = hit.map(|h| self.redirect_to_group(h));
                let menu_ctx = match hit {
                    Some(HitTarget::ComponentInstance(_)) => {
                        if !self.selected.contains(&hit.unwrap()) {
                            self.selected.clear();
                            self.selected.push(hit.unwrap());
                        }
                        MenuContext::ComponentInstance
                    }
                    Some(HitTarget::ComponentDef(_)) => {
                        if !self.selected.contains(&hit.unwrap()) {
                            self.selected.clear();
                            self.selected.push(hit.unwrap());
                        }
                        MenuContext::ComponentDef
                    }
                    Some(target) => {
                        if !self.selected.contains(&target) {
                            self.selected.clear();
                            self.selected.push(target);
                        }
                        let has_waveforms = self
                            .selected
                            .iter()
                            .any(|t| matches!(t, HitTarget::Waveform(_)));
                        let has_midi_clips = self
                            .selected
                            .iter()
                            .any(|t| matches!(t, HitTarget::MidiClip(_)));
                        let instrument_id = self.selected.iter().find_map(|t| match t {
                            HitTarget::MidiClip(id) => self.midi_clips.get(id).and_then(|mc| mc.instrument_id),
                            _ => None,
                        });
                        if let Some(inst_id) = instrument_id {
                            self.keyboard_instrument_id = Some(inst_id);
                        }
                        let current_waveform_color = self
                            .selected
                            .iter()
                            .find_map(|t| match t {
                                HitTarget::Waveform(i) => self.waveforms.get(i).map(|wf| wf.color),
                                _ => None,
                            });
                        let current_midi_color = self
                            .selected
                            .iter()
                            .find_map(|t| match t {
                                HitTarget::MidiClip(i) => self.midi_clips.get(i).map(|mc| mc.color),
                                _ => None,
                            });
                        MenuContext::Selection {
                            has_waveforms,
                            has_midi_clips,
                            has_instrument: instrument_id.is_some(),
                            current_waveform_color,
                            current_midi_color,
                        }
                    }
                    None => {
                        self.selected.clear();
                        MenuContext::Grid
                    }
                };
                self.context_menu =
                    Some(ContextMenu::new(self.mouse_pos, menu_ctx, &self.settings));
                self.request_redraw();
            }
        }

        MouseButton::Left => match state {
            ElementState::Pressed => {
                // Stop sample preview when clicking anywhere except the preview strip
                #[cfg(feature = "native")]
                {
                    let (_, sh, scale) = self.screen_info();
                    let in_preview_strip = self.sample_browser.visible
                        && self.sample_browser.preview_audio.is_some()
                        && {
                            let strip = self.sample_browser.preview_strip_rect(sh, scale);
                            self.mouse_pos[0] <= strip[0] + strip[2]
                                && self.mouse_pos[1] >= strip[1]
                        };
                    if !in_preview_strip {
                        if let Some(engine) = &self.audio_engine {
                            if engine.is_preview_playing() {
                                engine.stop_preview();
                            }
                        }
                    }
                }
                // Handle browser toggle button click (≡ in header or collapsed strip)
                {
                    let (_, _, scale) = self.screen_info();
                    if self.sample_browser.hit_toggle_button(self.mouse_pos, scale) {
                        self.sample_browser.visible = !self.sample_browser.visible;
                        #[cfg(feature = "native")]
                        if self.sample_browser.visible {
                            self.refresh_project_browser_entries();
                            self.ensure_plugins_scanned();
                        }
                        self.request_redraw();
                        return;
                    }
                }
                // Handle follow mode avatar click
                if self.network.is_connected() {
                    if let Some(clicked_uid) = self.hit_test_avatar_circles() {
                        if self.following_user == Some(clicked_uid) {
                            self.following_user = None;
                        } else {
                            self.following_user = Some(clicked_uid);
                            // Immediately sync camera to followed user's viewport
                            if let Some(remote) = self.remote_users.get(&clicked_uid) {
                                if let Some(vp) = &remote.viewport {
                                    self.camera.position = vp.position;
                                    self.camera.zoom = vp.zoom;
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }
                // Handle search clear button click
                {
                    let (_, _, scale) = self.screen_info();
                    if self.sample_browser.visible && self.sample_browser.hit_clear_button(self.mouse_pos, scale) {
                        self.sample_browser.search_query.clear();
                        self.sample_browser.search_focused = false;
                        self.sample_browser.rebuild_entries();
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                    }
                }
                // Handle search bar focus/unfocus
                {
                    let (_, _, scale) = self.screen_info();
                    if self.sample_browser.visible {
                        let hit = self.sample_browser.hit_search_bar(self.mouse_pos, scale);
                        if hit != self.sample_browser.search_focused {
                            self.sample_browser.search_focused = hit;
                            if hit {
                                self.sample_browser.cursor_blink_start = crate::TimeInstant::now();
                                self.sample_browser.cursor_blink_visible = true;
                            }
                            self.sample_browser.text_dirty = true;
                            self.request_redraw();
                        }
                    } else if self.sample_browser.search_focused {
                        self.sample_browser.search_focused = false;
                        self.sample_browser.text_dirty = true;
                    }
                }
                // Cancel browser inline rename if clicking outside the editing entry
                if self.sample_browser.editing_browser_name.is_some() {
                    let (_, sh, scale) = self.screen_info();
                    let still_on_entry = self.sample_browser.visible
                        && self.sample_browser.contains(self.mouse_pos, sh, scale)
                        && self.sample_browser.item_at(self.mouse_pos, sh, scale)
                            .and_then(|idx| self.sample_browser.entries.get(idx))
                            .and_then(|e| match &e.kind {
                                ui::browser::EntryKind::LayerNode { id, .. } => Some(*id),
                                ui::browser::EntryKind::ProjectInstrument { id } => Some(*id),
                                _ => None,
                            })
                            == self.sample_browser.editing_browser_name.as_ref().map(|(id, _, _)| *id);
                    if !still_on_entry {
                        self.sample_browser.editing_browser_name = None;
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                    }
                }
                // Cancel canvas waveform name editing if clicking outside that waveform
                if let Some((id, _)) = &self.editing_waveform_name.clone() {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    let still_on = self.waveforms.get(id).map_or(false, |wf| {
                        world[0] >= wf.position[0] && world[0] <= wf.position[0] + wf.size[0]
                            && world[1] >= wf.position[1] && world[1] <= wf.position[1] + wf.size[1]
                    });
                    if !still_on {
                        self.editing_waveform_name = None;
                        self.request_redraw();
                    }
                }
                if self.editing_bpm.is_editing() {
                    let (sw, sh, scale) = self.screen_info();
                    if !TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                        self.editing_bpm.cancel();
                        self.request_redraw();
                    }
                }

                // Cancel vol_entry editing on click outside the text
                {
                    let (sw, sh, scale) = self.screen_info();
                    let should_cancel = self.right_window.as_ref().map_or(false, |rw| {
                        rw.vol_entry.is_editing()
                            && !rw.hit_test_vol_text(self.mouse_pos, sw, sh, scale)
                    });
                    if should_cancel {
                        if let Some(rw) = &mut self.right_window {
                            rw.vol_entry.cancel();
                        }
                        self.request_redraw();
                    }
                }

                // Export window click
                if self.export_window.is_some() {
                    let (scr_w, scr_h, scale) = self.screen_info();
                    let inside = self.export_window.as_ref().map_or(false, |ew| {
                        ew.contains(self.mouse_pos, scr_w, scr_h, scale)
                    });
                    if inside {
                        // Format dropdown clicks
                        if let Some(ew) = &mut self.export_window {
                            if ew.handle_format_click(self.mouse_pos, scr_w, scr_h, scale) {
                                self.request_redraw();
                                return;
                            }
                            // Click inside window but outside dropdown — close it
                            if ew.format_dropdown_open {
                                ew.format_dropdown_open = false;
                                self.request_redraw();
                            }
                        }

                        // Export button click
                        let export_clicked = self.export_window.as_ref().map_or(false, |ew| {
                            ew.hit_test_export_button(self.mouse_pos, scr_w, scr_h, scale)
                        });
                        if export_clicked {
                            self.start_export();
                            self.request_redraw();
                            return;
                        }
                    } else {
                        // Click outside: close dropdown first, then window
                        let is_idle = self.export_window.as_ref().map_or(false, |ew| {
                            ew.state == ui::export_window::ExportState::Idle
                        });
                        if is_idle {
                            let dd_open = self.export_window.as_ref().map_or(false, |ew| ew.format_dropdown_open);
                            if dd_open {
                                if let Some(ew) = &mut self.export_window {
                                    ew.format_dropdown_open = false;
                                }
                            } else {
                                self.export_window = None;
                            }
                        }
                    }
                    self.request_redraw();
                    return;
                }

                // Share window click
                if self.share_window.is_some() {
                    let (scr_w, scr_h, scale) = self.screen_info();
                    let inside = self.share_window.as_ref().map_or(false, |sw| {
                        sw.contains(self.mouse_pos, scr_w, scr_h, scale)
                    });
                    if inside {
                        let copy_clicked = self.share_window.as_ref().map_or(false, |sw| {
                            sw.hit_copy_button(self.mouse_pos, scr_w, scr_h, scale)
                        });
                        if copy_clicked {
                            let url = self.share_window.as_ref().map(|sw| sw.url.clone());
                            if let Some(url) = url {
                                self.copy_share_url_to_clipboard(&url);
                            }
                            if let Some(sw) = &mut self.share_window {
                                sw.copied = true;
                                sw.copied_timer = 2.0;
                            }
                        }
                    } else {
                        self.share_window = None;
                    }
                    self.request_redraw();
                    return;
                }

                // Share button click
                {
                    let (scr_w, _scr_h, scale) = self.screen_info();
                    let (sbp, sbs) = crate::share_button_rect(scr_w, scale);
                    let on_btn = self.mouse_pos[0] >= sbp[0]
                        && self.mouse_pos[0] <= sbp[0] + sbs[0]
                        && self.mouse_pos[1] >= sbp[1]
                        && self.mouse_pos[1] <= sbp[1] + sbs[1];
                    if on_btn {
                        self.open_share_window();
                        self.request_redraw();
                        return;
                    }
                }

                // Settings window click
                #[cfg(feature = "native")]
                if self.settings_window.is_some() {
                    let (scr_w, scr_h, scale) = self.screen_info();
                    let inside = self.settings_window.as_ref().map_or(false, |sw| {
                        sw.contains(self.mouse_pos, scr_w, scr_h, scale)
                    });
                    if inside {
                        // Try audio dropdown interaction first
                        let prev_output_device = self.settings.audio_output_device.clone();
                        let prev_input_device = self.settings.audio_input_device.clone();
                        let prev_buffer_size = self.settings.buffer_size;
                        let audio_consumed =
                            self.settings_window.as_mut().map_or(false, |sw| {
                                sw.handle_audio_click(
                                    self.mouse_pos,
                                    &mut self.settings,
                                    scr_w,
                                    scr_h,
                                    scale,
                                )
                            });
                        if audio_consumed {
                            self.settings.save();

                            if self.settings.audio_output_device != prev_output_device {
                                println!(
                                    "[audio] Output device changed: '{}' -> '{}'",
                                    prev_output_device, self.settings.audio_output_device
                                );

                                let old_pos = self
                                    .audio_engine
                                    .as_ref()
                                    .map(|e| e.position_seconds());
                                let old_vol =
                                    self.audio_engine.as_ref().map(|e| e.master_volume());
                                let was_playing = self
                                    .audio_engine
                                    .as_ref()
                                    .map_or(false, |e| e.is_playing());

                                let device_name =
                                    if self.settings.audio_output_device == "No Device" {
                                        None
                                    } else {
                                        Some(self.settings.audio_output_device.as_str())
                                    };
                                self.audio_engine =
                                    AudioEngine::new_with_device(device_name, self.settings.buffer_size as usize);

                                if let Some(ref engine) = self.audio_engine {
                                    let actual = engine.device_name().to_string();
                                    if self.settings.audio_output_device != actual {
                                        println!(
                                            "[audio] Device '{}' not available, using '{}'",
                                            self.settings.audio_output_device, actual
                                        );
                                        self.settings.audio_output_device = actual;
                                        self.settings.save();
                                    }
                                    if let Some(pos) = old_pos {
                                        engine.seek_to_seconds(pos);
                                    }
                                    if let Some(vol) = old_vol {
                                        engine.set_master_volume(vol);
                                    }
                                } else {
                                    println!("[audio] Warning: failed to create audio engine for device");
                                }

                                self.sync_audio_clips();
                                if was_playing {
                                    if let Some(engine) = &self.audio_engine {
                                        engine.toggle_playback();
                                    }
                                }
                            }

                            if self.settings.buffer_size != prev_buffer_size {
                                println!(
                                    "[audio] Buffer size changed: {} -> {}",
                                    prev_buffer_size, self.settings.buffer_size
                                );

                                let old_pos = self.audio_engine.as_ref().map(|e| e.position_seconds());
                                let old_vol = self.audio_engine.as_ref().map(|e| e.master_volume());
                                let was_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());

                                let device_name = if self.settings.audio_output_device == "No Device" {
                                    None
                                } else {
                                    Some(self.settings.audio_output_device.as_str())
                                };
                                self.audio_engine = AudioEngine::new_with_device(
                                    device_name,
                                    self.settings.buffer_size as usize,
                                );

                                if let Some(ref engine) = self.audio_engine {
                                    if let Some(pos) = old_pos { engine.seek_to_seconds(pos); }
                                    if let Some(vol) = old_vol { engine.set_master_volume(vol); }
                                }
                                self.sync_audio_clips();
                                if was_playing {
                                    if let Some(engine) = &self.audio_engine {
                                        engine.toggle_playback();
                                    }
                                }
                            }

                            #[cfg(feature = "native")]
                            if self.settings.audio_input_device != prev_input_device {
                                println!(
                                    "[audio] Input device changed: '{}' -> '{}'",
                                    prev_input_device, self.settings.audio_input_device
                                );

                                // Recreate aggregate device for new input/output pair
                                #[cfg(target_os = "macos")]
                                {
                                    self.aggregate_device = None; // drop old
                                    let have_in = self.settings.audio_input_device != "No Device"
                                        && !self.settings.audio_input_device.is_empty();
                                    let have_out = self.settings.audio_output_device != "No Device"
                                        && !self.settings.audio_output_device.is_empty();
                                    if have_in && have_out {
                                        self.aggregate_device = crate::aggregate_device::AggregateDevice::new(
                                            &self.settings.audio_input_device,
                                            &self.settings.audio_output_device,
                                        );
                                    }
                                }

                                #[cfg(target_os = "macos")]
                                let agg_name = self.aggregate_device.as_ref().map(|a| a.name.clone());
                                #[cfg(not(target_os = "macos"))]
                                let agg_name: Option<String> = None;

                                let device_name =
                                    if self.settings.audio_input_device == "No Device" {
                                        None
                                    } else {
                                        agg_name.as_deref().or(Some(self.settings.audio_input_device.as_str()))
                                    };
                                let mut new_rec = AudioRecorder::new_with_device(device_name);
                                if let (Some(ref mut rec), Some(ref eng)) =
                                    (&mut new_rec, &self.audio_engine)
                                {
                                    rec.set_monitor_ring(
                                        eng.monitor_ring(),
                                        eng.monitoring_enabled_flag(),
                                        eng.monitor_input_channels_flag(),
                                        eng.monitor_input_sample_rate_flag(),
                                    );
                                }
                                self.recorder = new_rec;
                            }

                            self.request_redraw();
                            return;
                        }

                        // Try theme preset dropdown interaction
                        let theme_consumed =
                            self.settings_window.as_mut().map_or(false, |sw| {
                                sw.handle_theme_panel_click(
                                    self.mouse_pos,
                                    &mut self.settings,
                                    scr_w,
                                    scr_h,
                                    scale,
                                )
                            });
                        if theme_consumed {
                            self.settings.save();
                            self.request_redraw();
                            return;
                        }

                        // Try developer dropdown interaction
                        let dev_consumed =
                            self.settings_window.as_mut().map_or(false, |sw| {
                                sw.handle_developer_click(
                                    self.mouse_pos,
                                    &mut self.settings,
                                    scr_w,
                                    scr_h,
                                    scale,
                                )
                            });
                        if dev_consumed {
                            self.settings.save();
                            self.request_redraw();
                            return;
                        }

                        // Try plug-ins panel interaction
                        let plugins_consumed =
                            self.settings_window.as_mut().map_or(false, |sw| {
                                sw.handle_plugins_click(
                                    self.mouse_pos,
                                    &mut self.settings,
                                    scr_w,
                                    scr_h,
                                    scale,
                                )
                            });
                        if plugins_consumed {
                            self.settings.save();
                            // Consume rescan flag
                            let needs_rescan = self
                                .settings_window
                                .as_ref()
                                .map_or(false, |sw| sw.rescan_requested);
                            if needs_rescan {
                                if let Some(sw) = &mut self.settings_window {
                                    sw.rescan_requested = false;
                                }
                                #[cfg(feature = "native")]
                                {
                                    self.plugin_registry.rescan();
                                    self.ensure_plugins_scanned();
                                }
                            }
                            // Consume browse flag
                            let needs_browse = self
                                .settings_window
                                .as_ref()
                                .map_or(false, |sw| sw.browse_custom_folder_requested);
                            if needs_browse {
                                if let Some(sw) = &mut self.settings_window {
                                    sw.browse_custom_folder_requested = false;
                                }
                                #[cfg(feature = "native")]
                                {
                                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                        self.settings.vst3_custom_folder_path =
                                            folder.to_string_lossy().to_string();
                                        self.settings.save();
                                    }
                                }
                            }
                            self.request_redraw();
                            return;
                        }

                        let slider_hit = self.settings_window.as_ref().and_then(|sw| {
                            sw.slider_hit_test(
                                self.mouse_pos,
                                &self.settings,
                                scr_w,
                                scr_h,
                                scale,
                            )
                        });
                        if let Some(idx) = slider_hit {
                            if let Some(sw) = &mut self.settings_window {
                                sw.dragging_slider = Some(idx);
                            }
                            if let Some(sw) = &self.settings_window {
                                sw.slider_drag(
                                    idx,
                                    self.mouse_pos[0],
                                    &mut self.settings,
                                    scr_w,
                                    scr_h,
                                    scale,
                                );
                            }
                        } else if let Some(cat_idx) =
                            self.settings_window.as_ref().and_then(|sw| {
                                sw.category_at(self.mouse_pos, scr_w, scr_h, scale)
                            })
                        {
                            if let Some(sw) = &mut self.settings_window {
                                sw.active_category = CATEGORIES[cat_idx];
                                sw.open_dropdown = None;
                            }
                        }
                    } else {
                        self.settings_window = None;
                    }
                    self.request_redraw();
                    return;
                }

                // Clear vol fader / pan knob / pitch / sample_bpm focus on any click (re-set below if clicking them)
                if let Some(rw) = &mut self.right_window {
                    rw.vol_fader_focused = false;
                    rw.pan_knob_focused = false;
                    rw.pitch_focused = false;
                    rw.sample_bpm_focused = false;
                }

                // Right window knob mouse down (skip if context menu is open)
                if self.context_menu.is_none() {
                if let Some(rw) = &self.right_window {
                    let (sw, sh, scale) = self.screen_info();
                    let (pp, ps) = ui::right_window::RightWindow::panel_rect(sw, sh, scale);
                    let in_panel = self.mouse_pos[0] >= pp[0] && self.mouse_pos[0] <= pp[0] + ps[0];
                    if in_panel {
                        let wf_id = rw.target_id();
                        let hit_sm = rw.hit_test_solo_mute(self.mouse_pos, sw, sh, scale);
                        let hit_vol_text = rw.hit_test_vol_text(self.mouse_pos, sw, sh, scale);
                        let hit_vol = rw.hit_test_vol_knob(self.mouse_pos, sw, sh, scale);
                        let hit_vol_track = rw.hit_test_vol_track(self.mouse_pos, sw, sh, scale);
                        let hit_pan = rw.hit_test_pan_knob(self.mouse_pos, sw, sh, scale);
                        let hit_reverse_btn = rw.hit_test_reverse_button(self.mouse_pos, sw, sh, scale);
                        let hit_warp_btn = rw.hit_test_warp_mode_button(self.mouse_pos, sw, sh, scale);
                        let hit_warp_sel = rw.hit_test_warp_mode_selector(self.mouse_pos, sw, sh, scale);
                        let hit_sbpm_text = rw.hit_test_sample_bpm_text(self.mouse_pos, sw, sh, scale);
                        let hit_pitch_text = rw.hit_test_pitch_text(self.mouse_pos, sw, sh, scale);
                        let hit_paulstretch_text = rw.hit_test_paulstretch_factor_text(self.mouse_pos, sw, sh, scale);
                        let hit_paulstretch_render_btn = rw.hit_test_paulstretch_render_button(self.mouse_pos, sw, sh, scale);
                        if hit_sm != ui::solo_mute::SoloMuteHit::None {
                            let target_id = rw.target_id();
                            match hit_sm {
                                ui::solo_mute::SoloMuteHit::Solo => {
                                    let shift = self.modifiers.shift_key();
                                    self.toggle_solo(target_id, shift);
                                }
                                ui::solo_mute::SoloMuteHit::Mute => {
                                    self.toggle_mute_disabled(target_id);
                                }
                                ui::solo_mute::SoloMuteHit::Monitor => {
                                    if rw.is_instrument() {
                                        self.toggle_instrument_keyboard_preview(target_id);
                                    } else {
                                        self.toggle_group_monitoring(target_id);
                                    }
                                }
                                ui::solo_mute::SoloMuteHit::None => {}
                            }
                            self.update_right_window();
                            self.refresh_project_browser_entries();
                            #[cfg(feature = "native")]
                            self.sync_audio_clips();
                            self.mark_dirty();
                            return;
                        } else if hit_reverse_btn {
                            self.execute_command(ui::palette::CommandAction::ReverseSample);
                            self.update_right_window();
                            self.request_redraw();
                            return;
                        } else if hit_warp_btn {
                            // Toggle warp on/off (default to Semitone when enabling)
                            let wf_id = rw.target_id();
                            let current = rw.warp_mode;
                            let new_mode = if current == ui::waveform::WarpMode::Off {
                                ui::waveform::WarpMode::Semitone
                            } else {
                                ui::waveform::WarpMode::Off
                            };
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
                                // Restore original audio when switching away from PaulStretch
                                #[cfg(feature = "native")]
                                if current == ui::waveform::WarpMode::PaulStretch && new_mode != ui::waveform::WarpMode::PaulStretch {
                                    self.restore_paulstretch(wf_id);
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                        id: wf_id, before, after,
                                    });
                                }
                            }
                            self.update_right_window();
                            self.mark_dirty();
                            #[cfg(feature = "native")]
                            self.sync_audio_clips();
                            self.request_redraw();
                            return;
                        } else if hit_warp_sel {
                            // Open warp mode dropdown
                            let current = rw.warp_mode;
                            let (sel_pos, sel_size) = ui::right_window::RightWindow::warp_mode_selector_rect_pub(sw, sh, scale);
                            self.context_menu = Some(ContextMenu::new(
                                [sel_pos[0], sel_pos[1] + sel_size[1]],
                                MenuContext::WarpModeSelect { current },
                                &self.settings,
                            ));
                            self.request_redraw();
                            return;
                        } else if hit_sbpm_text {
                            let now = TimeInstant::now();
                            let is_dbl = now.duration_since(self.last_sample_bpm_text_click_time).as_millis() < 400;
                            self.last_sample_bpm_text_click_time = now;
                            if is_dbl {
                                if let Some(rw) = &mut self.right_window {
                                    rw.sample_bpm_entry.enter();
                                    rw.sample_bpm_dragging = false;
                                    rw.sample_bpm_focused = true;
                                    rw.vol_fader_focused = false;
                                    rw.pan_knob_focused = false;
                                    rw.pitch_focused = false;
                                }
                            } else {
                                let start_value = rw.sample_bpm;
                                if let Some(rw) = &mut self.right_window {
                                    rw.sample_bpm_dragging = true;
                                    rw.drag_start_y = self.mouse_pos[1];
                                    rw.drag_start_value = start_value;
                                    rw.sample_bpm_focused = true;
                                    rw.vol_fader_focused = false;
                                    rw.pan_knob_focused = false;
                                    rw.pitch_focused = false;
                                }
                            }
                            self.request_redraw();
                            return;
                        } else if hit_pitch_text {
                            let now = TimeInstant::now();
                            let is_dbl = now.duration_since(self.last_pitch_text_click_time).as_millis() < 400;
                            self.last_pitch_text_click_time = now;
                            if is_dbl {
                                if let Some(rw) = &mut self.right_window {
                                    rw.pitch_entry.enter();
                                    rw.pitch_dragging = false;
                                    rw.pitch_focused = true;
                                    rw.vol_fader_focused = false;
                                    rw.pan_knob_focused = false;
                                    rw.sample_bpm_focused = false;
                                }
                            } else {
                                let start_value = rw.pitch_semitones;
                                if let Some(rw) = &mut self.right_window {
                                    rw.pitch_dragging = true;
                                    rw.drag_start_y = self.mouse_pos[1];
                                    rw.drag_start_value = start_value;
                                    rw.pitch_focused = true;
                                    rw.vol_fader_focused = false;
                                    rw.pan_knob_focused = false;
                                    rw.sample_bpm_focused = false;
                                }
                            }
                            self.request_redraw();
                            return;
                        } else if hit_paulstretch_text {
                            let start_value = rw.paulstretch_factor;
                            if let Some(rw) = &mut self.right_window {
                                rw.paulstretch_factor_dragging = true;
                                rw.drag_start_y = self.mouse_pos[1];
                                rw.drag_start_value = start_value;
                                rw.paulstretch_factor_focused = true;
                                rw.vol_fader_focused = false;
                                rw.pan_knob_focused = false;
                                rw.sample_bpm_focused = false;
                                rw.pitch_focused = false;
                            }
                            self.request_redraw();
                            return;
                        } else if hit_paulstretch_render_btn {
                            let wf_id = rw.target_id();
                            #[cfg(feature = "native")]
                            {
                                self.apply_paulstretch(wf_id);
                                self.sync_audio_clips();
                            }
                            self.request_redraw();
                            return;
                        } else if hit_vol_text {
                            let now = TimeInstant::now();
                            let is_dbl = now.duration_since(self.last_vol_text_click_time).as_millis() < 400;
                            self.last_vol_text_click_time = now;
                            if is_dbl {
                                if let Some(rw) = &mut self.right_window {
                                    rw.vol_entry.enter();
                                    rw.vol_dragging = false;
                                }
                            }
                            let _ = wf_id;
                            self.request_redraw();
                            return;
                        } else if hit_vol {
                            let now = TimeInstant::now();
                            let is_dbl = now.duration_since(self.last_vol_knob_click_time).as_millis() < 400;
                            self.last_vol_knob_click_time = now;
                            if is_dbl {
                                // Double-click resets volume to 0 dB
                                let target = self.right_window.as_ref().map(|rw| rw.target);
                                let multi_ids = self.right_window.as_ref()
                                    .map(|rw| rw.multi_target_ids.clone()).unwrap_or_default();
                                let mut ops = Vec::new();
                                if let Some(ui::right_window::RightWindowTarget::Group(gid)) = target {
                                    if let Some(before) = self.groups.get(&gid).cloned() {
                                        if let Some(g) = self.groups.get_mut(&gid) {
                                            g.volume = 1.0;
                                        }
                                        if let Some(after) = self.groups.get(&gid).cloned() {
                                            ops.push(crate::operations::Operation::UpdateGroup { id: gid, before, after });
                                        }
                                    }
                                } else {
                                    for &mid in &multi_ids {
                                        if let Some(before) = self.waveforms.get(&mid).cloned() {
                                            if let Some(wf) = self.waveforms.get_mut(&mid) {
                                                wf.volume = 1.0;
                                            }
                                            if let Some(after) = self.waveforms.get(&mid).cloned() {
                                                ops.push(crate::operations::Operation::UpdateWaveform { id: mid, before, after });
                                            }
                                        }
                                    }
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.volume = 1.0;
                                    rw.vol_dragging = false;
                                    rw.vol_fader_focused = true;
                                    rw.pan_knob_focused = false;
                                    rw.pitch_focused = false;
                                    rw.sample_bpm_focused = false;
                                }
                                if ops.len() == 1 {
                                    self.push_op(ops.into_iter().next().unwrap());
                                } else if ops.len() > 1 {
                                    self.push_op(crate::operations::Operation::Batch(ops));
                                }
                                self.sync_audio_clips();
                                self.request_redraw();
                                return;
                            }
                            let start_value = ui::palette::gain_to_vol_fader_pos(rw.volume);
                            let multi_ids = rw.multi_target_ids.clone();
                            // Capture snapshots before borrowing right_window mutably
                            let snapshots: Vec<_> = multi_ids.iter()
                                .filter_map(|id| self.waveforms.get(id).map(|wf| (*id, wf.clone())))
                                .collect();
                            if let Some(rw) = &mut self.right_window {
                                rw.vol_dragging = true;
                                rw.drag_start_y = self.mouse_pos[1];
                                rw.drag_start_value = start_value;
                                rw.vol_fader_focused = true;
                                rw.pan_knob_focused = false;
                                rw.pitch_focused = false;
                                rw.sample_bpm_focused = false;
                                rw.drag_start_snapshots = snapshots;
                            }
                            let _ = wf_id;
                            self.request_redraw();
                            return;
                        } else if hit_vol_track {
                            if let Some(rw) = &mut self.right_window {
                                rw.vol_fader_focused = true;
                                rw.pan_knob_focused = false;
                                rw.pitch_focused = false;
                                rw.sample_bpm_focused = false;
                            }
                            self.request_redraw();
                            return;
                        } else if hit_pan {
                            let now = TimeInstant::now();
                            let is_dbl = now.duration_since(self.last_pan_knob_click_time).as_millis() < 400;
                            self.last_pan_knob_click_time = now;
                            if is_dbl {
                                // Double-click resets pan to center
                                let target = self.right_window.as_ref().map(|rw| rw.target);
                                let multi_ids = self.right_window.as_ref()
                                    .map(|rw| rw.multi_target_ids.clone()).unwrap_or_default();
                                let mut ops = Vec::new();
                                if let Some(ui::right_window::RightWindowTarget::Group(gid)) = target {
                                    if let Some(before) = self.groups.get(&gid).cloned() {
                                        if let Some(g) = self.groups.get_mut(&gid) {
                                            g.pan = 0.5;
                                        }
                                        if let Some(after) = self.groups.get(&gid).cloned() {
                                            ops.push(crate::operations::Operation::UpdateGroup { id: gid, before, after });
                                        }
                                    }
                                } else {
                                    for &mid in &multi_ids {
                                        if let Some(before) = self.waveforms.get(&mid).cloned() {
                                            if let Some(wf) = self.waveforms.get_mut(&mid) {
                                                wf.pan = 0.5;
                                            }
                                            if let Some(after) = self.waveforms.get(&mid).cloned() {
                                                ops.push(crate::operations::Operation::UpdateWaveform { id: mid, before, after });
                                            }
                                        }
                                    }
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.pan = 0.5;
                                    rw.pan_dragging = false;
                                    rw.pan_knob_focused = true;
                                    rw.vol_fader_focused = false;
                                    rw.pitch_focused = false;
                                    rw.sample_bpm_focused = false;
                                }
                                if ops.len() == 1 {
                                    self.push_op(ops.into_iter().next().unwrap());
                                } else if ops.len() > 1 {
                                    self.push_op(crate::operations::Operation::Batch(ops));
                                }
                                self.sync_audio_clips();
                                self.request_redraw();
                                return;
                            }
                            let start_value = rw.pan;
                            let multi_ids = rw.multi_target_ids.clone();
                            let snapshots: Vec<_> = multi_ids.iter()
                                .filter_map(|id| self.waveforms.get(id).map(|wf| (*id, wf.clone())))
                                .collect();
                            if let Some(rw) = &mut self.right_window {
                                rw.pan_dragging = true;
                                rw.drag_start_y = self.mouse_pos[1];
                                rw.drag_start_value = start_value;
                                rw.pan_knob_focused = true;
                                rw.vol_fader_focused = false;
                                rw.pitch_focused = false;
                                rw.sample_bpm_focused = false;
                                rw.drag_start_snapshots = snapshots;
                            }
                            let _ = wf_id;
                            self.request_redraw();
                            return;
                        }

                        // --- Group/MainLayer export button click → open export window ---
                        if rw.hit_test_export_button(self.mouse_pos, sw, sh, scale) {
                            if let ui::right_window::RightWindowTarget::Group(group_id) = rw.target {
                                let group_name = self.groups.get(&group_id)
                                    .map(|g| g.name.clone())
                                    .unwrap_or_else(|| "export".to_string());
                                self.export_window = Some(
                                    ui::export_window::ExportWindow::new(group_id, group_name)
                                );
                            } else if let ui::right_window::RightWindowTarget::Master = rw.target {
                                self.export_window = Some(
                                    ui::export_window::ExportWindow::new(
                                        ui::right_window::MAIN_LAYER_ID,
                                        "Main".to_string(),
                                    )
                                );
                            }
                            self.request_redraw();
                            return;
                        }

                        // --- Effect chain slot clicks ---
                        {
                            let target = rw.target;
                            let chain_id = match target {
                                ui::right_window::RightWindowTarget::Waveform(wf_id) => self.waveforms.get(&wf_id).and_then(|w| w.effect_chain_id),
                                ui::right_window::RightWindowTarget::Instrument(inst_id) => self.instruments.get(&inst_id).and_then(|i| i.effect_chain_id),
                                ui::right_window::RightWindowTarget::Group(group_id) => self.groups.get(&group_id).and_then(|g| g.effect_chain_id),
                                ui::right_window::RightWindowTarget::Master => self.master.effect_chain_id,
                                ui::right_window::RightWindowTarget::Monitor => self.monitor_effect_chain_id,
                            };
                            let slot_count = chain_id
                                .and_then(|cid| self.effect_chains.get(&cid))
                                .map_or(0, |c| c.slots.len());
                            let ref_count = chain_id.map_or(0, |cid| {
                                ui::right_window::RightWindow::chain_ref_count_all(cid, &self.waveforms, &self.instruments, &self.groups)
                            });

                            // Detach button
                            if rw.hit_test_detach_button(self.mouse_pos, ref_count, sw, sh, scale) {
                                match target {
                                    ui::right_window::RightWindowTarget::Waveform(wf_id) => self.detach_effect_chain(wf_id),
                                    ui::right_window::RightWindowTarget::Instrument(inst_id) => self.detach_instrument_effect_chain(inst_id),
                                    ui::right_window::RightWindowTarget::Group(group_id) => self.detach_group_effect_chain(group_id),
                                    ui::right_window::RightWindowTarget::Master => {
                                        self.master.effect_chain_id = None;
                                    }
                                    ui::right_window::RightWindowTarget::Monitor => {
                                        self.monitor_effect_chain_id = None;
                                        self.sync_monitor_effects();
                                    }
                                }
                                self.request_redraw();
                                return;
                            }

                            if let Some(slot_idx) = rw.hit_test_effect_slot(self.mouse_pos, slot_count, sw, sh, scale) {
                                // Bypass toggle
                                if rw.hit_test_effect_bypass(self.mouse_pos, slot_idx, sw, sh, scale) {
                                    if let Some(cid) = chain_id {
                                        if let Some(chain) = self.effect_chains.get_mut(&cid) {
                                            if let Some(slot) = chain.slots.get_mut(slot_idx) {
                                                let before = slot.snapshot();
                                                slot.bypass = !slot.bypass;
                                                let after = slot.snapshot();
                                                self.push_op(crate::operations::Operation::UpdateEffectSlot {
                                                    chain_id: cid, slot_idx, before, after,
                                                });
                                            }
                                        }
                                    }
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                    return;
                                }
                                // Delete button
                                if rw.hit_test_effect_delete(self.mouse_pos, slot_idx, sw, sh, scale) {
                                    if let Some(cid) = chain_id {
                                        if let Some(chain) = self.effect_chains.get(&cid) {
                                            if slot_idx < chain.slots.len() {
                                                let slot_snapshot = chain.slots[slot_idx].snapshot_with_state();
                                                self.effect_chains.get_mut(&cid).unwrap().slots.remove(slot_idx);

                                                let mut ops = vec![crate::operations::Operation::RemoveEffectSlot {
                                                    chain_id: cid, slot_idx, data: slot_snapshot,
                                                }];

                                                // If chain is now empty, remove it and clear references
                                                if self.effect_chains.get(&cid).map_or(true, |c| c.slots.is_empty()) {
                                                    self.effect_chains.shift_remove(&cid);
                                                    ops.push(crate::operations::Operation::DeleteEffectChain { id: cid });
                                                    for wf in self.waveforms.values_mut() {
                                                        if wf.effect_chain_id == Some(cid) {
                                                            wf.effect_chain_id = None;
                                                        }
                                                    }
                                                    for inst in self.instruments.values_mut() {
                                                        if inst.effect_chain_id == Some(cid) {
                                                            inst.effect_chain_id = None;
                                                        }
                                                    }
                                                    for g in self.groups.values_mut() {
                                                        if g.effect_chain_id == Some(cid) {
                                                            g.effect_chain_id = None;
                                                        }
                                                    }
                                                    if self.master.effect_chain_id == Some(cid) {
                                                        self.master.effect_chain_id = None;
                                                    }
                                                }

                                                let op = if ops.len() == 1 { ops.remove(0) } else { crate::operations::Operation::Batch(ops) };
                                                self.push_op(op);
                                            }
                                        }
                                    }
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                    return;
                                }
                                // Click on slot body — start potential drag for reorder
                                if let Some(cid) = chain_id {
                                    self.drag = DragState::DraggingEffectSlot {
                                        chain_id: cid,
                                        slot_idx,
                                        start_y: self.mouse_pos[1],
                                    };
                                }
                                self.request_redraw();
                                return;
                            }

                            // "Add Effect" button — switch browser to Effects tab and focus search
                            if rw.hit_test_add_effect_button(self.mouse_pos, slot_count, sw, sh, scale) {
                                self.sample_browser.active_category = ui::browser::BrowserCategory::Effects;
                                self.sample_browser.rebuild_entries();
                                self.sample_browser.visible = true;
                                self.sample_browser.search_focused = true;
                                self.request_redraw();
                                return;
                            }
                        }

                        // Click inside panel but not on knob - don't propagate
                        self.request_redraw();
                        return;
                    }
                }
                }

                if self.context_menu.is_some() {
                    let (sw, sh, scale) = self.screen_info();
                    let inside = self
                        .context_menu
                        .as_ref()
                        .map_or(false, |cm| cm.contains(self.mouse_pos, sw, sh, scale));
                    let clicked_action = self.context_menu.as_ref().and_then(|cm| {
                        let idx = cm.item_at(self.mouse_pos, sw, sh, scale)?;
                        cm.action_at(idx)
                    });

                    if let Some(action) = clicked_action {
                        self.execute_command(action);
                        self.context_menu = None;
                    } else {
                        self.context_menu = None;
                    }
                    self.request_redraw();
                    if inside {
                        return;
                    }
                    if let Some(_rw) = &self.right_window {
                        let (pp, ps) = ui::right_window::RightWindow::panel_rect(sw, sh, scale);
                        if self.mouse_pos[0] >= pp[0] && self.mouse_pos[0] <= pp[0] + ps[0] {
                            return;
                        }
                    }
                }

                if self.command_palette.is_some() {
                    let (sw, sh, scale) = self.screen_info();
                    let inside = self
                        .command_palette
                        .as_ref()
                        .map_or(false, |p| p.contains(self.mouse_pos, sw, sh, scale));

                    let is_fader = self
                        .command_palette
                        .as_ref()
                        .map_or(false, |p| matches!(p.mode, PaletteMode::VolumeFader));

                    if is_fader {
                        if inside {
                            let hit = self.command_palette.as_ref().map_or(false, |p| {
                                p.fader_hit_test(self.mouse_pos, sw, sh, scale)
                            });
                            if hit {
                                if let Some(p) = &mut self.command_palette {
                                    p.fader_dragging = true;
                                }
                            }
                        } else {
                            self.command_palette = None;
                        }
                        self.request_redraw();
                        return;
                    }

                    let picker_mode = self
                        .command_palette
                        .as_ref()
                        .and_then(|p| match p.mode {
                            PaletteMode::PluginPicker => Some(PaletteMode::PluginPicker),
                            PaletteMode::InstrumentPicker => Some(PaletteMode::InstrumentPicker),
                            _ => None,
                        });

                    if let Some(mode) = picker_mode {
                        let plugin_info = self.command_palette.as_ref().and_then(|p| {
                            let idx = p.item_at(self.mouse_pos, sw, sh, scale)?;
                            let entry_idx = *p.filtered_plugin_indices.get(idx)?;
                            let e = p.plugin_entries.get(entry_idx)?;
                            Some((e.unique_id.clone(), e.name.clone()))
                        });
                        if let Some((_plugin_id, _plugin_name)) = plugin_info {
                            self.command_palette = None;
                            #[cfg(feature = "native")]
                            if mode == PaletteMode::InstrumentPicker {
                                self.add_instrument(&_plugin_id, &_plugin_name);
                            } else {
                                self.add_plugin_to_selected_effect_region(&_plugin_id, &_plugin_name);
                            }
                            let _ = mode;
                        } else if !inside {
                            self.command_palette = None;
                        }
                    } else {
                        enum ClickResult {
                            Action(CommandAction),
                            InlinePlugin { unique_id: String, name: String, is_instrument: bool },
                        }
                        let click_result = self.command_palette.as_ref().and_then(|p| {
                            let idx = p.item_at(self.mouse_pos, sw, sh, scale)?;
                            let mut cmd_i = 0;
                            for row in p.visible_rows() {
                                match row {
                                    PaletteRow::Command(ci) => {
                                        if cmd_i == idx {
                                            return Some(ClickResult::Action(COMMANDS[*ci].action));
                                        }
                                        cmd_i += 1;
                                    }
                                    PaletteRow::Plugin(pi) => {
                                        if cmd_i == idx {
                                            let e = &p.plugin_entries[*pi];
                                            return Some(ClickResult::InlinePlugin {
                                                unique_id: e.unique_id.clone(),
                                                name: e.name.clone(),
                                                is_instrument: e.is_instrument,
                                            });
                                        }
                                        cmd_i += 1;
                                    }
                                    PaletteRow::Section(_) => {}
                                }
                            }
                            None
                        });

                        match click_result {
                            Some(ClickResult::Action(action)) => {
                                if matches!(action, CommandAction::SetMasterVolume | CommandAction::AddPlugin | CommandAction::AddInstrument) {
                                    self.execute_command(action);
                                } else {
                                    self.command_palette = None;
                                    self.execute_command(action);
                                }
                            }
                            Some(ClickResult::InlinePlugin { unique_id, name, is_instrument }) => {
                                self.command_palette = None;
                                #[cfg(feature = "native")]
                                {
                                    if is_instrument {
                                        self.add_instrument(&unique_id, &name);
                                    } else {
                                        self.add_plugin_to_selected_effect_region(&unique_id, &name);
                                    }
                                }
                                let _ = (&unique_id, &name, is_instrument);
                            }
                            None => {
                                if !inside {
                                    self.command_palette = None;
                                }
                            }
                        }
                    }
                    self.request_redraw();
                    return;
                }

                // --- sample browser click ---
                if self.sample_browser.visible {
                    let (_, sh, scale) = self.screen_info();
                    if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                        if self.sample_browser.hit_resize_handle(self.mouse_pos, scale) {
                            self.drag = DragState::ResizingBrowser;
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        } else if let Some(cat) = self.sample_browser.hit_sidebar(self.mouse_pos, scale) {
                            if cat != self.sample_browser.active_category {
                                self.sample_browser.active_category = cat;
                                self.sample_browser.scroll_offset = 0.0;
                                self.sample_browser.scroll_velocity = 0.0;
                                self.sample_browser.search_query.clear();
                                self.sample_browser.search_focused = false;
                                if cat == ui::browser::BrowserCategory::Layers {
                                    self.refresh_project_browser_entries();
                                } else {
                                    self.sample_browser.rebuild_entries();
                                }
                            }
                            self.request_redraw();
                            return;
                        } else if self.sample_browser.hit_places_add(self.mouse_pos, scale) {
                            #[cfg(feature = "native")]
                            self.open_add_folder_dialog();
                        } else if let Some(place_idx) = self.sample_browser.hit_place_row(self.mouse_pos, scale) {
                            if place_idx != self.sample_browser.selected_place {
                                self.sample_browser.selected_place = place_idx;
                                self.sample_browser.scroll_offset = 0.0;
                                self.sample_browser.scroll_velocity = 0.0;
                                self.sample_browser.rebuild_entries();
                            }
                        } else if self.sample_browser.preview_audio.is_some() && {
                            let strip = self.sample_browser.preview_strip_rect(sh, scale);
                            self.mouse_pos[1] >= strip[1]
                        } {
                            // Click is in the preview strip area
                            let toggle_rect = self.sample_browser.preview_toggle_rect(sh, scale);
                            let wf_rect = self.sample_browser.preview_waveform_rect(sh, scale);
                            let [mx, my] = self.mouse_pos;
                            if mx >= toggle_rect[0] && mx <= toggle_rect[0] + toggle_rect[2]
                                && my >= toggle_rect[1] && my <= toggle_rect[1] + toggle_rect[3]
                            {
                                self.sample_browser.auto_preview = !self.sample_browser.auto_preview;
                                self.sample_browser.text_dirty = true;
                            } else if mx >= wf_rect[0] && mx <= wf_rect[0] + wf_rect[2]
                                && my >= wf_rect[1] && my <= wf_rect[1] + wf_rect[3]
                            {
                                let norm = ((mx - wf_rect[0]) / wf_rect[2]).clamp(0.0, 1.0) as f64;
                                #[cfg(feature = "native")]
                                if let Some(engine) = &self.audio_engine {
                                    engine.seek_preview(norm);
                                }
                                // Set up pending drag so user can drag preview to canvas
                                if let Some(ref path) = self.sample_browser.preview_path {
                                    let filename = path.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    self.drag = DragState::PendingBrowserDrag {
                                        path: path.clone(),
                                        filename,
                                        start_mouse: self.mouse_pos,
                                    };
                                }
                            }
                            self.request_redraw();
                            return;
                        } else if let Some(idx) =
                            self.sample_browser.item_at(self.mouse_pos, sh, scale)
                        {
                            let entry = self.sample_browser.entries[idx].clone();
                            match &entry.kind {
                                ui::browser::EntryKind::Dir | ui::browser::EntryKind::PluginHeader => {
                                    self.sample_browser.toggle_expand(idx);
                                }
                                ui::browser::EntryKind::File => {
                                    let ext = entry
                                        .path
                                        .extension()
                                        .map(|e| e.to_string_lossy().to_lowercase())
                                        .unwrap_or_default();
                                    if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                                        self.drag = DragState::PendingBrowserDrag {
                                            path: entry.path.clone(),
                                            filename: entry.name.clone(),
                                            start_mouse: self.mouse_pos,
                                        };
                                        self.sample_browser.selected_entry = Some(idx);
                                    }
                                }
                                ui::browser::EntryKind::Plugin { unique_id, is_instrument } => {
                                    let now = TimeInstant::now();
                                    let is_dbl = now.duration_since(self.last_browser_click_time).as_millis() < 400
                                        && self.last_browser_click_idx == Some(idx);
                                    self.last_browser_click_time = now;
                                    self.last_browser_click_idx = Some(idx);

                                    if is_dbl && !is_instrument {
                                        if let Some(HitTarget::Waveform(wf_id)) = self.selected.first().copied() {
                                            self.add_plugin_to_waveform_chain(wf_id, unique_id, &entry.name);
                                            self.request_redraw();
                                            return;
                                        }
                                        // Add effect to instrument if instrument right window is open
                                        if let Some(rw) = &self.right_window {
                                            if let ui::right_window::RightWindowTarget::Instrument(inst_id) = rw.target {
                                                self.add_plugin_to_instrument_chain(inst_id, unique_id, &entry.name);
                                                self.request_redraw();
                                                return;
                                            }
                                            if let ui::right_window::RightWindowTarget::Group(group_id) = rw.target {
                                                self.add_plugin_to_group_chain(group_id, unique_id, &entry.name);
                                                self.request_redraw();
                                                return;
                                            }
                                            if let ui::right_window::RightWindowTarget::Master = rw.target {
                                                self.add_plugin_to_master_chain(unique_id, &entry.name);
                                                self.request_redraw();
                                                return;
                                            }
                                        }
                                    }
                                    self.drag = DragState::DraggingPlugin {
                                        plugin_id: unique_id.clone(),
                                        plugin_name: entry.name.clone(),
                                        is_instrument: *is_instrument,
                                    };
                                }
                                ui::browser::EntryKind::ProjectInstrument { id } => {
                                    self.keyboard_instrument_id = Some(*id);
                                    #[cfg(feature = "native")]
                                    self.sync_computer_keyboard_to_engine();
                                }
                                ui::browser::EntryKind::EmptyState => {}
                                ui::browser::EntryKind::Master => {
                                    self.selected.clear();
                                    self.sample_browser.master_selected = true;
                                    self.open_right_window_for_master();
                                    self.request_redraw();
                                }
                                ui::browser::EntryKind::LayerNode { id, kind, has_children, .. } => {
                                    self.sample_browser.master_selected = false;
                                    // Solo/Mute button hit test (right-aligned buttons)
                                    if matches!(kind, crate::layers::LayerNodeKind::Waveform | crate::layers::LayerNodeKind::Instrument | crate::layers::LayerNodeKind::Group) {
                                        let (_, _, scale) = self.screen_info();
                                        let row_right = self.sample_browser.content_x(scale) + self.sample_browser.content_width(scale);
                                        let row_cy = self.mouse_pos[1]; // Y doesn't matter for horizontal hit-test
                                        let layout = ui::solo_mute::layout_right_aligned(row_right, row_cy, scale);
                                        let show_mon = matches!(kind, crate::layers::LayerNodeKind::Group | crate::layers::LayerNodeKind::Instrument);
                                        let hit = ui::solo_mute::hit_test(&layout, self.mouse_pos, show_mon);
                                        if hit != ui::solo_mute::SoloMuteHit::None {
                                            match hit {
                                                ui::solo_mute::SoloMuteHit::Solo => {
                                                    let shift = self.modifiers.shift_key();
                                                    self.toggle_solo(*id, shift);
                                                }
                                                ui::solo_mute::SoloMuteHit::Mute => {
                                                    self.toggle_mute_disabled(*id);
                                                }
                                                ui::solo_mute::SoloMuteHit::Monitor => {
                                                    if matches!(kind, crate::layers::LayerNodeKind::Instrument) {
                                                        self.toggle_instrument_keyboard_preview(*id);
                                                    } else {
                                                        self.toggle_group_monitoring(*id);
                                                    }
                                                }
                                                ui::solo_mute::SoloMuteHit::None => {}
                                            }
                                            self.update_right_window();
                                            self.refresh_project_browser_entries();
                                            #[cfg(feature = "native")]
                                            self.sync_audio_clips();
                                            self.mark_dirty();
                                            return;
                                        }
                                    }
                                    // Double-click enters inline rename in the browser
                                    let now = TimeInstant::now();
                                    let is_dbl = now.duration_since(self.last_browser_click_time).as_millis() < 400
                                        && self.last_browser_click_idx == Some(idx);
                                    self.last_browser_click_time = now;
                                    self.last_browser_click_idx = Some(idx);
                                    if is_dbl {
                                        use crate::layers::LayerNodeKind;
                                        let initial_text = match kind {
                                            LayerNodeKind::Waveform => self.waveforms.get(id)
                                                .map(|wf| if !wf.audio.filename.is_empty() { wf.audio.filename.clone() } else { wf.filename.clone() })
                                                .unwrap_or_default(),
                                            LayerNodeKind::Group => self.groups.get(id)
                                                .map(|g| g.name.clone())
                                                .unwrap_or_default(),
                                            _ => String::new(),
                                        };
                                        if matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Group) {
                                            self.sample_browser.editing_browser_name = Some((*id, *kind, initial_text));
                                            self.sample_browser.text_dirty = true;
                                            self.request_redraw();
                                            return;
                                        }
                                    }
                                    if *has_children && !matches!(kind, crate::layers::LayerNodeKind::Instrument) {
                                        // Only toggle expand when clicking the chevron icon area
                                        let entry = &self.sample_browser.entries[idx];
                                        let (_, _, scale) = self.screen_info();
                                        let cx = self.sample_browser.content_x(scale);
                                        let indent = entry.depth as f32 * 16.0 * scale;
                                        let chev_hit_x = cx + indent + 4.0 * scale;
                                        let chev_hit_w = 20.0 * scale;
                                        if self.mouse_pos[0] >= chev_hit_x && self.mouse_pos[0] <= chev_hit_x + chev_hit_w {
                                            crate::layers::toggle_expanded(&mut self.layer_tree, *id);
                                            self.refresh_project_browser_entries();
                                        }
                                    }
                                    let shift = self.modifiers.shift_key();
                                    match kind {
                                        crate::layers::LayerNodeKind::Instrument => {
                                            self.select_with_shift(HitTarget::Instrument(*id), shift);
                                            self.keyboard_instrument_id = Some(*id);
                                            self.computer_keyboard_armed = true;
                                            #[cfg(feature = "native")]
                                            self.sync_computer_keyboard_to_engine();
                                            self.update_right_window_for_instrument(*id);
                                        }
                                        crate::layers::LayerNodeKind::MidiClip => {
                                            if !shift {
                                                if let Some(mc) = self.midi_clips.get(id) {
                                                    let (sw, sh, _) = self.screen_info();
                                                    let cx = mc.position[0] + mc.size[0] * 0.5;
                                                    let cy = mc.position[1] + mc.size[1] * 0.5;
                                                    self.camera.position = [
                                                        cx - sw * 0.5 / self.camera.zoom,
                                                        cy - sh * 0.5 / self.camera.zoom,
                                                    ];
                                                }
                                            }
                                            self.select_with_shift(HitTarget::MidiClip(*id), shift);
                                            self.update_right_window();
                                        }
                                        crate::layers::LayerNodeKind::Waveform => {
                                            if !shift {
                                                if let Some(wf) = self.waveforms.get(id) {
                                                    let (sw, sh, _) = self.screen_info();
                                                    let cx = wf.position[0] + wf.size[0] * 0.5;
                                                    let cy = wf.position[1] + wf.size[1] * 0.5;
                                                    self.camera.position = [
                                                        cx - sw * 0.5 / self.camera.zoom,
                                                        cy - sh * 0.5 / self.camera.zoom,
                                                    ];
                                                }
                                            }
                                            self.select_with_shift(HitTarget::Waveform(*id), shift);
                                            self.update_right_window();
                                        }
                                        crate::layers::LayerNodeKind::TextNote => {
                                            if !shift {
                                                if let Some(tn) = self.text_notes.get(id) {
                                                    let (sw, sh, _) = self.screen_info();
                                                    let cx = tn.position[0] + tn.size[0] * 0.5;
                                                    let cy = tn.position[1] + tn.size[1] * 0.5;
                                                    self.camera.position = [
                                                        cx - sw * 0.5 / self.camera.zoom,
                                                        cy - sh * 0.5 / self.camera.zoom,
                                                    ];
                                                }
                                            }
                                            self.select_with_shift(HitTarget::TextNote(*id), shift);
                                            self.update_right_window();
                                        }
                                        crate::layers::LayerNodeKind::Group => {
                                            if !shift {
                                                if let Some(g) = self.groups.get(id) {
                                                    let (sw, sh, _) = self.screen_info();
                                                    let cx = g.position[0] + g.size[0] * 0.5;
                                                    let cy = g.position[1] + g.size[1] * 0.5;
                                                    self.camera.position = [
                                                        cx - sw * 0.5 / self.camera.zoom,
                                                        cy - sh * 0.5 / self.camera.zoom,
                                                    ];
                                                }
                                            }
                                            self.select_with_shift(HitTarget::Group(*id), shift);
                                            self.update_right_window();
                                        }
                                    }
                                    // Initiate layer reorder drag (not for MidiClip, not when shift-selecting)
                                    if !matches!(kind, crate::layers::LayerNodeKind::MidiClip) && !is_dbl && !shift {
                                        self.drag = DragState::ReorderingLayerNode {
                                            entity_id: *id,
                                            kind: *kind,
                                            start_y: self.mouse_pos[1],
                                            start_flat_index: idx,
                                            drag_active: false,
                                            drop_target: None,
                                            source_group_before: None,
                                            hover_expand_target: None,
                                        };
                                    }
                                    self.mark_dirty();
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // --- transport panel click ---
                {
                    let (sw, sh, scale) = self.screen_info();
                    if TransportPanel::contains(self.mouse_pos, sw, sh, scale) {
                        if TransportPanel::hit_metronome_button(self.mouse_pos, sw, sh, scale) {
                            self.settings.metronome_enabled = !self.settings.metronome_enabled;
                            self.settings.save();
                            #[cfg(feature = "native")]
                            if let Some(engine) = &self.audio_engine {
                                engine.set_metronome_enabled(self.settings.metronome_enabled);
                            }
                        } else if TransportPanel::hit_computer_keyboard_button(
                            self.mouse_pos, sw, sh, scale,
                        ) {
                            let was_armed = self.computer_keyboard_armed;
                            self.computer_keyboard_armed = !self.computer_keyboard_armed;
                            if was_armed && !self.computer_keyboard_armed {
                                self.release_computer_keyboard_notes();
                            }
                            #[cfg(feature = "native")]
                            self.sync_computer_keyboard_to_engine();
                        } else if TransportPanel::hit_record_button(self.mouse_pos, sw, sh, scale)
                        {
                            self.toggle_recording();
                        } else if TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                            let now = TimeInstant::now();
                            let elapsed = now.duration_since(self.last_click_time);
                            let is_dbl = elapsed.as_millis() < 400;
                            self.last_click_time = now;
                            if is_dbl {
                                self.editing_bpm.enter();
                                self.dragging_bpm = None;
                            } else {
                                self.dragging_bpm = Some((self.bpm, self.mouse_pos[1]));
                                self.editing_bpm.cancel();
                            }
                        } else if TransportPanel::hit_play_pause(self.mouse_pos, sw, sh, scale) {
                            #[cfg(feature = "native")]
                            if let Some(engine) = &self.audio_engine {
                                engine.toggle_playback();
                            }
                            self.broadcast_playback_if_connected();
                        } else {
                            #[cfg(feature = "native")]
                            if let Some(engine) = &self.audio_engine {
                                engine.toggle_playback();
                            }
                            self.broadcast_playback_if_connected();
                        }
                        self.request_redraw();
                        return;
                    }
                }

                let world = self.camera.screen_to_world(self.mouse_pos);
                self.last_canvas_click_world = world;

                // --- component def corner resize ---
                for (&ci, def) in self.components.iter() {
                    if let Some((anchor, nwse)) = hit_test_corner_resize(def.position, def.size, world, self.camera.zoom) {
                        let before = def.clone();
                        self.drag = DragState::ResizingComponentDef { comp_id: ci, anchor, nwse, before };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // InstrumentRegion corner resize removed — instruments are non-spatial now

                // --- midi clip pitch range resize (top/bottom edges) ---
                for (&i, mc) in self.midi_clips.iter() {
                    if mc.hit_test_pitch_range_top(world, &self.camera) {
                        self.drag = DragState::ResizingMidiPitchRange {
                            clip_id: i,
                            edge: PitchRangeEdge::Top,
                            start_y: world[1],
                            before: mc.clone(),
                        };
                        self.request_redraw();
                        return;
                    }
                    if mc.hit_test_pitch_range_bottom(world, &self.camera) {
                        self.drag = DragState::ResizingMidiPitchRange {
                            clip_id: i,
                            edge: PitchRangeEdge::Bottom,
                            start_y: world[1],
                            before: mc.clone(),
                        };
                        self.request_redraw();
                        return;
                    }
                }

                // --- midi clip horizontal edge resize (left/right edges) ---
                for (&i, mc) in self.midi_clips.iter() {
                    if mc.hit_test_left_edge(world, &self.camera) {
                        self.drag = DragState::ResizingMidiClipEdge {
                            clip_id: i, is_left: true, before: mc.clone(),
                            overlap_snapshots: IndexMap::new(), overlap_temp_splits: Vec::new(),
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                    if mc.hit_test_right_edge(world, &self.camera) {
                        self.drag = DragState::ResizingMidiClipEdge {
                            clip_id: i, is_left: false, before: mc.clone(),
                            overlap_snapshots: IndexMap::new(), overlap_temp_splits: Vec::new(),
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- midi clip corner resize ---
                for (&i, mc) in self.midi_clips.iter() {
                    if let Some((anchor, nwse)) = hit_test_corner_resize(mc.position, mc.size, world, self.camera.zoom) {
                        let before = mc.clone();
                        self.drag = DragState::ResizingMidiClip { clip_id: i, anchor, nwse, before };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- midi clip body move (when not editing notes) ---
                if self.editing_midi_clip.is_none() {
                    let hit_clip = self.midi_clips.iter().find(|(_, mc)| {
                        point_in_rect(world, mc.position, mc.size)
                    }).map(|(&id, mc)| (id, mc.position));
                    if let Some((i, pos)) = hit_clip {
                        if self.camera.zoom >= MIDI_AUTO_EDIT_ZOOM_THRESHOLD {
                            self.editing_midi_clip = Some(i);
                            self.selected_midi_notes.clear();
                            // Fall through to note-editing section below
                        } else {
                            let clip_id = if self.modifiers.alt_key() {
                                let mc = self.midi_clips[&i].clone();
                                let new_id = new_id();
                                self.midi_clips.insert(new_id, mc.clone());
                                self.push_op(crate::operations::Operation::CreateMidiClip { id: new_id, data: mc });
                                new_id
                            } else {
                                i
                            };
                            let before = self.midi_clips[&clip_id].clone();
                            if !self.selected.contains(&HitTarget::MidiClip(clip_id)) {
                                self.selected.clear();
                                self.selected.push(HitTarget::MidiClip(clip_id));
                            }
                            let offset = [world[0] - pos[0], world[1] - pos[1]];
                            self.drag = DragState::MovingMidiClip {
                                clip_id, offset, before,
                                overlap_snapshots: IndexMap::new(), overlap_temp_splits: Vec::new(),
                            };
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }
                    }
                }

                // --- export region corner resize ---
                for (&i, er) in self.export_regions.iter() {
                    if let Some((anchor, nwse)) = hit_test_corner_resize(er.position, er.size, world, self.camera.zoom) {
                        let before = er.clone();
                        self.drag = DragState::ResizingExportRegion { region_id: i, anchor, nwse, before };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- export region render pill click ---
                for er in self.export_regions.values() {
                    let pill_w = EXPORT_RENDER_PILL_W / self.camera.zoom;
                    let pill_h = EXPORT_RENDER_PILL_H / self.camera.zoom;
                    let pill_x = er.position[0] + 4.0 / self.camera.zoom;
                    let pill_y = er.position[1] + 4.0 / self.camera.zoom;
                    if point_in_rect(world, [pill_x, pill_y], [pill_w, pill_h]) {
                        #[cfg(feature = "native")]
                        self.trigger_export_render();
                        self.request_redraw();
                        return;
                    }
                }

                // --- loop region edge resize ---
                for (&i, lr) in self.loop_regions.iter() {
                    if !lr.enabled {
                        continue;
                    }
                    let edge_zone = 8.0 / self.camera.zoom;
                    let left_x = lr.position[0];
                    let right_x = left_x + lr.size[0];
                    let near_left = (world[0] - left_x).abs() <= edge_zone;
                    let near_right = (world[0] - right_x).abs() <= edge_zone;
                    if near_left || near_right {
                        let before = lr.clone();
                        self.drag = DragState::ResizingLoopRegion {
                            region_id: i,
                            is_left_edge: near_left,
                            initial_position_x: lr.position[0],
                            initial_size_w: lr.size[0],
                            before,
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- text note corner resize ---
                for (&i, tn) in self.text_notes.iter() {
                    if let Some((anchor, nwse)) = hit_test_corner_resize(tn.position, tn.size, world, self.camera.zoom) {
                        let before = tn.clone();
                        self.drag = DragState::ResizingTextNote { note_id: i, anchor, nwse, before };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- fade handle drag (checked before edge resize: fade handle sits at top corners) ---
                if let Some((wf_idx, is_fade_in)) =
                    hit_test_fade_handle(&self.waveforms, world, &self.camera)
                {
                    if !self.is_in_non_entered_group(&wf_idx) {
                        let before = self.waveforms[&wf_idx].clone();
                        self.drag = DragState::DraggingFade {
                            waveform_id: wf_idx,
                            is_fade_in,
                            before,
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- waveform edge resize ---
                match hit_test_waveform_edge(&self.waveforms, world, &self.camera) {
                    WaveformEdgeHover::LeftEdge(i) | WaveformEdgeHover::RightEdge(i) if !self.is_in_non_entered_group(&i) => {
                        let is_left = matches!(self.waveform_edge_hover, WaveformEdgeHover::LeftEdge(_));
                        let wf = &self.waveforms[&i];
                        let pos_x = wf.position[0];
                        let size_w = wf.size[0];
                        let offset = wf.sample_offset_px;
                        let before = wf.clone();
                        self.drag = DragState::ResizingWaveform {
                            waveform_id: i,
                            is_left_edge: is_left,
                            initial_position_x: pos_x,
                            initial_size_w: size_w,
                            initial_offset_px: offset,
                            before,
                            overlap_snapshots: indexmap::IndexMap::new(),
                            overlap_temp_splits: Vec::new(),
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                    WaveformEdgeHover::None | _ => {}
                }

                // Check automation lane close (×) button
                if self.automation_mode {
                    if let Some(gpu) = &self.gpu {
                        for &(wf_idx, rect) in &gpu.auto_lane_close_rects {
                            let [rx, ry, rw, rh] = rect;
                            if self.mouse_pos[0] >= rx && self.mouse_pos[0] <= rx + rw
                                && self.mouse_pos[1] >= ry && self.mouse_pos[1] <= ry + rh
                            {
                                let before = self.waveforms[&wf_idx].clone();
                                let param = self.active_automation_param;
                                self.waveforms[&wf_idx].automation.lane_for_mut(param).points.clear();
                                let after = self.waveforms[&wf_idx].clone();
                                self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_idx, before, after });
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                }

                // --- automation point interaction ---
                if self.automation_mode {
                    let param = self.active_automation_param;
                    // Check existing point first
                    if let Some((wf_idx, pt_idx)) =
                        hit_test_automation_point(&self.waveforms, world, &self.camera, param)
                    {
                        let wf = &self.waveforms[&wf_idx];
                        let orig_t = wf.automation.lane_for(param).points[pt_idx].t;
                        let orig_v = wf.automation.lane_for(param).points[pt_idx].value;
                        let before = wf.clone();
                        self.drag = DragState::DraggingAutomationPoint {
                            waveform_id: wf_idx,
                            param,
                            point_idx: pt_idx,
                            original_t: orig_t,
                            original_value: orig_v,
                            before,
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                    // Check line segment for inserting new point
                    if let Some((wf_idx, t, value)) =
                        hit_test_automation_line(&self.waveforms, world, &self.camera, param)
                    {
                        let before = self.waveforms[&wf_idx].clone();
                        let pt_idx = self.waveforms.get_mut(&wf_idx).unwrap()
                            .automation
                            .lane_for_mut(param)
                            .insert_point(t, value);
                        self.drag = DragState::DraggingAutomationPoint {
                            waveform_id: wf_idx,
                            param,
                            point_idx: pt_idx,
                            original_t: t,
                            original_value: value,
                            before,
                        };
                        self.mark_dirty();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                    // Click inside waveform to create new point
                    // Collect keys in reverse to iterate back-to-front
                    let wf_keys: Vec<EntityId> = self.waveforms.keys().copied().collect();
                    for &wf_id in wf_keys.iter().rev() {
                        let wf = &self.waveforms[&wf_id];
                        if point_in_rect(world, wf.position, wf.size) {
                            let t = ((world[0] - wf.position[0]) / wf.size[0]).clamp(0.0, 1.0);
                            let y_top = wf.position[1];
                            let y_bot = wf.position[1] + wf.size[1];
                            let value = ((world[1] - y_bot) / (y_top - y_bot)).clamp(0.0, 1.0);
                            let before = self.waveforms[&wf_id].clone();
                            let pt_idx = self.waveforms.get_mut(&wf_id).unwrap()
                                .automation
                                .lane_for_mut(param)
                                .insert_point(t, value);
                            self.drag = DragState::DraggingAutomationPoint {
                                waveform_id: wf_id,
                                param,
                                point_idx: pt_idx,
                                original_t: t,
                                original_value: value,
                                before,
                            };
                            self.mark_dirty();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }
                    }
                }

                // --- fade curve shape drag ---
                if let Some((wf_idx, is_fade_in)) =
                    hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
                {
                    if !self.is_in_non_entered_group(&wf_idx) {
                        let wf = &self.waveforms[&wf_idx];
                        let start_curve = if is_fade_in { wf.fade_in_curve } else { wf.fade_out_curve };
                        let before = wf.clone();
                        self.drag = DragState::DraggingFadeCurve {
                            waveform_id: wf_idx,
                            is_fade_in,
                            start_mouse_y: self.mouse_pos[1],
                            start_curve,
                            before,
                        };
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                let (sw, sh, _) = self.screen_info();
                let hit = hit_test(
                    &self.objects,
                    &self.waveforms,
                    &self.loop_regions,
                    &self.export_regions,
                    &self.components,
                    &self.component_instances,
                    &self.midi_clips,
                    &self.text_notes,
                    &self.groups,
                    self.editing_component,
                    world,
                    &self.camera,
                    self.editing_group,
                    sw,
                    sh,
                );

                // Double-click detection: enter component edit mode
                let now = TimeInstant::now();
                let elapsed = now.duration_since(self.last_click_time);
                let dist = ((world[0] - self.last_click_world[0]).powi(2)
                    + (world[1] - self.last_click_world[1]).powi(2))
                .sqrt();
                let is_double_click =
                    elapsed.as_millis() < 400 && dist < 10.0 / self.camera.zoom;
                self.last_click_time = now;
                self.last_click_world = world;

                if is_double_click {
                    // Double-clicking a member of a non-entered group enters the group
                    if let Some(ref raw_hit) = hit {
                        let redirected = self.redirect_to_group(*raw_hit);
                        if let HitTarget::Group(group_id) = redirected {
                            self.editing_group = Some(group_id);
                            self.selected.clear();
                            self.request_redraw();
                            return;
                        }
                    }
                    if let Some(HitTarget::ComponentDef(ci)) = hit {
                        self.editing_component = Some(ci);
                        self.selected.clear();
                        println!(
                            "Entered component edit mode: {}",
                            self.components[&ci].name
                        );
                        self.request_redraw();
                        return;
                    }
                    if let Some(HitTarget::MidiClip(idx)) = hit {
                        if self.editing_midi_clip == Some(idx) {
                            self.select_area = None;
                            self.selected.clear();
                            let mc = &self.midi_clips[&idx];
                            // TODO: refactor velocity lane rendering before re-enabling
                            // let in_vel_lane = world[1] >= mc.velocity_lane_top();
                            let in_vel_lane = false;
                            let hit_note = midi::hit_test_midi_note_editing(mc, world, &self.camera, true);
                            if hit_note.is_none() && !in_vel_lane {
                                let mc = self.midi_clips.get_mut(&idx).unwrap();
                                let pitch = mc.y_to_pitch_editing(world[1], true);
                                let start_px = mc.x_to_start_px(world[0]);
                                let note = midi::MidiNote {
                                    pitch,
                                    start_px,
                                    duration_px: midi::DEFAULT_NOTE_DURATION_PX,
                                    velocity: 100,
                                };
                                mc.notes.push(note.clone());
                                let new_idx = mc.notes.len() - 1;
                                self.push_op(crate::operations::Operation::CreateMidiNote { clip_id: idx, note_idx: new_idx, data: note });
                                self.selected_midi_notes = vec![new_idx];
                                self.sync_audio_clips();
                            }
                            self.request_redraw();
                            return;
                        }
                        self.editing_midi_clip = Some(idx);
                        self.selected_midi_notes.clear();
                        println!("Entered MIDI clip edit mode");
                        self.request_redraw();
                        return;
                    }
                    // InstrumentRegion double-click removed — use MidiClip to open instrument GUI
                    if let Some(HitTarget::TextNote(idx)) = hit {
                        self.enter_text_note_edit(idx);
                        return;
                    }
                }

                // Click outside editing text note commits edit
                if let Some(ref edit) = self.editing_text_note {
                    let note_id = edit.note_id;
                    if let Some(tn) = self.text_notes.get(&note_id) {
                        if !point_in_rect(world, tn.position, tn.size) {
                            self.commit_text_note_edit();
                            self.request_redraw();
                        }
                    } else {
                        self.editing_text_note = None;
                    }
                }

                // Click outside editing MIDI clip exits edit mode
                if let Some(mc_idx) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get(&mc_idx) {
                        if !point_in_rect(world, mc.position, mc.size) {
                            self.editing_midi_clip = None;
                            self.selected_midi_notes.clear();
                            println!("Exited MIDI clip edit mode");
                        }
                    } else {
                        self.editing_midi_clip = None;
                        self.selected_midi_notes.clear();
                    }
                }

                // MIDI note editing when inside an editing clip
                if let Some(mc_idx) = self.editing_midi_clip {
                    if let Some(mc) = self.midi_clips.get(&mc_idx) {
                        let mc_pos = mc.position;
                        let mc_size = mc.size;
                        if point_in_rect(world, mc_pos, mc_size) {
                            self.select_area = None;
                            self.selected.clear();

                            // Seek playback to clicked position
                            #[cfg(feature = "native")]
                            {
                                let snapped_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
                                if let Some(engine) = &self.audio_engine {
                                    let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                    engine.seek_to_seconds(secs);
                                }
                                self.broadcast_playback_if_connected();
                            }

                            // TODO: refactor velocity lane rendering before re-enabling
                            // // Check velocity lane divider first (for resizing)
                            // if midi::hit_test_velocity_divider(&self.midi_clips[&mc_idx], world, &self.camera) {
                            //     self.drag = DragState::ResizingVelocityLane {
                            //         clip_id: mc_idx,
                            //         start_world_y: world[1],
                            //         original_height: self.midi_clips[&mc_idx].velocity_lane_height,
                            //     };
                            //     self.update_cursor();
                            //     self.request_redraw();
                            //     return;
                            // }

                            // // Check velocity bar
                            // let vel_hit = midi::hit_test_velocity_bar(&self.midi_clips[&mc_idx], world, &self.camera);
                            // if let Some(note_idx) = vel_hit {
                            //     if self.selected_midi_notes.contains(&note_idx) {
                            //         // already selected
                            //     } else if self.modifiers.shift_key() {
                            //         self.selected_midi_notes.push(note_idx);
                            //     } else {
                            //         self.selected_midi_notes.clear();
                            //         self.selected_midi_notes.push(note_idx);
                            //     }
                            //     self.push_undo();
                            //     let indices = self.selected_midi_notes.clone();
                            //     let velocities: Vec<u8> = indices.iter().map(|&ni| {
                            //         self.midi_clips[&mc_idx].notes[ni].velocity
                            //     }).collect();
                            //     self.drag = DragState::DraggingVelocity {
                            //         clip_id: mc_idx,
                            //         note_indices: indices,
                            //         original_velocities: velocities,
                            //         start_world_y: world[1],
                            //     };
                            //     self.mark_dirty();
                            //     self.request_redraw();
                            //     return;
                            // }

                            // Check if clicking on existing note (editing-aware)
                            let hit_note = midi::hit_test_midi_note_editing(&self.midi_clips[&mc_idx], world, &self.camera, true);
                            if let Some((note_idx, zone)) = hit_note {
                                if self.cmd_held() && !matches!(zone, midi::MidiNoteHitZone::VelocityBar) {
                                    let indices = if self.selected_midi_notes.contains(&note_idx) {
                                        self.selected_midi_notes.clone()
                                    } else {
                                        self.selected_midi_notes.clear();
                                        self.selected_midi_notes.push(note_idx);
                                        vec![note_idx]
                                    };
                                    let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                    let velocities: Vec<u8> = indices.iter().map(|&ni| {
                                        self.midi_clips[&mc_idx].notes[ni].velocity
                                    }).collect();
                                    self.drag = DragState::DraggingVelocity {
                                        clip_id: mc_idx,
                                        note_indices: indices,
                                        original_velocities: velocities,
                                        start_world_y: world[1],
                                        before_notes,
                                    };
                                    self.mark_dirty();
                                    self.request_redraw();
                                    return;
                                }
                                match zone {
                                    midi::MidiNoteHitZone::RightEdge => {
                                        let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                        let mut indices = self.selected_midi_notes.clone();
                                        if !indices.contains(&note_idx) {
                                            indices = vec![note_idx];
                                        }
                                        let durations: Vec<f32> = indices.iter().map(|&ni| {
                                            self.midi_clips[&mc_idx].notes[ni].duration_px
                                        }).collect();
                                        self.drag = DragState::ResizingMidiNote {
                                            clip_id: mc_idx,
                                            anchor_idx: note_idx,
                                            note_indices: indices,
                                            original_durations: durations,
                                            before_notes,
                                        };
                                    }
                                    midi::MidiNoteHitZone::LeftEdge => {
                                        let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                        let mut indices = self.selected_midi_notes.clone();
                                        if !indices.contains(&note_idx) {
                                            indices = vec![note_idx];
                                        }
                                        let starts: Vec<f32> = indices.iter().map(|&ni| {
                                            self.midi_clips[&mc_idx].notes[ni].start_px
                                        }).collect();
                                        let durations: Vec<f32> = indices.iter().map(|&ni| {
                                            self.midi_clips[&mc_idx].notes[ni].duration_px
                                        }).collect();
                                        self.drag = DragState::ResizingMidiNoteLeft {
                                            clip_id: mc_idx,
                                            anchor_idx: note_idx,
                                            note_indices: indices,
                                            original_starts: starts,
                                            original_durations: durations,
                                            before_notes,
                                        };
                                    }
                                    midi::MidiNoteHitZone::Body => {
                                        if self.selected_midi_notes.contains(&note_idx) {
                                            self.pending_midi_note_click = Some(note_idx);
                                        } else if self.modifiers.shift_key() {
                                            self.selected_midi_notes.push(note_idx);
                                        } else {
                                            self.selected_midi_notes.clear();
                                            self.selected_midi_notes.push(note_idx);
                                        }
                                        if self.modifiers.alt_key() {
                                            let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                            let mut new_indices: Vec<usize> = Vec::new();
                                            for &ni in &self.selected_midi_notes {
                                                if ni < self.midi_clips[&mc_idx].notes.len() {
                                                    let cloned = self.midi_clips[&mc_idx].notes[ni].clone();
                                                    self.midi_clips[&mc_idx].notes.push(cloned);
                                                    new_indices.push(self.midi_clips[&mc_idx].notes.len() - 1);
                                                }
                                            }
                                            self.selected_midi_notes = new_indices.clone();
                                            let nh = self.midi_clips[&mc_idx].note_height_editing(true);
                                            let offsets: Vec<[f32; 2]> = new_indices.iter().map(|&ni| {
                                                let n = &self.midi_clips[&mc_idx].notes[ni];
                                                let nx = mc_pos[0] + n.start_px;
                                                let ny = self.midi_clips[&mc_idx].pitch_to_y_editing(n.pitch, true) + nh * 0.5;
                                                [world[0] - nx, world[1] - ny]
                                            }).collect();
                                            self.drag = DragState::MovingMidiNote {
                                                clip_id: mc_idx,
                                                note_indices: new_indices,
                                                offsets,
                                                start_world: world,
                                                before_notes,
                                            };
                                        } else {
                                            let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                            let nh = self.midi_clips[&mc_idx].note_height_editing(true);
                                            let offsets: Vec<[f32; 2]> = self.selected_midi_notes.iter().map(|&ni| {
                                                let n = &self.midi_clips[&mc_idx].notes[ni];
                                                let nx = mc_pos[0] + n.start_px;
                                                let ny = self.midi_clips[&mc_idx].pitch_to_y_editing(n.pitch, true) + nh * 0.5;
                                                [world[0] - nx, world[1] - ny]
                                            }).collect();
                                            self.drag = DragState::MovingMidiNote {
                                                clip_id: mc_idx,
                                                note_indices: self.selected_midi_notes.clone(),
                                                offsets,
                                                start_world: world,
                                                before_notes,
                                            };
                                        }
                                    }
                                    midi::MidiNoteHitZone::VelocityBar => unreachable!(),
                                }
                            } else {
                                self.selected_midi_notes.clear();
                                self.midi_note_select_rect = None;
                                self.drag = DragState::SelectingMidiNotes {
                                    clip_id: mc_idx,
                                    start_world: world,
                                };
                            }
                            self.mark_dirty();
                            self.request_redraw();
                            return;
                        }
                    }
                }

                // Click outside the editing group exits group edit mode
                if let Some(group_id) = self.editing_group {
                    if let Some(group) = self.groups.get(&group_id) {
                        let gx = group.position[0];
                        let gy = group.position[1];
                        let gw = group.size[0];
                        let gh = group.size[1];
                        if world[0] < gx || world[0] > gx + gw || world[1] < gy || world[1] > gy + gh {
                            self.editing_group = None;
                            self.selected.clear();
                            // Fall through to normal hit testing
                        }
                    } else {
                        self.editing_group = None;
                    }
                }

                // Click outside the editing component exits edit mode
                if let Some(ec_idx) = self.editing_component {
                    if let Some(def) = self.components.get(&ec_idx) {
                        if !point_in_rect(world, def.position, def.size) {
                            self.editing_component = None;
                            self.selected.clear();
                            println!("Exited component edit mode");
                            // Re-do hit test without edit mode
                            let (sw2, sh2, _) = self.screen_info();
                            let hit2 = hit_test(
                                &self.objects,
                                &self.waveforms,
                                &self.loop_regions,
                                &self.export_regions,
                                &self.components,
                                &self.component_instances,
                                &self.midi_clips,
                                &self.text_notes,
                                &self.groups,
                                None,
                                world,
                                &self.camera,
                                self.editing_group,
                                sw2,
                                sh2,
                            );
                            if let Some(raw_target) = hit2 {
                                let target = self.redirect_to_group(raw_target);
                                self.selected.push(target);
                                self.begin_move_selection(world, self.modifiers.alt_key(), Some(target));
                            } else {
                                self.drag = DragState::Selecting { start_world: world };
                            }
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }
                    }
                }

                match hit {
                    Some(raw_target) => {
                        // If clicking a disabled child take, switch to it instead of selecting
                        if let HitTarget::Waveform(wf_id) = raw_target {
                            if let Some(wf) = self.waveforms.get(&wf_id) {
                                if wf.disabled {
                                    if let Some(parent_id) = self.find_take_parent(wf_id) {
                                        if let Some(parent) = self.waveforms.get(&parent_id) {
                                            if let Some(tg) = &parent.take_group {
                                                if let Some(pos) = tg.take_ids.iter().position(|id| *id == wf_id) {
                                                    self.switch_active_take(parent_id, pos + 1);
                                                    self.selected.clear();
                                                    self.selected.push(HitTarget::Waveform(wf_id));
                                                    self.update_right_window();
                                                    self.update_cursor();
                                                    self.request_redraw();
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let target = self.redirect_to_group(raw_target);
                        self.select_area = None;
                        let shift = self.modifiers.shift_key();
                        if shift {
                            let is_selected = self.select_with_shift(target, true);
                            self.update_right_window();
                            if is_selected {
                                self.begin_move_selection(world, self.modifiers.alt_key(), Some(target));
                            }
                        } else {
                            if self.selected.contains(&target) {
                                // Already selected -> drag whole selection
                            } else {
                                self.selected.clear();
                                self.selected.push(target);
                                self.update_right_window();
                            }
                            self.begin_move_selection(world, self.modifiers.alt_key(), Some(target));
                        }
                    }
                    None => {
                        self.drag = DragState::Selecting { start_world: world };
                    }
                }

                self.update_cursor();
                self.request_redraw();
            }

            ElementState::Released => {
                // Finish right window knob drag
                {
                    let is_vol_drag = self.right_window.as_ref().map_or(false, |rw| rw.vol_dragging);
                    let is_pan_drag = self.right_window.as_ref().map_or(false, |rw| rw.pan_dragging);
                    let is_sbpm_drag = self.right_window.as_ref().map_or(false, |rw| rw.sample_bpm_dragging);
                    let is_pitch_drag = self.right_window.as_ref().map_or(false, |rw| rw.pitch_dragging);
                    let is_paulstretch_drag = self.right_window.as_ref().map_or(false, |rw| rw.paulstretch_factor_dragging);
                    if is_vol_drag || is_pan_drag || is_sbpm_drag || is_pitch_drag || is_paulstretch_drag {
                        if let Some(rw) = &mut self.right_window {
                            rw.vol_dragging = false;
                            rw.pan_dragging = false;
                            rw.sample_bpm_dragging = false;
                            rw.pitch_dragging = false;
                            rw.paulstretch_factor_dragging = false;
                        }
                        if let Some(rw) = &self.right_window {
                            let target = rw.target;
                            let drag_start_value = rw.drag_start_value;
                            let snapshots = rw.drag_start_snapshots.clone();
                            let multi_ids = rw.multi_target_ids.clone();
                            match target {
                                ui::right_window::RightWindowTarget::Waveform(_wf_id) => {
                                    if (is_vol_drag || is_pan_drag) && multi_ids.len() > 1 {
                                        // Batch undo for multi-selection vol/pan drag
                                        let mut ops = Vec::new();
                                        for (id, before) in &snapshots {
                                            if let Some(after) = self.waveforms.get(id).cloned() {
                                                ops.push(crate::operations::Operation::UpdateWaveform {
                                                    id: *id, before: before.clone(), after,
                                                });
                                            }
                                        }
                                        if !ops.is_empty() {
                                            self.push_op(crate::operations::Operation::Batch(ops));
                                        }
                                    } else if let Some(after) = self.waveforms.get(&_wf_id).cloned() {
                                        let mut before = after.clone();
                                        if is_vol_drag {
                                            before.volume = ui::palette::vol_fader_pos_to_gain(drag_start_value);
                                        } else if is_pan_drag {
                                            before.pan = drag_start_value;
                                        } else if is_sbpm_drag {
                                            before.sample_bpm = drag_start_value;
                                        } else if is_pitch_drag {
                                            before.pitch_semitones = drag_start_value;
                                        } else if is_paulstretch_drag {
                                            before.paulstretch_factor = drag_start_value;
                                        }
                                        self.push_op(crate::operations::Operation::UpdateWaveform {
                                            id: _wf_id,
                                            before,
                                            after,
                                        });
                                    }
                                }
                                ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                                    if let Some(inst) = self.instruments.get(&inst_id) {
                                        let mut before_snap = crate::instruments::InstrumentSnapshot {
                                            name: inst.name.clone(),
                                            plugin_id: inst.plugin_id.clone(),
                                            plugin_name: inst.plugin_name.clone(),
                                            plugin_path: inst.plugin_path.clone(),
                                            volume: inst.volume,
                                            pan: inst.pan,
                                            effect_chain_id: inst.effect_chain_id,
                                            disabled: inst.disabled,
                                        };
                                        let after_snap = before_snap.clone();
                                        if is_vol_drag {
                                            before_snap.volume = ui::palette::vol_fader_pos_to_gain(drag_start_value);
                                        } else if is_pan_drag {
                                            before_snap.pan = drag_start_value;
                                        }
                                        self.push_op(crate::operations::Operation::UpdateInstrument {
                                            id: inst_id,
                                            before: before_snap,
                                            after: after_snap,
                                        });
                                    }
                                }
                                ui::right_window::RightWindowTarget::Group(gid) => {
                                    if let Some(g) = self.groups.get(&gid) {
                                        let after = g.clone();
                                        let mut before = after.clone();
                                        if is_vol_drag {
                                            before.volume = ui::palette::vol_fader_pos_to_gain(drag_start_value);
                                        } else if is_pan_drag {
                                            before.pan = drag_start_value;
                                        }
                                        self.push_op(crate::operations::Operation::UpdateGroup {
                                            id: gid, before, after,
                                        });
                                    }
                                }
                                ui::right_window::RightWindowTarget::Master => {
                                    // MainLayer vol/pan — no undo for now
                                }
                                ui::right_window::RightWindowTarget::Monitor => {
                                    // Monitor vol/pan — no undo for now
                                    #[cfg(feature = "native")]
                                    self.sync_monitor_effects();
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // Finish settings slider drag
                #[cfg(feature = "native")]
                if let Some(sw) = &mut self.settings_window {
                    if sw.dragging_slider.is_some() {
                        sw.dragging_slider = None;
                        self.settings.save();
                        self.request_redraw();
                        return;
                    }
                }

                if let Some((before_bpm, _)) = self.dragging_bpm.take() {
                    let pre_round = self.bpm;
                    self.bpm = self.bpm.round();
                    let after = self.bpm;
                    if (pre_round - after).abs() > f32::EPSILON {
                        let scale = pre_round / after;
                        self.rescale_clip_positions(scale);
                        self.rescale_camera_for_bpm(scale);
                    }
                    self.resize_warped_clips();
                    // Re-resolve after rounding correction, then commit snapshots
                    let mut snaps = std::mem::take(&mut self.bpm_drag_overlap_snapshots);
                    let mut tsplits = std::mem::take(&mut self.bpm_drag_overlap_temp_splits);
                    self.resolve_all_waveform_overlaps_live(&mut snaps, &mut tsplits);
                    let mut ops = Vec::new();
                    if (before_bpm - after).abs() > f32::EPSILON {
                        ops.push(crate::operations::Operation::SetBpm { before: before_bpm, after });
                    }
                    for (id, original) in snaps {
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
                    for id in tsplits {
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
                    #[cfg(feature = "native")]
                    if let Some(engine) = &self.audio_engine {
                        engine.set_bpm(self.bpm);
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                if let Some(p) = &mut self.command_palette {
                    if p.fader_dragging {
                        p.fader_dragging = false;
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish automation point drag ---
                if matches!(self.drag, DragState::DraggingAutomationPoint { .. }) {
                    if let DragState::DraggingAutomationPoint { waveform_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.waveforms.get(&waveform_id) {
                            self.push_op(crate::operations::Operation::UpdateWaveform { id: waveform_id, before, after: after.clone() });
                        }
                        self.sync_audio_clips();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish browser resize ---
                if matches!(self.drag, DragState::ResizingBrowser) {
                    self.drag = DragState::None;
                    self.update_hover();
                    self.update_cursor();
                    self.request_redraw();
                    return;
                }

                // --- finish resizing component def ---
                if matches!(self.drag, DragState::ResizingComponentDef { .. }) {
                    if let DragState::ResizingComponentDef { comp_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.components.get(&comp_id) {
                            self.push_op(crate::operations::Operation::UpdateComponent { id: comp_id, before, after: after.clone() });
                        }
                        self.sync_audio_clips();
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish resizing effect region ---
                // InstrumentRegion resize drag removed — instruments are non-spatial now

                // --- finish MIDI note drag/resize ---
                // --- finish MIDI pitch range resize ---
                if let DragState::ResizingMidiPitchRange { clip_id, before, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let before = before.clone();
                    self.drag = DragState::None;
                    if let Some(after) = self.midi_clips.get(&clip_id) {
                        if after.pitch_range != before.pitch_range {
                            self.push_op(crate::operations::Operation::UpdateMidiClip {
                                id: clip_id, before, after: after.clone(),
                            });
                        }
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // --- finish MIDI clip edge resize ---
                if matches!(self.drag, DragState::ResizingMidiClipEdge { .. }) {
                    if let DragState::ResizingMidiClipEdge { clip_id, before, overlap_snapshots, overlap_temp_splits, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        let mut ops = Vec::new();
                        if let Some(after) = self.midi_clips.get(&clip_id) {
                            if (after.position[0] - before.position[0]).abs() > 0.01
                                || (after.size[0] - before.size[0]).abs() > 0.01
                            {
                                ops.push(crate::operations::Operation::UpdateMidiClip {
                                    id: clip_id, before, after: after.clone(),
                                });
                            }
                        }
                        for (id, original) in &overlap_snapshots {
                            if let Some(mc) = self.midi_clips.get(id) {
                                if mc.disabled {
                                    let _ = self.midi_clips.shift_remove(id);
                                    ops.push(crate::operations::Operation::DeleteMidiClip { id: *id, data: original.clone() });
                                } else {
                                    ops.push(crate::operations::Operation::UpdateMidiClip { id: *id, before: original.clone(), after: mc.clone() });
                                }
                            }
                        }
                        for id in &overlap_temp_splits {
                            if let Some(mc) = self.midi_clips.get(id) {
                                ops.push(crate::operations::Operation::CreateMidiClip { id: *id, data: mc.clone() });
                            }
                        }
                        if ops.len() == 1 {
                            self.push_op(ops.into_iter().next().unwrap());
                        } else if ops.len() > 1 {
                            self.push_op(crate::operations::Operation::Batch(ops));
                        }
                    }
                    self.sync_audio_clips();
                    self.mark_dirty();
                    self.update_cursor();
                    self.request_redraw();
                    return;
                }

                if matches!(self.drag, DragState::MovingMidiNote { .. } | DragState::ResizingMidiNote { .. } | DragState::ResizingMidiNoteLeft { .. } | DragState::ResizingMidiClip { .. }) {
                    self.broadcast_drag_end();
                    let old_drag = std::mem::replace(&mut self.drag, DragState::None);
                    if let Some(note_idx) = self.pending_midi_note_click.take() {
                        // No-op click — restore before state
                        let (clip_id, before_notes) = match &old_drag {
                            DragState::MovingMidiNote { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            DragState::ResizingMidiNote { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            DragState::ResizingMidiNoteLeft { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            _ => (None, None),
                        };
                        if let (Some(cid), Some(bn)) = (clip_id, before_notes) {
                            if let Some(mc) = self.midi_clips.get_mut(&cid) {
                                mc.notes = bn;
                            }
                        }
                        self.selected_midi_notes = vec![note_idx];
                    } else {
                        // Extract before_notes and emit op
                        let (clip_id, before_notes) = match &old_drag {
                            DragState::MovingMidiNote { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            DragState::ResizingMidiNote { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            DragState::ResizingMidiNoteLeft { clip_id, before_notes, .. } => (Some(*clip_id), Some(before_notes.clone())),
                            DragState::ResizingMidiClip { clip_id, before, .. } => {
                                if let Some(after) = self.midi_clips.get(clip_id) {
                                    self.push_op(crate::operations::Operation::UpdateMidiClip { id: *clip_id, before: before.clone(), after: after.clone() });
                                }
                                (None, None)
                            }
                            _ => (None, None),
                        };
                        if let (Some(cid), Some(bn)) = (clip_id, before_notes) {
                            // Resolve overlaps
                            let note_indices: Vec<usize> = match &old_drag {
                                DragState::MovingMidiNote { note_indices, .. } => note_indices.clone(),
                                DragState::ResizingMidiNote { note_indices, .. } => note_indices.clone(),
                                DragState::ResizingMidiNoteLeft { note_indices, .. } => note_indices.clone(),
                                _ => vec![],
                            };
                            if let Some(mc) = self.midi_clips.get_mut(&cid) {
                                if !note_indices.is_empty() {
                                    let new_indices = mc.resolve_note_overlaps(&note_indices);
                                    self.selected_midi_notes = new_indices;
                                }
                            }
                            if let Some(mc) = self.midi_clips.get(&cid) {
                                self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: cid, before: bn, after: mc.notes.clone() });
                            }
                        }
                    }
                    self.sync_audio_clips();
                    self.update_cursor();
                    self.request_redraw();
                    return;
                }

                // --- finish velocity drag ---
                if matches!(self.drag, DragState::DraggingVelocity { .. }) {
                    if let DragState::DraggingVelocity { clip_id, before_notes, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(mc) = self.midi_clips.get(&clip_id) {
                            self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id, before: before_notes, after: mc.notes.clone() });
                        }
                        self.sync_audio_clips();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish velocity lane resize ---
                if matches!(self.drag, DragState::ResizingVelocityLane { .. }) {
                    self.drag = DragState::None;
                    self.update_hover();
                    self.update_cursor();
                    self.request_redraw();
                    return;
                }

                // --- finish MIDI clip move ---
                if matches!(self.drag, DragState::MovingMidiClip { .. }) {
                    self.broadcast_drag_end();
                    if let DragState::MovingMidiClip { clip_id, before, overlap_snapshots, overlap_temp_splits, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        let mut ops = Vec::new();
                        if let Some(after) = self.midi_clips.get(&clip_id) {
                            ops.push(crate::operations::Operation::UpdateMidiClip { id: clip_id, before, after: after.clone() });
                        }
                        // Commit overlap changes
                        for (id, original) in &overlap_snapshots {
                            if let Some(mc) = self.midi_clips.get(id) {
                                if mc.disabled {
                                    if let Some(data) = self.midi_clips.shift_remove(id) {
                                        ops.push(crate::operations::Operation::DeleteMidiClip { id: *id, data: original.clone() });
                                    }
                                } else {
                                    ops.push(crate::operations::Operation::UpdateMidiClip { id: *id, before: original.clone(), after: mc.clone() });
                                }
                            }
                        }
                        for id in &overlap_temp_splits {
                            if let Some(mc) = self.midi_clips.get(id) {
                                ops.push(crate::operations::Operation::CreateMidiClip { id: *id, data: mc.clone() });
                            }
                        }
                        if ops.len() == 1 {
                            self.push_op(ops.into_iter().next().unwrap());
                        } else if ops.len() > 1 {
                            self.push_op(crate::operations::Operation::Batch(ops));
                        }
                        self.sync_audio_clips();
                        self.update_groups_containing(clip_id);
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish MIDI note selection drag ---
                if matches!(self.drag, DragState::SelectingMidiNotes { .. }) {
                    self.drag = DragState::None;
                    self.midi_note_select_rect = None;
                    self.update_cursor();
                    self.request_redraw();
                    return;
                }

                // --- finish resizing text note ---
                if matches!(self.drag, DragState::ResizingTextNote { .. }) {
                    if let DragState::ResizingTextNote { note_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.text_notes.get(&note_id) {
                            self.push_op(crate::operations::Operation::UpdateTextNote { id: note_id, before, after: after.clone() });
                        }
                        self.render_generation += 1;
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish resizing export region ---
                if matches!(self.drag, DragState::ResizingExportRegion { .. }) {
                    if let DragState::ResizingExportRegion { region_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.export_regions.get(&region_id) {
                            self.push_op(crate::operations::Operation::UpdateExportRegion { id: region_id, before, after: after.clone() });
                        }
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish resizing loop region ---
                if matches!(self.drag, DragState::ResizingLoopRegion { .. }) {
                    if let DragState::ResizingLoopRegion { region_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.loop_regions.get(&region_id) {
                            self.push_op(crate::operations::Operation::UpdateLoopRegion { id: region_id, before, after: after.clone() });
                        }
                        self.sync_loop_region();
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish fade handle drag ---
                if matches!(self.drag, DragState::DraggingFade { .. }) {
                    if let DragState::DraggingFade { waveform_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.waveforms.get(&waveform_id) {
                            self.push_op(crate::operations::Operation::UpdateWaveform { id: waveform_id, before, after: after.clone() });
                        }
                        self.sync_audio_clips();
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish fade curve drag ---
                if matches!(self.drag, DragState::DraggingFadeCurve { .. }) {
                    if let DragState::DraggingFadeCurve { waveform_id, before, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        if let Some(after) = self.waveforms.get(&waveform_id) {
                            self.push_op(crate::operations::Operation::UpdateWaveform { id: waveform_id, before, after: after.clone() });
                        }
                        self.sync_audio_clips();
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish effect slot drag (reorder or click-to-open) ---
                if let DragState::DraggingEffectSlot { chain_id, slot_idx, start_y } = self.drag {
                    let dy = (self.mouse_pos[1] - start_y).abs();
                    let (sw2, sh, scale) = self.screen_info();
                    if dy < 5.0 * scale {
                        // Minimal movement — treat as click to open plugin GUI
                        self.open_effect_chain_slot_gui(chain_id, slot_idx);
                    } else {
                        // Reorder: determine target index based on mouse position
                        let slot_count = self.effect_chains.get(&chain_id).map_or(0, |c| c.slots.len());
                        if let Some(rw) = &self.right_window {
                            if let Some(target_idx) = rw.hit_test_effect_slot(self.mouse_pos, slot_count, sw2, sh, scale) {
                                if target_idx != slot_idx {
                                    if let Some(chain) = self.effect_chains.get_mut(&chain_id) {
                                        let slot = chain.slots.remove(slot_idx);
                                        let insert_at = if target_idx > slot_idx { target_idx } else { target_idx };
                                        let insert_at = insert_at.min(chain.slots.len());
                                        chain.slots.insert(insert_at, slot);
                                        self.push_op(crate::operations::Operation::ReorderEffectSlot {
                                            chain_id, from_idx: slot_idx, to_idx: insert_at,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    self.drag = DragState::None;
                    self.sync_audio_clips();
                    self.request_redraw();
                    return;
                }

                // --- finish layer node reorder drag ---
                if let DragState::ReorderingLayerNode { drag_active, drop_target, source_group_before, entity_id, .. } = &self.drag {
                    let drag_active = *drag_active;
                    let drop_target = drop_target.clone();
                    let source_group_before = source_group_before.clone();
                    let entity_id = *entity_id;
                    self.drag = DragState::None;
                    self.sample_browser.layer_drop_indicator = None;

                    if drag_active {
                        if let Some(target) = drop_target {
                            // Snapshot destination group before-state (if dropping into a group)
                            let dest_group_before = if let crate::layers::DropTarget::InsideGroup { group_id, .. } = &target {
                                self.groups.get(group_id).map(|g| (*group_id, g.clone()))
                            } else {
                                None
                            };

                            // Execute the tree move
                            if crate::layers::execute_drop(&mut self.layer_tree, &target, entity_id) {
                                let mut ops = Vec::new();

                                // If removed from a source group, update the group's member_ids
                                if let Some((src_id, src_before)) = &source_group_before {
                                    if let Some(g) = self.groups.get_mut(src_id) {
                                        g.member_ids.retain(|mid| *mid != entity_id);
                                    }
                                    self.update_group_bounds(*src_id);
                                    if let Some(g) = self.groups.get(src_id) {
                                        ops.push(crate::operations::Operation::UpdateGroup {
                                            id: *src_id,
                                            before: src_before.clone(),
                                            after: g.clone(),
                                        });
                                    }
                                }

                                // If dropped into a destination group, update its member_ids
                                if let crate::layers::DropTarget::InsideGroup { group_id, child_index } = &target {
                                    if let Some(g) = self.groups.get_mut(group_id) {
                                        if !g.member_ids.contains(&entity_id) {
                                            let insert_at = (*child_index).min(g.member_ids.len());
                                            g.member_ids.insert(insert_at, entity_id);
                                        }
                                    }
                                    self.update_group_bounds(*group_id);
                                    if let Some((_, dest_before)) = &dest_group_before {
                                        if let Some(g) = self.groups.get(group_id) {
                                            ops.push(crate::operations::Operation::UpdateGroup {
                                                id: *group_id,
                                                before: dest_before.clone(),
                                                after: g.clone(),
                                            });
                                        }
                                    }
                                }

                                if !ops.is_empty() {
                                    if ops.len() == 1 {
                                        self.push_op(ops.remove(0));
                                    } else {
                                        self.push_op(crate::operations::Operation::Batch(ops));
                                    }
                                }

                                self.refresh_project_browser_entries();
                            }
                        }
                    }
                    self.request_redraw();
                    return;
                }

                // --- drop from browser to canvas ---
                // --- pending browser drag released = click → trigger preview ---
                if let DragState::PendingBrowserDrag { ref path, .. } = self.drag {
                    // Only load if not already previewing this file (avoid restarting on preview strip click)
                    let already_loaded = self.sample_browser.preview_path.as_ref() == Some(path)
                        && self.sample_browser.preview_audio.is_some();
                    if !already_loaded {
                        let path = path.clone();
                        self.load_browser_preview(&path);
                    }
                    self.drag = DragState::None;
                    self.update_hover();
                    self.request_redraw();
                    return;
                }

                // --- drop from browser to canvas ---
                if let DragState::DraggingFromBrowser { ref path, .. } = self.drag {
                    let (_, sh, scale) = self.screen_info();
                    let in_browser = self.sample_browser.visible
                        && self.sample_browser.contains(self.mouse_pos, sh, scale);
                    if !in_browser {
                        let path = path.clone();
                        self.drop_audio_from_browser(&path);
                    }
                    self.drag = DragState::None;
                    self.update_hover();
                    self.request_redraw();
                    return;
                }

                // --- drop plugin from browser to canvas/effect region ---
                if let DragState::DraggingPlugin {
                    ref plugin_id,
                    ref plugin_name,
                    is_instrument,
                } = self.drag
                {
                    let plugin_id = plugin_id.clone();
                    let plugin_name = plugin_name.clone();
                    let (_, sh, scale) = self.screen_info();
                    let in_browser = self.sample_browser.visible
                        && self.sample_browser.contains(self.mouse_pos, sh, scale);
                    if !in_browser {
                        if is_instrument {
                            self.add_instrument(&plugin_id, &plugin_name);
                        } else {
                            // Check if dropped on the right window panel → add to inspected target's chain
                            let rw_target = self.right_window.as_ref().map(|rw| rw.target);
                            if let Some(target) = rw_target {
                                let (sw, sh, scale) = self.screen_info();
                                let (pp, ps) = ui::right_window::RightWindow::panel_rect(sw, sh, scale);
                                if point_in_rect(self.mouse_pos, pp, ps) {
                                    match target {
                                        ui::right_window::RightWindowTarget::Waveform(id) =>
                                            self.add_plugin_to_waveform_chain(id, &plugin_id, &plugin_name),
                                        ui::right_window::RightWindowTarget::Instrument(id) =>
                                            self.add_plugin_to_instrument_chain(id, &plugin_id, &plugin_name),
                                        ui::right_window::RightWindowTarget::Group(id) =>
                                            self.add_plugin_to_group_chain(id, &plugin_id, &plugin_name),
                                        ui::right_window::RightWindowTarget::Master =>
                                            self.add_plugin_to_master_chain(&plugin_id, &plugin_name),
                                        ui::right_window::RightWindowTarget::Monitor =>
                                            self.add_plugin_to_monitor_chain(&plugin_id, &plugin_name),
                                    }
                                    self.drag = DragState::None;
                                    self.update_hover();
                                    self.request_redraw();
                                    return;
                                }
                            }
                            // Check if dropped on a waveform — add to its effect chain
                            let world = self.camera.screen_to_world(self.mouse_pos);
                            let mut target_wf: Option<EntityId> = None;
                            for (&wf_id, wf) in self.waveforms.iter().rev() {
                                if !wf.disabled && point_in_rect(world, wf.position, wf.size) {
                                    target_wf = Some(wf_id);
                                    break;
                                }
                            }
                            if let Some(wf_id) = target_wf {
                                self.add_plugin_to_waveform_chain(wf_id, &plugin_id, &plugin_name);
                            }
                        }
                    }
                    self.drag = DragState::None;
                    self.update_hover();
                    self.request_redraw();
                    return;
                }

                // --- finish resizing waveform ---
                if matches!(self.drag, DragState::ResizingWaveform { .. }) {
                    if let DragState::ResizingWaveform { waveform_id, before, overlap_snapshots, overlap_temp_splits, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
                        let mut ops = Vec::new();
                        if let Some(after) = self.waveforms.get(&waveform_id) {
                            ops.push(crate::operations::Operation::UpdateWaveform { id: waveform_id, before, after: after.clone() });
                        }
                        for (id, original) in overlap_snapshots {
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
                        for id in overlap_temp_splits {
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
                        self.update_groups_containing(waveform_id);
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                // --- finish moving selection ---
                if matches!(self.drag, DragState::MovingSelection { .. }) {
                    self.broadcast_drag_end();
                    if let DragState::MovingSelection { before_states, overlap_snapshots, overlap_temp_splits, .. } =
                        std::mem::replace(&mut self.drag, DragState::None)
                    {
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
                            (HitTarget::Group(id), EntityBeforeState::Group(before)) => {
                                if let Some(after) = self.groups.get(&id) {
                                    ops.push(crate::operations::Operation::UpdateGroup { id, before, after: after.clone() });
                                }
                            }
                            _ => {}
                        }
                    }
                    // Commit overlap changes from live resolution
                    for (id, original) in overlap_snapshots {
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
                    for id in overlap_temp_splits {
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
                        // Update group bounds for any moved members
                        let moved_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t {
                            HitTarget::Waveform(id) | HitTarget::MidiClip(id)
                            | HitTarget::TextNote(id) | HitTarget::Object(id) => Some(*id),
                            _ => None,
                        }).collect();
                        for id in moved_ids {
                            self.update_groups_containing(id);
                        }
                        self.update_hover();
                        self.update_cursor();
                        self.request_redraw();
                        return;
                    }
                }

                if let DragState::Selecting { start_world } = &self.drag {
                    let start = *start_world;
                    let current = self.camera.screen_to_world(self.mouse_pos);
                    let (rp, rs) = canonical_rect(start, current);

                    let min_sz = 5.0 / self.camera.zoom;
                    if rs[0] < min_sz && rs[1] < min_sz {
                        self.selected.clear();
                        let snapped_x = snap_to_grid(current[0], &self.settings, self.camera.zoom, self.bpm);
                        #[cfg(feature = "native")]
                        {
                            if let Some(engine) = &self.audio_engine {
                                if !engine.is_playing() {
                                    let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                    engine.seek_to_seconds(secs);
                                }
                            }
                            self.broadcast_playback_if_connected();
                        }
                        let h = self.clip_height();
                        let line_y = grid::snap_to_clip_row(current[1], self.bpm);
                        let line_w = 2.0 / self.camera.zoom;
                        self.select_area = Some(SelectArea {
                            position: [snapped_x, line_y],
                            size: [line_w, h],
                        });
                        self.mark_dirty();
                    } else {
                        let raw_targets = targets_in_rect(
                            &self.objects,
                            &self.waveforms,
                            &self.loop_regions,
                            &self.export_regions,
                            &self.components,
                            &self.component_instances,
                            &self.midi_clips,
                            &self.text_notes,
                            self.editing_component,
                            rp,
                            rs,
                        );
                        self.selected = self.normalize_group_selection(raw_targets);
                        self.include_paired_instruments();
                        self.select_area = Some(SelectArea { position: rp, size: rs });
                    }
                }

                self.drag = DragState::None;
                self.update_right_window();
                self.sync_audio_clips();
                self.update_hover();
                self.request_redraw();
            }
        },
        _ => {}
        }
    }

    /// Open file picker and start background export from the export window.
    pub(crate) fn start_export(&mut self) {
        let ew = match &self.export_window {
            Some(ew) if ew.state == ui::export_window::ExportState::Idle => ew,
            _ => return,
        };

        let group_id = ew.group_id;
        let format = ew.format;
        let ext = format.extension();
        let default_name = format!("{}.{}", ew.group_name, ext);

        #[cfg(feature = "native")]
        {
            let path = match rfd::FileDialog::new()
                .set_title("Export")
                .set_file_name(&default_name)
                .save_file()
            {
                Some(p) => p,
                None => return, // user cancelled
            };

            let export_result = if group_id == ui::right_window::MAIN_LAYER_ID {
                crate::export::start_export_main(self, path, format)
            } else {
                crate::export::start_export(self, group_id, path, format)
            };
            match export_result {
                Ok(rx) => {
                    if let Some(ew) = &mut self.export_window {
                        ew.state = ui::export_window::ExportState::Exporting;
                        ew.progress = 0.0;
                        ew.progress_rx = Some(rx);
                    }
                }
                Err(e) => {
                    self.export_window = None;
                    self.toast_manager.push(
                        format!("Export failed: {}", e),
                        crate::ui::toast::ToastKind::Error,
                    );
                }
            }
        }
    }
}
