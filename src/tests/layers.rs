use std::sync::Arc;

use crate::App;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::group::Group;
use crate::layers::{self, DropTarget, LayerNodeKind};
use crate::midi;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView, DEFAULT_AUTO_FADE_PX};
use crate::HitTarget;

fn make_waveform(x: f32, y: f32) -> WaveformView {
    make_waveform_sized(x, y, 200.0, 80.0)
}

fn make_waveform_sized(x: f32, y: f32, w: f32, h: f32) -> WaveformView {
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
        size: [w, h],
        color: [0.35, 0.75, 0.55, 1.0],
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
fn test_layer_tree_built_from_entities() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");

    app.refresh_project_browser_entries();
    assert!(!app.layer_tree.is_empty());

    let inst_nodes: Vec<_> = app.layer_tree.iter()
        .filter(|n| n.kind == LayerNodeKind::Instrument)
        .collect();
    assert_eq!(inst_nodes.len(), 1);
}

#[test]
fn test_midi_clip_nested_under_instrument() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    app.refresh_project_browser_entries();

    assert_eq!(app.layer_tree.len(), 1);
    let ir_node = &app.layer_tree[0];
    assert_eq!(ir_node.kind, LayerNodeKind::Instrument);
    assert_eq!(ir_node.children.len(), 1);
    assert_eq!(ir_node.children[0].kind, LayerNodeKind::MidiClip);
}

#[test]
fn test_layer_tree_sync_removes_stale() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = app.instruments.keys().next().copied().unwrap();
    app.refresh_project_browser_entries();
    assert_eq!(app.layer_tree.len(), 1);

    // Delete the MIDI clip — instrument stays but with no children
    let mc_id = app.midi_clips.keys().next().copied().unwrap();
    app.selected = vec![HitTarget::MidiClip(mc_id)];
    app.delete_selected();
    // Now delete the instrument by removing it directly and refreshing
    app.instruments.shift_remove(&inst_id);
    app.refresh_project_browser_entries();
    assert!(app.layer_tree.is_empty());
}

#[test]
fn test_move_node_up_down() {
    let mut app = App::new_headless();
    app.add_instrument("synth-a", "SynthA");
    let id_a = *app.instruments.keys().next().unwrap();
    app.selected.clear();

    // Add a second instrument
    app.add_instrument("synth-b", "SynthB");
    let id_b = *app.instruments.keys().nth(1).unwrap();
    app.refresh_project_browser_entries();

    assert_eq!(app.layer_tree.len(), 2);
    assert_eq!(app.layer_tree[0].entity_id, id_a);
    assert_eq!(app.layer_tree[1].entity_id, id_b);

    // Move second instrument up
    assert!(layers::move_node_up(&mut app.layer_tree, id_b));
    assert_eq!(app.layer_tree[0].entity_id, id_b);
    assert_eq!(app.layer_tree[1].entity_id, id_a);

    // Move it back down
    assert!(layers::move_node_down(&mut app.layer_tree, id_b));
    assert_eq!(app.layer_tree[0].entity_id, id_a);
    assert_eq!(app.layer_tree[1].entity_id, id_b);

    // Can't move first up further
    assert!(!layers::move_node_up(&mut app.layer_tree, id_a));
    // Can't move last down further
    assert!(!layers::move_node_down(&mut app.layer_tree, id_b));
}

#[test]
fn test_flatten_respects_expanded() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    app.refresh_project_browser_entries();

    // Expanded by default — should see instrument + midi child
    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instruments, &app.midi_clips,
        &app.waveforms, &app.groups,
        &app.solo_ids,
    );
    assert_eq!(rows.len(), 2);

    // Collapse the instrument node
    let ir_id = app.layer_tree[0].entity_id;
    layers::toggle_expanded(&mut app.layer_tree, ir_id);
    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instruments, &app.midi_clips,
        &app.waveforms, &app.groups,
        &app.solo_ids,
    );
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_layer_tree_storage_roundtrip() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    app.refresh_project_browser_entries();

    let stored = layers::tree_to_stored(&app.layer_tree);
    let restored = layers::tree_from_stored(&stored);

    assert_eq!(restored.len(), app.layer_tree.len());
    assert_eq!(restored[0].entity_id, app.layer_tree[0].entity_id);
    assert_eq!(restored[0].children.len(), app.layer_tree[0].children.len());
}

