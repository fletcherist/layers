use super::*;

impl App {
    // -----------------------------------------------------------------------
    // Non-live overlap resolution (for clipboard paste, audio load, etc.)
    // Returns ops for undo support. Mutates waveforms/midi_clips in place.
    // -----------------------------------------------------------------------

    /// Resolve overlapping clips (both waveforms and MIDI clips).
    /// `active_ids` are the clips that "win" — other clips on the same track
    /// (Y-overlap) that collide horizontally get cropped or deleted.
    /// Returns ops describing all mutations (for undo support).
    pub(crate) fn resolve_clip_overlaps(&mut self, active_ids: &[EntityId]) -> Vec<operations::Operation> {
        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut to_delete_wf: HashSet<EntityId> = HashSet::new();
        let mut to_delete_mc: HashSet<EntityId> = HashSet::new();
        let mut wf_updates: Vec<(EntityId, ui::waveform::WaveformView, ui::waveform::WaveformView)> = Vec::new();
        let mut mc_updates: Vec<(EntityId, midi::MidiClip, midi::MidiClip)> = Vec::new();
        let mut wf_creates: Vec<(EntityId, ui::waveform::WaveformView, Option<AudioClipData>)> = Vec::new();
        let mut mc_creates: Vec<(EntityId, midi::MidiClip)> = Vec::new();

        for &aid in active_ids {
            // Get active clip rect from either map
            let (a_start, a_end, a_y0, a_y1) = if let Some(wf) = self.waveforms.get(&aid) {
                (wf.position[0], wf.position[0] + wf.size[0], wf.position[1], wf.position[1] + wf.size[1])
            } else if let Some(mc) = self.midi_clips.get(&aid) {
                (mc.position[0], mc.position[0] + mc.size[0], mc.position[1], mc.position[1] + mc.size[1])
            } else {
                continue;
            };

            // Check against all waveforms
            let wf_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id) && !to_delete_wf.contains(id))
                .copied()
                .collect();

