use super::*;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

use crate::midi_keyboard;

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

impl App {
    pub(crate) fn handle_keyboard_input(&mut self, event: KeyEvent) {
        #[cfg(feature = "native")]
        if event.state == ElementState::Released {
            self.handle_computer_midi_key_release(&event);
            return;
        }
        if event.state == ElementState::Pressed {
            println!("[KEY] pressed: {:?} super={} shift={}", event.logical_key, self.cmd_held(), self.modifiers.shift_key());
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
                if !self.cmd_held() {
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
                if self.cmd_held() && matches!(physical_key_to_char(&event.physical_key), Some("d")) {
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
                    let target = rw.target;
                    let current_db = ui::palette::gain_to_db(rw.volume);
                    let new_db = (current_db + delta_db).clamp(ui::palette::VOL_FADER_DB_BOTTOM, ui::palette::VOL_FADER_DB_MAX);
                    let new_gain = if new_db <= ui::palette::VOL_FADER_DB_BOTTOM { 0.0 } else { ui::palette::db_to_gain(new_db) };
                    if let Some(rw) = &mut self.right_window {
                        rw.volume = new_gain;
                    }
                    match target {
                        ui::right_window::RightWindowTarget::Waveform(wf_id) => {
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.volume = new_gain;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_id, before, after });
                                }
                                self.sync_audio_clips();
                            }
                        }
                        ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                            if let Some(inst) = self.instruments.get_mut(&inst_id) {
                                let before = crate::instruments::InstrumentSnapshot {
                                    name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
                                    plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
                                    volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id,
                                };
                                inst.volume = new_gain;
                                let after = crate::instruments::InstrumentSnapshot { volume: new_gain, ..before.clone() };
                                self.push_op(crate::operations::Operation::UpdateInstrument { id: inst_id, before, after });
                            }
                            self.sync_instrument_regions();
                        }
                    }
                    self.mark_dirty();
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
                    let target = rw.target;
                    let new_pan = (rw.pan + delta).clamp(0.0, 1.0);
                    if let Some(rw) = &mut self.right_window {
                        rw.pan = new_pan;
                    }
                    match target {
                        ui::right_window::RightWindowTarget::Waveform(wf_id) => {
                            if let Some(before) = self.waveforms.get(&wf_id).cloned() {
                                if let Some(wf) = self.waveforms.get_mut(&wf_id) {
                                    wf.pan = new_pan;
                                }
                                if let Some(after) = self.waveforms.get(&wf_id).cloned() {
                                    self.push_op(crate::operations::Operation::UpdateWaveform { id: wf_id, before, after });
                                }
                                self.sync_audio_clips();
                            }
                        }
                        ui::right_window::RightWindowTarget::Instrument(inst_id) => {
                            if let Some(inst) = self.instruments.get_mut(&inst_id) {
                                let before = crate::instruments::InstrumentSnapshot {
                                    name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
                                    plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
                                    volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id,
                                };
                                inst.pan = new_pan;
                                let after = crate::instruments::InstrumentSnapshot { pan: new_pan, ..before.clone() };
                                self.push_op(crate::operations::Operation::UpdateInstrument { id: inst_id, before, after });
                            }
                            self.sync_instrument_regions();
                        }
                    }
                    self.mark_dirty();
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
                    let wf_id = rw.target_id();
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
                    let wf_id = rw.target_id();
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
                && self.editing_text_note.is_none()
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
                                    #[cfg(feature = "native")]
                                    if let Some(engine) = &self.audio_engine {
                                        engine.set_bpm(self.bpm);
                                    }
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
                    Key::Character(ch) if !self.cmd_held() => {
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
                                let wf_id = self.right_window.as_ref().map(|rw| rw.target_id());
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
                    Key::Character(ch) if !self.cmd_held() => {
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
                                let wf_id = self.right_window.as_ref().map(|rw| rw.target_id());
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
                    Key::Character(ch) if !self.cmd_held() => {
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
                                let wf_id = self.right_window.as_ref().map(|rw| rw.target_id());
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
                    Key::Character(ch) if !self.cmd_held() => {
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

            // --- text note editing input ---
            if self.editing_text_note.is_some() {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        // Cancel editing — revert to before_text
                        if let Some(edit) = self.editing_text_note.take() {
                            if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                tn.text = edit.before_text;
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if self.cmd_held() {
                            // Cmd+Enter: commit edit
                            self.commit_text_note_edit();
                            self.request_redraw();
                            return;
                        }
                        // Regular Enter: insert newline
                        if let Some(ref mut edit) = self.editing_text_note {
                            edit.text.insert(edit.cursor, '\n');
                            edit.cursor += 1;
                            if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                tn.text = edit.text.clone();
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            if edit.cursor > 0 {
                                let prev = edit.text[..edit.cursor]
                                    .char_indices()
                                    .next_back()
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                edit.cursor = prev;
                                edit.text.remove(edit.cursor);
                                if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                    tn.text = edit.text.clone();
                                }
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Delete) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            if edit.cursor < edit.text.len() {
                                edit.text.remove(edit.cursor);
                                if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                    tn.text = edit.text.clone();
                                }
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            if edit.cursor > 0 {
                                edit.cursor = edit.text[..edit.cursor]
                                    .char_indices()
                                    .next_back()
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            if edit.cursor < edit.text.len() {
                                edit.cursor = edit.text[edit.cursor..]
                                    .char_indices()
                                    .nth(1)
                                    .map(|(i, _)| edit.cursor + i)
                                    .unwrap_or(edit.text.len());
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            // Find start of current line and column offset
                            let before = &edit.text[..edit.cursor];
                            if let Some(cur_line_start) = before.rfind('\n') {
                                let col = edit.cursor - cur_line_start - 1;
                                // Find start of previous line
                                let prev_line_start = before[..cur_line_start].rfind('\n')
                                    .map(|p| p + 1).unwrap_or(0);
                                let prev_line_len = cur_line_start - prev_line_start;
                                edit.cursor = prev_line_start + col.min(prev_line_len);
                            }
                            // If on first line, do nothing
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            let before = &edit.text[..edit.cursor];
                            let cur_line_start = before.rfind('\n')
                                .map(|p| p + 1).unwrap_or(0);
                            let col = edit.cursor - cur_line_start;
                            // Find end of current line (next \n)
                            if let Some(next_nl) = edit.text[edit.cursor..].find('\n') {
                                let next_line_start = edit.cursor + next_nl + 1;
                                let next_line_end = edit.text[next_line_start..].find('\n')
                                    .map(|p| next_line_start + p)
                                    .unwrap_or(edit.text.len());
                                let next_line_len = next_line_end - next_line_start;
                                edit.cursor = next_line_start + col.min(next_line_len);
                            }
                            // If on last line, do nothing
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Home) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            edit.cursor = 0;
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::End) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            edit.cursor = edit.text.len();
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Space) => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            edit.text.insert(edit.cursor, ' ');
                            edit.cursor += 1;
                            if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                tn.text = edit.text.clone();
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    Key::Character(ch) if self.cmd_held() => {
                        match ch.as_ref() {
                            "a" => {
                                // Select all (no-op for simple cursor model, just move to end)
                                if let Some(ref mut edit) = self.editing_text_note {
                                    edit.cursor = edit.text.len();
                                }
                                self.request_redraw();
                                return;
                            }
                            _ => {}
                        }
                    }
                    Key::Character(ch) if !self.cmd_held() => {
                        if let Some(ref mut edit) = self.editing_text_note {
                            for c in ch.chars() {
                                edit.text.insert(edit.cursor, c);
                                edit.cursor += c.len_utf8();
                            }
                            if let Some(tn) = self.text_notes.get_mut(&edit.note_id) {
                                tn.text = edit.text.clone();
                            }
                        }
                        self.render_generation += 1;
                        self.request_redraw();
                        return;
                    }
                    _ => {}
                }
            }

            // --- browser search bar input ---
            if self.sample_browser.search_focused {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.sample_browser.search_query.clear();
                        self.sample_browser.search_focused = false;
                        // Escape clears immediately (no debounce needed).
                        self.sample_browser.rebuild_entries();
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        if self.cmd_held() {
                            self.sample_browser.search_query.clear();
                        } else {
                            self.sample_browser.search_query.pop();
                        }
                        self.sample_browser.schedule_search_rebuild();
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Space) => {
                        self.sample_browser.search_query.push(' ');
                        self.sample_browser.schedule_search_rebuild();
                        self.request_redraw();
                        return;
                    }
                    Key::Character(ch) if !self.cmd_held() => {
                        self.sample_browser.search_query.push_str(ch.as_ref());
                        self.sample_browser.schedule_search_rebuild();
                        self.request_redraw();
                        return;
                    }
                    _ => {}
                }
            }

            // --- browser inline name editing input ---
            if self.sample_browser.editing_browser_name.is_some() {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.sample_browser.editing_browser_name = None;
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if let Some((id, kind, text)) = self.sample_browser.editing_browser_name.take() {
                            use crate::layers::LayerNodeKind;
                            match kind {
                                LayerNodeKind::Waveform => {
                                    if self.waveforms.contains_key(&id) {
                                        let before = self.waveforms[&id].clone();
                                        let wf = self.waveforms.get_mut(&id).unwrap();
                                        let name = if text.trim().is_empty() {
                                            wf.audio.filename.clone()
                                        } else {
                                            text
                                        };
                                        let mut new_audio = (*wf.audio).clone();
                                        new_audio.filename = name;
                                        wf.audio = std::sync::Arc::new(new_audio);
                                        let after = self.waveforms[&id].clone();
                                        self.push_op(crate::operations::Operation::UpdateWaveform { id, before, after });
                                        self.mark_dirty();
                                    }
                                }
                                LayerNodeKind::EffectRegion => {
                                    if self.effect_regions.contains_key(&id) {
                                        let before = self.effect_regions[&id].clone();
                                        let name = if text.trim().is_empty() {
                                            "effects".to_string()
                                        } else {
                                            text
                                        };
                                        self.effect_regions.get_mut(&id).unwrap().name = name;
                                        let after = self.effect_regions[&id].clone();
                                        self.push_op(crate::operations::Operation::UpdateEffectRegion { id, before, after });
                                        self.mark_dirty();
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let cmd = self.cmd_held();
                        if let Some((_, _, ref mut text)) = self.sample_browser.editing_browser_name {
                            if cmd {
                                text.clear();
                            } else {
                                text.pop();
                            }
                        }
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Space) => {
                        if let Some((_, _, ref mut text)) = self.sample_browser.editing_browser_name {
                            text.push(' ');
                        }
                        self.sample_browser.text_dirty = true;
                        self.request_redraw();
                        return;
                    }
                    Key::Character(ch) if !self.cmd_held() => {
                        if let Some((_, _, ref mut text)) = self.sample_browser.editing_browser_name {
                            text.push_str(ch.as_ref());
                        }
                        self.sample_browser.text_dirty = true;
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
                        let cmd = self.cmd_held();
                        if let Some((_, ref mut text)) = self.editing_effect_name {
                            if cmd {
                                text.clear();
                            } else {
                                text.pop();
                            }
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
                    Key::Character(ch) if !self.cmd_held() => {
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
                                self.mark_dirty();
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let cmd = self.cmd_held();
                        if let Some((_, ref mut text)) = self.editing_waveform_name {
                            if cmd {
                                text.clear();
                            } else {
                                text.pop();
                            }
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
                    Key::Character(ch) if !self.cmd_held() => {
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
                        Key::Character(ch) if !self.cmd_held() => {
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
                    Key::Character(ch) if !self.cmd_held() => {
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

            #[cfg(feature = "native")]
            {
                self.sync_keyboard_instrument_from_selection();
                self.sync_computer_keyboard_to_engine();
                if self.try_computer_midi_keyboard(&event) {
                    return;
                }
            }

            // --- global shortcuts ---
            match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.selected.clear();
                    self.update_right_window();
                    self.select_area = None;
                    #[cfg(feature = "native")]
                    {
                        self.sync_keyboard_instrument_from_selection();
                        self.sync_computer_keyboard_to_engine();
                    }
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
                Key::Character(ch) if !self.cmd_held() => match ch.as_ref() {
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
                _ if self.cmd_held() => {
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
                                    self.refresh_project_browser_entries();
                                    self.ensure_plugins_scanned();
                                }
                                self.request_redraw();
                            }
                            "f" => {
                                // Cmd+F: focus the browser search bar (open browser if needed)
                                if !self.sample_browser.visible {
                                    self.sample_browser.visible = true;
                                    #[cfg(feature = "native")]
                                    {
                                        self.refresh_project_browser_entries();
                                        self.ensure_plugins_scanned();
                                    }
                                }
                                self.sample_browser.search_focused = true;
                                self.sample_browser.text_dirty = true;
                                self.request_redraw();
                            }
                            "a" if self.modifiers.shift_key() => {
                                #[cfg(feature = "native")]
                                self.open_add_folder_dialog();
                            }
                            "r" => {
                                let (_, sh, scale) = self.screen_info();
                                let mouse_over_layers = self.sample_browser.visible
                                    && self.sample_browser.contains(self.mouse_pos, sh, scale)
                                    && self.sample_browser.active_category == ui::browser::BrowserCategory::Layers;
                                if mouse_over_layers {
                                    use crate::layers::LayerNodeKind;
                                    let browser_target = self.selected.iter().find_map(|t| match t {
                                        HitTarget::Waveform(id) => Some((*id, LayerNodeKind::Waveform)),
                                        HitTarget::EffectRegion(id) => Some((*id, LayerNodeKind::EffectRegion)),
                                        _ => None,
                                    });
                                    if let Some((id, kind)) = browser_target {
                                        let initial_text = match kind {
                                            LayerNodeKind::Waveform => self.waveforms.get(&id)
                                                .map(|wf| if !wf.audio.filename.is_empty() { wf.audio.filename.clone() } else { wf.filename.clone() })
                                                .unwrap_or_default(),
                                            LayerNodeKind::EffectRegion => self.effect_regions.get(&id)
                                                .map(|er| er.name.clone())
                                                .unwrap_or_default(),
                                            _ => String::new(),
                                        };
                                        self.sample_browser.editing_browser_name = Some((id, kind, initial_text));
                                        self.sample_browser.text_dirty = true;
                                    }
                                } else {
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
                            "g" => {
                                if self.modifiers.shift_key() {
                                    self.execute_command(CommandAction::UngroupSelected);
                                } else {
                                    self.execute_command(CommandAction::CreateGroup);
                                }
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
                            "[" => {
                                self.execute_command(CommandAction::MoveLayerUp);
                            }
                            "]" => {
                                self.execute_command(CommandAction::MoveLayerDown);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[cfg(feature = "native")]
    fn handle_computer_midi_key_release(&mut self, event: &KeyEvent) {
        if !self.computer_keyboard_armed {
            return;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if let Some((target, note)) = self.midi_keyboard_held.remove(&code) {
            if let Some(engine) = &self.audio_engine {
                engine.keyboard_preview_note_off(target, note);
            }
            self.request_redraw();
        }
    }

    #[cfg(feature = "native")]
    fn try_computer_midi_keyboard(&mut self, event: &KeyEvent) -> bool {
        if !self.computer_keyboard_armed || self.audio_engine.is_none() {
            return false;
        }
        if !self.computer_midi_keyboard_guards_ok() {
            return false;
        }

        let PhysicalKey::Code(code) = event.physical_key else {
            return false;
        };

        if !self.cmd_held() {
            match code {
                KeyCode::KeyZ => {
                    self.computer_keyboard_octave_offset = (self.computer_keyboard_octave_offset - 1)
                        .clamp(-midi_keyboard::OCTAVE_OFFSET_MAX, midi_keyboard::OCTAVE_OFFSET_MAX);
                    self.request_redraw();
                    return true;
                }
                KeyCode::KeyX => {
                    self.computer_keyboard_octave_offset = (self.computer_keyboard_octave_offset + 1)
                        .clamp(-midi_keyboard::OCTAVE_OFFSET_MAX, midi_keyboard::OCTAVE_OFFSET_MAX);
                    self.request_redraw();
                    return true;
                }
                KeyCode::KeyC => {
                    self.computer_keyboard_velocity = midi_keyboard::adjust_velocity(
                        self.computer_keyboard_velocity,
                        -(midi_keyboard::VELOCITY_STEP as i16),
                    );
                    self.request_redraw();
                    return true;
                }
                KeyCode::KeyV => {
                    self.computer_keyboard_velocity = midi_keyboard::adjust_velocity(
                        self.computer_keyboard_velocity,
                        midi_keyboard::VELOCITY_STEP as i16,
                    );
                    self.request_redraw();
                    return true;
                }
                _ => {}
            }
        }

        if self.cmd_held() {
            return false;
        }

        if let Some(base) = midi_keyboard::piano_key_midi_before_octave(&event.physical_key) {
            if self.midi_keyboard_held.contains_key(&code) {
                return true;
            }
            let note = match midi_keyboard::with_octave_offset(base, self.computer_keyboard_octave_offset) {
                Some(n) => n,
                None => return true,
            };
            let Some(target) = self.keyboard_instrument_id else {
                return true;
            };
            let can_send = self.instruments.get(&target).map_or(false, |inst| inst.has_plugin());
            if !can_send {
                return true;
            }
            self.midi_keyboard_held.insert(code, (target, note));
            if let Some(engine) = &self.audio_engine {
                engine.keyboard_preview_note_on(target, note, self.computer_keyboard_velocity);
            }
            self.request_redraw();
            return true;
        }

        false
    }

    #[cfg(feature = "native")]
    fn computer_midi_keyboard_guards_ok(&self) -> bool {
        self.command_palette.is_none()
            && self.settings_window.is_none()
            && self.plugin_editor.is_none()
            && self.context_menu.is_none()
            && self.editing_component.is_none()
            && self.editing_midi_clip.is_none()
            && !self.editing_bpm.is_editing()
            && self.editing_effect_name.is_none()
            && self.editing_waveform_name.is_none()
            && !self
                .right_window
                .as_ref()
                .map_or(false, |rw| rw.vol_entry.is_editing())
            && !self
                .right_window
                .as_ref()
                .map_or(false, |rw| rw.sample_bpm_entry.is_editing())
            && !self
                .right_window
                .as_ref()
                .map_or(false, |rw| rw.pitch_entry.is_editing())
    }
}
