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
            if self.export_window.is_some() {
                let (scr_w, scr_h, scale) = self.screen_info();
                let pos = self.mouse_pos;
                if let Some(ew) = &mut self.export_window {
                    ew.update_hover(pos, scr_w, scr_h, scale);
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
            #[cfg(feature = "native")]
            if let Some(engine) = &self.audio_engine {
                engine.set_bpm(self.bpm);
            }
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
                let target = self.right_window.as_ref().map(|rw| rw.target);
                if let Some(rw) = self.right_window.as_mut() {
                    if is_vol_drag {
                        let new_vol = ui::right_window::RightWindow::drag_vol_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.volume = new_vol;
                        let snapshots = rw.drag_start_snapshots.clone();
                        match target {
                            Some(ui::right_window::RightWindowTarget::Waveform(_)) => {
                                if snapshots.len() > 1 {
                                    // Relative: compute dB delta from first clip, apply to each clip's original
                                    let first_original_db = ui::palette::gain_to_db(snapshots[0].1.volume);
                                    let new_db = ui::palette::gain_to_db(new_vol);
                                    let db_delta = new_db - first_original_db;
                                    for (id, orig) in &snapshots {
                                        if let Some(wf) = self.waveforms.get_mut(id) {
                                            let orig_db = ui::palette::gain_to_db(orig.volume);
                                            let target_db = (orig_db + db_delta).clamp(
                                                ui::palette::VOL_FADER_DB_BOTTOM,
                                                ui::palette::VOL_FADER_DB_MAX,
                                            );
                                            wf.volume = if target_db <= ui::palette::VOL_FADER_DB_BOTTOM {
                                                0.0
                                            } else {
                                                ui::palette::db_to_gain(target_db)
                                            };
                                        }
                                    }
                                } else {
                                    // Single clip: absolute
                                    for (id, _) in &snapshots {
                                        if let Some(wf) = self.waveforms.get_mut(id) {
                                            wf.volume = new_vol;
                                        }
                                    }
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Instrument(inst_id)) => {
                                if let Some(inst) = self.instruments.get_mut(&inst_id) {
                                    inst.volume = new_vol;
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Group(gid)) => {
                                if let Some(g) = self.groups.get_mut(&gid) {
                                    g.volume = new_vol;
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Master) => {
                                self.master.volume = new_vol;
                            }
                            None => {}
                        }
                    } else if is_pan_drag {
                        let new_pan = ui::right_window::RightWindow::drag_pan_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.pan = new_pan;
                        let snapshots = rw.drag_start_snapshots.clone();
                        match target {
                            Some(ui::right_window::RightWindowTarget::Waveform(_)) => {
                                if snapshots.len() > 1 {
                                    // Relative: compute pan delta from first clip, apply to each
                                    let pan_delta = new_pan - snapshots[0].1.pan;
                                    for (id, orig) in &snapshots {
                                        if let Some(wf) = self.waveforms.get_mut(id) {
                                            wf.pan = (orig.pan + pan_delta).clamp(0.0, 1.0);
                                        }
                                    }
                                } else {
                                    // Single clip: absolute
                                    for (id, _) in &snapshots {
                                        if let Some(wf) = self.waveforms.get_mut(id) {
                                            wf.pan = new_pan;
                                        }
                                    }
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Instrument(inst_id)) => {
                                if let Some(inst) = self.instruments.get_mut(&inst_id) {
                                    inst.pan = new_pan;
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Group(gid)) => {
                                if let Some(g) = self.groups.get_mut(&gid) {
                                    g.pan = new_pan;
                                }
                            }
                            Some(ui::right_window::RightWindowTarget::Master) => {
                                self.master.pan = new_pan;
                            }
                            None => {}
                        }
                    } else if is_sbpm_drag {
                        let new_bpm = ui::right_window::RightWindow::drag_sample_bpm_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.sample_bpm = new_bpm;
                        if let Some(ui::right_window::RightWindowTarget::Waveform(wf_id)) = target {
                            if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                wf.sample_bpm = new_bpm;
                                if wf.warp_mode == ui::waveform::WarpMode::RePitch {
                                    if let Some(clip) = self.audio_clips.get(&wf_id) {
                                        let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                        wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                                    }
                                }
                            }
                        }
                    } else {
                        let new_pitch = ui::right_window::RightWindow::drag_pitch_delta(
                            rw.drag_start_y, self.mouse_pos[1], rw.drag_start_value, scale
                        );
                        rw.pitch_semitones = new_pitch;
                        if let Some(ui::right_window::RightWindowTarget::Waveform(wf_id)) = target {
                            if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                wf.pitch_semitones = new_pitch;
                            }
                            self.resize_warped_clips();
                        }
                    }
                }
                self.mark_dirty();
                #[cfg(feature = "native")]
                match target {
                    Some(ui::right_window::RightWindowTarget::Instrument(_)) => self.sync_instrument_regions_auto(),
                    _ => self.sync_audio_clips(),
                }
                self.request_redraw();
                return;
            }
        }

        // Right window "Add Effect" / "Export" button hover
        if let Some(rw) = &self.right_window {
            let (sw, sh, scale) = self.screen_info();
            let (add_hovered, export_hovered) = {
                let chain_id = match rw.target {
                    ui::right_window::RightWindowTarget::Waveform(wf_id) => self.waveforms.get(&wf_id).and_then(|w| w.effect_chain_id),
                    ui::right_window::RightWindowTarget::Instrument(inst_id) => self.instruments.get(&inst_id).and_then(|i| i.effect_chain_id),
                    ui::right_window::RightWindowTarget::Group(group_id) => self.groups.get(&group_id).and_then(|g| g.effect_chain_id),
                    ui::right_window::RightWindowTarget::Master => self.master.effect_chain_id,
                };
                let slot_count = chain_id
                    .and_then(|cid| self.effect_chains.get(&cid))
                    .map_or(0, |c| c.slots.len());
                let add_h = rw.hit_test_add_effect_button(self.mouse_pos, slot_count, sw, sh, scale);
                let export_h = if rw.is_group() || rw.is_master() {
                    rw.hit_test_export_button(self.mouse_pos, sw, sh, scale)
                } else {
                    false
                };
                (add_h, export_h)
            };
            let add_changed = add_hovered != rw.add_effect_hovered;
            let export_changed = export_hovered != rw.export_button_hovered;
            if add_changed || export_changed {
                if let Some(rw) = &mut self.right_window {
                    if add_changed { rw.add_effect_hovered = add_hovered; }
                    if export_changed { rw.export_button_hovered = export_hovered; }
                }
                self.request_redraw();
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

        // --- Layer node reorder drag ---
        if matches!(self.drag, DragState::ReorderingLayerNode { .. }) {
            let (_, sh, scale) = self.screen_info();
            let mouse_y = self.mouse_pos[1];
            let threshold = 4.0 * scale;

            // Extract immutable fields we need
            let (start_y, entity_id, was_active) = if let DragState::ReorderingLayerNode {
                start_y, entity_id, drag_active, ..
            } = &self.drag {
                (*start_y, *entity_id, *drag_active)
            } else {
                unreachable!()
            };

            // Activate drag if threshold exceeded
            if !was_active && (mouse_y - start_y).abs() >= threshold {
                let src_before = crate::layers::find_parent_group(&self.layer_tree, entity_id)
                    .and_then(|gid| self.groups.get(&gid).map(|g| (gid, g.clone())));
                if let DragState::ReorderingLayerNode {
                    ref mut drag_active,
                    ref mut source_group_before,
                    ..
                } = self.drag {
                    *drag_active = true;
                    *source_group_before = src_before;
                }
            }

            let is_active = if let DragState::ReorderingLayerNode { drag_active, .. } = &self.drag {
                *drag_active
            } else { false };

            if is_active {
                // Compute drop target
                let item_at = self.sample_browser.item_at(self.mouse_pos, sh, scale);
                let mut new_target = None;
                let mut new_indicator = None;
                let mut expand_group_id: Option<crate::entity_id::EntityId> = None;
                let mut clear_hover_expand = false;

                if let Some(idx) = item_at {
                    let top = self.sample_browser.content_top(scale);
                    let item_h = crate::ui::browser::ITEM_HEIGHT * scale;
                    let row_y = top + idx as f32 * item_h - self.sample_browser.scroll_offset;
                    let y_fraction = ((mouse_y - row_y) / item_h).clamp(0.0, 1.0);

                    let target = crate::layers::compute_drop_target(
                        &self.sample_browser.layer_rows, &self.layer_tree, idx, y_fraction, entity_id,
                    );

                    if let Some(ref t) = target {
                        new_indicator = crate::layers::drop_target_indicator(t, &self.layer_tree, &self.sample_browser.layer_rows);
                    }

                    // Check auto-expand
                    if let Some(crate::layers::DropTarget::InsideGroup { group_id, .. }) = &target {
                        let is_collapsed = self.layer_tree.iter()
                            .find(|n| n.entity_id == *group_id)
                            .map_or(false, |n| !n.expanded);
                        if is_collapsed {
                            let now = TimeInstant::now();
                            if let DragState::ReorderingLayerNode { ref hover_expand_target, .. } = &self.drag {
                                let should_expand = match hover_expand_target {
                                    Some((prev_id, started)) if *prev_id == *group_id => {
                                        now.duration_since(*started).as_millis() >= 500
                                    }
                                    _ => false,
                                };
                                let needs_set = match hover_expand_target {
                                    Some((prev_id, _)) if *prev_id == *group_id => false,
                                    _ => true,
                                };
                                if should_expand {
                                    expand_group_id = Some(*group_id);
                                } else if needs_set {
                                    if let DragState::ReorderingLayerNode { ref mut hover_expand_target, .. } = self.drag {
                                        *hover_expand_target = Some((*group_id, now));
                                    }
                                }
                            }
                        } else {
                            clear_hover_expand = true;
                        }
                    } else {
                        clear_hover_expand = true;
                    }

                    new_target = target;
                } else {
                    new_target = Some(crate::layers::DropTarget::AfterLastRoot);
                    let total = self.sample_browser.layer_rows.len();
                    new_indicator = Some((total, 0, false));
                    clear_hover_expand = true;
                }

                // Apply updates
                if let DragState::ReorderingLayerNode { ref mut drop_target, ref mut hover_expand_target, .. } = self.drag {
                    *drop_target = new_target;
                    if clear_hover_expand {
                        *hover_expand_target = None;
                    }
                }
                self.sample_browser.layer_drop_indicator = new_indicator;

                // Handle auto-expand (must be outside drag borrow)
                if let Some(gid) = expand_group_id {
                    if let DragState::ReorderingLayerNode { ref mut hover_expand_target, .. } = self.drag {
                        *hover_expand_target = None;
                    }
                    crate::layers::toggle_expanded(&mut self.layer_tree, gid);
                    self.refresh_project_browser_entries();
                }
            }

            self.update_cursor();
            self.request_redraw();
            return;
        }

        // Update browser hover state
        if !matches!(self.drag, DragState::ResizingBrowser) {
            let (_, sh, scale) = self.screen_info();
            if self.sample_browser.visible {
                if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                    self.sample_browser.update_hover(self.mouse_pos, sh, scale);
                } else {
                    self.sample_browser.toggle_hovered = false;
                    self.sample_browser.hovered_entry = None;
                    self.sample_browser.resize_hovered = false;
                }
            } else {
                self.sample_browser.toggle_hovered =
                    self.sample_browser.hit_toggle_button(self.mouse_pos, scale);
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

        // Promote pending browser drag to real drag after threshold
        if let DragState::PendingBrowserDrag { ref path, ref filename, start_mouse } = self.drag {
            let dx = self.mouse_pos[0] - start_mouse[0];
            let dy = self.mouse_pos[1] - start_mouse[1];
            if dx * dx + dy * dy > 9.0 { // 3px threshold
                self.drag = DragState::DraggingFromBrowser {
                    path: path.clone(),
                    filename: filename.clone(),
                };
            }
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

        // Resizing text note
        if let DragState::ResizingTextNote { note_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(tn) = self.text_notes.get_mut(&note_id) {
                tn.position = pos;
                tn.size = size;
            }
            self.render_generation += 1;
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

        // Resizing MIDI clip
        if let DragState::ResizingMidiClip { clip_id, anchor, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
            if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                mc.position = pos;
                mc.size = size;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing MIDI pitch range (semitone-based)
        if let DragState::ResizingMidiPitchRange { clip_id, edge, start_y, ref before } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let dy = world[1] - start_y;
            let nh = before.note_height();
            if nh > 0.0 {
                let delta_semitones = (dy / nh).round() as i32;
                if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                    // Reset to before state, then apply delta
                    mc.position = before.position;
                    mc.size = before.size;
                    mc.pitch_range = before.pitch_range;
                    let min_range: i32 = 12;
                    match edge {
                        PitchRangeEdge::Top => {
                            // Dragging up (dy < 0) → extend top, down → reduce
                            let new_high = (before.pitch_range.1 as i32 - delta_semitones)
                                .clamp(before.pitch_range.0 as i32 + min_range, 127);
                            let added = new_high - before.pitch_range.1 as i32;
                            mc.pitch_range.1 = new_high as u8;
                            mc.position[1] = before.position[1] - nh * added as f32;
                            let range = (mc.pitch_range.1 - mc.pitch_range.0) as f32;
                            mc.size[1] = nh * range;
                        }
                        PitchRangeEdge::Bottom => {
                            // Dragging down (dy > 0) → extend bottom, up → reduce
                            let new_low = (before.pitch_range.0 as i32 - delta_semitones)
                                .clamp(0, before.pitch_range.1 as i32 - min_range);
                            mc.pitch_range.0 = new_low as u8;
                            let range = (mc.pitch_range.1 - mc.pitch_range.0) as f32;
                            mc.size[1] = nh * range;
                        }
                    }
                }
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing MIDI clip edge (horizontal)
        if matches!(self.drag, DragState::ResizingMidiClipEdge { .. }) {
            let (clip_id, is_left, before) = match &self.drag {
                DragState::ResizingMidiClipEdge { clip_id, is_left, before, .. } => (*clip_id, *is_left, before.clone()),
                _ => unreachable!(),
            };
            let world = self.camera.screen_to_world(self.mouse_pos);
            let snap = !self.is_snap_override_active();
            let snapped_x = if snap {
                crate::grid::snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm)
            } else {
                world[0]
            };
            let min_w = 20.0;
            if let Some(mc) = self.midi_clips.get_mut(&clip_id) {
                if is_left {
                    let right = before.position[0] + before.size[0];
                    let new_x = snapped_x.min(right - min_w);
                    mc.position[0] = new_x;
                    mc.size[0] = right - new_x;
                } else {
                    let new_w = (snapped_x - before.position[0]).max(min_w);
                    mc.size[0] = new_w;
                }
            }
            let mut snaps = if let DragState::ResizingMidiClipEdge { overlap_snapshots, .. } = &mut self.drag {
                std::mem::take(overlap_snapshots)
            } else { IndexMap::new() };
            let mut tsplits = if let DragState::ResizingMidiClipEdge { overlap_temp_splits, .. } = &mut self.drag {
                std::mem::take(overlap_temp_splits)
            } else { Vec::new() };
            self.resolve_midi_clip_overlaps_live(&[clip_id], &mut snaps, &mut tsplits);
            if let DragState::ResizingMidiClipEdge { overlap_snapshots, overlap_temp_splits, .. } = &mut self.drag {
                *overlap_snapshots = snaps;
                *overlap_temp_splits = tsplits;
            }
            self.mark_dirty();
            self.request_redraw();
            return;
        }

        // Resizing loop region (horizontal only)
        if let DragState::ResizingLoopRegion { region_id, is_left_edge, initial_position_x, initial_size_w, .. } = self.drag {
            let world = self.camera.screen_to_world(self.mouse_pos);
            let snapped_x = if self.is_snap_override_active() {
                world[0]
            } else {
                snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm)
            };
            let min_w = 40.0;
            if let Some(lr) = self.loop_regions.get_mut(&region_id) {
                if is_left_edge {
                    let right_x = initial_position_x + initial_size_w;
                    let new_left = snapped_x.min(right_x - min_w);
                    lr.position[0] = new_left;
                    lr.size[0] = (right_x - new_left).max(min_w);
                } else {
                    let new_right = snapped_x.max(initial_position_x + min_w);
                    lr.size[0] = new_right - lr.position[0];
                }
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
            if let DragState::ResizingWaveform { waveform_id, .. } = self.drag {
                self.update_groups_containing(waveform_id);
            }
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
                self.following_user = None;
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
                            | HitTarget::LoopRegion(_)
                            | HitTarget::ExportRegion(_)
                            | HitTarget::ComponentDef(_)
                            | HitTarget::ComponentInstance(_)
                            | HitTarget::MidiClip(_)
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
                if let Some(ec_idx) = self.editing_component {
                    self.update_component_bounds(ec_idx);
                }
                // Live group bounds update during drag
                for (target, _) in &offsets {
                    match target {
                        HitTarget::Waveform(id) | HitTarget::MidiClip(id)
                        | HitTarget::TextNote(id) | HitTarget::Object(id) => {
                            self.update_groups_containing(*id);
                        }
                        _ => {}
                    }
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
                        self.include_paired_instruments();
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
                if matches!(self.drag, DragState::MovingMidiClip { .. }) {
                    let (clip_id, offset) = match &self.drag {
                        DragState::MovingMidiClip { clip_id, offset, .. } => (*clip_id, *offset),
                        _ => unreachable!(),
                    };
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
                        let mut snaps = if let DragState::MovingMidiClip { overlap_snapshots, .. } = &mut self.drag {
                            std::mem::take(overlap_snapshots)
                        } else { IndexMap::new() };
                        let mut tsplits = if let DragState::MovingMidiClip { overlap_temp_splits, .. } = &mut self.drag {
                            std::mem::take(overlap_temp_splits)
                        } else { Vec::new() };
                        self.resolve_midi_clip_overlaps_live(&[clip_id], &mut snaps, &mut tsplits);
                        if let DragState::MovingMidiClip { overlap_snapshots, overlap_temp_splits, .. } = &mut self.drag {
                            *overlap_snapshots = snaps;
                            *overlap_temp_splits = tsplits;
                        }
                        let mc = &self.midi_clips[&clip_id];
                        self.broadcast_drag_preview(crate::user::DragPreview::MovingEntities {
                            targets: vec![(HitTarget::MidiClip(clip_id), mc.position, mc.size)],
                        });
                        self.mark_dirty();
                        self.sync_audio_clips();
                        self.update_groups_containing(clip_id);
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

        // Transport tooltip detection
        let (sw, sh, scale) = self.screen_info();
        if TransportPanel::contains(self.mouse_pos, sw, sh, scale) {
            if TransportPanel::hit_metronome_button(self.mouse_pos, sw, sh, scale) {
                let text = if self.settings.metronome_enabled { "Metronome (On)" } else { "Metronome" };
                let rect = TransportPanel::metronome_button_rect(sw, sh, scale);
                self.tooltip.set_target("transport:metronome", text, rect);
            } else if TransportPanel::hit_computer_keyboard_button(self.mouse_pos, sw, sh, scale) {
                let text = if self.computer_keyboard_armed {
                    "Computer MIDI keyboard (on) \u{2014} A row plays, Z/X octaves, C/V velocity"
                } else {
                    "Computer MIDI keyboard (off) \u{2014} click to preview the selected instrument"
                };
                let rect = TransportPanel::computer_keyboard_button_rect(sw, sh, scale);
                self.tooltip.set_target("transport:computer_keys", text, rect);
            } else if TransportPanel::hit_play_pause(self.mouse_pos, sw, sh, scale) {
                #[cfg(feature = "native")]
                let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
                #[cfg(not(feature = "native"))]
                let is_playing = false;
                let text = if is_playing { "Pause" } else { "Play" };
                let rect = TransportPanel::play_pause_rect(sw, sh, scale);
                self.tooltip.set_target("transport:play_pause", text, rect);
            } else if TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                let rect = TransportPanel::bpm_rect(sw, sh, scale);
                self.tooltip.set_target("transport:bpm", "Tempo \u{2014} double-click to edit", rect);
            } else if TransportPanel::hit_monitor_button(self.mouse_pos, sw, sh, scale) {
                let text = if self.input_monitoring { "Disable Input Monitor" } else { "Input Monitor" };
                let rect = TransportPanel::monitor_button_rect(sw, sh, scale);
                self.tooltip.set_target("transport:monitor", text, rect);
            } else if TransportPanel::hit_record_button(self.mouse_pos, sw, sh, scale) {
                #[cfg(feature = "native")]
                let is_recording = self.recorder.as_ref().map_or(false, |r| r.is_recording());
                #[cfg(not(feature = "native"))]
                let is_recording = false;
                let text = if is_recording { "Stop Recording" } else { "Record" };
                let rect = TransportPanel::record_button_rect(sw, sh, scale);
                self.tooltip.set_target("transport:record", text, rect);
            } else {
                self.tooltip.clear();
            }
        } else {
            self.tooltip.clear();
        }

        self.request_redraw();
    }

    pub(crate) fn update_cursor(&self) {
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
                    DragState::ResizingMidiPitchRange { .. } => CursorIcon::NsResize,
                    DragState::ResizingMidiClipEdge { .. } => CursorIcon::EwResize,
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
                    DragState::ResizingLoopRegion { .. } => {
                        CursorIcon::EwResize
                    }
                    DragState::ReorderingLayerNode { drag_active, .. } => {
                        if *drag_active { CursorIcon::Grabbing } else { CursorIcon::Default }
                    }
                    DragState::None => {
                        if self.cmd_velocity_hover_note.is_some() {
                            CursorIcon::NsResize
                        } else if self.sample_browser.visible && self.sample_browser.resize_hovered {
                            CursorIcon::EwResize
                        } else if self.waveform_edge_hover != WaveformEdgeHover::None {
                            CursorIcon::EwResize
                        } else if self.midi_clip_edge_hover_ns {
                            CursorIcon::NsResize
                        } else if self.midi_clip_edge_hover_ew {
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
                                ComponentDefHover::None => match self.export_hover {
                                    ExportHover::CornerNW(_) | ExportHover::CornerSE(_) => {
                                        CursorIcon::NwseResize
                                    }
                                    ExportHover::CornerNE(_) | ExportHover::CornerSW(_) => {
                                        CursorIcon::NeswResize
                                    }
                                    ExportHover::RenderPill(_) => CursorIcon::Pointer,
                                    ExportHover::None => match self.loop_hover {
                                        LoopHover::LeftEdge(_) | LoopHover::RightEdge(_) => {
                                            CursorIcon::EwResize
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
                            }
                        }
                    }
                    _ => CursorIcon::Default,
                };
            gpu.window.set_cursor(icon);
        }
    }

    pub(crate) fn update_hover(&mut self) {
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
        self.fade_handle_hovered = hit_test_fade_handle(&self.waveforms, world, &self.camera)
            .filter(|(id, _)| !self.is_in_non_entered_group(id));
        self.waveform_edge_hover = if self.fade_handle_hovered.is_none() {
            let edge = hit_test_waveform_edge(&self.waveforms, world, &self.camera);
            match &edge {
                WaveformEdgeHover::LeftEdge(id) | WaveformEdgeHover::RightEdge(id)
                    if self.is_in_non_entered_group(id) => WaveformEdgeHover::None,
                _ => edge,
            }
        } else {
            WaveformEdgeHover::None
        };
        self.midi_note_edge_hover = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                matches!(
                    midi::hit_test_midi_note_editing(mc, world, &self.camera, true),
                    Some((_, midi::MidiNoteHitZone::RightEdge | midi::MidiNoteHitZone::LeftEdge))
                )
            } else {
                false
            }
        } else {
            false
        };
        // MIDI clip edge hover (right edge = EW, bottom edge = NS)
        let mut ew_hover = false;
        let mut ns_hover = false;
        for (_, mc) in self.midi_clips.iter() {
            if mc.hit_test_right_edge(world, &self.camera) || mc.hit_test_left_edge(world, &self.camera) {
                ew_hover = true;
                break;
            }
            if mc.hit_test_pitch_range_bottom(world, &self.camera) || mc.hit_test_pitch_range_top(world, &self.camera) {
                ns_hover = true;
                break;
            }
        }
        self.midi_clip_edge_hover_ew = ew_hover;
        self.midi_clip_edge_hover_ns = ns_hover;

        self.velocity_divider_hovered = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                midi::hit_test_velocity_divider(mc, world, &self.camera)
            } else {
                false
            }
        } else {
            false
        };
        self.velocity_bar_hovered = if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                !self.velocity_divider_hovered && midi::hit_test_velocity_bar(mc, world, &self.camera).is_some()
            } else {
                false
            }
        } else {
            false
        };
        self.cmd_velocity_hover_note = if self.cmd_held() {
            if let Some(mc_id) = self.editing_midi_clip {
                if let Some(mc) = self.midi_clips.get(&mc_id) {
                    match midi::hit_test_midi_note_editing(mc, world, &self.camera, true) {
                        Some((note_idx, midi::MidiNoteHitZone::Body | midi::MidiNoteHitZone::LeftEdge | midi::MidiNoteHitZone::RightEdge)) => {
                            Some((mc_id, note_idx))
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
        self.fade_curve_hovered = if self.fade_handle_hovered.is_none() && self.waveform_edge_hover == WaveformEdgeHover::None {
            hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
                .filter(|(id, _)| !self.is_in_non_entered_group(id))
        } else {
            None
        };
        let (sw, sh, _) = self.screen_info();
        let raw_hover = hit_test(
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
        self.hovered = raw_hover.map(|h| self.redirect_to_group(h));

        self.component_def_hover = ComponentDefHover::None;
        for (&ci, def) in self.components.iter() {
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


        self.export_hover = ExportHover::None;
        for (&i, er) in self.export_regions.iter() {
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
        let (sw_l, sh_l, _) = self.screen_info();
        let wt = self.camera.position[1];
        let wb = wt + sh_l / self.camera.zoom;
        for (&i, lr) in self.loop_regions.iter() {
            if !lr.enabled {
                continue;
            }
            let border_zone = 8.0 / self.camera.zoom;
            let p_x = lr.position[0];
            let right_x = p_x + lr.size[0];
            // Left edge: vertical strip [p_x - border_zone, p_x + border_zone] x full viewport
            if world[0] >= p_x - border_zone && world[0] <= p_x + border_zone
                && world[1] >= wt && world[1] <= wb
            {
                self.loop_hover = LoopHover::LeftEdge(i);
                break;
            }
            // Right edge
            if world[0] >= right_x - border_zone && world[0] <= right_x + border_zone
                && world[1] >= wt && world[1] <= wb
            {
                self.loop_hover = LoopHover::RightEdge(i);
                break;
            }
        }

        self.update_cursor();
    }
}
