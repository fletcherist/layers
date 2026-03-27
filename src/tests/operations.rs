use std::sync::Arc;

use crate::audio::AudioClipData;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::midi::{MidiClip, MidiNote, MIDI_CLIP_DEFAULT_PITCH_RANGE};
use crate::operations::{commit_op, commit_op_as, Operation};
use crate::regions::LoopRegion;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks};
use crate::{App, CanvasObject, WaveformView};

#[test]
fn test_operation_invert_create_delete() {
    let id = new_id();
    let obj = CanvasObject {
        position: [10.0, 20.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 5.0,
    };
    let create = Operation::CreateObject { id, data: obj.clone() };
    let inverted = create.invert();
    match &inverted {
        Operation::DeleteObject { id: del_id, data } => {
            assert_eq!(*del_id, id);
            assert_eq!(data.position, obj.position);
        }
        _ => panic!("Expected DeleteObject"),
    }
    // Double invert should return to original
    let double = inverted.invert();
    match &double {
        Operation::CreateObject { id: create_id, .. } => {
            assert_eq!(*create_id, id);
        }
        _ => panic!("Expected CreateObject"),
    }
}

#[test]
fn test_operation_invert_update_swaps_before_after() {
    let id = new_id();
    let before = CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 5.0,
    };
    let after = CanvasObject {
        position: [50.0, 50.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 5.0,
    };
    let op = Operation::UpdateObject { id, before: before.clone(), after: after.clone() };
    let inverted = op.invert();
    match &inverted {
        Operation::UpdateObject { before: inv_before, after: inv_after, .. } => {
            assert_eq!(inv_before.position, after.position);
            assert_eq!(inv_after.position, before.position);
        }
        _ => panic!("Expected UpdateObject"),
    }
}

#[test]
fn test_operation_invert_batch() {
    let id1 = new_id();
    let id2 = new_id();
    let obj = CanvasObject {
        position: [0.0, 0.0],
        size: [10.0, 10.0],
        color: [1.0; 4],
        border_radius: 0.0,
    };
    let batch = Operation::Batch(vec![
        Operation::CreateObject { id: id1, data: obj.clone() },
        Operation::CreateObject { id: id2, data: obj.clone() },
    ]);
    let inverted = batch.invert();
    match &inverted {
        Operation::Batch(ops) => {
            assert_eq!(ops.len(), 2);
            // Batch invert reverses order
            match &ops[0] {
                Operation::DeleteObject { id, .. } => assert_eq!(*id, id2),
                _ => panic!("Expected DeleteObject for id2"),
            }
            match &ops[1] {
                Operation::DeleteObject { id, .. } => assert_eq!(*id, id1),
                _ => panic!("Expected DeleteObject for id1"),
            }
        }
        _ => panic!("Expected Batch"),
    }
}

#[test]
fn test_committed_op_has_unique_seq() {
    let id = new_id();
    let obj = CanvasObject {
        position: [0.0; 2],
        size: [10.0; 2],
        color: [1.0; 4],
        border_radius: 0.0,
    };
    let op1 = commit_op(Operation::CreateObject { id, data: obj.clone() });
    let op2 = commit_op(Operation::CreateObject { id, data: obj });
    assert!(op2.seq > op1.seq);
}

#[test]
fn test_set_bpm_invert() {
    let op = Operation::SetBpm { before: 120.0, after: 140.0 };
    let inverted = op.invert();
    match &inverted {
        Operation::SetBpm { before, after } => {
            assert_eq!(*before, 140.0);
            assert_eq!(*after, 120.0);
        }
        _ => panic!("Expected SetBpm"),
    }
}

// ---------------------------------------------------------------------------
// Operation::apply() tests
// ---------------------------------------------------------------------------

fn make_obj(x: f32, y: f32) -> CanvasObject {
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
fn test_apply_create_object() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(10.0, 20.0);
    let op = Operation::CreateObject { id, data: obj.clone() };
    op.apply(&mut app);
    assert_eq!(app.objects.len(), 1);
    assert_eq!(app.objects[&id].position, [10.0, 20.0]);
}

#[test]
fn test_apply_delete_object() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(10.0, 20.0);
    app.objects.insert(id, obj.clone());
    let op = Operation::DeleteObject { id, data: obj };
    op.apply(&mut app);
    assert!(app.objects.is_empty());
}

