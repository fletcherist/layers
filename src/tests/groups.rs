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
        crate::ui::text_input::TextInput::with_text("My Custom Group".to_string(), crate::ui::text_input::TextInputConfig::default()),
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
    app.add_instrument("test-synth", "TestSynth", None);
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
    app.add_instrument("test-synth", "TestSynth", None);
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
        &app.solo_ids, app.monitoring_group_id, None,
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
    app.add_instrument("test-synth", "TestSynth", None);
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

    // MIDI clip resolves to its parent instrument, so group should contain
    // instrument + waveform (deduplicated)
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert_eq!(group.member_ids.len(), 2);
    assert!(group.member_ids.contains(&inst_id), "instrument should be in group");
    assert!(!group.member_ids.contains(&mc_id), "midi clip should be resolved to instrument, not stored directly");
    assert!(group.member_ids.contains(&wf_id), "waveform should be in group");
}

#[test]
fn select_all_includes_instruments_and_midi_clips() {
    let mut app = App::new_headless();

    // Add an instrument (creates a paired MIDI clip)
    app.add_instrument("test-synth", "TestSynth", None);
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
    app.add_instrument("test-synth", "TestSynth", None);
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
    app.add_instrument("test-synth", "TestSynth", None);
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Simulate marquee selection that only caught the MIDI clip (instrument is non-spatial)
    app.selected.clear();
    app.selected.push(HitTarget::MidiClip(mc_id));
    app.include_paired_instruments();

    // Instrument should be auto-included
    assert!(app.selected.contains(&HitTarget::Instrument(inst_id)), "instrument should be auto-included when its MIDI clip is marquee-selected");

    // Now group — MIDI clip resolves to instrument, so only instrument in member_ids
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert!(group.member_ids.contains(&inst_id), "instrument should be in group");
    assert!(!group.member_ids.contains(&mc_id), "midi clip should be resolved to instrument");
}

#[test]
fn cmd_g_with_midi_clip_selected_groups_instrument() {
    let mut app = App::new_headless();

    app.add_instrument("test-synth", "TestSynth", None);
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Select only the MIDI clip (as happens when clicking on canvas)
    app.selected.clear();
    app.selected.push(HitTarget::MidiClip(mc_id));

    // Cmd+G — should resolve MIDI clip to its parent instrument
    app.execute_command(CommandAction::CreateGroup);

    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert_eq!(group.member_ids.len(), 1);
    assert!(group.member_ids.contains(&inst_id), "group should contain the instrument, not the midi clip");
}

#[test]
fn duplicate_group_deep_clones_members() {
    let mut app = App::new_headless();

    // Create two waveforms and a group containing them
    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();
    let original_members = app.groups[&group_id].member_ids.clone();

    // Duplicate the group
    app.selected = vec![HitTarget::Group(group_id)];
    app.duplicate_selected();

    // Should now have 2 groups
    assert_eq!(app.groups.len(), 2, "duplicate should create a second group");

    // Find the new group
    let dup_group = app.groups.values()
        .find(|g| g.member_ids != original_members)
        .expect("duplicated group should have different member_ids");

    // Duplicated group must have the same count of members
    assert_eq!(dup_group.member_ids.len(), 2, "duplicated group should have 2 members");

    // All member IDs must be different from the original
    for mid in &dup_group.member_ids {
        assert!(!original_members.contains(mid), "member ID {:?} should be a new clone, not the original", mid);
    }

    // The cloned waveforms must actually exist in app.waveforms
    for mid in &dup_group.member_ids {
        assert!(app.waveforms.contains_key(mid), "cloned waveform {:?} must exist in app.waveforms", mid);
    }

    // Total waveforms: 2 original + 2 cloned = 4
    assert_eq!(app.waveforms.len(), 4, "should have 4 waveforms total after duplicate");

    // Verify cloned member positions are shifted by group width
    let orig_group = &app.groups[&group_id];
    let shift_x = orig_group.size[0];
    for mid in &dup_group.member_ids {
        let pos = app.waveforms[mid].position;
        // Each cloned waveform should be shifted right by shift_x from its original
        assert!(pos[0] >= shift_x, "cloned waveform at x={} should be shifted by {} from original", pos[0], shift_x);
    }
    // More specifically: original waveforms at 0.0 and 200.0, so clones should be at shift_x and 200+shift_x
    let mut dup_positions: Vec<f32> = dup_group.member_ids.iter()
        .map(|mid| app.waveforms[mid].position[0])
        .collect();
    dup_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut orig_positions: Vec<f32> = original_members.iter()
        .map(|mid| app.waveforms[mid].position[0])
        .collect();
    orig_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (dup_x, orig_x) in dup_positions.iter().zip(orig_positions.iter()) {
        assert!(
            (*dup_x - (*orig_x + shift_x)).abs() < 0.01,
            "duplicated waveform x={} should equal original x={} + shift={}",
            dup_x, orig_x, shift_x
        );
    }
}

