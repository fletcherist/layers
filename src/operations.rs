use crate::ui::waveform::AudioClipData;
use crate::component::{ComponentDef, ComponentInstance};
use crate::effects::{EffectRegion, PluginBlock, PluginBlockSnapshot};
use crate::entity_id::EntityId;
use crate::instruments::{InstrumentRegion, InstrumentRegionSnapshot};
use crate::midi::{MidiClip, MidiNote};
use crate::regions::{ExportRegion, LoopRegion};
use crate::text_note::TextNote;
use crate::{CanvasObject, WaveformView};

pub type UserId = EntityId;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommittedOp {
    pub op: Operation,
    pub user_id: UserId,
    pub timestamp_ms: u64,
    pub seq: u64,
}

/// Invertible operations for undo/redo and network sync.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Operation {
    // --- CanvasObject ---
    CreateObject { id: EntityId, data: CanvasObject },
    DeleteObject { id: EntityId, data: CanvasObject },
    UpdateObject { id: EntityId, before: CanvasObject, after: CanvasObject },

    // --- WaveformView ---
    CreateWaveform { id: EntityId, data: WaveformView, audio_clip: Option<(EntityId, AudioClipData)> },
    DeleteWaveform { id: EntityId, data: WaveformView, audio_clip: Option<(EntityId, AudioClipData)> },
    UpdateWaveform { id: EntityId, before: WaveformView, after: WaveformView },

    // --- MidiClip ---
    CreateMidiClip { id: EntityId, data: MidiClip },
    DeleteMidiClip { id: EntityId, data: MidiClip },
    UpdateMidiClip { id: EntityId, before: MidiClip, after: MidiClip },

    // --- MidiNote (within a clip) ---
    CreateMidiNote { clip_id: EntityId, note_idx: usize, data: MidiNote },
    DeleteMidiNote { clip_id: EntityId, note_idx: usize, data: MidiNote },
    UpdateMidiNotes { clip_id: EntityId, before: Vec<MidiNote>, after: Vec<MidiNote> },

    // --- EffectRegion ---
    CreateEffectRegion { id: EntityId, data: EffectRegion },
    DeleteEffectRegion { id: EntityId, data: EffectRegion },
    UpdateEffectRegion { id: EntityId, before: EffectRegion, after: EffectRegion },

    // --- PluginBlock ---
    CreatePluginBlock { id: EntityId, data: PluginBlockSnapshot },
    DeletePluginBlock { id: EntityId, data: PluginBlockSnapshot },

    // --- LoopRegion ---
    CreateLoopRegion { id: EntityId, data: LoopRegion },
    DeleteLoopRegion { id: EntityId, data: LoopRegion },
    UpdateLoopRegion { id: EntityId, before: LoopRegion, after: LoopRegion },

    // --- ExportRegion ---
    CreateExportRegion { id: EntityId, data: ExportRegion },
    DeleteExportRegion { id: EntityId, data: ExportRegion },
    UpdateExportRegion { id: EntityId, before: ExportRegion, after: ExportRegion },

    // --- ComponentDef ---
    CreateComponent { id: EntityId, data: ComponentDef },
    DeleteComponent { id: EntityId, data: ComponentDef },
    UpdateComponent { id: EntityId, before: ComponentDef, after: ComponentDef },

    // --- ComponentInstance ---
    CreateComponentInstance { id: EntityId, data: ComponentInstance },
    DeleteComponentInstance { id: EntityId, data: ComponentInstance },
    UpdateComponentInstance { id: EntityId, before: ComponentInstance, after: ComponentInstance },

    // --- InstrumentRegion ---
    CreateInstrumentRegion { id: EntityId, data: InstrumentRegionSnapshot },
    DeleteInstrumentRegion { id: EntityId, data: InstrumentRegionSnapshot },
    UpdateInstrumentRegion { id: EntityId, before: InstrumentRegionSnapshot, after: InstrumentRegionSnapshot },

    // --- TextNote ---
    CreateTextNote { id: EntityId, data: TextNote },
    DeleteTextNote { id: EntityId, data: TextNote },
    UpdateTextNote { id: EntityId, before: TextNote, after: TextNote },

    // --- Global state ---
    SetBpm { before: f32, after: f32 },

    // --- Batch ---
    Batch(Vec<Operation>),
}

