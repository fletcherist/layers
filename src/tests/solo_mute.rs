use std::sync::Arc;
use crate::entity_id::new_id;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::automation::AutomationData;
use crate::{App, HitTarget};
use crate::ui::palette::CommandAction;

fn make_waveform(x: f32, y: f32) -> WaveformView {
    WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 48000,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [x, y],
        size: [200.0, 80.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 4.0,
        fade_in_px: 0.0,
        fade_out_px: 0.0,
        fade_in_curve: 0.5,
        fade_out_curve: 0.5,
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

#[test]
fn toggle_mute() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(0.0, 0.0));

    assert!(app.should_play(id));

    app.toggle_mute_disabled(id);
    assert!(!app.should_play(id));

    app.toggle_mute_disabled(id);
    assert!(app.should_play(id));
}

#[test]
fn toggle_solo_exclusive() {
    let mut app = App::new_headless();
    let a = new_id();
    let b = new_id();
    app.waveforms.insert(a, make_waveform(0.0, 0.0));
    app.waveforms.insert(b, make_waveform(300.0, 0.0));

    // Solo A exclusively
    app.toggle_solo(a, false);
    assert!(app.should_play(a));
    assert!(!app.should_play(b));

    // Solo B exclusively — should replace A
    app.toggle_solo(b, false);
    assert!(!app.should_play(a));
    assert!(app.should_play(b));

    // Click solo on B again — should clear all solos
    app.toggle_solo(b, false);
    assert!(app.should_play(a));
    assert!(app.should_play(b));
    assert!(app.solo_ids.is_empty());
}

#[test]
fn toggle_solo_additive() {
    let mut app = App::new_headless();
    let a = new_id();
    let b = new_id();
    let c = new_id();
    app.waveforms.insert(a, make_waveform(0.0, 0.0));
    app.waveforms.insert(b, make_waveform(300.0, 0.0));
    app.waveforms.insert(c, make_waveform(600.0, 0.0));

    // Solo A with shift
    app.toggle_solo(a, true);
    assert!(app.should_play(a));
    assert!(!app.should_play(b));

    // Add B to solo with shift
    app.toggle_solo(b, true);
    assert!(app.should_play(a));
    assert!(app.should_play(b));
    assert!(!app.should_play(c));
}

#[test]
fn mute_overrides_solo() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(0.0, 0.0));

    app.toggle_solo(id, false);
    assert!(app.should_play(id));

    app.toggle_mute_disabled(id);
    assert!(!app.should_play(id));
}

#[test]
fn group_mute_affects_members() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));
    app.selected = vec![HitTarget::Waveform(wf1), HitTarget::Waveform(wf2)];
    app.execute_command(CommandAction::CreateGroup);

    let group_id = *app.groups.keys().next().unwrap();

    assert!(app.should_play(wf1));
    assert!(app.should_play(wf2));

    app.toggle_mute_disabled(group_id);
    assert!(!app.should_play(wf1));
    assert!(!app.should_play(wf2));
}

#[test]
fn group_solo_includes_members() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    let wf3 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));
    app.waveforms.insert(wf3, make_waveform(700.0, 100.0));
    app.selected = vec![HitTarget::Waveform(wf1), HitTarget::Waveform(wf2)];
    app.execute_command(CommandAction::CreateGroup);

    let group_id = *app.groups.keys().next().unwrap();

    // Solo the group
    app.toggle_solo(group_id, false);
    assert!(app.should_play(wf1), "member of soloed group should play");
    assert!(app.should_play(wf2), "member of soloed group should play");
    assert!(!app.should_play(wf3), "non-member should not play when group is soloed");
}

#[test]
fn no_solo_all_play() {
    let mut app = App::new_headless();
    let a = new_id();
    let b = new_id();
    app.waveforms.insert(a, make_waveform(0.0, 0.0));
    app.waveforms.insert(b, make_waveform(300.0, 0.0));

    assert!(app.solo_ids.is_empty());
    assert!(app.should_play(a));
    assert!(app.should_play(b));
}

#[test]
fn solo_member_of_soloed_group() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));
    app.selected = vec![HitTarget::Waveform(wf1), HitTarget::Waveform(wf2)];
    app.execute_command(CommandAction::CreateGroup);

    let group_id = *app.groups.keys().next().unwrap();

    // Solo just one member (exclusive) — group is NOT soloed, so other member should not play
    app.toggle_solo(wf1, false);
    assert!(app.should_play(wf1), "directly soloed member should play");
    assert!(!app.should_play(wf2), "non-soloed member should not play when solo is active");
    assert!(!app.solo_ids.contains(&group_id), "group itself should not be soloed");
}

#[test]
fn mute_member_of_soloed_group() {
    let mut app = App::new_headless();
    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.waveforms.insert(wf2, make_waveform(400.0, 100.0));
    app.selected = vec![HitTarget::Waveform(wf1), HitTarget::Waveform(wf2)];
    app.execute_command(CommandAction::CreateGroup);

    let group_id = *app.groups.keys().next().unwrap();

    // Solo the group, then mute one member — mute should override
    app.toggle_solo(group_id, false);
    assert!(app.should_play(wf1));
    assert!(app.should_play(wf2));

    app.toggle_mute_disabled(wf1);
    assert!(!app.should_play(wf1), "muted member should not play even in soloed group");
    assert!(app.should_play(wf2), "unmuted member of soloed group should still play");
}