#[test]
fn copy_paste_group_deep_clones_members() {
    let mut app = App::new_headless();

    // Create two waveforms and a group containing them
    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();
    let original_members = app.groups[&group_id].member_ids.clone();

    // Copy then paste
    app.selected = vec![HitTarget::Group(group_id)];
    app.copy_selected();
    app.paste_clipboard();

    // Should now have 2 groups
    assert_eq!(app.groups.len(), 2, "paste should create a second group");

    // Find the pasted group
    let pasted_group = app.groups.values()
        .find(|g| g.member_ids != original_members)
        .expect("pasted group should have different member_ids");

    // Pasted group must have 2 members
    assert_eq!(pasted_group.member_ids.len(), 2, "pasted group should have 2 members");

    // All member IDs must be different from the original
    for mid in &pasted_group.member_ids {
        assert!(!original_members.contains(mid), "member ID {:?} should be a new clone, not the original", mid);
    }

    // The cloned waveforms must actually exist in app.waveforms
    for mid in &pasted_group.member_ids {
        assert!(app.waveforms.contains_key(mid), "cloned waveform {:?} must exist in app.waveforms", mid);
    }

    // Total waveforms: 2 original + 2 cloned = 4
    assert_eq!(app.waveforms.len(), 4, "should have 4 waveforms total after paste");

    // Verify pasted member positions are offset consistently with the pasted group
    let orig_group = &app.groups[&group_id];
    let dx = pasted_group.position[0] - orig_group.position[0];
    let dy = pasted_group.position[1] - orig_group.position[1];
    let mut pasted_positions: Vec<[f32; 2]> = pasted_group.member_ids.iter()
        .map(|mid| app.waveforms[mid].position)
        .collect();
    pasted_positions.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    let mut orig_positions: Vec<[f32; 2]> = original_members.iter()
        .map(|mid| app.waveforms[mid].position)
        .collect();
    orig_positions.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    for (pasted_pos, orig_pos) in pasted_positions.iter().zip(orig_positions.iter()) {
        assert!(
            (pasted_pos[0] - (orig_pos[0] + dx)).abs() < 0.01,
            "pasted waveform x={} should equal original x={} + dx={}",
            pasted_pos[0], orig_pos[0], dx
        );
        assert!(
            (pasted_pos[1] - (orig_pos[1] + dy)).abs() < 0.01,
            "pasted waveform y={} should equal original y={} + dy={}",
            pasted_pos[1], orig_pos[1], dy
        );
    }
}

#[test]
fn undo_paste_group_removes_members() {
    let mut app = App::new_headless();

    // Create two waveforms and a group containing them
    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();

    // Copy then paste
    app.selected = vec![HitTarget::Group(group_id)];
    app.copy_selected();
    app.paste_clipboard();

    // Should now have 2 groups and 4 waveforms
    assert_eq!(app.groups.len(), 2);
    assert_eq!(app.waveforms.len(), 4);

    // Undo the paste
    app.undo_op();

    // Should be back to 1 group and 2 waveforms
    assert_eq!(app.groups.len(), 1, "undo paste should remove the pasted group");
    assert_eq!(app.waveforms.len(), 2, "undo paste should remove the pasted waveforms");

    // Original waveforms should still exist
    assert!(app.waveforms.contains_key(&wf_id1), "original waveform 1 should still exist");
    assert!(app.waveforms.contains_key(&wf_id2), "original waveform 2 should still exist");
}

