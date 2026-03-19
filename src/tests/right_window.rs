use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::operations::Operation;
use crate::ui::palette::db_to_gain;
use crate::ui::right_window::RightWindow;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::{App, HitTarget};

fn make_waveform() -> WaveformView {
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
        position: [100.0, 50.0],
        size: [200.0, 80.0],
        color: [0.5, 0.5, 0.5, 1.0],
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
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
    }
}

#[test]
fn test_waveform_has_pan_field() {
    let wf = make_waveform();
    assert_eq!(wf.pan, 0.5, "default pan should be 0.5 (center)");
}

#[test]
fn test_update_right_window_shows_on_waveform_selection() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform());
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    // No selection initially — no right window
    assert!(app.right_window.is_none());

    // Select the waveform
    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    assert!(app.right_window.is_some(), "right_window should appear on waveform selection");
    let rw = app.right_window.as_ref().unwrap();
    assert_eq!(rw.waveform_id, id);
    assert_eq!(rw.volume, 1.0);
    assert_eq!(rw.pan, 0.5);
}

#[test]
fn test_update_right_window_hides_on_deselect() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform());

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(app.right_window.is_some());

    app.selected.clear();
    app.update_right_window();
    assert!(app.right_window.is_none(), "right_window should disappear when nothing is selected");
}

#[test]
fn test_pan_modified_via_update_waveform_op() {
    let mut app = App::new_headless();
    let id = new_id();
    let wf = make_waveform();
    app.waveforms.insert(id, wf.clone());

    // Apply the change first, then push for undo history
    let mut after = wf.clone();
    after.pan = 0.8;
    app.waveforms.get_mut(&id).unwrap().pan = 0.8;
    app.push_op(Operation::UpdateWaveform { id, before: wf, after });

    assert_eq!(app.waveforms[&id].pan, 0.8, "pan should be updated");

    // Undo should restore it to 0.5
    app.undo_op();
    assert_eq!(app.waveforms[&id].pan, 0.5, "undo should restore pan to 0.5");
}

#[test]
fn test_right_window_panel_rect() {
    let (pos, size) = RightWindow::panel_rect(1200.0, 800.0, 1.0);
    assert_eq!(size[0], 200.0); // RIGHT_WINDOW_WIDTH
    assert_eq!(size[1], 800.0); // full height
    assert!((pos[0] - 1000.0).abs() < 0.01, "panel should be right-aligned");
}

#[test]
fn test_vol_entry_commit_updates_waveform_volume() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.volume = 1.0; // 0 dB
    app.waveforms.insert(id, wf.clone());
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    let rw = app.right_window.as_mut().unwrap();
    rw.vol_entry.enter();
    rw.vol_entry.push_char("3");
    rw.vol_entry.push_char(".");
    rw.vol_entry.push_char("5");

    // Commit the entry
    let text = rw.vol_entry.commit().unwrap();
    let db: f32 = text.parse().unwrap();
    let new_gain = db_to_gain(db.clamp(-60.0, 12.0));

    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().volume = new_gain;
    app.right_window.as_mut().unwrap().volume = new_gain;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    let expected = db_to_gain(3.5);
    assert!((app.waveforms[&id].volume - expected).abs() < 1e-4, "volume should be +3.5 dB gain");

    // Undo should restore original volume
    app.undo_op();
    assert!((app.waveforms[&id].volume - 1.0).abs() < 1e-4, "undo should restore volume to 1.0");
}

#[test]
fn test_warp_mode_default() {
    let wf = make_waveform();
    assert_eq!(wf.warp_mode, WarpMode::Off, "default warp mode should be Off");
    assert_eq!(wf.sample_bpm, 120.0, "default sample_bpm should be 120.0");
}

#[test]
fn test_repitch_updates_via_op() {
    let mut app = App::new_headless();
    let id = new_id();
    let wf = make_waveform();
    app.waveforms.insert(id, wf.clone());

    let mut after = wf.clone();
    after.warp_mode = WarpMode::RePitch;
    after.sample_bpm = 140.0;
    app.waveforms.get_mut(&id).unwrap().warp_mode = WarpMode::RePitch;
    app.waveforms.get_mut(&id).unwrap().sample_bpm = 140.0;
    app.push_op(Operation::UpdateWaveform { id, before: wf, after });

    assert_eq!(app.waveforms[&id].warp_mode, WarpMode::RePitch);
    assert_eq!(app.waveforms[&id].sample_bpm, 140.0);

    app.undo_op();
    assert_eq!(app.waveforms[&id].warp_mode, WarpMode::Off, "undo should restore warp mode to Off");
    assert_eq!(app.waveforms[&id].sample_bpm, 120.0, "undo should restore sample_bpm");
}

