use super::*;

impl App {
    /// Resolve overlapping audio waveforms, analogous to `MidiClip::resolve_note_overlaps`.
    /// `active_ids` are the waveforms that "win" — other waveforms on the same track
    /// (Y-overlap) that collide horizontally get cropped or deleted.
    /// Returns ops describing all mutations (for undo support).
    pub(crate) fn resolve_waveform_overlaps(&mut self, active_ids: &[EntityId]) -> Vec<operations::Operation> {
        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut to_delete: HashSet<EntityId> = HashSet::new();
        let mut updates: Vec<(EntityId, ui::waveform::WaveformView, ui::waveform::WaveformView)> = Vec::new();
        let mut to_create: Vec<(EntityId, ui::waveform::WaveformView, Option<AudioClipData>)> = Vec::new();

        for &aid in active_ids {
            let (a_pos, a_size) = match self.waveforms.get(&aid) {
                Some(wf) => (wf.position, wf.size),
                None => continue,
            };
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id) && !to_delete.contains(id))
                .copied()
                .collect();

            for bid in other_ids {
                let bwf = match self.waveforms.get(&bid) {
                    Some(wf) => wf,
                    None => continue,
                };
                let b_y0 = bwf.position[1];
                let b_y1 = b_y0 + bwf.size[1];
                if !(a_y0 < b_y1 && a_y1 > b_y0) {
                    continue;
                }

                let b_start = bwf.position[0];
                let b_end = b_start + bwf.size[0];

                // Case 1: B fully covered by A
                if b_start >= a_start && b_end <= a_end {
                    to_delete.insert(bid);
                    continue;
                }

                // Case 4: A fully inside B — split B into left and right portions
                if b_start < a_start && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let left_width = a_start - b_start;
                    let right_width = b_end - a_end;

                    if left_width >= WAVEFORM_MIN_WIDTH_PX {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = left_width;
                        wf.fade_out_px = 0.0;
                        if wf.fade_in_px > left_width * 0.5 {
                            wf.fade_in_px = left_width * 0.5;
                        }
                        updates.push((bid, before.clone(), wf.clone()));
                    } else {
                        to_delete.insert(bid);
                    }

                    if right_width >= WAVEFORM_MIN_WIDTH_PX {
                        let right_id = new_id();
                        let mut right_wf = before.clone();
                        right_wf.position[0] = a_end;
                        right_wf.size[0] = right_width;
                        right_wf.sample_offset_px += a_end - b_start;
                        right_wf.fade_in_px = 0.0;
                        if right_wf.fade_out_px > right_width * 0.5 {
                            right_wf.fade_out_px = right_width * 0.5;
                        }
                        let ac = self.audio_clips.get(&bid).cloned();
                        self.waveforms.insert(right_id, right_wf.clone());
                        if let Some(clip) = ac.clone() {
                            self.audio_clips.insert(right_id, clip);
                        }
                        to_create.push((right_id, right_wf, ac));

                        for comp in self.components.values_mut() {
                            if comp.waveform_ids.contains(&bid) {
                                let mut new_ids = Vec::new();
                                for &wi in &comp.waveform_ids {
                                    new_ids.push(wi);
                                    if wi == bid {
                                        new_ids.push(right_id);
                                    }
                                }
                                comp.waveform_ids = new_ids;
                            }
                        }
                    }
                    continue;
                }

