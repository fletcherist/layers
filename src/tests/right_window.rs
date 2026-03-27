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
        is_reversed: false,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
    effect_chain_id: None,
    take_group: None,
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
    assert_eq!(rw.target_id(), id);
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
    let wf_id = rw.target_id();

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

#[test]
fn test_pan_keyboard_adjust() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.pan = 0.5;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    // Focus the pan knob
    app.right_window.as_mut().unwrap().pan_knob_focused = true;

    // Simulate Up arrow: +0.01
    let rw = app.right_window.as_ref().unwrap();
    let new_pan = (rw.pan + 0.01).clamp(0.0, 1.0);
    let wf_id = rw.target_id();

    let before = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().pan = new_pan;
    app.right_window.as_mut().unwrap().pan = new_pan;
    let after = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before, after });

    assert!((app.waveforms[&wf_id].pan - 0.51).abs() < 1e-4, "pan should be 0.51 after Up");

    // Undo
    app.undo_op();
    assert!((app.waveforms[&wf_id].pan - 0.5).abs() < 1e-4, "undo should restore pan to 0.5");
}

#[test]
fn test_undo_volume_preserves_selection() {
    use crate::ui::palette::db_to_gain;

    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.volume = 1.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(app.right_window.is_some());

    // Apply volume change (+1 dB)
    let new_gain = db_to_gain(1.0);
    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().volume = new_gain;
    app.right_window.as_mut().unwrap().volume = new_gain;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    // Undo — selection and right window should be preserved
    app.undo_op();
    assert!((app.waveforms[&id].volume - 1.0).abs() < 1e-4, "volume should be restored");
    assert!(!app.selected.is_empty(), "selection should be preserved after undo");
    assert!(app.right_window.is_some(), "right_window should stay open after undo");
}

#[test]
fn test_undo_pan_preserves_selection() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.pan = 0.5;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(app.right_window.is_some());

    // Apply pan change
    let new_pan = 0.7;
    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().pan = new_pan;
    app.right_window.as_mut().unwrap().pan = new_pan;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    // Undo — selection and right window should be preserved
    app.undo_op();
    assert!((app.waveforms[&id].pan - 0.5).abs() < 1e-4, "pan should be restored");
    assert!(!app.selected.is_empty(), "selection should be preserved after undo");
    assert!(app.right_window.is_some(), "right_window should stay open after undo");

    // Redo — should also preserve selection
    app.redo_op();
    assert!((app.waveforms[&id].pan - 0.7).abs() < 1e-4, "pan should be re-applied");
    assert!(!app.selected.is_empty(), "selection should be preserved after redo");
    assert!(app.right_window.is_some(), "right_window should stay open after redo");
}

#[test]
fn test_pitch_keyboard_adjust() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::Semitone;
    wf.pitch_semitones = 0.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    // Focus the pitch control
    app.right_window.as_mut().unwrap().pitch_focused = true;

    // Simulate Up arrow: +1 semitone
    let rw = app.right_window.as_ref().unwrap();
    let new_pitch = (rw.pitch_semitones + 1.0).clamp(-24.0, 24.0);
    let wf_id = rw.target_id();

    let before = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().pitch_semitones = new_pitch;
    app.right_window.as_mut().unwrap().pitch_semitones = new_pitch;
    let after = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before, after });
    app.resize_warped_clips();

    assert!((app.waveforms[&wf_id].pitch_semitones - 1.0).abs() < 1e-4, "pitch should be +1 st after Up");

    // Simulate Down arrow: -1 semitone (back to 0)
    let new_pitch2 = (app.right_window.as_ref().unwrap().pitch_semitones - 1.0).clamp(-24.0, 24.0);
    let before2 = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().pitch_semitones = new_pitch2;
    app.right_window.as_mut().unwrap().pitch_semitones = new_pitch2;
    let after2 = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before: before2, after: after2 });
    app.resize_warped_clips();

    assert!((app.waveforms[&wf_id].pitch_semitones - 0.0).abs() < 1e-4, "pitch should be 0 st after Down");

    // Undo should restore to +1
    app.undo_op();
    assert!((app.waveforms[&wf_id].pitch_semitones - 1.0).abs() < 1e-4, "undo should restore pitch to +1 st");

    // Undo again should restore to 0
    app.undo_op();
    assert!((app.waveforms[&wf_id].pitch_semitones - 0.0).abs() < 1e-4, "undo should restore pitch to 0 st");
}

