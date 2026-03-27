use super::*;

impl App {
    fn build_create_ops(&self, targets: &[HitTarget]) -> Vec<operations::Operation> {
        let mut ops = Vec::new();
        for t in targets {
            match t {
                HitTarget::Object(id) => { if let Some(d) = self.objects.get(id) { ops.push(operations::Operation::CreateObject { id: *id, data: d.clone() }); } }
                HitTarget::Waveform(id) => { if let Some(d) = self.waveforms.get(id) { let ac = self.audio_clips.get(id).cloned(); ops.push(operations::Operation::CreateWaveform { id: *id, data: d.clone(), audio_clip: ac.map(|c| (*id, c)) }); } }
                HitTarget::LoopRegion(id) => { if let Some(d) = self.loop_regions.get(id) { ops.push(operations::Operation::CreateLoopRegion { id: *id, data: d.clone() }); } }
                HitTarget::ExportRegion(id) => { if let Some(d) = self.export_regions.get(id) { ops.push(operations::Operation::CreateExportRegion { id: *id, data: d.clone() }); } }
                HitTarget::ComponentDef(id) => { if let Some(d) = self.components.get(id) { ops.push(operations::Operation::CreateComponent { id: *id, data: d.clone() }); } }
                HitTarget::ComponentInstance(id) => { if let Some(d) = self.component_instances.get(id) { ops.push(operations::Operation::CreateComponentInstance { id: *id, data: d.clone() }); } }
                HitTarget::MidiClip(id) => { if let Some(d) = self.midi_clips.get(id) { ops.push(operations::Operation::CreateMidiClip { id: *id, data: d.clone() }); } }
                HitTarget::TextNote(id) => { if let Some(d) = self.text_notes.get(id) { ops.push(operations::Operation::CreateTextNote { id: *id, data: d.clone() }); } }
                HitTarget::Group(id) => {
                    if let Some(d) = self.groups.get(id) {
                        // Emit create ops for all group members so undo removes them too
                        for mid in &d.member_ids {
                            if let Some(w) = self.waveforms.get(mid) {
                                let ac = self.audio_clips.get(mid).cloned();
                                ops.push(operations::Operation::CreateWaveform { id: *mid, data: w.clone(), audio_clip: ac.map(|c| (*mid, c)) });
                            } else if let Some(mc) = self.midi_clips.get(mid) {
                                ops.push(operations::Operation::CreateMidiClip { id: *mid, data: mc.clone() });
                            } else if let Some(obj) = self.objects.get(mid) {
                                ops.push(operations::Operation::CreateObject { id: *mid, data: obj.clone() });
                            } else if let Some(tn) = self.text_notes.get(mid) {
                                ops.push(operations::Operation::CreateTextNote { id: *mid, data: tn.clone() });
                            } else if let Some(lr) = self.loop_regions.get(mid) {
                                ops.push(operations::Operation::CreateLoopRegion { id: *mid, data: lr.clone() });
                            } else if let Some(xr) = self.export_regions.get(mid) {
                                ops.push(operations::Operation::CreateExportRegion { id: *mid, data: xr.clone() });
                            } else if let Some(ci) = self.component_instances.get(mid) {
                                ops.push(operations::Operation::CreateComponentInstance { id: *mid, data: ci.clone() });
                            } else if let Some(inst) = self.instruments.get(mid) {
                                let snap = crate::instruments::InstrumentSnapshot {
                                    name: inst.name.clone(),
                                    plugin_id: inst.plugin_id.clone(),
                                    plugin_name: inst.plugin_name.clone(),
                                    plugin_path: inst.plugin_path.clone(),
                                    volume: inst.volume,
                                    pan: inst.pan,
                                    effect_chain_id: inst.effect_chain_id,
                                    disabled: inst.disabled,
                                };
                                ops.push(operations::Operation::CreateInstrument { id: *mid, data: snap });
                            }
                        }
                        ops.push(operations::Operation::CreateGroup { id: *id, data: d.clone() });
                    }
                }
                HitTarget::Instrument(_) => {}
            }
        }
        ops
    }

    /// Shift an entity's position by (dx, dy). Mirrors the logic in set_target_pos for Group.
    fn shift_entity_pos(&mut self, id: EntityId, dx: f32, dy: f32) {
        if let Some(w) = self.waveforms.get_mut(&id) { w.position[0] += dx; w.position[1] += dy; }
        else if let Some(mc) = self.midi_clips.get_mut(&id) { mc.position[0] += dx; mc.position[1] += dy; }
        else if let Some(tn) = self.text_notes.get_mut(&id) { tn.position[0] += dx; tn.position[1] += dy; }
        else if let Some(obj) = self.objects.get_mut(&id) { obj.position[0] += dx; obj.position[1] += dy; }
        else if let Some(lr) = self.loop_regions.get_mut(&id) { lr.position[0] += dx; }
        else if let Some(xr) = self.export_regions.get_mut(&id) { xr.position[0] += dx; xr.position[1] += dy; }
        else if let Some(ci) = self.component_instances.get_mut(&id) { ci.position[0] += dx; ci.position[1] += dy; }
    }