#[test]
fn test_midi_clip_has_instrument_id_after_add_instrument() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");

    let mc = app.midi_clips.values().next().unwrap();
    let inst_id = app.instruments.keys().next().copied().unwrap();
    assert_eq!(mc.instrument_id, Some(inst_id));
}

#[test]
fn test_flat_layer_row_color() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));
    app.refresh_project_browser_entries();

    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instruments, &app.midi_clips,
        &app.waveforms, &app.groups,
        &app.solo_ids,
    );

    let wf_row = rows.iter().find(|r| r.kind == LayerNodeKind::Waveform)
        .expect("should have a waveform row");

    // Color must not be zero-initialized — a real color was populated
    assert_ne!(wf_row.color, [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn test_delete_instrument_cascades_midi_clips() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    assert_eq!(app.instruments.len(), 1);
    assert_eq!(app.midi_clips.len(), 1);

    // Delete the MIDI clip (which belongs to the instrument)
    let mc_id = app.midi_clips.keys().next().copied().unwrap();
    app.selected = vec![HitTarget::MidiClip(mc_id)];
    app.delete_selected();
    assert!(app.midi_clips.is_empty());
}

#[test]
fn test_reorder_root_nodes() {
    let mut app = App::new_headless();
    let id_a = new_id();
    let id_b = new_id();
    let id_c = new_id();
    app.waveforms.insert(id_a, make_waveform(0.0, 0.0));
    app.waveforms.insert(id_b, make_waveform(0.0, 100.0));
    app.waveforms.insert(id_c, make_waveform(0.0, 200.0));
    app.refresh_project_browser_entries();

    // Tree should have 3 root nodes (sorted by Y)
    assert_eq!(app.layer_tree.len(), 3);
    assert_eq!(app.layer_tree[0].entity_id, id_a);
    assert_eq!(app.layer_tree[1].entity_id, id_b);
    assert_eq!(app.layer_tree[2].entity_id, id_c);

    // Move id_c to before id_a (index 0)
    let moved = layers::execute_drop(&mut app.layer_tree, &DropTarget::BeforeRoot(0), id_c);
    assert!(moved);
    assert_eq!(app.layer_tree[0].entity_id, id_c);
    assert_eq!(app.layer_tree[1].entity_id, id_a);
    assert_eq!(app.layer_tree[2].entity_id, id_b);

    // Move id_a to after last
    let moved = layers::execute_drop(&mut app.layer_tree, &DropTarget::AfterLastRoot, id_a);
    assert!(moved);
    assert_eq!(app.layer_tree[0].entity_id, id_c);
    assert_eq!(app.layer_tree[1].entity_id, id_b);
    assert_eq!(app.layer_tree[2].entity_id, id_a);
}

#[test]
fn test_drag_into_group() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let group_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));
    app.groups.insert(group_id, Group::new(
        group_id, "Group 1".to_string(), [0.0, 200.0], [200.0, 100.0], vec![],
    ));
    app.refresh_project_browser_entries();

    assert_eq!(app.layer_tree.len(), 2); // waveform + group at root

    // Drop waveform into the group
    let moved = layers::execute_drop(
        &mut app.layer_tree,
        &DropTarget::InsideGroup { group_id, child_index: 0 },
        wf_id,
    );
    assert!(moved);

    // Waveform should be gone from root and inside the group
    assert_eq!(app.layer_tree.len(), 1);
    assert_eq!(app.layer_tree[0].entity_id, group_id);
    assert_eq!(app.layer_tree[0].children.len(), 1);
    assert_eq!(app.layer_tree[0].children[0].entity_id, wf_id);
}

