use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::component;
use crate::entity_id::new_id;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView, DEFAULT_AUTO_FADE_PX};
use crate::{App, HitTarget};

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

fn make_waveform_with_samples(x: f32, y: f32, num_samples: usize) -> WaveformView {
    let samples: Vec<f32> = (0..num_samples).map(|i| i as f32 / num_samples as f32).collect();
    WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(samples.clone()),
            right_samples: Arc::new(samples),
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

fn make_audio_clip() -> AudioClipData {
    AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    }
}

fn make_audio_clip_with_samples(num_samples: usize) -> AudioClipData {
    let samples: Vec<f32> = (0..num_samples).map(|i| i as f32 / num_samples as f32).collect();
    AudioClipData {
        samples: Arc::new(samples),
        sample_rate: 48000,
        duration_secs: num_samples as f32 / 48000.0,
    }
}

// ---- Delete tests ----

#[test]
fn test_delete_waveform_removes_audio_clip() {
    let mut app = App::new_headless();
    let id0 = new_id();
    let id1 = new_id();
    app.waveforms.insert(id0, make_waveform(0.0, 0.0));
    app.audio_clips.insert(id0, make_audio_clip());
    app.waveforms.insert(id1, make_waveform(300.0, 0.0));
    app.audio_clips.insert(id1, make_audio_clip());

    app.selected.push(HitTarget::Waveform(id0));
    app.delete_selected();

    assert_eq!(app.waveforms.len(), 1);
    assert_eq!(app.audio_clips.len(), 1);
}

#[test]
fn test_delete_component_removes_owned_waveforms() {
    let mut app = App::new_headless();
    // Add 3 waveforms: wf0, wf1, wf2
    let mut wf_ids = Vec::new();
    for i in 0..3 {
        let id = new_id();
        app.waveforms.insert(id, make_waveform(i as f32 * 300.0, 0.0));
        app.audio_clips.insert(id, make_audio_clip());
        wf_ids.push(id);
    }
    // Component owns wf0 and wf1
    let comp_id = new_id();
    app.components.insert(comp_id, component::ComponentDef {
        id: comp_id,
        name: "C1".to_string(),
        position: [0.0, 0.0],
        size: [500.0, 100.0],
        waveform_ids: vec![wf_ids[0], wf_ids[1]],
    });

    app.selected.push(HitTarget::ComponentDef(comp_id));
    app.delete_selected();

    // Component and its 2 waveforms should be removed, leaving wf2
    assert_eq!(app.components.len(), 0);
    assert_eq!(app.waveforms.len(), 1);
    assert_eq!(app.audio_clips.len(), 1);
}

#[test]
fn test_delete_component_other_component_retains_waveforms() {
    let mut app = App::new_headless();
    // Add 4 waveforms
    let mut wf_ids = Vec::new();
    for i in 0..4 {
        let id = new_id();
        app.waveforms.insert(id, make_waveform(i as f32 * 300.0, 0.0));
        app.audio_clips.insert(id, make_audio_clip());
        wf_ids.push(id);
    }
    // Component A owns wf0, wf1
    let comp_a_id = new_id();
    app.components.insert(comp_a_id, component::ComponentDef {
        id: comp_a_id,
        name: "A".to_string(),
        position: [0.0, 0.0],
        size: [500.0, 100.0],
        waveform_ids: vec![wf_ids[0], wf_ids[1]],
    });
    // Component B owns wf2, wf3
    let comp_b_id = new_id();
    app.components.insert(comp_b_id, component::ComponentDef {
        id: comp_b_id,
        name: "B".to_string(),
        position: [0.0, 200.0],
        size: [500.0, 100.0],
        waveform_ids: vec![wf_ids[2], wf_ids[3]],
    });

    // Delete component A
    app.selected.push(HitTarget::ComponentDef(comp_a_id));
    app.delete_selected();

    // Component B should still have its waveforms
    assert_eq!(app.components.len(), 1);
    let b = app.components.get(&comp_b_id).unwrap();
    assert_eq!(b.name, "B");
    assert_eq!(b.waveform_ids.len(), 2);
    assert_eq!(app.waveforms.len(), 2);
}

// ---- Split tests ----

#[test]
fn test_split_waveform_inserts_both_halves() {
    let mut app = App::new_headless();
    let num_samples = 48000; // 1 second of audio
    let id = new_id();
    app.waveforms.insert(id, make_waveform_with_samples(0.0, 0.0, num_samples));
    app.audio_clips.insert(id, make_audio_clip_with_samples(num_samples));

    let wf_len_before = app.waveforms.len();
    let clip_len_before = app.audio_clips.len();

    // Position mouse in the middle of the waveform
    app.mouse_pos = [200.0, 90.0];

    app.split_sample_at_cursor();

    assert_eq!(
        app.waveforms.len(),
        wf_len_before + 1,
        "split should add one waveform"
    );
    assert_eq!(
        app.audio_clips.len(),
        clip_len_before + 1,
        "split should add one audio clip"
    );
}

