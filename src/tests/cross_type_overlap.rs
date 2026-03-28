use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::midi::{self, MidiNote};
use crate::settings::{GridMode, FixedGrid};
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::App;

fn make_waveform(x: f32, y: f32, width: f32) -> WaveformView {
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
        size: [width, 80.0],
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

fn make_audio_clip() -> AudioClipData {
    AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    }
}

fn make_midi_clip(x: f32, y: f32, width: f32) -> midi::MidiClip {
    midi::MidiClip {
        position: [x, y],
        size: [width, 80.0],
        color: [0.6, 0.3, 0.9, 0.7],
        notes: vec![
            MidiNote { pitch: 60, start_px: 10.0, duration_px: 30.0, velocity: 100 },
            MidiNote { pitch: 64, start_px: 50.0, duration_px: 40.0, velocity: 80 },
            MidiNote { pitch: 67, start_px: 100.0, duration_px: 20.0, velocity: 90 },
        ],
        pitch_range: (48, 84),
        grid_mode: GridMode::Fixed(FixedGrid::Eighth),
        triplet_grid: false,
        velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
        instrument_id: None,
        disabled: false,
    }
}

// -----------------------------------------------------------------------
// Cross-type overlap tests: MIDI clip active, waveform victim
// -----------------------------------------------------------------------

