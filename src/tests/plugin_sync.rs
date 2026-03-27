use crate::App;
use crate::entity_id::new_id;
use crate::effects::{EffectChain, EffectChainSlot, EffectChainSlotSnapshot};
use crate::operations::Operation;
use std::path::PathBuf;

fn make_slot(name: &str) -> EffectChainSlot {
    EffectChainSlot::new(
        format!("{}_id", name),
        name.to_string(),
        PathBuf::from("/test/plugin.vst3"),
    )
}

fn make_slot_snapshot(name: &str) -> EffectChainSlotSnapshot {
    let slot = make_slot(name);
    slot.snapshot()
}

#[test]
fn add_effect_slot_via_operation() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    app.effect_chains.insert(chain_id, EffectChain::new());

    let snap = make_slot_snapshot("Reverb");
    let op = Operation::AddEffectSlot { chain_id, slot_idx: 0, data: snap };
    op.apply(&mut app);

    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "Reverb");
}

#[test]
fn remove_effect_slot_via_operation() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    chain.slots.push(make_slot("Delay"));
    app.effect_chains.insert(chain_id, chain);

    let snap = app.effect_chains[&chain_id].slots[0].snapshot();
    let op = Operation::RemoveEffectSlot { chain_id, slot_idx: 0, data: snap };
    op.apply(&mut app);

    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "Delay");
}

#[test]
fn reorder_effect_slot_via_operation() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("A"));
    chain.slots.push(make_slot("B"));
    chain.slots.push(make_slot("C"));
    app.effect_chains.insert(chain_id, chain);

    // Move slot 0 (A) to position 2
    let op = Operation::ReorderEffectSlot { chain_id, from_idx: 0, to_idx: 2 };
    op.apply(&mut app);

    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "B");
    assert_eq!(app.effect_chains[&chain_id].slots[1].plugin_name, "C");
    assert_eq!(app.effect_chains[&chain_id].slots[2].plugin_name, "A");
}

#[test]
fn bypass_toggle_via_operation() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);

    let before = app.effect_chains[&chain_id].slots[0].snapshot();
    let after = EffectChainSlotSnapshot { bypass: true, ..before.clone() };

    let op = Operation::UpdateEffectSlot { chain_id, slot_idx: 0, before, after };
    op.apply(&mut app);

    assert!(app.effect_chains[&chain_id].slots[0].bypass);
}

#[test]
fn add_effect_slot_undo_redo() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    app.effect_chains.insert(chain_id, EffectChain::new());

    let snap = make_slot_snapshot("Reverb");
    let op = Operation::AddEffectSlot { chain_id, slot_idx: 0, data: snap };
    op.apply(&mut app);
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);

    // Undo (invert + apply)
    let inverse = op.invert();
    inverse.apply(&mut app);
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 0);

    // Redo (invert the inverse + apply)
    let redo = inverse.invert();
    redo.apply(&mut app);
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "Reverb");
}

#[test]
fn remove_effect_slot_undo_redo() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);

    let snap = app.effect_chains[&chain_id].slots[0].snapshot();
    let op = Operation::RemoveEffectSlot { chain_id, slot_idx: 0, data: snap };
    op.apply(&mut app);
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 0);

    // Undo — slot should be restored
    let inverse = op.invert();
    inverse.apply(&mut app);
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "Reverb");
}

#[test]
fn effect_chain_cleanup_on_last_slot_removal() {
    let mut app = App::new_headless();

    // Create a waveform with an effect chain
    let wf_id = new_id();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);

    use std::sync::Arc;
    use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView, WarpMode};
    use crate::automation::AutomationData;
    let mut wf = WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 44100,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 60.0],
        color: [0.3, 0.5, 0.9, 1.0],
        border_radius: 4.0,
        fade_in_px: 0.0,
        fade_out_px: 0.0,
        fade_in_curve: 0.0,
        fade_out_curve: 0.0,
        volume: 1.0,
        pan: 0.5,
        warp_mode: WarpMode::Off,
        sample_bpm: 120.0,
        pitch_semitones: 0.0,
            paulstretch_factor: 8.0,
        is_reversed: false,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
        effect_chain_id: Some(chain_id),
        take_group: None,
    };
    app.waveforms.insert(wf_id, wf);

    // Remove the only slot — apply RemoveEffectSlot
    let snap = app.effect_chains[&chain_id].slots[0].snapshot();
    let op = Operation::RemoveEffectSlot { chain_id, slot_idx: 0, data: snap };
    op.apply(&mut app);

    // Chain still exists but is empty (cleanup is the caller's responsibility via DeleteEffectChain)
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 0);

    // Apply DeleteEffectChain to clean up
    let delete_op = Operation::DeleteEffectChain { id: chain_id };
    delete_op.apply(&mut app);

    assert!(!app.effect_chains.contains_key(&chain_id));
    assert!(app.waveforms[&wf_id].effect_chain_id.is_none());
}

#[test]
fn create_effect_chain_for_waveform_via_batch() {
    let mut app = App::new_headless();

    use std::sync::Arc;
    use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView, WarpMode};
    use crate::automation::AutomationData;
    let wf_id = new_id();
    let mut wf = WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 44100,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 60.0],
        color: [0.3, 0.5, 0.9, 1.0],
        border_radius: 4.0,
        fade_in_px: 0.0,
        fade_out_px: 0.0,
        fade_in_curve: 0.0,
        fade_out_curve: 0.0,
        volume: 1.0,
        pan: 0.5,
        warp_mode: WarpMode::Off,
        sample_bpm: 120.0,
        pitch_semitones: 0.0,
            paulstretch_factor: 8.0,
        is_reversed: false,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
        effect_chain_id: None,
        take_group: None,
    };
    app.waveforms.insert(wf_id, wf.clone());

    // Simulate what add_plugin_to_waveform_chain does: batch of CreateEffectChain + UpdateWaveform + AddEffectSlot
    let chain_id = new_id();
    let before = wf.clone();
    wf.effect_chain_id = Some(chain_id);
    let after = wf.clone();

    let slot_snap = make_slot_snapshot("TestPlugin");

    let batch = Operation::Batch(vec![
        Operation::CreateEffectChain { id: chain_id },
        Operation::UpdateWaveform { id: wf_id, before, after },
        Operation::AddEffectSlot { chain_id, slot_idx: 0, data: slot_snap },
    ]);

    batch.apply(&mut app);

    assert!(app.effect_chains.contains_key(&chain_id));
    assert_eq!(app.waveforms[&wf_id].effect_chain_id, Some(chain_id));
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "TestPlugin");

    // Undo the batch
    let inverse = batch.invert();
    inverse.apply(&mut app);

    assert!(!app.effect_chains.contains_key(&chain_id));
    assert!(app.waveforms[&wf_id].effect_chain_id.is_none());
}