#[test]
fn test_undo_pitch_preserves_selection() {
    let mut app = App::new_headless();
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::Semitone;
    wf.pitch_semitones = 0.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(app.right_window.is_some());

    // Apply pitch change (+3 semitones)
    let new_pitch = 3.0;
    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().pitch_semitones = new_pitch;
    app.right_window.as_mut().unwrap().pitch_semitones = new_pitch;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    // Undo — selection and right window should be preserved
    app.undo_op();
    assert!((app.waveforms[&id].pitch_semitones - 0.0).abs() < 1e-4, "pitch should be restored");
    assert!(!app.selected.is_empty(), "selection should be preserved after undo");
    assert!(app.right_window.is_some(), "right_window should stay open after undo");
}

#[test]
fn test_sample_bpm_keyboard_adjust() {
    let mut app = App::new_headless();
    app.bpm = 120.0;
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::RePitch;
    wf.sample_bpm = 120.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    // Focus the sample BPM control
    app.right_window.as_mut().unwrap().sample_bpm_focused = true;

    // Simulate Up arrow: +1 BPM
    let rw = app.right_window.as_ref().unwrap();
    let new_bpm = (rw.sample_bpm + 1.0).clamp(20.0, 999.0);
    let wf_id = rw.target_id();

    let before = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().sample_bpm = new_bpm;
    app.right_window.as_mut().unwrap().sample_bpm = new_bpm;
    let after = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before, after });
    app.resize_warped_clips();

    assert!((app.waveforms[&wf_id].sample_bpm - 121.0).abs() < 1e-4, "sample_bpm should be 121 after Up");

    // Simulate Down arrow: -1 BPM (back to 120)
    let new_bpm2 = (app.right_window.as_ref().unwrap().sample_bpm - 1.0).clamp(20.0, 999.0);
    let before2 = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().sample_bpm = new_bpm2;
    app.right_window.as_mut().unwrap().sample_bpm = new_bpm2;
    let after2 = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before: before2, after: after2 });
    app.resize_warped_clips();

    assert!((app.waveforms[&wf_id].sample_bpm - 120.0).abs() < 1e-4, "sample_bpm should be 120 after Down");

    // Undo should restore to 121
    app.undo_op();
    assert!((app.waveforms[&wf_id].sample_bpm - 121.0).abs() < 1e-4, "undo should restore sample_bpm to 121");

    // Undo again should restore to 120
    app.undo_op();
    assert!((app.waveforms[&wf_id].sample_bpm - 120.0).abs() < 1e-4, "undo should restore sample_bpm to 120");
}

#[test]
fn test_undo_sample_bpm_preserves_selection() {
    let mut app = App::new_headless();
    app.bpm = 120.0;
    let id = new_id();
    let mut wf = make_waveform();
    wf.warp_mode = WarpMode::RePitch;
    wf.sample_bpm = 120.0;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(app.right_window.is_some());

    // Apply sample_bpm change
    let new_bpm = 130.0;
    let before = app.waveforms[&id].clone();
    app.waveforms.get_mut(&id).unwrap().sample_bpm = new_bpm;
    app.right_window.as_mut().unwrap().sample_bpm = new_bpm;
    let after = app.waveforms[&id].clone();
    app.push_op(Operation::UpdateWaveform { id, before, after });

    // Undo — selection and right window should be preserved
    app.undo_op();
    assert!((app.waveforms[&id].sample_bpm - 120.0).abs() < 1e-4, "sample_bpm should be restored");
    assert!(!app.selected.is_empty(), "selection should be preserved after undo");
    assert!(app.right_window.is_some(), "right_window should stay open after undo");
}