#[test]
fn test_apply_update_object() {
    let mut app = App::new_headless();
    let id = new_id();
    let before = make_obj(0.0, 0.0);
    let after = make_obj(50.0, 50.0);
    app.objects.insert(id, before.clone());
    let op = Operation::UpdateObject { id, before, after: after.clone() };
    op.apply(&mut app);
    assert_eq!(app.objects[&id].position, [50.0, 50.0]);
}

#[test]
fn test_apply_create_waveform_with_audio_clip() {
    let mut app = App::new_headless();
    let id = new_id();
    let wf = make_waveform(100.0, 200.0);
    let ac = AudioClipData {
        samples: Arc::new(vec![0.0; 100]),
        sample_rate: 48000,
        duration_secs: 1.0,
    };
    let op = Operation::CreateWaveform { id, data: wf, audio_clip: Some((id, ac)) };
    op.apply(&mut app);
    assert_eq!(app.waveforms.len(), 1);
    assert_eq!(app.audio_clips.len(), 1);
}

#[test]
fn test_apply_delete_waveform() {
    let mut app = App::new_headless();
    let id = new_id();
    app.waveforms.insert(id, make_waveform(0.0, 0.0));
    app.audio_clips.insert(id, AudioClipData {
        samples: Arc::new(Vec::new()),
        sample_rate: 48000,
        duration_secs: 0.0,
    });
    let op = Operation::DeleteWaveform { id, data: make_waveform(0.0, 0.0), audio_clip: None };
    op.apply(&mut app);
    assert!(app.waveforms.is_empty());
    assert!(app.audio_clips.is_empty());
}

#[test]
fn test_apply_set_bpm() {
    let mut app = App::new_headless();
    assert_eq!(app.bpm, 120.0);
    let op = Operation::SetBpm { before: 120.0, after: 140.0 };
    op.apply(&mut app);
    assert_eq!(app.bpm, 140.0);
}

#[test]
fn test_apply_create_loop_region() {
    let mut app = App::new_headless();
    let id = new_id();
    let lr = LoopRegion { position: [100.0, 0.0], size: [200.0, 30.0], enabled: true };
    let op = Operation::CreateLoopRegion { id, data: lr };
    op.apply(&mut app);
    assert_eq!(app.loop_regions.len(), 1);
    assert!(app.loop_regions[&id].enabled);
}

#[test]
fn test_apply_batch() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();
    let op = Operation::Batch(vec![
        Operation::CreateObject { id: id1, data: make_obj(0.0, 0.0) },
        Operation::CreateObject { id: id2, data: make_obj(10.0, 10.0) },
    ]);
    op.apply(&mut app);
    assert_eq!(app.objects.len(), 2);
}

// ---------------------------------------------------------------------------
// Op-based undo/redo tests
// ---------------------------------------------------------------------------

#[test]
fn test_push_op_and_undo_op() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(10.0, 20.0);
    // Apply the operation manually, then push
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });
    assert_eq!(app.objects.len(), 1);
    assert_eq!(app.op_undo_stack.len(), 1);

    // Undo should remove the object
    app.undo_op();
    assert!(app.objects.is_empty());
    assert_eq!(app.op_redo_stack.len(), 1);
}

#[test]
fn test_redo_op_after_undo_op() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(10.0, 20.0);
    app.objects.insert(id, obj.clone());
    app.push_op(Operation::CreateObject { id, data: obj });

    app.undo_op();
    assert!(app.objects.is_empty());

    app.redo_op();
    assert_eq!(app.objects.len(), 1);
    assert_eq!(app.objects[&id].position, [10.0, 20.0]);
}

#[test]
fn test_push_op_clears_redo() {
    let mut app = App::new_headless();
    let id1 = new_id();
    let id2 = new_id();
    let obj = make_obj(0.0, 0.0);

    app.objects.insert(id1, obj.clone());
    app.push_op(Operation::CreateObject { id: id1, data: obj.clone() });

    app.undo_op();
    assert!(!app.op_redo_stack.is_empty());

    // New push should clear redo
    app.objects.insert(id2, obj.clone());
    app.push_op(Operation::CreateObject { id: id2, data: obj });
    assert!(app.op_redo_stack.is_empty());
}