    /// Clone any entity by ID, inserting the clone under a new ID. Returns the new ID.
    pub(crate) fn clone_entity(&mut self, id: EntityId) -> Option<EntityId> {
        if let Some(obj) = self.objects.get(&id).cloned() {
            let nid = new_id();
            self.objects.insert(nid, obj);
            return Some(nid);
        }
        if let Some(wf) = self.waveforms.get(&id).cloned() {
            let nid = new_id();
            self.waveforms.insert(nid, wf);
            if let Some(clip) = self.audio_clips.get(&id).cloned() {
                self.audio_clips.insert(nid, clip);
            }
            return Some(nid);
        }
        if let Some(lr) = self.loop_regions.get(&id).cloned() {
            let nid = new_id();
            self.loop_regions.insert(nid, lr);
            return Some(nid);
        }
        if let Some(xr) = self.export_regions.get(&id).cloned() {
            let nid = new_id();
            self.export_regions.insert(nid, xr);
            return Some(nid);
        }
        if let Some(mc) = self.midi_clips.get(&id).cloned() {
            let nid = new_id();
            self.midi_clips.insert(nid, mc);
            return Some(nid);
        }
        if let Some(tn) = self.text_notes.get(&id).cloned() {
            let nid = new_id();
            self.text_notes.insert(nid, tn);
            return Some(nid);
        }
        if let Some(ci) = self.component_instances.get(&id).cloned() {
            let nid = new_id();
            self.component_instances.insert(nid, ci);
            return Some(nid);
        }
        if let Some(inst) = self.instruments.get(&id).cloned() {
            let nid = new_id();
            self.instruments.insert(nid, inst);
            return Some(nid);
        }
        None
    }

    /// Snapshot a group member entity for clipboard storage.
    fn snapshot_entity(&self, id: EntityId) -> Option<crate::types::GroupMemberSnapshot> {
        use crate::types::GroupMemberSnapshot;
        if let Some(obj) = self.objects.get(&id) {
            return Some(GroupMemberSnapshot::Object(obj.clone()));
        }
        if let Some(wf) = self.waveforms.get(&id) {
            let clip = self.audio_clips.get(&id).cloned();
            return Some(GroupMemberSnapshot::Waveform(wf.clone(), clip));
        }
        if let Some(lr) = self.loop_regions.get(&id) {
            return Some(GroupMemberSnapshot::LoopRegion(lr.clone()));
        }
        if let Some(xr) = self.export_regions.get(&id) {
            return Some(GroupMemberSnapshot::ExportRegion(xr.clone()));
        }
        if let Some(mc) = self.midi_clips.get(&id) {
            return Some(GroupMemberSnapshot::MidiClip(mc.clone()));
        }
        if let Some(tn) = self.text_notes.get(&id) {
            return Some(GroupMemberSnapshot::TextNote(tn.clone()));
        }
        if let Some(ci) = self.component_instances.get(&id) {
            return Some(GroupMemberSnapshot::ComponentInstance(ci.clone()));
        }
        if let Some(inst) = self.instruments.get(&id) {
            let snap = crate::instruments::InstrumentSnapshot {
                name: inst.name.clone(),
                plugin_id: inst.plugin_id.clone(),
                plugin_name: inst.plugin_name.clone(),
                plugin_path: inst.plugin_path.clone(),
                volume: inst.volume,
                pan: inst.pan,
                effect_chain_id: inst.effect_chain_id,
                disabled: inst.disabled,
            };
            return Some(GroupMemberSnapshot::Instrument(snap));
        }
        None
    }

