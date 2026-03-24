use std::sync::Arc;
use crate::App;
use crate::entity_id::new_id;
use crate::effects::{EffectChain, EffectChainSlot};
use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView, WarpMode};
use crate::automation::AutomationData;
use std::path::PathBuf;

fn make_waveform(x: f32, y: f32) -> WaveformView {
    WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 44100,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [x, y],
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
        is_reversed: false,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
        effect_chain_id: None,
        take_group: None,
    }
}

fn make_slot(name: &str) -> EffectChainSlot {
    EffectChainSlot::new(
        format!("{}_id", name),
        name.to_string(),
        PathBuf::from("/test/plugin.vst3"),
    )
}

#[test]
fn add_effect_chain_to_waveform() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));

    // Waveform starts with no chain
    assert!(app.waveforms[&wf_id].effect_chain_id.is_none());
    assert!(app.effect_chains.is_empty());

    // Create a chain and assign it
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    chain.slots.push(make_slot("Delay"));
    app.effect_chains.insert(chain_id, chain);
    app.waveforms.get_mut(&wf_id).unwrap().effect_chain_id = Some(chain_id);

    assert_eq!(app.waveforms[&wf_id].effect_chain_id, Some(chain_id));
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 2);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "Reverb");
    assert_eq!(app.effect_chains[&chain_id].slots[1].plugin_name, "Delay");
}

#[test]
fn shared_chain_across_waveforms() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));

    // Create shared chain
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Compressor"));
    app.effect_chains.insert(chain_id, chain);

    // Assign same chain to both
    app.waveforms.get_mut(&wf1).unwrap().effect_chain_id = Some(chain_id);
    app.waveforms.get_mut(&wf2).unwrap().effect_chain_id = Some(chain_id);

    // Both reference same chain
    assert_eq!(app.waveforms[&wf1].effect_chain_id, app.waveforms[&wf2].effect_chain_id);

    // Ref count should be 2
    let ref_count = crate::ui::right_window::RightWindow::chain_ref_count(chain_id, &app.waveforms);
    assert_eq!(ref_count, 2);

    // Modifying chain affects both
    app.effect_chains.get_mut(&chain_id).unwrap().slots.push(make_slot("EQ"));
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 2);
}

#[test]
fn detach_effect_chain() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));

    // Create shared chain
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);
    app.waveforms.get_mut(&wf1).unwrap().effect_chain_id = Some(chain_id);
    app.waveforms.get_mut(&wf2).unwrap().effect_chain_id = Some(chain_id);

    // Detach wf2's chain
    app.detach_effect_chain(wf2);

    // wf1 still references original chain
    assert_eq!(app.waveforms[&wf1].effect_chain_id, Some(chain_id));

    // wf2 now has a different chain
    let wf2_chain_id = app.waveforms[&wf2].effect_chain_id.unwrap();
    assert_ne!(wf2_chain_id, chain_id);

    // Both chains have the same content
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&wf2_chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&wf2_chain_id].slots[0].plugin_name, "Reverb");

    // Ref counts are now 1 each
    let ref1 = crate::ui::right_window::RightWindow::chain_ref_count(chain_id, &app.waveforms);
    let ref2 = crate::ui::right_window::RightWindow::chain_ref_count(wf2_chain_id, &app.waveforms);
    assert_eq!(ref1, 1);
    assert_eq!(ref2, 1);
}

#[test]
fn detach_noop_when_unique() {
    let mut app = App::new_headless();
    let wf = new_id();
    app.waveforms.insert(wf, make_waveform(100.0, 100.0));

    let chain_id = new_id();
    app.effect_chains.insert(chain_id, EffectChain::new());
    app.waveforms.get_mut(&wf).unwrap().effect_chain_id = Some(chain_id);

    // Detaching when ref_count=1 should be a no-op
    app.detach_effect_chain(wf);
    assert_eq!(app.waveforms[&wf].effect_chain_id, Some(chain_id));
    assert_eq!(app.effect_chains.len(), 1); // no new chain created
}