                // Case 2: B's tail overlaps A's start (B starts before A, ends inside A)
                if b_start < a_start && b_end > a_start {
                    let before = self.waveforms[&bid].clone();
                    let new_width = a_start - b_start;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.size[0] = new_width;
                        if wf.fade_out_px > new_width * 0.5 {
                            wf.fade_out_px = new_width * 0.5;
                        }
                        updates.push((bid, before, wf.clone()));
                    }
                }

                // Case 3: B's head overlaps A's end (B starts inside A, ends after A)
                if b_start >= a_start && b_start < a_end && b_end > a_end {
                    let before = self.waveforms[&bid].clone();
                    let crop_amount = a_end - b_start;
                    let new_width = b_end - a_end;
                    if new_width < WAVEFORM_MIN_WIDTH_PX {
                        to_delete.insert(bid);
                    } else {
                        let wf = self.waveforms.get_mut(&bid).unwrap();
                        wf.position[0] = a_end;
                        wf.size[0] = new_width;
                        wf.sample_offset_px += crop_amount;
                        if wf.fade_in_px > new_width * 0.5 {
                            wf.fade_in_px = new_width * 0.5;
                        }
                        updates.push((bid, before, wf.clone()));
                    }
                }
            }
        }

        let mut ops: Vec<operations::Operation> = Vec::new();
        for (id, before, after) in updates {
            if !to_delete.contains(&id) {
                ops.push(operations::Operation::UpdateWaveform { id, before, after });
            }
        }
        for (id, wf_data, ac) in to_create {
            ops.push(operations::Operation::CreateWaveform {
                id,
                data: wf_data,
                audio_clip: ac.map(|c| (id, c)),
            });
        }
        for &id in &to_delete {
            if let Some(data) = self.waveforms.shift_remove(&id) {
                let ac = self.audio_clips.shift_remove(&id);
                ops.push(operations::Operation::DeleteWaveform {
                    id,
                    data,
                    audio_clip: ac.map(|c| (id, c)),
                });
            }
        }
        ops
    }

    /// Resolve mutual overlaps among ALL waveforms on the same track.
    /// Rightmost waveform wins; waveforms to its left get cropped/deleted.
    /// Used after BPM changes where every waveform's position shifts.
    pub(crate) fn resolve_all_waveform_overlaps(&mut self) -> Vec<operations::Operation> {
        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut all_ops = Vec::new();

        for (id, _) in &sorted {
            if !self.waveforms.contains_key(id) { continue; }
            let ops = self.resolve_waveform_overlaps(&[*id]);
            all_ops.extend(ops);
        }
        all_ops
    }

    /// Same as `resolve_all_waveform_overlaps` but live (uses snapshots for restore).
    pub(crate) fn resolve_all_waveform_overlaps_live(
        &mut self,
        snapshots: &mut IndexMap<EntityId, WaveformView>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }

        for (id, original) in snapshots.iter() {
            if let Some(wf) = self.waveforms.get_mut(id) {
                *wf = original.clone();
            } else {
                self.waveforms.insert(*id, original.clone());
            }
        }

        let mut sorted: Vec<(EntityId, f32)> = self.waveforms.iter()
            .map(|(&id, wf)| (id, wf.position[0]))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut new_snapshots: IndexMap<EntityId, WaveformView> = IndexMap::new();

        for (active_id, _) in &sorted {
            if !self.waveforms.contains_key(active_id) { continue; }
            let a_pos = self.waveforms[active_id].position;
            let a_size = self.waveforms[active_id].size;
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| *id != active_id)
                .copied()
                .collect();

            for bid in other_ids {
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
                    new_snapshots.insert(bid, bwf.clone());
                } else if snapshots.contains_key(&bid) && !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                if b_start >= a_start && b_end <= a_end {
                    self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    continue;
                }
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
                        let orig = new_snapshots.get(&bid)
                            .or_else(|| snapshots.get(&bid))
                            .unwrap();
                        let mut right_wf = orig.clone();
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
                    continue;
                }
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

    /// Live overlap resolution during drag. Restores previously-affected waveforms
    /// from `snapshots`, then re-resolves. Mutates `snapshots` to track affected waveforms.
    /// Deleted waveforms are hidden (set disabled=true) rather than removed, so they
    /// can be restored if the user drags away.
    /// `temp_splits` tracks temporary waveforms created by split (Case 4) so they can
    /// be cleaned up on subsequent calls or on commit.
    pub(crate) fn resolve_waveform_overlaps_live(
        &mut self,
        active_ids: &[EntityId],
        snapshots: &mut IndexMap<EntityId, WaveformView>,
        temp_splits: &mut Vec<EntityId>,
    ) {
        // 0. Remove temporary split waveforms from previous frame
        for id in temp_splits.drain(..) {
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }

        // 1. Restore all previously-affected waveforms to their original state
        for (id, original) in snapshots.iter() {
            if let Some(wf) = self.waveforms.get_mut(id) {
                *wf = original.clone();
            } else {
                self.waveforms.insert(*id, original.clone());
            }
        }

        let active_set: HashSet<EntityId> = active_ids.iter().copied().collect();
        let mut new_snapshots: IndexMap<EntityId, WaveformView> = IndexMap::new();

        for &aid in active_ids {
            let (a_pos, a_size) = match self.waveforms.get(&aid) {
                Some(wf) => (wf.position, wf.size),
                None => continue,
            };
            let a_start = a_pos[0];
            let a_end = a_start + a_size[0];
            let a_y0 = a_pos[1];
            let a_y1 = a_y0 + a_size[1];

            let other_ids: Vec<EntityId> = self.waveforms.keys()
                .filter(|id| !active_set.contains(id))
                .copied()
                .collect();

            for bid in other_ids {
                if new_snapshots.contains_key(&bid) {
                    let bwf = match self.waveforms.get(&bid) {
                        Some(wf) => wf,
                        None => continue,
                    };
                    if bwf.disabled { continue; }
                    let b_y0 = bwf.position[1];
                    let b_y1 = b_y0 + bwf.size[1];
                    if !(a_y0 < b_y1 && a_y1 > b_y0) { continue; }
                    let b_start = bwf.position[0];
                    let b_end = b_start + bwf.size[0];

                    if b_start >= a_start && b_end <= a_end {
                        self.waveforms.get_mut(&bid).unwrap().disabled = true;
                        continue;
                    }
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
                            let orig = new_snapshots.get(&bid).unwrap_or(&self.waveforms[&bid]);
                            let mut right_wf = orig.clone();
                            right_wf.position[0] = a_end;
                            right_wf.size[0] = right_width;
                            right_wf.sample_offset_px += a_end - orig.position[0];
                            right_wf.fade_in_px = 0.0;
                            if right_wf.fade_out_px > right_width * 0.5 { right_wf.fade_out_px = right_width * 0.5; }
                            let right_id = new_id();
                            if let Some(clip) = self.audio_clips.get(&bid).cloned() {
                                self.audio_clips.insert(right_id, clip);
                            }
                            self.waveforms.insert(right_id, right_wf);
                            temp_splits.push(right_id);
                        }
                        continue;
                    }
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

                if !snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, bwf.clone());
                } else if !new_snapshots.contains_key(&bid) {
                    new_snapshots.insert(bid, snapshots[&bid].clone());
                }

                if b_start >= a_start && b_end <= a_end {
                    self.waveforms.get_mut(&bid).unwrap().disabled = true;
                    continue;
                }
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
                        let orig = new_snapshots.get(&bid)
                            .or_else(|| snapshots.get(&bid))
                            .unwrap_or(&self.waveforms[&bid]);
                        let mut right_wf = orig.clone();
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
                    continue;
                }
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
}