#[test]
fn undo_duplicate_group_removes_members() {
    let mut app = App::new_headless();

    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    // Duplicate
    let group_id = *app.groups.keys().next().unwrap();
    app.selected = vec![HitTarget::Group(group_id)];
    app.duplicate_selected();

    assert_eq!(app.groups.len(), 2);
    assert_eq!(app.waveforms.len(), 4);

    // Undo
    app.undo_op();

    assert_eq!(app.groups.len(), 1, "undo duplicate should remove the duplicated group");
    assert_eq!(app.waveforms.len(), 2, "undo duplicate should remove the duplicated waveforms");
    assert!(app.waveforms.contains_key(&wf_id1));
    assert!(app.waveforms.contains_key(&wf_id2));
}

#[test]
fn undo_move_group_restores_member_positions() {
    let mut app = App::new_headless();

    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(100.0, 50.0));
    app.waveforms.insert(wf_id2, make_waveform(300.0, 50.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    let group_id = *app.groups.keys().next().unwrap();

    // Capture before positions
    let before_wf1_pos = app.waveforms[&wf_id1].position;
    let before_wf2_pos = app.waveforms[&wf_id2].position;
    let before_group = app.groups[&group_id].clone();

    // Simulate moving the group by capturing before states and applying move
    let before_wf1 = app.waveforms[&wf_id1].clone();
    let before_wf2 = app.waveforms[&wf_id2].clone();

    // Move group right by 200px
    app.set_target_pos(&HitTarget::Group(group_id), [
        before_group.position[0] + 200.0,
        before_group.position[1],
    ]);

    // Verify members moved
    assert!((app.waveforms[&wf_id1].position[0] - (before_wf1_pos[0] + 200.0)).abs() < 0.01);
    assert!((app.waveforms[&wf_id2].position[0] - (before_wf2_pos[0] + 200.0)).abs() < 0.01);

    // Commit ops like drag-end does: update ops for group + members
    let after_group = app.groups[&group_id].clone();
    let after_wf1 = app.waveforms[&wf_id1].clone();
    let after_wf2 = app.waveforms[&wf_id2].clone();
    let ops = vec![
        crate::operations::Operation::UpdateWaveform { id: wf_id1, before: before_wf1, after: after_wf1 },
        crate::operations::Operation::UpdateWaveform { id: wf_id2, before: before_wf2, after: after_wf2 },
        crate::operations::Operation::UpdateGroup { id: group_id, before: before_group, after: after_group },
    ];
    app.push_op(crate::operations::Operation::Batch(ops));

    // Undo
    app.undo_op();

    // Members should be back at original positions
    assert!(
        (app.waveforms[&wf_id1].position[0] - before_wf1_pos[0]).abs() < 0.01,
        "wf1 x={} should be back to {}", app.waveforms[&wf_id1].position[0], before_wf1_pos[0]
    );
    assert!(
        (app.waveforms[&wf_id2].position[0] - before_wf2_pos[0]).abs() < 0.01,
        "wf2 x={} should be back to {}", app.waveforms[&wf_id2].position[0], before_wf2_pos[0]
    );
    assert!(
        (app.groups[&group_id].position[0] - before_wf1_pos[0]).abs() < 0.01,
        "group position should be restored"
    );
}

#[test]
fn delete_group_also_deletes_members() {
    let mut app = App::new_headless();

    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    let group_id = *app.groups.keys().next().unwrap();

    // Select just the group and delete
    app.selected = vec![HitTarget::Group(group_id)];
    app.delete_selected();

    assert_eq!(app.groups.len(), 0, "group should be deleted");
    assert_eq!(app.waveforms.len(), 0, "member waveforms should also be deleted");

    // Undo should restore both group and members
    app.undo_op();
    assert_eq!(app.groups.len(), 1, "group should be restored on undo");
    assert_eq!(app.waveforms.len(), 2, "member waveforms should be restored on undo");
}

#[test]
fn delete_all_members_also_deletes_group() {
    let mut app = App::new_headless();

    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));

    // Create a group containing the single waveform
    app.selected.push(HitTarget::Waveform(wf_id));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();

    // Select just the waveform (not the group) and delete
    app.selected = vec![HitTarget::Waveform(wf_id)];
    app.delete_selected();

    assert_eq!(app.waveforms.len(), 0, "waveform should be deleted");
    assert_eq!(app.groups.len(), 0, "empty group should be auto-deleted");

    // Undo should restore both the waveform and the group
    app.undo_op();
    assert_eq!(app.waveforms.len(), 1, "waveform should be restored on undo");
    assert_eq!(app.groups.len(), 1, "group should be restored on undo");
    assert!(app.groups[&group_id].member_ids.contains(&wf_id), "group should contain waveform again");
}