#[test]
fn test_split_is_non_destructive_preserves_full_audio() {
    use crate::ui::hit_testing::full_audio_width_px;

    let mut app = App::new_headless();
    let num_samples = 48000;
    let id = new_id();
    let original_wf = make_waveform_with_samples(0.0, 0.0, num_samples);
    let original_size_w = original_wf.size[0];
    let original_audio_ptr = Arc::as_ptr(&original_wf.audio);
    app.waveforms.insert(id, original_wf);
    app.audio_clips.insert(id, make_audio_clip_with_samples(num_samples));

    // Split roughly in the middle (same coords as test_split_waveform_inserts_both_halves)
    app.mouse_pos = [200.0, 90.0];
    app.split_sample_at_cursor();

    assert_eq!(app.waveforms.len(), 2);

    let left = &app.waveforms[&id];
    let right_id = *app.waveforms.keys().find(|&&k| k != id).unwrap();
    let right = &app.waveforms[&right_id];

    // Both halves share the same full audio data
    assert!(
        Arc::ptr_eq(&left.audio, &right.audio),
        "both halves should share the same Arc<AudioData>"
    );
    assert_eq!(
        Arc::as_ptr(&left.audio), original_audio_ptr,
        "audio data should be the original Arc"
    );

    // Full audio preserved in both halves
    assert_eq!(left.audio.left_samples.len(), num_samples);
    assert_eq!(right.audio.left_samples.len(), num_samples);
    let full_w_left = full_audio_width_px(left);
    let full_w_right = full_audio_width_px(right);
    assert_eq!(full_w_left, full_w_right, "both halves should report same full audio width");

    // Widths sum to original
    let width_sum = left.size[0] + right.size[0];
    assert!(
        (width_sum - original_size_w).abs() < 0.1,
        "left width {} + right width {} should equal original {}",
        left.size[0], right.size[0], original_size_w
    );

    // Right half has correct offset
    assert!(
        (right.sample_offset_px - (left.sample_offset_px + left.size[0])).abs() < 0.1,
        "right offset {} should equal left offset {} + left width {}",
        right.sample_offset_px, left.sample_offset_px, left.size[0]
    );

    // AudioClipData also shares full samples
    let left_clip = &app.audio_clips[&id];
    let right_clip = &app.audio_clips[&right_id];
    assert!(
        Arc::ptr_eq(&left_clip.samples, &right_clip.samples),
        "both audio clips should share the same sample data"
    );
    assert_eq!(left_clip.samples.len(), num_samples);
}

// ---- Duplicate tests ----

#[test]
fn test_duplicate_waveform_syncs_audio_clips() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(0.0, 0.0));
    app.audio_clips.insert(id, make_audio_clip());

    app.selected.push(HitTarget::Waveform(id));
    app.duplicate_selected();

    assert_eq!(app.waveforms.len(), 2);
    assert_eq!(app.audio_clips.len(), 2);
}

#[test]
fn test_duplicate_component_creates_new_waveforms() {
    let mut app = App::new_headless();

    // wf0 and wf1 owned by component
    let wf0_id = new_id();
    let wf1_id = new_id();
    app.waveforms.insert(wf0_id, make_waveform(0.0, 0.0));
    app.audio_clips.insert(wf0_id, make_audio_clip());
    app.waveforms.insert(wf1_id, make_waveform(0.0, 100.0));
    app.audio_clips.insert(wf1_id, make_audio_clip());

    let comp_id = new_id();
    app.components.insert(comp_id, component::ComponentDef {
        id: comp_id,
        name: "C".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 200.0],
        waveform_ids: vec![wf0_id, wf1_id],
    });

    app.selected.push(HitTarget::ComponentDef(comp_id));
    app.duplicate_selected();

    // Should have 2 components, 4 waveforms, 4 audio clips
    assert_eq!(app.components.len(), 2);
    assert_eq!(app.waveforms.len(), 4);
    assert_eq!(app.audio_clips.len(), 4);

    // New component's waveform ids should point to different waveforms
    let orig = app.components.get(&comp_id).unwrap();
    let new_comp = app.components.values().find(|c| c.id != comp_id).unwrap();
    assert_eq!(new_comp.waveform_ids.len(), 2);
    for wi in &new_comp.waveform_ids {
        assert!(
            !orig.waveform_ids.contains(wi),
            "new component's waveform ids should not overlap original"
        );
    }
}