impl Operation {
    /// Returns the enum variant name as a string (for logging).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Operation::CreateObject { .. } => "CreateObject",
            Operation::DeleteObject { .. } => "DeleteObject",
            Operation::UpdateObject { .. } => "UpdateObject",
            Operation::CreateWaveform { .. } => "CreateWaveform",
            Operation::DeleteWaveform { .. } => "DeleteWaveform",
            Operation::UpdateWaveform { .. } => "UpdateWaveform",
            Operation::CreateMidiClip { .. } => "CreateMidiClip",
            Operation::DeleteMidiClip { .. } => "DeleteMidiClip",
            Operation::UpdateMidiClip { .. } => "UpdateMidiClip",
            Operation::CreateMidiNote { .. } => "CreateMidiNote",
            Operation::DeleteMidiNote { .. } => "DeleteMidiNote",
            Operation::UpdateMidiNotes { .. } => "UpdateMidiNotes",
            Operation::CreateEffectRegion { .. } => "CreateEffectRegion",
            Operation::DeleteEffectRegion { .. } => "DeleteEffectRegion",
            Operation::UpdateEffectRegion { .. } => "UpdateEffectRegion",
            Operation::CreatePluginBlock { .. } => "CreatePluginBlock",
            Operation::DeletePluginBlock { .. } => "DeletePluginBlock",
            Operation::CreateLoopRegion { .. } => "CreateLoopRegion",
            Operation::DeleteLoopRegion { .. } => "DeleteLoopRegion",
            Operation::UpdateLoopRegion { .. } => "UpdateLoopRegion",
            Operation::CreateExportRegion { .. } => "CreateExportRegion",
            Operation::DeleteExportRegion { .. } => "DeleteExportRegion",
            Operation::UpdateExportRegion { .. } => "UpdateExportRegion",
            Operation::CreateComponent { .. } => "CreateComponent",
            Operation::DeleteComponent { .. } => "DeleteComponent",
            Operation::UpdateComponent { .. } => "UpdateComponent",
            Operation::CreateComponentInstance { .. } => "CreateComponentInstance",
            Operation::DeleteComponentInstance { .. } => "DeleteComponentInstance",
            Operation::UpdateComponentInstance { .. } => "UpdateComponentInstance",
            Operation::CreateInstrumentRegion { .. } => "CreateInstrumentRegion",
            Operation::DeleteInstrumentRegion { .. } => "DeleteInstrumentRegion",
            Operation::UpdateInstrumentRegion { .. } => "UpdateInstrumentRegion",
            Operation::CreateTextNote { .. } => "CreateTextNote",
            Operation::DeleteTextNote { .. } => "DeleteTextNote",
            Operation::UpdateTextNote { .. } => "UpdateTextNote",
            Operation::SetBpm { .. } => "SetBpm",
            Operation::Batch(_) => "Batch",
        }
    }

    /// Returns the inverse of this operation (for undo).
    pub fn invert(&self) -> Operation {
        match self {
            // Objects
            Operation::CreateObject { id, data } => Operation::DeleteObject { id: *id, data: data.clone() },
            Operation::DeleteObject { id, data } => Operation::CreateObject { id: *id, data: data.clone() },
            Operation::UpdateObject { id, before, after } => Operation::UpdateObject { id: *id, before: after.clone(), after: before.clone() },

            // Waveforms
            Operation::CreateWaveform { id, data, audio_clip } => Operation::DeleteWaveform { id: *id, data: data.clone(), audio_clip: audio_clip.clone() },
            Operation::DeleteWaveform { id, data, audio_clip } => Operation::CreateWaveform { id: *id, data: data.clone(), audio_clip: audio_clip.clone() },
            Operation::UpdateWaveform { id, before, after } => Operation::UpdateWaveform { id: *id, before: after.clone(), after: before.clone() },

            // MidiClips
            Operation::CreateMidiClip { id, data } => Operation::DeleteMidiClip { id: *id, data: data.clone() },
            Operation::DeleteMidiClip { id, data } => Operation::CreateMidiClip { id: *id, data: data.clone() },
            Operation::UpdateMidiClip { id, before, after } => Operation::UpdateMidiClip { id: *id, before: after.clone(), after: before.clone() },

            // MidiNotes
            Operation::CreateMidiNote { clip_id, note_idx, data } => Operation::DeleteMidiNote { clip_id: *clip_id, note_idx: *note_idx, data: data.clone() },
            Operation::DeleteMidiNote { clip_id, note_idx, data } => Operation::CreateMidiNote { clip_id: *clip_id, note_idx: *note_idx, data: data.clone() },
            Operation::UpdateMidiNotes { clip_id, before, after } => Operation::UpdateMidiNotes { clip_id: *clip_id, before: after.clone(), after: before.clone() },

            // EffectRegions
            Operation::CreateEffectRegion { id, data } => Operation::DeleteEffectRegion { id: *id, data: data.clone() },
            Operation::DeleteEffectRegion { id, data } => Operation::CreateEffectRegion { id: *id, data: data.clone() },
            Operation::UpdateEffectRegion { id, before, after } => Operation::UpdateEffectRegion { id: *id, before: after.clone(), after: before.clone() },

            // PluginBlocks
            Operation::CreatePluginBlock { id, data } => Operation::DeletePluginBlock { id: *id, data: data.clone() },
            Operation::DeletePluginBlock { id, data } => Operation::CreatePluginBlock { id: *id, data: data.clone() },

            // LoopRegions
            Operation::CreateLoopRegion { id, data } => Operation::DeleteLoopRegion { id: *id, data: data.clone() },
            Operation::DeleteLoopRegion { id, data } => Operation::CreateLoopRegion { id: *id, data: data.clone() },
            Operation::UpdateLoopRegion { id, before, after } => Operation::UpdateLoopRegion { id: *id, before: after.clone(), after: before.clone() },

            // ExportRegions
            Operation::CreateExportRegion { id, data } => Operation::DeleteExportRegion { id: *id, data: data.clone() },
            Operation::DeleteExportRegion { id, data } => Operation::CreateExportRegion { id: *id, data: data.clone() },
            Operation::UpdateExportRegion { id, before, after } => Operation::UpdateExportRegion { id: *id, before: after.clone(), after: before.clone() },

            // Components
            Operation::CreateComponent { id, data } => Operation::DeleteComponent { id: *id, data: data.clone() },
            Operation::DeleteComponent { id, data } => Operation::CreateComponent { id: *id, data: data.clone() },
            Operation::UpdateComponent { id, before, after } => Operation::UpdateComponent { id: *id, before: after.clone(), after: before.clone() },

            // ComponentInstances
            Operation::CreateComponentInstance { id, data } => Operation::DeleteComponentInstance { id: *id, data: data.clone() },
            Operation::DeleteComponentInstance { id, data } => Operation::CreateComponentInstance { id: *id, data: data.clone() },
            Operation::UpdateComponentInstance { id, before, after } => Operation::UpdateComponentInstance { id: *id, before: after.clone(), after: before.clone() },

            // InstrumentRegions
            Operation::CreateInstrumentRegion { id, data } => Operation::DeleteInstrumentRegion { id: *id, data: data.clone() },
            Operation::DeleteInstrumentRegion { id, data } => Operation::CreateInstrumentRegion { id: *id, data: data.clone() },
            Operation::UpdateInstrumentRegion { id, before, after } => Operation::UpdateInstrumentRegion { id: *id, before: after.clone(), after: before.clone() },

            // TextNotes
            Operation::CreateTextNote { id, data } => Operation::DeleteTextNote { id: *id, data: data.clone() },
            Operation::DeleteTextNote { id, data } => Operation::CreateTextNote { id: *id, data: data.clone() },
            Operation::UpdateTextNote { id, before, after } => Operation::UpdateTextNote { id: *id, before: after.clone(), after: before.clone() },

            // BPM
            Operation::SetBpm { before, after } => Operation::SetBpm { before: *after, after: *before },

            // Batch
            Operation::Batch(ops) => Operation::Batch(ops.iter().rev().map(|o| o.invert()).collect()),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Local user ID for single-user mode (before networking is added).
pub fn local_user_id() -> UserId {
    // Use a fixed UUID for the local user — this will be replaced with actual user IDs in Phase 3.
    uuid::Uuid::nil()
}

static OP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn commit_op(op: Operation) -> CommittedOp {
    CommittedOp {
        op,
        user_id: local_user_id(),
        timestamp_ms: now_ms(),
        seq: OP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    }
}

pub fn commit_op_as(op: Operation, user_id: UserId) -> CommittedOp {
    CommittedOp {
        op,
        user_id,
        timestamp_ms: now_ms(),
        seq: OP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    }
}

// ---------------------------------------------------------------------------
// Apply: mutate App state according to an Operation
// ---------------------------------------------------------------------------

use crate::{App, EntityBeforeState, HitTarget};

impl Operation {
    /// Apply this operation to the app state (forward direction).
    pub fn apply(&self, app: &mut App) {
        match self {
            // --- CanvasObject ---
            Operation::CreateObject { id, data } => {
                app.objects.insert(*id, data.clone());
            }
            Operation::DeleteObject { id, .. } => {
                app.objects.shift_remove(id);
            }
            Operation::UpdateObject { id, after, .. } => {
                if let Some(obj) = app.objects.get_mut(id) {
                    *obj = after.clone();
                }
            }

            // --- WaveformView ---
            Operation::CreateWaveform { id, data, audio_clip } => {
                app.waveforms.insert(*id, data.clone());
                if let Some((ac_id, ac)) = audio_clip {
                    app.audio_clips.insert(*ac_id, ac.clone());
                }
            }
            Operation::DeleteWaveform { id, .. } => {
                app.waveforms.shift_remove(id);
                app.audio_clips.shift_remove(id);
            }
            Operation::UpdateWaveform { id, after, .. } => {
                if let Some(wf) = app.waveforms.get_mut(id) {
                    let existing_audio = wf.audio.clone();
                    *wf = after.clone();
                    // Preserve audio/peaks if the incoming waveform has empty audio
                    // (audio is #[serde(skip)] so remote operations always have empty audio)
                    if wf.audio.left_samples.is_empty() && !existing_audio.left_samples.is_empty()
                    {
                        wf.audio = existing_audio;
                    }
                }
            }

            // --- MidiClip ---
            Operation::CreateMidiClip { id, data } => {
                app.midi_clips.insert(*id, data.clone());
            }
            Operation::DeleteMidiClip { id, .. } => {
                app.midi_clips.shift_remove(id);
            }
            Operation::UpdateMidiClip { id, after, .. } => {
                if let Some(clip) = app.midi_clips.get_mut(id) {
                    *clip = after.clone();
                }
            }

            // --- MidiNote ---
            Operation::CreateMidiNote { clip_id, note_idx, data } => {
                if let Some(clip) = app.midi_clips.get_mut(clip_id) {
                    let idx = (*note_idx).min(clip.notes.len());
                    clip.notes.insert(idx, data.clone());
                }
            }
            Operation::DeleteMidiNote { clip_id, note_idx, .. } => {
                if let Some(clip) = app.midi_clips.get_mut(clip_id) {
                    if *note_idx < clip.notes.len() {
                        clip.notes.remove(*note_idx);
                    }
                }
            }
            Operation::UpdateMidiNotes { clip_id, after, .. } => {
                if let Some(clip) = app.midi_clips.get_mut(clip_id) {
                    clip.notes = after.clone();
                }
            }

            // --- EffectRegion ---
            Operation::CreateEffectRegion { id, data } => {
                app.effect_regions.insert(*id, data.clone());
            }
            Operation::DeleteEffectRegion { id, .. } => {
                app.effect_regions.shift_remove(id);
            }
            Operation::UpdateEffectRegion { id, after, .. } => {
                if let Some(er) = app.effect_regions.get_mut(id) {
                    *er = after.clone();
                }
            }

            // --- PluginBlock (snapshot-based create/delete) ---
            Operation::CreatePluginBlock { id, data } => {
                use std::sync::{Arc, Mutex};
                app.plugin_blocks.insert(*id, crate::effects::PluginBlock {
                    position: data.position,
                    size: data.size,
                    color: data.color,
                    plugin_id: data.plugin_id.clone(),
                    plugin_name: data.plugin_name.clone(),
                    plugin_path: data.plugin_path.clone(),
                    bypass: data.bypass,
                    gui: Arc::new(Mutex::new(None)),
                    pending_state: None,
                    pending_params: None,
                });
            }
            Operation::DeletePluginBlock { id, .. } => {
                app.plugin_blocks.shift_remove(id);
            }

            // --- LoopRegion ---
            Operation::CreateLoopRegion { id, data } => {
                app.loop_regions.insert(*id, data.clone());
            }
            Operation::DeleteLoopRegion { id, .. } => {
                app.loop_regions.shift_remove(id);
            }
            Operation::UpdateLoopRegion { id, after, .. } => {
                if let Some(lr) = app.loop_regions.get_mut(id) {
                    *lr = after.clone();
                }
            }

            // --- ExportRegion ---
            Operation::CreateExportRegion { id, data } => {
                app.export_regions.insert(*id, data.clone());
            }
            Operation::DeleteExportRegion { id, .. } => {
                app.export_regions.shift_remove(id);
            }
            Operation::UpdateExportRegion { id, after, .. } => {
                if let Some(xr) = app.export_regions.get_mut(id) {
                    *xr = after.clone();
                }
            }

            // --- ComponentDef ---
            Operation::CreateComponent { id, data } => {
                app.components.insert(*id, data.clone());
            }
            Operation::DeleteComponent { id, .. } => {
                app.components.shift_remove(id);
            }
            Operation::UpdateComponent { id, after, .. } => {
                if let Some(c) = app.components.get_mut(id) {
                    *c = after.clone();
                }
            }

            // --- ComponentInstance ---
            Operation::CreateComponentInstance { id, data } => {
                app.component_instances.insert(*id, data.clone());
            }
            Operation::DeleteComponentInstance { id, .. } => {
                app.component_instances.shift_remove(id);
            }
            Operation::UpdateComponentInstance { id, after, .. } => {
                if let Some(ci) = app.component_instances.get_mut(id) {
                    *ci = after.clone();
                }
            }

            // --- InstrumentRegion (snapshot-based) ---
            Operation::CreateInstrumentRegion { id, data } => {
                let mut ir = crate::instruments::InstrumentRegion::new(data.position, data.size);
                ir.name = data.name.clone();
                ir.plugin_id = data.plugin_id.clone();
                ir.plugin_name = data.plugin_name.clone();
                ir.plugin_path = data.plugin_path.clone();
                app.instrument_regions.insert(*id, ir);
            }
            Operation::DeleteInstrumentRegion { id, .. } => {
                app.instrument_regions.shift_remove(id);
            }
            Operation::UpdateInstrumentRegion { id, after, .. } => {
                if let Some(ir) = app.instrument_regions.get_mut(id) {
                    ir.position = after.position;
                    ir.size = after.size;
                    ir.name = after.name.clone();
                    ir.plugin_id = after.plugin_id.clone();
                    ir.plugin_name = after.plugin_name.clone();
                    ir.plugin_path = after.plugin_path.clone();
                }
            }

            // --- TextNote ---
            Operation::CreateTextNote { id, data } => {
                app.text_notes.insert(*id, data.clone());
            }
            Operation::DeleteTextNote { id, .. } => {
                app.text_notes.shift_remove(id);
            }
            Operation::UpdateTextNote { id, after, .. } => {
                if let Some(tn) = app.text_notes.get_mut(id) {
                    *tn = after.clone();
                }
            }

            // --- Global state ---
            Operation::SetBpm { before, after } => {
                let scale = before / after;
                app.rescale_clip_positions(scale);
                app.rescale_camera_for_bpm(scale);
                app.bpm = *after;
                app.resize_warped_clips();
                app.resolve_all_waveform_overlaps();
            }

            // --- Batch ---
            Operation::Batch(ops) => {
                for op in ops {
                    op.apply(app);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Op-based undo/redo on App
// ---------------------------------------------------------------------------

impl App {
    /// Push an operation onto the op-based undo stack. The operation should already
    /// have been applied to the app state before calling this.
    pub(crate) fn push_op(&mut self, op: Operation) {
        // Flush any pending arrow-nudge coalescing before pushing a new op
        if self.arrow_nudge_before.is_some() {
            // Take the before-states to avoid re-entrant call
            let before_states = self.arrow_nudge_before.take();
            self.arrow_nudge_last = None;
            if let Some(states) = before_states {
                let mut nudge_ops = Vec::new();
                for (target, bs) in states {
                    match (target, bs) {
                        (HitTarget::Object(id), EntityBeforeState::Object(before)) => {
                            if let Some(after) = self.objects.get(&id) {
                                nudge_ops.push(Operation::UpdateObject { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::Waveform(id), EntityBeforeState::Waveform(before)) => {
                            if let Some(after) = self.waveforms.get(&id) {
                                nudge_ops.push(Operation::UpdateWaveform { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::EffectRegion(id), EntityBeforeState::EffectRegion(before)) => {
                            if let Some(after) = self.effect_regions.get(&id) {
                                nudge_ops.push(Operation::UpdateEffectRegion { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::PluginBlock(id), EntityBeforeState::PluginBlock(before)) => {
                            if let Some(after) = self.plugin_blocks.get(&id) {
                                nudge_ops.push(Operation::DeletePluginBlock { id, data: before });
                                nudge_ops.push(Operation::CreatePluginBlock { id, data: after.snapshot() });
                            }
                        }
                        (HitTarget::LoopRegion(id), EntityBeforeState::LoopRegion(before)) => {
                            if let Some(after) = self.loop_regions.get(&id) {
                                nudge_ops.push(Operation::UpdateLoopRegion { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::ExportRegion(id), EntityBeforeState::ExportRegion(before)) => {
                            if let Some(after) = self.export_regions.get(&id) {
                                nudge_ops.push(Operation::UpdateExportRegion { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::ComponentDef(id), EntityBeforeState::ComponentDef(before)) => {
                            if let Some(after) = self.components.get(&id) {
                                nudge_ops.push(Operation::UpdateComponent { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::ComponentInstance(id), EntityBeforeState::ComponentInstance(before)) => {
                            if let Some(after) = self.component_instances.get(&id) {
                                nudge_ops.push(Operation::UpdateComponentInstance { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::MidiClip(id), EntityBeforeState::MidiClip(before)) => {
                            if let Some(after) = self.midi_clips.get(&id) {
                                nudge_ops.push(Operation::UpdateMidiClip { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::TextNote(id), EntityBeforeState::TextNote(before)) => {
                            if let Some(after) = self.text_notes.get(&id) {
                                nudge_ops.push(Operation::UpdateTextNote { id, before, after: after.clone() });
                            }
                        }
                        (HitTarget::InstrumentRegion(id), EntityBeforeState::InstrumentRegion(before)) => {
                            if let Some(ir) = self.instrument_regions.get(&id) {
                                let after = crate::instruments::InstrumentRegionSnapshot {
                                    position: ir.position, size: ir.size,
                                    name: ir.name.clone(), plugin_id: ir.plugin_id.clone(),
                                    plugin_name: ir.plugin_name.clone(), plugin_path: ir.plugin_path.clone(),
                                };
                                nudge_ops.push(Operation::UpdateInstrumentRegion { id, before, after });
                            }
                        }
                        _ => {}
                    }
                }
                // Commit overlap changes from live resolution
                let overlap_snaps = std::mem::take(&mut self.arrow_nudge_overlap_snapshots);
                for (id, original) in overlap_snaps {
                    if let Some(wf) = self.waveforms.get(&id) {
                        if wf.disabled {
                            self.waveforms.shift_remove(&id);
                            let ac = self.audio_clips.shift_remove(&id);
                            nudge_ops.push(Operation::DeleteWaveform {
                                id, data: original, audio_clip: ac.map(|c| (id, c)),
                            });
                        } else {
                            nudge_ops.push(Operation::UpdateWaveform {
                                id, before: original, after: wf.clone(),
                            });
                        }
                    }
                }
                for id in self.arrow_nudge_overlap_temp_splits.drain(..) {
                    if let Some(wf_data) = self.waveforms.get(&id).cloned() {
                        let ac = self.audio_clips.get(&id).cloned();
                        nudge_ops.push(Operation::CreateWaveform {
                            id, data: wf_data, audio_clip: ac.map(|c| (id, c)),
                        });
                    }
                }
                if !nudge_ops.is_empty() {
                    let nudge_batch = Operation::Batch(nudge_ops);
                    // Push the nudge batch directly (inline to avoid recursion)
                    if self.can_mutate() {
                        let committed = commit_op_as(nudge_batch, self.local_user.id);
                        log::info!("[SYNC] push_op: {} (seq={}, user={})", committed.op.variant_name(), committed.seq, committed.user_id);
                        self.network.send_op(committed.clone());
                        self.op_undo_stack.push(committed);
                        if self.op_undo_stack.len() > crate::history::MAX_UNDO_HISTORY {
                            self.op_undo_stack.remove(0);
                        }
                        // Don't clear redo here — the subsequent push_op will do it
                    }
                }
            }
        }
        // Block mutations when disconnected from server
        if !self.can_mutate() {
            op.invert().apply(self);
            self.toast_manager.push(
                "Cannot edit while disconnected".to_string(),
                crate::ui::toast::ToastKind::Error,
            );
            return;
        }
        let committed = commit_op_as(op, self.local_user.id);
        log::info!("[SYNC] push_op: {} (seq={}, user={})", committed.op.variant_name(), committed.seq, committed.user_id);
        // If connected, also broadcast to network
        self.network.send_op(committed.clone());
        self.op_undo_stack.push(committed);
        if self.op_undo_stack.len() > crate::history::MAX_UNDO_HISTORY {
            self.op_undo_stack.remove(0);
        }
        self.op_redo_stack.clear();
        self.mark_dirty();
    }

    /// Undo the most recent operation (op-based).
    pub(crate) fn undo_op(&mut self) {
        if let Some(committed) = self.op_undo_stack.pop() {
            // Check if this is a position-only batch (arrow nudge) before applying
            let restore_targets = Self::batch_update_targets(&committed.op);
            let inverse = committed.op.invert();
            inverse.apply(self);
            self.op_redo_stack.push(committed);
            if let Some(targets) = restore_targets {
                self.selected = targets;
            } else {
                self.selected.clear();
            }
            self.update_right_window();
            self.mark_dirty();
            self.sync_audio_clips();
            self.sync_loop_region();
            self.request_redraw();
        }
    }

    /// Redo the most recently undone operation (op-based).
    pub(crate) fn redo_op(&mut self) {
        if let Some(committed) = self.op_redo_stack.pop() {
            let restore_targets = Self::batch_update_targets(&committed.op);
            committed.op.apply(self);
            self.op_undo_stack.push(committed);
            if let Some(targets) = restore_targets {
                self.selected = targets;
            } else {
                self.selected.clear();
            }
            self.update_right_window();
            self.mark_dirty();
            self.sync_audio_clips();
            self.sync_loop_region();
            self.request_redraw();
        }
    }

    /// If the operation is a Batch containing only Update* variants, return
    /// the HitTargets so the selection can be preserved across undo/redo.
    fn batch_update_targets(op: &Operation) -> Option<Vec<HitTarget>> {
        // Handle standalone update operations (e.g., volume/pan keyboard change)
        match op {
            Operation::UpdateObject { id, .. } => return Some(vec![HitTarget::Object(*id)]),
            Operation::UpdateWaveform { id, .. } => return Some(vec![HitTarget::Waveform(*id)]),
            Operation::UpdateEffectRegion { id, .. } => return Some(vec![HitTarget::EffectRegion(*id)]),
            Operation::UpdateLoopRegion { id, .. } => return Some(vec![HitTarget::LoopRegion(*id)]),
            Operation::UpdateExportRegion { id, .. } => return Some(vec![HitTarget::ExportRegion(*id)]),
            Operation::UpdateComponent { id, .. } => return Some(vec![HitTarget::ComponentDef(*id)]),
            Operation::UpdateComponentInstance { id, .. } => return Some(vec![HitTarget::ComponentInstance(*id)]),
            Operation::UpdateMidiClip { id, .. } => return Some(vec![HitTarget::MidiClip(*id)]),
            Operation::UpdateInstrumentRegion { id, .. } => return Some(vec![HitTarget::InstrumentRegion(*id)]),
            Operation::UpdateTextNote { id, .. } => return Some(vec![HitTarget::TextNote(*id)]),
            _ => {}
        }
        // Handle batch of update operations (arrow nudge)
        if let Operation::Batch(ops) = op {
            let mut targets = Vec::new();
            for sub in ops {
                match sub {
                    Operation::UpdateObject { id, .. } => targets.push(HitTarget::Object(*id)),
                    Operation::UpdateWaveform { id, .. } => targets.push(HitTarget::Waveform(*id)),
                    Operation::UpdateEffectRegion { id, .. } => targets.push(HitTarget::EffectRegion(*id)),
                    Operation::UpdateLoopRegion { id, .. } => targets.push(HitTarget::LoopRegion(*id)),
                    Operation::UpdateExportRegion { id, .. } => targets.push(HitTarget::ExportRegion(*id)),
                    Operation::UpdateComponent { id, .. } => targets.push(HitTarget::ComponentDef(*id)),
                    Operation::UpdateComponentInstance { id, .. } => targets.push(HitTarget::ComponentInstance(*id)),
                    Operation::UpdateMidiClip { id, .. } => targets.push(HitTarget::MidiClip(*id)),
                    Operation::UpdateInstrumentRegion { id, .. } => targets.push(HitTarget::InstrumentRegion(*id)),
                    Operation::UpdateTextNote { id, .. } => targets.push(HitTarget::TextNote(*id)),
                    // PluginBlock uses Delete+Create pair for updates
                    Operation::DeletePluginBlock { id, .. } => targets.push(HitTarget::PluginBlock(*id)),
                    Operation::CreatePluginBlock { .. } => { /* paired with Delete above */ }
                    _ => return None, // non-update op → don't preserve selection
                }
            }
            if !targets.is_empty() {
                // Deduplicate (PluginBlock has two ops per entity)
                targets.dedup();
                return Some(targets);
            }
        }
        None
    }

    /// Apply a remote operation (from network) without pushing to local undo.
    /// Deduplicates by (user_id, seq) to prevent double-application.
    pub(crate) fn apply_remote_op(&mut self, committed: CommittedOp) {
        let key = (committed.user_id, committed.seq);
        if !self.applied_remote_seqs.insert(key) {
            log::info!("[SYNC] apply_remote_op: SKIPPED duplicate {} (user={}, seq={})", committed.op.variant_name(), committed.user_id, committed.seq);
            return; // duplicate — already applied
        }
        // Bound memory: clear set when it gets too large
        if self.applied_remote_seqs.len() > 20_000 {
            self.applied_remote_seqs.clear();
        }
        log::info!("[SYNC] apply_remote_op: {} (user={}, seq={})", committed.op.variant_name(), committed.user_id, committed.seq);
        committed.op.apply(self);

        // After applying, load audio from remote storage for any new waveforms
        #[cfg(feature = "native")]
        if self.remote_storage.is_some() {
            let wf_ids = collect_create_waveform_ids(&committed.op);
            for wf_id in wf_ids {
                self.load_waveform_audio_from_remote(wf_id);
            }
        }

        self.mark_dirty();
        self.sync_audio_clips();
        self.request_redraw();
    }

    #[cfg(feature = "native")]
    fn load_waveform_audio_from_remote(&mut self, wf_id: EntityId) {
        let wf = match self.waveforms.get(&wf_id) {
            Some(wf) => wf,
            None => return,
        };

        // Only load if audio data is empty (i.e. lost during serialization)
        if !wf.audio.left_samples.is_empty() {
            return;
        }

        let rs = match &self.remote_storage {
            Some(rs) => rs.clone(),
            None => return,
        };

        let filename = wf.filename.clone();
        let tx = self.pending_remote_audio_tx.clone();

        std::thread::spawn(move || {
            let wf_id_str = wf_id.to_string();

            if let Some((file_bytes, ext)) = rs.load_audio(&wf_id_str) {
                use crate::ui::waveform::{AudioData, WaveformPeaks};

                let Some(loaded) = crate::audio::load_audio_from_bytes(&file_bytes, &ext) else {
                    eprintln!("[RemoteAudio] Failed to decode audio for {wf_id_str}");
                    return;
                };

                let left_peaks = WaveformPeaks::build(&loaded.left_samples);
                let right_peaks = WaveformPeaks::build(&loaded.right_samples);

                let new_audio = std::sync::Arc::new(AudioData {
                    left_samples: loaded.left_samples.clone(),
                    right_samples: loaded.right_samples.clone(),
                    left_peaks: std::sync::Arc::new(left_peaks),
                    right_peaks: std::sync::Arc::new(right_peaks),
                    sample_rate: loaded.sample_rate,
                    filename,
                });

                let ac = crate::ui::waveform::AudioClipData {
                    samples: loaded.samples,
                    sample_rate: loaded.sample_rate,
                    duration_secs: loaded.duration_secs,
                };

                let _ = tx.send(crate::PendingRemoteAudioFetch {
                    wf_id,
                    audio: new_audio,
                    ac,
                });
            }
        });
    }
}

#[cfg(feature = "native")]
fn collect_create_waveform_ids(op: &Operation) -> Vec<EntityId> {
    let mut ids = Vec::new();
    match op {
        Operation::CreateWaveform { id, .. } => ids.push(*id),
        Operation::Batch(ops) => {
            for o in ops {
                ids.extend(collect_create_waveform_ids(o));
            }
        }
        _ => {}
    }
    ids
}