#[test]
fn alt_drag_group_deep_clones_members() {
    let mut app = App::new_headless();

    let wf_id1 = new_id();
    let wf_id2 = new_id();
    app.waveforms.insert(wf_id1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_id2, make_waveform(200.0, 0.0));

    app.selected.push(HitTarget::Waveform(wf_id1));
    app.selected.push(HitTarget::Waveform(wf_id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();
    let original_members = app.groups[&group_id].member_ids.clone();

    // Simulate option+drag: begin_move_selection with alt_copy=true
    app.selected = vec![HitTarget::Group(group_id)];
    app.begin_move_selection([100.0, 100.0], true, Some(HitTarget::Group(group_id)));

    // Should now have 2 groups
    assert_eq!(app.groups.len(), 2, "alt+drag should create a second group");

    // Find the new group (not the original)
    let new_group_id = *app.groups.keys().find(|id| **id != group_id).unwrap();
    let new_members = app.groups[&new_group_id].member_ids.clone();

    // New group must have same number of members
    assert_eq!(new_members.len(), 2, "cloned group should have 2 members");

    // All member IDs must be different from the original
    for mid in &new_members {
        assert!(!original_members.contains(mid), "member {:?} should be a new clone", mid);
    }

    // Cloned waveforms must exist
    for mid in &new_members {
        assert!(app.waveforms.contains_key(mid), "cloned waveform {:?} must exist", mid);
    }

    // Total: 2 original + 2 cloned = 4
    assert_eq!(app.waveforms.len(), 4, "should have 4 waveforms total");
}

// ---------------------------------------------------------------------------
// Nested-group helpers
// ---------------------------------------------------------------------------

#[test]
fn nested_group_parent_group_of() {
    use crate::group::{self, Group};

    let mut app = App::new_headless();

    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf2, make_waveform(300.0, 0.0));

    let child_group_id = new_id();
    let child = Group::new(child_group_id, "Child".into(), [0.0, 0.0], [200.0, 80.0], vec![wf1, wf2]);
    app.groups.insert(child_group_id, child);

    let parent_group_id = new_id();
    let parent = Group::new(parent_group_id, "Parent".into(), [0.0, 0.0], [500.0, 80.0], vec![child_group_id]);
    app.groups.insert(parent_group_id, parent);

    // wf1's direct parent is the child group
    assert_eq!(group::parent_group_of(wf1, &app.groups), Some(child_group_id));
    // child group's parent is the parent group
    assert_eq!(group::parent_group_of(child_group_id, &app.groups), Some(parent_group_id));
    // parent group has no parent
    assert_eq!(group::parent_group_of(parent_group_id, &app.groups), None);
}

#[test]
fn nested_group_ancestor_chain() {
    use crate::group::{self, Group};

    let mut app = App::new_headless();

    let wf = new_id();
    app.waveforms.insert(wf, make_waveform(0.0, 0.0));

    let g1 = new_id();
    app.groups.insert(g1, Group::new(g1, "G1".into(), [0.0, 0.0], [200.0, 80.0], vec![wf]));

    let g2 = new_id();
    app.groups.insert(g2, Group::new(g2, "G2".into(), [0.0, 0.0], [300.0, 80.0], vec![g1]));

    let g3 = new_id();
    app.groups.insert(g3, Group::new(g3, "G3".into(), [0.0, 0.0], [400.0, 80.0], vec![g2]));

    // wf -> g1 -> g2 -> g3
    let chain = group::ancestor_chain(wf, &app.groups);
    assert_eq!(chain, vec![g1, g2, g3]);

    // g1 -> g2 -> g3
    let chain = group::ancestor_chain(g1, &app.groups);
    assert_eq!(chain, vec![g2, g3]);

    // g3 has no ancestors
    let chain = group::ancestor_chain(g3, &app.groups);
    assert!(chain.is_empty());

    // entity not in any group
    let orphan = new_id();
    let chain = group::ancestor_chain(orphan, &app.groups);
    assert!(chain.is_empty());
}

#[test]
fn nested_group_all_transitive_members() {
    use crate::group::{self, Group};

    let mut app = App::new_headless();

    let wf1 = new_id();
    let wf2 = new_id();
    let wf3 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf2, make_waveform(200.0, 0.0));
    app.waveforms.insert(wf3, make_waveform(400.0, 0.0));

    let child_a = new_id();
    app.groups.insert(child_a, Group::new(child_a, "A".into(), [0.0, 0.0], [200.0, 80.0], vec![wf1]));

    let child_b = new_id();
    app.groups.insert(child_b, Group::new(child_b, "B".into(), [200.0, 0.0], [200.0, 80.0], vec![wf2, wf3]));

    let parent = new_id();
    app.groups.insert(parent, Group::new(parent, "Parent".into(), [0.0, 0.0], [600.0, 80.0], vec![child_a, child_b]));

    let members = group::all_transitive_members(parent, &app.groups);
    assert_eq!(members.len(), 3);
    assert!(members.contains(&wf1));
    assert!(members.contains(&wf2));
    assert!(members.contains(&wf3));

    // child_a has only wf1
    let members = group::all_transitive_members(child_a, &app.groups);
    assert_eq!(members, vec![wf1]);
}

