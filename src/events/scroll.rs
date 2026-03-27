use super::*;

use winit::event::MouseScrollDelta;

impl App {
    pub(crate) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.context_menu.is_some() || self.settings_window.is_some() || self.export_window.is_some() {
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

        // Right window volume/pan mousewheel
        if let Some(rw) = &self.right_window {
            let (sw, sh, scale) = self.screen_info();
            let target = rw.target;
            // Trackpad (PixelDelta): natural scrolling is inverted, reduce sensitivity
            // Mouse wheel (LineDelta): direct mapping, normal sensitivity
            let rw_step = if is_pixel_delta {
                -dy_raw / (800.0 * scale)
            } else {
                dy_raw * 0.03
            };
            if rw.hit_test_vol_track(self.mouse_pos, sw, sh, scale) {
                let current_pos = ui::palette::gain_to_vol_fader_pos(rw.volume);
                let new_pos = (current_pos + rw_step).clamp(0.0, 1.0);
                let new_vol = ui::palette::vol_fader_pos_to_gain(new_pos);
                if let Some(rw) = &mut self.right_window {
                    rw.volume = new_vol;
                }
                match target {
                    ui::right_window::RightWindowTarget::Waveform(wf_id) => {
                        let before = self.waveforms.get(&wf_id).cloned();
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.volume = new_vol;
                        }
                        if let Some(before) = before {
                            if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                self.push_op(crate::operations::Operation::UpdateWaveform {
                                    id: wf_id, before, after,
                                });
                            }
                        }
                        self.sync_audio_clips();
                    }
                    ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                        if let Some(inst) = self.instruments.get_mut(&inst_id) {
                            let before = crate::instruments::InstrumentSnapshot {
                                name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
                                plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
                                volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id, disabled: inst.disabled,
                            };
                            inst.volume = new_vol;
                            let after = crate::instruments::InstrumentSnapshot { volume: new_vol, ..before.clone() };
                            self.push_op(crate::operations::Operation::UpdateInstrument { id: inst_id, before, after });
                        }
                        self.sync_instrument_regions_auto();
                    }
                    ui::right_window::RightWindowTarget::Group(gid) => {
                        if let Some(before) = self.groups.get(&gid).cloned() {
                            if let Some(g) = self.groups.get_mut(&gid) {
                                g.volume = new_vol;
                            }
                            if let Some(after) = self.groups.get(&gid).cloned() {
                                self.push_op(crate::operations::Operation::UpdateGroup { id: gid, before, after });
                            }
                        }
                        self.sync_audio_clips();
                    }
                    ui::right_window::RightWindowTarget::Master => {
                        self.master.volume = new_vol;
                        self.sync_audio_clips();
                    }
                }
                self.request_redraw();
                return;
            }
            if rw.hit_test_pan_knob(self.mouse_pos, sw, sh, scale) {
                let new_pan = (rw.pan + rw_step).clamp(0.0, 1.0);
                if let Some(rw) = &mut self.right_window {
                    rw.pan = new_pan;
                }
                match target {
                    ui::right_window::RightWindowTarget::Waveform(wf_id) => {
                        let before = self.waveforms.get(&wf_id).cloned();
                        if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                            wf.pan = new_pan;
                        }
                        if let Some(before) = before {
                            if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                self.push_op(crate::operations::Operation::UpdateWaveform {
                                    id: wf_id, before, after,
                                });
                            }
                        }
                        self.sync_audio_clips();
                    }
                    ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                        if let Some(inst) = self.instruments.get_mut(&inst_id) {
                            let before = crate::instruments::InstrumentSnapshot {
                                name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
                                plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
                                volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id, disabled: inst.disabled,
                            };
                            inst.pan = new_pan;
                            let after = crate::instruments::InstrumentSnapshot { pan: new_pan, ..before.clone() };
                            self.push_op(crate::operations::Operation::UpdateInstrument { id: inst_id, before, after });
                        }
                        self.sync_instrument_regions_auto();
                    }
                    ui::right_window::RightWindowTarget::Group(gid) => {
                        if let Some(before) = self.groups.get(&gid).cloned() {
                            if let Some(g) = self.groups.get_mut(&gid) {
                                g.pan = new_pan;
                            }
                            if let Some(after) = self.groups.get(&gid).cloned() {
                                self.push_op(crate::operations::Operation::UpdateGroup { id: gid, before, after });
                            }
                        }
                        self.sync_audio_clips();
                    }
                    ui::right_window::RightWindowTarget::Master => {
                        self.master.pan = new_pan;
                        self.sync_audio_clips();
                    }
                }
                self.request_redraw();
                return;
            }
        }

        // Break follow mode on any local pan/zoom
        self.following_user = None;

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
        if self.command_palette.is_some() || self.context_menu.is_some() || self.settings_window.is_some() || self.export_window.is_some() {
            return;
        }
        self.following_user = None;
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
