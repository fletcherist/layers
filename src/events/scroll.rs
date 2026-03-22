use super::*;

use winit::event::MouseScrollDelta;

impl App {
    pub(crate) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
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
            self.cmd_held() || self.modifiers.control_key()
        } else {
            self.cmd_held()
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
        } else if self.modifiers.shift_key() {
            // Shift+scroll → horizontal pan (Ableton-style).
            // On macOS the OS converts Shift+vertical-scroll into a horizontal
            // scroll event (dx != 0, dy == 0), so prefer dx; fall back to dy
            // for plain mice that only emit a vertical delta.
            let horizontal = if dx != 0.0 { dx } else { dy };
            self.camera.position[0] -= horizontal / self.camera.zoom;
            self.broadcast_cursor_if_connected();
        } else {
            self.camera.position[0] -= dx / self.camera.zoom;
            self.camera.position[1] -= dy / self.camera.zoom;
            self.broadcast_cursor_if_connected();
        }

        self.update_hover();
        self.request_redraw();
    }

    pub(crate) fn handle_pinch_gesture(&mut self, delta: f64) {
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
}
