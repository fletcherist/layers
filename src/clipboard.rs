use super::*;

impl App {
    fn build_create_ops(&self, targets: &[HitTarget]) -> Vec<operations::Operation> {
        let mut ops = Vec::new();
        for t in targets {
            match t {
                HitTarget::Object(id) => { if let Some(d) = self.objects.get(id) { ops.push(operations::Operation::CreateObject { id: *id, data: d.clone() }); } }
                HitTarget::Waveform(id) => { if let Some(d) = self.waveforms.get(id) { let ac = self.audio_clips.get(id).cloned(); ops.push(operations::Operation::CreateWaveform { id: *id, data: d.clone(), audio_clip: ac.map(|c| (*id, c)) }); } }
                HitTarget::EffectRegion(id) => { if let Some(d) = self.effect_regions.get(id) { ops.push(operations::Operation::CreateEffectRegion { id: *id, data: d.clone() }); } }
                HitTarget::PluginBlock(id) => { if let Some(d) = self.plugin_blocks.get(id) { ops.push(operations::Operation::CreatePluginBlock { id: *id, data: d.snapshot() }); } }
                HitTarget::LoopRegion(id) => { if let Some(d) = self.loop_regions.get(id) { ops.push(operations::Operation::CreateLoopRegion { id: *id, data: d.clone() }); } }
                HitTarget::ExportRegion(id) => { if let Some(d) = self.export_regions.get(id) { ops.push(operations::Operation::CreateExportRegion { id: *id, data: d.clone() }); } }
                HitTarget::ComponentDef(id) => { if let Some(d) = self.components.get(id) { ops.push(operations::Operation::CreateComponent { id: *id, data: d.clone() }); } }
                HitTarget::ComponentInstance(id) => { if let Some(d) = self.component_instances.get(id) { ops.push(operations::Operation::CreateComponentInstance { id: *id, data: d.clone() }); } }
                HitTarget::MidiClip(id) => { if let Some(d) = self.midi_clips.get(id) { ops.push(operations::Operation::CreateMidiClip { id: *id, data: d.clone() }); } }
                HitTarget::TextNote(id) => { if let Some(d) = self.text_notes.get(id) { ops.push(operations::Operation::CreateTextNote { id: *id, data: d.clone() }); } }
            }
        }
        ops
    }