    pub(crate) fn split_sample_at_cursor(&mut self) {
        let screen_pos = self
            .context_menu
            .as_ref()
            .map(|m| m.position)
            .unwrap_or(self.mouse_pos);
        let world = self.camera.screen_to_world(screen_pos);
        let (sw, sh, _) = self.screen_info();
        let hit = hit_test(
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
        let wf_id = match hit {
            Some(HitTarget::Waveform(i)) => i,
            _ => return,
        };
        let wf = match self.waveforms.get(&wf_id) {
            Some(w) => w,
            None => return,
        };
        let clip = match self.audio_clips.get(&wf_id) {
            Some(c) => c,
            None => return,
        };

        let pos = wf.position;
        let size = wf.size;
        let offset_px = wf.sample_offset_px;
        let split_x = snap_to_grid(world[0], &self.settings, self.camera.zoom, self.bpm);
        let t = ((split_x - pos[0]) / size[0]).clamp(0.01, 0.99);

        if clip.samples.is_empty() {
            return;
        }

        let orig_color = wf.color;
        let orig_border_radius = wf.border_radius;
        let orig_fade_in = wf.fade_in_px;
        let orig_fade_out = wf.fade_out_px;
        let orig_fade_in_curve = wf.fade_in_curve;
        let orig_fade_out_curve = wf.fade_out_curve;
        let orig_volume = wf.volume;
        let orig_pan = wf.pan;

        let before_wf = self.waveforms[&wf_id].clone();

        // Non-destructive split: share the same full audio data via Arc
        let left_width = t * size[0];
        let right_width = size[0] - left_width;
        let right_offset_px = offset_px + left_width;

        let left_clip = AudioClipData {
            samples: Arc::clone(&clip.samples),
            sample_rate: clip.sample_rate,
            duration_secs: clip.duration_secs,
        };
        let left_waveform = WaveformView {
            audio: Arc::clone(&wf.audio),
            filename: wf.filename.clone(),
            position: pos,
            size: [left_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: orig_fade_in,
            fade_out_px: 0.0,
            fade_in_curve: orig_fade_in_curve,
            fade_out_curve: 0.0,
            volume: orig_volume,
            pan: orig_pan,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: self.bpm,
            pitch_semitones: 0.0,
            is_reversed: false,
            disabled: false,
            sample_offset_px: offset_px,
            automation: AutomationData::new(),
            effect_chain_id: None,
            take_group: None,
        };

        let right_clip = AudioClipData {
            samples: Arc::clone(&clip.samples),
            sample_rate: clip.sample_rate,
            duration_secs: clip.duration_secs,
        };
        let right_waveform = WaveformView {
            audio: Arc::clone(&wf.audio),
            filename: wf.filename.clone(),
            position: [pos[0] + left_width, pos[1]],
            size: [right_width, size[1]],
            color: orig_color,
            border_radius: orig_border_radius,
            fade_in_px: 0.0,
            fade_out_px: orig_fade_out,
            fade_in_curve: 0.0,
            fade_out_curve: orig_fade_out_curve,
            volume: orig_volume,
            pan: orig_pan,
            warp_mode: ui::waveform::WarpMode::Off,
            sample_bpm: self.bpm,
            pitch_semitones: 0.0,
            is_reversed: false,
            disabled: false,
            sample_offset_px: right_offset_px,
            automation: AutomationData::new(),
            effect_chain_id: None,
            take_group: None,
        };

        // Replace original with left half
        *self.waveforms.get_mut(&wf_id).unwrap() = left_waveform;
        *self.audio_clips.get_mut(&wf_id).unwrap() = left_clip;

        // Insert right half as new entity
        let right_id = new_id();
        self.waveforms.insert(right_id, right_waveform);
        self.audio_clips.insert(right_id, right_clip);

        // Fix up waveform_ids in component defs
        for comp in self.components.values_mut() {
            let mut new_ids = Vec::new();
            for &wi in &comp.waveform_ids {
                new_ids.push(wi);
                if wi == wf_id {
                    new_ids.push(right_id);
                }
            }
            comp.waveform_ids = new_ids;
        }

        // Add right half to selection
        self.selected.push(HitTarget::Waveform(right_id));

        let after_wf = self.waveforms[&wf_id].clone();
        let right_wf_data = self.waveforms[&right_id].clone();
        let right_ac_data = self.audio_clips.get(&right_id).cloned();
        let mut split_ops = vec![
            operations::Operation::UpdateWaveform { id: wf_id, before: before_wf, after: after_wf },
            operations::Operation::CreateWaveform { id: right_id, data: right_wf_data, audio_clip: right_ac_data.map(|c| (right_id, c)) },
        ];
        let overlap_ops = self.resolve_waveform_overlaps(&[wf_id, right_id]);
        split_ops.extend(overlap_ops);
        self.push_op(operations::Operation::Batch(split_ops));
        self.sync_audio_clips();
    }

    pub(crate) fn create_component_from_selection(&mut self) {
        let wf_ids: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                HitTarget::Waveform(i) => Some(*i),
                _ => None,
            })
            .collect();
        if wf_ids.is_empty() {
            println!("No waveforms selected to create component");
            return;
        }
        let (pos, size) = component::bounding_box_of_waveforms(&self.waveforms, &wf_ids);
        let comp_id = new_id();
        self.next_component_id = new_id();
        let name = format!("Component {}", &comp_id.to_string()[..8]);
        let wf_count = wf_ids.len();
        let def = component::ComponentDef {
            id: comp_id,
            name: name.clone(),
            position: pos,
            size,
            waveform_ids: wf_ids,
        };
        self.components.insert(comp_id, def.clone());
        self.push_op(operations::Operation::CreateComponent { id: comp_id, data: def });
        self.selected.clear();
        self.selected.push(HitTarget::ComponentDef(comp_id));
        println!(
            "Created component '{}' with {} waveforms",
            name,
            wf_count
        );
    }

