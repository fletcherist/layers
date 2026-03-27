use std::sync::Arc;
use crate::App;
use crate::entity_id::new_id;
use crate::takes::TakeGroup;
use crate::automation::AutomationData;
use crate::ui::waveform::{AudioData, WaveformPeaks, WaveformView, WarpMode, AudioClipData};

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

fn make_clip() -> AudioClipData {
    AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 1.0,
    }
}

#[test]
fn test_create_take_group() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 1,
        expanded: true,
    });
    parent.disabled = true; // parent inactive

    let child = make_waveform(100.0, 130.0); // below parent

    app.waveforms.insert(parent_id, parent);
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child_id, child);
    app.audio_clips.insert(child_id, make_clip());

    let tg = app.waveforms[&parent_id].take_group.as_ref().unwrap();
    assert_eq!(tg.take_count(), 2);
    assert_eq!(tg.active_index, 1);
    assert!(tg.contains(child_id));
    assert!(!tg.contains(parent_id)); // parent is NOT in take_ids

    // Parent should be disabled, child enabled
    assert!(app.waveforms[&parent_id].disabled);
    assert!(!app.waveforms[&child_id].disabled);
}

#[test]
fn test_switch_active_take() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child1_id = new_id();
    let child2_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child1_id, child2_id],
        active_index: 1, // child1 is active
        expanded: true,
    });
    parent.disabled = true;

    let child1 = make_waveform(100.0, 130.0);
    let mut child2 = make_waveform(100.0, 210.0);
    child2.disabled = true;

    app.waveforms.insert(parent_id, parent);
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child1_id, child1);
    app.audio_clips.insert(child1_id, make_clip());
    app.waveforms.insert(child2_id, child2);
    app.audio_clips.insert(child2_id, make_clip());

    // Switch to child2 (index 2)
    app.switch_active_take(parent_id, 2);

    assert_eq!(app.waveforms[&parent_id].take_group.as_ref().unwrap().active_index, 2);
    assert!(app.waveforms[&parent_id].disabled);  // parent still disabled
    assert!(app.waveforms[&child1_id].disabled);   // child1 now disabled
    assert!(!app.waveforms[&child2_id].disabled);  // child2 now active

    // Switch to parent (index 0)
    app.switch_active_take(parent_id, 0);

    assert_eq!(app.waveforms[&parent_id].take_group.as_ref().unwrap().active_index, 0);
    assert!(!app.waveforms[&parent_id].disabled); // parent active
    assert!(app.waveforms[&child1_id].disabled);  // child1 disabled
    assert!(app.waveforms[&child2_id].disabled);  // child2 disabled
}

#[test]
fn test_find_take_parent() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();
    let standalone_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 1,
        expanded: true,
    });

    app.waveforms.insert(parent_id, parent);
    app.waveforms.insert(child_id, make_waveform(100.0, 130.0));
    app.waveforms.insert(standalone_id, make_waveform(500.0, 50.0));

    assert_eq!(app.find_take_parent(child_id), Some(parent_id));
    assert_eq!(app.find_take_parent(standalone_id), None);
    assert_eq!(app.find_take_parent(parent_id), None); // parent is not a child
}

#[test]
fn test_toggle_take_expanded() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 1,
        expanded: true,
    });

    app.waveforms.insert(parent_id, parent);
    app.waveforms.insert(child_id, make_waveform(100.0, 130.0));

    assert!(app.waveforms[&parent_id].take_group.as_ref().unwrap().expanded);

    app.toggle_take_expanded(parent_id);
    assert!(!app.waveforms[&parent_id].take_group.as_ref().unwrap().expanded);

    app.toggle_take_expanded(parent_id);
    assert!(app.waveforms[&parent_id].take_group.as_ref().unwrap().expanded);

    // Undo should restore expanded state
    app.undo_op();
    assert!(!app.waveforms[&parent_id].take_group.as_ref().unwrap().expanded);
}

#[test]
fn test_switch_take_undo_redo() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 0, // parent is active
        expanded: true,
    });

    let mut child = make_waveform(100.0, 130.0);
    child.disabled = true;

    app.waveforms.insert(parent_id, parent);
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child_id, child);
    app.audio_clips.insert(child_id, make_clip());

    // Switch to child
    app.switch_active_take(parent_id, 1);
    assert_eq!(app.waveforms[&parent_id].take_group.as_ref().unwrap().active_index, 1);
    assert!(app.waveforms[&parent_id].disabled);
    assert!(!app.waveforms[&child_id].disabled);

    // Undo: should revert to parent active
    app.undo_op();
    assert_eq!(app.waveforms[&parent_id].take_group.as_ref().unwrap().active_index, 0);
    assert!(!app.waveforms[&parent_id].disabled);

    // Redo: child active again
    app.redo_op();
    assert_eq!(app.waveforms[&parent_id].take_group.as_ref().unwrap().active_index, 1);
    assert!(app.waveforms[&parent_id].disabled);
}

#[test]
fn test_take_group_serde_roundtrip() {
    let tg = TakeGroup {
        take_ids: vec![new_id(), new_id()],
        active_index: 1,
        expanded: false,
    };
    let json = serde_json::to_string(&tg).unwrap();
    let restored: TakeGroup = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.take_ids.len(), 2);
    assert_eq!(restored.active_index, 1);
    assert!(!restored.expanded);
}