#[test]
fn test_reverse_sample_toggles_is_reversed() {
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();
    let id = new_id();

    let samples = vec![1.0f32, 2.0, 3.0, 4.0];
    let audio = Arc::new(AudioData {
        left_samples: Arc::new(samples.clone()),
        right_samples: Arc::new(samples.clone()),
        left_peaks: Arc::new(WaveformPeaks::empty()),
        right_peaks: Arc::new(WaveformPeaks::empty()),
        sample_rate: 48000,
        filename: "test.wav".to_string(),
    });
    let mut wf = make_waveform();
    wf.audio = audio;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(samples),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));

    assert!(!app.waveforms[&id].is_reversed);

    // First reverse: samples flip, is_reversed = true
    app.execute_command(CommandAction::ReverseSample);
    assert!(app.waveforms[&id].is_reversed, "is_reversed should be true after first reverse");
    assert_eq!(*app.waveforms[&id].audio.left_samples, vec![4.0, 3.0, 2.0, 1.0]);

    // Second reverse: samples flip back, is_reversed = false
    app.execute_command(CommandAction::ReverseSample);
    assert!(!app.waveforms[&id].is_reversed, "is_reversed should be false after second reverse");
    assert_eq!(*app.waveforms[&id].audio.left_samples, vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn test_reverse_reflects_in_right_window() {
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();
    let id = new_id();

    let samples = vec![1.0f32, 2.0, 3.0];
    let audio = Arc::new(AudioData {
        left_samples: Arc::new(samples.clone()),
        right_samples: Arc::new(samples.clone()),
        left_peaks: Arc::new(WaveformPeaks::empty()),
        right_peaks: Arc::new(WaveformPeaks::empty()),
        sample_rate: 48000,
        filename: "test.wav".to_string(),
    });
    let mut wf = make_waveform();
    wf.audio = audio;
    app.waveforms.insert(id, wf);
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(samples),
        sample_rate: 48000,
        duration_secs: 1.0,
    });

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();
    assert!(!app.right_window.as_ref().unwrap().is_reversed);

    app.execute_command(CommandAction::ReverseSample);
    app.update_right_window();
    assert!(app.right_window.as_ref().unwrap().is_reversed, "right window should reflect reversed state");
}

#[test]
fn test_multi_select_updates_right_window() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();
    let id3 = new_id();

    let mut wf1 = make_waveform();
    wf1.volume = 0.8;
    wf1.pan = 0.3;
    let mut wf2 = make_waveform();
    wf2.volume = 1.2;
    wf2.pan = 0.7;
    let wf3 = make_waveform();

    app.waveforms.insert(id1, wf1);
    app.waveforms.insert(id2, wf2);
    app.waveforms.insert(id3, wf3);

    // Select all three
    app.selected.push(HitTarget::Waveform(id1));
    app.selected.push(HitTarget::Waveform(id2));
    app.selected.push(HitTarget::Waveform(id3));
    app.update_right_window();

    let rw = app.right_window.as_ref().unwrap();
    assert_eq!(rw.multi_target_ids.len(), 3, "should have 3 multi target ids");
    assert!(rw.is_multi(), "should be multi-selection");
    // Volume/pan should come from first selected waveform
    assert!((rw.volume - 0.8).abs() < 1e-4, "volume should be from first waveform");
    assert!((rw.pan - 0.3).abs() < 1e-4, "pan should be from first waveform");
}

#[test]
fn test_multi_select_volume_relative_batch_undo() {
    use crate::ui::palette::gain_to_db;

    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();

    // wf1 at -3 dB, wf2 at 0 dB
    let mut wf1 = make_waveform();
    wf1.volume = db_to_gain(-3.0);
    let mut wf2 = make_waveform();
    wf2.volume = 1.0; // 0 dB

    app.waveforms.insert(id1, wf1.clone());
    app.waveforms.insert(id2, wf2.clone());

    // Simulate relative +2 dB change: wf1 → -1 dB, wf2 → +2 dB
    let db_delta = 2.0;
    let before1 = app.waveforms[&id1].clone();
    let before2 = app.waveforms[&id2].clone();
    let wf1_new_db = gain_to_db(before1.volume) + db_delta; // -3 + 2 = -1
    let wf2_new_db = gain_to_db(before2.volume) + db_delta; // 0 + 2 = 2
    app.waveforms.get_mut(&id1).unwrap().volume = db_to_gain(wf1_new_db);
    app.waveforms.get_mut(&id2).unwrap().volume = db_to_gain(wf2_new_db);
    let after1 = app.waveforms[&id1].clone();
    let after2 = app.waveforms[&id2].clone();

    app.push_op(Operation::Batch(vec![
        Operation::UpdateWaveform { id: id1, before: before1, after: after1 },
        Operation::UpdateWaveform { id: id2, before: before2, after: after2 },
    ]));

    // Verify relative values maintained
    assert!((gain_to_db(app.waveforms[&id1].volume) - (-1.0)).abs() < 0.1, "wf1 should be -1 dB");
    assert!((gain_to_db(app.waveforms[&id2].volume) - 2.0).abs() < 0.1, "wf2 should be +2 dB");

    // Undo should restore both to original
    app.undo_op();
    assert!((gain_to_db(app.waveforms[&id1].volume) - (-3.0)).abs() < 0.1, "undo should restore wf1 to -3 dB");
    assert!((gain_to_db(app.waveforms[&id2].volume) - 0.0).abs() < 0.1, "undo should restore wf2 to 0 dB");
}