#[test]
fn test_op_undo_redo_bpm_cycle() {
    let mut app = App::new_headless();
    assert_eq!(app.bpm, 120.0);

    app.bpm = 140.0;
    app.push_op(Operation::SetBpm { before: 120.0, after: 140.0 });
    assert_eq!(app.bpm, 140.0);

    app.undo_op();
    assert_eq!(app.bpm, 120.0);

    app.redo_op();
    assert_eq!(app.bpm, 140.0);
}

#[test]
fn test_op_undo_multiple_objects() {
    let mut app = App::new_headless();

    let id1 = new_id();
    app.objects.insert(id1, make_obj(0.0, 0.0));
    app.push_op(Operation::CreateObject { id: id1, data: make_obj(0.0, 0.0) });

    let id2 = new_id();
    app.objects.insert(id2, make_obj(10.0, 10.0));
    app.push_op(Operation::CreateObject { id: id2, data: make_obj(10.0, 10.0) });

    assert_eq!(app.objects.len(), 2);

    app.undo_op();
    assert_eq!(app.objects.len(), 1);
    assert!(app.objects.contains_key(&id1));

    app.undo_op();
    assert!(app.objects.is_empty());

    app.redo_op();
    assert_eq!(app.objects.len(), 1);
    app.redo_op();
    assert_eq!(app.objects.len(), 2);
}

// ---------------------------------------------------------------------------
// Network integration test (using channels)
// ---------------------------------------------------------------------------

#[test]
fn test_apply_remote_op() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(42.0, 42.0);
    let committed = commit_op(Operation::CreateObject { id, data: obj });
    app.apply_remote_op(committed);
    assert_eq!(app.objects.len(), 1);
    assert_eq!(app.objects[&id].position, [42.0, 42.0]);
    // Remote ops should NOT be in local undo stack
    assert!(app.op_undo_stack.is_empty());
}

#[test]
fn test_network_manager_offline_noop() {
    // Offline manager should not send or receive anything
    let mut mgr = crate::network::NetworkManager::new_offline();
    assert!(!mgr.is_connected());
    assert!(mgr.poll_ops().is_empty());
    assert!(mgr.poll_ephemeral().is_empty());
}

#[test]
fn test_network_manager_connected_roundtrip() {
    let (mut mgr, remote_op_tx, mut remote_op_rx, remote_eph_tx, mut remote_eph_rx) =
        crate::network::NetworkManager::new_connected();
    // Simulate the surreal_client setting Connected after Welcome
    mgr.connection_state.set(crate::network::NetworkMode::Connected);
    assert!(mgr.is_connected());

    // Simulate sending an op from local
    let id = new_id();
    let committed = commit_op(Operation::CreateObject { id, data: make_obj(1.0, 2.0) });
    mgr.send_op(committed.clone());
    // The op should appear on the remote side
    let received = remote_op_rx.try_recv().unwrap();
    assert_eq!(received.seq, committed.seq);

    // Simulate receiving an op from remote
    let remote_id = new_id();
    let remote_committed = commit_op(Operation::CreateObject { id: remote_id, data: make_obj(3.0, 4.0) });
    remote_op_tx.send(remote_committed).unwrap();
    let polled = mgr.poll_ops();
    assert_eq!(polled.len(), 1);

    // Simulate ephemeral message roundtrip
    let user_id = new_id();
    remote_eph_tx.send(crate::user::EphemeralMessage::CursorMove {
        user_id,
        position: [100.0, 200.0],
    }).unwrap();
    let eph = mgr.poll_ephemeral();
    assert_eq!(eph.len(), 1);
}

