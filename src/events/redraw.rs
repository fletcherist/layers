use super::*;

impl App {
    pub(crate) fn handle_redraw(&mut self) {
        self.toast_manager.tick();
        self.tooltip.tick();
        self.update_recording_waveform();
        self.poll_pending_audio_loads();
        self.poll_export_progress();

        // Keep redrawing while sample preview is playing (animates playhead)
        #[cfg(feature = "native")]
        if self.audio_engine.as_ref().map_or(false, |e| e.is_preview_playing()) {
            self.request_redraw();
        }
        if let Some(gpu) = &mut self.gpu {
            let w = gpu.config.width as f32;
            let h = gpu.config.height as f32;

            let sel_rect = if let DragState::Selecting { start_world } = &self.drag {
                Some((*start_world, self.camera.screen_to_world(self.mouse_pos)))
            } else {
                None
            };

            #[cfg(feature = "native")]
            let playhead_world_x = self
                .audio_engine
                .as_ref()
                .filter(|e| e.is_playing())
                .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32);
            #[cfg(not(feature = "native"))]
            let playhead_world_x: Option<f32> = None;

            let camera_moved = self.camera.position != self.last_rendered_camera_pos
                || self.camera.zoom != self.last_rendered_camera_zoom;
            let hover_changed = self.hovered != self.last_rendered_hovered;
            let sel_changed = self.selected.len() != self.last_rendered_selected_len;
            let gen_changed = self.render_generation != self.last_rendered_generation;
            let preview_playing = {
                #[cfg(feature = "native")]
                { self.audio_engine.as_ref().map_or(false, |e| e.is_preview_playing()) }
                #[cfg(not(feature = "native"))]
                { false }
            };
            let needs_rebuild = camera_moved
                || hover_changed
                || sel_changed
                || gen_changed
                || playhead_world_x.is_some()
                || sel_rect.is_some()
                || self.file_hovering
                || self.network.is_connected()
                || preview_playing;

            // Collect child take IDs whose parent has expanded=false
            let hidden_take_children: HashSet<EntityId> = self.waveforms.iter()
                .filter_map(|(_, wf)| {
                    wf.take_group.as_ref().and_then(|tg| {
                        if !tg.expanded { Some(tg.take_ids.iter().copied()) } else { None }
                    })
                })
                .flatten()
                .collect();

            if needs_rebuild {
                let selected_set: HashSet<HitTarget> =
                    self.selected.iter().copied().collect();
                let render_ctx = RenderContext {
                    camera: &self.camera,
                    screen_w: w,
                    screen_h: h,
                    objects: &self.objects,
                    waveforms: &self.waveforms,
                    hovered: self.hovered,
                    selected: &selected_set,
                    selection_rect: sel_rect,
                    select_area: self.select_area.as_ref(),
                    file_hovering: self.file_hovering,
                    playhead_world_x,
                    export_regions: &self.export_regions,
                    loop_regions: &self.loop_regions,
                    components: &self.components,
                    component_instances: &self.component_instances,
                    editing_component: self.editing_component,
                    settings: &self.settings,
                    fade_curve_hovered: self.fade_curve_hovered,
                    fade_curve_dragging: if let DragState::DraggingFadeCurve { waveform_id, is_fade_in, .. } = self.drag {
                        Some((waveform_id, is_fade_in))
                    } else {
                        None
                    },
                    mouse_world: self.camera.screen_to_world(self.mouse_pos),
                    bpm: self.bpm,
                    automation_mode: self.automation_mode,
                    active_automation_param: self.active_automation_param,
                    editing_midi_clip: self.editing_midi_clip,
                    instruments: &self.instruments,
                    text_notes: &self.text_notes,
                    midi_clips: &self.midi_clips,
                    selected_midi_notes: &self.selected_midi_notes,
                    midi_note_select_rect: self.midi_note_select_rect,
                    groups: &self.groups,
                    remote_users: &self.remote_users,
                    network_mode: self.network.mode(),
                    hidden_take_children: &hidden_take_children,
                    solo_ids: &self.solo_ids,
                    following_user: self.following_user,
                };
                build_instances(&mut self.cached_instances, &render_ctx);
                build_waveform_vertices(&mut self.cached_wf_verts, &render_ctx);

                self.last_rendered_generation = self.render_generation;
                self.last_rendered_camera_pos = self.camera.position;
                self.last_rendered_camera_zoom = self.camera.zoom;
                self.last_rendered_hovered = self.hovered;
                self.last_rendered_selected_len = self.selected.len();
            }

            if self.sample_browser.visible {
                self.sample_browser.get_text_entries(&self.settings.theme, h, gpu.scale_factor);
            }

            // Build preview waveform vertices (screen-space, every frame when playing)
            self.cached_preview_wf_verts.clear();
            if let Some(ref audio) = self.sample_browser.preview_audio {
                if self.sample_browser.visible {
                    let [wf_x, wf_y, wf_w, wf_h] = self.sample_browser.preview_waveform_rect(h, gpu.scale_factor);
                    let progress = {
                        #[cfg(feature = "native")]
                        { self.audio_engine.as_ref().map_or(0.0, |e| e.preview_position() as f32) }
                        #[cfg(not(feature = "native"))]
                        { 0.0f32 }
                    };
                    let accent = self.settings.theme.accent;
                    let color = [accent[0], accent[1], accent[2], 0.8];
                    self.cached_preview_wf_verts = ui::waveform::build_preview_waveform_triangles(
                        &audio.left_peaks,
                        wf_x, wf_y, wf_w, wf_h,
                        color,
                        progress,
                    );
                }
            }

            let browser_ref = Some(&self.sample_browser);

            let drag_ghost =
                if let DragState::DraggingFromBrowser { ref filename, .. } = self.drag {
                    Some((filename.as_str(), self.mouse_pos))
                } else if let DragState::DraggingPlugin {
                    ref plugin_name, ..
                } = self.drag
                {
                    Some((plugin_name.as_str(), self.mouse_pos))
                } else {
                    None
                };

            let effect_chain_drag = if let DragState::DraggingEffectSlot { chain_id, slot_idx, start_y } = self.drag {
                let offset_y = self.mouse_pos[1] - start_y;
                let hover = if let Some(rw) = &self.right_window {
                    let slot_count = self.effect_chains.get(&chain_id).map_or(0, |c| c.slots.len());
                    rw.hit_test_effect_slot(self.mouse_pos, slot_count, w, h, gpu.scale_factor)
                } else {
                    None
                };
                Some((chain_id, slot_idx, offset_y, hover))
            } else {
                None
            };

            if let Some(p) = &mut self.command_palette {
                if p.mode == PaletteMode::VolumeFader {
                    #[cfg(feature = "native")]
                    { p.fader_rms = self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak()); }
                }
            }

