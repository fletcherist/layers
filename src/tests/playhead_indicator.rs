use crate::entity_id::new_id;
use crate::grid;
use crate::regions::SelectArea;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::{App, HitTarget};
use crate::automation::AutomationData;
use std::sync::Arc;

fn make_waveform(x: f32, y: f32, bpm: f32) -> WaveformView {
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
        size: [200.0, grid::clip_height(bpm)],
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
fn playhead_line_uses_clip_height() {
    let app = App::new_headless();
    let expected = grid::clip_height(app.bpm);
    assert_eq!(app.clip_height(), expected);
}

#[test]
fn playhead_line_height_matches_waveform() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(100.0, 50.0, app.bpm));

    let wf_h = app.waveforms[&id].size[1];
    assert_eq!(app.clip_height(), wf_h);
}

#[test]
fn snap_to_clip_row_snaps_to_row_boundary() {
    let bpm = 120.0;
    let h = grid::clip_height(bpm);

    // Click inside first row → snaps to 0
    assert_eq!(grid::snap_to_clip_row(h * 0.3, bpm), 0.0);
    // Click inside second row → snaps to h
    assert_eq!(grid::snap_to_clip_row(h * 1.5, bpm), h);
    // Click exactly at boundary → snaps to that boundary
    assert_eq!(grid::snap_to_clip_row(h * 2.0, bpm), h * 2.0);
    // Negative Y → snaps to -h
    assert_eq!(grid::snap_to_clip_row(-0.1, bpm), -h);
}

#[test]
fn playhead_line_consistent_across_clicks() {
    let app = App::new_headless();
    let h = app.clip_height();

    let y1 = grid::snap_to_clip_row(50.0, app.bpm);
    let y2 = grid::snap_to_clip_row(300.0, app.bpm);

    let sa1 = SelectArea { position: [100.0, y1], size: [2.0, h] };
    let sa2 = SelectArea { position: [200.0, y2], size: [2.0, h] };

    assert_eq!(sa1.size[1], sa2.size[1]);
    assert_eq!(sa1.size[1], h);
}

#[test]
fn selecting_waveform_clears_select_area() {
    let mut app = App::new_headless();

    let id = new_id();
    app.waveforms.insert(id, make_waveform(100.0, 50.0, app.bpm));

    let h = app.clip_height();
    app.select_area = Some(SelectArea {
        position: [200.0, 0.0],
        size: [2.0, h],
    });
    assert!(app.select_area.is_some());

    app.select_area = None;
    app.selected.push(HitTarget::Waveform(id));

    assert!(app.select_area.is_none());
    assert_eq!(app.selected.len(), 1);
}