#[test]
fn test_delete_child_take_updates_parent() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child1_id = new_id();
    let child2_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child1_id, child2_id],
        active_index: 2, // child2 is active
        expanded: true,
    });
    parent.disabled = true;

    let mut child1 = make_waveform(100.0, 130.0);
    child1.disabled = true;
    let child2 = make_waveform(100.0, 210.0);

    app.waveforms.insert(parent_id, parent.clone());
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child1_id, child1.clone());
    app.audio_clips.insert(child1_id, make_clip());
    app.waveforms.insert(child2_id, child2.clone());
    app.audio_clips.insert(child2_id, make_clip());

    // Delete child1 (inactive child, index 1)
    let op = crate::operations::Operation::DeleteWaveform {
        id: child1_id,
        data: child1,
        audio_clip: None,
    };
    crate::operations::Operation::apply(&op, &mut app);

    // child1 should be gone
    assert!(!app.waveforms.contains_key(&child1_id));
    // Parent's take_ids should only have child2
    let tg = app.waveforms[&parent_id].take_group.as_ref().unwrap();
    assert_eq!(tg.take_ids.len(), 1);
    assert_eq!(tg.take_ids[0], child2_id);
    // active_index was 2, now should be 1 (shifted down)
    assert_eq!(tg.active_index, 1);
}

#[test]
fn test_delete_active_child_falls_back_to_parent() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 1,
        expanded: true,
    });
    parent.disabled = true;

    let child = make_waveform(100.0, 130.0);

    app.waveforms.insert(parent_id, parent.clone());
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child_id, child.clone());
    app.audio_clips.insert(child_id, make_clip());

    // Delete the active child
    let op = crate::operations::Operation::DeleteWaveform {
        id: child_id,
        data: child,
        audio_clip: None,
    };
    crate::operations::Operation::apply(&op, &mut app);

    // Child gone, take_group removed entirely, parent re-enabled
    assert!(!app.waveforms.contains_key(&child_id));
    assert!(app.waveforms[&parent_id].take_group.is_none());
    assert!(!app.waveforms[&parent_id].disabled);
}

#[test]
fn test_delete_parent_deletes_children() {
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child1_id = new_id();
    let child2_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child1_id, child2_id],
        active_index: 1,
        expanded: true,
    });
    parent.disabled = true;

    app.waveforms.insert(parent_id, parent.clone());
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child1_id, make_waveform(100.0, 130.0));
    app.audio_clips.insert(child1_id, make_clip());
    app.waveforms.insert(child2_id, make_waveform(100.0, 210.0));
    app.audio_clips.insert(child2_id, make_clip());

    // Delete the parent
    let op = crate::operations::Operation::DeleteWaveform {
        id: parent_id,
        data: parent,
        audio_clip: None,
    };
    crate::operations::Operation::apply(&op, &mut app);

    // All three should be gone
    assert!(!app.waveforms.contains_key(&parent_id));
    assert!(!app.waveforms.contains_key(&child1_id));
    assert!(!app.waveforms.contains_key(&child2_id));
}

#[test]
fn test_find_take_parent_resolves_child() {
    // When a child take is selected, find_take_parent should resolve to the parent.
    // This is used during recording to avoid creating nested take groups.
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child_id],
        active_index: 1,
        expanded: true,
    });
    parent.disabled = true;

    let child = make_waveform(100.0, 130.0);

    app.waveforms.insert(parent_id, parent);
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child_id, child);
    app.audio_clips.insert(child_id, make_clip());

    // Child resolves to parent
    assert_eq!(app.find_take_parent(child_id), Some(parent_id));
    // Parent does not resolve (it IS the parent)
    assert_eq!(app.find_take_parent(parent_id), None);
    // So the recording logic: unwrap_or(*id) gives parent_id for child, parent_id for parent
    let resolved_from_child = app.find_take_parent(child_id).unwrap_or(child_id);
    assert_eq!(resolved_from_child, parent_id);
    let resolved_from_parent = app.find_take_parent(parent_id).unwrap_or(parent_id);
    assert_eq!(resolved_from_parent, parent_id);
}

#[test]
fn test_move_parent_moves_children() {
    use crate::HitTarget;
    let mut app = App::new_headless();
    let parent_id = new_id();
    let child1_id = new_id();
    let child2_id = new_id();

    let mut parent = make_waveform(100.0, 50.0);
    parent.take_group = Some(TakeGroup {
        take_ids: vec![child1_id, child2_id],
        active_index: 1,
        expanded: true,
    });
    parent.disabled = true;

    let child1 = make_waveform(100.0, 130.0);
    let mut child2 = make_waveform(100.0, 170.0);
    child2.disabled = true;

    app.waveforms.insert(parent_id, parent);
    app.audio_clips.insert(parent_id, make_clip());
    app.waveforms.insert(child1_id, child1);
    app.audio_clips.insert(child1_id, make_clip());
    app.waveforms.insert(child2_id, child2);
    app.audio_clips.insert(child2_id, make_clip());

    // Move parent by (+50, +20)
    app.set_target_pos(&HitTarget::Waveform(parent_id), [150.0, 70.0]);

    assert_eq!(app.waveforms[&parent_id].position, [150.0, 70.0]);
    assert_eq!(app.waveforms[&child1_id].position, [150.0, 150.0]); // 100+50, 130+20
    assert_eq!(app.waveforms[&child2_id].position, [150.0, 190.0]); // 100+50, 170+20
}