// ---- Copy/Paste tests ----

#[test]
fn test_paste_waveform_adds_audio_clip() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(0.0, 0.0));
    app.audio_clips.insert(id, make_audio_clip());

    app.selected.push(HitTarget::Waveform(id));
    app.copy_selected();

    let wf_before = app.waveforms.len();
    let clip_before = app.audio_clips.len();

    app.paste_clipboard();

    assert_eq!(app.waveforms.len(), wf_before + 1);
    assert_eq!(app.audio_clips.len(), clip_before + 1);
}

#[test]
fn test_paste_component_creates_independent_copy() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));
    app.audio_clips.insert(wf_id, make_audio_clip());

    let comp_id = new_id();
    app.components.insert(comp_id, component::ComponentDef {
        id: comp_id,
        name: "C".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 80.0],
        waveform_ids: vec![wf_id],
    });

    app.selected.push(HitTarget::ComponentDef(comp_id));
    app.copy_selected();
    app.paste_clipboard();

    assert_eq!(app.components.len(), 2);
    // The pasted component should have a different id
    let ids: Vec<_> = app.components.values().map(|c| c.id).collect();
    assert_ne!(ids[0], ids[1]);
    // And different waveform ids
    let orig_wf_ids = &app.components.values().next().unwrap().waveform_ids;
    let new_wf_ids = &app.components.values().nth(1).unwrap().waveform_ids;
    for wi in new_wf_ids {
        assert!(
            !orig_wf_ids.contains(wi),
            "pasted component should not share waveform ids"
        );
    }
}

#[test]
fn rescale_camera_for_bpm_keeps_screen_center_stable() {
    let mut app = App::new_headless();
    app.camera.position = [100.0, 50.0];
    app.camera.zoom = 1.0;

    // screen_info() returns (1280, 800, 1.0) in headless mode
    let cx = 1280.0 / 2.0;
    let cy = 800.0 / 2.0;
    let world_center_before = app.camera.screen_to_world([cx, cy]);

    let old_bpm = 120.0_f32;
    let new_bpm = 180.0_f32;
    let scale = old_bpm / new_bpm;

    app.rescale_clip_positions(scale);
    app.rescale_camera_for_bpm(scale);
    app.bpm = new_bpm;

    let world_center_after = app.camera.screen_to_world([cx, cy]);

    // The screen center should now point to the rescaled version of the
    // original world center (i.e. world_center_before * scale).
    let expected = [world_center_before[0] * scale, world_center_before[1] * scale];
    assert!(
        (world_center_after[0] - expected[0]).abs() < 0.01,
        "x: got {} expected {}",
        world_center_after[0],
        expected[0]
    );
    assert!(
        (world_center_after[1] - expected[1]).abs() < 0.01,
        "y: got {} expected {}",
        world_center_after[1],
        expected[1]
    );
}

#[test]
fn test_auto_clip_fades_default_on() {
    let app = App::new_headless();
    assert!(
        app.settings.auto_clip_fades,
        "auto_clip_fades should default to true"
    );
    assert!(
        DEFAULT_AUTO_FADE_PX > 0.0,
        "DEFAULT_AUTO_FADE_PX should be positive"
    );

    // Logic: when enabled, fade = DEFAULT_AUTO_FADE_PX
    let fade_on =
        if app.settings.auto_clip_fades { DEFAULT_AUTO_FADE_PX } else { 0.0 };
    assert_eq!(fade_on, DEFAULT_AUTO_FADE_PX);

    // Logic: when disabled, fade = 0.0
    let mut app2 = App::new_headless();
    app2.settings.auto_clip_fades = false;
    let fade_off =
        if app2.settings.auto_clip_fades { DEFAULT_AUTO_FADE_PX } else { 0.0 };
    assert_eq!(fade_off, 0.0);
}

#[test]
fn test_buffer_size_default_and_options() {
    use crate::settings::BUFFER_SIZE_OPTIONS;

    let app = App::new_headless();
    assert_eq!(app.settings.buffer_size, 512, "buffer_size should default to 512");

    assert!(BUFFER_SIZE_OPTIONS.contains(&512), "512 must be a valid option");
    assert!(BUFFER_SIZE_OPTIONS.contains(&256), "256 must be a valid option");
    assert!(BUFFER_SIZE_OPTIONS.contains(&1024), "1024 must be a valid option");

    let mut app2 = App::new_headless();
    app2.settings.buffer_size = 256;
    assert_eq!(app2.settings.buffer_size, 256);
}
