use super::*;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{Window, WindowId},
};

fn physical_key_to_char(key: &PhysicalKey) -> Option<&'static str> {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => Some("a"),
            KeyCode::KeyB => Some("b"),
            KeyCode::KeyC => Some("c"),
            KeyCode::KeyD => Some("d"),
            KeyCode::KeyE => Some("e"),
            KeyCode::KeyF => Some("f"),
            KeyCode::KeyG => Some("g"),
            KeyCode::KeyH => Some("h"),
            KeyCode::KeyI => Some("i"),
            KeyCode::KeyJ => Some("j"),
            KeyCode::KeyK => Some("k"),
            KeyCode::KeyL => Some("l"),
            KeyCode::KeyM => Some("m"),
            KeyCode::KeyN => Some("n"),
            KeyCode::KeyO => Some("o"),
            KeyCode::KeyP => Some("p"),
            KeyCode::KeyQ => Some("q"),
            KeyCode::KeyR => Some("r"),
            KeyCode::KeyS => Some("s"),
            KeyCode::KeyT => Some("t"),
            KeyCode::KeyU => Some("u"),
            KeyCode::KeyV => Some("v"),
            KeyCode::KeyW => Some("w"),
            KeyCode::KeyX => Some("x"),
            KeyCode::KeyY => Some("y"),
            KeyCode::KeyZ => Some("z"),
            KeyCode::Digit0 => Some("0"),
            KeyCode::Digit1 => Some("1"),
            KeyCode::Digit2 => Some("2"),
            KeyCode::Digit3 => Some("3"),
            KeyCode::Digit4 => Some("4"),
            KeyCode::Digit5 => Some("5"),
            KeyCode::Digit6 => Some("6"),
            KeyCode::Digit7 => Some("7"),
            KeyCode::Digit8 => Some("8"),
            KeyCode::Digit9 => Some("9"),
            KeyCode::Comma => Some(","),
            _ => None,
        },
        _ => None,
    }
}
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Layers")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));

        #[cfg(target_os = "macos")]
        let attrs = attrs
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.window = Some(window.clone());

        if !self.has_saved_state {
            self.camera.zoom = window.scale_factor() as f32;
        }

        // On web, attach canvas to DOM and init GPU asynchronously
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            let canvas = window.canvas().expect("winit window should have a canvas on web");
            let web_window = web_sys::window().unwrap();
            let document = web_window.document().unwrap();
            let container = document.get_element_by_id("canvas-container")
                .unwrap_or_else(|| document.body().unwrap().into());
            container.append_child(&canvas).ok();

            // Prevent browser from intercepting Ctrl+scroll (used for zoom)
            window.set_prevent_default(true);

            // Prevent browser from intercepting keyboard shortcuts (Cmd+T, Cmd+, etc.)
            // Uses both logical key and physical code so shortcuts work on non-Latin layouts
            {
                use wasm_bindgen::prelude::*;
                let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                    if e.meta_key() || e.ctrl_key() {
                        let key = e.key();
                        let code = e.code();
                        let dominated_logical = matches!(
                            key.as_str(),
                            "t" | "p" | "k" | "b" | "," | "r" | "c" | "v" | "d"
                            | "e" | "l" | "s" | "z" | "1" | "2" | "3" | "4"
                        );
                        let dominated_physical = matches!(
                            code.as_str(),
                            "KeyT" | "KeyP" | "KeyK" | "KeyB" | "Comma" | "KeyR"
                            | "KeyC" | "KeyV" | "KeyD" | "KeyE" | "KeyL" | "KeyS"
                            | "KeyZ" | "Digit1" | "Digit2" | "Digit3" | "Digit4"
                        );
                        let shift_logical = e.shift_key() && matches!(key.as_str(), "a" | "z");
                        let shift_physical = e.shift_key() && matches!(code.as_str(), "KeyA" | "KeyZ");
                        if dominated_logical || dominated_physical || shift_logical || shift_physical {
                            e.prevent_default();
                        }
                    }
                });
                document
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                    .expect("failed to add keydown listener");
                closure.forget(); // leak closure to keep it alive
            }

            // Set canvas physical size to fill viewport (CSS 100% on container
            // doesn't set the canvas element's width/height attributes that wgpu reads)
            let dpr = web_window.device_pixel_ratio();
            let vw = web_window.inner_width().unwrap().as_f64().unwrap();
            let vh = web_window.inner_height().unwrap().as_f64().unwrap();
            canvas.set_width((vw * dpr) as u32);
            canvas.set_height((vh * dpr) as u32);
            canvas.style().set_property("width", &format!("{}px", vw)).ok();
            canvas.style().set_property("height", &format!("{}px", vh)).ok();
            let window_clone = window.clone();
            let gpu_slot = self.pending_gpu.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let gpu = Gpu::new(window_clone).await;
                *gpu_slot.lock().unwrap() = Some(gpu);
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.gpu = Some(pollster::block_on(Gpu::new(window)));
        }

        #[cfg(feature = "native")]
        {
            if let Some(ms) = &mut self.menu_state {
                if !ms.initialized {
                    ms.menu.init_for_nsapp();
                    ms.initialized = true;
                }
            }

            // Scan plugins and restore saved plugin/instrument state at startup
            self.ensure_plugins_scanned();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Pick up GPU from async init (WASM)
        if self.gpu.is_none() {
            let taken = self.pending_gpu.try_lock().ok().and_then(|mut slot| slot.take());
            if let Some(gpu) = taken {
                self.gpu = Some(gpu);
                // Force resize to actual window dimensions (async init may have
                // captured stale/zero size)
                if let Some(window) = &self.window {
                    if let Some(gpu) = &mut self.gpu {
                        let size = window.inner_size();
                        gpu.resize(size);
                    }
                }
                self.mark_dirty();
                self.request_redraw();
            }
            // Keep polling until GPU is ready
            self.request_redraw();
        }

        #[cfg(feature = "native")]
        let is_playing = self.audio_engine.as_ref().map_or(false, |e| e.is_playing());
        #[cfg(not(feature = "native"))]
        let is_playing = false;

        // Flush coalesced arrow-nudge after 500ms idle
        if let Some(last) = self.arrow_nudge_last {
            if last.elapsed().as_millis() >= 500 {
                self.commit_arrow_nudge();
            }
        }

        if self.sample_browser.visible && self.sample_browser.tick_scroll() {
            self.request_redraw();
        }

        if is_playing || self.is_recording() {
            self.request_redraw();
        }

        // Keep event loop alive when connected so background windows poll network
        if self.network.is_connected() {
            self.request_redraw();
        }

        // Keep redrawing while background audio loads are in flight
        if self.pending_audio_loads_count > 0 {
            self.request_redraw();
        }

        #[cfg(feature = "native")]
        {
            if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
                self.handle_menu_event(event.id);
            }

            // --- Check for pending Welcome response (non-blocking) ---
            if let Some(rx) = &mut self.pending_welcome {
                if let Ok(assigned_user) = rx.try_recv() {
                    log::info!("Connected as {} ({})", assigned_user.name, assigned_user.id);
                    self.local_user = assigned_user;
                    self.reconnect_attempt = 0;
                    self.last_reconnect_time = None;
                    self.pending_welcome = None;
                }
            }

            // --- Auto-reconnect on disconnect ---
            if self.network.mode() == crate::network::NetworkMode::Disconnected {
                if let (Some(url), Some(pid)) = (self.connect_url.clone(), self.connect_project_id.clone()) {
                    let now = TimeInstant::now();
                    let delay_secs = (1u64 << self.reconnect_attempt.min(5)).min(30);
                    let should_retry = match self.last_reconnect_time {
                        Some(last) => now.duration_since(last).as_secs() >= delay_secs,
                        None => true,
                    };
                    if should_retry {
                        log::info!("Reconnecting (attempt {})...", self.reconnect_attempt + 1);
                        self.last_reconnect_time = Some(now);
                        self.reconnect_attempt += 1;
                        self.connect_to_server(&url, &pid);
                    }
                }
            }
        }

        // --- Poll network for remote operations ---
        let remote_ops = self.network.poll_ops();
        if !remote_ops.is_empty() {
            log::info!("[SYNC] polled {} remote ops", remote_ops.len());
        }
        for committed in remote_ops {
            self.apply_remote_op(committed);
        }

        // --- Poll network for ephemeral messages (cursors, presence) ---
        let ephemeral_msgs = self.network.poll_ephemeral();
        for msg in ephemeral_msgs {
            match msg {
                crate::user::EphemeralMessage::CursorMove { user_id, position } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.cursor_world = Some(position);
                    } else {
                        // Unknown user — create placeholder (UserJoined may have been missed)
                        let idx = self.remote_users.len();
                        self.remote_users.insert(user_id, crate::user::RemoteUserState {
                            user: crate::user::User {
                                id: user_id,
                                name: "Remote".to_string(),
                                color: crate::user::color_for_user_index(idx + 1),
                            },
                            cursor_world: Some(position),
                            drag_preview: None,
                            online: true,
                        });
                    }
                }
                crate::user::EphemeralMessage::DragUpdate { user_id, preview } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.drag_preview = Some(preview);
                    }
                }
                crate::user::EphemeralMessage::DragEnd { user_id } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.drag_preview = None;
                    }
                }
                crate::user::EphemeralMessage::UserJoined { user } => {
                    self.remote_users.insert(user.id, crate::user::RemoteUserState {
                        user: user.clone(),
                        cursor_world: None,
                        drag_preview: None,
                        online: true,
                    });
                }
                crate::user::EphemeralMessage::UserLeft { user_id } => {
                    self.remote_users.remove(&user_id);
                }
            }
            self.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                #[cfg(feature = "native")]
                {
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
                #[cfg(not(feature = "native"))]
                {
                    event_loop.exit();
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

                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    // Reuse the same background-thread path as browser drag-drop.
                    self.drop_audio_from_browser(&path);
                } else {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
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
                    MoveSelection(Vec<(HitTarget, [f32; 2])>),
                    Other,
                }
                let action = match &self.drag {
                    DragState::Panning {
                        start_mouse,
                        start_camera,
                    } => Action::Pan(*start_mouse, *start_camera),
                    DragState::MovingSelection { offsets, .. } => {
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
                            let raw_y = world[1] - offset[1];
                            let snapped_y = if self.is_snap_override_active() {
                                raw_y
                            } else {
                                snap_to_vertical_grid(raw_y, &self.settings, self.camera.zoom, self.bpm)
                            };
                            self.set_target_pos(target, [snapped_x, snapped_y]);
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

            // --- mouse buttons ---
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Middle => match state {
                    ElementState::Pressed => {
                        if self.context_menu.is_some() || self.settings_window.is_some() {
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
                                        HitTarget::Waveform(i) => self.waveforms.get(i).map(|wf| wf.color),
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
                                        let _new_val = pe.slider_drag(
                                            idx,
                                            self.mouse_pos[0],
                                            scr_w,
                                            scr_h,
                                            scale,
                                        );
                                        #[cfg(feature = "native")]
                                        {
                                            let pb_idx = pe.region_id; // repurposed as plugin_block index
                                            if let Some(pb) = self.plugin_blocks.get(&pb_idx) {
                                                if let Ok(guard) = pb.gui.lock() {
                                                    if let Some(gui) = guard.as_ref() {
                                                        gui.set_parameter(idx, _new_val as f64);
                                                    }
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
                        #[cfg(feature = "native")]
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
                                let wf_id = rw.waveform_id;
                                let hit_vol_text = rw.hit_test_vol_text(self.mouse_pos, sw, sh, scale);
                                let hit_vol = rw.hit_test_vol_knob(self.mouse_pos, sw, sh, scale);
                                let hit_vol_track = rw.hit_test_vol_track(self.mouse_pos, sw, sh, scale);
                                let hit_pan = rw.hit_test_pan_knob(self.mouse_pos, sw, sh, scale);
                                let hit_warp_btn = rw.hit_test_warp_mode_button(self.mouse_pos, sw, sh, scale);
                                let hit_warp_sel = rw.hit_test_warp_mode_selector(self.mouse_pos, sw, sh, scale);
                                let hit_sbpm_text = rw.hit_test_sample_bpm_text(self.mouse_pos, sw, sh, scale);
                                let hit_pitch_text = rw.hit_test_pitch_text(self.mouse_pos, sw, sh, scale);
                                if hit_warp_btn {
                                    // Toggle warp on/off (default to Semitone when enabling)
                                    let wf_id = rw.waveform_id;
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
                                        let before = self.waveforms[&wf_id].clone();
                                        self.waveforms.get_mut(&wf_id).unwrap().volume = 1.0;
                                        if let Some(rw) = &mut self.right_window {
                                            rw.volume = 1.0;
                                            rw.vol_dragging = false;
                                            rw.vol_fader_focused = true;
                                            rw.pan_knob_focused = false;
                                            rw.pitch_focused = false;
                                            rw.sample_bpm_focused = false;
                                        }
                                        let after = self.waveforms[&wf_id].clone();
                                        self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_id, before, after });
                                        self.sync_audio_clips();
                                        self.request_redraw();
                                        return;
                                    }
                                    let start_value = ui::palette::gain_to_vol_fader_pos(rw.volume);
                                    if let Some(rw) = &mut self.right_window {
                                        rw.vol_dragging = true;
                                        rw.drag_start_y = self.mouse_pos[1];
                                        rw.drag_start_value = start_value;
                                        rw.vol_fader_focused = true;
                                        rw.pan_knob_focused = false;
                                        rw.pitch_focused = false;
                                        rw.sample_bpm_focused = false;
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
                                        // Double-click resets pan to center (0.5)
                                        let before = self.waveforms[&wf_id].clone();
                                        self.waveforms.get_mut(&wf_id).unwrap().pan = 0.5;
                                        if let Some(rw) = &mut self.right_window {
                                            rw.pan = 0.5;
                                            rw.pan_dragging = false;
                                            rw.pan_knob_focused = true;
                                            rw.vol_fader_focused = false;
                                            rw.pitch_focused = false;
                                            rw.sample_bpm_focused = false;
                                        }
                                        let after = self.waveforms[&wf_id].clone();
                                        self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_id, before, after });
                                        self.sync_audio_clips();
                                        self.request_redraw();
                                        return;
                                    }
                                    let start_value = rw.pan;
                                    if let Some(rw) = &mut self.right_window {
                                        rw.pan_dragging = true;
                                        rw.drag_start_y = self.mouse_pos[1];
                                        rw.drag_start_value = start_value;
                                        rw.pan_knob_focused = true;
                                        rw.vol_fader_focused = false;
                                        rw.pitch_focused = false;
                                        rw.sample_bpm_focused = false;
                                    }
                                    let _ = wf_id;
                                    self.request_redraw();
                                    return;
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
                                } else if self.sample_browser.hit_add_button(self.mouse_pos, scale)
                                {
                                    #[cfg(feature = "native")]
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
                                } else {
                                    #[cfg(feature = "native")]
                                    if let Some(engine) = &self.audio_engine {
                                        engine.toggle_playback();
                                    }
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

                        // --- effect region corner resize ---
                        for (&i, er) in self.effect_regions.iter() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(er.position, er.size, world, self.camera.zoom) {
                                let before = er.clone();
                                self.drag = DragState::ResizingEffectRegion { region_id: i, anchor, nwse, before };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- instrument region corner resize ---
                        for (&i, ir) in self.instrument_regions.iter() {
                            if let Some((anchor, nwse)) = hit_test_corner_resize(ir.position, ir.size, world, self.camera.zoom) {
                                let before = crate::instruments::InstrumentRegionSnapshot {
                                    position: ir.position, size: ir.size,
                                    name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                    plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                                };
                                self.drag = DragState::ResizingInstrumentRegion { region_id: i, anchor, nwse, before };
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
                                    self.drag = DragState::MovingMidiClip { clip_id, offset, before };
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

                        // --- loop region corner resize ---
                        for (&i, lr) in self.loop_regions.iter() {
                            if !lr.enabled {
                                continue;
                            }
                            if let Some((anchor, nwse)) = hit_test_corner_resize(lr.position, lr.size, world, self.camera.zoom) {
                                let before = lr.clone();
                                self.drag = DragState::ResizingLoopRegion { region_id: i, anchor, nwse, before };
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- fade handle drag (checked before edge resize: fade handle sits at top corners) ---
                        if let Some((wf_idx, is_fade_in)) =
                            hit_test_fade_handle(&self.waveforms, world, &self.camera)
                        {
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

                        // --- waveform edge resize ---
                        match hit_test_waveform_edge(&self.waveforms, world, &self.camera) {
                            WaveformEdgeHover::LeftEdge(i) | WaveformEdgeHover::RightEdge(i) => {
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
                            if let Some(HitTarget::PluginBlock(_idx)) = hit {
                                #[cfg(feature = "native")]
                                self.open_plugin_block_gui(_idx);
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
                                if self.instrument_regions[&idx].has_plugin() {
                                    #[cfg(feature = "native")]
                                    self.open_instrument_region_gui(idx);
                                }
                                self.request_redraw();
                                return;
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
                                        if self.modifiers.super_key() && !matches!(zone, midi::MidiNoteHitZone::VelocityBar) {
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

                        // Click outside the editing component exits edit mode
                        if let Some(ec_idx) = self.editing_component {
                            if let Some(def) = self.components.get(&ec_idx) {
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
                                    self.update_right_window();
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
                        // Finish right window knob drag
                        {
                            let is_vol_drag = self.right_window.as_ref().map_or(false, |rw| rw.vol_dragging);
                            let is_pan_drag = self.right_window.as_ref().map_or(false, |rw| rw.pan_dragging);
                            let is_sbpm_drag = self.right_window.as_ref().map_or(false, |rw| rw.sample_bpm_dragging);
                            let is_pitch_drag = self.right_window.as_ref().map_or(false, |rw| rw.pitch_dragging);
                            if is_vol_drag || is_pan_drag || is_sbpm_drag || is_pitch_drag {
                                if let Some(rw) = &mut self.right_window {
                                    rw.vol_dragging = false;
                                    rw.pan_dragging = false;
                                    rw.sample_bpm_dragging = false;
                                    rw.pitch_dragging = false;
                                }
                                let wf_id = self.right_window.as_ref().map(|rw| rw.waveform_id);
                                if let Some(wf_id) = wf_id {
                                    if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                        let mut before = after.clone();
                                        if let Some(rw) = &self.right_window {
                                            if is_vol_drag {
                                                before.volume = ui::palette::vol_fader_pos_to_gain(rw.drag_start_value);
                                            } else if is_pan_drag {
                                                before.pan = rw.drag_start_value;
                                            } else if is_sbpm_drag {
                                                before.sample_bpm = rw.drag_start_value;
                                            } else {
                                                before.pitch_semitones = rw.drag_start_value;
                                            }
                                        }
                                        self.push_op(crate::operations::Operation::UpdateWaveform {
                                            id: wf_id,
                                            before,
                                            after,
                                        });
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                        }

                        // Finish plugin editor slider drag
                        if let Some(pe) = &mut self.plugin_editor {
                            if pe.dragging_slider.is_some() {
                                pe.dragging_slider = None;
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
                        if matches!(self.drag, DragState::ResizingEffectRegion { .. }) {
                            if let DragState::ResizingEffectRegion { region_id, before, .. } =
                                std::mem::replace(&mut self.drag, DragState::None)
                            {
                                if let Some(after) = self.effect_regions.get(&region_id) {
                                    self.push_op(crate::operations::Operation::UpdateEffectRegion { id: region_id, before, after: after.clone() });
                                }
                                self.sync_audio_clips();
                                self.update_hover();
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- finish resizing instrument region ---
                        if matches!(self.drag, DragState::ResizingInstrumentRegion { .. }) {
                            if let DragState::ResizingInstrumentRegion { region_id, before, .. } =
                                std::mem::replace(&mut self.drag, DragState::None)
                            {
                                if let Some(ir) = self.instrument_regions.get(&region_id) {
                                    let after = crate::instruments::InstrumentRegionSnapshot {
                                        position: ir.position, size: ir.size,
                                        name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                        plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                                    };
                                    self.push_op(crate::operations::Operation::UpdateInstrumentRegion { id: region_id, before, after });
                                }
                                self.sync_audio_clips();
                                self.update_hover();
                                self.update_cursor();
                                self.request_redraw();
                                return;
                            }
                        }

                        // --- finish MIDI note drag/resize ---
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
                            if let DragState::MovingMidiClip { clip_id, before, .. } =
                                std::mem::replace(&mut self.drag, DragState::None)
                            {
                                if let Some(after) = self.midi_clips.get(&clip_id) {
                                    self.push_op(crate::operations::Operation::UpdateMidiClip { id: clip_id, before, after: after.clone() });
                                }
                                self.sync_audio_clips();
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
                                let _hit_er = self
                                    .effect_regions
                                    .iter()
                                    .rev()
                                    .find(|(_, er)| point_in_rect(world, er.position, er.size))
                                    .map(|(&id, _)| id);

                                self.add_plugin_block(world, &plugin_id, &plugin_name);
                                if let Some(&pb_id) = self.plugin_blocks.keys().last() {
                                    let snap = self.plugin_blocks[&pb_id].snapshot();
                                    self.push_op(crate::operations::Operation::CreatePluginBlock { id: pb_id, data: snap });
                                    self.selected.clear();
                                    self.selected.push(HitTarget::PluginBlock(pb_id));
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
                                    (HitTarget::EffectRegion(id), EntityBeforeState::EffectRegion(before)) => {
                                        if let Some(after) = self.effect_regions.get(&id) {
                                            ops.push(crate::operations::Operation::UpdateEffectRegion { id, before, after: after.clone() });
                                        }
                                    }
                                    (HitTarget::PluginBlock(id), EntityBeforeState::PluginBlock(before)) => {
                                        if let Some(after) = self.plugin_blocks.get(&id) {
                                            ops.push(crate::operations::Operation::DeletePluginBlock { id, data: before });
                                            ops.push(crate::operations::Operation::CreatePluginBlock { id, data: after.snapshot() });
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
                                    (HitTarget::InstrumentRegion(id), EntityBeforeState::InstrumentRegion(before)) => {
                                        if let Some(ir) = self.instrument_regions.get(&id) {
                                            let after = crate::instruments::InstrumentRegionSnapshot {
                                                position: ir.position, size: ir.size,
                                                name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                                plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                                            };
                                            ops.push(crate::operations::Operation::UpdateInstrumentRegion { id, before, after });
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
                                if let Some(engine) = &self.audio_engine {
                                    if !engine.is_playing() {
                                        let secs = snapped_x as f64 / PIXELS_PER_SECOND as f64;
                                        engine.seek_to_seconds(secs);
                                    }
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
                        self.update_right_window();
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
                                if self.midi_clips.contains_key(&mc_idx) && !self.selected_midi_notes.is_empty() {
                                    let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                    let mut indices = self.selected_midi_notes.clone();
                                    indices.sort_unstable_by(|a, b| b.cmp(a));
                                    let mc = self.midi_clips.get_mut(&mc_idx).unwrap();
                                    for &i in &indices {
                                        if i < mc.notes.len() {
                                            mc.notes.remove(i);
                                        }
                                    }
                                    let after_notes = self.midi_clips[&mc_idx].notes.clone();
                                    self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: mc_idx, before: before_notes, after: after_notes });
                                    self.selected_midi_notes.clear();
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }
                        // Cmd+D: duplicate selected MIDI notes
                        if self.modifiers.super_key() && matches!(physical_key_to_char(&event.physical_key), Some("d")) {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if self.midi_clips.contains_key(&mc_idx) && !self.selected_midi_notes.is_empty() {
                                    let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                    let notes = &self.midi_clips[&mc_idx].notes;
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
                                        if ni < self.midi_clips[&mc_idx].notes.len() {
                                            let mut cloned = self.midi_clips[&mc_idx].notes[ni].clone();
                                            cloned.start_px += group_shift;
                                            self.midi_clips[&mc_idx].notes.push(cloned);
                                            new_indices.push(self.midi_clips[&mc_idx].notes.len() - 1);
                                        }
                                    }
                                    let after_notes = self.midi_clips[&mc_idx].notes.clone();
                                    self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: mc_idx, before: before_notes, after: after_notes });
                                    self.selected_midi_notes = self.midi_clips[&mc_idx].resolve_note_overlaps(&new_indices);
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }
                        // Left/Right: move notes; Shift+Left/Right: resize note duration
                        if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight)) {
                            if let Some(mc_idx) = self.editing_midi_clip {
                                if self.midi_clips.contains_key(&mc_idx) && !self.selected_midi_notes.is_empty() {
                                    let mc = &self.midi_clips[&mc_idx];
                                    let step = grid::clip_grid_spacing(mc.grid_mode, mc.triplet_grid, self.camera.zoom, self.bpm);
                                    let delta = if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) { step } else { -step };
                                    if self.modifiers.shift_key() {
                                        // Resize duration
                                        let min_dur = step;
                                        let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                            if ni >= self.midi_clips[&mc_idx].notes.len() { return false; }
                                            self.midi_clips[&mc_idx].notes[ni].duration_px + delta >= min_dur
                                        });
                                        if all_valid {
                                            let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                            for &ni in &self.selected_midi_notes {
                                                if ni < self.midi_clips[&mc_idx].notes.len() {
                                                    self.midi_clips[&mc_idx].notes[ni].duration_px += delta;
                                                }
                                            }
                                            let after_notes = self.midi_clips[&mc_idx].notes.clone();
                                            self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: mc_idx, before: before_notes, after: after_notes });
                                            self.selected_midi_notes = self.midi_clips[&mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                            self.sync_audio_clips();
                                            self.request_redraw();
                                            return;
                                        }
                                    } else {
                                        // Move position
                                        let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                            if ni >= self.midi_clips[&mc_idx].notes.len() { return false; }
                                            self.midi_clips[&mc_idx].notes[ni].start_px + delta >= 0.0
                                        });
                                        if all_valid {
                                            let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                            for &ni in &self.selected_midi_notes {
                                                if ni < self.midi_clips[&mc_idx].notes.len() {
                                                    self.midi_clips[&mc_idx].notes[ni].start_px += delta;
                                                }
                                            }
                                            let after_notes = self.midi_clips[&mc_idx].notes.clone();
                                            self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: mc_idx, before: before_notes, after: after_notes });
                                            self.selected_midi_notes = self.midi_clips[&mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                            self.sync_audio_clips();
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
                                if self.midi_clips.contains_key(&mc_idx) && !self.selected_midi_notes.is_empty() {
                                    let delta: i16 = if self.modifiers.shift_key() { 12 } else { 1 };
                                    let delta = if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp)) { delta } else { -delta };
                                    // Check if all notes stay in valid range (0..=127)
                                    let all_valid = self.selected_midi_notes.iter().all(|&ni| {
                                        if ni >= self.midi_clips[&mc_idx].notes.len() { return false; }
                                        let new_pitch = self.midi_clips[&mc_idx].notes[ni].pitch as i16 + delta;
                                        (0..=127).contains(&new_pitch)
                                    });
                                    if all_valid {
                                        let before_notes = self.midi_clips[&mc_idx].notes.clone();
                                        for &ni in &self.selected_midi_notes {
                                            if ni < self.midi_clips[&mc_idx].notes.len() {
                                                self.midi_clips[&mc_idx].notes[ni].pitch =
                                                    (self.midi_clips[&mc_idx].notes[ni].pitch as i16 + delta) as u8;
                                            }
                                        }
                                        let after_notes = self.midi_clips[&mc_idx].notes.clone();
                                        self.push_op(crate::operations::Operation::UpdateMidiNotes { clip_id: mc_idx, before: before_notes, after: after_notes });
                                        self.selected_midi_notes = self.midi_clips[&mc_idx].resolve_note_overlaps(&self.selected_midi_notes);
                                        self.sync_audio_clips();
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    // Escape clears vol fader / pan knob / pitch / sample_bpm focus
                    if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                        if let Some(rw) = &mut self.right_window {
                            if rw.vol_fader_focused || rw.pan_knob_focused || rw.pitch_focused || rw.sample_bpm_focused {
                                rw.vol_fader_focused = false;
                                rw.pan_knob_focused = false;
                                rw.pitch_focused = false;
                                rw.sample_bpm_focused = false;
                                self.request_redraw();
                                return;
                            }
                        }
                    }

                    // Up/Down arrow volume adjustment when fader is focused
                    if let Some(rw) = &self.right_window {
                        if rw.vol_fader_focused && matches!(event.logical_key,
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown))
                        {
                            let shift = self.modifiers.shift_key();
                            let delta_db = match event.logical_key {
                                Key::Named(NamedKey::ArrowUp) => if shift { 0.1 } else { 1.0 },
                                _ => if shift { -0.1 } else { -1.0 },
                            };
                            let wf_id = rw.waveform_id;
                            let current_db = ui::palette::gain_to_db(rw.volume);
                            let new_db = (current_db + delta_db).clamp(ui::palette::VOL_FADER_DB_BOTTOM, ui::palette::VOL_FADER_DB_MAX);
                            let new_gain = if new_db <= ui::palette::VOL_FADER_DB_BOTTOM { 0.0 } else { ui::palette::db_to_gain(new_db) };
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.volume = new_gain;
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.volume = new_gain;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                        id: wf_id,
                                        before,
                                        after,
                                    });
                                }
                                self.sync_audio_clips();
                                self.mark_dirty();
                            }
                            self.request_redraw();
                            return;
                        }
                    }

                    // Up/Down arrow pan adjustment when pan knob is focused
                    if let Some(rw) = &self.right_window {
                        if rw.pan_knob_focused && matches!(event.logical_key,
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown))
                        {
                            let shift = self.modifiers.shift_key();
                            let delta = match event.logical_key {
                                Key::Named(NamedKey::ArrowUp) => if shift { 0.001 } else { 0.01 },
                                _ => if shift { -0.001 } else { -0.01 },
                            };
                            let wf_id = rw.waveform_id;
                            let new_pan = (rw.pan + delta).clamp(0.0, 1.0);
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.pan = new_pan;
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.pan = new_pan;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                        id: wf_id,
                                        before,
                                        after,
                                    });
                                }
                                self.sync_audio_clips();
                                self.mark_dirty();
                            }
                            self.request_redraw();
                            return;
                        }
                    }

                    // Up/Down arrow sample BPM adjustment when sample_bpm is focused
                    if let Some(rw) = &self.right_window {
                        if rw.sample_bpm_focused && rw.warp_mode == ui::waveform::WarpMode::RePitch && matches!(event.logical_key,
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown))
                        {
                            let shift = self.modifiers.shift_key();
                            let delta = match event.logical_key {
                                Key::Named(NamedKey::ArrowUp) => if shift { 0.1 } else { 1.0 },
                                _ => if shift { -0.1 } else { -1.0 },
                            };
                            let wf_id = rw.waveform_id;
                            let new_bpm = (rw.sample_bpm + delta).clamp(20.0, 999.0);
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.sample_bpm = new_bpm;
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.sample_bpm = new_bpm;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                        id: wf_id,
                                        before,
                                        after,
                                    });
                                }
                                self.resize_warped_clips();
                                self.sync_audio_clips();
                                self.mark_dirty();
                            }
                            self.request_redraw();
                            return;
                        }
                    }

                    // Up/Down arrow pitch adjustment when pitch is focused
                    if let Some(rw) = &self.right_window {
                        if rw.pitch_focused && rw.warp_mode == ui::waveform::WarpMode::Semitone && matches!(event.logical_key,
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown))
                        {
                            let shift = self.modifiers.shift_key();
                            let delta = match event.logical_key {
                                Key::Named(NamedKey::ArrowUp) => if shift { 0.1 } else { 1.0 },
                                _ => if shift { -0.1 } else { -1.0 },
                            };
                            let wf_id = rw.waveform_id;
                            let new_pitch = (rw.pitch_semitones + delta).clamp(-24.0, 24.0);
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.pitch_semitones = new_pitch;
                                }
                                if let Some(rw) = &mut self.right_window {
                                    rw.pitch_semitones = new_pitch;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                        id: wf_id,
                                        before,
                                        after,
                                    });
                                }
                                self.resize_warped_clips();
                                self.sync_audio_clips();
                                self.mark_dirty();
                            }
                            self.request_redraw();
                            return;
                        }
                    }

                    // Arrow-key nudge for selected canvas entities
                    if matches!(event.logical_key,
                        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) |
                        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown))
                        && self.editing_midi_clip.is_none()
                        && !self.selected.is_empty()
                    {
                        let shift = self.modifiers.shift_key();
                        let (dx, dy) = match event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => {
                                let step = if shift {
                                    grid::pixels_per_beat(self.bpm) * 4.0
                                } else {
                                    grid::grid_spacing_for_settings(&self.settings, self.camera.zoom, self.bpm)
                                };
                                (-step, 0.0)
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                let step = if shift {
                                    grid::pixels_per_beat(self.bpm) * 4.0
                                } else {
                                    grid::grid_spacing_for_settings(&self.settings, self.camera.zoom, self.bpm)
                                };
                                (step, 0.0)
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                let step = if shift {
                                    grid::clip_height(self.bpm)
                                } else {
                                    grid::grid_spacing_for_settings(&self.settings, self.camera.zoom, self.bpm)
                                };
                                (0.0, -step)
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                let step = if shift {
                                    grid::clip_height(self.bpm)
                                } else {
                                    grid::grid_spacing_for_settings(&self.settings, self.camera.zoom, self.bpm)
                                };
                                (0.0, step)
                            }
                            _ => (0.0, 0.0),
                        };
                        self.nudge_selection(dx, dy);
                        return;
                    }

                    // --- BPM editing input ---
                    if self.editing_bpm.is_editing() {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                self.editing_bpm.cancel();
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if let Some(text) = self.editing_bpm.commit() {
                                    if let Ok(val) = text.parse::<f32>() {
                                        let before = self.bpm;
                                        let after = val.clamp(20.0, 999.0);
                                        if (before - after).abs() > f32::EPSILON {
                                            let scale = before / after;
                                            self.rescale_clip_positions(scale);
                                            self.rescale_camera_for_bpm(scale);
                                            self.bpm = after;
                                            self.resize_warped_clips();
                                            let overlap_ops = self.resolve_all_waveform_overlaps();
                                            let mut ops = vec![crate::operations::Operation::SetBpm { before, after }];
                                            ops.extend(overlap_ops);
                                            self.push_op(crate::operations::Operation::Batch(ops));
                                            self.sync_audio_clips();
                                        }
                                        self.mark_dirty();
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                self.editing_bpm.pop_char();
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
                                    self.editing_bpm.push_char(s);
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- vol dB editing input ---
                    let vol_editing = self.right_window.as_ref().map_or(false, |rw| rw.vol_entry.is_editing());
                    if vol_editing {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.vol_entry.cancel();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let commit = self.right_window.as_mut().and_then(|rw| rw.vol_entry.commit());
                                if let Some(text) = commit {
                                    if let Ok(db) = text.parse::<f32>() {
                                        let new_gain = if db <= ui::palette::VOL_FADER_DB_BOTTOM {
                                            0.0
                                        } else {
                                            ui::palette::db_to_gain(db.clamp(ui::palette::VOL_FADER_DB_BOTTOM, ui::palette::VOL_FADER_DB_MAX))
                                        };
                                        let wf_id = self.right_window.as_ref().map(|rw| rw.waveform_id);
                                        if let Some(wf_id) = wf_id {
                                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                                    wf.volume = new_gain;
                                                }
                                                if let Some(rw) = &mut self.right_window {
                                                    rw.volume = new_gain;
                                                }
                                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                                        id: wf_id,
                                                        before,
                                                        after,
                                                    });
                                                }
                                                self.sync_audio_clips();
                                                self.mark_dirty();
                                            }
                                        }
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.vol_entry.pop_char();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
                                    if let Some(rw) = &mut self.right_window {
                                        rw.vol_entry.push_char(s);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- sample BPM editing input ---
                    let sbpm_editing = self.right_window.as_ref().map_or(false, |rw| rw.sample_bpm_entry.is_editing());
                    if sbpm_editing {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.sample_bpm_entry.cancel();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let commit = self.right_window.as_mut().and_then(|rw| rw.sample_bpm_entry.commit());
                                if let Some(text) = commit {
                                    if let Ok(val) = text.parse::<f32>() {
                                        let new_bpm = val.clamp(20.0, 999.0);
                                        let wf_id = self.right_window.as_ref().map(|rw| rw.waveform_id);
                                        if let Some(wf_id) = wf_id {
                                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                                    wf.sample_bpm = new_bpm;
                                                    // Resize clip if in RePitch mode
                                                    if wf.warp_mode == ui::waveform::WarpMode::RePitch {
                                                        if let Some(clip) = self.audio_clips.get(&wf_id) {
                                                            let original_duration_px = clip.duration_secs * PIXELS_PER_SECOND;
                                                            wf.size[0] = original_duration_px * (self.bpm / wf.sample_bpm);
                                                        }
                                                    }
                                                }
                                                if let Some(rw) = &mut self.right_window {
                                                    rw.sample_bpm = new_bpm;
                                                }
                                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                                        id: wf_id,
                                                        before,
                                                        after,
                                                    });
                                                }
                                                self.sync_audio_clips();
                                                self.mark_dirty();
                                            }
                                        }
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.sample_bpm_entry.pop_char();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
                                    if let Some(rw) = &mut self.right_window {
                                        rw.sample_bpm_entry.push_char(s);
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // --- pitch semitones editing input ---
                    let pitch_editing = self.right_window.as_ref().map_or(false, |rw| rw.pitch_entry.is_editing());
                    if pitch_editing {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.pitch_entry.cancel();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let commit = self.right_window.as_mut().and_then(|rw| rw.pitch_entry.commit());
                                if let Some(text) = commit {
                                    if let Ok(semitones) = text.parse::<f32>() {
                                        let new_pitch = semitones.clamp(-24.0, 24.0);
                                        let wf_id = self.right_window.as_ref().map(|rw| rw.waveform_id);
                                        if let Some(wf_id) = wf_id {
                                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                                    wf.pitch_semitones = new_pitch;
                                                }
                                                if let Some(rw) = &mut self.right_window {
                                                    rw.pitch_semitones = new_pitch;
                                                }
                                                self.resize_warped_clips();
                                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                                    self.push_op(crate::operations::Operation::UpdateWaveform {
                                                        id: wf_id,
                                                        before,
                                                        after,
                                                    });
                                                }
                                                self.sync_audio_clips();
                                                self.mark_dirty();
                                            }
                                        }
                                    }
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(rw) = &mut self.right_window {
                                    rw.pitch_entry.pop_char();
                                }
                                self.request_redraw();
                                return;
                            }
                            Key::Character(ch) if !self.modifiers.super_key() => {
                                let s = ch.as_ref();
                                if s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
                                    if let Some(rw) = &mut self.right_window {
                                        rw.pitch_entry.push_char(s);
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
                                    if self.effect_regions.contains_key(&idx) {
                                        let before = self.effect_regions[&idx].clone();
                                        let name = if text.trim().is_empty() {
                                            "effects".to_string()
                                        } else {
                                            text
                                        };
                                        self.effect_regions.get_mut(&idx).unwrap().name = name;
                                        let after = self.effect_regions[&idx].clone();
                                        self.push_op(crate::operations::Operation::UpdateEffectRegion { id: idx, before, after });
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
                                    if self.waveforms.contains_key(&idx) {
                                        let before = self.waveforms[&idx].clone();
                                        let wf = self.waveforms.get_mut(&idx).unwrap();
                                        let name = if text.trim().is_empty() {
                                            wf.audio.filename.clone()
                                        } else {
                                            text
                                        };
                                        let mut new_audio = (*wf.audio).clone();
                                        new_audio.filename = name;
                                        wf.audio = Arc::new(new_audio);
                                        let after = self.waveforms[&idx].clone();
                                        self.push_op(crate::operations::Operation::UpdateWaveform { id: idx, before, after });
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
                                    let _is_instrument = matches!(fader_mode, Some(PaletteMode::InstrumentPicker));
                                    let plugin_info = self
                                        .command_palette
                                        .as_ref()
                                        .and_then(|p| p.selected_plugin())
                                        .map(|e| (e.unique_id.clone(), e.name.clone()));
                                    self.command_palette = None;
                                    if let Some((_plugin_id, _plugin_name)) = plugin_info {
                                        #[cfg(feature = "native")]
                                        if _is_instrument {
                                            self.add_instrument(&_plugin_id, &_plugin_name);
                                        } else {
                                            self.add_plugin_to_selected_effect_region(&_plugin_id, &_plugin_name);
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
                                if let Some((_plugin_id, _plugin_name, _is_instrument)) = inline_plugin {
                                    self.command_palette = None;
                                    #[cfg(feature = "native")]
                                    {
                                        if _is_instrument {
                                            self.add_instrument(&_plugin_id, &_plugin_name);
                                        } else {
                                            self.add_plugin_to_selected_effect_region(&_plugin_id, &_plugin_name);
                                        }
                                    }
                                    self.request_redraw();
                                    return;
                                }

                                let action = self
                                    .command_palette
                                    .as_ref()
                                    .and_then(|p| p.selected_action());
                                if let Some(a) = action {
                                    if matches!(a, CommandAction::SetMasterVolume | CommandAction::AddPlugin | CommandAction::AddInstrument) {
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
                    #[cfg(feature = "native")]
                    if matches!(event.logical_key, Key::Named(NamedKey::Enter)) {
                        if let Some(HitTarget::EffectRegion(idx)) = self.selected.first().copied() {
                            if let Some(er) = self.effect_regions.get(&idx) {
                                let block_ids = effects::collect_plugins_for_region(er, &self.plugin_blocks);
                                if block_ids.is_empty() {
                                    println!("  Effect region {:?} has no overlapping plugins", idx);
                                } else {
                                    println!("  Effect region {:?} plugin chain:", idx);
                                    for (j, &bi) in block_ids.iter().enumerate() {
                                        let pb = &self.plugin_blocks[&bi];
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
                            if self.plugin_blocks.contains_key(&idx) {
                                self.open_plugin_block_gui(idx);
                            }
                            self.request_redraw();
                        }
                    }

                    // --- global shortcuts ---
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.selected.clear();
                            self.update_right_window();
                            self.select_area = None;
                            self.request_redraw();
                        }
                        Key::Named(NamedKey::Space) => {
                            if self.is_recording() {
                                self.toggle_recording();
                                self.request_redraw();
                            } else {
                                #[cfg(feature = "native")]
                                if let Some(engine) = &self.audio_engine {
                                    if !engine.is_playing() {
                                        let seek_target = self.selected.first().and_then(|t| {
                                            if let HitTarget::Waveform(id) = t {
                                                self.waveforms.get(id).map(|wf| wf.position[0])
                                            } else {
                                                None
                                            }
                                        });
                                        if let Some(x) = seek_target {
                                            let secs = x as f64 / PIXELS_PER_SECOND as f64;
                                            engine.seek_to_seconds(secs);
                                        } else if let Some(sa) = &self.select_area {
                                            let secs = sa.position[0] as f64 / PIXELS_PER_SECOND as f64;
                                            engine.seek_to_seconds(secs);
                                        }
                                    }
                                    engine.toggle_playback();
                                    self.request_redraw();
                                }
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
                                let wf_ids: Vec<EntityId> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::Waveform(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                let lr_ids: Vec<EntityId> = self
                                    .selected
                                    .iter()
                                    .filter_map(|t| {
                                        if let HitTarget::LoopRegion(i) = t { Some(*i) } else { None }
                                    })
                                    .collect();
                                if !wf_ids.is_empty() || !lr_ids.is_empty() {
                                    let mut ops = Vec::new();
                                    if !wf_ids.is_empty() {
                                        let any_enabled = wf_ids.iter().any(|i| self.waveforms.get(i).map_or(false, |wf| !wf.disabled));
                                        let new_disabled = any_enabled;
                                        for i in &wf_ids {
                                            if let Some(wf) = self.waveforms.get_mut(i) {
                                                let before = wf.clone();
                                                wf.disabled = new_disabled;
                                                ops.push(crate::operations::Operation::UpdateWaveform { id: *i, before, after: wf.clone() });
                                            }
                                        }
                                    }
                                    if !lr_ids.is_empty() {
                                        let any_enabled = lr_ids.iter().any(|i| self.loop_regions.get(i).map_or(false, |lr| lr.enabled));
                                        let new_enabled = !any_enabled;
                                        for i in &lr_ids {
                                            if let Some(lr) = self.loop_regions.get_mut(i) {
                                                let before = lr.clone();
                                                lr.enabled = new_enabled;
                                                ops.push(crate::operations::Operation::UpdateLoopRegion { id: *i, before, after: lr.clone() });
                                            }
                                        }
                                        self.sync_loop_region();
                                    }
                                    if !ops.is_empty() {
                                        self.push_op(crate::operations::Operation::Batch(ops));
                                    }
                                    self.sync_audio_clips();
                                    self.request_redraw();
                                }
                            }
                            _ => {}
                        },
                        _ if self.modifiers.super_key() => {
                            if let Some(ch) = physical_key_to_char(&event.physical_key) {
                                match ch {
                                    "," => {
                                        #[cfg(feature = "native")]
                                        {
                                            self.command_palette = None;
                                            self.context_menu = None;
                                            self.settings_window = if self.settings_window.is_some() {
                                                None
                                            } else {
                                                Some(SettingsWindow::new())
                                            };
                                            self.request_redraw();
                                        }
                                    }
                                    "k" => {
                                        self.context_menu = None;
                                        self.settings_window = None;
                                        self.command_palette = if self.command_palette.is_some() {
                                            None
                                        } else {
                                            #[allow(unused_mut)]
                                            let mut p = CommandPalette::new(self.settings.dev_mode);
                                            #[cfg(feature = "native")]
                                            { p.plugin_entries = self.build_palette_plugin_entries(); }
                                            Some(p)
                                        };
                                        self.request_redraw();
                                    }
                                    "t" | "p" => {
                                        self.context_menu = None;
                                        self.settings_window = None;
                                        self.command_palette = if self.command_palette.is_some() {
                                            None
                                        } else {
                                            #[allow(unused_mut)]
                                            let mut p = CommandPalette::new(self.settings.dev_mode);
                                            #[cfg(feature = "native")]
                                            { p.plugin_entries = self.build_palette_plugin_entries(); }
                                            Some(p)
                                        };
                                        self.request_redraw();
                                    }
                                    "b" => {
                                        self.sample_browser.visible = !self.sample_browser.visible;
                                        #[cfg(feature = "native")]
                                        if self.sample_browser.visible {
                                            self.ensure_plugins_scanned();
                                        }
                                        self.request_redraw();
                                    }
                                    "a" if self.modifiers.shift_key() => {
                                        #[cfg(feature = "native")]
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
                                            self.redo_op();
                                        } else {
                                            self.undo_op();
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
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // --- scroll = pan, Cmd+scroll = zoom, pinch = zoom ---
            WindowEvent::MouseWheel { delta, .. } => {
                if self.context_menu.is_some() || self.settings_window.is_some() {
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

                let zoom_modifier = if cfg!(target_arch = "wasm32") {
                    // In browsers, trackpad pinch-to-zoom is reported as ctrl+wheel
                    self.modifiers.super_key() || self.modifiers.control_key()
                } else {
                    self.modifiers.super_key()
                };
                if zoom_modifier {
                    let zoom_sensitivity = 0.005;
                    let factor = (1.0 + dy * zoom_sensitivity).clamp(0.5, 2.0);
                    self.camera.zoom_at(self.mouse_pos, factor);
                    self.broadcast_cursor_if_connected();
                    if self.camera.zoom < MIDI_AUTO_EDIT_ZOOM_THRESHOLD && self.editing_midi_clip.is_some() {
                        self.editing_midi_clip = None;
                        self.selected_midi_notes.clear();
                    }
                } else {
                    self.camera.position[0] -= dx / self.camera.zoom;
                    self.camera.position[1] -= dy / self.camera.zoom;
                    self.broadcast_cursor_if_connected();
                }

                self.update_hover();
                self.request_redraw();
            }

            WindowEvent::PinchGesture { delta, .. } => {
                if self.command_palette.is_some() || self.context_menu.is_some() || self.settings_window.is_some() {
                    return;
                }
                let factor = (1.0 + delta as f32).clamp(0.5, 2.0);
                self.camera.zoom_at(self.mouse_pos, factor);
                self.broadcast_cursor_if_connected();
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
                self.poll_pending_audio_loads();
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
                            instrument_regions: &self.instrument_regions,
                            midi_clips: &self.midi_clips,
                            selected_midi_notes: &self.selected_midi_notes,
                            midi_note_select_rect: self.midi_note_select_rect,
                            remote_users: &self.remote_users,
                            network_mode: self.network.mode(),
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
                            #[cfg(feature = "native")]
                            { p.fader_rms = self.audio_engine.as_ref().map_or(0.0, |e| e.rms_peak()); }
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
                        {
                            #[cfg(feature = "native")]
                            { self.settings_window.as_ref() }
                            #[cfg(not(feature = "native"))]
                            { Option::<&ui::settings_window::SettingsWindow>::None }
                        },
                        &self.settings,
                        &self.toast_manager,
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