#[test]
fn nested_group_would_create_cycle() {
    use crate::group::{self, Group};

    let mut app = App::new_headless();

    let g1 = new_id();
    let g2 = new_id();
    let g3 = new_id();

    app.groups.insert(g1, Group::new(g1, "G1".into(), [0.0, 0.0], [100.0, 80.0], vec![]));
    app.groups.insert(g2, Group::new(g2, "G2".into(), [0.0, 0.0], [200.0, 80.0], vec![g1]));
    app.groups.insert(g3, Group::new(g3, "G3".into(), [0.0, 0.0], [300.0, 80.0], vec![g2]));

    // Self-cycle
    assert!(group::would_create_cycle(g1, g1, &app.groups));

    // Direct cycle: g1 contains g2, adding g2 → g1 would cycle
    // g3 -> g2 -> g1 ; adding g1 as member of g1 is self-cycle (already tested)
    // adding g3 as member of g1 would create cycle (g1 is inside g2 which is inside g3)
    assert!(group::would_create_cycle(g1, g3, &app.groups));

    // No cycle: g1 and independent group
    let g4 = new_id();
    app.groups.insert(g4, Group::new(g4, "G4".into(), [0.0, 0.0], [100.0, 80.0], vec![]));
    assert!(!group::would_create_cycle(g1, g4, &app.groups));
    assert!(!group::would_create_cycle(g4, g1, &app.groups));

    // OK to add g1 into g3 (g1 is already transitively inside g3 via g2, but adding directly is not a cycle issue — it's membership, not containment)
    // Actually: g3 contains g2 which contains g1. Adding g1 to g3 directly is fine (g1 doesn't contain g3).
    assert!(!group::would_create_cycle(g3, g1, &app.groups));
}

#[test]
fn nested_group_create_subgroup() {
    use crate::group::Group;
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();

    // Create two waveforms and group them
    let wf1 = new_id();
    let wf2 = new_id();
    let wf3 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf2, make_waveform(250.0, 0.0));
    app.waveforms.insert(wf3, make_waveform(500.0, 0.0));

    // Create a parent group with all three
    let parent_id = new_id();
    let parent = Group::new(parent_id, "Parent".into(), [0.0, 0.0], [700.0, 80.0], vec![wf1, wf2, wf3]);
    app.groups.insert(parent_id, parent);

    // Select wf1 and wf2 (which are inside the parent group), and create a sub-group
    app.selected = vec![HitTarget::Waveform(wf1), HitTarget::Waveform(wf2)];
    app.execute_command(CommandAction::CreateGroup);

    // Now there should be 2 groups
    assert_eq!(app.groups.len(), 2, "should have parent + child group");

    // Find the child group (not the parent)
    let child_id = *app.groups.keys().find(|id| **id != parent_id).unwrap();

    // Child group should have wf1 and wf2
    let child = &app.groups[&child_id];
    assert_eq!(child.member_ids.len(), 2);
    assert!(child.member_ids.contains(&wf1));
    assert!(child.member_ids.contains(&wf2));

    // Parent group should now have child_id and wf3 (not wf1, wf2)
    let parent = &app.groups[&parent_id];
    assert!(parent.member_ids.contains(&child_id), "parent should contain child group");
    assert!(parent.member_ids.contains(&wf3), "parent should still contain wf3");
    assert!(!parent.member_ids.contains(&wf1), "wf1 should be moved to child");
    assert!(!parent.member_ids.contains(&wf2), "wf2 should be moved to child");
}