#[test]
fn test_remote_user_cursor_update_via_ephemeral() {
    let mut app = App::new_headless();
    let user_id = new_id();

    // Add a remote user
    app.remote_users.insert(user_id, crate::user::RemoteUserState {
        user: crate::user::User {
            id: user_id,
            name: "Alice".to_string(),
            color: crate::user::USER_COLORS[1],
        },
        cursor_world: None,
        drag_preview: None,
        online: true,
        viewport: None,
        playback: None,
    });

    // Simulate receiving a cursor move
    let state = app.remote_users.get_mut(&user_id).unwrap();
    state.cursor_world = Some([150.0, 250.0]);

    assert_eq!(app.remote_users[&user_id].cursor_world, Some([150.0, 250.0]));
}

#[test]
fn test_apply_update_waveform() {
    let mut app = App::new_headless();
    let id = new_id();
    let before = make_waveform(0.0, 0.0);
    let mut after = make_waveform(0.0, 0.0);
    after.position = [50.0, 50.0];
    after.volume = 0.5;

    app.waveforms.insert(id, before.clone());
    let op = Operation::UpdateWaveform { id, before, after: after.clone() };
    op.apply(&mut app);
    assert_eq!(app.waveforms[&id].position, [50.0, 50.0]);
    assert_eq!(app.waveforms[&id].volume, 0.5);
}

#[test]
fn test_apply_create_delete_midi_clip() {
    let mut app = App::new_headless();
    let id = new_id();
    let clip = MidiClip {
        position: [100.0, 50.0],
        size: [200.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![MidiNote {
            pitch: 60,
            start_px: 0.0,
            duration_px: 30.0,
            velocity: 100,
        }],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 0.0,
        instrument_id: None, disabled: false,
    };
    Operation::CreateMidiClip { id, data: clip.clone() }.apply(&mut app);
    assert_eq!(app.midi_clips.len(), 1);
    assert_eq!(app.midi_clips[&id].notes.len(), 1);

    Operation::DeleteMidiClip { id, data: clip }.apply(&mut app);
    assert!(app.midi_clips.is_empty());
}

// ---------------------------------------------------------------------------
// Serde roundtrip tests
// ---------------------------------------------------------------------------

fn serde_roundtrip_op(op: Operation) {
    let committed = commit_op(op);
    let json = serde_json::to_string(&committed).expect("serialize failed");
    let deserialized: crate::operations::CommittedOp =
        serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(deserialized.seq, committed.seq);
    assert_eq!(deserialized.user_id, committed.user_id);
    assert_eq!(deserialized.timestamp_ms, committed.timestamp_ms);
}

#[test]
fn test_serde_roundtrip_create_object() {
    serde_roundtrip_op(Operation::CreateObject {
        id: new_id(),
        data: make_obj(10.0, 20.0),
    });
}

#[test]
fn test_serde_roundtrip_update_object() {
    let id = new_id();
    serde_roundtrip_op(Operation::UpdateObject {
        id,
        before: make_obj(0.0, 0.0),
        after: make_obj(50.0, 50.0),
    });
}

#[test]
fn test_serde_roundtrip_create_waveform() {
    let id = new_id();
    let wf = make_waveform(100.0, 200.0);
    let ac = AudioClipData {
        samples: Arc::new(vec![0.0; 10]),
        sample_rate: 48000,
        duration_secs: 1.0,
    };
    serde_roundtrip_op(Operation::CreateWaveform {
        id,
        data: wf,
        audio_clip: Some((id, ac)),
    });
}

#[test]
fn test_serde_roundtrip_create_midi_clip() {
    serde_roundtrip_op(Operation::CreateMidiClip {
        id: new_id(),
        data: MidiClip {
            position: [100.0, 50.0],
            size: [200.0, 100.0],
            color: [0.5, 0.5, 0.5, 1.0],
            notes: vec![MidiNote {
                pitch: 60,
                start_px: 0.0,
                duration_px: 30.0,
                velocity: 100,
            }],
            pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
            grid_mode: crate::settings::GridMode::default(),
            triplet_grid: false,
            velocity_lane_height: 40.0,
            instrument_id: None, disabled: false,
        },
    });
}

#[test]
fn test_serde_roundtrip_create_midi_note() {
    serde_roundtrip_op(Operation::CreateMidiNote {
        clip_id: new_id(),
        note_idx: 0,
        data: MidiNote {
            pitch: 72,
            start_px: 10.0,
            duration_px: 20.0,
            velocity: 80,
        },
    });
}

#[test]
fn test_serde_roundtrip_loop_region() {
    serde_roundtrip_op(Operation::CreateLoopRegion {
        id: new_id(),
        data: LoopRegion {
            position: [100.0, 0.0],
            size: [200.0, 30.0],
            enabled: true,
        },
    });
}

#[test]
fn test_serde_roundtrip_export_region() {
    serde_roundtrip_op(Operation::CreateExportRegion {
        id: new_id(),
        data: crate::regions::ExportRegion {
            position: [0.0, 0.0],
            size: [800.0, 300.0],
        },
    });
}

#[test]
fn test_serde_roundtrip_component() {
    serde_roundtrip_op(Operation::CreateComponent {
        id: new_id(),
        data: crate::component::ComponentDef {
            id: new_id(),
            name: "Test Component".to_string(),
            position: [0.0, 0.0],
            size: [200.0, 100.0],
            waveform_ids: vec![new_id(), new_id()],
        },
    });
}

#[test]
fn test_serde_roundtrip_component_instance() {
    serde_roundtrip_op(Operation::CreateComponentInstance {
        id: new_id(),
        data: crate::component::ComponentInstance {
            component_id: new_id(),
            position: [50.0, 50.0],
        },
    });
}

#[test]
fn test_serde_roundtrip_instrument() {
    serde_roundtrip_op(Operation::CreateInstrument {
        id: new_id(),
        data: crate::instruments::InstrumentSnapshot {
            name: "Piano".to_string(),
            plugin_id: "com.example.piano".to_string(),
            plugin_name: "Piano VST".to_string(),
            plugin_path: std::path::PathBuf::from("/Library/Audio/Plug-Ins/VST3/Piano.vst3"),
            volume: 1.0,
            pan: 0.5,
            effect_chain_id: None,
            disabled: false,
        },
    });
}

#[test]
fn test_serde_roundtrip_set_bpm() {
    serde_roundtrip_op(Operation::SetBpm {
        before: 120.0,
        after: 140.0,
    });
}

#[test]
fn test_serde_roundtrip_batch() {
    serde_roundtrip_op(Operation::Batch(vec![
        Operation::CreateObject {
            id: new_id(),
            data: make_obj(0.0, 0.0),
        },
        Operation::SetBpm {
            before: 120.0,
            after: 90.0,
        },
    ]));
}

// ---------------------------------------------------------------------------
// Op JSON serialization roundtrip tests (used by surreal_client)
// ---------------------------------------------------------------------------

#[test]
fn test_committed_op_json_roundtrip() {
    let op = commit_op(Operation::CreateObject {
        id: new_id(),
        data: make_obj(1.0, 2.0),
    });

    // Simulate surreal_client's serialization: op → JSON string → back
    let op_json = serde_json::to_string(&op.op).unwrap();
    let deserialized: Operation = serde_json::from_str(&op_json).unwrap();
    assert_eq!(deserialized.variant_name(), "CreateObject");

    let bpm_op = commit_op(Operation::SetBpm {
        before: 120.0,
        after: 140.0,
    });
    let json = serde_json::to_string(&bpm_op.op).unwrap();
    let deserialized: Operation = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.variant_name(), "SetBpm");
}

