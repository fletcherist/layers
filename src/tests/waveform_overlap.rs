use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
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
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
    }
}

fn make_audio_clip() -> AudioClipData {
    AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    }
}

#[test]
fn test_fully_covered_waveform_deleted() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=100, width=300 (covers 100..400)
    app.waveforms.insert(active_id, make_waveform(100.0, 50.0, 300.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=150, width=100 (covers 150..250) — fully inside active
    app.waveforms.insert(bg_id, make_waveform(150.0, 50.0, 100.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(!ops.is_empty(), "should produce delete ops");
    assert!(app.waveforms.get(&bg_id).is_none(), "background waveform should be deleted");
    assert!(app.audio_clips.get(&bg_id).is_none(), "background audio clip should be deleted");
    assert!(app.waveforms.get(&active_id).is_some(), "active waveform should remain");
}

#[test]
fn test_tail_overlap_crops_right_edge() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=200, width=200 (covers 200..400)
    app.waveforms.insert(active_id, make_waveform(200.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=100, width=200 (covers 100..300) — tail overlaps active's start
    app.waveforms.insert(bg_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(!ops.is_empty(), "should produce update ops");
    let bg = app.waveforms.get(&bg_id).expect("background should still exist");
    assert!((bg.size[0] - 100.0).abs() < 0.01, "background width should be cropped to 100, got {}", bg.size[0]);
    assert!((bg.position[0] - 100.0).abs() < 0.01, "background position should remain at 100");
}

#[test]
fn test_head_overlap_crops_left_edge() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=100, width=200 (covers 100..300)
    app.waveforms.insert(active_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=200, width=200 (covers 200..400) — head overlaps active's end
    app.waveforms.insert(bg_id, make_waveform(200.0, 50.0, 200.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(!ops.is_empty(), "should produce update ops");
    let bg = app.waveforms.get(&bg_id).expect("background should still exist");
    assert!((bg.position[0] - 300.0).abs() < 0.01, "background should be moved to x=300, got {}", bg.position[0]);
    assert!((bg.size[0] - 100.0).abs() < 0.01, "background width should be 100, got {}", bg.size[0]);
    assert!((bg.sample_offset_px - 100.0).abs() < 0.01, "sample_offset_px should be 100, got {}", bg.sample_offset_px);
}

#[test]
fn test_no_overlap_different_y() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: y=50, height=80 (covers 50..130)
    app.waveforms.insert(active_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: y=200, height=80 (covers 200..280) — different track, no Y overlap
    app.waveforms.insert(bg_id, make_waveform(100.0, 200.0, 200.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(ops.is_empty(), "should produce no ops for non-overlapping Y");
    assert!(app.waveforms.get(&bg_id).is_some(), "background waveform should remain");
    assert!((app.waveforms[&bg_id].size[0] - 200.0).abs() < 0.01, "background should be unchanged");
}

#[test]
fn test_no_overlap_same_y_different_x() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=100, width=100 (covers 100..200)
    app.waveforms.insert(active_id, make_waveform(100.0, 50.0, 100.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=300, width=100 (covers 300..400) — no horizontal overlap
    app.waveforms.insert(bg_id, make_waveform(300.0, 50.0, 100.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(ops.is_empty(), "should produce no ops when no horizontal overlap");
    assert!(app.waveforms.get(&bg_id).is_some());
}

#[test]
fn test_small_tail_overlap_deletes() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=195, width=200 (covers 195..395)
    app.waveforms.insert(active_id, make_waveform(195.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=190, width=10 (covers 190..200) — after crop, width would be 5 < WAVEFORM_MIN_WIDTH_PX
    app.waveforms.insert(bg_id, make_waveform(190.0, 50.0, 10.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let _ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(app.waveforms.get(&bg_id).is_none(), "tiny waveform should be deleted");
}

#[test]
fn test_fade_clamped_on_tail_crop() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    app.waveforms.insert(active_id, make_waveform(200.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    let mut bg = make_waveform(50.0, 50.0, 200.0);
    bg.fade_out_px = 80.0;
    app.waveforms.insert(bg_id, bg);
    app.audio_clips.insert(bg_id, make_audio_clip());

    let _ops = app.resolve_waveform_overlaps(&[active_id]);

    let bg = app.waveforms.get(&bg_id).expect("should still exist");
    let new_width = bg.size[0]; // should be 150
    assert!(bg.fade_out_px <= new_width * 0.5, "fade_out_px should be clamped to half of new width");
}

#[test]
fn test_live_overlap_restores_on_move_away() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active starts at x=500 (no overlap)
    app.waveforms.insert(active_id, make_waveform(500.0, 50.0, 200.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background at x=100, width=200 (covers 100..300)
    app.waveforms.insert(bg_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let mut snaps: IndexMap<crate::entity_id::EntityId, WaveformView> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Simulate drag: move active to overlap bg
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 200.0; // now 200..400, overlaps bg 100..300
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    assert!(!snaps.is_empty(), "should have snapshotted bg");
    let bg = app.waveforms.get(&bg_id).unwrap();
    assert!((bg.size[0] - 100.0).abs() < 0.01, "bg should be cropped to 100 during live drag, got {}", bg.size[0]);

    // Simulate drag: move active away (no more overlap)
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 500.0; // back to 500..700
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    let bg = app.waveforms.get(&bg_id).unwrap();
    assert!((bg.size[0] - 200.0).abs() < 0.01, "bg should be restored to original 200 after moving away, got {}", bg.size[0]);
    assert!((bg.position[0] - 100.0).abs() < 0.01, "bg position should be restored to 100");
    assert!(snaps.is_empty(), "snapshots should be cleared when no overlap");
}

#[test]
fn test_live_overlap_disabled_restored_on_move_away() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: large, will fully cover bg
    app.waveforms.insert(active_id, make_waveform(500.0, 50.0, 400.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=100, width=100 (covers 100..200)
    app.waveforms.insert(bg_id, make_waveform(100.0, 50.0, 100.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let mut snaps: IndexMap<crate::entity_id::EntityId, WaveformView> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Move active to fully cover bg
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 50.0; // 50..450 covers 100..200
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    let bg = app.waveforms.get(&bg_id).unwrap();
    assert!(bg.disabled, "bg should be disabled (hidden) when fully covered");

    // Move active away
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 500.0;
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    let bg = app.waveforms.get(&bg_id).unwrap();
    assert!(!bg.disabled, "bg should be restored and not disabled after moving away");
    assert!((bg.size[0] - 100.0).abs() < 0.01, "bg width should be restored");
}

#[test]
fn test_resolve_all_waveform_overlaps_rightmost_wins() {
    let mut app = App::new_headless();

    let left_id = new_id();
    let right_id = new_id();

    // Two waveforms on the same Y track, overlapping horizontally.
    // Left: x=100, width=200 (covers 100..300)
    // Right: x=250, width=200 (covers 250..450)
    // Rightmost should win — left gets its tail cropped to 250.
    app.waveforms.insert(left_id, make_waveform(100.0, 50.0, 200.0));
    app.audio_clips.insert(left_id, make_audio_clip());
    app.waveforms.insert(right_id, make_waveform(250.0, 50.0, 200.0));
    app.audio_clips.insert(right_id, make_audio_clip());

    let ops = app.resolve_all_waveform_overlaps();

    assert!(!ops.is_empty(), "should produce ops");
    let left = app.waveforms.get(&left_id).expect("left should exist");
    assert!((left.size[0] - 150.0).abs() < 0.01, "left width should be cropped to 150, got {}", left.size[0]);
    let right = app.waveforms.get(&right_id).expect("right should exist");
    assert!((right.size[0] - 200.0).abs() < 0.01, "right should be unchanged at 200, got {}", right.size[0]);
}

#[test]
fn test_bpm_decrease_causes_overlap_resolved() {
    let mut app = App::new_headless();

    let id_a = new_id();
    let id_b = new_id();

    // At BPM=120, place two waveforms adjacent on the same track.
    // A at x=100, width=100 (covers 100..200)
    // B at x=200, width=100 (covers 200..300) — touching but not overlapping.
    app.waveforms.insert(id_a, make_waveform(100.0, 50.0, 100.0));
    app.audio_clips.insert(id_a, make_audio_clip());
    app.waveforms.insert(id_b, make_waveform(200.0, 50.0, 100.0));
    app.audio_clips.insert(id_b, make_audio_clip());
    app.bpm = 120.0;

    // Decrease BPM: positions compress but sizes stay the same.
    // scale = old_bpm / new_bpm = 120/60 = 2.0 ... wait, that would expand.
    // Actually scale = old/new: lowering BPM means new < old so scale > 1, which EXPANDS positions.
    // For compression: increase BPM. old=120, new=240, scale=0.5.
    // A.x = 100*0.5 = 50, B.x = 200*0.5 = 100. A covers 50..150, B covers 100..200 — overlap!
    let old_bpm = 120.0;
    let new_bpm = 240.0;
    app.rescale_clip_positions(old_bpm / new_bpm);
    app.bpm = new_bpm;

    let ops = app.resolve_all_waveform_overlaps();

    // A (x=50, w=100, covers 50..150) and B (x=100, w=100, covers 100..200) overlap.
    // B is rightmost (starts at 100) so B wins. A's tail gets cropped at B's start (100).
    // A's new width = 100 - 50 = 50.
    assert!(!ops.is_empty(), "should produce overlap ops after BPM change");
    let a = app.waveforms.get(&id_a).expect("A should still exist");
    assert!((a.size[0] - 50.0).abs() < 0.01, "A should be cropped to 50, got {}", a.size[0]);
    let b = app.waveforms.get(&id_b).expect("B should still exist");
    assert!((b.size[0] - 100.0).abs() < 0.01, "B should remain at 100, got {}", b.size[0]);
}

#[test]
fn test_bpm_live_drag_overlap_snapshots() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let id_a = new_id();
    let id_b = new_id();

    // Adjacent waveforms at y=50
    app.waveforms.insert(id_a, make_waveform(100.0, 50.0, 100.0));
    app.audio_clips.insert(id_a, make_audio_clip());
    app.waveforms.insert(id_b, make_waveform(200.0, 50.0, 100.0));
    app.audio_clips.insert(id_b, make_audio_clip());
    app.bpm = 120.0;

    let mut snaps: IndexMap<crate::entity_id::EntityId, WaveformView> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Simulate BPM drag that compresses positions (scale = 0.5)
    app.rescale_clip_positions(0.5);
    app.bpm = 240.0;
    app.resolve_all_waveform_overlaps_live(&mut snaps, &mut tsplits);

    // Should have cropped A
    let a = app.waveforms.get(&id_a).unwrap();
    assert!(a.size[0] < 100.0, "A should be cropped during live drag, got {}", a.size[0]);
    assert!(!snaps.is_empty(), "should have snapshots");

    // Simulate dragging BPM back (undo compression, scale = 2.0 to go from 240 back to 120)
    app.rescale_clip_positions(2.0);
    app.bpm = 120.0;
    app.resolve_all_waveform_overlaps_live(&mut snaps, &mut tsplits);

    // Should restore A to original
    let a = app.waveforms.get(&id_a).unwrap();
    assert!((a.size[0] - 100.0).abs() < 0.01, "A should be restored to 100, got {}", a.size[0]);
    assert!(snaps.is_empty(), "snapshots should be empty after restoring");
}

#[test]
fn test_clip_height_adapts_to_bpm_change() {
    use crate::grid;

    let mut app = App::new_headless();
    app.bpm = 120.0;

    let id = new_id();
    let initial_height = grid::clip_height(120.0);
    let initial_y = 100.0;

    app.waveforms.insert(id, make_waveform(50.0, initial_y, 200.0));
    app.waveforms.get_mut(&id).unwrap().size[1] = initial_height;
    app.audio_clips.insert(id, make_audio_clip());

    // Change BPM from 120 to 60 (scale = 2.0, grid rows double)
    let scale = 120.0_f32 / 60.0;
    app.rescale_clip_positions(scale);
    app.bpm = 60.0;

    let wf = app.waveforms.get(&id).unwrap();
    let expected_height = grid::clip_height(60.0);
    assert!(
        (wf.size[1] - expected_height).abs() < 0.01,
        "height should be {} at 60 BPM, got {}",
        expected_height, wf.size[1]
    );
    assert!(
        (wf.position[1] - initial_y * scale).abs() < 0.01,
        "Y position should scale to {}, got {}",
        initial_y * scale, wf.position[1]
    );

    // Change BPM from 60 to 240 (scale = 0.25, grid rows shrink)
    let scale2 = 60.0_f32 / 240.0;
    app.rescale_clip_positions(scale2);
    app.bpm = 240.0;

    let wf = app.waveforms.get(&id).unwrap();
    let expected_height2 = grid::clip_height(240.0);
    assert!(
        (wf.size[1] - expected_height2).abs() < 0.01,
        "height should be {} at 240 BPM, got {}",
        expected_height2, wf.size[1]
    );

    // Round-trip: go back to 120 BPM and verify original height is restored
    let scale3 = 240.0_f32 / 120.0;
    app.rescale_clip_positions(scale3);
    app.bpm = 120.0;

    let wf = app.waveforms.get(&id).unwrap();
    assert!(
        (wf.size[1] - initial_height).abs() < 0.01,
        "height should be restored to {} after round-trip, got {}",
        initial_height, wf.size[1]
    );
}

#[test]
fn test_split_when_active_inside_background() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=200, width=100 (covers 200..300) — smaller clip moved inside bg
    app.waveforms.insert(active_id, make_waveform(200.0, 50.0, 100.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=100, width=400 (covers 100..500) — larger clip
    let mut bg = make_waveform(100.0, 50.0, 400.0);
    bg.fade_in_px = 20.0;
    bg.fade_out_px = 30.0;
    bg.sample_offset_px = 10.0;
    app.waveforms.insert(bg_id, bg);
    app.audio_clips.insert(bg_id, make_audio_clip());

    let initial_wf_count = app.waveforms.len();
    let ops = app.resolve_waveform_overlaps(&[active_id]);

    assert!(!ops.is_empty(), "should produce ops");
    assert_eq!(app.waveforms.len(), initial_wf_count + 1, "should have created one new waveform");

    // Left portion: bg_id should be cropped to [100, 200]
    let left = app.waveforms.get(&bg_id).expect("left portion should still exist");
    assert!((left.position[0] - 100.0).abs() < 0.01, "left pos should be 100, got {}", left.position[0]);
    assert!((left.size[0] - 100.0).abs() < 0.01, "left width should be 100, got {}", left.size[0]);
    assert!((left.sample_offset_px - 10.0).abs() < 0.01, "left offset should remain 10");
    assert!((left.fade_in_px - 20.0).abs() < 0.01, "left should keep original fade_in");
    assert!((left.fade_out_px).abs() < 0.01, "left fade_out should be 0 (internal edge)");

    // Right portion: new waveform at [300, 500]
    let right_id = app.waveforms.keys()
        .find(|id| **id != active_id && **id != bg_id)
        .expect("should find right portion");
    let right = &app.waveforms[right_id];
    assert!((right.position[0] - 300.0).abs() < 0.01, "right pos should be 300, got {}", right.position[0]);
    assert!((right.size[0] - 200.0).abs() < 0.01, "right width should be 200, got {}", right.size[0]);
    assert!((right.sample_offset_px - 210.0).abs() < 0.01, "right offset should be 10 + (300-100) = 210, got {}", right.sample_offset_px);
    assert!((right.fade_in_px).abs() < 0.01, "right fade_in should be 0 (internal edge)");
    assert!((right.fade_out_px - 30.0).abs() < 0.01, "right should keep original fade_out");

    // Audio clip should exist for right portion
    assert!(app.audio_clips.contains_key(right_id), "right portion should have audio clip");
}

#[test]
fn test_split_live_preview_and_restore() {
    use indexmap::IndexMap;

    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active starts far away (no overlap)
    app.waveforms.insert(active_id, make_waveform(800.0, 50.0, 100.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=100, width=400 (covers 100..500)
    app.waveforms.insert(bg_id, make_waveform(100.0, 50.0, 400.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let mut snaps: IndexMap<crate::entity_id::EntityId, WaveformView> = IndexMap::new();
    let mut tsplits: Vec<crate::entity_id::EntityId> = Vec::new();

    // Move active into bg: active at 200..300, bg covers 100..500
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 200.0;
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    // Should have split bg: left [100..200], right [300..500]
    assert!(!snaps.is_empty(), "should have snapshotted bg");
    assert_eq!(tsplits.len(), 1, "should have one temp split");

    let left = app.waveforms.get(&bg_id).unwrap();
    assert!((left.size[0] - 100.0).abs() < 0.01, "left should be cropped to 100, got {}", left.size[0]);

    let right_id = tsplits[0];
    let right = app.waveforms.get(&right_id).unwrap();
    assert!((right.position[0] - 300.0).abs() < 0.01, "right pos should be 300, got {}", right.position[0]);
    assert!((right.size[0] - 200.0).abs() < 0.01, "right width should be 200, got {}", right.size[0]);

    // Move active away: should restore bg and remove temp split
    app.waveforms.get_mut(&active_id).unwrap().position[0] = 800.0;
    app.resolve_waveform_overlaps_live(&[active_id], &mut snaps, &mut tsplits);

    let bg = app.waveforms.get(&bg_id).unwrap();
    assert!((bg.size[0] - 400.0).abs() < 0.01, "bg should be restored to original 400, got {}", bg.size[0]);
    assert!((bg.position[0] - 100.0).abs() < 0.01, "bg pos should be restored to 100");
    assert!(tsplits.is_empty(), "temp splits should be empty after moving away");
    assert!(app.waveforms.get(&right_id).is_none(), "temp right waveform should be removed");
    assert!(snaps.is_empty(), "snapshots should be cleared when no overlap");
}

#[test]
fn test_split_min_width_threshold() {
    let mut app = App::new_headless();

    let active_id = new_id();
    let bg_id = new_id();

    // Active: x=105, width=390 (covers 105..495) — leaves tiny left (5px) and right (5px)
    app.waveforms.insert(active_id, make_waveform(105.0, 50.0, 390.0));
    app.audio_clips.insert(active_id, make_audio_clip());

    // Background: x=100, width=400 (covers 100..500)
    app.waveforms.insert(bg_id, make_waveform(100.0, 50.0, 400.0));
    app.audio_clips.insert(bg_id, make_audio_clip());

    let _ops = app.resolve_waveform_overlaps(&[active_id]);

    // Both left (5px) and right (5px) are below WAVEFORM_MIN_WIDTH_PX (10.0)
    // Left should be deleted, right should not be created
    assert!(app.waveforms.get(&bg_id).is_none(), "bg should be deleted (left too small)");
    let extra_count = app.waveforms.keys().filter(|id| **id != active_id).count();
    assert_eq!(extra_count, 0, "no right portion should be created (too small)");
}