            for bid in wf_ids {
                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) => wf,
                    None => continue,
                };

                // Skip overlap resolution for waveforms in the same take group
                if let Some(a_wf) = self.waveforms.get(&aid) {
                    if let Some(tg) = &a_wf.take_group {
                        if tg.contains(bid) { continue; }
                    }
                }
                if let Some(b_tg_parent) = self.find_take_parent(bid) {
                    if b_tg_parent == aid { continue; }
                    if self.find_take_parent(aid) == Some(b_tg_parent) { continue; }
                }
                if let Some(a_tg_parent) = self.find_take_parent(aid) {
                    if let Some(p_wf) = self.waveforms.get(&a_tg_parent) {
                        if let Some(tg) = &p_wf.take_group {
                            if tg.contains(bid) { continue; }
                        }
                    }
                }

                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }

                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];

                // Case 1: B fully covered by A
                if b_start >= a_start && b_end <= a_end {
                    to_delete_wf.insert(bid);
                    continue;
                }

                // Case 4: A fully inside B — split B
                if b_start < a_start && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;

                    if left_width >= WAVEFORM_MIN_WIDTH_PX {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = left_width;
                        wf.fade_out_px = 0.0;
                        if wf.fade_in_px > left_width * 0.5 { wf.fade_in_px = left_width * 0.5; }
                        wf_updates.push((bid, before.clone(), wf.clone()));
                    } else {
                        to_delete_wf.insert(bid);
                    }

                    if right_width >= WAVEFORM_MIN_WIDTH_PX {
                        let right_id = new_id();
                        let mut right_wf = before.clone();
                        right_wf.position[0] = a_end;
                        right_wf.size[0] = right_width;
                        right_wf.sample_offset_px += a_end - b_start;
                        right_wf.fade_in_px = 0.0;
                        if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                        let ac = self.audio_clips.get(&bid).cloned();
                        self.waveforms.insert(right_id, right_wf.clone());
                        if let Some(clip) = ac.clone() { self.audio_clips.insert(right_id, clip); }
                        wf_creates.push((right_id, right_wf, ac));

                        for comp in self.components.values_mut() {
                            if comp.waveform_ids.contains(&bid) {
                                let mut new_ids = Vec::new();
                                for &wi in &comp.waveform_ids {
                                    new_ids.push(wi);
                                    if wi == bid { new_ids.push(right_id); }
                                }
                                comp.waveform_ids = new_ids;
                            }
                        }
                    }
                    continue;
                }

                // Case 2: B's tail overlaps A's start
                if b_start < a_start && b_end > a_start {
                    let before = self.waveforms[&bid].clone();
                    let new_width = a_start - b_start;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete_wf.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = new_width;
                        if wf.fade_out_px > new_width * 0.5 { wf.fade_out_px = new_width * 0.5; }
                        wf_updates.push((bid, before, wf.clone()));
                    }
                }

                // Case 3: B's head overlaps A's end
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete_wf.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.position[0] = a_end;
                        wf.size[0] = new_width;
                        wf.sample_offset_px += crop_amount;
                        if wf.fade_in_px > new_width * 0.5 { wf.fade_in_px = new_width * 0.5; }
                        wf_updates.push((bid, before, wf.clone()));
                    }
                }
            }

            // Check against all MIDI clips
            let mc_ids: Vec<EntityId> = self.midi_clips.keys()
                .filter(|id| !active_set.contains(id) && !to_delete_mc.contains(id))
                .copied()
                .collect();

            for bid in mc_ids {
                let bmc = match self.midi_clips.get(&bid) {
                    Some(mc) => mc,
                    None => continue,
                };
                let b_y0 = bmc.position[1];
                let b_y1 = b_y0 + bmc.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }

                let b_start = bmc.position[0];
                let b_end = b_start + bmc.size[0];

                // Case 1: B fully covered by A
                if b_start >= a_start && b_end <= a_end {
                    to_delete_mc.insert(bid);
                    continue;
                }

                // Case 4: A fully inside B — split B
                if b_start < a_start && b_end > a_end {
                    let before = self.midi_clips[&bid].clone();
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;

                    if left_width >= Self::MIDI_CLIP_MIN_WIDTH_PX {
                        let mc = self.midi_clips.get_mut(&bid).unwrap();
                        mc.size[0] = left_width;
                        mc.notes.retain(|n| n.start_px < left_width);
                        for n in &mut mc.notes { n.duration_px = n.duration_px.min(left_width - n.start_px); }
                        mc_updates.push((bid, before.clone(), mc.clone()));
                    } else {
                        to_delete_mc.insert(bid);
                    }

                    if right_width >= Self::MIDI_CLIP_MIN_WIDTH_PX {
                        let right_id = new_id();
                        let mut right_mc = before.clone();
                        let crop_left = a_end - b_start;
                        right_mc.position[0] = a_end;
                        right_mc.size[0] = right_width;
                        right_mc.notes.retain(|n| n.start_px + n.duration_px > crop_left);
                        for n in &mut right_mc.notes {
                            let new_start = (n.start_px - crop_left).max(0.0);
                            if n.start_px < crop_left { n.duration_px -= crop_left - n.start_px; }
                            n.start_px = new_start;
                        }
                        self.midi_clips.insert(right_id, right_mc.clone());
                        mc_creates.push((right_id, right_mc));
                    }
                    continue;
                }

                // Case 2: B's tail overlaps A's start
                if b_start < a_start && b_end > a_start {
                    let before = self.midi_clips[&bid].clone();
                    let new_width = a_start - b_start;
                    if new_width < Self::MIDI_CLIP_MIN_WIDTH_PX {
                        to_delete_mc.insert(bid);
                    } else {
                        let mc = self.midi_clips.get_mut(&bid).unwrap();
                        mc.size[0] = new_width;
                        mc.notes.retain(|n| n.start_px < new_width);
                        for n in &mut mc.notes { n.duration_px = n.duration_px.min(new_width - n.start_px); }
                        mc_updates.push((bid, before, mc.clone()));
                    }
                }

                // Case 3: B's head overlaps A's end
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let before = self.midi_clips[&bid].clone();
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < Self::MIDI_CLIP_MIN_WIDTH_PX {
                        to_delete_mc.insert(bid);
                    } else {
                        let mc = self.midi_clips.get_mut(&bid).unwrap();
                        mc.position[0] = a_end;
                        mc.size[0] = new_width;
                        mc.notes.retain(|n| n.start_px + n.duration_px > crop_amount);
                        for n in &mut mc.notes {
                            let new_start = (n.start_px - crop_amount).max(0.0);
                            if n.start_px < crop_amount { n.duration_px -= crop_amount - n.start_px; }
                            n.start_px = new_start;
                        }
                        mc_updates.push((bid, before, mc.clone()));
                    }
                }
            }
        }

        // Build ops
        let mut ops: Vec<operations::Operation> = Vec::new();
        for (id, before, after) in wf_updates {
            if !to_delete_wf.contains(&id) {
                ops.push(operations::Operation::UpdateWaveform { id, before, after });
            }
        }
        for (id, before, after) in mc_updates {
            if !to_delete_mc.contains(&id) {
                ops.push(operations::Operation::UpdateMidiClip { id, before, after });
            }
        }
        for (id, wf_data, ac) in wf_creates {
            ops.push(operations::Operation::CreateWaveform { id, data: wf_data, audio_clip: ac.map(|c| (id, c)) });
        }
        for (id, mc_data) in mc_creates {
            ops.push(operations::Operation::CreateMidiClip { id, data: mc_data });
        }
        for &id in &to_delete_wf {
            if let Some(data) = self.waveforms.shift_remove(&id) {
                let ac = self.audio_clips.shift_remove(&id);
                ops.push(operations::Operation::DeleteWaveform { id, data, audio_clip: ac.map(|c| (id, c)) });
            }
        }
        for &id in &to_delete_mc {
            if let Some(data) = self.midi_clips.shift_remove(&id) {
                ops.push(operations::Operation::DeleteMidiClip { id, data });
            }
        }
        ops
    }

    /// Resolve mutual overlaps among ALL clips on the same track.
    /// Rightmost clip wins; clips to its left get cropped/deleted.
    /// Used after BPM changes where every clip's position shifts.
    pub(crate) fn resolve_all_clip_overlaps(&mut self) -> Vec<operations::Operation> {
        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .chain(self.midi_clips.iter().map(|(&id, mc)| (id, mc.position[0])))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut all_ops = Vec::new();

        for (id, _) in &sorted {
            if !self.waveforms.contains_key(id) && !self.midi_clips.contains_key(id) { continue; }
            let ops = self.resolve_clip_overlaps(&[*id]);
            all_ops.extend(ops);
        }
        all_ops
    }

    /// Same as `resolve_all_clip_overlaps` but live (uses snapshots for restore).
    pub(crate) fn resolve_all_clip_overlaps_live(
        &mut self,
        snapshots: &mut IndexMap<EntityId, ClipSnapshot>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
            self.midi_clips.shift_remove(&id);
        }

        for (id, original) in snapshots.iter() {
            match original {
                ClipSnapshot::Waveform(wf) => {
                    if let Some(existing) = self.waveforms.get_mut(id) {
                        *existing = wf.clone();
                    } else {
                        self.waveforms.insert(*id, wf.clone());
                    }
                }
                ClipSnapshot::MidiClip(mc) => {
                    if let Some(existing) = self.midi_clips.get_mut(id) {
                        *existing = mc.clone();
                    } else {
                        self.midi_clips.insert(*id, mc.clone());
                    }
                }
            }
        }

        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .chain(self.midi_clips.iter().map(|(&id, mc)| (id, mc.position[0])))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut new_snapshots: IndexMap<EntityId, ClipSnapshot> = IndexMap::new();

        for (active_id, _) in &sorted {
            // Get active clip rect
            let (a_start, a_end, a_y0, a_y1) = if let Some(wf) = self.waveforms.get(active_id) {
                (wf.position[0], wf.position[0] + wf.size[0], wf.position[1], wf.position[1] + wf.size[1])
            } else if let Some(mc) = self.midi_clips.get(active_id) {
                (mc.position[0], mc.position[0] + mc.size[0], mc.position[1], mc.position[1] + mc.size[1])
            } else {
                continue;
            };

            // Check against other waveforms
            let wf_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| *id != active_id)
                .copied()
                .collect();

            for bid in wf_ids {
                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) if !wf.disabled => wf,
                    _ => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];
                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                if !snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, ClipSnapshot::Waveform(bwf.clone()));
                } else if snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                self.apply_waveform_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
            }

            // Check against other MIDI clips
            let mc_ids: Vec<EntityId> = self.midi_clips.keys()
                .filter(|id| *id != active_id)
                .copied()
                .collect();

            for bid in mc_ids {
                let bmc = match self.midi_clips.get(&bid) {
                    Some(mc) if !mc.disabled => mc,
                    _ => continue,
                };
                let b_y0 = bmc.position[1];
                let b_y1 = b_y0 + bmc.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bmc.position[0];
                let b_end = b_start + bmc.size[0];
                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                if !snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, ClipSnapshot::MidiClip(bmc.clone()));
                } else if snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                self.apply_midi_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
            }
        }

        let prev_keys: Vec<EntityId> = snapshots.keys().copied().collect();
        for id in prev_keys {
            if !new_snapshots.contains_key(&id) {
                snapshots.shift_remove(&id);
            }
        }
        for (id, original) in new_snapshots {
            snapshots.entry(id).or_insert(original);
        }
    }

    // -----------------------------------------------------------------------
    // Live overlap resolution during drag (unified for all clip types)
    // -----------------------------------------------------------------------

    const MIDI_CLIP_MIN_WIDTH_PX: f32 = 10.0;

    /// Live overlap resolution for all clips during drag.
    /// `active_ids` are the clips being moved/resized (they win).
    /// Works across both waveforms and MIDI clips — any clip type can
    /// overlap with any other clip type.
    pub(crate) fn resolve_clip_overlaps_live(
        &mut self,
        active_ids: &[EntityId],
        snapshots: &mut IndexMap<EntityId, ClipSnapshot>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        // 0. Remove temporary split clips from previous frame
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
            self.midi_clips.shift_remove(&id);
        }

        // 1. Restore all previously-affected clips to their original state
        for (id, original) in snapshots.iter() {
            match original {
                ClipSnapshot::Waveform(wf) => {
                    if let Some(existing) = self.waveforms.get_mut(id) {
                        *existing = wf.clone();
                    } else {
                        self.waveforms.insert(*id, wf.clone());
                    }
                }
                ClipSnapshot::MidiClip(mc) => {
                    if let Some(existing) = self.midi_clips.get_mut(id) {
                        *existing = mc.clone();
                    } else {
                        self.midi_clips.insert(*id, mc.clone());
                    }
                }
            }
        }

        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut new_snapshots: IndexMap<EntityId, ClipSnapshot> = IndexMap::new();

        for &aid in active_ids {
            // Get active clip rect from either map
            let (a_start, a_end, a_y0, a_y1) = if let Some(wf) = self.waveforms.get(&aid) {
                (wf.position[0], wf.position[0] + wf.size[0], wf.position[1], wf.position[1] + wf.size[1])
            } else if let Some(mc) = self.midi_clips.get(&aid) {
                (mc.position[0], mc.position[0] + mc.size[0], mc.position[1], mc.position[1] + mc.size[1])
            } else {
                continue;
            };

            // --- Check against all waveforms ---
            let wf_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id))
                .copied()
                .collect();

            for bid in wf_ids {
                // Handle already-snapshotted waveforms (may have been modified by a previous active clip)
                if new_snapshots.contains_key(&bid) {
                    let bwf = match self.waveforms.get(&bid) {
                        Some(wf) if !wf.disabled => wf,
                        _ => continue,
                    };
                    let b_y0 = bwf.position[1];
                    let b_y1 = b_y0 + bwf.size[1];
                    if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                    let b_start = bwf.position[0];
                    let b_end = b_start + bwf.size[0];

                    self.apply_waveform_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
                    continue;
                }

                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) => wf,
                    None => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];

                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                // Snapshot the original state
                if !snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, ClipSnapshot::Waveform(bwf.clone()));
                } else if !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                self.apply_waveform_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
            }

            // --- Check against all MIDI clips ---
            let mc_ids: Vec<EntityId> = self.midi_clips.keys()
                .filter(|id| !active_set.contains(id))
                .copied()
                .collect();

            for bid in mc_ids {
                // Handle already-snapshotted MIDI clips
                if new_snapshots.contains_key(&bid) {
                    let bmc = match self.midi_clips.get(&bid) {
                        Some(mc) if !mc.disabled => mc,
                        _ => continue,
                    };
                    let b_y0 = bmc.position[1];
                    let b_y1 = b_y0 + bmc.size[1];
                    if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                    let b_start = bmc.position[0];
                    let b_end = b_start + bmc.size[0];

                    self.apply_midi_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
                    continue;
                }

                let bmc = match self.midi_clips.get(&bid) {
                    Some(mc) if !mc.disabled => mc,
                    _ => continue,
                };
                let b_y0 = bmc.position[1];
                let b_y1 = b_y0 + bmc.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                let b_start = bmc.position[0];
                let b_end = b_start + bmc.size[0];

                let has_x_overlap = b_start < a_end && b_end > a_start;
                if !has_x_overlap { continue; }

                // Snapshot the original state
                if !snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, ClipSnapshot::MidiClip(bmc.clone()));
                } else if !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                self.apply_midi_overlap_case(bid, a_start, a_end, b_start, b_end, &new_snapshots, snapshots, temp_splits);
            }
        }

        // Update snapshots
        let prev_keys: Vec<EntityId> = snapshots.keys().copied().collect();
        for id in prev_keys {
            if !new_snapshots.contains_key(&id) {
                snapshots.shift_remove(&id);
            }
        }
        for (id, original) in new_snapshots {
            snapshots.entry(id).or_insert(original);
        }
    }

    // -----------------------------------------------------------------------
    // Type-specific overlap case helpers
    // -----------------------------------------------------------------------

    fn apply_waveform_overlap_case(
        &mut self,
        bid: EntityId,
        a_start: f32,
        a_end: f32,
        b_start: f32,
        b_end: f32,
        new_snapshots: &IndexMap<EntityId, ClipSnapshot>,
        snapshots: &IndexMap<EntityId, ClipSnapshot>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        // Case 1: B fully covered by A
        if b_start >= a_start && b_end <= a_end {
            self.waveforms.get_mut(&bid).unwrap().disabled = true;
            return;
        }

        // Case 4: A fully inside B — split B
        if b_start < a_start && b_end > a_end {
            let left_width = a_start - b_start;
            let right_width = b_end - a_end;
            if left_width < WAVEFORM_MIN_WIDTH_PX {
                self.waveforms.get_mut(&bid).unwrap().disabled = true;
            } else {
                let wf = self.waveforms.get_mut(&bid).unwrap();
                wf.size[0] = left_width;
                wf.fade_out_px = 0.0;
                if wf.fade_in_px > left_width * 0.5 { wf.fade_in_px = left_width * 0.5; }
            }
            if right_width >= WAVEFORM_MIN_WIDTH_PX {
                let orig = Self::get_waveform_snapshot(bid, new_snapshots, snapshots)
                    .unwrap_or_else(|| self.waveforms[&bid].clone());
                let mut right_wf = orig;
                right_wf.position[0] = a_end;
                right_wf.size[0] = right_width;
                right_wf.sample_offset_px += a_end - b_start;
                right_wf.fade_in_px = 0.0;
                if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                let right_id = new_id();
                if let Some(clip) = self.audio_clips.get(&bid).cloned() {
                    self.audio_clips.insert(right_id, clip);
                }
                self.waveforms.insert(right_id, right_wf);
                temp_splits.push(right_id);
            }
            return;
        }

        // Case 2: B's tail overlaps A's start
        if b_start < a_start && b_end > a_start {
            let new_width = a_start - b_start;
            if new_width < WAVEFORM_MIN_WIDTH_PX {
                self.waveforms.get_mut(&bid).unwrap().disabled = true;
            } else {
                let wf = self.waveforms.get_mut(&bid).unwrap();
                wf.size[0] = new_width;
                if wf.fade_out_px > new_width * 0.5 { wf.fade_out_px = new_width * 0.5; }
            }
        }

        // Case 3: B's head overlaps A's end
        if b_start >= a_start && b_start < a_end && b_end > a_end {
            let crop_amount = a_end - b_start;
            let new_width = b_end - a_end;
            if new_width < WAVEFORM_MIN_WIDTH_PX {
                self.waveforms.get_mut(&bid).unwrap().disabled = true;
            } else {
                let wf = self.waveforms.get_mut(&bid).unwrap();
                wf.position[0] = a_end;
                wf.size[0] = new_width;
                wf.sample_offset_px += crop_amount;
                if wf.fade_in_px > new_width * 0.5 { wf.fade_in_px = new_width * 0.5; }
            }
        }
    }

    fn apply_midi_overlap_case(
        &mut self,
        bid: EntityId,
        a_start: f32,
        a_end: f32,
        b_start: f32,
        b_end: f32,
        new_snapshots: &IndexMap<EntityId, ClipSnapshot>,
        snapshots: &IndexMap<EntityId, ClipSnapshot>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        // Case 1: B fully covered by A → disable
        if b_start >= a_start && b_end <= a_end {
            self.midi_clips.get_mut(&bid).unwrap().disabled = true;
            return;
        }

        // Case 4: A fully inside B → split B
        if b_start < a_start && b_end > a_end {
            let left_width = a_start - b_start;
            let right_width = b_end - a_end;
            if left_width < Self::MIDI_CLIP_MIN_WIDTH_PX {
                self.midi_clips.get_mut(&bid).unwrap().disabled = true;
            } else {
                let mc = self.midi_clips.get_mut(&bid).unwrap();
                mc.size[0] = left_width;
                mc.notes.retain(|n| n.start_px < left_width);
                for n in &mut mc.notes { n.duration_px = n.duration_px.min(left_width - n.start_px); }
            }
            if right_width >= Self::MIDI_CLIP_MIN_WIDTH_PX {
                let orig = Self::get_midi_snapshot(bid, new_snapshots, snapshots)
                    .unwrap_or_else(|| self.midi_clips[&bid].clone());
                let mut right_mc = orig;
                let crop_left = a_end - b_start;
                right_mc.position[0] = a_end;
                right_mc.size[0] = right_width;
                right_mc.notes.retain(|n| n.start_px + n.duration_px > crop_left);
                for n in &mut right_mc.notes {
                    let new_start = (n.start_px - crop_left).max(0.0);
                    if n.start_px < crop_left { n.duration_px -= crop_left - n.start_px; }
                    n.start_px = new_start;
                }
                let right_id = new_id();
                self.midi_clips.insert(right_id, right_mc);
                temp_splits.push(right_id);
            }
            return;
        }

        // Case 2: B's tail overlaps A's start
        if b_start < a_start && b_end > a_start {
            let new_width = a_start - b_start;
            if new_width < Self::MIDI_CLIP_MIN_WIDTH_PX {
                self.midi_clips.get_mut(&bid).unwrap().disabled = true;
            } else {
                let mc = self.midi_clips.get_mut(&bid).unwrap();
                mc.size[0] = new_width;
                mc.notes.retain(|n| n.start_px < new_width);
                for n in &mut mc.notes { n.duration_px = n.duration_px.min(new_width - n.start_px); }
            }
        }

        // Case 3: B's head overlaps A's end
        if b_start >= a_start && b_start < a_end && b_end > a_end {
            let crop_amount = a_end - b_start;
            let new_width = b_end - a_end;
            if new_width < Self::MIDI_CLIP_MIN_WIDTH_PX {
                self.midi_clips.get_mut(&bid).unwrap().disabled = true;
            } else {
                let mc = self.midi_clips.get_mut(&bid).unwrap();
                mc.position[0] = a_end;
                mc.size[0] = new_width;
                mc.notes.retain(|n| n.start_px + n.duration_px > crop_amount);
                for n in &mut mc.notes {
                    let new_start = (n.start_px - crop_amount).max(0.0);
                    if n.start_px < crop_amount { n.duration_px -= crop_amount - n.start_px; }
                    n.start_px = new_start;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Snapshot lookup helpers
    // -----------------------------------------------------------------------

    fn get_waveform_snapshot(
        id: EntityId,
        new_snapshots: &IndexMap<EntityId, ClipSnapshot>,
        snapshots: &IndexMap<EntityId, ClipSnapshot>,
    ) -> Option<WaveformView> {
        new_snapshots.get(&id)
            .or_else(|| snapshots.get(&id))
            .and_then(|s| match s {
                ClipSnapshot::Waveform(wf) => Some(wf.clone()),
                _ => None,
            })
    }

    fn get_midi_snapshot(
        id: EntityId,
        new_snapshots: &IndexMap<EntityId, ClipSnapshot>,
        snapshots: &IndexMap<EntityId, ClipSnapshot>,
    ) -> Option<midi::MidiClip> {
        new_snapshots.get(&id)
            .or_else(|| snapshots.get(&id))
            .and_then(|s| match s {
                ClipSnapshot::MidiClip(mc) => Some(mc.clone()),
                _ => None,
            })
    }

    // -----------------------------------------------------------------------
    // Commit helper — converts snapshots + temp_splits into Operations
    // -----------------------------------------------------------------------

    pub(crate) fn commit_overlap_ops(
        &mut self,
        snapshots: IndexMap<EntityId, ClipSnapshot>,
        temp_splits: Vec<EntityId>,
    ) -> Vec<operations::Operation> {
        let mut ops = Vec::new();
        for (id, snapshot) in &snapshots {
            match snapshot {
                ClipSnapshot::Waveform(original) => {
                    if let Some(wf) = self.waveforms.get(id) {
                        if wf.disabled {
                            self.waveforms.shift_remove(id);
                            let ac = self.audio_clips.shift_remove(id);
                            ops.push(operations::Operation::DeleteWaveform {
                                id: *id, data: original.clone(), audio_clip: ac.map(|c| (*id, c)),
                            });
                        } else {
                            ops.push(operations::Operation::UpdateWaveform {
                                id: *id, before: original.clone(), after: wf.clone(),
                            });
                        }
                    }
                }
                ClipSnapshot::MidiClip(original) => {
                    if let Some(mc) = self.midi_clips.get(id) {
                        if mc.disabled {
                            self.midi_clips.shift_remove(id);
                            ops.push(operations::Operation::DeleteMidiClip {
                                id: *id, data: original.clone(),
                            });
                        } else {
                            ops.push(operations::Operation::UpdateMidiClip {
                                id: *id, before: original.clone(), after: mc.clone(),
                            });
                        }
                    }
                }
            }
        }
        for id in &temp_splits {
            if let Some(wf_data) = self.waveforms.get(id).cloned() {
                let ac = self.audio_clips.get(id).cloned();
                ops.push(operations::Operation::CreateWaveform {
                    id: *id, data: wf_data, audio_clip: ac.map(|c| (*id, c)),
                });
            } else if let Some(mc_data) = self.midi_clips.get(id).cloned() {
                ops.push(operations::Operation::CreateMidiClip { id: *id, data: mc_data });
            }
        }
        ops
    }
}