#[test]
fn remove_slot_cleans_up_empty_chain() {
    let mut app = App::new_headless();
    let wf = new_id();
    app.waveforms.insert(wf, make_waveform(100.0, 100.0));

    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);
    app.waveforms.get_mut(&wf).unwrap().effect_chain_id = Some(chain_id);

    // Remove the only slot
    app.effect_chains.get_mut(&chain_id).unwrap().slots.remove(0);
    let chain = app.effect_chains.get(&chain_id).unwrap();
    assert!(chain.slots.is_empty());

    // Simulate the cleanup logic (same as in click handler)
    if chain.slots.is_empty() {
        app.effect_chains.shift_remove(&chain_id);
        for wf_mut in app.waveforms.values_mut() {
            if wf_mut.effect_chain_id == Some(chain_id) {
                wf_mut.effect_chain_id = None;
            }
        }
    }

    assert!(app.waveforms[&wf].effect_chain_id.is_none());
    assert!(app.effect_chains.is_empty());
}

#[test]
fn reorder_slots() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("A"));
    chain.slots.push(make_slot("B"));
    chain.slots.push(make_slot("C"));
    app.effect_chains.insert(chain_id, chain);

    // Move slot 0 (A) to position 2
    let chain = app.effect_chains.get_mut(&chain_id).unwrap();
    let slot = chain.slots.remove(0);
    chain.slots.insert(1, slot); // after removing idx 0, position 2 becomes 1

    assert_eq!(chain.slots[0].plugin_name, "B");
    assert_eq!(chain.slots[1].plugin_name, "A");
    assert_eq!(chain.slots[2].plugin_name, "C");
}

#[test]
fn bypass_toggle() {
    let mut app = App::new_headless();
    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);

    assert!(!app.effect_chains[&chain_id].slots[0].bypass);

    app.effect_chains.get_mut(&chain_id).unwrap().slots[0].bypass = true;
    assert!(app.effect_chains[&chain_id].slots[0].bypass);

    app.effect_chains.get_mut(&chain_id).unwrap().slots[0].bypass = false;
    assert!(!app.effect_chains[&chain_id].slots[0].bypass);
}

#[test]
fn copy_paste_shares_chain() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let mut wf = make_waveform(100.0, 100.0);

    let chain_id = new_id();
    let mut chain = EffectChain::new();
    chain.slots.push(make_slot("Reverb"));
    app.effect_chains.insert(chain_id, chain);
    wf.effect_chain_id = Some(chain_id);
    app.waveforms.insert(wf_id, wf);

    // Simulate paste: clone the waveform
    let pasted_wf = app.waveforms[&wf_id].clone();
    let pasted_id = new_id();
    app.waveforms.insert(pasted_id, pasted_wf);

    // Both should share the same chain
    assert_eq!(
        app.waveforms[&wf_id].effect_chain_id,
        app.waveforms[&pasted_id].effect_chain_id
    );
    let ref_count = crate::ui::right_window::RightWindow::chain_ref_count(chain_id, &app.waveforms);
    assert_eq!(ref_count, 2);
}

#[test]
fn drop_plugin_on_right_window_adds_to_chain() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));

    // No chain initially
    assert!(app.waveforms[&wf_id].effect_chain_id.is_none());

    // Add plugin via the method that the right-window drop calls
    app.add_plugin_to_waveform_chain(wf_id, "test_reverb_id", "TestReverb");

    // Chain should be created with one slot
    let chain_id = app.waveforms[&wf_id].effect_chain_id.unwrap();
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 1);
    assert_eq!(app.effect_chains[&chain_id].slots[0].plugin_name, "TestReverb");

    // Add a second plugin — should append
    app.add_plugin_to_waveform_chain(wf_id, "test_delay_id", "TestDelay");
    assert_eq!(app.effect_chains[&chain_id].slots.len(), 2);
    assert_eq!(app.effect_chains[&chain_id].slots[1].plugin_name, "TestDelay");
}