#[test]
fn nested_group_ungroup_child() {
    use crate::group::Group;
    use crate::ui::palette::CommandAction;

    let mut app = App::new_headless();

    let wf1 = new_id();
    let wf2 = new_id();
    let wf3 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf2, make_waveform(250.0, 0.0));
    app.waveforms.insert(wf3, make_waveform(500.0, 0.0));

    // Create child group with wf1, wf2
    let child_id = new_id();
    app.groups.insert(child_id, Group::new(child_id, "Child".into(), [0.0, 0.0], [450.0, 80.0], vec![wf1, wf2]));

    // Create parent group with child_id and wf3
    let parent_id = new_id();
    app.groups.insert(parent_id, Group::new(parent_id, "Parent".into(), [0.0, 0.0], [700.0, 80.0], vec![child_id, wf3]));

    // Select the child group and ungroup it
    app.selected = vec![HitTarget::Group(child_id)];
    app.execute_command(CommandAction::UngroupSelected);

    // Child group should be removed
    assert!(!app.groups.contains_key(&child_id), "child group should be deleted");

    // Parent should now contain wf1, wf2, wf3 directly
    let parent = &app.groups[&parent_id];
    assert!(parent.member_ids.contains(&wf1));
    assert!(parent.member_ids.contains(&wf2));
    assert!(parent.member_ids.contains(&wf3));
    assert_eq!(parent.member_ids.len(), 3);
}

#[test]
fn nested_group_mute_parent_mutes_children() {
    use crate::group::Group;

    let mut app = App::new_headless();

    let wf1 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));

    let child_id = new_id();
    app.groups.insert(child_id, Group::new(child_id, "Child".into(), [0.0, 0.0], [200.0, 80.0], vec![wf1]));

    let parent_id = new_id();
    app.groups.insert(parent_id, Group::new(parent_id, "Parent".into(), [0.0, 0.0], [300.0, 80.0], vec![child_id]));

    // Initially audible
    assert!(crate::is_entity_audible(wf1, &app.solo_ids, &app.groups));

    // Mute the parent group
    app.groups.get_mut(&parent_id).unwrap().disabled = true;

    // wf1 should not be audible (parent of its parent group is muted)
    assert!(!crate::is_entity_audible(wf1, &app.solo_ids, &app.groups));
}

#[test]
fn nested_group_solo_parent_solos_children() {
    use crate::group::Group;

    let mut app = App::new_headless();

    let wf_in = new_id();
    let wf_out = new_id();
    app.waveforms.insert(wf_in, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf_out, make_waveform(500.0, 0.0));

    let child_id = new_id();
    app.groups.insert(child_id, Group::new(child_id, "Child".into(), [0.0, 0.0], [200.0, 80.0], vec![wf_in]));

    let parent_id = new_id();
    app.groups.insert(parent_id, Group::new(parent_id, "Parent".into(), [0.0, 0.0], [300.0, 80.0], vec![child_id]));

    // Solo the parent group
    app.solo_ids.insert(parent_id);

    // wf_in (inside nested child) should be audible via parent solo
    assert!(crate::is_entity_audible(wf_in, &app.solo_ids, &app.groups));

    // wf_out (outside any group) should NOT be audible
    assert!(!crate::is_entity_audible(wf_out, &app.solo_ids, &app.groups));
}

#[test]
fn nested_group_update_bounds_propagates() {
    use crate::group::Group;

    let mut app = App::new_headless();

    let wf1 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 50.0));

    let child_id = new_id();
    app.groups.insert(child_id, Group::new(child_id, "Child".into(), [0.0, 0.0], [0.0, 0.0], vec![wf1]));

    let parent_id = new_id();
    app.groups.insert(parent_id, Group::new(parent_id, "Parent".into(), [0.0, 0.0], [0.0, 0.0], vec![child_id]));

    // Update child bounds — should propagate to parent
    app.update_group_bounds(child_id);

    let child = &app.groups[&child_id];
    assert_eq!(child.position, [100.0, 50.0]);
    assert_eq!(child.size, [200.0, 80.0]);

    // Parent should also have updated bounds (contains child group's bounds)
    let parent = &app.groups[&parent_id];
    assert_eq!(parent.position, [100.0, 50.0]);
    assert_eq!(parent.size, [200.0, 80.0]);
}

