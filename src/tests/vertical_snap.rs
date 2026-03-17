use crate::grid::{snap_to_vertical_grid, grid_spacing_for_settings};
use crate::settings::Settings;

#[test]
fn snap_to_vertical_grid_uses_musical_grid_spacing() {
    let mut settings = Settings::default();
    settings.snap_to_vertical_grid = true;
    settings.grid_enabled = true;
    settings.snap_to_grid = true;

    let zoom = 1.0;
    let bpm = 120.0;
    let spacing = grid_spacing_for_settings(&settings, zoom, bpm);

    assert_eq!(snap_to_vertical_grid(0.0, &settings, zoom, bpm), 0.0);
    assert_eq!(
        snap_to_vertical_grid(spacing * 0.4, &settings, zoom, bpm),
        0.0
    );
    assert_eq!(
        snap_to_vertical_grid(spacing * 0.6, &settings, zoom, bpm),
        spacing
    );
    assert_eq!(
        snap_to_vertical_grid(spacing * 1.3, &settings, zoom, bpm),
        spacing
    );
    assert_eq!(
        snap_to_vertical_grid(spacing * 1.6, &settings, zoom, bpm),
        spacing * 2.0
    );
    assert_eq!(
        snap_to_vertical_grid(-spacing * 0.6, &settings, zoom, bpm),
        -spacing
    );
}

#[test]
fn snap_to_vertical_grid_disabled_passthrough() {
    let mut settings = Settings::default();
    settings.snap_to_vertical_grid = false;

    assert_eq!(snap_to_vertical_grid(77.0, &settings, 1.0, 120.0), 77.0);
    assert_eq!(snap_to_vertical_grid(123.456, &settings, 1.0, 120.0), 123.456);
}

#[test]
fn snap_to_vertical_grid_matches_horizontal() {
    use crate::grid::snap_to_grid;

    let mut settings = Settings::default();
    settings.snap_to_vertical_grid = true;
    settings.grid_enabled = true;
    settings.snap_to_grid = true;

    let zoom = 1.0;
    let bpm = 120.0;
    let val = 133.0;

    let snapped_x = snap_to_grid(val, &settings, zoom, bpm);
    let snapped_y = snap_to_vertical_grid(val, &settings, zoom, bpm);
    assert_eq!(snapped_x, snapped_y);
}

#[test]
fn snap_to_vertical_grid_waveform_position() {
    use std::sync::Arc;
    use crate::App;
    use crate::entity_id::new_id;
    use crate::automation::AutomationData;
    use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView};
    use crate::audio::AudioClipData;

    let mut app = App::new_headless();
    app.settings.snap_to_vertical_grid = true;
    app.settings.grid_enabled = true;
    app.settings.snap_to_grid = true;

    let zoom = 1.0;
    let bpm = app.bpm;
    let spacing = grid_spacing_for_settings(&app.settings, zoom, bpm);

    let id = new_id();
    app.waveforms.insert(id, WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 48000,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [100.0, spacing * 1.3],
        size: [200.0, 80.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 4.0,
        fade_in_px: 0.0,
        fade_out_px: 0.0,
        fade_in_curve: 0.5,
        fade_out_curve: 0.5,
        volume: 1.0,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
    });
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    });

    let snapped_y = snap_to_vertical_grid(
        app.waveforms[&id].position[1],
        &app.settings,
        zoom,
        bpm,
    );
    assert_eq!(snapped_y, spacing);
}
