use std::sync::Arc;

use crate::automation::AutomationData;
use crate::midi;
use crate::settings::{FixedGrid, GridMode};
use crate::storage::{self, f32_slice_to_u8, u8_slice_to_f32};
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};

#[test]
fn test_f32_u8_roundtrip() {
    let original: Vec<f32> = vec![0.0, 1.0, -1.0, 0.5, std::f32::consts::PI];
    let bytes = f32_slice_to_u8(&original);
    let restored = u8_slice_to_f32(&bytes);
    assert_eq!(original, restored);
}

#[test]
fn test_u8_to_f32_odd_bytes_returns_empty() {
    let odd_bytes = vec![1u8, 2, 3, 4, 5]; // 5 bytes, not multiple of 4
    let result = u8_slice_to_f32(&odd_bytes);
    assert!(result.is_empty());
}

#[test]
fn test_automation_points_survive_save_load() {
    // Simulate the save path: extract pairs from AutomationData
    let mut data = AutomationData::new();
    data.volume_lane_mut().insert_point(0.1, 0.3);
    data.volume_lane_mut().insert_point(0.5, 0.8);
    data.pan_lane_mut().insert_point(0.2, 0.7);

    // Extract stored format (what gets serialized)
    let vol_stored: Vec<[f32; 2]> = data
        .volume_lane()
        .points
        .iter()
        .map(|p| [p.t, p.value])
        .collect();
    let pan_stored: Vec<[f32; 2]> = data
        .pan_lane()
        .points
        .iter()
        .map(|p| [p.t, p.value])
        .collect();

    // Reconstruct (what happens on load)
    let restored = AutomationData::from_stored(&vol_stored, &pan_stored);

    assert_eq!(restored.volume_lane().points.len(), 2);
    assert_eq!(restored.pan_lane().points.len(), 1);

    // Verify values match at stored points
    for (orig, rest) in data
        .volume_lane()
        .points
        .iter()
        .zip(restored.volume_lane().points.iter())
    {
        assert!((orig.t - rest.t).abs() < 1e-6);
        assert!((orig.value - rest.value).abs() < 1e-6);
    }
}

#[test]
fn test_f32_u8_roundtrip_with_single_value() {
    let single = vec![42.0f32];
    let bytes = f32_slice_to_u8(&single);
    assert_eq!(bytes.len(), 4);
    let restored = u8_slice_to_f32(&bytes);
    assert_eq!(restored, single);
}

#[test]
fn test_midi_clip_survives_save_load_roundtrip() {
    let original = midi::MidiClip {
        position: [100.0, 200.0],
        size: [480.0, 200.0],
        color: [0.6, 0.3, 0.9, 0.7],
        notes: vec![
            midi::MidiNote { pitch: 60, start_px: 10.0, duration_px: 30.0, velocity: 100 },
            midi::MidiNote { pitch: 72, start_px: 50.0, duration_px: 15.0, velocity: 80 },
        ],
        pitch_range: (48, 84),
        grid_mode: GridMode::Fixed(FixedGrid::Eighth),
        triplet_grid: true,
        velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
        instrument_id: None,
    };

    // Save path: MidiClip -> StoredMidiClip
    let (grid_tag, grid_val) = storage::grid_mode_to_stored(original.grid_mode);
    let stored = storage::StoredMidiClip {
        id: crate::entity_id::new_id().to_string(),
        position: original.position,
        size: original.size,
        color: original.color,
        notes: original.notes.iter().map(|n| storage::StoredMidiNote {
            pitch: n.pitch as u32,
            start_px: n.start_px,
            duration_px: n.duration_px,
            velocity: n.velocity as u32,
        }).collect(),
        pitch_low: original.pitch_range.0 as u32,
        pitch_high: original.pitch_range.1 as u32,
        grid_mode_tag: grid_tag,
        grid_mode_value: grid_val,
        triplet_grid: original.triplet_grid,
        instrument_region_id: String::new(),
    };

    // Load path: StoredMidiClip -> MidiClip
    let restored = midi::MidiClip {
        position: stored.position,
        size: stored.size,
        color: stored.color,
        notes: stored.notes.into_iter().map(|n| midi::MidiNote {
            pitch: n.pitch as u8,
            start_px: n.start_px,
            duration_px: n.duration_px,
            velocity: n.velocity as u8,
        }).collect(),
        pitch_range: (stored.pitch_low as u8, stored.pitch_high as u8),
        grid_mode: storage::grid_mode_from_stored(&stored.grid_mode_tag, &stored.grid_mode_value),
        triplet_grid: stored.triplet_grid,
        velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
        instrument_id: None,
    };

    assert_eq!(restored.position, original.position);
    assert_eq!(restored.size, original.size);
    assert_eq!(restored.color, original.color);
    assert_eq!(restored.pitch_range, original.pitch_range);
    assert_eq!(restored.notes.len(), original.notes.len());
    for (orig, rest) in original.notes.iter().zip(restored.notes.iter()) {
        assert_eq!(orig.pitch, rest.pitch);
        assert_eq!(orig.start_px, rest.start_px);
        assert_eq!(orig.duration_px, rest.duration_px);
        assert_eq!(orig.velocity, rest.velocity);
    }
    assert_eq!(restored.grid_mode, original.grid_mode);
    assert_eq!(restored.triplet_grid, original.triplet_grid);
}

#[test]
fn test_waveform_filename_survives_serde_roundtrip() {
    let wf = WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(vec![0.1, 0.2, 0.3]),
            right_samples: Arc::new(vec![0.4, 0.5, 0.6]),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 44100,
            filename: "kick.wav".to_string(),
        }),
        filename: "kick.wav".to_string(),
        position: [100.0, 200.0],
        size: [300.0, 80.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 8.0,
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
    };

    // Serialize and deserialize (simulates network transfer)
    let json = serde_json::to_string(&wf).unwrap();
    let restored: WaveformView = serde_json::from_str(&json).unwrap();

    // filename on WaveformView survives (not skipped)
    assert_eq!(restored.filename, "kick.wav");
    // audio data is lost (serde skip) but filename on WaveformView persists
    assert!(restored.audio.left_samples.is_empty());
    assert!(restored.audio.filename.is_empty());
    // position and other fields survive
    assert_eq!(restored.position, wf.position);
    assert_eq!(restored.size, wf.size);
}
