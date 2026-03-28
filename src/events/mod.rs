use super::*;

mod commands;
mod cursor;
mod keyboard;
mod mouse;
mod redraw;
mod scroll;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
};

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
                    #[cfg(target_os = "macos")]
                    ms.menu.init_for_nsapp();
                    ms.initialized = true;
                }
            }

            // Scan plugins and restore saved plugin/instrument state at startup
            self.ensure_plugins_scanned();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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

        // Flush coalesced arrow-resize after 500ms idle
        if let Some(last) = self.arrow_resize_last {
            if last.elapsed().as_millis() >= 500 {
                self.commit_arrow_resize();
            }
        }

        if self.sample_browser.visible && self.sample_browser.tick_scroll() {
            self.request_redraw();
        }

        // Blink cursor in search bar while focused.
        if self.sample_browser.visible && self.sample_browser.search_focused {
            if self.sample_browser.search_input.tick_cursor_blink() {
                self.sample_browser.cursor_text_dirty = true;
                self.request_redraw();
            }
            let wake = self.sample_browser.search_input.next_cursor_blink_toggle();
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
        }

        // Blink cursor in palette search bar while open.
        let (palette_toggled, palette_wake) = match self.command_palette {
            Some(ref mut palette) => {
                let t = palette.search_input.tick_cursor_blink();
                // Also tick session input blink
                let _ = palette.session_input.tick_cursor_blink();
                let w = palette.search_input.next_cursor_blink_toggle();
                (t, Some(w))
            }
            None => (false, None),
        };
        if palette_toggled {
            self.request_redraw();
        }
        if let Some(wake) = palette_wake {
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
        }

        // Flush debounced search rebuild when deadline has passed.
        if self.sample_browser.tick_search_debounce() {
            self.request_redraw();
        }
        if self.sample_browser.is_search_pending() {
            self.request_redraw();
        }
        // Poll background file index build.
        if self.sample_browser.tick_file_index() {
            self.request_redraw();
        }
        if self.sample_browser.is_file_index_building() {
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

                    // Dismiss persistent reconnecting toast and show connected
                    self.toast_manager.dismiss_by_id("reconnecting");
                    self.toast_manager.push("Connected", crate::ui::toast::ToastKind::Success);

                    // Clean-slate: clear all entities before ops replay
                    // Skip if we joined via snapshot — state is already loaded
                    if self.joined_via_snapshot {
                        self.joined_via_snapshot = false;
                    } else {
                        self.clear_entity_state();
                    }
                }
            }

            // --- Check for pending sync-complete signal (non-blocking) ---
            if let Some(rx) = &mut self.pending_sync_complete {
                match rx.try_recv() {
                    Ok(()) => {
                        self.pending_sync_complete = None;
                        self.toast_manager.push("State synced", crate::ui::toast::ToastKind::Success);
                        log::info!("[SYNC] Initial sync complete — all ops replayed, live queries active");
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        // Sender dropped — connection died during sync
                        self.pending_sync_complete = None;
                        log::warn!("[SYNC] Sync-complete channel closed — connection lost during initial sync");
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                        // Not ready yet, keep polling
                    }
                }
            }

            // --- Poll share upload progress (non-blocking) ---
            if let Some(rx) = &self.share_upload_rx {
                let mut done = false;
                while let Ok(status) = rx.try_recv() {
                    match status {
                        crate::ShareUploadStatus::Progress { current, total, .. } => {
                            self.toast_manager.push_persistent(
                                "share-upload",
                                format!("Uploading audio {current}/{total}…"),
                                crate::ui::toast::ToastKind::Info,
                            );
                        }
                        crate::ShareUploadStatus::Done => {
                            self.toast_manager.dismiss_by_id("share-upload");
                            self.toast_manager.push("Project shared", crate::ui::toast::ToastKind::Success);
                            done = true;
                        }
                        crate::ShareUploadStatus::Failed(msg) => {
                            self.toast_manager.dismiss_by_id("share-upload");
                            self.toast_manager.push(
                                format!("Share failed: {msg}"),
                                crate::ui::toast::ToastKind::Error,
                            );
                            done = true;
                        }
                    }
                }
                if done {
                    self.share_upload_rx = None;
                }
            }

            // --- Poll join download result (non-blocking) ---
            if let Some(rx) = &self.join_download_rx {
                if let Ok(result) = rx.try_recv() {
                    self.join_download_rx = None;
                    self.toast_manager.dismiss_by_id("join-download");
                    match result {
                        crate::JoinDownloadResult::Ready(store) => {
                            self.apply_join_download(*store);
                        }
                        crate::JoinDownloadResult::Failed(msg) => {
                            self.toast_manager.push(
                                format!("Join failed: {msg}"),
                                crate::ui::toast::ToastKind::Error,
                            );
                        }
                    }
                }
            }

            // --- Auto-reconnect on disconnect ---
            const MAX_RECONNECT_ATTEMPTS: u32 = 10;
            if self.network.mode() == crate::network::NetworkMode::Disconnected {
                if let (Some(url), Some(pid)) = (self.connect_url.clone(), self.connect_project_id.clone()) {
                    let now = TimeInstant::now();
                    let delay_secs = (1u64 << self.reconnect_attempt.min(5)).min(30);
                    let should_retry = match self.last_reconnect_time {
                        Some(last) => now.duration_since(last).as_secs() >= delay_secs,
                        None => true,
                    };
                    if should_retry {
                        let attempt = self.reconnect_attempt + 1;
                        // Persistent toast — stays until connected or max retries exhausted
                        self.toast_manager.push_persistent(
                            "reconnecting",
                            format!("Reconnecting {attempt}/{MAX_RECONNECT_ATTEMPTS}…"),
                            crate::ui::toast::ToastKind::Error,
                        );
                        log::info!("Reconnecting (attempt {attempt})...");
                        self.last_reconnect_time = Some(now);
                        self.reconnect_attempt += 1;
                        if self.reconnect_attempt <= MAX_RECONNECT_ATTEMPTS {
                            let pass = self.connect_password.clone();
                            self.connect_to_server(&url, &pid, pass.as_deref());
                        } else {
                            self.toast_manager.dismiss_by_id("reconnecting");
                            self.toast_manager.push(
                                "Connection failed after maximum retries",
                                crate::ui::toast::ToastKind::Error,
                            );
                        }
                    }
                }
            }
        }

        // --- Detect plugin GUI close → sync state blob ---
        self.poll_plugin_gui_close();

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
                            viewport: None,
                            playback: None,
                            editing_plugin: None,
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
                        viewport: None,
                        playback: None,
                        editing_plugin: None,
                    });
                }
                crate::user::EphemeralMessage::UserLeft { user_id } => {
                    self.remote_users.remove(&user_id);
                    if self.following_user == Some(user_id) {
                        self.following_user = None;
                    }
                }
                crate::user::EphemeralMessage::ViewportUpdate { user_id, position, zoom } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.viewport = Some(crate::user::RemoteViewport { position, zoom });
                    }
                }
                crate::user::EphemeralMessage::PlaybackUpdate { user_id, is_playing, position_seconds, timestamp_ms } => {
                    // Follow mode: sync play/stop transitions from followed user
                    #[cfg(feature = "native")]
                    if self.following_user == Some(user_id) {
                        if let Some(engine) = &self.audio_engine {
                            if is_playing && !engine.is_playing() {
                                engine.seek_to_seconds(position_seconds);
                                engine.toggle_playback();
                            } else if !is_playing && engine.is_playing() {
                                engine.toggle_playback();
                            }
                        }
                    }
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.playback = Some(crate::user::RemotePlaybackState {
                            is_playing, position_seconds, timestamp_ms,
                        });
                    }
                }
                crate::user::EphemeralMessage::PluginParamChange { user_id, chain_id, slot_idx, param_idx, value } => {
                    if user_id != self.local_user.id {
                        if let Some(chain) = self.effect_chains.get(&chain_id) {
                            if let Some(slot) = chain.slots.get(slot_idx) {
                                if let Ok(g) = slot.gui.lock() {
                                    if let Some(gui) = g.as_ref() {
                                        gui.set_parameter(param_idx, value);
                                    }
                                }
                            }
                        }
                    }
                }
                crate::user::EphemeralMessage::InstrumentParamChange { user_id, instrument_id, param_idx, value } => {
                    if user_id != self.local_user.id {
                        if let Some(inst) = self.instruments.get(&instrument_id) {
                            if let Ok(g) = inst.gui.lock() {
                                if let Some(gui) = g.as_ref() {
                                    gui.set_parameter(param_idx, value);
                                }
                            }
                        }
                    }
                }
                crate::user::EphemeralMessage::PluginGuiOpened { user_id, chain_id, slot_idx } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.editing_plugin = Some((chain_id, slot_idx));
                    }
                    if user_id != self.local_user.id {
                        self.open_effect_chain_slot_gui(chain_id, slot_idx);
                    }
                }
                crate::user::EphemeralMessage::PluginGuiClosed { user_id, chain_id: _, slot_idx: _ } => {
                    if let Some(state) = self.remote_users.get_mut(&user_id) {
                        state.editing_plugin = None;
                    }
                }
                crate::user::EphemeralMessage::InstrumentGuiOpened { user_id, instrument_id } => {
                    if user_id != self.local_user.id {
                        self.open_instrument_gui(instrument_id);
                    }
                }
                crate::user::EphemeralMessage::InstrumentGuiClosed { user_id: _, instrument_id: _ } => {
                    // No special handling needed — GUI close is local
                }
            }
            self.request_redraw();
        }

        // --- Follow mode: sync camera and playback to followed user ---
        if let Some(followed_id) = self.following_user {
            let mut should_unfollow = false;
            if let Some(remote) = self.remote_users.get(&followed_id) {
                if !remote.online {
                    should_unfollow = true;
                } else {
                    if let Some(viewport) = &remote.viewport {
                        self.camera.position = viewport.position;
                        self.camera.zoom = viewport.zoom;
                        self.request_redraw();
                    }
                }
            } else {
                should_unfollow = true;
            }
            if should_unfollow {
                self.following_user = None;
            }
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
                self.handle_cursor_moved();
            }

            // --- mouse buttons ---
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(state, button);
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                self.update_hover();
                self.update_cursor();
                self.request_redraw();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard_input(event);
            }

            // --- scroll = pan, Cmd+scroll = zoom, pinch = zoom ---
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta);
            }

            WindowEvent::PinchGesture { delta, .. } => {
                self.handle_pinch_gesture(delta);
            }

            WindowEvent::RedrawRequested => {
                self.handle_redraw();
            }

            _ => {}
        }
    }
}