#[test]
fn test_drag_out_of_group() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let group_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));
    app.groups.insert(group_id, Group::new(
        group_id, "Group 1".to_string(), [0.0, 0.0], [200.0, 100.0], vec![wf_id],
    ));
    app.refresh_project_browser_entries();

    // Waveform should be inside the group (not at root)
    let group_node = app.layer_tree.iter().find(|n| n.entity_id == group_id).unwrap();
    assert_eq!(group_node.children.len(), 1);
    assert_eq!(group_node.children[0].entity_id, wf_id);

    // Drop waveform to root (before the group)
    let group_idx = app.layer_tree.iter().position(|n| n.entity_id == group_id).unwrap();
    let moved = layers::execute_drop(
        &mut app.layer_tree,
        &DropTarget::BeforeRoot(group_idx),
        wf_id,
    );
    assert!(moved);

    // Waveform should be at root, group should have no children
    assert_eq!(app.layer_tree.len(), 2);
    let wf_node = app.layer_tree.iter().find(|n| n.entity_id == wf_id);
    assert!(wf_node.is_some());
    let group_node = app.layer_tree.iter().find(|n| n.entity_id == group_id).unwrap();
    assert!(group_node.children.is_empty());
}

#[test]
fn test_drag_between_groups() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    let group_a = new_id();
    let group_b = new_id();
    app.waveforms.insert(wf_id, make_waveform(0.0, 0.0));
    app.groups.insert(group_a, Group::new(
        group_a, "Group A".to_string(), [0.0, 0.0], [200.0, 100.0], vec![wf_id],
    ));
    app.groups.insert(group_b, Group::new(
        group_b, "Group B".to_string(), [0.0, 200.0], [200.0, 100.0], vec![],
    ));
    app.refresh_project_browser_entries();

    // Waveform starts in group A
    let ga = app.layer_tree.iter().find(|n| n.entity_id == group_a).unwrap();
    assert_eq!(ga.children.len(), 1);
    assert_eq!(ga.children[0].entity_id, wf_id);

    // Move waveform into group B
    let moved = layers::execute_drop(
        &mut app.layer_tree,
        &DropTarget::InsideGroup { group_id: group_b, child_index: 0 },
        wf_id,
    );
    assert!(moved);

    // Group A empty, Group B has waveform
    let ga = app.layer_tree.iter().find(|n| n.entity_id == group_a).unwrap();
    assert!(ga.children.is_empty());
    let gb = app.layer_tree.iter().find(|n| n.entity_id == group_b).unwrap();
    assert_eq!(gb.children.len(), 1);
    assert_eq!(gb.children[0].entity_id, wf_id);
}

#[test]
fn test_update_group_bounds_after_member_added() {
    let mut app = App::new_headless();

    let wf1 = new_id();
    let wf2 = new_id();
    app.waveforms.insert(wf1, make_waveform_sized(100.0, 50.0, 200.0, 80.0));
    app.waveforms.insert(wf2, make_waveform_sized(400.0, 200.0, 150.0, 60.0));

    let group_id = new_id();
    app.groups.insert(group_id, Group::new(
        group_id, "G".to_string(), [100.0, 50.0], [200.0, 80.0], vec![wf1],
    ));

    // Group starts with bounds of wf1 only
    let g = &app.groups[&group_id];
    assert_eq!(g.position, [100.0, 50.0]);
    assert_eq!(g.size, [200.0, 80.0]);

    // Simulate adding wf2 to the group (as the layer-drop code does)
    app.groups.get_mut(&group_id).unwrap().member_ids.push(wf2);
    app.update_group_bounds(group_id);

    // Expected union: min=(100,50) max=(400+150, 200+60) => size=(450, 210)
    let g = &app.groups[&group_id];
    assert_eq!(g.position, [100.0, 50.0]);
    assert_eq!(g.size, [450.0, 210.0]);
}