    pub(crate) fn split_sample_at_cursor(&mut self) {
        let screen_pos = self
            .context_menu
            .as_ref()
            .map(|m| m.position)
            .unwrap_or(self.mouse_pos);
        let world = self.camera.screen_to_world(screen_pos);
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
            &self.text_notes,
            self.editing_component,
            world,
            &self.camera,
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

        let audio = Arc::clone(&wf.audio);
        let mono_samples = Arc::clone(&clip.samples);
        let total_mono = mono_samples.len();
        if total_mono == 0 {
            return;
        }

        let full_w = full_audio_width_px(wf);
        let vis_start_frac = if full_w > 0.0 { offset_px / full_w } else { 0.0 };
        let vis_end_frac = if full_w > 0.0 { (offset_px + size[0]) / full_w } else { 1.0 };
        let split_frac = vis_start_frac + t * (vis_end_frac - vis_start_frac);

        let vis_start_mono = (vis_start_frac * total_mono as f32) as usize;
        let vis_end_mono = (vis_end_frac * total_mono as f32).min(total_mono as f32) as usize;
        let split_mono = (split_frac * total_mono as f32) as usize;

        let vis_start_left = (vis_start_frac * audio.left_samples.len() as f32) as usize;
        let vis_end_left = (vis_end_frac * audio.left_samples.len() as f32).min(audio.left_samples.len() as f32) as usize;
        let split_left = (split_frac * audio.left_samples.len() as f32) as usize;

        let vis_start_right = (vis_start_frac * audio.right_samples.len() as f32) as usize;
        let vis_end_right = (vis_end_frac * audio.right_samples.len() as f32).min(audio.right_samples.len() as f32) as usize;
        let split_right = (split_frac * audio.right_samples.len() as f32) as usize;

        let orig_color = wf.color;
        let orig_border_radius = wf.border_radius;
        let orig_fade_in = wf.fade_in_px;
        let orig_fade_out = wf.fade_out_px;
        let orig_fade_in_curve = wf.fade_in_curve;
        let orig_fade_out_curve = wf.fade_out_curve;
        let orig_volume = wf.volume;
        let orig_pan = wf.pan;

        let before_wf = self.waveforms[&wf_id].clone();

        let sample_rate = audio.sample_rate;
        let filename = audio.filename.clone();

        let left_mono: Vec<f32> = mono_samples[vis_start_mono..split_mono].to_vec();
        let right_mono: Vec<f32> = mono_samples[split_mono..vis_end_mono].to_vec();
        let left_l: Vec<f32> = audio.left_samples[vis_start_left..split_left].to_vec();
        let left_r: Vec<f32> = audio.right_samples[vis_start_right..split_right].to_vec();
        let right_l: Vec<f32> = audio.left_samples[split_left..vis_end_left].to_vec();
        let right_r: Vec<f32> = audio.right_samples[split_right..vis_end_right].to_vec();

        let left_duration = left_mono.len() as f32 / sample_rate as f32;
        let right_duration = right_mono.len() as f32 / sample_rate as f32;
        let left_width = left_duration * PIXELS_PER_SECOND;
        let right_width = right_duration * PIXELS_PER_SECOND;

        let left_clip = AudioClipData {
            samples: Arc::new(left_mono.clone()),
            sample_rate,
            duration_secs: left_duration,
        };
        let left_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&left_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&left_r)),
            left_samples: Arc::new(left_l),
            right_samples: Arc::new(left_r),
            sample_rate,
            filename: filename.clone(),
        });
        let left_waveform = WaveformView {
            audio: left_audio,
            filename: filename.clone(),
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
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
            effect_chain_id: None,
        };

        let right_clip = AudioClipData {
            samples: Arc::new(right_mono.clone()),
            sample_rate,
            duration_secs: right_duration,
        };
        let right_audio = Arc::new(AudioData {
            left_peaks: Arc::new(WaveformPeaks::build(&right_l)),
            right_peaks: Arc::new(WaveformPeaks::build(&right_r)),
            left_samples: Arc::new(right_l),
            right_samples: Arc::new(right_r),
            sample_rate,
            filename: filename.clone(),
        });
        let right_waveform = WaveformView {
            audio: right_audio,
            filename,
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
            sample_offset_px: 0.0,
            automation: AutomationData::new(),
            effect_chain_id: None,
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
                HitTarget::EffectRegion(i) => {
                    if let Some(er) = self.effect_regions.get(&i).cloned() {
                        let mut er = er;
                        er.position[0] += er.size[0];
                        let nid = new_id();
                        self.effect_regions.insert(nid, er);
                        new_selected.push(HitTarget::EffectRegion(nid));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if let Some(pb) = self.plugin_blocks.get(&i).cloned() {
                        let mut pb = pb;
                        pb.position[0] += pb.size[0];
                        let nid = new_id();
                        self.plugin_blocks.insert(nid, pb);
                        new_selected.push(HitTarget::PluginBlock(nid));
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
                HitTarget::EffectRegion(i) => {
                    if let Some(er) = self.effect_regions.get(i) {
                        self.clipboard.items.push(ClipboardItem::EffectRegion(er.clone()));
                    }
                }
                HitTarget::PluginBlock(i) => {
                    if let Some(pb) = self.plugin_blocks.get(i) {
                        self.clipboard.items.push(ClipboardItem::PluginBlock(pb.clone()));
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
                ClipboardItem::EffectRegion(e) => e.position,
                ClipboardItem::PluginBlock(pb) => pb.position,
                ClipboardItem::LoopRegion(l) => l.position,
                ClipboardItem::ExportRegion(x) => x.position,
                ClipboardItem::ComponentDef(d, _) => d.position,
                ClipboardItem::ComponentInstance(ci) => ci.position,
                ClipboardItem::MidiClip(mc) => mc.position,
                ClipboardItem::MidiNotes(_) => continue,
                ClipboardItem::TextNote(tn) => tn.position,
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
                ClipboardItem::EffectRegion(mut e) => {
                    e.position[0] += dx;
                    e.position[1] += dy;
                    let nid = new_id();
                    self.effect_regions.insert(nid, e);
                    new_selected.push(HitTarget::EffectRegion(nid));
                }
                ClipboardItem::PluginBlock(mut pb) => {
                    pb.position[0] += dx;
                    pb.position[1] += dy;
                    let nid = new_id();
                    self.plugin_blocks.insert(nid, pb);
                    new_selected.push(HitTarget::PluginBlock(nid));
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
        let er_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::EffectRegion(i) => Some(*i), _ => None }).collect();
        let pb_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::PluginBlock(i) => Some(*i), _ => None }).collect();
        let lr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::LoopRegion(i) => Some(*i), _ => None }).collect();
        let xr_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ExportRegion(i) => Some(*i), _ => None }).collect();
        let comp_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentDef(i) => Some(*i), _ => None }).collect();
        let inst_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::ComponentInstance(i) => Some(*i), _ => None }).collect();
        let mc_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::MidiClip(i) => Some(*i), _ => None }).collect();
        let tn_ids: Vec<EntityId> = self.selected.iter().filter_map(|t| match t { HitTarget::TextNote(i) => Some(*i), _ => None }).collect();

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
        for &id in &er_ids { if let Some(d) = self.effect_regions.get(&id) { del_ops.push(operations::Operation::DeleteEffectRegion { id, data: d.clone() }); } self.effect_regions.shift_remove(&id); }
        for &id in &pb_ids { if let Some(d) = self.plugin_blocks.get(&id) { del_ops.push(operations::Operation::DeletePluginBlock { id, data: d.snapshot() }); } self.plugin_blocks.shift_remove(&id); }
        for &id in &lr_ids { if let Some(d) = self.loop_regions.get(&id) { del_ops.push(operations::Operation::DeleteLoopRegion { id, data: d.clone() }); } self.loop_regions.shift_remove(&id); }
        for &id in &xr_ids { if let Some(d) = self.export_regions.get(&id) { del_ops.push(operations::Operation::DeleteExportRegion { id, data: d.clone() }); } self.export_regions.shift_remove(&id); }
        for &id in &mc_ids { if let Some(d) = self.midi_clips.get(&id) { del_ops.push(operations::Operation::DeleteMidiClip { id, data: d.clone() }); } self.midi_clips.shift_remove(&id); }
        for &id in &tn_ids { if let Some(d) = self.text_notes.get(&id) { del_ops.push(operations::Operation::DeleteTextNote { id, data: d.clone() }); } self.text_notes.shift_remove(&id); }
        if !del_ops.is_empty() {
            self.push_op(operations::Operation::Batch(del_ops));
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
