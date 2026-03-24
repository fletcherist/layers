use std::sync::Arc;
use crate::entity_id::new_id;
use crate::storage;
use crate::ui::palette::CommandAction;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::automation::AutomationData;
use crate::{App, CanvasObject, HitTarget};

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
fn create_group_from_selection() {
    let mut app = App::new_headless();

    // Add two objects
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select both
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));

    // Execute CreateGroup
    app.execute_command(CommandAction::CreateGroup);

    // Should have one group
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert_eq!(group.member_ids.len(), 2);
    assert!(group.member_ids.contains(&id1));
    assert!(group.member_ids.contains(&id2));

    // Selection should now be the group
    assert_eq!(app.selected.len(), 1);
    assert!(matches!(app.selected[0], HitTarget::Group(_)));
}

#[test]
fn ungroup_selected_restores_members() {
    let mut app = App::new_headless();

    // Add two objects
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select both and create group
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    // Now ungroup
    app.execute_command(CommandAction::UngroupSelected);

    // Group should be removed
    assert_eq!(app.groups.len(), 0);

    // Selection should contain the former members
    assert_eq!(app.selected.len(), 2);
    assert!(app.selected.contains(&HitTarget::Object(id1)));
    assert!(app.selected.contains(&HitTarget::Object(id2)));
}

#[test]
fn create_group_allows_single_item() {
    let mut app = App::new_headless();

    let id1 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select only one
    app.selected.push(HitTarget::Object(id1));
    app.execute_command(CommandAction::CreateGroup);

    // Group should be created with a single item
    assert_eq!(app.groups.len(), 1);
}

#[test]
fn select_group_opens_right_window() {
    let mut app = App::new_headless();

    // Add two objects and create a group
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();

    // Select the group and update right window
    app.selected.clear();
    app.selected.push(HitTarget::Group(group_id));
    app.update_right_window();

    // Right window should be open with Group target
    let rw = app.right_window.as_ref().expect("right window should be open");
    assert!(rw.is_group());
    assert_eq!(rw.target_id(), group_id);
    assert_eq!(rw.group_name, "Group 1");
    assert_eq!(rw.group_member_count, 2);
}

#[test]
fn rename_group_via_browser_inline_edit() {
    let mut app = App::new_headless();

    // Add two objects and create a group
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();
    assert_eq!(app.groups[&group_id].name, "Group 1");

    // Simulate inline rename: set editing state then commit
    app.sample_browser.editing_browser_name = Some((
        group_id,
        crate::layers::LayerNodeKind::Group,
        "My Custom Group".to_string(),
    ));

    // Commit by directly applying the same logic as Enter key handler
    let before = app.groups[&group_id].clone();
    app.groups.get_mut(&group_id).unwrap().name = "My Custom Group".to_string();
    let after = app.groups[&group_id].clone();
    app.push_op(crate::operations::Operation::UpdateGroup { id: group_id, before, after });
    app.sample_browser.editing_browser_name = None;

    assert_eq!(app.groups[&group_id].name, "My Custom Group");

    // Undo should revert to original name
    app.undo_op();
    assert_eq!(app.groups[&group_id].name, "Group 1");

    // Redo should restore the new name
    app.redo_op();
    assert_eq!(app.groups[&group_id].name, "My Custom Group");
}

#[test]
fn group_roundtrip_serialization() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [10.0, 20.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 30.0],
        size: [80.0, 60.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();
    let original = app.groups[&group_id].clone();

    // Roundtrip through storage
    let stored = storage::groups_to_stored(&app.groups);
    assert_eq!(stored.len(), 1);

    let restored = storage::groups_from_stored(stored);
    assert_eq!(restored.len(), 1);

    let restored_group = &restored[&group_id];
    assert_eq!(restored_group.id, original.id);
    assert_eq!(restored_group.name, original.name);
    assert_eq!(restored_group.position, original.position);
    assert_eq!(restored_group.size, original.size);
    assert_eq!(restored_group.member_ids, original.member_ids);
    assert_eq!(restored_group.effect_chain_id, original.effect_chain_id);
}

#[test]
fn normalize_group_selection_deduplicates_members() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let id2 = new_id();
    let id3 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id3, CanvasObject {
        position: [400.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 0.0, 1.0, 1.0],
        border_radius: 0.0,
    });

    // Create a group from id1 and id2
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    // Simulate a marquee that covers all three objects (two grouped, one free)
    let raw_targets = vec![
        HitTarget::Object(id1),
        HitTarget::Object(id2),
        HitTarget::Object(id3),
    ];
    let normalized = app.normalize_group_selection(raw_targets);

    // id1 and id2 should collapse into one HitTarget::Group, id3 stays as Object
    assert_eq!(normalized.len(), 2);
    assert!(normalized.iter().any(|t| matches!(t, HitTarget::Group(_))));
    assert!(normalized.contains(&HitTarget::Object(id3)));
}

