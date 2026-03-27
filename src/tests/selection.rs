use std::sync::Arc;

use crate::entity_id::new_id;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::automation::AutomationData;
use crate::{App, HitTarget};
use crate::group::Group;

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
            paulstretch_factor: 8.0,
        is_reversed: false,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
        effect_chain_id: None,
        take_group: None,
    }
}

#[test]
fn shift_click_adds_to_selection() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();
    app.waveforms.insert(id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(id2, make_waveform(300.0, 0.0));

    // Select first normally
    app.select_with_shift(HitTarget::Waveform(id1), false);
    assert_eq!(app.selected.len(), 1);

    // Shift-click second — both should be selected
    app.select_with_shift(HitTarget::Waveform(id2), true);
    assert_eq!(app.selected.len(), 2);
    assert!(app.selected.contains(&HitTarget::Waveform(id1)));
    assert!(app.selected.contains(&HitTarget::Waveform(id2)));
}

#[test]
fn shift_click_deselects() {
    let mut app = App::new_headless();
    let id1 = new_id();
    app.waveforms.insert(id1, make_waveform(0.0, 0.0));

    // Select it
    app.select_with_shift(HitTarget::Waveform(id1), false);
    assert_eq!(app.selected.len(), 1);

    // Shift-click same item — should deselect
    let result = app.select_with_shift(HitTarget::Waveform(id1), true);
    assert!(!result);
    assert!(app.selected.is_empty());
}

#[test]
fn non_shift_click_clears_previous() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();
    app.waveforms.insert(id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(id2, make_waveform(300.0, 0.0));

    // Select both via shift
    app.select_with_shift(HitTarget::Waveform(id1), false);
    app.select_with_shift(HitTarget::Waveform(id2), true);
    assert_eq!(app.selected.len(), 2);

    // Non-shift click on a NEW item — should clear and select only that
    let id3 = new_id();
    app.waveforms.insert(id3, make_waveform(600.0, 0.0));
    app.select_with_shift(HitTarget::Waveform(id3), false);
    assert_eq!(app.selected.len(), 1);
    assert_eq!(app.selected[0], HitTarget::Waveform(id3));
}

#[test]
fn loop_from_selected_waveform_matches_width() {
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform(100.0, 50.0);
    wf.size = [400.0, 80.0];
    app.waveforms.insert(id, wf);

    // Select the waveform, then create a loop
    app.selected.clear();
    app.selected.push(HitTarget::Waveform(id));
    app.execute_command(CommandAction::AddLoopArea);

    assert_eq!(app.loop_regions.len(), 1);
    let lr = app.loop_regions.values().next().unwrap();
    // Loop width should match the waveform width (possibly snapped, but close)
    assert!((lr.size[0] - 400.0).abs() < 50.0, "loop width {} should be close to 400", lr.size[0]);
    assert!((lr.position[0] - 100.0).abs() < 50.0, "loop x {} should be close to 100", lr.position[0]);
    // Selection should remain on the waveform, not switch to loop
    assert_eq!(app.selected, vec![HitTarget::Waveform(id)]);
}

#[test]
fn loop_from_selected_group_matches_width() {
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();
    let gid = new_id();
    let group = Group::new(gid, "G".into(), [200.0, 30.0], [600.0, 120.0], vec![]);
    app.groups.insert(gid, group);

    app.selected.clear();
    app.selected.push(HitTarget::Group(gid));
    app.execute_command(CommandAction::AddLoopArea);

    assert_eq!(app.loop_regions.len(), 1);
    let lr = app.loop_regions.values().next().unwrap();
    assert!((lr.size[0] - 600.0).abs() < 50.0, "loop width {} should be close to 600", lr.size[0]);
    assert!((lr.position[0] - 200.0).abs() < 50.0, "loop x {} should be close to 200", lr.position[0]);
}
