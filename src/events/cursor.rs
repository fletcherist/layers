use super::*;

impl App {
    pub(crate) fn handle_cursor_moved(&mut self) {
        // Broadcast cursor position to network
        self.broadcast_cursor_if_connected();

        // Plugin editor: slider drag
        {
            let is_dragging_pe = self
                .plugin_editor
                .as_ref()
                .map_or(false, |pe| pe.dragging_slider.is_some());
            if is_dragging_pe {
                let (scr_w, scr_h, scale) = self.screen_info();
                let mx = self.mouse_pos[0];
                if let Some(pe) = &mut self.plugin_editor {
                    let idx = pe.dragging_slider.unwrap();
                    let _new_val = pe.slider_drag(idx, mx, scr_w, scr_h, scale);
                    #[cfg(feature = "native")]
                    {
                        let pb_idx = pe.region_id; // now repurposed as plugin_block index
                        if let Some(pb) = self.plugin_blocks.get(&pb_idx) {
                            if let Ok(guard) = pb.gui.lock() {
                                if let Some(gui) = guard.as_ref() {
                                    gui.set_parameter(idx, _new_val as f64);
                                }
                            }
                        }
                    }
                }
                self.request_redraw();
                return;
            }
        }

        // Settings window: slider drag + hover
        #[cfg(feature = "native")]
        {
            let is_dragging_settings = self
                .settings_window
                .as_ref()
                .map_or(false, |sw| sw.dragging_slider.is_some());
            if is_dragging_settings {
                let (scr_w, scr_h, scale) = self.screen_info();
                let mx = self.mouse_pos[0];
                if let Some(sw) = &self.settings_window {
                    let idx = sw.dragging_slider.unwrap();
                    sw.slider_drag(idx, mx, &mut self.settings, scr_w, scr_h, scale);
                }
                self.mark_dirty();
                self.request_redraw();
                return;
            }
            if self.settings_window.is_some() {
                let (scr_w, scr_h, scale) = self.screen_info();
                let pos = self.mouse_pos;
                if let Some(sw) = &mut self.settings_window {
                    sw.update_hover(pos, scr_w, scr_h, scale);
                }
                self.request_redraw();
                return;
            }
        }

        if let Some((initial_bpm, initial_y)) = self.dragging_bpm {
            let dy = initial_y - self.mouse_pos[1];
            let new_bpm = (initial_bpm + dy * 0.5).clamp(20.0, 999.0);
            if (self.bpm - new_bpm).abs() > f32::EPSILON {
                let scale = self.bpm / new_bpm;
                self.rescale_clip_positions(scale);
                self.rescale_camera_for_bpm(scale);
            }
            self.bpm = new_bpm;
            self.resize_warped_clips();
            let mut snaps = std::mem::take(&mut self.bpm_drag_overlap_snapshots);
            let mut tsplits = std::mem::take(&mut self.bpm_drag_overlap_temp_splits);
            self.resolve_all_waveform_overlaps_live(&mut snaps, &mut tsplits);
            self.bpm_drag_overlap_snapshots = snaps;
            self.bpm_drag_overlap_temp_splits = tsplits;
            self.sync_audio_clips();
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        if self.context_menu.is_some() {
            let (sw, sh, scale) = self.screen_info();
            if let Some(cm) = self.context_menu.as_mut() {
                cm.update_hover(self.mouse_pos, sw, sh, scale);
            }
            self.request_redraw();
            return;
        }

        // Right window knob drag
        {
            let (_sw, _sh, scale) = self.screen_info();
            let is_vol_drag = self.right_window.as_ref().map_or(false, |rw| rw.vol_dragging);
            let is_pan_drag = self.right_window.as_ref().map_or(false, |rw| rw.pan_dragging);
            let is_sbpm_drag = self.right_window.as_ref().map_or(false, |rw| rw.sample_bpm_dragging);
            let is_pitch_drag = self.right_window.as_ref().map_or(false, |rw| rw.pitch_dragging);
            if is_vol_drag || is_pan_drag || is_sbpm_drag || is_pitch_drag {
                let (before, wf_id) = if let Some(rw) = &self.right_window {
                    let wf_id = rw.waveform_id;
                    let before = self.waveforms.get(&wf_id).cloned();
                    (before, wf_id)
                } else {
                    (None, crate::entity_id::EntityId::default())
                };
                if let (Some(before_wf), Some(rw)) = (before, self.right_window.as_mut()) {
                    if is_vol_drag {
                        let new_vol = ui::right_window::RightWindow::drag_vol_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.volume = new_vol;
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.volume = new_vol;
                        }
                    } else if is_pan_drag {
                        let new_pan = ui::right_window::RightWindow::drag_pan_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.pan = new_pan;
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.pan = new_pan;
                        }
                    } else if is_sbpm_drag {
                        let new_bpm = ui::right_window::RightWindow::drag_sample_bpm_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.sample_bpm = new_bpm;
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.sample_bpm = new_bpm;
                            if wf.warp_mode == ui::waveform::WarpMode::RePitch {
                                if let Some(clip) = self.audio_clips.get(&wf_id) {
                                    let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                    wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                                }
                            }
                        }
                    } else {
                        let new_pitch = ui::right_window::RightWindow::drag_pitch_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.pitch_semitones = new_pitch;
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.pitch_semitones = new_pitch;
                        }
                        self.resize_warped_clips();
                    }
                    let _ = before_wf;
                }
                self.mark_dirty();
                #[cfg(feature = "native")]
                self.sync_audio_clips();
                self.request_redraw();
                return;
            }
        }

        {
            let is_dragging_fader = self
                .command_palette
                .as_ref()
                .map_or(false, |p| p.fader_dragging);
            if is_dragging_fader {
                let (sw, sh, scale) = self.screen_info();
                if let Some(p) = &mut self.command_palette {
                    let mx = self.mouse_pos[0];
                    p.fader_drag(mx, sw, sh, scale);
                    #[cfg(feature = "native")]
                    if let Some(engine) = &self.audio_engine {
                        engine.set_master_volume(p.fader_value);
                    }
                }
                self.request_redraw();
                return;
            }
        }

        // Update browser hover state
        if self.sample_browser.visible && !matches!(self.drag, DragState::ResizingBrowser) {
            let (_, sh, scale) = self.screen_info();
            if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                self.sample_browser.update_hover(self.mouse_pos, sh, scale);
            } else {
                self.sample_browser.hovered_entry = None;
                self.sample_browser.add_button_hovered = false;
                self.sample_browser.resize_hovered = false;
            }
            self.update_cursor();
        }

        // If resizing browser panel, update width
        if matches!(self.drag, DragState::ResizingBrowser) {
            let (_, _, scale) = self.screen_info();
            self.sample_browser
                .set_width_from_screen(self.mouse_pos[0], scale);
            self.request_redraw();
            return;
        }

        // If dragging from browser or plugin, just request redraw for ghost
        if matches!(
            self.drag,
            DragState::DraggingFromBrowser { .. } | DragState::DraggingPlugin { .. }
        ) {
            self.request_redraw();
            return;
        }

        // Resizing component def
        if let DragState::ResizingComponentDef { comp_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(comp) = self.components.get_mut(&comp_id) {
                comp.position = pos;
                comp.size = size;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing export region
        if let DragState::ResizingExportRegion { region_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(er) = self.export_regions.get_mut(&region_id) {
                er.position = pos;
                er.size = size;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing effect region
        if let DragState::ResizingEffectRegion { region_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(er) = self.effect_regions.get_mut(&region_id) {
                er.position = pos;
                er.size = size;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing instrument region
        if let DragState::ResizingInstrumentRegion { region_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(ir) = self.instrument_regions.get_mut(&region_id) {
                ir.position = pos;
                ir.size = size;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing MIDI clip
        if let DragState::ResizingMidiClip { clip_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                mc.position = pos;
                mc.size = size;
                // Auto-extend any overlapping instrument region
                let padding = instruments::INSTRUMENT_REGION_PADDING;
                for ir in self.instrument_regions.values_mut() {
                    if rects_overlap(ir.position, ir.size, pos, size) {
                        instruments::ensure_region_contains_clip(ir, pos, size, padding);
                    }
                }
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing loop region
        if let DragState::ResizingLoopRegion { region_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(lr) = self.loop_regions.get_mut(&region_id) {
                lr.position = pos;
                lr.size = size;
            }
            self.sync_loop_region();
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing waveform edge
        if let DragState::ResizingWaveform {
            waveform_id,
            is_left_edge,
            initial_position_x,
            initial_size_w,
            initial_offset_px,
            ..
        } = self.drag
        {
            let world = self.camera.screen_to_world(self.mouse_pos);
            if let Some(wf) = self.waveforms.get(&waveform_id) {
                let full_w = full_audio_width_px(wf);
                let min_w = if self.settings.grid_enabled && self.settings.snap_to_grid {
                    grid_spacing_for_settings(&self.settings, self.camera.zoom, self.bpm)
                } else {
                    WAVEFORM_MIN_WIDTH_PX
                };

                if is_left_edge {
                    let snapped_x = if self.is_snap_override_active() {
                        world[0]
                    } else {
                        snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm)
                    };
                    let dx = snapped_x - initial_position_x;
                    let mut new_offset = initial_offset_px + dx;
                    let mut new_size_w = initial_size_w - dx;
                    let mut new_pos_x = snapped_x;

                    if new_offset < 0.0 {
                        new_offset = 0.0;
                        new_size_w = initial_size_w + initial_offset_px;
                        new_pos_x = initial_position_x - initial_offset_px;
                    }
                    if new_size_w < min_w {
                        new_size_w = min_w;
                        new_offset = initial_offset_px + initial_size_w - min_w;
                        new_pos_x = initial_position_x + initial_size_w - min_w;
                    }
                    if new_offset + new_size_w > full_w {
                        new_size_w = full_w - new_offset;
                    }

                    let wf = self.waveforms.get_mut(&waveform_id).unwrap();
                    wf.position[0] = new_pos_x;
                    wf.size[0] = new_size_w;
                    wf.sample_offset_px = new_offset;
                    wf.fade_in_px = wf.fade_in_px.min(new_size_w * 0.5);
                    wf.fade_out_px = wf.fade_out_px.min(new_size_w * 0.5);
                } else {
                    let snapped_right = if self.is_snap_override_active() {
                        world[0]
                    } else {
                        snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm)
                    };
                    let wf = self.waveforms.get(&waveform_id).unwrap();
                    let mut new_size_w = snapped_right - wf.position[0];
                    let cur_offset = wf.sample_offset_px;

                    if new_size_w < min_w {
                        new_size_w = min_w;
                    }
                    if cur_offset + new_size_w > full_w {
                        new_size_w = full_w - cur_offset;
                    }

                    let wf = self.waveforms.get_mut(&waveform_id).unwrap();
                    wf.size[0] = new_size_w;
                    wf.fade_in_px = wf.fade_in_px.min(new_size_w * 0.5);
                    wf.fade_out_px = wf.fade_out_px.min(new_size_w * 0.5);
                }
            }
            // Live waveform overlap resolution during resize
            let (mut snaps, mut tsplits) = if let DragState::ResizingWaveform { ref mut overlap_snapshots, ref mut overlap_temp_splits, .. } = self.drag {
                (std::mem::take(overlap_snapshots), std::mem::take(overlap_temp_splits))
            } else {
                (indexmap::IndexMap::new(), Vec::new())
            };
            self.resolve_waveform_overlaps_live(&[waveform_id], &mut snaps, &mut tsplits);
            if let DragState::ResizingWaveform { ref mut overlap_snapshots, ref mut overlap_temp_splits, .. } = self.drag {
                *overlap_snapshots = snaps;
                *overlap_temp_splits = tsplits;
            }
            self.sync_audio_clips();
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Dragging automation point
        if let DragState::DraggingAutomationPoint {
            waveform_id,
            param,
            point_idx,
            ..
        } = self.drag
        {
            let world = self.camera.screen_to_world(self.mouse_pos);
            if let Some(wf) = self.waveforms.get_mut(&waveform_id) {
                let t = ((world[0] - wf.position[0]) / wf.size[0]).clamp(0.0, 1.0);
                let y_top = wf.position[1];
                let y_bot = wf.position[1] + wf.size[1];
                let value = ((world[1] - y_bot) / (y_top - y_bot)).clamp(0.0, 1.0);

                // Clamp t between neighbor points to maintain sort order
                let lane = wf.automation.lane_for_mut(param);
                let t_min = if point_idx > 0 {
                    lane.points[point_idx - 1].t + 0.001
                } else {
                    0.0
                };
                let t_max = if point_idx + 1 < lane.points.len() {
                    lane.points[point_idx + 1].t - 0.001
                } else {
                    1.0
                };
                let t = t.clamp(t_min, t_max);
                lane.points[point_idx].t = t;
                lane.points[point_idx].value = value;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Dragging fade handle
        if let DragState::DraggingFade {
            waveform_id,
            is_fade_in,
            ..
        } = self.drag
        {
            let world = self.camera.screen_to_world(self.mouse_pos);
            if let Some(wf) = self.waveforms.get_mut(&waveform_id) {
                let max_fade = wf.size[0] * 0.5;
                if is_fade_in {
                    let new_val = (world[0] - wf.position[0]).clamp(0.0, max_fade);
                    wf.fade_in_px = new_val;
                } else {
                    let new_val =
                        (wf.position[0] + wf.size[0] - world[0]).clamp(0.0, max_fade);
                    wf.fade_out_px = new_val;
                }
            }
            self.mark_dirty();
            self.sync_audio_clips();
            self.request_redraw();
            return;
        }

        // Dragging fade curve shape
        if let DragState::DraggingFadeCurve {
            waveform_id,
            is_fade_in,
            start_mouse_y,
            start_curve,
            ..
        } = self.drag
        {
            let dy = self.mouse_pos[1] - start_mouse_y;
            let sensitivity = 0.005;
            let new_curve = (start_curve - dy * sensitivity).clamp(-1.0, 1.0);
            if let Some(wf) = self.waveforms.get_mut(&waveform_id) {
                if is_fade_in {
                    wf.fade_in_curve = new_curve;
                } else {
                    wf.fade_out_curve = new_curve;
                }
            }
            self.mark_dirty();
            self.sync_audio_clips();
            self.request_redraw();
            return;
        }

        enum Action {
            Pan([f32; 2], [f32; 2]),
            MoveSelection(Vec<(HitTarget, [f32; 2])>, usize),
            Other,
        }
        let action = match &self.drag {
            DragState::Panning {
                start_mouse,
                start_camera,
            } => Action::Pan(*start_mouse, *start_camera),
            DragState::MovingSelection { offsets, anchor_idx, .. } => {
                Action::MoveSelection(offsets.clone(), *anchor_idx)
            }
            _ => Action::Other,
        };

        match action {
            Action::Pan(sm, sc) => {
                self.camera.position[0] =
                    sc[0] - (self.mouse_pos[0] - sm[0]) / self.camera.zoom;
                self.camera.position[1] =
                    sc[1] - (self.mouse_pos[1] - sm[1]) / self.camera.zoom;
            }
            Action::MoveSelection(offsets, anchor_idx) => {
                let world = self.camera.screen_to_world(self.mouse_pos);
                // Snap only the anchor clip, then apply the same snap delta to all clips
                let anchor_offset = &offsets[anchor_idx].1;
                let raw_anchor_x = world[0] - anchor_offset[0];
                let raw_anchor_y = world[1] - anchor_offset[1];
                let snap_delta_x = if self.is_snap_override_active() {
                    0.0
                } else {
                    snap_to_grid(raw_anchor_x, &self.settings, self.camera.zoom, self.bpm) - raw_anchor_x
                };
                let snap_delta_y = if self.is_snap_override_active() {
                    0.0
                } else {
                    snap_to_vertical_grid(raw_anchor_y, &self.settings, self.camera.zoom, self.bpm) - raw_anchor_y
                };
                let mut needs_sync = false;
                for (target, offset) in &offsets {
                    let final_x = (world[0] - offset[0]) + snap_delta_x;
                    let final_y = (world[1] - offset[1]) + snap_delta_y;
                    self.set_target_pos(target, [final_x, final_y]);
                    if matches!(
                        target,
                        HitTarget::Waveform(_)
                            | HitTarget::EffectRegion(_)
                            | HitTarget::LoopRegion(_)
                            | HitTarget::ExportRegion(_)
                            | HitTarget::ComponentDef(_)
                            | HitTarget::ComponentInstance(_)
                            | HitTarget::MidiClip(_)
                            | HitTarget::InstrumentRegion(_)
                    ) {
                        needs_sync = true;
                    }
                }
                // Live waveform overlap resolution during drag
                let moved_wf_ids: Vec<crate::entity_id::EntityId> = offsets.iter()
                    .filter_map(|(t, _)| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
                    .collect();
                if !moved_wf_ids.is_empty() {
                    let (mut snaps, mut tsplits) = if let DragState::MovingSelection { ref mut overlap_snapshots, ref mut overlap_temp_splits, .. } = self.drag {
                        (std::mem::take(overlap_snapshots), std::mem::take(overlap_temp_splits))
                    } else {
                        (indexmap::IndexMap::new(), Vec::new())
                    };
                    self.resolve_waveform_overlaps_live(&moved_wf_ids, &mut snaps, &mut tsplits);
                    if let DragState::MovingSelection { ref mut overlap_snapshots, ref mut overlap_temp_splits, .. } = self.drag {
                        *overlap_snapshots = snaps;
                        *overlap_temp_splits = tsplits;
                    }
                    needs_sync = true;
                }
                // Auto-extend instrument regions for moved MIDI clips
                let padding = instruments::INSTRUMENT_REGION_PADDING;
                for (target, _) in &offsets {
                    if let HitTarget::MidiClip(ci) = target {
                        if let Some(mc) = self.midi_clips.get(ci) {
                            let cp = mc.position;
                            let cs = mc.size;
                            for ir in self.instrument_regions.values_mut() {
                                if rects_overlap(ir.position, ir.size, cp, cs) {
                                    instruments::ensure_region_contains_clip(ir, cp, cs, padding);
                                }
                            }
                        }
                    }
                }
                if let Some(ec_idx) = self.editing_component {
                    self.update_component_bounds(ec_idx);
                }
                if needs_sync {
                    self.sync_audio_clips();
                    self.sync_loop_region();
                }
                // Broadcast drag preview to remote users
                let preview_targets: Vec<_> = offsets.iter().map(|(t, _)| {
                    let pos = self.get_target_pos(t);
                    let size = self.get_target_size(t);
                    (t.clone(), pos, size)
                }).collect();
                self.broadcast_drag_preview(crate::user::DragPreview::MovingEntities {
                    targets: preview_targets,
                });
                self.mark_dirty();
            }
            Action::Other => {
                let world = self.camera.screen_to_world(self.mouse_pos);
                if let DragState::Selecting { start_world } = &self.drag {
                    let start = *start_world;
                    let current = world;
                    let (rp, rs) = canonical_rect(start, current);
                    let min_sz = 5.0 / self.camera.zoom;
                    if rs[0] >= min_sz || rs[1] >= min_sz {
                        self.selected = targets_in_rect(
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
                            rp,
                            rs,
                        );
                        self.update_right_window();
                    }
                }
                if let DragState::MovingMidiNote { clip_id, note_indices, offsets, start_world, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let note_indices = note_indices.clone();
                    let offsets = offsets.clone();
                    let sw = *start_world;
                    if self.midi_clips.contains_key(&clip_id) {
                        let drag_threshold = 3.0 / self.camera.zoom;
                        let dx = world[0] - sw[0];
                        let dy = world[1] - sw[1];
                        let below_threshold = self.pending_midi_note_click.is_some()
                            && (dx * dx + dy * dy) < drag_threshold * drag_threshold;
                        if !below_threshold {
                            self.pending_midi_note_click = None;
                            let mc = &self.midi_clips[&clip_id];
                            let mc_pos = mc.position;
                            let mc_pr = mc.pitch_range;
                            let editing = self.editing_midi_clip == Some(clip_id);
                            let area_h = mc.note_area_height(editing);
                            let first_raw_x = world[0] - offsets[0][0];
                            let mc_gm = mc.grid_mode;
                            let mc_trip = mc.triplet_grid;
                            let snap_delta = if self.is_snap_override_active() {
                                0.0
                            } else {
                                snap_to_clip_grid(first_raw_x, &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm) - first_raw_x
                            };
                            let mc = self.midi_clips.get_mut(&clip_id).unwrap();
                            for (i, &ni) in note_indices.iter().enumerate() {
                                if ni < mc.notes.len() {
                                    let raw_x = world[0] - offsets[i][0];
                                    let ny = world[1] - offsets[i][1];
                                    let start_px = (raw_x + snap_delta - mc_pos[0]).max(0.0);
                                    let nh = area_h / (mc_pr.1 - mc_pr.0) as f32;
                                    let relative = mc_pos[1] + area_h - ny;
                                    let pitch = ((relative / nh) as u8 + mc_pr.0).clamp(mc_pr.0, mc_pr.1 - 1);
                                    mc.notes[ni].start_px = start_px;
                                    mc.notes[ni].pitch = pitch;
                                }
                            }
                            // Broadcast clip as drag preview so remote sees note-editing activity
                            let mc = &self.midi_clips[&clip_id];
                            self.broadcast_drag_preview(crate::user::DragPreview::MovingEntities {
                                targets: vec![(HitTarget::MidiClip(clip_id), mc.position, mc.size)],
                            });
                            self.mark_dirty();
                        }
                    }
                }
                if let DragState::ResizingMidiNote { clip_id, anchor_idx, note_indices, original_durations, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let anchor_idx = *anchor_idx;
                    let indices = note_indices.clone();
                    let orig_durs = original_durations.clone();
                    if let Some(mc) = self.midi_clips.get(&clip_id) {
                        if anchor_idx < mc.notes.len() {
                        let mc_gm = mc.grid_mode;
                        let mc_trip = mc.triplet_grid;
                        let snapped_edge = if self.is_snap_override_active() {
                            world[0]
                        } else {
                            snap_to_clip_grid(world[0], &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm)
                        };
                        let anchor_x = mc.position[0] + mc.notes[anchor_idx].start_px;
                        let anchor_new_dur = (snapped_edge - anchor_x).max(10.0);
                        let mc = self.midi_clips.get_mut(&clip_id).unwrap();
                        if let Some(ai) = indices.iter().position(|&ni| ni == anchor_idx) {
                            let delta = anchor_new_dur - orig_durs[ai];
                            for (j, &ni) in indices.iter().enumerate() {
                                if ni < mc.notes.len() {
                                    mc.notes[ni].duration_px = (orig_durs[j] + delta).max(10.0);
                                }
                            }
                        } else {
                            mc.notes[anchor_idx].duration_px = anchor_new_dur;
                        }
                        self.mark_dirty();
                    }
                    }
                }
                if let DragState::ResizingMidiNoteLeft { clip_id, anchor_idx, note_indices, original_starts, original_durations, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let anchor_idx = *anchor_idx;
                    let indices = note_indices.clone();
                    let orig_starts = original_starts.clone();
                    let orig_durs = original_durations.clone();
                    if let Some(mc) = self.midi_clips.get(&clip_id) {
                        if anchor_idx < mc.notes.len() {
                        if let Some(ai) = indices.iter().position(|&ni| ni == anchor_idx) {
                            let clip_x = mc.position[0];
                            let mc_gm = mc.grid_mode;
                            let mc_trip = mc.triplet_grid;
                            let snapped_x = if self.is_snap_override_active() {
                                world[0]
                            } else {
                                snap_to_clip_grid(world[0], &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm)
                            };
                            let anchor_new_start = (snapped_x - clip_x).max(0.0);
                            let anchor_right = orig_starts[ai] + orig_durs[ai];
                            let anchor_clamped = anchor_new_start.min(anchor_right - 10.0);
                            let delta = anchor_clamped - orig_starts[ai];
                            let mc = self.midi_clips.get_mut(&clip_id).unwrap();
                            for (j, &ni) in indices.iter().enumerate() {
                                if ni < mc.notes.len() {
                                    let new_start = (orig_starts[j] + delta).max(0.0);
                                    let right_edge = orig_starts[j] + orig_durs[j];
                                    let clamped = new_start.min(right_edge - 10.0);
                                    mc.notes[ni].start_px = clamped;
                                    mc.notes[ni].duration_px = right_edge - clamped;
                                }
                            }
                        }
                        self.mark_dirty();
                    }
                    }
                }
                if let DragState::MovingMidiClip { clip_id, offset, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let offset = *offset;
                    if self.midi_clips.contains_key(&clip_id) {
                        let raw_x = world[0] - offset[0];
                        let snapped_x = if self.is_snap_override_active() {
                            raw_x
                        } else {
                            snap_to_grid(raw_x, &self.settings, self.camera.zoom, self.bpm)
                        };
                        let raw_y = world[1] - offset[1];
                        let snapped_y = if self.is_snap_override_active() {
                            raw_y
                        } else {
                            snap_to_vertical_grid(raw_y, &self.settings, self.camera.zoom, self.bpm)
                        };
                        self.midi_clips.get_mut(&clip_id).unwrap().position = [snapped_x, snapped_y];
                        let mc = &self.midi_clips[&clip_id];
                        self.broadcast_drag_preview(crate::user::DragPreview::MovingEntities {
                            targets: vec![(HitTarget::MidiClip(clip_id), mc.position, mc.size)],
                        });
                        self.mark_dirty();
                        self.sync_audio_clips();
                    }
                }
                if let DragState::SelectingMidiNotes { clip_id, start_world } = &self.drag {
                    let clip_id = *clip_id;
                    let start = *start_world;
                    if let Some(mc) = self.midi_clips.get(&clip_id) {
                        let mc_pos = mc.position;
                        let mc_size = mc.size;
                        // Compute selection rect, clamped to clip bounds
                        let rx = start[0].min(world[0]).max(mc_pos[0]);
                        let ry = start[1].min(world[1]).max(mc_pos[1]);
                        let rx2 = start[0].max(world[0]).min(mc_pos[0] + mc_size[0]);
                        let ry2 = start[1].max(world[1]).min(mc_pos[1] + mc_size[1]);
                        let rw = (rx2 - rx).max(0.0);
                        let rh = (ry2 - ry).max(0.0);
                        self.midi_note_select_rect = Some([rx, ry, rw, rh]);
                        let editing = self.editing_midi_clip == Some(clip_id);
                        let nh = mc.note_height_editing(editing);
                        let mut selected = Vec::new();
                        for (i, note) in mc.notes.iter().enumerate() {
                            let nx = mc_pos[0] + note.start_px;
                            let ny = mc.pitch_to_y_editing(note.pitch, editing);
                            let nw = note.duration_px;
                            // AABB intersection
                            if nx < rx + rw && nx + nw > rx && ny < ry + rh && ny + nh > ry {
                                selected.push(i);
                            }
                        }
                        self.selected_midi_notes = selected;
                        self.mark_dirty();
                    }
                }
                if let DragState::DraggingVelocity { clip_id, note_indices, original_velocities, start_world_y, .. } = &self.drag {
                    let clip_id = *clip_id;
                    let indices = note_indices.clone();
                    let orig_vels = original_velocities.clone();
                    let start_y = *start_world_y;
                    if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                        let lane_height = mc.velocity_lane_height;
                        let delta_y = start_y - world[1];
                        let vel_delta = (delta_y / lane_height * 127.0) as i16;
                        for (j, &ni) in indices.iter().enumerate() {
                            if ni < mc.notes.len() {
                                let new_vel = (orig_vels[j] as i16 + vel_delta).clamp(0, 127) as u8;
                                mc.notes[ni].velocity = new_vel;
                            }
                        }
                        self.mark_dirty();
                    }
                }
                if let DragState::ResizingVelocityLane { clip_id, start_world_y, original_height } = &self.drag {
                    let clip_id = *clip_id;
                    let start_y = *start_world_y;
                    let orig_h = *original_height;
                    if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                        let delta_y = start_y - world[1];
                        let new_height = (orig_h + delta_y)
                            .clamp(midi::VELOCITY_LANE_MIN_HEIGHT, midi::VELOCITY_LANE_MAX_HEIGHT);
                        mc.velocity_lane_height = new_height;
                        self.mark_dirty();
                    }
                }
            }
        }

        self.update_hover();
        self.request_redraw();
    }
}
