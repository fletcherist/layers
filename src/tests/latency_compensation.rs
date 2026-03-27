use std::sync::{Arc, Mutex};
use crate::App;
use crate::entity_id::new_id;
use crate::effects::{EffectChain, EffectChainSlot};
use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView, WarpMode};
use crate::automation::AutomationData;
use crate::ui::waveform::AudioClipData;
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

/// Collect chain latency returns 0 for stubs (no real plugins loaded in headless mode).
#[test]
fn collect_chain_latency_returns_zero_for_stubs() {
    let latency = App::collect_chain_latency(&[]);
    assert_eq!(latency, 0);
}

/// Collect chain latency returns 0 for None-valued plugin handles.
#[test]
fn collect_chain_latency_returns_zero_for_none_plugins() {
    let handle: Arc<Mutex<Option<crate::effects::PluginGuiHandle>>> = Arc::new(Mutex::new(None));
    let latency = App::collect_chain_latency(&[handle]);
    assert_eq!(latency, 0);
}

/// Waveform with effect chain has chain plugins collected correctly.
#[test]
fn waveform_chain_plugins_collected_with_latency() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let chain_id = new_id();

    let mut wf = make_waveform(100.0, 100.0);
    wf.effect_chain_id = Some(chain_id);
    app.waveforms.insert(wf_id, wf);

    let clip = AudioClipData {
        samples: Arc::new(vec![0.0; 44100]),
        sample_rate: 44100,
        duration_secs: 1.0,
    };
    app.audio_clips.insert(wf_id, clip);

    let slot = EffectChainSlot::new(
        "test_uid".to_string(),
        "Test Plugin".to_string(),
        PathBuf::from("/test/plugin.vst3"),
    );
    let mut chain = EffectChain::new();
    chain.slots.push(slot);
    app.effect_chains.insert(chain_id, chain);

    let group_of = app.build_group_membership();
    let plugins = app.collect_chain_plugins(wf_id, Some(chain_id), &group_of);

    let latency = App::collect_chain_latency(&plugins);
    assert_eq!(latency, 0);
    assert_eq!(plugins.len(), 1);
}

/// collect_clip_chain_plugins returns only the entity's own chain, not the group's.
#[test]
fn collect_clip_chain_plugins_excludes_group_chain() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let wf_chain_id = new_id();
    let group_chain_id = new_id();
    let group_id = new_id();

    let mut wf = make_waveform(100.0, 100.0);
    wf.effect_chain_id = Some(wf_chain_id);
    app.waveforms.insert(wf_id, wf);

    let clip = AudioClipData {
        samples: Arc::new(vec![0.0; 44100]),
        sample_rate: 44100,
        duration_secs: 1.0,
    };
    app.audio_clips.insert(wf_id, clip);

    // Waveform's own chain: 1 slot
    let mut wf_chain = EffectChain::new();
    wf_chain.slots.push(EffectChainSlot::new(
        "clip_uid".to_string(), "Clip FX".to_string(), PathBuf::from("/clip.vst3"),
    ));
    app.effect_chains.insert(wf_chain_id, wf_chain);

    // Group chain: 1 slot
    let mut group_chain = EffectChain::new();
    group_chain.slots.push(EffectChainSlot::new(
        "group_uid".to_string(), "Group FX".to_string(), PathBuf::from("/group.vst3"),
    ));
    app.effect_chains.insert(group_chain_id, group_chain);

    app.groups.insert(group_id, crate::group::Group {
        id: group_id,
        name: "Test Group".to_string(),
        position: [0.0, 0.0],
        size: [400.0, 200.0],
        member_ids: vec![wf_id],
        effect_chain_id: Some(group_chain_id),
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });

    // collect_chain_plugins (old path, used by instruments) includes group
    let group_of = app.build_group_membership();
    let full = app.collect_chain_plugins(wf_id, Some(wf_chain_id), &group_of);
    assert_eq!(full.len(), 2);

    // collect_clip_chain_plugins returns only the clip's own chain
    let clip_only = app.collect_clip_chain_plugins(Some(wf_chain_id));
    assert_eq!(clip_only.len(), 1);

    // collect_group_chain_plugins returns only the group's chain
    let group_only = app.collect_group_chain_plugins(group_id);
    assert_eq!(group_only.len(), 1);
}

/// Grouped waveform without its own chain still gets a group_bus_index
/// (handled by sync_audio_clips routing).
#[test]
fn grouped_waveform_without_own_chain_gets_group_bus() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let group_chain_id = new_id();
    let group_id = new_id();

    let wf = make_waveform(100.0, 100.0);
    app.waveforms.insert(wf_id, wf);

    let clip = AudioClipData {
        samples: Arc::new(vec![0.0; 44100]),
        sample_rate: 44100,
        duration_secs: 1.0,
    };
    app.audio_clips.insert(wf_id, clip);

    let mut group_chain = EffectChain::new();
    group_chain.slots.push(EffectChainSlot::new(
        "group_uid".to_string(), "Group FX".to_string(), PathBuf::from("/group.vst3"),
    ));
    app.effect_chains.insert(group_chain_id, group_chain);

    app.groups.insert(group_id, crate::group::Group {
        id: group_id,
        name: "Test Group".to_string(),
        position: [0.0, 0.0],
        size: [400.0, 200.0],
        member_ids: vec![wf_id],
        effect_chain_id: Some(group_chain_id),
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });

    // clip has no own chain → collect_clip_chain_plugins returns empty
    let clip_plugins = app.collect_clip_chain_plugins(None);
    assert_eq!(clip_plugins.len(), 0);

    // But group has a chain, so collect_group_chain_plugins returns 1
    let group_plugins = app.collect_group_chain_plugins(group_id);
    assert_eq!(group_plugins.len(), 1);
}

/// Bypassed group chain slots are excluded from group bus plugins.
#[test]
fn bypassed_group_chain_slots_excluded() {
    let mut app = App::new_headless();
    let group_chain_id = new_id();
    let group_id = new_id();

    let mut group_chain = EffectChain::new();
    let mut slot = EffectChainSlot::new(
        "group_uid".to_string(), "Group FX".to_string(), PathBuf::from("/group.vst3"),
    );
    slot.bypass = true;
    group_chain.slots.push(slot);
    app.effect_chains.insert(group_chain_id, group_chain);

    app.groups.insert(group_id, crate::group::Group {
        id: group_id,
        name: "Test Group".to_string(),
        position: [0.0, 0.0],
        size: [400.0, 200.0],
        member_ids: vec![],
        effect_chain_id: Some(group_chain_id),
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });

    let group_plugins = app.collect_group_chain_plugins(group_id);
    assert_eq!(group_plugins.len(), 0);
}