#[test]
fn test_ephemeral_message_json_roundtrip() {
    let cursor_msg = crate::user::EphemeralMessage::CursorMove {
        user_id: new_id(),
        position: [100.0, 200.0],
    };
    let json = serde_json::to_string(&cursor_msg).unwrap();
    let _: crate::user::EphemeralMessage = serde_json::from_str(&json).unwrap();
}

// ---------------------------------------------------------------------------
// Two-app integration test (channel-based, no actual WebSocket)
// ---------------------------------------------------------------------------

#[test]
fn test_two_apps_sync_via_channels() {
    let mut app_a = App::new_headless();
    let mut app_b = App::new_headless();

    // Create connected network managers for both apps
    let (mgr_a, a_remote_op_tx, mut a_remote_op_rx, a_remote_eph_tx, mut _a_remote_eph_rx) =
        crate::network::NetworkManager::new_connected();
    let (mgr_b, b_remote_op_tx, mut b_remote_op_rx, b_remote_eph_tx, mut _b_remote_eph_rx) =
        crate::network::NetworkManager::new_connected();

    mgr_a.connection_state.set(crate::network::NetworkMode::Connected);
    mgr_b.connection_state.set(crate::network::NetworkMode::Connected);
    app_a.network = mgr_a;
    app_b.network = mgr_b;

    // App A creates an object
    let id = new_id();
    let obj = make_obj(42.0, 42.0);
    app_a.objects.insert(id, obj.clone());
    app_a.push_op(Operation::CreateObject { id, data: obj });

    // Simulate relay: read from A's outbound, send to B's inbound
    let op_from_a = a_remote_op_rx.try_recv().unwrap();
    b_remote_op_tx.send(op_from_a).unwrap();

    // App B polls and applies
    let remote_ops = app_b.network.poll_ops();
    assert_eq!(remote_ops.len(), 1);
    for committed in remote_ops {
        app_b.apply_remote_op(committed);
    }

    // Verify both apps have the same object
    assert_eq!(app_a.objects.len(), 1);
    assert_eq!(app_b.objects.len(), 1);
    assert_eq!(app_b.objects[&id].position, [42.0, 42.0]);

    // App B changes BPM
    app_b.bpm = 140.0;
    app_b.push_op(Operation::SetBpm { before: 120.0, after: 140.0 });

    // Relay B → A
    let op_from_b = b_remote_op_rx.try_recv().unwrap();
    a_remote_op_tx.send(op_from_b).unwrap();

    let remote_ops = app_a.network.poll_ops();
    for committed in remote_ops {
        app_a.apply_remote_op(committed);
    }

    assert_eq!(app_a.bpm, 140.0);
    assert_eq!(app_b.bpm, 140.0);
}