#[test]
fn test_midi_clip_fully_covers_waveform() {
    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip: x=100, width=300 (covers 100..400)
    app.midi_clips.insert(midi_id, make_midi_clip(100.0, 50.0, 300.0));

    // Waveform: x=150, width=100 (covers 150..250) — fully inside MIDI clip
    app.waveforms.insert(wf_id, make_waveform(150.0, 50.0, 100.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let ops = app.resolve_clip_overlaps(&[midi_id]);

    assert!(!ops.is_empty(), "should produce delete ops");
    assert!(app.waveforms.get(&wf_id).is_none(), "waveform should be deleted");
    assert!(app.midi_clips.get(&midi_id).is_some(), "MIDI clip should remain");
}

#[test]
fn test_midi_clip_crops_waveform_tail() {
    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip: x=200, width=200 (covers 200..400)
    app.midi_clips.insert(midi_id, make_midi_clip(200.0, 50.0, 200.0));

    // Waveform: x=100, width=200 (covers 100..300) — tail overlaps MIDI
    app.waveforms.insert(wf_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let ops = app.resolve_clip_overlaps(&[midi_id]);

    assert!(!ops.is_empty());
    let wf = app.waveforms.get(&wf_id).expect("waveform should still exist");
    assert!((wf.size[0] - 100.0).abs() < 0.01, "waveform should be cropped to 100, got {}", wf.size[0]);
    assert!((wf.position[0] - 100.0).abs() < 0.01, "waveform position should remain at 100");
}

#[test]
fn test_midi_clip_crops_waveform_head() {
    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip: x=100, width=200 (covers 100..300)
    app.midi_clips.insert(midi_id, make_midi_clip(100.0, 50.0, 200.0));

    // Waveform: x=200, width=200 (covers 200..400) — head overlaps MIDI's end
    app.waveforms.insert(wf_id, make_waveform(200.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let ops = app.resolve_clip_overlaps(&[midi_id]);

    assert!(!ops.is_empty());
    let wf = app.waveforms.get(&wf_id).expect("waveform should still exist");
    assert!((wf.position[0] - 300.0).abs() < 0.01, "waveform should be moved to 300, got {}", wf.position[0]);
    assert!((wf.size[0] - 100.0).abs() < 0.01, "waveform should be 100 wide, got {}", wf.size[0]);
    assert!((wf.sample_offset_px - 100.0).abs() < 0.01, "sample_offset should be 100");
}

#[test]
fn test_midi_clip_splits_waveform() {
    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip: x=200, width=100 (covers 200..300) — inside waveform
    app.midi_clips.insert(midi_id, make_midi_clip(200.0, 50.0, 100.0));

    // Waveform: x=100, width=400 (covers 100..500) — larger
    let mut wf = make_waveform(100.0, 50.0, 400.0);
    wf.fade_in_px = 20.0;
    wf.fade_out_px = 30.0;
    app.waveforms.insert(wf_id, wf);
    app.audio_clips.insert(wf_id, make_audio_clip());

    let initial_wf_count = app.waveforms.len();
    let ops = app.resolve_clip_overlaps(&[midi_id]);

    assert!(!ops.is_empty());
    assert_eq!(app.waveforms.len(), initial_wf_count + 1, "should have created a split waveform");

    // Left portion
    let left = app.waveforms.get(&wf_id).expect("left portion should exist");
    assert!((left.size[0] - 100.0).abs() < 0.01, "left width should be 100");
    assert!((left.fade_out_px).abs() < 0.01, "left fade_out should be 0");

    // Right portion
    let right_id = app.waveforms.keys()
        .find(|id| **id != wf_id)
        .expect("should find right portion");
    let right = &app.waveforms[right_id];
    assert!((right.position[0] - 300.0).abs() < 0.01, "right pos should be 300");
    assert!((right.size[0] - 200.0).abs() < 0.01, "right width should be 200");
    assert!((right.fade_in_px).abs() < 0.01, "right fade_in should be 0");
}

// -----------------------------------------------------------------------
// Cross-type overlap tests: Waveform active, MIDI clip victim
// -----------------------------------------------------------------------

#[test]
fn test_waveform_fully_covers_midi_clip() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    let midi_id = new_id();

    // Waveform: x=100, width=300 (covers 100..400)
    app.waveforms.insert(wf_id, make_waveform(100.0, 50.0, 300.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    // MIDI clip: x=150, width=100 (covers 150..250) — fully inside waveform
    app.midi_clips.insert(midi_id, make_midi_clip(150.0, 50.0, 100.0));

    let ops = app.resolve_clip_overlaps(&[wf_id]);

    assert!(!ops.is_empty(), "should produce delete ops");
    assert!(app.midi_clips.get(&midi_id).is_none(), "MIDI clip should be deleted");
    assert!(app.waveforms.get(&wf_id).is_some(), "waveform should remain");
}

#[test]
fn test_waveform_crops_midi_clip_tail() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    let midi_id = new_id();

    // Waveform: x=200, width=200 (covers 200..400)
    app.waveforms.insert(wf_id, make_waveform(200.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    // MIDI clip: x=100, width=200 (covers 100..300) — tail overlaps waveform
    app.midi_clips.insert(midi_id, make_midi_clip(100.0, 50.0, 200.0));

    let ops = app.resolve_clip_overlaps(&[wf_id]);

    assert!(!ops.is_empty());
    let mc = app.midi_clips.get(&midi_id).expect("MIDI clip should still exist");
    assert!((mc.size[0] - 100.0).abs() < 0.01, "MIDI clip width should be cropped to 100, got {}", mc.size[0]);
    // Notes beyond 100px should be removed
    assert!(mc.notes.iter().all(|n| n.start_px < 100.0), "all notes should be within cropped bounds");
}

#[test]
fn test_waveform_crops_midi_clip_head() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    let midi_id = new_id();

    // Waveform: x=100, width=200 (covers 100..300)
    app.waveforms.insert(wf_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    // MIDI clip: x=200, width=200 (covers 200..400) — head overlaps waveform's end
    let mut mc = make_midi_clip(200.0, 50.0, 200.0);
    // Add notes spread across the clip
    mc.notes = vec![
        MidiNote { pitch: 60, start_px: 10.0, duration_px: 30.0, velocity: 100 },  // at 10-40, will be cropped
        MidiNote { pitch: 64, start_px: 120.0, duration_px: 40.0, velocity: 80 },  // at 120-160, will shift
    ];
    app.midi_clips.insert(midi_id, mc);

    let ops = app.resolve_clip_overlaps(&[wf_id]);

    assert!(!ops.is_empty());
    let mc = app.midi_clips.get(&midi_id).expect("MIDI clip should still exist");
    assert!((mc.position[0] - 300.0).abs() < 0.01, "MIDI clip should move to 300, got {}", mc.position[0]);
    assert!((mc.size[0] - 100.0).abs() < 0.01, "MIDI clip width should be 100, got {}", mc.size[0]);
    // The first note (start_px=10) should be removed (it's within the cropped area 0..100)
    // The second note (start_px=120) should be shifted: 120-100=20
    assert_eq!(mc.notes.len(), 1, "first note should be removed by crop");
    assert!((mc.notes[0].start_px - 20.0).abs() < 0.01, "surviving note should be shifted to 20");
}

#[test]
fn test_waveform_splits_midi_clip() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    let midi_id = new_id();

    // Waveform: x=200, width=100 (covers 200..300) — inside MIDI clip
    app.waveforms.insert(wf_id, make_waveform(200.0, 50.0, 100.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    // MIDI clip: x=100, width=300 (covers 100..400) — larger
    let mut mc = make_midi_clip(100.0, 50.0, 300.0);
    mc.notes = vec![
        MidiNote { pitch: 60, start_px: 10.0, duration_px: 30.0, velocity: 100 },   // at 10-40 -> left portion
        MidiNote { pitch: 64, start_px: 130.0, duration_px: 40.0, velocity: 80 },   // at 130-170 -> removed (inside waveform)
        MidiNote { pitch: 67, start_px: 220.0, duration_px: 20.0, velocity: 90 },   // at 220-240 -> right portion
    ];
    app.midi_clips.insert(midi_id, mc);

    let initial_mc_count = app.midi_clips.len();
    let ops = app.resolve_clip_overlaps(&[wf_id]);

    assert!(!ops.is_empty());
    assert_eq!(app.midi_clips.len(), initial_mc_count + 1, "should have created a split MIDI clip");

    // Left portion: midi_id cropped to [100, 200]
    let left = app.midi_clips.get(&midi_id).expect("left portion should exist");
    assert!((left.size[0] - 100.0).abs() < 0.01, "left width should be 100");
    assert_eq!(left.notes.len(), 1, "left should have one note");
    assert!((left.notes[0].start_px - 10.0).abs() < 0.01, "left note start should be 10");

    // Right portion: new clip at [300, 400]
    let right_id = app.midi_clips.keys()
        .find(|id| **id != midi_id)
        .expect("should find right portion");
    let right = &app.midi_clips[right_id];
    assert!((right.position[0] - 300.0).abs() < 0.01, "right pos should be 300");
    assert!((right.size[0] - 100.0).abs() < 0.01, "right width should be 100");
    assert_eq!(right.notes.len(), 1, "right should have one note");
    // Original note at 220, crop_left = 200, so new start = 220-200 = 20
    assert!((right.notes[0].start_px - 20.0).abs() < 0.01, "right note should be shifted to 20");
}

// -----------------------------------------------------------------------
// Cross-type live overlap tests
// -----------------------------------------------------------------------

#[test]
fn test_live_midi_over_waveform_and_restore() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip starts far away
    app.midi_clips.insert(midi_id, make_midi_clip(800.0, 50.0, 100.0));

    // Waveform at x=100, width=200
    app.waveforms.insert(wf_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let mut snaps: IndexMap<crate::entity_id::EntityId, crate::ClipSnapshot> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Move MIDI clip to overlap waveform: 200..300 overlaps 100..300
    app.midi_clips.get_mut(&midi_id).unwrap().position[0] = 200.0;
    app.resolve_clip_overlaps_live(&[midi_id], &mut snaps, &mut tsplits);

    assert!(!snaps.is_empty(), "should have snapshotted waveform");
    let wf = app.waveforms.get(&wf_id).unwrap();
    assert!((wf.size[0] - 100.0).abs() < 0.01, "waveform should be cropped to 100 during live drag");

    // Move MIDI clip away
    app.midi_clips.get_mut(&midi_id).unwrap().position[0] = 800.0;
    app.resolve_clip_overlaps_live(&[midi_id], &mut snaps, &mut tsplits);

    let wf = app.waveforms.get(&wf_id).unwrap();
    assert!((wf.size[0] - 200.0).abs() < 0.01, "waveform should be restored to 200 after moving away");
    assert!(snaps.is_empty(), "snapshots should be cleared");
}

#[test]
fn test_live_waveform_over_midi_and_restore() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let wf_id = new_id();
    let midi_id = new_id();

    // Waveform starts far away
    app.waveforms.insert(wf_id, make_waveform(800.0, 50.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    // MIDI clip at x=100, width=200
    app.midi_clips.insert(midi_id, make_midi_clip(100.0, 50.0, 200.0));

    let mut snaps: IndexMap<crate::entity_id::EntityId, crate::ClipSnapshot> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Move waveform to fully cover MIDI clip: 50..250 covers 100..200
    // Waveform is 200px wide, MIDI clip is at 100..300 (200px wide)
    // Make waveform start at 50, it's 200px so covers 50..250 — doesn't fully cover 100..300
    // Need waveform to be wider or MIDI narrower. Set waveform size to 400 for full coverage.
    app.waveforms.get_mut(&wf_id).unwrap().size[0] = 400.0;
    app.waveforms.get_mut(&wf_id).unwrap().position[0] = 50.0;
    app.resolve_clip_overlaps_live(&[wf_id], &mut snaps, &mut tsplits);

    assert!(!snaps.is_empty(), "should have snapshotted MIDI clip");
    let mc = app.midi_clips.get(&midi_id).unwrap();
    assert!(mc.disabled, "MIDI clip should be disabled when fully covered");

    // Move waveform away (restore the original 200px size for clean test)
    app.waveforms.get_mut(&wf_id).unwrap().position[0] = 800.0;
    app.waveforms.get_mut(&wf_id).unwrap().size[0] = 400.0;
    app.resolve_clip_overlaps_live(&[wf_id], &mut snaps, &mut tsplits);

    let mc = app.midi_clips.get(&midi_id).unwrap();
    assert!(!mc.disabled, "MIDI clip should be restored");
    assert!((mc.size[0] - 200.0).abs() < 0.01, "MIDI clip size should be restored");
    assert!(snaps.is_empty(), "snapshots should be cleared");
}

#[test]
fn test_no_cross_type_overlap_different_y() {
    let mut app = App::new_headless();

    let midi_id = new_id();
    let wf_id = new_id();

    // MIDI clip at y=50 (covers 50..130)
    app.midi_clips.insert(midi_id, make_midi_clip(100.0, 50.0, 200.0));

    // Waveform at y=200 (covers 200..280) — different track
    app.waveforms.insert(wf_id, make_waveform(100.0, 200.0, 200.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let ops = app.resolve_clip_overlaps(&[midi_id]);
    assert!(ops.is_empty(), "should produce no ops for non-overlapping Y");
}
