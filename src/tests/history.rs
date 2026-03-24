use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::history::MAX_UNDO_HISTORY;
use crate::operations::Operation;
use crate::regions::LoopRegion;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks};
use crate::{App, CanvasObject, HitTarget, WaveformView};

fn make_object(x: f32, y: f32) -> CanvasObject {
    CanvasObject {
        position: [x, y],
        size: [100.0, 60.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    }
}

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

#[test]
fn test_undo_restores_objects() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_object(0.0, 0.0);
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });

    let id2 = new_id();
    let obj2 = make_object(100.0, 100.0);
    app.objects.insert(id2, obj2.clone());
    app.push_op(Operation::CreateObject { id: id2, data: obj2 });

    assert_eq!(app.objects.len(), 2);

    app.undo_op();
    assert_eq!(app.objects.len(), 1);

    app.undo_op();
    assert_eq!(app.objects.len(), 0);
}

#[test]
fn test_redo_after_undo() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_object(10.0, 20.0);
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });

    app.undo_op();
    assert!(app.objects.is_empty());

    app.redo_op();
    assert_eq!(app.objects.len(), 1);
}

#[test]
fn test_undo_empty_stack_noop() {
    let mut app = App::new_headless();
    let obj_count = app.objects.len();
    app.undo_op();
    assert_eq!(app.objects.len(), obj_count);
}

#[test]
fn test_redo_empty_stack_noop() {
    let mut app = App::new_headless();
    app.redo_op();
    assert!(app.objects.is_empty());
}

#[test]
fn test_push_clears_redo() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_object(0.0, 0.0);
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });

    app.undo_op();
    assert!(!app.op_redo_stack.is_empty());

    // A new push_op should clear the redo stack
    let id2 = new_id();
    let obj2 = make_object(1.0, 1.0);
    app.objects.insert(id2, obj2.clone());
    app.push_op(Operation::CreateObject { id: id2, data: obj2 });
    assert!(app.op_redo_stack.is_empty());
}

#[test]
fn test_max_history_limit() {
    let mut app = App::new_headless();
    for i in 0..55 {
        let id = new_id();
        let obj = make_object(i as f32, 0.0);
        app.objects.insert(id, obj.clone());
        app.push_op(Operation::CreateObject { id, data: obj });
    }
    assert!(app.op_undo_stack.len() <= MAX_UNDO_HISTORY);
}

#[test]
fn test_multiple_undo_redo_cycle() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let obj1 = make_object(0.0, 0.0);
    app.objects.insert(id1, obj1.clone());
    app.push_op(Operation::CreateObject { id: id1, data: obj1 });
    let state1 = app.objects.clone();

    let id2 = new_id();
    let obj2 = make_object(1.0, 1.0);
    app.objects.insert(id2, obj2.clone());
    app.push_op(Operation::CreateObject { id: id2, data: obj2 });
    let state2 = app.objects.clone();

    let id3 = new_id();
    let obj3 = make_object(2.0, 2.0);
    app.objects.insert(id3, obj3.clone());
    app.push_op(Operation::CreateObject { id: id3, data: obj3 });

    assert_eq!(app.objects.len(), 3);

    // Undo back
    app.undo_op();
    assert_eq!(app.objects, state2);

    app.undo_op();
    assert_eq!(app.objects, state1);

    app.undo_op();
    assert!(app.objects.is_empty());

    // Redo forward
    app.redo_op();
    assert_eq!(app.objects, state1);

    app.redo_op();
    assert_eq!(app.objects, state2);
}

#[test]
fn test_undo_clears_selection() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_object(0.0, 0.0);
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });

    app.selected.push(HitTarget::Object(id));
    assert!(!app.selected.is_empty());

    app.undo_op();
    assert!(app.selected.is_empty());
}

#[test]
fn test_undo_restores_waveform_positions() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let wf = make_waveform(50.0, 50.0);
    app.waveforms.insert(wf_id, wf.clone());
    app.audio_clips.insert(wf_id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    });

    // Move waveform via op
    let before = app.waveforms[&wf_id].clone();
    app.waveforms.get_mut(&wf_id).unwrap().position = [300.0, 400.0];
    let after = app.waveforms[&wf_id].clone();
    app.push_op(Operation::UpdateWaveform { id: wf_id, before, after });

    app.undo_op();
    assert_eq!(app.waveforms.get(&wf_id).unwrap().position, [50.0, 50.0]);
}

#[test]
fn test_undo_restores_loop_regions() {
    let mut app = App::new_headless();
    let id = new_id();
    let lr = LoopRegion {
        position: [100.0, 0.0],
        size: [200.0, 30.0],
        enabled: true,
    };
    app.loop_regions.insert(id, lr.clone());
    app.push_op(Operation::CreateLoopRegion { id, data: lr });
    assert_eq!(app.loop_regions.len(), 1);

    app.undo_op();
    assert!(app.loop_regions.is_empty());
}