// ---------------------------------------------------------------------------
// Phase 7: Robustness tests
// ---------------------------------------------------------------------------

#[test]
fn test_op_deduplication() {
    let mut app = App::new_headless();
    let id = new_id();
    let obj = make_obj(42.0, 42.0);
    let committed = commit_op(Operation::CreateObject { id, data: obj });

    // Apply same op twice
    app.apply_remote_op(committed.clone());
    app.apply_remote_op(committed);

    // Should only have been applied once
    assert_eq!(app.objects.len(), 1);
}

#[test]
fn test_user_left_removes_from_map() {
    let mut app = App::new_headless();
    let user_id = new_id();

    // Add a remote user
    app.remote_users.insert(user_id, crate::user::RemoteUserState {
        user: crate::user::User {
            id: user_id,
            name: "Alice".to_string(),
            color: crate::user::USER_COLORS[1],
        },
        cursor_world: Some([100.0, 200.0]),
        drag_preview: None,
        online: true,
        viewport: None,
        playback: None,
    });
    assert_eq!(app.remote_users.len(), 1);

    // Simulate UserLeft
    app.remote_users.remove(&user_id);
    assert!(app.remote_users.is_empty());
}

#[test]
fn test_network_mode_transitions() {
    use crate::network::{NetworkMode, SharedConnectionState};

    let state = SharedConnectionState::new(NetworkMode::Offline);
    assert_eq!(state.get(), NetworkMode::Offline);

    state.set(NetworkMode::Connecting);
    assert_eq!(state.get(), NetworkMode::Connecting);

    state.set(NetworkMode::Connected);
    assert_eq!(state.get(), NetworkMode::Connected);

    state.set(NetworkMode::Disconnected);
    assert_eq!(state.get(), NetworkMode::Disconnected);
}

// ---------------------------------------------------------------------------
// Serde roundtrip: UpdateMidiClip
// ---------------------------------------------------------------------------