            // Poll per-entity RMS for right window meter
            #[cfg(feature = "native")]
            if let Some(rw) = &mut self.right_window {
                let raw_rms = match rw.target {
                    crate::ui::right_window::RightWindowTarget::Master => {
                        self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak())
                    }
                    crate::ui::right_window::RightWindowTarget::Waveform(id)
                    | crate::ui::right_window::RightWindowTarget::Instrument(id)
                    | crate::ui::right_window::RightWindowTarget::Group(id) => {
                        self.audio_engine.as_ref().map_or(0.0, |e| e.entity_rms(id))
                    }
                };
                rw.update_rms(raw_rms);
                if rw.meter_rms > 0.001 {
                    gpu.window.request_redraw();
                }
            }

            #[cfg(feature = "native")]
            let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
            #[cfg(not(feature = "native"))]
            let is_playing = false;

            #[cfg(feature = "native")]
            let playback_pos = self
                .audio_engine
                .as_ref()
                .map_or(0.0, |e| e.position_seconds());
            #[cfg(not(feature = "native"))]
            let playback_pos = 0.0;

            #[cfg(feature = "native")]
            let is_recording = self.recorder.as_ref().map_or(false, |r| r.is_recording());
            #[cfg(not(feature = "native"))]
            let is_recording = false;

            let computer_keyboard_armed = self.computer_keyboard_armed;

            let selected_entity_ids: std::collections::HashSet<crate::entity_id::EntityId> = self.selected.iter()
                .filter_map(|t| match t {
                    HitTarget::Waveform(id) |
                    HitTarget::MidiClip(id) | HitTarget::TextNote(id) |
                    HitTarget::Group(id) | HitTarget::Instrument(id) => Some(*id),
                    _ => None,
                })
                .collect();

            gpu.render(
                &self.camera,
                &self.cached_instances,
                &self.cached_wf_verts,
                &self.cached_preview_wf_verts,
                self.command_palette.as_ref(),
                self.context_menu.as_ref(),
                browser_ref,
                drag_ghost,
                is_playing,
                is_recording,
                computer_keyboard_armed,
                playback_pos,
                &self.export_regions,
                &self.loop_regions,
                None,
                &self.waveforms,
                self.editing_waveform_name
                    .as_ref()
                    .map(|(idx, s)| (*idx, s.as_str())),
                self.plugin_editor.as_ref(),
                self.export_window.as_ref(),
                {
                    #[cfg(feature = "native")]
                    { self.settings_window.as_ref() }
                    #[cfg(not(feature = "native"))]
                    { Option::<&ui::settings_window::SettingsWindow>::None }
                },
                &self.settings,
                &self.toast_manager,
                &self.tooltip,
                self.bpm,
                self.editing_bpm.input.as_deref(),
                self.automation_mode,
                self.active_automation_param,
                &self.midi_clips,
                match self.hovered {
                    Some(HitTarget::MidiClip(i)) => Some(i),
                    _ => None,
                },
                self.editing_midi_clip,
                self.camera.screen_to_world(self.mouse_pos),
                match &self.drag {
                    DragState::DraggingVelocity { clip_id, note_indices, .. } => {
                        note_indices.first().map(|&ni| (*clip_id, ni))
                    }
                    _ => self.cmd_velocity_hover_note,
                },
                self.remote_storage.is_some(),
                self.right_window.as_ref(),
                {
                    if let Some(rw) = &self.right_window {
                        let chain_id = match rw.target {
                            crate::ui::right_window::RightWindowTarget::Waveform(wf_id) => {
                                self.waveforms.get(&wf_id).and_then(|w| w.effect_chain_id)
                            }
                            crate::ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                                self.instruments.get(&inst_id).and_then(|i| i.effect_chain_id)
                            }
                            crate::ui::right_window::RightWindowTarget::Group(group_id) => {
                                self.groups.get(&group_id).and_then(|g| g.effect_chain_id)
                            }
                            crate::ui::right_window::RightWindowTarget::Master => {
                                self.master.effect_chain_id
                            }
                        };
                        if let Some(cid) = chain_id {
                            self.effect_chains.get(&cid).map(|c| {
                                let ref_count = crate::ui::right_window::RightWindow::chain_ref_count_all(cid, &self.waveforms, &self.instruments, &self.groups);
                                (c, cid, ref_count)
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                },
                effect_chain_drag,
                self.input_monitoring,
                &self.text_notes,
                self.editing_text_note.as_ref().map(|e| (e.note_id, e.cursor)),
                &selected_entity_ids,
                &self.groups,
                &hidden_take_children,
            );
        }
        if self.toast_manager.has_active() {
            self.request_redraw();
        }
        if self.tooltip.is_pending() {
            self.request_redraw();
        }
    }

    fn poll_export_progress(&mut self) {
        if let Some(ew) = &mut self.export_window {
            if let Some(result) = ew.poll_progress() {
                match result {
                    Ok(()) => {
                        self.toast_manager.push(
                            "Export complete".to_string(),
                            crate::ui::toast::ToastKind::Success,
                        );
                    }
                    Err(e) => {
                        self.toast_manager.push(
                            format!("Export failed: {}", e),
                            crate::ui::toast::ToastKind::Error,
                        );
                    }
                }
                self.export_window = None;
                self.request_redraw();
            } else if ew.state == crate::ui::export_window::ExportState::Exporting {
                // Keep redrawing while export is in progress
                self.request_redraw();
            }
        }
    }
}
