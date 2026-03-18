use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::operations::Operation;
use crate::ui::palette::db_to_gain;
use crate::ui::right_window::RightWindow;
use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView};
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