#[test]
fn nested_group_layer_tree() {
    use crate::group::Group;

    let mut app = App::new_headless();

    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform(0.0, 0.0));
    app.waveforms.insert(wf2, make_waveform(200.0, 0.0));

    let child_id = new_id();
    app.groups.insert(child_id, Group::new(child_id, "Inner".into(), [0.0, 0.0], [200.0, 80.0], vec![wf1]));

    let parent_id = new_id();
    app.groups.insert(parent_id, Group::new(parent_id, "Outer".into(), [0.0, 0.0], [400.0, 80.0], vec![child_id, wf2]));

    let tree = crate::layers::build_default_tree(
        &app.instruments, &app.midi_clips, &app.waveforms, &app.groups,
    );

    // Only the parent should be at root level (child group is nested)
    let group_roots: Vec<_> = tree.iter()
        .filter(|n| n.kind == crate::layers::LayerNodeKind::Group)
        .collect();
    assert_eq!(group_roots.len(), 1, "only parent group at root");
    assert_eq!(group_roots[0].entity_id, parent_id);

    // Parent should have 2 children: child_id (group) and wf2
    let parent_node = &group_roots[0];
    assert_eq!(parent_node.children.len(), 2);

    // Find child group node
    let child_node = parent_node.children.iter()
        .find(|c| c.entity_id == child_id)
        .expect("child group should be a child of parent in tree");
    assert_eq!(child_node.kind, crate::layers::LayerNodeKind::Group);
    assert_eq!(child_node.children.len(), 1);
    assert_eq!(child_node.children[0].entity_id, wf1);
}

#[test]
fn drop_audio_into_group_adds_member() {
    let mut app = App::new_headless();

    // Create a group with one waveform
    let wf1 = new_id();
    app.waveforms.insert(wf1, make_waveform(100.0, 100.0));
    app.selected.push(HitTarget::Waveform(wf1));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);
    let group_id = *app.groups.keys().next().unwrap();
    assert_eq!(app.groups[&group_id].member_ids.len(), 1);

    // Simulate dropping a new waveform into the group
    let wf2 = new_id();
    app.waveforms.insert(wf2, make_waveform(120.0, 120.0));
    let before = app.groups[&group_id].clone();
    app.groups.get_mut(&group_id).unwrap().member_ids.push(wf2);
    app.update_group_bounds(group_id);
    let after = app.groups[&group_id].clone();

    assert_eq!(app.groups[&group_id].member_ids.len(), 2);
    assert!(app.groups[&group_id].member_ids.contains(&wf1));
    assert!(app.groups[&group_id].member_ids.contains(&wf2));
}

#[test]
fn find_drop_target_group_picks_smallest() {
    let mut app = App::new_headless();

    // Create two nested groups — outer is bigger, inner is smaller
    let outer_id = new_id();
    let inner_id = new_id();
    app.groups.insert(outer_id, crate::group::Group {
        id: outer_id,
        name: "Outer".to_string(),
        position: [0.0, 0.0],
        size: [400.0, 400.0],
        member_ids: vec![inner_id],
        effect_chain_id: None,
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });
    app.groups.insert(inner_id, crate::group::Group {
        id: inner_id,
        name: "Inner".to_string(),
        position: [50.0, 50.0],
        size: [100.0, 100.0],
        member_ids: vec![],
        effect_chain_id: None,
        volume: 1.0,
        pan: 0.5,
        disabled: false,
    });

    // Default camera is at (-100, -50) with zoom 1.0
    // screen_to_world(screen) = [screen[0] + cam.x, screen[1] + cam.y]
    // So to hit world (75, 75) we need screen (175, 125)
    app.mouse_pos = [175.0, 125.0];
    let target = app.find_drop_target_group();
    assert_eq!(target, Some(inner_id), "should pick the innermost (smallest) group");

    // Position mouse outside inner but inside outer (world 10, 10 → screen 110, 60)
    app.mouse_pos = [110.0, 60.0];
    let target = app.find_drop_target_group();
    assert_eq!(target, Some(outer_id), "should pick outer group when not inside inner");

    // Position mouse outside both groups (world 500, 500 → screen 600, 550)
    app.mouse_pos = [600.0, 550.0];
    let target = app.find_drop_target_group();
    assert_eq!(target, None, "should return None when not inside any group");
}