#[test]
fn target_rect_returns_group_bounds() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [10.0, 20.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 30.0],
        size: [80.0, 60.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();
    let group = &app.groups[&group_id];

    let result = crate::ui::rendering::target_rect(
        &app.objects,
        &app.waveforms,
        &app.loop_regions,
        &app.export_regions,
        &app.components,
        &app.component_instances,
        &app.midi_clips,
        &app.text_notes,
        &app.groups,
        &HitTarget::Group(group_id),
    );

    let (pos, size) = result.expect("target_rect should return Some for groups");
    assert_eq!(pos, group.position);
    assert_eq!(size, group.size);
}

#[test]
fn add_effects_area_creates_group() {
    let mut app = App::new_headless();
    assert_eq!(app.groups.len(), 0);

    app.execute_command(CommandAction::AddEffectsArea);

    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert!(group.member_ids.is_empty());
    assert!(group.size[0] > 0.0);
    assert!(group.size[1] > 0.0);

    assert_eq!(app.selected.len(), 1);
    assert!(matches!(app.selected[0], HitTarget::Group(_)));
}

#[test]
fn group_volume_pan_defaults_and_update() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);

    let group_id = app.groups.keys().next().copied().unwrap();

    // Defaults
    assert!((app.groups[&group_id].volume - 1.0).abs() < f32::EPSILON);
    assert!((app.groups[&group_id].pan - 0.5).abs() < f32::EPSILON);

    // Right window reads group vol/pan
    app.selected.clear();
    app.selected.push(HitTarget::Group(group_id));
    app.update_right_window();
    let rw = app.right_window.as_ref().unwrap();
    assert!((rw.volume - 1.0).abs() < f32::EPSILON);
    assert!((rw.pan - 0.5).abs() < f32::EPSILON);

    // Mutate via UpdateGroup and undo
    let before = app.groups[&group_id].clone();
    app.groups.get_mut(&group_id).unwrap().volume = 0.5;
    app.groups.get_mut(&group_id).unwrap().pan = 0.75;
    let after = app.groups[&group_id].clone();
    app.push_op(crate::operations::Operation::UpdateGroup { id: group_id, before, after });

    assert!((app.groups[&group_id].volume - 0.5).abs() < f32::EPSILON);
    assert!((app.groups[&group_id].pan - 0.75).abs() < f32::EPSILON);

    app.undo_op();
    assert!((app.groups[&group_id].volume - 1.0).abs() < f32::EPSILON);
    assert!((app.groups[&group_id].pan - 0.5).abs() < f32::EPSILON);

    app.redo_op();
    assert!((app.groups[&group_id].volume - 0.5).abs() < f32::EPSILON);
    assert!((app.groups[&group_id].pan - 0.75).abs() < f32::EPSILON);
}

#[test]
fn group_volume_pan_roundtrip_serialization() {
    let mut app = App::new_headless();

    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);

    let group_id = app.groups.keys().next().copied().unwrap();
    app.groups.get_mut(&group_id).unwrap().volume = 0.3;
    app.groups.get_mut(&group_id).unwrap().pan = 0.8;

    let stored = storage::groups_to_stored(&app.groups);
    let restored = storage::groups_from_stored(stored);

    let rg = &restored[&group_id];
    assert!((rg.volume - 0.3).abs() < 1e-5);
    assert!((rg.pan - 0.8).abs() < 1e-5);
}

#[test]
fn group_bounds_include_instrument_midi_clips() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Set known position/size on the MIDI clip
    let mc = app.midi_clips.get_mut(&mc_id).unwrap();
    mc.position = [100.0, 200.0];
    mc.size = [300.0, 150.0];

    // Create a group containing the instrument
    let group_id = new_id();
    let group = crate::group::Group::new(
        group_id,
        "Test Group".to_string(),
        [0.0, 0.0],
        [10.0, 10.0],
        vec![inst_id],
    );
    app.groups.insert(group_id, group);

    // Recalculate bounds — should expand to encompass the instrument's MIDI clip
    app.update_group_bounds(group_id);

    let g = app.groups.get(&group_id).unwrap();
    assert!((g.position[0] - 100.0).abs() < 1e-3, "group x should match MIDI clip x");
    assert!((g.position[1] - 200.0).abs() < 1e-3, "group y should match MIDI clip y");
    assert!((g.size[0] - 300.0).abs() < 1e-3, "group width should match MIDI clip width");
    assert!((g.size[1] - 150.0).abs() < 1e-3, "group height should match MIDI clip height");
}

