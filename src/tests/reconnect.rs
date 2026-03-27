use std::sync::Arc;

use crate::entity_id::new_id;
use crate::network::NetworkMode;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::automation::AutomationData;
use crate::{App, CanvasObject, HitTarget};
use crate::ui::toast::{ToastKind, ToastManager};

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

#[test]
fn test_clear_entity_state_empties_all_maps() {
    let mut app = App::new_headless();

    // Insert entities into various maps
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));

    let obj_id = new_id();
    app.objects.insert(obj_id, CanvasObject {
        position: [50.0, 50.0],
        size: [100.0, 60.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 4.0,
    });

    let group_id = new_id();
    app.groups.insert(group_id, crate::group::Group {
        id: group_id,
        name: "Test Group".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 100.0],
        member_ids: vec![],
        effect_chain_id: None,
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });

    app.selected.push(HitTarget::Waveform(wf_id));
    app.applied_remote_seqs.insert((uuid::Uuid::new_v4(), 1));

    // Push onto undo stack
    app.op_undo_stack.push(crate::operations::CommittedOp {
        op: crate::operations::Operation::CreateObject {
            id: obj_id,
            data: CanvasObject {
                position: [50.0, 50.0],
                size: [100.0, 60.0],
                color: [1.0, 0.0, 0.0, 1.0],
                border_radius: 4.0,
            },
        },
        user_id: uuid::Uuid::new_v4(),
        timestamp_ms: 0,
        seq: 0,
        before_selection: vec![],
    });

    assert!(!app.waveforms.is_empty());
    assert!(!app.objects.is_empty());
    assert!(!app.groups.is_empty());
    assert!(!app.selected.is_empty());
    assert!(!app.applied_remote_seqs.is_empty());
    assert!(!app.op_undo_stack.is_empty());

    app.clear_entity_state();

    assert!(app.waveforms.is_empty());
    assert!(app.objects.is_empty());
    assert!(app.groups.is_empty());
    assert!(app.selected.is_empty());
    assert!(app.applied_remote_seqs.is_empty());
    assert!(app.op_undo_stack.is_empty());
    assert!(app.op_redo_stack.is_empty());
    assert!(app.midi_clips.is_empty());
    assert!(app.instruments.is_empty());
    assert!(app.effect_chains.is_empty());
    assert!(app.components.is_empty());
    assert!(app.component_instances.is_empty());
    assert!(app.export_regions.is_empty());
    assert!(app.loop_regions.is_empty());
    assert!(app.text_notes.is_empty());
    assert!(app.audio_clips.is_empty());
    assert!(app.source_audio_files.is_empty());
}

#[test]
fn test_clear_entity_state_clears_editing_state() {
    let mut app = App::new_headless();

    let id = new_id();
    app.editing_midi_clip = Some(id);
    app.editing_component = Some(id);
    app.editing_group = Some(id);
    app.editing_waveform_name = Some((id, "test".to_string()));
    app.solo_ids.insert(id);
    app.following_user = Some(uuid::Uuid::new_v4());

    app.clear_entity_state();

    assert!(app.editing_midi_clip.is_none());
    assert!(app.editing_component.is_none());
    assert!(app.editing_group.is_none());
    assert!(app.editing_waveform_name.is_none());
    assert!(app.editing_text_note.is_none());
    assert!(app.solo_ids.is_empty());
    assert!(app.following_user.is_none());
}

#[test]
fn test_persistent_toast_survives_tick() {
    let mut mgr = ToastManager::new();
    mgr.push_persistent("test", "Reconnecting…", ToastKind::Error);

    // Simulate time passing by ticking — persistent toast should survive
    mgr.tick();
    assert!(mgr.has_active());
    assert_eq!(mgr.toasts.len(), 1);
    assert_eq!(mgr.toasts[0].message, "Reconnecting…");

    // Dismiss it
    assert!(mgr.dismiss_by_id("test"));
    assert!(!mgr.has_active());
}

#[test]
fn test_persistent_toast_update_replaces() {
    let mut mgr = ToastManager::new();
    mgr.push_persistent("reconnecting", "Reconnecting 1/10…", ToastKind::Error);
    mgr.push_persistent("reconnecting", "Reconnecting 2/10…", ToastKind::Error);

    assert_eq!(mgr.toasts.len(), 1);
    assert_eq!(mgr.toasts[0].message, "Reconnecting 2/10…");
}

#[test]
fn test_dismiss_by_id_returns_false_when_missing() {
    let mut mgr = ToastManager::new();
    assert!(!mgr.dismiss_by_id("nonexistent"));
}

#[test]
fn test_disconnect_session_clears_connection_state() {
    let mut app = App::new_headless();

    // Simulate connected state
    app.connect_url = Some("ws://db.layers.audio".to_string());
    app.connect_project_id = Some("test-project".to_string());
    app.connect_password = Some("secret".to_string());
    app.reconnect_attempt = 3;
    app.following_user = Some(uuid::Uuid::new_v4());
    app.remote_users.insert(uuid::Uuid::new_v4(), crate::user::RemoteUserState {
        user: crate::user::User {
            id: uuid::Uuid::new_v4(),
            name: "Remote".to_string(),
            color: [1.0, 0.0, 0.0, 1.0],
        },
        cursor_world: None,
        drag_preview: None,
        online: true,
        viewport: None,
        playback: None,
        editing_plugin: None,
    });
    app.applied_remote_seqs.insert((uuid::Uuid::new_v4(), 1));

    // Set network to a non-offline state so disconnect_session doesn't early-return
    app.network.connection_state.set(NetworkMode::Connected);

    app.disconnect_session();

    assert_eq!(app.network.mode(), NetworkMode::Offline);
    assert!(app.connect_url.is_none());
    assert!(app.connect_project_id.is_none());
    assert!(app.connect_password.is_none());
    assert_eq!(app.reconnect_attempt, 0);
    assert!(app.last_reconnect_time.is_none());
    assert!(app.remote_users.is_empty());
    assert!(app.applied_remote_seqs.is_empty());
    assert!(app.following_user.is_none());
}