#[test]
fn test_serde_roundtrip_update_midi_clip() {
    let id = new_id();
    let before = MidiClip {
        position: [100.0, 50.0],
        size: [200.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![MidiNote {
            pitch: 60,
            start_px: 0.0,
            duration_px: 30.0,
            velocity: 100,
        }],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 40.0,
        instrument_id: None, disabled: false,
    };
    let mut after = before.clone();
    after.position = [300.0, 150.0];

    serde_roundtrip_op(Operation::UpdateMidiClip { id, before, after });
}

// ---------------------------------------------------------------------------
// Two-app MIDI clip move sync test
// ---------------------------------------------------------------------------

#[test]
fn test_two_apps_midi_clip_move_sync() {
    let mut app_a = App::new_headless();
    let mut app_b = App::new_headless();

    // Create connected network managers for both apps
    let (mgr_a, _a_remote_op_tx, mut a_remote_op_rx, _a_remote_eph_tx, mut _a_remote_eph_rx) =
        crate::network::NetworkManager::new_connected();
    let (mgr_b, b_remote_op_tx, mut _b_remote_op_rx, _b_remote_eph_tx, mut _b_remote_eph_rx) =
        crate::network::NetworkManager::new_connected();

    mgr_a.connection_state.set(crate::network::NetworkMode::Connected);
    mgr_b.connection_state.set(crate::network::NetworkMode::Connected);
    app_a.network = mgr_a;
    app_b.network = mgr_b;

    // Step 1: App A creates a MIDI clip
    let clip_id = new_id();
    let clip = MidiClip {
        position: [100.0, 50.0],
        size: [200.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![MidiNote {
            pitch: 60,
            start_px: 0.0,
            duration_px: 30.0,
            velocity: 100,
        }],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 0.0,
        instrument_id: None, disabled: false,
    };
    app_a.midi_clips.insert(clip_id, clip.clone());
    app_a.push_op(Operation::CreateMidiClip { id: clip_id, data: clip.clone() });

    // Relay A → B
    let op_from_a = a_remote_op_rx.try_recv().unwrap();
    b_remote_op_tx.send(op_from_a).unwrap();
    let remote_ops = app_b.network.poll_ops();
    assert_eq!(remote_ops.len(), 1);
    for committed in remote_ops {
        app_b.apply_remote_op(committed);
    }

    // Verify B has the clip at original position
    assert_eq!(app_b.midi_clips.len(), 1);
    assert_eq!(app_b.midi_clips[&clip_id].position, [100.0, 50.0]);

    // Step 2: App A moves the clip (simulating drag end)
    let before = clip.clone();
    let mut after = clip.clone();
    after.position = [300.0, 150.0];
    app_a.midi_clips.insert(clip_id, after.clone());
    app_a.push_op(Operation::UpdateMidiClip { id: clip_id, before, after: after.clone() });

    // Relay A → B
    let op_from_a = a_remote_op_rx.try_recv().unwrap();
    b_remote_op_tx.send(op_from_a).unwrap();
    let remote_ops = app_b.network.poll_ops();
    assert_eq!(remote_ops.len(), 1);
    for committed in remote_ops {
        app_b.apply_remote_op(committed);
    }

    // Verify B's clip has the new position
    assert_eq!(app_b.midi_clips[&clip_id].position, [300.0, 150.0]);
    assert_eq!(app_b.midi_clips[&clip_id].notes.len(), 1);
}

// ---------------------------------------------------------------------------
// Test SurrealDB op JSON roundtrip for UpdateMidiClip
// ---------------------------------------------------------------------------

#[test]
fn test_surreal_json_roundtrip_update_midi_clip() {
    let clip_id = new_id();
    let before = MidiClip {
        position: [100.0, 50.0],
        size: [200.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![MidiNote {
            pitch: 60,
            start_px: 0.0,
            duration_px: 30.0,
            velocity: 100,
        }],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 0.0,
        instrument_id: None, disabled: false,
    };
    let mut after = before.clone();
    after.position = [300.0, 150.0];

    let committed = commit_op(Operation::UpdateMidiClip {
        id: clip_id,
        before,
        after,
    });

    // Simulate surreal_client: serialize op to JSON string, then back
    let op_json = serde_json::to_string(&committed.op).unwrap();
    let deserialized: Operation = serde_json::from_str(&op_json).unwrap();

    match &deserialized {
        Operation::UpdateMidiClip { id, after, .. } => {
            assert_eq!(*id, clip_id);
            assert_eq!(after.position, [300.0, 150.0]);
        }
        other => panic!("Expected UpdateMidiClip, got {:?}", other.variant_name()),
    }
}

// ---------------------------------------------------------------------------
// BPM grid-sync tests
// ---------------------------------------------------------------------------

#[test]
fn test_set_bpm_rescales_waveform_positions() {
    let mut app = App::new_headless();
    assert_eq!(app.bpm, 120.0);

    // At 120 BPM: pixels_per_beat = PIXELS_PER_SECOND*60/120 = 60px.
    // Bar 1 starts at beat 0 (0px), bar 2 starts at beat 4 (240px).
    let id1 = new_id();
    let id2 = new_id();
    app.waveforms.insert(id1, make_waveform(0.0, 100.0));   // bar 1
    app.waveforms.insert(id2, make_waveform(240.0, 100.0)); // bar 2

    // Halve the BPM → scale = 120/60 = 2
    let op = Operation::SetBpm { before: 120.0, after: 60.0 };
    op.apply(&mut app);

    assert_eq!(app.bpm, 60.0);
    // Clip at bar 1 stays at 0
    assert_eq!(app.waveforms[&id1].position[0], 0.0);
    // Clip at bar 2 moves to 480px (bar 2 at 60 BPM = beat 4 * 120px/beat)
    assert_eq!(app.waveforms[&id2].position[0], 480.0);
    // Audio clip width should NOT be rescaled (audio length is fixed in seconds)
    assert_eq!(app.waveforms[&id2].size[0], 200.0);

    // Undo by applying the inverse: scale = 60/120 = 0.5
    op.invert().apply(&mut app);

    assert_eq!(app.bpm, 120.0);
    assert_eq!(app.waveforms[&id1].position[0], 0.0);
    assert_eq!(app.waveforms[&id2].position[0], 240.0);
}

#[test]
fn test_set_bpm_rescales_midi_clip_and_notes() {
    let mut app = App::new_headless();

    // Place a MIDI clip at bar 2 (240px at 120 BPM), width = 240px (4 bars).
    // It contains one note starting at 60px with duration 30px.
    let id = new_id();
    let clip = MidiClip {
        position: [240.0, 50.0],
        size: [240.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![MidiNote {
            pitch: 60,
            start_px: 60.0,
            duration_px: 30.0,
            velocity: 100,
        }],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 0.0,
        instrument_id: None, disabled: false,
    };
    app.midi_clips.insert(id, clip);

    // Double the BPM → scale = 120/240 = 0.5
    let op = Operation::SetBpm { before: 120.0, after: 240.0 };
    op.apply(&mut app);

    assert_eq!(app.bpm, 240.0);
    let mc = &app.midi_clips[&id];
    assert_eq!(mc.position[0], 120.0);  // 240 * 0.5
    assert_eq!(mc.size[0], 120.0);      // 240 * 0.5 (MIDI clips are beat-based)
    assert_eq!(mc.notes[0].start_px, 30.0);    // 60 * 0.5
    assert_eq!(mc.notes[0].duration_px, 15.0); // 30 * 0.5

    // Undo
    op.invert().apply(&mut app);
    let mc = &app.midi_clips[&id];
    assert_eq!(app.bpm, 120.0);
    assert_eq!(mc.position[0], 240.0);
    assert_eq!(mc.size[0], 240.0);
    assert_eq!(mc.notes[0].start_px, 60.0);
    assert_eq!(mc.notes[0].duration_px, 30.0);
}


#[test]
fn test_surreal_json_roundtrip_create_midi_clip() {
    let clip_id = new_id();
    let clip = MidiClip {
        position: [100.0, 50.0],
        size: [200.0, 100.0],
        color: [0.5, 0.5, 0.5, 1.0],
        notes: vec![],
        pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
        grid_mode: crate::settings::GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: 0.0,
        instrument_id: None, disabled: false,
    };

    let committed = commit_op(Operation::CreateMidiClip { id: clip_id, data: clip });

    let op_json = serde_json::to_string(&committed.op).unwrap();
    let deserialized: Operation = serde_json::from_str(&op_json).unwrap();

    match &deserialized {
        Operation::CreateMidiClip { id, data } => {
            assert_eq!(*id, clip_id);
            assert_eq!(data.position, [100.0, 50.0]);
        }
        other => panic!("Expected CreateMidiClip, got {:?}", other.variant_name()),
    }
}
