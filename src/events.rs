use super::*;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    platform::macos::WindowAttributesExtMacOS,
    window::{Window, WindowId},
};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Layers")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800))
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        if !self.has_saved_state {
            self.camera.zoom = window.scale_factor() as f32;
        }

        self.gpu = Some(pollster::block_on(Gpu::new(window)));

        if let Some(ms) = &mut self.menu_state {
            if !ms.initialized {
                ms.menu.init_for_nsapp();
                ms.initialized = true;
            }
        }

        // Scan plugins and restore saved plugin/instrument state at startup
        self.ensure_plugins_scanned();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());

        if self.sample_browser.visible && self.sample_browser.tick_scroll() {
            self.request_redraw();
        }

        if is_playing || self.is_recording() {
            self.request_redraw();
        }

        if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            self.handle_menu_event(event.id);
        }

        // GUI handles are kept alive (just hidden when user closes window).
        // No teardown or state sync needed here.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if !self.project_dirty {
                    self.shutdown_plugins();
                    event_loop.exit();
                    return;
                }

                let is_temp = self
                    .storage
                    .as_ref()
                    .map(|s| s.is_temp_project())
                    .unwrap_or(false);

                let result = rfd::MessageDialog::new()
                    .set_title("Save Changes?")
                    .set_description(
                        "Your project has unsaved changes. Would you like to save before closing?",
                    )
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();

                match result {
                    rfd::MessageDialogResult::Yes => {
                        if is_temp {
                            self.save_project();
                        } else {
                            self.save_project_state();
                        }
                        self.shutdown_plugins();
                        event_loop.exit();
                    }
                    rfd::MessageDialogResult::No => {
                        if is_temp && !self.waveforms.is_empty() {
                            if let Some(storage) = &mut self.storage {
                                if let Some(path) = storage
                                    .current_project_path()
                                    .map(|p| p.to_string_lossy().to_string())
                                {
                                    storage.delete_project(&path);
                                }
                            }
                        }
                        self.shutdown_plugins();
                        event_loop.exit();
                    }
                    _ => {}
                }
            }

            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size);
                    self.request_redraw();
                }
            }

            // --- drag & drop files ---
            WindowEvent::HoveredFile(_) => {
                self.file_hovering = true;
                self.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                self.file_hovering = false;
                self.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                self.file_hovering = false;
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    if let Some(loaded) = load_audio_file(&path) {
                        self.push_undo();
                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let height = 150.0;
                        let color_idx = self.waveforms.len() % WAVEFORM_COLORS.len();
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
                                filename: filename.clone(),
                            }),
                            position: [world[0] - loaded.width * 0.5, world[1] - height * 0.5],
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
                            automation: crate::automation::AutomationData::new(),
                        });
                        self.audio_clips.push(AudioClipData {
                            samples: loaded.samples,
                            sample_rate: loaded.sample_rate,
                            duration_secs: loaded.duration_secs,
                        });
                        self.sync_audio_clips();
                    } else {
                        self.toast_manager.push(
                            format!(
                                "Cannot load '{}' \u{2014} unsupported or corrupted file",
                                filename
                            ),
                            ui::toast::ToastKind::Error,
                        );
                    }
                } else {
                    self.toast_manager.push(
                        format!(
                            "Cannot load '{}' \u{2014} not a supported audio format",
                            filename
                        ),
                        ui::toast::ToastKind::Error,
                    );
                }
                self.request_redraw();
            }

            // --- cursor ---
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = [position.x as f32, position.y as f32];

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
                            let new_val = pe.slider_drag(idx, mx, scr_w, scr_h, scale);
                            let pb_idx = pe.region_idx; // now repurposed as plugin_block index
                            if let Some(pb) = self.plugin_blocks.get(pb_idx) {
                                if let Ok(guard) = pb.gui.lock() {
                                    if let Some(gui) = guard.as_ref() {
                                        gui.set_parameter(idx, new_val as f64);
                                    }
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // Settings window: slider drag + hover
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
                    }
                }

                if let Some((initial_bpm, initial_y)) = self.dragging_bpm {
                    let dy = initial_y - self.mouse_pos[1];
                    let new_bpm = (initial_bpm + dy * 0.5).clamp(20.0, 999.0);
                    self.bpm = new_bpm;
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

                {
                    let is_dragging_fader = self
                        .command_palette
                        .as_ref()
                        .map_or(false, |p| p.fader_dragging);
                    if is_dragging_fader {
                        let (sw, sh, scale) = self.screen_info();
                        if let Some(p) = &mut self.command_palette {
                            match p.mode {
                                PaletteMode::SampleVolumeFader => {
                                    let my = self.mouse_pos[1];
                                    p.sample_fader_drag(my, sw, sh, scale);
                                    if let Some(idx) = p.fader_target_waveform {
                                        if idx < self.waveforms.len() {
                                            self.waveforms[idx].volume = p.fader_value;
                                            self.sync_audio_clips();
                                        }
                                    }
                                }
                                _ => {
                                    let mx = self.mouse_pos[0];
                                    p.fader_drag(mx, sw, sh, scale);
                                    if let Some(engine) = &self.audio_engine {
                                        engine.set_master_volume(p.fader_value);
                                    }
                                }
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
                if let DragState::ResizingComponentDef { comp_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if comp_idx < self.components.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.components[comp_idx].position = pos;
                        self.components[comp_idx].size = size;
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing export region
                if let DragState::ResizingExportRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.export_regions.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.export_regions[region_idx].position = pos;
                        self.export_regions[region_idx].size = size;
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing effect region
                if let DragState::ResizingEffectRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.effect_regions.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.effect_regions[region_idx].position = pos;
                        self.effect_regions[region_idx].size = size;
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing instrument region
                if let DragState::ResizingInstrumentRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.instrument_regions.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.instrument_regions[region_idx].position = pos;
                        self.instrument_regions[region_idx].size = size;
                    }
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing MIDI clip
                if let DragState::ResizingMidiClip { clip_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if clip_idx < self.midi_clips.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.midi_clips[clip_idx].position = pos;
                        self.midi_clips[clip_idx].size = size;
                        // Auto-extend any overlapping instrument region
                        let padding = instruments::INSTRUMENT_REGION_PADDING;
                        for ir in &mut self.instrument_regions {
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
                if let DragState::ResizingLoopRegion { region_idx, anchor, .. } = self.drag {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if region_idx < self.loop_regions.len() {
                        let (pos, size) = compute_resize(anchor, world, 40.0, !self.is_snap_override_active(), &self.settings, self.camera.zoom, self.bpm);
                        self.loop_regions[region_idx].position = pos;
                        self.loop_regions[region_idx].size = size;
                    }
                    self.sync_loop_region();
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Resizing waveform edge
                if let DragState::ResizingWaveform {
                    waveform_idx,
                    is_left_edge,
                    initial_position_x,
                    initial_size_w,
                    initial_offset_px,
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if let Some(wf) = self.waveforms.get(waveform_idx) {
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

                            let wf = &mut self.waveforms[waveform_idx];
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
                            let mut new_size_w = snapped_right - self.waveforms[waveform_idx].position[0];
                            let cur_offset = self.waveforms[waveform_idx].sample_offset_px;

                            if new_size_w < min_w {
                                new_size_w = min_w;
                            }
                            if cur_offset + new_size_w > full_w {
                                new_size_w = full_w - cur_offset;
                            }

                            let wf = &mut self.waveforms[waveform_idx];
                            wf.size[0] = new_size_w;
                            wf.fade_in_px = wf.fade_in_px.min(new_size_w * 0.5);
                            wf.fade_out_px = wf.fade_out_px.min(new_size_w * 0.5);
                        }
                    }
                    self.sync_audio_clips();
                    self.mark_dirty();
                    self.request_redraw();
                    return;
                }

                // Dragging automation point
                if let DragState::DraggingAutomationPoint {
                    waveform_idx,
                    param,
                    point_idx,
                    ..
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if let Some(wf) = self.waveforms.get_mut(waveform_idx) {
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
                    waveform_idx,
                    is_fade_in,
                } = self.drag
                {
                    let world = self.camera.screen_to_world(self.mouse_pos);
                    if let Some(wf) = self.waveforms.get_mut(waveform_idx) {
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
                    waveform_idx,
                    is_fade_in,
                    start_mouse_y,
                    start_curve,
                } = self.drag
                {
                    let dy = self.mouse_pos[1] - start_mouse_y;
                    let sensitivity = 0.005;
                    let new_curve = (start_curve - dy * sensitivity).clamp(-1.0, 1.0);
                    if let Some(wf) = self.waveforms.get_mut(waveform_idx) {
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
                    MoveSelection(Vec<(HitTarget, [f32; 2])>),
                    Other,
                }
                let action = match &self.drag {
                    DragState::Panning {
                        start_mouse,
                        start_camera,
                    } => Action::Pan(*start_mouse, *start_camera),
                    DragState::MovingSelection { offsets } => {
                        Action::MoveSelection(offsets.clone())
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
                    Action::MoveSelection(offsets) => {
                        let world = self.camera.screen_to_world(self.mouse_pos);
                        let mut needs_sync = false;
                        for (target, offset) in &offsets {
                            let raw_x = world[0] - offset[0];
                            let snapped_x = if self.is_snap_override_active() {
                                raw_x
                            } else {
                                snap_to_grid(raw_x, &self.settings, self.camera.zoom, self.bpm)
                            };
                            self.set_target_pos(target, [snapped_x, world[1] - offset[1]]);
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
                        // Auto-extend instrument regions for moved MIDI clips
                        let padding = instruments::INSTRUMENT_REGION_PADDING;
                        for (target, _) in &offsets {
                            if let HitTarget::MidiClip(ci) = target {
                                if *ci < self.midi_clips.len() {
                                    let cp = self.midi_clips[*ci].position;
                                    let cs = self.midi_clips[*ci].size;
                                    for ir in &mut self.instrument_regions {
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
                            }
                        }
                        if let DragState::MovingMidiNote { clip_idx, note_indices, offsets, start_world } = &self.drag {
                            let clip_idx = *clip_idx;
                            let note_indices = note_indices.clone();
                            let offsets = offsets.clone();
                            let sw = *start_world;
                            if clip_idx < self.midi_clips.len() {
                                let drag_threshold = 3.0 / self.camera.zoom;
                                let dx = world[0] - sw[0];
                                let dy = world[1] - sw[1];
                                let below_threshold = self.pending_midi_note_click.is_some()
                                    && (dx * dx + dy * dy) < drag_threshold * drag_threshold;
                                if !below_threshold {
                                    self.pending_midi_note_click = None;
                                    let mc_pos = self.midi_clips[clip_idx].position;
                                    let mc_pr = self.midi_clips[clip_idx].pitch_range;
                                    let editing = self.editing_midi_clip == Some(clip_idx);
                                    let area_h = self.midi_clips[clip_idx].note_area_height(editing);
                                    let first_raw_x = world[0] - offsets[0][0];
                                    let mc_gm = self.midi_clips[clip_idx].grid_mode;
                                    let mc_trip = self.midi_clips[clip_idx].triplet_grid;
                                    let snap_delta = if self.is_snap_override_active() {
                                        0.0
                                    } else {
                                        snap_to_clip_grid(first_raw_x, &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm) - first_raw_x
                                    };
                                    for (i, &ni) in note_indices.iter().enumerate() {
                                        if ni < self.midi_clips[clip_idx].notes.len() {
                                            let raw_x = world[0] - offsets[i][0];
                                            let ny = world[1] - offsets[i][1];
                                            let start_px = (raw_x + snap_delta - mc_pos[0]).max(0.0);
                                            let nh = area_h / (mc_pr.1 - mc_pr.0) as f32;
                                            let relative = mc_pos[1] + area_h - ny;
                                            let pitch = ((relative / nh) as u8 + mc_pr.0).clamp(mc_pr.0, mc_pr.1 - 1);
                                            self.midi_clips[clip_idx].notes[ni].start_px = start_px;
                                            self.midi_clips[clip_idx].notes[ni].pitch = pitch;
                                        }
                                    }
                                    self.mark_dirty();
                                }
                            }
                        }
                        if let DragState::ResizingMidiNote { clip_idx, anchor_idx, note_indices, original_durations } = &self.drag {
                            let clip_idx = *clip_idx;
                            let anchor_idx = *anchor_idx;
                            let indices = note_indices.clone();
                            let orig_durs = original_durations.clone();
                            if clip_idx < self.midi_clips.len() && anchor_idx < self.midi_clips[clip_idx].notes.len() {
                                let mc_gm = self.midi_clips[clip_idx].grid_mode;
                                let mc_trip = self.midi_clips[clip_idx].triplet_grid;
                                let snapped_edge = if self.is_snap_override_active() {
                                    world[0]
                                } else {
                                    snap_to_clip_grid(world[0], &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm)
                                };
                                let anchor_x = self.midi_clips[clip_idx].position[0] + self.midi_clips[clip_idx].notes[anchor_idx].start_px;
                                let anchor_new_dur = (snapped_edge - anchor_x).max(10.0);
                                if let Some(ai) = indices.iter().position(|&ni| ni == anchor_idx) {
                                    let delta = anchor_new_dur - orig_durs[ai];
                                    for (j, &ni) in indices.iter().enumerate() {
                                        if ni < self.midi_clips[clip_idx].notes.len() {
                                            self.midi_clips[clip_idx].notes[ni].duration_px = (orig_durs[j] + delta).max(10.0);
                                        }
                                    }
                                } else {
                                    self.midi_clips[clip_idx].notes[anchor_idx].duration_px = anchor_new_dur;
                                }
                                self.mark_dirty();
                            }
                        }
                        if let DragState::ResizingMidiNoteLeft { clip_idx, anchor_idx, note_indices, original_starts, original_durations } = &self.drag {
                            let clip_idx = *clip_idx;
                            let anchor_idx = *anchor_idx;
                            let indices = note_indices.clone();
                            let orig_starts = original_starts.clone();
                            let orig_durs = original_durations.clone();
                            if clip_idx < self.midi_clips.len() && anchor_idx < self.midi_clips[clip_idx].notes.len() {
                                if let Some(ai) = indices.iter().position(|&ni| ni == anchor_idx) {
                                    let clip_x = self.midi_clips[clip_idx].position[0];
                                    let mc_gm = self.midi_clips[clip_idx].grid_mode;
                                    let mc_trip = self.midi_clips[clip_idx].triplet_grid;
                                    let snapped_x = if self.is_snap_override_active() {
                                        world[0]
                                    } else {
                                        snap_to_clip_grid(world[0], &self.settings, mc_gm, mc_trip, self.camera.zoom, self.bpm)
                                    };
                                    let anchor_new_start = (snapped_x - clip_x).max(0.0);
                                    let anchor_right = orig_starts[ai] + orig_durs[ai];
                                    let anchor_clamped = anchor_new_start.min(anchor_right - 10.0);
                                    let delta = anchor_clamped - orig_starts[ai];
                                    for (j, &ni) in indices.iter().enumerate() {
                                        if ni < self.midi_clips[clip_idx].notes.len() {
                                            let new_start = (orig_starts[j] + delta).max(0.0);
                                            let right_edge = orig_starts[j] + orig_durs[j];
                                            let clamped = new_start.min(right_edge - 10.0);
                                            self.midi_clips[clip_idx].notes[ni].start_px = clamped;
                                            self.midi_clips[clip_idx].notes[ni].duration_px = right_edge - clamped;
                                        }
                                    }
                                }
                                self.mark_dirty();
                            }
                        }
                        if let DragState::MovingMidiClip { clip_idx, offset } = &self.drag {
                            let clip_idx = *clip_idx;
                            let offset = *offset;
                            if clip_idx < self.midi_clips.len() {
                                let raw_x = world[0] - offset[0];
                                let snapped_x = if self.is_snap_override_active() {
                                    raw_x
                                } else {
                                    snap_to_grid(raw_x, &self.settings, self.camera.zoom, self.bpm)
                                };
                                self.midi_clips[clip_idx].position = [snapped_x, world[1] - offset[1]];
                                self.mark_dirty();
                                self.sync_audio_clips();
                            }
                        }
                        if let DragState::SelectingMidiNotes { clip_idx, start_world } = &self.drag {
                            let clip_idx = *clip_idx;
                            let start = *start_world;
                            if clip_idx < self.midi_clips.len() {
                                let mc_pos = self.midi_clips[clip_idx].position;
                                let mc_size = self.midi_clips[clip_idx].size;
                                // Compute selection rect, clamped to clip bounds
                                let rx = start[0].min(world[0]).max(mc_pos[0]);
                                let ry = start[1].min(world[1]).max(mc_pos[1]);
                                let rx2 = start[0].max(world[0]).min(mc_pos[0] + mc_size[0]);
                                let ry2 = start[1].max(world[1]).min(mc_pos[1] + mc_size[1]);
                                let rw = (rx2 - rx).max(0.0);
                                let rh = (ry2 - ry).max(0.0);
                                self.midi_note_select_rect = Some([rx, ry, rw, rh]);
                                let editing = self.editing_midi_clip == Some(clip_idx);
                                let nh = self.midi_clips[clip_idx].note_height_editing(editing);
                                let mut selected = Vec::new();
                                for (i, note) in self.midi_clips[clip_idx].notes.iter().enumerate() {
                                    let nx = mc_pos[0] + note.start_px;
                                    let ny = self.midi_clips[clip_idx].pitch_to_y_editing(note.pitch, editing);
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
                        if let DragState::DraggingVelocity { clip_idx, note_indices, original_velocities, start_world_y } = &self.drag {
                            let clip_idx = *clip_idx;
                            let indices = note_indices.clone();
                            let orig_vels = original_velocities.clone();
                            let start_y = *start_world_y;
                            if clip_idx < self.midi_clips.len() {
                                let lane_height = self.midi_clips[clip_idx].velocity_lane_height;
                                let delta_y = start_y - world[1];
                                let vel_delta = (delta_y / lane_height * 127.0) as i16;
                                for (j, &ni) in indices.iter().enumerate() {
                                    if ni < self.midi_clips[clip_idx].notes.len() {
                                        let new_vel = (orig_vels[j] as i16 + vel_delta).clamp(0, 127) as u8;
                                        self.midi_clips[clip_idx].notes[ni].velocity = new_vel;
                                    }
                                }
                                self.mark_dirty();
                            }
                        }
                        if let DragState::ResizingVelocityLane { clip_idx, start_world_y, original_height } = &self.drag {
                            let clip_idx = *clip_idx;
                            let start_y = *start_world_y;
                            let orig_h = *original_height;
                            if clip_idx < self.midi_clips.len() {
                                let delta_y = start_y - world[1];
                                let new_height = (orig_h + delta_y)
                                    .clamp(midi::VELOCITY_LANE_MIN_HEIGHT, midi::VELOCITY_LANE_MAX_HEIGHT);
                                self.midi_clips[clip_idx].velocity_lane_height = new_height;
                                self.mark_dirty();
                            }
                        }
                    }
                }

                self.update_hover();
                self.request_redraw();
            }

            // --- mouse buttons ---
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Middle => match state {
                    ElementState::Pressed => {
                        if self.context_menu.is_some() {
                            return;
                        }
                        self.command_palette = None;
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
                                self.push_undo();
                                self.waveforms[wf_idx]
                                    .automation
                                    .lane_for_mut(param)
                                    .remove_point(pt_idx);
                                self.mark_dirty();
                                self.request_redraw();
                                return;
                            }
                        }

                        if self.sample_browser.visible {
                            let (_, sh, scale) = self.screen_info();
                            if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                                if let Some(idx) =
                                    self.sample_browser.item_at(self.mouse_pos, sh, scale)
                                {
                                    let entry = &self.sample_browser.entries[idx];
                                    self.browser_context_path = Some(entry.path.clone());
                                    self.context_menu = Some(ContextMenu::new(
                                        self.mouse_pos,
                                        MenuContext::BrowserEntry,
                                        &self.settings,
                                    ));
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        let world = self.camera.screen_to_world(self.mouse_pos);

                        if let Some(mc_idx) = self.editing_midi_clip {
                            if mc_idx < self.midi_clips.len() {
                                let mc = &self.midi_clips[mc_idx];
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
                                let has_effect_region = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::EffectRegion(_)));
                                let current_waveform_color = self
                                    .selected
                                    .iter()
                                    .find_map(|t| match t {
                                        HitTarget::Waveform(i) => Some(self.waveforms[*i].color),
                                        _ => None,
                                    });
                                MenuContext::Selection {
                                    has_waveforms,
                                    has_effect_region,
                                    current_waveform_color,
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
                        if self.editing_bpm.is_some() {
                            let (sw, sh, scale) = self.screen_info();
                            if !TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                                self.editing_bpm = None;
                                self.request_redraw();
                            }
                        }

                        // Plugin editor click
                        if self.plugin_editor.is_some() {
                            let (scr_w, scr_h, scale) = self.screen_info();
                            let inside = self.plugin_editor.as_ref().map_or(false, |pe| {
                                pe.contains(self.mouse_pos, scr_w, scr_h, scale)
                            });
                            if inside {
                                let slider_hit = self.plugin_editor.as_ref().and_then(|pe| {
                                    pe.slider_hit_test(self.mouse_pos, scr_w, scr_h, scale)
                                });
                                if let Some(idx) = slider_hit {
                                    if let Some(pe) = &mut self.plugin_editor {
                                        pe.dragging_slider = Some(idx);
                                        let new_val = pe.slider_drag(
                                            idx,
                                            self.mouse_pos[0],
                                            scr_w,
                                            scr_h,
                                            scale,
                                        );
                                        let pb_idx = pe.region_idx; // repurposed as plugin_block index
                                        if let Some(pb) = self.plugin_blocks.get(pb_idx) {
                                            if let Ok(guard) = pb.gui.lock() {
                                                if let Some(gui) = guard.as_ref() {
                                                    gui.set_parameter(idx, new_val as f64);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                self.plugin_editor = None;
                            }
                            self.request_redraw();
                            return;
                        }

                        // Settings window click
                        if self.settings_window.is_some() {
                            let (scr_w, scr_h, scale) = self.screen_info();
                            let inside = self.settings_window.as_ref().map_or(false, |sw| {
                                sw.contains(self.mouse_pos, scr_w, scr_h, scale)
                            });
                            if inside {
                                // Try audio dropdown interaction first
                                let prev_output_device = self.settings.audio_output_device.clone();
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
                                            AudioEngine::new_with_device(device_name);

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
                                self.context_menu = None;
                                self.execute_command(action);
                            } else {
                                self.context_menu = None;
                            }
                            self.request_redraw();
                            if inside {
                                return;
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
                                .map_or(false, |p| matches!(p.mode, PaletteMode::VolumeFader | PaletteMode::SampleVolumeFader));

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
                                if let Some((plugin_id, plugin_name)) = plugin_info {
                                    self.command_palette = None;
                                    if mode == PaletteMode::InstrumentPicker {
                                        self.add_instrument(&plugin_id, &plugin_name);
                                    } else {
                                        self.add_plugin_to_selected_effect_region(&plugin_id, &plugin_name);
                                    }
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
                                        if matches!(action, CommandAction::SetMasterVolume | CommandAction::SetSampleVolume | CommandAction::AddPlugin | CommandAction::AddInstrument) {
                                            self.execute_command(action);
                                        } else {
                                            self.command_palette = None;
                                            self.execute_command(action);
                                        }
                                    }
                                    Some(ClickResult::InlinePlugin { unique_id, name, is_instrument }) => {
                                        self.command_palette = None;
                                        if is_instrument {
                                            self.add_instrument(&unique_id, &name);
                                        } else {
                                            self.add_plugin_to_selected_effect_region(&unique_id, &name);
                                        }
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
                                } else if self.sample_browser.hit_add_button(self.mouse_pos, scale)
                                {
                                    self.open_add_folder_dialog();
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
                                                self.drag = DragState::DraggingFromBrowser {
                                                    path: entry.path.clone(),
                                                    filename: entry.name.clone(),
                                                };
                                            }
                                        }
                                        ui::browser::EntryKind::Plugin { unique_id } => {
                                            self.drag = DragState::DraggingPlugin {
                                                plugin_id: unique_id.clone(),
                                                plugin_name: entry.name.clone(),
                                            };
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
                                if TransportPanel::hit_record_button(self.mouse_pos, sw, sh, scale)
                                {
                                    self.toggle_recording();
                                } else if TransportPanel::hit_bpm(self.mouse_pos, sw, sh, scale) {
                                    let now = std::time::Instant::now();
                                    let elapsed = now.duration_since(self.last_click_time);
                                    let is_dbl = elapsed.as_millis() < 400;
                                    self.last_click_time = now;
                                    if is_dbl {
                                        self.editing_bpm = Some(String::new());
                                        self.dragging_bpm = None;
                                    } else {
                                        self.dragging_bpm = Some((self.bpm, self.mouse_pos[1]));
                                        self.editing_bpm = None;
                                    }
                                } else if let Some(engine) = &self.audio_engine {
                                    engine.toggle_playback();
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        let world = self.camera.screen_to_world(self.mouse_pos);
                        self.last_canvas_click_world = world;

                        // --- component def corner resize ---
                        for (ci, def) in self.components.iter().enumerate() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(def.position, def.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingComponentDef { comp_idx: ci, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- effect region corner resize ---
                        for (i, er) in self.effect_regions.iter().enumerate() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(er.position, er.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingEffectRegion { region_idx: i, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- instrument region corner resize ---
                        for (i, ir) in self.instrument_regions.iter().enumerate() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(ir.position, ir.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingInstrumentRegion { region_idx: i, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- midi clip corner resize ---
                        for (i, mc) in self.midi_clips.iter().enumerate() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(mc.position, mc.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingMidiClip { clip_idx: i, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- midi clip body move (when not editing notes) ---
                        if self.editing_midi_clip.is_none() {
                            let hit_clip = self.midi_clips.iter().enumerate().find(|(_, mc)| {
                                point_in_rect(world, mc.position, mc.size)
                            }).map(|(i, mc)| (i, mc.position));
                            if let Some((i, pos)) = hit_clip {
                                if self.camera.zoom >= MIDI_AUTO_EDIT_ZOOM_THRESHOLD {
                                    self.editing_midi_clip = Some(i);
                                    self.selected_midi_notes.clear();
                                    // Fall through to note-editing section below
                                } else {
                                    self.push_undo();
                                    let clip_idx = if self.modifiers.alt_key() {
                                        let mc = self.midi_clips[i].clone();
                                        self.midi_clips.push(mc);
                                        self.midi_clips.len() - 1
                                    } else {
                                        i
                                    };
                                    if !self.selected.contains(&HitTarget::MidiClip(clip_idx)) {
                                        self.selected.clear();
                                        self.selected.push(HitTarget::MidiClip(clip_idx));
                                    }
                                    let offset = [world[0] - pos[0], world[1] - pos[1]];
                                    self.drag = DragState::MovingMidiClip { clip_idx, offset };
                                    self.update_cursor();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        // --- export region corner resize ---
                        for (i, er) in self.export_regions.iter().enumerate() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(er.position, er.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingExportRegion { region_idx: i, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- export region render pill click ---
                        for er in &self.export_regions {
                            let pill_w = EXPORT_RENDER_PILL_W / self.camera.zoom;
                            let pill_h = EXPORT_RENDER_PILL_H / self.camera.zoom;
                            let pill_x = er.position[0] + 4.0 / self.camera.zoom;
                            let pill_y = er.position[1] + 4.0 / self.camera.zoom;
                            if point_in_rect(world, [pill_x, pill_y], [pill_w, pill_h]) {
                                self.trigger_export_render();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- loop region corner resize ---
                        for (i, lr) in self.loop_regions.iter().enumerate() {
                            if !lr.enabled {
                                continue;
                            }
                            if let Some((anchor, nwse)) = hit_test_corner_resize(lr.position, lr.size, world, self.camera.zoom) {
                                self.push_undo();
                                self.drag = DragState::ResizingLoopRegion { region_idx: i, anchor, nwse };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- waveform edge resize ---
                        match hit_test_waveform_edge(&self.waveforms, world, &self.camera) {
                            WaveformEdgeHover::LeftEdge(i) | WaveformEdgeHover::RightEdge(i) => {
                                let is_left = matches!(self.waveform_edge_hover, WaveformEdgeHover::LeftEdge(_));
                                let pos_x = self.waveforms[i].position[0];
                                let size_w = self.waveforms[i].size[0];
                                let offset = self.waveforms[i].sample_offset_px;
                                self.push_undo();
                                self.drag = DragState::ResizingWaveform {
                                    waveform_idx: i,
                                    is_left_edge: is_left,
                                    initial_position_x: pos_x,
                                    initial_size_w: size_w,
                                    initial_offset_px: offset,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                            WaveformEdgeHover::None => {}
                        }

                        // Check automation lane close (×) button
                        if self.automation_mode {
                            if let Some(gpu) = &self.gpu {
                                for &(wf_idx, rect) in &gpu.auto_lane_close_rects {
                                    let [rx, ry, rw, rh] = rect;
                                    if self.mouse_pos[0] >= rx && self.mouse_pos[0] <= rx + rw
                                        && self.mouse_pos[1] >= ry && self.mouse_pos[1] <= ry + rh
                                    {
                                        self.push_undo();
                                        let param = self.active_automation_param;
                                        self.waveforms[wf_idx].automation.lane_for_mut(param).points.clear();
                                        self.mark_dirty();
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
                                let orig_t = self.waveforms[wf_idx].automation.lane_for(param).points[pt_idx].t;
                                let orig_v = self.waveforms[wf_idx].automation.lane_for(param).points[pt_idx].value;
                                self.push_undo();
                                self.drag = DragState::DraggingAutomationPoint {
                                    waveform_idx: wf_idx,
                                    param,
                                    point_idx: pt_idx,
                                    original_t: orig_t,
                                    original_value: orig_v,
                                };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                            // Check line segment for inserting new point
                            if let Some((wf_idx, t, value)) =
                                hit_test_automation_line(&self.waveforms, world, &self.camera, param)
                            {
                                self.push_undo();
                                let pt_idx = self.waveforms[wf_idx]
                                    .automation
                                    .lane_for_mut(param)
                                    .insert_point(t, value);
                                self.drag = DragState::DraggingAutomationPoint {
                                    waveform_idx: wf_idx,
                                    param,
                                    point_idx: pt_idx,
                                    original_t: t,
                                    original_value: value,
                                };
                                self.mark_dirty();
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                            // Click inside waveform to create new point
                            for (i, wf) in self.waveforms.iter().enumerate().rev() {
                                if point_in_rect(world, wf.position, wf.size) {
                                    let t = ((world[0] - wf.position[0]) / wf.size[0]).clamp(0.0, 1.0);
                                    let y_top = wf.position[1];
                                    let y_bot = wf.position[1] + wf.size[1];
                                    let value = ((world[1] - y_bot) / (y_top - y_bot)).clamp(0.0, 1.0);
                                    self.push_undo();
                                    let pt_idx = self.waveforms[i]
                                        .automation
                                        .lane_for_mut(param)
                                        .insert_point(t, value);
                                    self.drag = DragState::DraggingAutomationPoint {
                                        waveform_idx: i,
                                        param,
                                        point_idx: pt_idx,
                                        original_t: t,
                                        original_value: value,
                                    };
                                    self.mark_dirty();
                                    self.update_cursor();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        // --- fade handle drag ---
                        if let Some((wf_idx, is_fade_in)) =
                            hit_test_fade_handle(&self.waveforms, world, &self.camera)
                        {
                            self.push_undo();
                            self.drag = DragState::DraggingFade {
                                waveform_idx: wf_idx,
                                is_fade_in,
                            };
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- fade curve shape drag ---
                        if let Some((wf_idx, is_fade_in)) =
                            hit_test_fade_curve_dot(&self.waveforms, world, &self.camera)
                        {
                            let wf = &self.waveforms[wf_idx];
                            let start_curve = if is_fade_in { wf.fade_in_curve } else { wf.fade_out_curve };
                            self.push_undo();
                            self.drag = DragState::DraggingFadeCurve {
                                waveform_idx: wf_idx,
                                is_fade_in,
                                start_mouse_y: self.mouse_pos[1],
                                start_curve,
                            };
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

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

                        // Double-click detection: enter component edit mode
                        let now = std::time::Instant::now();
                        let elapsed = now.duration_since(self.last_click_time);
                        let dist = ((world[0] - self.last_click_world[0]).powi(2)
                            + (world[1] - self.last_click_world[1]).powi(2))
                        .sqrt();
                        let is_double_click =
                            elapsed.as_millis() < 400 && dist < 10.0 / self.camera.zoom;
                        self.last_click_time = now;
                        self.last_click_world = world;

                        if is_double_click {
                            if let Some(HitTarget::ComponentDef(ci)) = hit {
                                self.editing_component = Some(ci);
                                self.selected.clear();
                                println!(
                                    "Entered component edit mode: {}",
                                    self.components[ci].name
                                );
                                self.request_redraw();
                                return;
                            }
                            if let Some(HitTarget::PluginBlock(idx)) = hit {
                                self.open_plugin_block_gui(idx);
                                self.request_redraw();
                                return;
                            }
                            if let Some(HitTarget::MidiClip(idx)) = hit {
                                if self.editing_midi_clip == Some(idx) {
                                    self.select_area = None;
                                    self.selected.clear();
                                    let mc = &self.midi_clips[idx];
                                    // Only create notes in the note area, not the velocity lane
                                    let in_vel_lane = world[1] >= mc.velocity_lane_top();
                                    let hit_note = midi::hit_test_midi_note_editing(mc, world, &self.camera, true);
                                    if hit_note.is_none() && !in_vel_lane {
                                        self.push_undo();
                                        let pitch = self.midi_clips[idx].y_to_pitch_editing(world[1], true);
                                        let start_px = self.midi_clips[idx].x_to_start_px(world[0]);
                                        self.midi_clips[idx].notes.push(midi::MidiNote {
                                            pitch,
                                            start_px,
                                            duration_px: midi::DEFAULT_NOTE_DURATION_PX,
                                            velocity: 100,
                                        });
                                        let new_idx = self.midi_clips[idx].notes.len() - 1;
                                        self.selected_midi_notes = vec![new_idx];
                                        self.mark_dirty();
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
                            if let Some(HitTarget::InstrumentRegion(idx)) = hit {
                                if self.instrument_regions[idx].has_plugin() {
                                    self.open_instrument_region_gui(idx);
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        // Click outside editing MIDI clip exits edit mode
                        if let Some(mc_idx) = self.editing_midi_clip {
                            if mc_idx < self.midi_clips.len() {
                                let mc = &self.midi_clips[mc_idx];
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
                            if mc_idx < self.midi_clips.len() {
                                let mc_pos = self.midi_clips[mc_idx].position;
                                let mc_size = self.midi_clips[mc_idx].size;
                                if point_in_rect(world, mc_pos, mc_size) {
                                    self.select_area = None;
                                    self.selected.clear();

                                    // Seek playback to clicked position
                                    let snapped_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
                                    if let Some(engine) = &self.audio_engine {
                                        let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                        engine.seek_to_seconds(secs);
                                    }

                                    // Check velocity lane divider first (for resizing)
                                    if midi::hit_test_velocity_divider(&self.midi_clips[mc_idx], world, &self.camera) {
                                        self.drag = DragState::ResizingVelocityLane {
                                            clip_idx: mc_idx,
                                            start_world_y: world[1],
                                            original_height: self.midi_clips[mc_idx].velocity_lane_height,
                                        };
                                        self.update_cursor();
                                        self.request_redraw();
                                        return;
                                    }

                                    // Check velocity bar
                                    let vel_hit = midi::hit_test_velocity_bar(&self.midi_clips[mc_idx], world, &self.camera);
                                    if let Some(note_idx) = vel_hit {
                                        if self.selected_midi_notes.contains(&note_idx) {
                                            // already selected
                                        } else if self.modifiers.shift_key() {
                                            self.selected_midi_notes.push(note_idx);
                                        } else {
                                            self.selected_midi_notes.clear();
                                            self.selected_midi_notes.push(note_idx);
                                        }
                                        self.push_undo();
                                        let indices = self.selected_midi_notes.clone();
                                        let velocities: Vec<u8> = indices.iter().map(|&ni| {
                                            self.midi_clips[mc_idx].notes[ni].velocity
                                        }).collect();
                                        self.drag = DragState::DraggingVelocity {
                                            clip_idx: mc_idx,
                                            note_indices: indices,
                                            original_velocities: velocities,
                                            start_world_y: world[1],
                                        };
                                        self.mark_dirty();
                                        self.request_redraw();
                                        return;
                                    }

                                    // Check if clicking on existing note (editing-aware)
                                    let hit_note = midi::hit_test_midi_note_editing(&self.midi_clips[mc_idx], world, &self.camera, true);
                                    if let Some((note_idx, zone)) = hit_note {
                                        if self.modifiers.super_key() && !matches!(zone, midi::MidiNoteHitZone::VelocityBar) {
                                            let indices = if self.selected_midi_notes.contains(&note_idx) {
                                                self.selected_midi_notes.clone()
                                            } else {
                                                self.selected_midi_notes.clear();
                                                self.selected_midi_notes.push(note_idx);
                                                vec![note_idx]
                                            };
                                            self.push_undo();
                                            let velocities: Vec<u8> = indices.iter().map(|&ni| {
                                                self.midi_clips[mc_idx].notes[ni].velocity
                                            }).collect();
                                            self.drag = DragState::DraggingVelocity {
                                                clip_idx: mc_idx,
                                                note_indices: indices,
                                                original_velocities: velocities,
                                                start_world_y: world[1],
                                            };
                                            self.mark_dirty();
                                            self.request_redraw();
                                            return;
                                        }
                                        match zone {
                                            midi::MidiNoteHitZone::RightEdge => {
                                                self.push_undo();
                                                let mut indices = self.selected_midi_notes.clone();
                                                if !indices.contains(&note_idx) {
                                                    indices = vec![note_idx];
                                                }
                                                let durations: Vec<f32> = indices.iter().map(|&ni| {
                                                    self.midi_clips[mc_idx].notes[ni].duration_px
                                                }).collect();
                                                self.drag = DragState::ResizingMidiNote {
                                                    clip_idx: mc_idx,
                                                    anchor_idx: note_idx,
                                                    note_indices: indices,
                                                    original_durations: durations,
                                                };
                                            }
                                            midi::MidiNoteHitZone::LeftEdge => {
                                                self.push_undo();
                                                let mut indices = self.selected_midi_notes.clone();
                                                if !indices.contains(&note_idx) {
                                                    indices = vec![note_idx];
                                                }
                                                let starts: Vec<f32> = indices.iter().map(|&ni| {
                                                    self.midi_clips[mc_idx].notes[ni].start_px
                                                }).collect();
                                                let durations: Vec<f32> = indices.iter().map(|&ni| {
                                                    self.midi_clips[mc_idx].notes[ni].duration_px
                                                }).collect();
                                                self.drag = DragState::ResizingMidiNoteLeft {
                                                    clip_idx: mc_idx,
                                                    anchor_idx: note_idx,
                                                    note_indices: indices,
                                                    original_starts: starts,
                                                    original_durations: durations,
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
                                                    self.push_undo();
                                                    let mut new_indices: Vec<usize> = Vec::new();
                                                    for &ni in &self.selected_midi_notes {
                                                        if ni < self.midi_clips[mc_idx].notes.len() {
                                                            let cloned = self.midi_clips[mc_idx].notes[ni].clone();
                                                            self.midi_clips[mc_idx].notes.push(cloned);
                                                            new_indices.push(self.midi_clips[mc_idx].notes.len() - 1);
                                                        }
                                                    }
                                                    self.selected_midi_notes = new_indices.clone();
                                                    let nh = self.midi_clips[mc_idx].note_height_editing(true);
                                                    let offsets: Vec<[f32; 2]> = new_indices.iter().map(|&ni| {
                                                        let n = &self.midi_clips[mc_idx].notes[ni];
                                                        let nx = mc_pos[0] + n.start_px;
                                                        let ny = self.midi_clips[mc_idx].pitch_to_y_editing(n.pitch, true) + nh * 0.5;
                                                        [world[0] - nx, world[1] - ny]
                                                    }).collect();
                                                    self.drag = DragState::MovingMidiNote {
                                                        clip_idx: mc_idx,
                                                        note_indices: new_indices,
                                                        offsets,
                                                        start_world: world,
                                                    };
                                                } else {
                                                    self.push_undo();
                                                    let nh = self.midi_clips[mc_idx].note_height_editing(true);
                                                    let offsets: Vec<[f32; 2]> = self.selected_midi_notes.iter().map(|&ni| {
                                                        let n = &self.midi_clips[mc_idx].notes[ni];
                                                        let nx = mc_pos[0] + n.start_px;
                                                        let ny = self.midi_clips[mc_idx].pitch_to_y_editing(n.pitch, true) + nh * 0.5;
                                                        [world[0] - nx, world[1] - ny]
                                                    }).collect();
                                                    self.drag = DragState::MovingMidiNote {
                                                        clip_idx: mc_idx,
                                                        note_indices: self.selected_midi_notes.clone(),
                                                        offsets,
                                                        start_world: world,
                                                    };
                                                }
                                            }
                                            midi::MidiNoteHitZone::VelocityBar => unreachable!(),
                                        }
                                    } else {
                                        self.selected_midi_notes.clear();
                                        self.midi_note_select_rect = None;
                                        self.drag = DragState::SelectingMidiNotes {
                                            clip_idx: mc_idx,
                                            start_world: world,
                                        };
                                    }
                                    self.mark_dirty();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        // Click outside the editing component exits edit mode
                        if let Some(ec_idx) = self.editing_component {
                            if let Some(def) = self.components.get(ec_idx) {
                                if !point_in_rect(world, def.position, def.size) {
                                    self.editing_component = None;
                                    self.selected.clear();
                                    println!("Exited component edit mode");
                                    // Re-do hit test without edit mode
                                    let hit2 = hit_test(
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
                                        None,
                                        world,
                                        &self.camera,
                                    );
                                    if let Some(target) = hit2 {
                                        self.selected.push(target);
                                        self.begin_move_selection(world, self.modifiers.alt_key());
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
                            Some(target) => {
                                self.select_area = None;
                                if self.selected.contains(&target) {
                                    // Already selected -> drag whole selection
                                } else {
                                    self.selected.clear();
                                    self.selected.push(target);
                                }
                                self.begin_move_selection(world, self.modifiers.alt_key());
                            }
                            None => {
                                self.drag = DragState::Selecting { start_world: world };
                            }
                        }

                        self.update_cursor();
                        self.request_redraw();
                    }

                    ElementState::Released => {
                        // Finish plugin editor slider drag
                        if let Some(pe) = &mut self.plugin_editor {
                            if pe.dragging_slider.is_some() {
                                pe.dragging_slider = None;
                                self.request_redraw();
                                return;
                            }
                        }

                        // Finish settings slider drag
                        if let Some(sw) = &mut self.settings_window {
                            if sw.dragging_slider.is_some() {
                                sw.dragging_slider = None;
                                self.settings.save();
                                self.request_redraw();
                                return;
                            }
                        }

                        if self.dragging_bpm.is_some() {
                            self.dragging_bpm = None;
                            self.bpm = self.bpm.round();
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
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_cursor();
                            self.request_redraw();
                            return;
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
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing effect region ---
                        if matches!(self.drag, DragState::ResizingEffectRegion { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing instrument region ---
                        if matches!(self.drag, DragState::ResizingInstrumentRegion { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish MIDI note drag/resize ---
                        if matches!(self.drag, DragState::MovingMidiNote { .. } | DragState::ResizingMidiNote { .. } | DragState::ResizingMidiNoteLeft { .. } | DragState::ResizingMidiClip { .. }) {
                            if let Some(note_idx) = self.pending_midi_note_click.take() {
                                self.undo();
                                self.selected_midi_notes = vec![note_idx];
                            } else {
                                let (clip_idx, note_indices) = match &self.drag {
                                    DragState::MovingMidiNote { clip_idx, note_indices, .. } => (*clip_idx, note_indices.clone()),
                                    DragState::ResizingMidiNote { clip_idx, note_indices, .. } => (*clip_idx, note_indices.clone()),
                                    DragState::ResizingMidiNoteLeft { clip_idx, note_indices, .. } => (*clip_idx, note_indices.clone()),
                                    _ => (0, vec![]),
                                };
                                if clip_idx < self.midi_clips.len() && !note_indices.is_empty() {
                                    let new_indices = self.midi_clips[clip_idx].resolve_note_overlaps(&note_indices);
                                    self.selected_midi_notes = new_indices;
                                }
                            }
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish velocity drag ---
                        if matches!(self.drag, DragState::DraggingVelocity { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_cursor();
                            self.request_redraw();
                            return;
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
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish MIDI note selection drag ---
                        if matches!(self.drag, DragState::SelectingMidiNotes { .. }) {
                            self.drag = DragState::None;
                            self.midi_note_select_rect = None;
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing export region ---
                        if matches!(self.drag, DragState::ResizingExportRegion { .. }) {
                            self.drag = DragState::None;
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish resizing loop region ---
                        if matches!(self.drag, DragState::ResizingLoopRegion { .. }) {
                            self.drag = DragState::None;
                            self.sync_loop_region();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish fade handle drag ---
                        if matches!(self.drag, DragState::DraggingFade { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
                            self.request_redraw();
                            return;
                        }

                        // --- finish fade curve drag ---
                        if matches!(self.drag, DragState::DraggingFadeCurve { .. }) {
                            self.drag = DragState::None;
                            self.sync_audio_clips();
                            self.update_hover();
                            self.update_cursor();
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
                        } = self.drag
                        {
                            let plugin_id = plugin_id.clone();
                            let plugin_name = plugin_name.clone();
                            let (_, sh, scale) = self.screen_info();
                            let in_browser = self.sample_browser.visible
                                && self.sample_browser.contains(self.mouse_pos, sh, scale);
                            if !in_browser {
                                let world = self.camera.screen_to_world(self.mouse_pos);
                                let hit_er = self
                                    .effect_regions
                                    .iter()
                                    .enumerate()
                                    .rev()
                                    .find(|(_, er)| point_in_rect(world, er.position, er.size))
                                    .map(|(i, _)| i);

                                self.push_undo();
                                self.add_plugin_block(world, &plugin_id, &plugin_name);
                                let idx = self.plugin_blocks.len() - 1;
                                self.selected.clear();
                                self.selected.push(HitTarget::PluginBlock(idx));
                            }
                            self.drag = DragState::None;
                            self.update_hover();
                            self.request_redraw();
                            return;
                        }

                        if let DragState::Selecting { start_world } = &self.drag {
                            let start = *start_world;
                            let current = self.camera.screen_to_world(self.mouse_pos);
                            let (rp, rs) = canonical_rect(start, current);

                            let min_sz = 5.0 / self.camera.zoom;
                            if rs[0] < min_sz && rs[1] < min_sz {
                                self.selected.clear();
                                let snapped_x = snap_to_grid(current[0], &self.settings, self.camera.zoom, self.bpm);
                                if let Some(engine) = &self.audio_engine {
                                    let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                    engine.seek_to_seconds(secs);
                                }
                                let (_, sh, _) = self.screen_info();
                                let world_top = self.camera.screen_to_world([0.0, 0.0])[1];
                                let world_bottom = self.camera.screen_to_world([0.0, sh])[1];
                                let line_w = 2.0 / self.camera.zoom;
                                self.select_area = Some(SelectArea {
                                    position: [snapped_x, world_top],
                                    size: [line_w, world_bottom - world_top],
                                });
                            } else {
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
                                self.select_area = Some(SelectArea { position: rp, size: rs });
                            }
                        }

                        self.drag = DragState::None;
                        self.sync_audio_clips();
                        self.update_hover();
                        self.request_redraw();
                    }
                },
                _ => {}
            },

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                self.update_hover();
                self.update_cursor();
                self.request_redraw();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    println!("[KEY] pressed: {:?} super={} shift={}", event.logical_key, self.modifiers.super_key(), self.modifiers.shift_key());
                    if self.plugin_editor.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.plugin_editor = None;
                            self.request_redraw();
                            return;
                        }
                        return;
                    }

                    if self.settings_window.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.settings_window = None;
                            self.request_redraw();
                            return;
                        }
                        // Block other keyboard input while settings is open
                        if !self.modifiers.super_key() {
                            return;
                        }
                    }

                    if self.context_menu.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.context_menu = None;
                            self.request_redraw();
                            return;
                        }
                    }

                    if self.editing_component.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.editing_component = None;
                            self.selected.clear();
                            println!("Exited component edit mode");
                            self.request_redraw();
                            return;
                        }
                    }

                    if self.editing_midi_clip.is_some() {
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.editing_midi_clip = None;
                            self.selected_midi_notes.clear();
                            println!("Exited MIDI clip edit mode");
                            self.request_redraw();
                            return;
                        }
                        // Delete selected MIDI notes
                        if matches!(event.logical_key, Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace)) {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if mc_idx < self.midi_clips.len() && !self.selected_midi_notes.is_empty() {
                                    self.push_undo();
                                    let mut indices = self.selected_midi_notes.clone();
                                    indices.sort_unstable_by(|a, b| b.cmp(a));
                                    for &i in &indices {
                                        if i < self.midi_clips[mc_idx].notes.len() {
                                            self.midi_clips[mc_idx].notes.remove(i);
                                        }
                                    }
                                    self.selected_midi_notes.clear();
                                    self.sync_audio_clips();
                                    self.mark_dirty();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }
                        // Cmd+D: duplicate selected MIDI notes
                        if matches!(&event.logical_key, Key::Character(ch) if ch.as_ref() == "d") && self.modifiers.super_key() {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if mc_idx < self.midi_clips.len() && !self.selected_midi_notes.is_empty() {
                                    self.push_undo();
                                    let notes = &self.midi_clips[mc_idx].notes;
                                    // Compute group span: shift = max_end - min_start
                                    let min_start = self.selected_midi_notes.iter()
                                        .filter(|&&ni| ni < notes.len())
                                        .map(|&ni| notes[ni].start_px)
                                        .fold(f32::INFINITY, f32::min);
                                    let max_end = self.selected_midi_notes.iter()
                                        .filter(|&&ni| ni < notes.len())
                                        .map(|&ni| notes[ni].start_px + notes[ni].duration_px)
                                        .fold(f32::NEG_INFINITY, f32::max);
                                    let group_shift = max_end - min_start;
                                    let mut new_indices: Vec<usize> = Vec::new();
                                    for &ni in &self.selected_midi_notes {
                                        if ni < self.midi_clips[mc_idx].notes.len() {
                                            let mut cloned = self.midi_clips[mc_idx].notes[ni].clone();
                                            cloned.start_px += group_shift;
                                            self.midi_clips[mc_idx].notes.push(cloned);
                                            new_indices.push(self.midi_clips[mc_idx].notes.len() - 1);
                                        }
                                    }
                                    self.selected_midi_notes = self.midi_clips[mc_idx].resolve_note_overlaps(&new_indices);
                                    self.sync_audio_clips();
                                    self.mark_dirty();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }
                        // Left/Right: move notes; Shift+Left/Right: resize note duration
                        if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight)) {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if mc_idx < self.midi_clips.len() && !self.selected_midi_notes.is_empty() {
                                    let mc = &self.midi_clips[mc_idx];
                                    let step = grid::clip_grid_spacing(mc.grid_mode, mc.triplet_grid, self.camera.zoom, self.bpm);
                                    let delta = if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) { step } else { -step };
                                    if self.modifiers.shift_key() {
                                        // Resize duration
                                        let min_dur = step;
                                        let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                            if ni >= self.midi_clips[mc_idx].notes.len() { return false; }
                                            self.midi_clips[mc_idx].notes[ni].duration_px + delta >= min_dur
                                        });
                                        if all_valid {
                                            self.push_undo();
                                            for &ni in &self.selected_midi_notes {
                                                if ni < self.midi_clips[mc_idx].notes.len() {
                                                    self.midi_clips[mc_idx].notes[ni].duration_px += delta;
                                                }
                                            }
                                            self.selected_midi_notes = self.midi_clips[mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                            self.sync_audio_clips();
                                            self.mark_dirty();
                                            self.request_redraw();
                                            return;
                                        }
                                    } else {
                                        // Move position
                                        let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                            if ni >= self.midi_clips[mc_idx].notes.len() { return false; }
                                            self.midi_clips[mc_idx].notes[ni].start_px + delta >= 0.0
                                        });
                                        if all_valid {
                                            self.push_undo();
                                            for &ni in &self.selected_midi_notes {
                                                if ni < self.midi_clips[mc_idx].notes.len() {
                                                    self.midi_clips[mc_idx].notes[ni].start_px += delta;
                                                }
                                            }
                                            self.selected_midi_notes = self.midi_clips[mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                            self.sync_audio_clips();
                                            self.mark_dirty();
                                            self.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        // Transpose selected notes by semitone with Up/Down arrows
                        // Shift+Up/Down transposes by an octave (12 semitones)
                        if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown)) {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if mc_idx < self.midi_clips.len() && !self.selected_midi_notes.is_empty() {
                                    let delta: i16 = if self.modifiers.shift_key() { 12 } else { 1 };
                                    let delta = if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp)) { delta } else { -delta };
                                    // Check if all notes stay in valid range (0..=127)
                                    let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                        if ni >= self.midi_clips[mc_idx].notes.len() { return false; }
                                        let new_pitch = self.midi_clips[mc_idx].notes[ni].pitch as i16 + delta;
                                        (0..=127).contains(&new_pitch)
                                    });
                                    if all_valid {
                                        self.push_undo();
                                        for &ni in &self.selected_midi_notes {
                                            if ni < self.midi_clips[mc_idx].notes.len() {
                                                self.midi_clips[mc_idx].notes[ni].pitch =
                                                    (self.midi_clips[mc_idx].notes[ni].pitch as i16 + delta) as u8;
                                            }
                                        }
                                        self.selected_midi_notes = self.midi_clips[mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                        self.sync_audio_clips();
                                        self.mark_dirty();
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    // --- BPM editing input ---
                    if self.editing_bpm.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_bpm = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some(text) = self.editing_bpm.take() {
                                    if let Ok(val) = text.parse::<f32>() {
                                        self.bpm = val.clamp(20.0, 999.0);
                                        self.mark_dirty();
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(ref mut text) = self.editing_bpm {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
                                    if let Some(ref mut text) = self.editing_bpm {
                                        text.push_str(s);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- effect region name editing input ---
                    if self.editing_effect_name.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_effect_name = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((idx, text)) = self.editing_effect_name.take() {
                                    if idx < self.effect_regions.len() {
                                        self.push_undo();
                                        let name = if text.trim().is_empty() {
                                            "effects".to_string()
                                        } else {
                                            text
                                        };
                                        self.effect_regions[idx].name = name;
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.push(' ');
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some((_, ref mut text)) = self.editing_effect_name {
                                    text.push_str(ch.as_ref());
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- waveform name editing input ---
                    if self.editing_waveform_name.is_some() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_waveform_name = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some((idx, text)) = self.editing_waveform_name.take() {
                                    if idx < self.waveforms.len() {
                                        self.push_undo();
                                        let name = if text.trim().is_empty() {
                                            self.waveforms[idx].audio.filename.clone()
                                        } else {
                                            text
                                        };
                                        let mut new_audio = (*self.waveforms[idx].audio).clone();
                                        new_audio.filename = name;
                                        self.waveforms[idx].audio = Arc::new(new_audio);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.pop();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.push(' ');
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some((_, ref mut text)) = self.editing_waveform_name {
                                    text.push_str(ch.as_ref());
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- command palette input ---
                    if self.command_palette.is_some() {
                        let fader_mode = self
                            .command_palette
                            .as_ref()
                            .map(|p| p.mode);

                        if matches!(fader_mode, Some(PaletteMode::VolumeFader)) {
                            match &event.logical_key {
                                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                                    self.command_palette = None;
                                    self.request_redraw();
                                    return;
                                }
                                _ => {
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        if matches!(fader_mode, Some(PaletteMode::SampleVolumeFader)) {
                            match &event.logical_key {
                                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                                    self.command_palette = None;
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::ArrowUp) => {
                                    if let Some(p) = &mut self.command_palette {
                                        let db = if p.fader_value < 0.00001 { -60.0 } else { gain_to_db(p.fader_value) };
                                        let new_db = (db + 1.0).min(6.0);
                                        p.fader_value = db_to_gain(new_db);
                                        if let Some(idx) = p.fader_target_waveform {
                                            if idx < self.waveforms.len() {
                                                self.waveforms[idx].volume = p.fader_value;
                                                self.sync_audio_clips();
                                            }
                                        }
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::ArrowDown) => {
                                    if let Some(p) = &mut self.command_palette {
                                        let db = if p.fader_value < 0.00001 { -60.0 } else { gain_to_db(p.fader_value) };
                                        let new_db = db - 1.0;
                                        p.fader_value = if new_db <= -60.0 { 0.0 } else { db_to_gain(new_db) };
                                        if let Some(idx) = p.fader_target_waveform {
                                            if idx < self.waveforms.len() {
                                                self.waveforms[idx].volume = p.fader_value;
                                                self.sync_audio_clips();
                                            }
                                        }
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                _ => {
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        if matches!(fader_mode, Some(PaletteMode::PluginPicker | PaletteMode::InstrumentPicker)) {
                            match &event.logical_key {
                                Key::Named(NamedKey::Escape) => {
                                    self.command_palette = None;
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::ArrowUp) => {
                                    let (_, _, scale) = self.screen_info();
                                    if let Some(p) = &mut self.command_palette {
                                        p.move_plugin_selection(-1, scale);
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::ArrowDown) => {
                                    let (_, _, scale) = self.screen_info();
                                    if let Some(p) = &mut self.command_palette {
                                        p.move_plugin_selection(1, scale);
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::Enter) => {
                                    let is_instrument = matches!(fader_mode, Some(PaletteMode::InstrumentPicker));
                                    let plugin_info = self
                                        .command_palette
                                        .as_ref()
                                        .and_then(|p| p.selected_plugin())
                                        .map(|e| (e.unique_id.clone(), e.name.clone()));
                                    self.command_palette = None;
                                    if let Some((plugin_id, plugin_name)) = plugin_info {
                                        if is_instrument {
                                            self.add_instrument(&plugin_id, &plugin_name);
                                        } else {
                                            self.add_plugin_to_selected_effect_region(&plugin_id, &plugin_name);
                                        }
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::Backspace) => {
                                    if let Some(p) = &mut self.command_palette {
                                        p.search_text.pop();
                                        p.update_filter(self.settings.dev_mode);
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Named(NamedKey::Space) => {
                                    if let Some(p) = &mut self.command_palette {
                                        p.search_text.push(' ');
                                        p.update_filter(self.settings.dev_mode);
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                Key::Character(ch) if !self.modifiers.super_key() => {
                                    if let Some(p) = &mut self.command_palette {
                                        p.search_text.push_str(ch.as_ref());
                                        p.update_filter(self.settings.dev_mode);
                                    }
                                    self.request_redraw();
                                    return;
                                }
                                _ => {
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }

                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.command_palette = None;
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.move_selection(-1);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.move_selection(1);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                // Check if an inline plugin row is selected
                                let inline_plugin = self
                                    .command_palette
                                    .as_ref()
                                    .and_then(|p| p.selected_inline_plugin())
                                    .map(|e| (e.unique_id.clone(), e.name.clone(), e.is_instrument));
                                if let Some((plugin_id, plugin_name, is_instrument)) = inline_plugin {
                                    self.command_palette = None;
                                    if is_instrument {
                                        self.add_instrument(&plugin_id, &plugin_name);
                                    } else {
                                        self.add_plugin_to_selected_effect_region(&plugin_id, &plugin_name);
                                    }
                                    self.request_redraw();
                                    return;
                                }

                                let action = self
                                    .command_palette
                                    .as_ref()
                                    .and_then(|p| p.selected_action());
                                if let Some(a) = action {
                                    if matches!(a, CommandAction::SetMasterVolume | CommandAction::SetSampleVolume | CommandAction::AddPlugin | CommandAction::AddInstrument) {
                                        self.execute_command(a);
                                    } else {
                                        self.command_palette = None;
                                        self.execute_command(a);
                                    }
                                } else {
                                    self.command_palette = None;
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.pop();
                                    p.update_filter(self.settings.dev_mode);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Space) => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.push(' ');
                                    p.update_filter(self.settings.dev_mode);
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                if let Some(p) = &mut self.command_palette {
                                    p.search_text.push_str(ch.as_ref());
                                    p.update_filter(self.settings.dev_mode);
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- Enter on selected effect region: show overlapping plugin info ---
                    if matches!(event.logical_key, Key::Named(NamedKey::Enter)) {
                        if let Some(HitTarget::EffectRegion(idx)) = self.selected.first().copied() {
                            if idx < self.effect_regions.len() {
                                let er = &self.effect_regions[idx];
                                let block_indices = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                                if block_indices.is_empty() {
                                    println!("  Effect region {} has no overlapping plugins", idx);
                                } else {
                                    println!("  Effect region {} plugin chain:", idx);
                                    for (j, &bi) in block_indices.iter().enumerate() {
                                        let pb = &self.plugin_blocks[bi];
                                        let param_count = pb
                                            .gui
                                            .lock()
                                            .ok()
                                            .and_then(|g| g.as_ref().map(|gui| gui.parameter_count()))
                                            .unwrap_or(0);
                                        println!(
                                            "    [{}] {} ({} params)",
                                            j, pb.plugin_name, param_count
                                        );
                                    }
                                }
                            }
                            self.request_redraw();
                        }
                        // Double-click on plugin block: open GUI
                        if let Some(HitTarget::PluginBlock(idx)) = self.selected.first().copied() {
                            if idx < self.plugin_blocks.len() {
                                self.open_plugin_block_gui(idx);
                            }
                            self.request_redraw();
                        }
                    }

                    // --- global shortcuts ---
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.selected.clear();
                            self.select_area = None;
                            self.request_redraw();
                        }
                        Key::Named(NamedKey::Space) => {
                            if self.is_recording() {
                                self.toggle_recording();
                                self.request_redraw();
                            } else if let Some(engine) = &self.audio_engine {
                                if !engine.is_playing() {
                                    if let Some(sa) = &self.select_area {
                                        let secs = sa.position[0] as f64 / PIXELS_PER_SECOND as f64;
                                        engine.seek_to_seconds(secs);
                                    }
                                }
                                engine.toggle_playback();
                                self.request_redraw();
                            }
                        }
                        Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete) => {
                            if !self.selected.is_empty() {
                                self.delete_selected();
                                self.request_redraw();
                            }
                        }
                        Key::Character(ch) if !self.modifiers.super_key() => match ch.as_ref() {
                            "0" => {
                                let wf_indices: Vec<usize> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::Waveform(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                let lr_indices: Vec<usize> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::LoopRegion(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                if !wf_indices.is_empty() || !lr_indices.is_empty() {
                                    self.push_undo();
                                    if !wf_indices.is_empty() {
                                        let any_enabled = wf_indices.iter().any(|&i| i < self.waveforms.len() && !self.waveforms[i].disabled);
                                        let new_disabled = any_enabled;
                                        for &i in &wf_indices {
                                            if i < self.waveforms.len() {
                                                self.waveforms[i].disabled = new_disabled;
                                            }
                                        }
                                    }
                                    if !lr_indices.is_empty() {
                                        let any_enabled = lr_indices.iter().any(|&i| i < self.loop_regions.len() && self.loop_regions[i].enabled);
                                        let new_enabled = !any_enabled;
                                        for &i in &lr_indices {
                                            if i < self.loop_regions.len() {
                                                self.loop_regions[i].enabled = new_enabled;
                                            }
                                        }
                                        self.sync_loop_region();
                                    }
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                }
                            }
                            _ => {}
                        },
                        Key::Character(ch) if self.modifiers.super_key() => match ch.as_ref() {
                            "," => {
                                self.command_palette = None;
                                self.context_menu = None;
                                self.settings_window = if self.settings_window.is_some() {
                                    None
                                } else {
                                    Some(SettingsWindow::new())
                                };
                                self.request_redraw();
                            }
                            "k" => {
                                self.context_menu = None;
                                self.settings_window = None;
                                self.command_palette = if self.command_palette.is_some() {
                                    None
                                } else {
                                    let mut p = CommandPalette::new(self.settings.dev_mode);
                                    p.plugin_entries = self.build_palette_plugin_entries();
                                    Some(p)
                                };
                                self.request_redraw();
                            }
                            "t" => {
                                self.context_menu = None;
                                self.settings_window = None;
                                self.command_palette = if self.command_palette.is_some() {
                                    None
                                } else {
                                    let mut p = CommandPalette::new(self.settings.dev_mode);
                                    p.plugin_entries = self.build_palette_plugin_entries();
                                    Some(p)
                                };
                                self.request_redraw();
                            }
                            "b" => {
                                self.sample_browser.visible = !self.sample_browser.visible;
                                if self.sample_browser.visible {
                                    self.ensure_plugins_scanned();
                                }
                                self.request_redraw();
                            }
                            "a" if self.modifiers.shift_key() => {
                                self.open_add_folder_dialog();
                            }
                            "r" => {
                                let has_er = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::EffectRegion(_)));
                                let has_wf = self
                                    .selected
                                    .iter()
                                    .any(|t| matches!(t, HitTarget::Waveform(_)));
                                if has_er {
                                    self.execute_command(CommandAction::RenameEffectRegion);
                                } else if has_wf {
                                    self.execute_command(CommandAction::RenameSample);
                                } else {
                                    self.toggle_recording();
                                }
                                self.request_redraw();
                            }
                            "c" => {
                                self.copy_selected();
                                self.request_redraw();
                            }
                            "v" => {
                                self.paste_clipboard();
                                self.request_redraw();
                            }
                            "d" => {
                                self.duplicate_selected();
                                self.request_redraw();
                            }
                            "e" => {
                                self.execute_command(CommandAction::SplitSample);
                            }
                            "l" => {
                                self.execute_command(CommandAction::AddLoopArea);
                            }
                            "s" => self.save_project(),
                            "z" => {
                                println!("[KEY] Cmd+Z pressed, shift={}", self.modifiers.shift_key());
                                if self.modifiers.shift_key() {
                                    self.redo();
                                } else {
                                    self.undo();
                                }
                            }
                            "1" => {
                                self.execute_command(CommandAction::NarrowGrid);
                            }
                            "2" => {
                                self.execute_command(CommandAction::WidenGrid);
                            }
                            "3" => {
                                self.execute_command(CommandAction::ToggleTripletGrid);
                            }
                            "4" => {
                                self.execute_command(CommandAction::ToggleSnapToGrid);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }

            // --- scroll = pan, Cmd+scroll = zoom, pinch = zoom ---
            WindowEvent::MouseWheel { delta, .. } => {
                if self.context_menu.is_some() {
                    return;
                }
                let is_pixel_delta = matches!(delta, MouseScrollDelta::PixelDelta(_));
                let (_dx_raw, dy_raw) = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => (_x, y),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                let palette_scale = {
                    let (_, _, s) = self.screen_info();
                    s
                };
                if let Some(p) = &mut self.command_palette {
                    if matches!(p.mode, PaletteMode::PluginPicker | PaletteMode::InstrumentPicker) {
                        let delta_px = if is_pixel_delta {
                            -dy_raw
                        } else {
                            -dy_raw * PALETTE_ITEM_HEIGHT * palette_scale
                        };
                        p.scroll_plugin_by(delta_px, palette_scale);
                    } else if is_pixel_delta {
                        p.scroll_by_pixels(-dy_raw, palette_scale);
                    } else {
                        let lines = -(dy_raw as i32);
                        if lines != 0 {
                            p.scroll_by(lines);
                        }
                    }
                    self.request_redraw();
                    return;
                }
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 50.0, y * 50.0),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };

                if self.sample_browser.visible {
                    let (_, sh, scale) = self.screen_info();
                    if self.sample_browser.contains(self.mouse_pos, sh, scale) {
                        if is_pixel_delta {
                            self.sample_browser.scroll_direct(dy, sh, scale);
                        } else {
                            self.sample_browser.scroll(dy, sh, scale);
                        }
                        self.sample_browser.update_hover(self.mouse_pos, sh, scale);
                        self.request_redraw();
                        return;
                    }
                }

                if self.modifiers.super_key() {
                    let zoom_sensitivity = 0.005;
                    let factor = (1.0 + dy * zoom_sensitivity).clamp(0.5, 2.0);
                    self.camera.zoom_at(self.mouse_pos, factor);
                    if self.camera.zoom < MIDI_AUTO_EDIT_ZOOM_THRESHOLD && self.editing_midi_clip.is_some() {
                        self.editing_midi_clip = None;
                        self.selected_midi_notes.clear();
                    }
                } else {
                    self.camera.position[0] -= dx / self.camera.zoom;
                    self.camera.position[1] -= dy / self.camera.zoom;
                }

                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::PinchGesture { delta, .. } => {
                if self.command_palette.is_some() || self.context_menu.is_some() {
                    return;
                }
                let factor = (1.0 + delta as f32).clamp(0.5, 2.0);
                self.camera.zoom_at(self.mouse_pos, factor);
                if self.camera.zoom < MIDI_AUTO_EDIT_ZOOM_THRESHOLD && self.editing_midi_clip.is_some() {
                    self.editing_midi_clip = None;
                    self.selected_midi_notes.clear();
                }
                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.toast_manager.tick();
                self.update_recording_waveform();
                if let Some(gpu) = &mut self.gpu {
                    let w = gpu.config.width as f32;
                    let h = gpu.config.height as f32;

                    let sel_rect = if let DragState::Selecting { start_world } = &self.drag {
                        Some((*start_world, self.camera.screen_to_world(self.mouse_pos)))
                    } else {
                        None
                    };

                    let playhead_world_x = self
                        .audio_engine
                        .as_ref()
                        .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32);

                    let camera_moved = self.camera.position != self.last_rendered_camera_pos
                        || self.camera.zoom != self.last_rendered_camera_zoom;
                    let hover_changed = self.hovered != self.last_rendered_hovered;
                    let sel_changed = self.selected.len() != self.last_rendered_selected_len;
                    let gen_changed = self.render_generation != self.last_rendered_generation;
                    let needs_rebuild = camera_moved
                        || hover_changed
                        || sel_changed
                        || gen_changed
                        || playhead_world_x.is_some()
                        || sel_rect.is_some()
                        || self.file_hovering;

                    if needs_rebuild {
                        let selected_set: HashSet<HitTarget> =
                            self.selected.iter().copied().collect();
                        let component_map: std::collections::HashMap<
                            component::ComponentId,
                            usize,
                        > = self
                            .components
                            .iter()
                            .enumerate()
                            .map(|(i, c)| (c.id, i))
                            .collect();
                        let render_ctx = RenderContext {
                            camera: &self.camera,
                            screen_w: w,
                            screen_h: h,
                            objects: &self.objects,
                            waveforms: &self.waveforms,
                            effect_regions: &self.effect_regions,
                            plugin_blocks: &self.plugin_blocks,
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
                            component_map: &component_map,
                            fade_curve_hovered: self.fade_curve_hovered,
                            fade_curve_dragging: if let DragState::DraggingFadeCurve { waveform_idx, is_fade_in, .. } = self.drag {
                                Some((waveform_idx, is_fade_in))
                            } else {
                                None
                            },
                            mouse_world: self.camera.screen_to_world(self.mouse_pos),
                            bpm: self.bpm,
                            automation_mode: self.automation_mode,
                            active_automation_param: self.active_automation_param,
                            editing_midi_clip: self.editing_midi_clip,
                            instrument_regions: &self.instrument_regions,
                            midi_clips: &self.midi_clips,
                            selected_midi_notes: &self.selected_midi_notes,
                            midi_note_select_rect: self.midi_note_select_rect,
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
                        self.sample_browser.get_text_entries(h, gpu.scale_factor);
                    }
                    let browser_ref = if self.sample_browser.visible {
                        Some(&self.sample_browser)
                    } else {
                        None
                    };

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

                    if let Some(p) = &mut self.command_palette {
                        if p.mode == PaletteMode::VolumeFader {
                            p.fader_rms = self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak());
                        }
                    }

                    let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
                    let playback_pos = self
                        .audio_engine
                        .as_ref()
                        .map_or(0.0, |e| e.position_seconds());
                    let is_recording = self.recorder.as_ref().map_or(false, |r| r.is_recording());

                    gpu.render(
                        &self.camera,
                        &self.cached_instances,
                        &self.cached_wf_verts,
                        self.command_palette.as_ref(),
                        self.context_menu.as_ref(),
                        browser_ref,
                        drag_ghost,
                        is_playing,
                        is_recording,
                        playback_pos,
                        &self.export_regions,
                        &self.effect_regions,
                        &self.plugin_blocks,
                        self.editing_effect_name
                            .as_ref()
                            .map(|(idx, s)| (*idx, s.as_str())),
                        &self.waveforms,
                        self.editing_waveform_name
                            .as_ref()
                            .map(|(idx, s)| (*idx, s.as_str())),
                        self.plugin_editor.as_ref(),
                        self.settings_window.as_ref(),
                        &self.settings,
                        &self.toast_manager,
                        self.bpm,
                        self.editing_bpm.as_deref(),
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
                            DragState::DraggingVelocity { clip_idx, note_indices, .. } => {
                                note_indices.first().map(|&ni| (*clip_idx, ni))
                            }
                            _ => self.cmd_velocity_hover_note,
                        },
                    );
                }
                if self.toast_manager.has_active() {
                    self.request_redraw();
                }
            }

            _ => {}
        }
    }
}