#[test]
fn test_multi_select_pan_relative_batch_undo() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();

    // wf1 panned left (0.3), wf2 panned right (0.7)
    let mut wf1 = make_waveform();
    wf1.pan = 0.3;
    let mut wf2 = make_waveform();
    wf2.pan = 0.7;

    app.waveforms.insert(id1, wf1);
    app.waveforms.insert(id2, wf2);

    // Simulate relative pan shift of +0.1: wf1 → 0.4, wf2 → 0.8
    let pan_delta = 0.1;
    let before1 = app.waveforms[&id1].clone();
    let before2 = app.waveforms[&id2].clone();
    app.waveforms.get_mut(&id1).unwrap().pan = (before1.pan + pan_delta).clamp(0.0, 1.0);
    app.waveforms.get_mut(&id2).unwrap().pan = (before2.pan + pan_delta).clamp(0.0, 1.0);
    let after1 = app.waveforms[&id1].clone();
    let after2 = app.waveforms[&id2].clone();

    app.push_op(Operation::Batch(vec![
        Operation::UpdateWaveform { id: id1, before: before1, after: after1 },
        Operation::UpdateWaveform { id: id2, before: before2, after: after2 },
    ]));

    // Verify relative values maintained
    assert!((app.waveforms[&id1].pan - 0.4).abs() < 1e-4, "wf1 should be 0.4");
    assert!((app.waveforms[&id2].pan - 0.8).abs() < 1e-4, "wf2 should be 0.8");

    // Undo should restore both
    app.undo_op();
    assert!((app.waveforms[&id1].pan - 0.3).abs() < 1e-4, "undo should restore wf1 pan to 0.3");
    assert!((app.waveforms[&id2].pan - 0.7).abs() < 1e-4, "undo should restore wf2 pan to 0.7");
}

#[test]
fn test_single_select_still_works() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform());

    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    let rw = app.right_window.as_ref().unwrap();
    assert_eq!(rw.multi_target_ids.len(), 1);
    assert!(!rw.is_multi(), "single selection should not be multi");
    assert_eq!(rw.target_id(), id);
}

#[test]
fn test_rms_meter_smoothing() {
    let mut app = App::new_headless();
    let id = new_id();
    let wf = make_waveform();
    app.waveforms.insert(id, wf);
    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    let rw = app.right_window.as_mut().unwrap();
    assert_eq!(rw.meter_rms, 0.0);
    assert_eq!(rw.meter_peak, 0.0);

    // Feed a loud signal — meter should rise quickly
    rw.update_rms(0.8);
    assert!(rw.meter_rms > 0.0, "meter should respond to input");
    assert!(rw.meter_peak >= 0.8, "peak should capture the maximum");

    // Feed several more frames to converge
    for _ in 0..100 {
        rw.update_rms(0.8);
    }
    assert!((rw.meter_rms - 0.8).abs() < 0.05, "meter should converge near input");

    // Drop to silence — meter should decay but not instantly
    rw.update_rms(0.0);
    assert!(rw.meter_rms > 0.1, "meter should decay slowly, not drop instantly");
    assert!(rw.meter_peak >= 0.75, "peak should hold");

    // After many silent frames, meter should approach zero
    for _ in 0..300 {
        rw.update_rms(0.0);
    }
    assert!(rw.meter_rms < 0.01, "meter should decay near zero after silence");
}

#[test]
fn test_rms_meter_peak_hold_decay() {
    let mut app = App::new_headless();
    let id = new_id();
    let wf = make_waveform();
    app.waveforms.insert(id, wf);
    app.selected.push(HitTarget::Waveform(id));
    app.update_right_window();

    let rw = app.right_window.as_mut().unwrap();

    // Hit a transient
    rw.update_rms(0.9);
    let peak_after_hit = rw.meter_peak;
    assert!(peak_after_hit >= 0.9);

    // Silence for 80 frames (~1.3s at 60fps) — peak should hold
    for _ in 0..80 {
        rw.update_rms(0.0);
    }
    assert!(rw.meter_peak >= 0.85, "peak should hold for ~1.5s");

    // After enough frames past hold time, peak should start decaying
    for _ in 0..200 {
        rw.update_rms(0.0);
    }
    assert!(rw.meter_peak < 0.5, "peak should decay after hold period");
}