    pub(crate) fn create_instance_of_selected_component(&mut self) {
        let comp_id = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentDef(i) => Some(*i),
            _ => None,
        });
        if let Some(ci) = comp_id {
            let (comp_id_val, def_name, inst_pos) = match self.components.get(&ci) {
                Some(d) => (d.id, d.name.clone(), [d.position[0] + d.size[0] + 50.0, d.position[1]]),
                None => return,
            };
            let inst = component::ComponentInstance {
                component_id: comp_id_val,
                position: inst_pos,
            };
            let inst_id = new_id();
            self.component_instances.insert(inst_id, inst.clone());
            self.push_op(operations::Operation::CreateComponentInstance { id: inst_id, data: inst });
            self.selected.clear();
            self.selected.push(HitTarget::ComponentInstance(inst_id));
            println!("Created instance of component {}", def_name);
            self.sync_audio_clips();
        }
    }

    pub(crate) fn go_to_component_of_selected_instance(&mut self) {
        let inst_id = self.selected.iter().find_map(|t| match t {
            HitTarget::ComponentInstance(i) => Some(*i),
            _ => None,
        });
        if let Some(ii) = inst_id {
            let comp_id = match self.component_instances.get(&ii) {
                Some(inst) => inst.component_id,
                None => return,
            };
            if let Some((&ci, def)) = self
                .components
                .iter()
                .find(|(_, c)| c.id == comp_id)
            {
                let (sw, sh, _) = self.screen_info();
                self.camera.position = [
                    def.position[0] + def.size[0] * 0.5 - sw * 0.5 / self.camera.zoom,
                    def.position[1] + def.size[1] * 0.5 - sh * 0.5 / self.camera.zoom,
                ];
                self.selected.clear();
                self.selected.push(HitTarget::ComponentDef(ci));
                println!("Navigated to component '{}'", def.name);
            }
        }
    }

    pub(crate) fn duplicate_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut new_selected: Vec<HitTarget> = Vec::new();
        let mut dup_ops: Vec<operations::Operation> = Vec::new();

        let selected_wf_ids: Vec<EntityId> = self
            .selected
            .iter()
            .filter_map(|t| {
                if let HitTarget::Waveform(i) = t {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();

        let wf_group_shift = if selected_wf_ids.len() >= 2 {
            let min_start = selected_wf_ids
                .iter()
                .filter_map(|i| self.waveforms.get(i))
                .map(|wf| wf.position[0])
                .fold(f32::INFINITY, f32::min);
            let max_end = selected_wf_ids
                .iter()
                .filter_map(|i| self.waveforms.get(i))
                .map(|wf| wf.position[0] + wf.size[0])
                .fold(f32::NEG_INFINITY, f32::max);
            Some(max_end - min_start)
        } else {
            None
        };

        for target in self.selected.clone() {
            match target {
                HitTarget::ComponentInstance(i) => {
                    if let Some(src) = self.component_instances.get(&i).cloned() {
                        let def = self.components.values().find(|c| c.id == src.component_id);
                        let shift = def.map(|d| d.size[0]).unwrap_or(100.0);
                        let inst = component::ComponentInstance {
                            component_id: src.component_id,
                            position: [src.position[0] + shift, src.position[1]],
                        };
                        let nid = new_id();
                        self.component_instances.insert(nid, inst);
                        new_selected.push(HitTarget::ComponentInstance(nid));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if let Some(src) = self.components.get(&i).cloned() {
                        let shift = src.size[0];
                        let comp_nid = new_id();
                        self.next_component_id = new_id();
                        let src_wf_ids = src.waveform_ids.clone();
                        let mut new_wf_ids = Vec::new();
                        for &wi in &src_wf_ids {
                            if let Some(wf) = self.waveforms.get(&wi).cloned() {
                                let mut wf = wf;
                                wf.position[0] += shift;
                                let wf_nid = new_id();
                                self.waveforms.insert(wf_nid, wf);
                                new_wf_ids.push(wf_nid);
                                if let Some(clip) = self.audio_clips.get(&wi).cloned() {
                                    self.audio_clips.insert(wf_nid, clip);
                                }
                            }
                        }
                        self.components.insert(comp_nid, component::ComponentDef {
                            id: comp_nid,
                            name: format!("{} copy", src.name),
                            position: [src.position[0] + shift, src.position[1]],
                            size: src.size,
                            waveform_ids: new_wf_ids,
                        });
                        new_selected.push(HitTarget::ComponentDef(comp_nid));
                    }
                }
                HitTarget::Waveform(i) => {
                    if let Some(wf) = self.waveforms.get(&i).cloned() {
                        let mut wf = wf;
                        let shift = wf_group_shift.unwrap_or(wf.size[0]);
                        wf.position[0] += shift;
                        let nid = new_id();
                        self.waveforms.insert(nid, wf);
                        if let Some(clip) = self.audio_clips.get(&i).cloned() {
                            self.audio_clips.insert(nid, clip);
                        }
                        new_selected.push(HitTarget::Waveform(nid));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if let Some(lr) = self.loop_regions.get(&i).cloned() {
                        let mut lr = lr;
                        lr.position[0] += lr.size[0];
                        let nid = new_id();
                        self.loop_regions.insert(nid, lr);
                        new_selected.push(HitTarget::LoopRegion(nid));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if let Some(xr) = self.export_regions.get(&i).cloned() {
                        let mut xr = xr;
                        xr.position[0] += xr.size[0];
                        let nid = new_id();
                        self.export_regions.insert(nid, xr);
                        new_selected.push(HitTarget::ExportRegion(nid));
                    }
                }
                HitTarget::Object(i) => {
                    if let Some(obj) = self.objects.get(&i).cloned() {
                        let mut obj = obj;
                        obj.position[0] += obj.size[0];
                        let nid = new_id();
                        self.objects.insert(nid, obj);
                        new_selected.push(HitTarget::Object(nid));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if let Some(mc) = self.midi_clips.get(&i).cloned() {
                        let mut mc = mc;
                        mc.position[0] += mc.size[0];
                        let nid = new_id();
                        self.midi_clips.insert(nid, mc);
                        new_selected.push(HitTarget::MidiClip(nid));
                    }
                }
                HitTarget::TextNote(i) => {
                    if let Some(tn) = self.text_notes.get(&i).cloned() {
                        let mut tn = tn;
                        tn.position[0] += tn.size[0];
                        let nid = new_id();
                        self.text_notes.insert(nid, tn);
                        new_selected.push(HitTarget::TextNote(nid));
                    }
                }
                HitTarget::Group(i) => {
                    if let Some(g) = self.groups.get(&i).cloned() {
                        let mut g = g;
                        let shift_x = g.size[0];
                        g.position[0] += shift_x;
                        let old_member_ids = g.member_ids.clone();
                        let mut new_member_ids = Vec::new();
                        for mid in old_member_ids {
                            let new_mid = self.clone_entity(mid).unwrap_or(mid);
                            self.shift_entity_pos(new_mid, shift_x, 0.0);
                            new_member_ids.push(new_mid);
                        }
                        g.member_ids = new_member_ids;
                        let nid = new_id();
                        self.groups.insert(nid, g);
                        new_selected.push(HitTarget::Group(nid));
                    }
                }
                HitTarget::Instrument(_) => {}
            }
        }

        // Build ops from all duplicated entities
        dup_ops.extend(self.build_create_ops(&new_selected));
        let dup_wf_ids: Vec<EntityId> = new_selected.iter()
            .filter_map(|t| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
            .collect();
        if !dup_wf_ids.is_empty() {
            let overlap_ops = self.resolve_waveform_overlaps(&dup_wf_ids);
            dup_ops.extend(overlap_ops);
        }
        if !dup_ops.is_empty() {
            self.push_op(operations::Operation::Batch(dup_ops));
        }
        self.selected = new_selected;
        self.sync_audio_clips();
    }

    pub(crate) fn copy_selected(&mut self) {
        self.clipboard.items.clear();
        // If editing a MIDI clip with selected notes, copy those instead
        if let Some(mc_id) = self.editing_midi_clip {
            if let Some(mc) = self.midi_clips.get(&mc_id) {
                if !self.selected_midi_notes.is_empty() {
                    let notes = &mc.notes;
                    let min_start = self.selected_midi_notes.iter()
                        .filter(|&&ni| ni < notes.len())
                        .map(|&ni| notes[ni].start_px)
                        .fold(f32::INFINITY, f32::min);
                    let mut copied: Vec<midi::MidiNote> = Vec::new();
                    for &ni in &self.selected_midi_notes {
                        if ni < notes.len() {
                            let mut n = notes[ni].clone();
                            n.start_px -= min_start;
                            copied.push(n);
                        }
                    }
                    self.clipboard.items.push(ClipboardItem::MidiNotes(copied));
                    return;
                }
            }
        }
        for target in &self.selected {
            match target {
                HitTarget::Object(i) => {
                    if let Some(obj) = self.objects.get(i) {
                        self.clipboard.items.push(ClipboardItem::Object(obj.clone()));
                    }
                }
                HitTarget::Waveform(i) => {
                    if let Some(wf) = self.waveforms.get(i) {
                        let clip = self.audio_clips.get(i).cloned();
                        self.clipboard.items.push(ClipboardItem::Waveform(wf.clone(), clip));
                    }
                }
                HitTarget::LoopRegion(i) => {
                    if let Some(lr) = self.loop_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::LoopRegion(lr.clone()));
                    }
                }
                HitTarget::ExportRegion(i) => {
                    if let Some(xr) = self.export_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::ExportRegion(xr.clone()));
                    }
                }
                HitTarget::ComponentDef(i) => {
                    if let Some(def) = self.components.get(i) {
                        let wfs: Vec<(WaveformView, Option<AudioClipData>)> = def
                            .waveform_ids
                            .iter()
                            .filter_map(|wi| {
                                if let Some(wf) = self.waveforms.get(wi) {
                                    let clip = self.audio_clips.get(wi).cloned();
                                    Some((wf.clone(), clip))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        self.clipboard.items.push(ClipboardItem::ComponentDef(def.clone(), wfs));
                    }
                }
                HitTarget::ComponentInstance(i) => {
                    if let Some(inst) = self.component_instances.get(i) {
                        self.clipboard.items.push(ClipboardItem::ComponentInstance(inst.clone()));
                    }
                }
                HitTarget::MidiClip(i) => {
                    if let Some(mc) = self.midi_clips.get(i) {
                        self.clipboard.items.push(ClipboardItem::MidiClip(mc.clone()));
                    }
                }
                HitTarget::TextNote(i) => {
                    if let Some(tn) = self.text_notes.get(i) {
                        self.clipboard.items.push(ClipboardItem::TextNote(tn.clone()));
                    }
                }
                HitTarget::Group(i) => {
                    if let Some(g) = self.groups.get(i) {
                        let members: Vec<(EntityId, crate::types::GroupMemberSnapshot)> = g
                            .member_ids
                            .iter()
                            .filter_map(|&mid| self.snapshot_entity(mid).map(|s| (mid, s)))
                            .collect();
                        self.clipboard.items.push(ClipboardItem::Group(g.clone(), members));
                    }
                }
                HitTarget::Instrument(_) => {}
            }
        }
    }

    pub(crate) fn paste_clipboard(&mut self) {
        if self.clipboard.items.is_empty() {
            return;
        }
        // If editing a MIDI clip and clipboard has MIDI notes, paste them
        if let Some(mc_id) = self.editing_midi_clip {
            let midi_notes = self.clipboard.items.iter().find_map(|item| {
                if let ClipboardItem::MidiNotes(notes) = item { Some(notes.clone()) } else { None }
            });
            if let Some(notes) = midi_notes {
                let clip_x = self.midi_clips.get(&mc_id).map(|mc| mc.position[0]);
                if let Some(clip_x) = clip_x {
                    let before_notes = self.midi_clips[&mc_id].notes.clone();
                    let paste_x = {
                        #[cfg(feature = "native")]
                        { self.audio_engine.as_ref()
                            .map(|e| (e.position_seconds() * PIXELS_PER_SECOND as f64) as f32)
                            .unwrap_or_else(|| self.camera.screen_to_world(self.mouse_pos)[0]) }
                        #[cfg(not(feature = "native"))]
                        { self.camera.screen_to_world(self.mouse_pos)[0] }
                    };
                    let offset = (paste_x - clip_x).max(0.0);
                    let new_indices = if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                        let mut indices: Vec<usize> = Vec::new();
                        for n in &notes {
                            let mut pasted = n.clone();
                            pasted.start_px += offset;
                            mc.notes.push(pasted);
                            indices.push(mc.notes.len() - 1);
                        }
                        Some(indices)
                    } else {
                        None
                    };
                    if let Some(indices) = new_indices {
                        if let Some(mc) = self.midi_clips.get_mut(&mc_id) {
                            self.selected_midi_notes = mc.resolve_note_overlaps(&indices);
                        }
                    }
                    let after_notes = self.midi_clips[&mc_id].notes.clone();
                    self.push_op(operations::Operation::UpdateMidiNotes { clip_id: mc_id, before: before_notes, after: after_notes });
                    self.sync_audio_clips();
                    return;
                }
            }
        }
        let world = self.camera.screen_to_world(self.mouse_pos);

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for item in &self.clipboard.items {
            let pos = match item {
                ClipboardItem::Object(o) => o.position,
                ClipboardItem::Waveform(w, _) => w.position,
                ClipboardItem::LoopRegion(l) => l.position,
                ClipboardItem::ExportRegion(x) => x.position,
                ClipboardItem::ComponentDef(d, _) => d.position,
                ClipboardItem::ComponentInstance(ci) => ci.position,
                ClipboardItem::MidiClip(mc) => mc.position,
                ClipboardItem::MidiNotes(_) => continue,
                ClipboardItem::TextNote(tn) => tn.position,
                ClipboardItem::Group(g, _) => g.position,
            };
            if pos[0] < min_x {
                min_x = pos[0];
            }
            if pos[1] < min_y {
                min_y = pos[1];
            }
        }

        let dx = world[0] - min_x;
        let dy = world[1] - min_y;
        let mut new_selected: Vec<HitTarget> = Vec::new();

        for item in self.clipboard.items.clone() {
            match item {
                ClipboardItem::Object(mut o) => {
                    o.position[0] += dx;
                    o.position[1] += dy;
                    let nid = new_id();
                    self.objects.insert(nid, o);
                    new_selected.push(HitTarget::Object(nid));
                }
                ClipboardItem::Waveform(mut w, clip) => {
                    w.position[0] += dx;
                    w.position[1] += dy;
                    let nid = new_id();
                    self.waveforms.insert(nid, w);
                    if let Some(c) = clip {
                        self.audio_clips.insert(nid, c);
                    }
                    new_selected.push(HitTarget::Waveform(nid));
                }
                ClipboardItem::LoopRegion(mut l) => {
                    l.position[0] += dx;
                    l.position[1] += dy;
                    let nid = new_id();
                    self.loop_regions.insert(nid, l);
                    new_selected.push(HitTarget::LoopRegion(nid));
                }
                ClipboardItem::ExportRegion(mut x) => {
                    x.position[0] += dx;
                    x.position[1] += dy;
                    let nid = new_id();
                    self.export_regions.insert(nid, x);
                    new_selected.push(HitTarget::ExportRegion(nid));
                }
                ClipboardItem::ComponentDef(mut d, wfs) => {
                    let comp_nid = new_id();
                    self.next_component_id = new_id();
                    d.id = comp_nid;
                    d.position[0] += dx;
                    d.position[1] += dy;
                    d.name = format!("{} copy", d.name);
                    let mut new_wf_ids = Vec::new();
                    for (mut wf, clip) in wfs {
                        wf.position[0] += dx;
                        wf.position[1] += dy;
                        let wf_nid = new_id();
                        self.waveforms.insert(wf_nid, wf);
                        new_wf_ids.push(wf_nid);
                        if let Some(c) = clip {
                            self.audio_clips.insert(wf_nid, c);
                        }
                    }
                    d.waveform_ids = new_wf_ids;
                    self.components.insert(comp_nid, d);
                    new_selected.push(HitTarget::ComponentDef(comp_nid));
                }
                ClipboardItem::ComponentInstance(mut ci) => {
                    ci.position[0] += dx;
                    ci.position[1] += dy;
                    let nid = new_id();
                    self.component_instances.insert(nid, ci);
                    new_selected.push(HitTarget::ComponentInstance(nid));
                }
                ClipboardItem::MidiClip(mut mc) => {
                    mc.position[0] += dx;
                    mc.position[1] += dy;
                    let nid = new_id();
                    self.midi_clips.insert(nid, mc);
                    new_selected.push(HitTarget::MidiClip(nid));
                }
                ClipboardItem::MidiNotes(_) => {
                    // Handled in MIDI editing mode (events.rs), skip in global paste
                }
                ClipboardItem::TextNote(mut tn) => {
                    tn.position[0] += dx;
                    tn.position[1] += dy;
                    let nid = new_id();
                    self.text_notes.insert(nid, tn);
                    new_selected.push(HitTarget::TextNote(nid));
                }
                ClipboardItem::Group(mut g, members) => {
                    g.position[0] += dx;
                    g.position[1] += dy;
                    let mut id_map: std::collections::HashMap<EntityId, EntityId> =
                        std::collections::HashMap::new();
                    for (old_id, snapshot) in members {
                        use crate::types::GroupMemberSnapshot;
                        let nid = new_id();
                        id_map.insert(old_id, nid);
                        match snapshot {
                            GroupMemberSnapshot::Object(mut o) => {
                                o.position[0] += dx;
                                o.position[1] += dy;
                                self.objects.insert(nid, o);
                            }
                            GroupMemberSnapshot::Waveform(mut w, c) => {
                                w.position[0] += dx;
                                w.position[1] += dy;
                                self.waveforms.insert(nid, w);
                                if let Some(c) = c {
                                    self.audio_clips.insert(nid, c);
                                }
                            }
                            GroupMemberSnapshot::LoopRegion(mut lr) => {
                                lr.position[0] += dx;
                                self.loop_regions.insert(nid, lr);
                            }
                            GroupMemberSnapshot::ExportRegion(mut xr) => {
                                xr.position[0] += dx;
                                xr.position[1] += dy;
                                self.export_regions.insert(nid, xr);
                            }
                            GroupMemberSnapshot::MidiClip(mut mc) => {
                                mc.position[0] += dx;
                                mc.position[1] += dy;
                                self.midi_clips.insert(nid, mc);
                            }
                            GroupMemberSnapshot::TextNote(mut tn) => {
                                tn.position[0] += dx;
                                tn.position[1] += dy;
                                self.text_notes.insert(nid, tn);
                            }
                            GroupMemberSnapshot::ComponentInstance(mut ci) => {
                                ci.position[0] += dx;
                                ci.position[1] += dy;
                                self.component_instances.insert(nid, ci);
                            }
                            GroupMemberSnapshot::Instrument(snap) => {
                                let mut inst = crate::instruments::Instrument::new();
                                inst.name = snap.name;
                                inst.plugin_id = snap.plugin_id;
                                inst.plugin_name = snap.plugin_name;
                                inst.plugin_path = snap.plugin_path;
                                inst.volume = snap.volume;
                                inst.pan = snap.pan;
                                inst.effect_chain_id = snap.effect_chain_id;
                                self.instruments.insert(nid, inst);
                            }
                        }
                    }
                    g.member_ids = g
                        .member_ids
                        .iter()
                        .map(|id| id_map.get(id).copied().unwrap_or(*id))
                        .collect();
                    let nid = new_id();
                    self.groups.insert(nid, g);
                    new_selected.push(HitTarget::Group(nid));
                }
            }
        }

        // Build ops from pasted entities
        let mut paste_ops = self.build_create_ops(&new_selected);
        let pasted_wf_ids: Vec<EntityId> = new_selected.iter()
            .filter_map(|t| if let HitTarget::Waveform(id) = t { Some(*id) } else { None })
            .collect();
        if !pasted_wf_ids.is_empty() {
            let overlap_ops = self.resolve_waveform_overlaps(&pasted_wf_ids);
            paste_ops.extend(overlap_ops);
        }
        if !paste_ops.is_empty() {
            self.push_op(operations::Operation::Batch(paste_ops));
        }
        self.selected = new_selected;
        self.sync_audio_clips();
    }

    pub(crate) fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut del_ops: Vec<operations::Operation> = Vec::new();
        let obj_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::Object(i) => Some(*i), _ => None }).collect();
        let wf_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::Waveform(i) => Some(*i), _ => None }).collect();
        let lr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::LoopRegion(i) => Some(*i), _ => None }).collect();
        let xr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ExportRegion(i) => Some(*i), _ => None }).collect();
        let comp_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentDef(i) => Some(*i), _ => None }).collect();
        let inst_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentInstance(i) => Some(*i), _ => None }).collect();
        let mc_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::MidiClip(i) => Some(*i), _ => None }).collect();
        let tn_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::TextNote(i) => Some(*i), _ => None }).collect();
        let group_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::Group(i) => Some(*i), _ => None }).collect();

        // Capture before removing
        for &id in &inst_ids {
            if let Some(d) = self.component_instances.get(&id) { del_ops.push(operations::Operation::DeleteComponentInstance { id, data: d.clone() }); }
            self.component_instances.shift_remove(&id);
        }
        for &id in &comp_ids {
            if let Some(comp) = self.components.shift_remove(&id) {
                del_ops.push(operations::Operation::DeleteComponent { id, data: comp.clone() });
                self.component_instances.retain(|_, inst| inst.component_id != comp.id);
                for &wi in &comp.waveform_ids {
                    if let Some(wf) = self.waveforms.get(&wi) {
                        let ac = self.audio_clips.get(&wi).cloned();
                        del_ops.push(operations::Operation::DeleteWaveform { id: wi, data: wf.clone(), audio_clip: ac.map(|c| (wi, c)) });
                    }
                    self.waveforms.shift_remove(&wi);
                    self.audio_clips.shift_remove(&wi);
                }
            }
        }
        for &id in &obj_ids { if let Some(d) = self.objects.get(&id) { del_ops.push(operations::Operation::DeleteObject { id, data: d.clone() }); } self.objects.shift_remove(&id); }
        for &id in &wf_ids {
            if let Some(d) = self.waveforms.get(&id) { let ac = self.audio_clips.get(&id).cloned(); del_ops.push(operations::Operation::DeleteWaveform { id, data: d.clone(), audio_clip: ac.map(|c| (id, c)) }); }
            self.waveforms.shift_remove(&id);
            self.audio_clips.shift_remove(&id);
        }
        for &id in &lr_ids { if let Some(d) = self.loop_regions.get(&id) { del_ops.push(operations::Operation::DeleteLoopRegion { id, data: d.clone() }); } self.loop_regions.shift_remove(&id); }
        for &id in &xr_ids { if let Some(d) = self.export_regions.get(&id) { del_ops.push(operations::Operation::DeleteExportRegion { id, data: d.clone() }); } self.export_regions.shift_remove(&id); }
        for &id in &mc_ids { if let Some(d) = self.midi_clips.get(&id) { del_ops.push(operations::Operation::DeleteMidiClip { id, data: d.clone() }); } self.midi_clips.shift_remove(&id); }
        for &id in &tn_ids { if let Some(d) = self.text_notes.get(&id) { del_ops.push(operations::Operation::DeleteTextNote { id, data: d.clone() }); } self.text_notes.shift_remove(&id); }
        // Collect all individually-selected IDs so we don't double-delete group members
        let already_deleted: std::collections::HashSet<EntityId> = [&obj_ids as &[EntityId], &wf_ids, &lr_ids, &xr_ids, &mc_ids, &tn_ids, &inst_ids, &comp_ids]
            .iter().flat_map(|v| v.iter().copied()).collect();
        for &id in &group_ids {
            if let Some(d) = self.groups.get(&id) {
                // Delete all member entities that weren't already individually deleted
                for mid in &d.member_ids {
                    if already_deleted.contains(mid) { continue; }
                    if let Some(w) = self.waveforms.get(mid) {
                        let ac = self.audio_clips.get(mid).cloned();
                        del_ops.push(operations::Operation::DeleteWaveform { id: *mid, data: w.clone(), audio_clip: ac.map(|c| (*mid, c)) });
                        self.waveforms.shift_remove(mid);
                        self.audio_clips.shift_remove(mid);
                    } else if let Some(mc) = self.midi_clips.get(mid) {
                        del_ops.push(operations::Operation::DeleteMidiClip { id: *mid, data: mc.clone() });
                        self.midi_clips.shift_remove(mid);
                    } else if let Some(obj) = self.objects.get(mid) {
                        del_ops.push(operations::Operation::DeleteObject { id: *mid, data: obj.clone() });
                        self.objects.shift_remove(mid);
                    } else if let Some(tn) = self.text_notes.get(mid) {
                        del_ops.push(operations::Operation::DeleteTextNote { id: *mid, data: tn.clone() });
                        self.text_notes.shift_remove(mid);
                    } else if let Some(lr) = self.loop_regions.get(mid) {
                        del_ops.push(operations::Operation::DeleteLoopRegion { id: *mid, data: lr.clone() });
                        self.loop_regions.shift_remove(mid);
                    } else if let Some(xr) = self.export_regions.get(mid) {
                        del_ops.push(operations::Operation::DeleteExportRegion { id: *mid, data: xr.clone() });
                        self.export_regions.shift_remove(mid);
                    } else if let Some(ci) = self.component_instances.get(mid) {
                        del_ops.push(operations::Operation::DeleteComponentInstance { id: *mid, data: ci.clone() });
                        self.component_instances.shift_remove(mid);
                    } else if let Some(inst) = self.instruments.get(mid) {
                        let snap = crate::instruments::InstrumentSnapshot {
                            name: inst.name.clone(),
                            plugin_id: inst.plugin_id.clone(),
                            plugin_name: inst.plugin_name.clone(),
                            plugin_path: inst.plugin_path.clone(),
                            volume: inst.volume,
                            pan: inst.pan,
                            effect_chain_id: inst.effect_chain_id,
                            disabled: inst.disabled,
                        };
                        del_ops.push(operations::Operation::DeleteInstrument { id: *mid, data: snap });
                        self.instruments.shift_remove(mid);
                    }
                }
                del_ops.push(operations::Operation::DeleteGroup { id, data: d.clone() });
            }
            self.groups.shift_remove(&id);
        }
        if !del_ops.is_empty() {
            self.push_op(operations::Operation::Batch(del_ops));
        }

        // Remove deleted entities from group member lists and update bounds
        let all_deleted: Vec<EntityId> = [&obj_ids, &wf_ids, &lr_ids, &xr_ids, &mc_ids, &tn_ids]
            .iter().flat_map(|v| v.iter().copied()).collect();
        let mut groups_to_update: Vec<EntityId> = Vec::new();
        for (gid, group) in self.groups.iter_mut() {
            let before_len = group.member_ids.len();
            group.member_ids.retain(|mid| !all_deleted.contains(mid));
            if group.member_ids.len() != before_len {
                groups_to_update.push(*gid);
            }
        }
        for gid in groups_to_update {
            self.update_group_bounds(gid);
        }

        self.selected.clear();
        #[cfg(feature = "native")]
        {
            self.sync_keyboard_instrument_from_selection();
            self.sync_computer_keyboard_to_engine();
            self.sync_audio_clips();
            self.sync_loop_region();
        }
        println!("Deleted selected items");
    }
}