#[test]
fn instrument_inside_group_shows_midi_clip_children_in_layer_tree() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Create a group containing the instrument
    let group_id = new_id();
    let group = crate::group::Group::new(
        group_id, "G".to_string(), [0.0, 0.0], [10.0, 10.0], vec![inst_id],
    );
    app.groups.insert(group_id, group);

    // Sync and flatten the layer tree
    crate::layers::sync_tree(
        &mut app.layer_tree, &app.instruments, &app.midi_clips, &app.waveforms, &app.groups,
    );
    let rows = crate::layers::flatten_tree(
        &app.layer_tree, &app.instruments, &app.midi_clips, &app.waveforms, &app.groups,
    );

    // Should have: Group (depth 0) → Instrument (depth 1) → MIDI clip (depth 2)
    assert!(rows.len() >= 3, "expected at least 3 rows, got {}", rows.len());
    let group_row = rows.iter().find(|r| r.entity_id == group_id).expect("group row");
    let inst_row = rows.iter().find(|r| r.entity_id == inst_id).expect("instrument row");
    let mc_row = rows.iter().find(|r| r.entity_id == mc_id).expect("midi clip row");
    assert_eq!(group_row.depth, 0);
    assert_eq!(inst_row.depth, 1, "instrument should be indented under group");
    assert_eq!(mc_row.depth, 2, "midi clip should be indented under instrument");
}

#[test]
fn group_includes_instrument_when_selected() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Add a waveform
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));

    // Select instrument + midi clip + waveform
    app.selected.clear();
    app.selected.push(HitTarget::Instrument(inst_id));
    app.selected.push(HitTarget::MidiClip(mc_id));
    app.selected.push(HitTarget::Waveform(wf_id));

    // Group them
    app.execute_command(CommandAction::CreateGroup);

    // Should have one group containing all three
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert_eq!(group.member_ids.len(), 3);
    assert!(group.member_ids.contains(&inst_id), "instrument should be in group");
    assert!(group.member_ids.contains(&mc_id), "midi clip should be in group");
    assert!(group.member_ids.contains(&wf_id), "waveform should be in group");
}

#[test]
fn select_all_includes_instruments_and_midi_clips() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Add a waveform
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));

    // SelectAll
    app.execute_command(CommandAction::SelectAll);

    assert!(app.selected.contains(&HitTarget::Instrument(inst_id)), "SelectAll should include instrument");
    assert!(app.selected.contains(&HitTarget::MidiClip(mc_id)), "SelectAll should include midi clip");
    assert!(app.selected.contains(&HitTarget::Waveform(wf_id)), "SelectAll should include waveform");
}

#[test]
fn ungroup_restores_instrument_to_selection() {
    let mut app = App::new_headless();

    // Add an instrument
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();

    // Add a waveform
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));

    // Create group with instrument + waveform
    app.selected.clear();
    app.selected.push(HitTarget::Instrument(inst_id));
    app.selected.push(HitTarget::Waveform(wf_id));
    app.execute_command(CommandAction::CreateGroup);
    let group_id = app.groups.keys().next().copied().unwrap();

    // Ungroup
    app.selected.clear();
    app.selected.push(HitTarget::Group(group_id));
    app.execute_command(CommandAction::UngroupSelected);

    // Instrument should be restored to selection
    assert!(app.selected.contains(&HitTarget::Instrument(inst_id)), "instrument should be in selection after ungroup");
    assert!(app.selected.contains(&HitTarget::Waveform(wf_id)), "waveform should be in selection after ungroup");
}

#[test]
fn marquee_selecting_midi_clip_auto_includes_instrument() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Simulate marquee selection that only caught the MIDI clip (instrument is non-spatial)
    app.selected.clear();
    app.selected.push(HitTarget::MidiClip(mc_id));
    app.include_paired_instruments();

    // Instrument should be auto-included
    assert!(app.selected.contains(&HitTarget::Instrument(inst_id)), "instrument should be auto-included when its MIDI clip is marquee-selected");

    // Now group — instrument should be a member
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert!(group.member_ids.contains(&inst_id), "instrument should be in group");
    assert!(group.member_ids.contains(&mc_id), "midi clip should be in group");
}