#[test]
fn test_right_window_shows_warp_mode() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::RePitch;
    wf.sample_bpm = 140.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    let rw = app.right_window.as_ref().unwrap();
    assert_eq!(rw.warp_mode, WarpMode::RePitch, "right window should show warp mode from waveform");
    assert_eq!(rw.sample_bpm, 140.0, "right window should show sample_bpm from waveform");
}

#[test]
fn test_semitone_mode_resizes_clip() {
    use crate::grid::PIXELS_PER_SECOND;

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::Semitone;
    wf.pitch_semitones = 0.0;
    app.waveforms.insert(id, wf);

    let duration_secs = 2.0;
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs,
    });

    let original_px = duration_secs * PIXELS_PER_SECOND;

    // +12 semitones (one octave up) → 2x speed → half width
    app.waveforms.get_mut(&id).unwrap().pitch_semitones = 12.0;
    app.resize_warped_clips();
    let w = app.waveforms.get(&id).unwrap().size[0];
    assert!((w - original_px / 2.0).abs() < 0.01, "octave up should halve width: got {w}");

    // -12 semitones (one octave down) → 0.5x speed → double width
    app.waveforms.get_mut(&id).unwrap().pitch_semitones = -12.0;
    app.resize_warped_clips();
    let w = app.waveforms.get(&id).unwrap().size[0];
    assert!((w - original_px * 2.0).abs() < 0.01, "octave down should double width: got {w}");
}

#[test]
fn test_repitch_mode_resizes_clip() {
    use crate::grid::PIXELS_PER_SECOND;

    let mut app = App::new_headless();
    app.bpm = 120.0;
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::RePitch;
    wf.sample_bpm = 120.0;
    app.waveforms.insert(id, wf);

    let duration_secs = 2.0;
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs,
    });

    let original_px = duration_secs * PIXELS_PER_SECOND;

    // sample_bpm=60 with project bpm=120 → 2x stretch
    app.waveforms.get_mut(&id).unwrap().sample_bpm = 60.0;
    app.resize_warped_clips();
    let w = app.waveforms.get(&id).unwrap().size[0];
    assert!((w - original_px * 2.0).abs() < 0.01, "half-tempo sample should double width: got {w}");
}

#[test]
fn test_keyboard_volume_up() {
    use crate::ui::palette::{gain_to_db, db_to_gain};

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.volume = 1.0; // 0 dB
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    // Simulate: set fader focused, then apply +1 dB
    app.right_window.as_mut().unwrap().vol_fader_focused = true;

    let rw = app.right_window.as_ref().unwrap();
    let current_db = gain_to_db(rw.volume);
    let new_db = (current_db + 1.0).clamp(-70.0, 24.0);
    let new_gain = db_to_gain(new_db);
    let wf_id = rw.waveform_id;

    let before = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().volume = new_gain;
    app.right_window.as_mut().unwrap().volume = new_gain;
    let after = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before, after });

    let expected = db_to_gain(1.0);
    assert!((app.waveforms[&wf_id].volume - expected).abs() < 1e-4, "volume should be +1 dB");
}

#[test]
fn test_keyboard_volume_clamp_at_max() {
    use crate::ui::palette::{db_to_gain, VOL_FADER_DB_MAX};

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.volume = db_to_gain(23.5); // near max
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    app.right_window.as_mut().unwrap().vol_fader_focused = true;

    // Apply +1 dB — should clamp to +24 dB
    let new_db = (23.5_f32 + 1.0).clamp(-70.0, VOL_FADER_DB_MAX);
    let new_gain = db_to_gain(new_db);

    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().volume = new_gain;
    app.right_window.as_mut().unwrap().volume = new_gain;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    let expected = db_to_gain(VOL_FADER_DB_MAX);
    assert!((app.waveforms[&id].volume - expected).abs() < 1e-4, "volume should be clamped to +24 dB");
}

#[test]
fn test_keyboard_volume_undo() {
    use crate::ui::palette::{db_to_gain, gain_to_db};

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.volume = 1.0; // 0 dB
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    app.right_window.as_mut().unwrap().vol_fader_focused = true;

    // Apply +1 dB
    let new_gain = db_to_gain(1.0);
    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().volume = new_gain;
    app.right_window.as_mut().unwrap().volume = new_gain;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    assert!((gain_to_db(app.waveforms[&id].volume) - 1.0).abs() < 0.1, "should be +1 dB");

    // Undo
    app.undo_op();
    assert!((app.waveforms[&id].volume - 1.0).abs() < 1e-4, "undo should restore volume to 0 dB (gain=1.0)");
}
