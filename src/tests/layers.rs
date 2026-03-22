use std::sync::Arc;

use crate::App;
use crate::automation::AutomationData;
use crate::entity_id::new_id;
use crate::layers::{self, LayerNodeKind};
use crate::midi;
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView, DEFAULT_AUTO_FADE_PX};
use crate::HitTarget;

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
    }
}

#[test]
fn test_layer_tree_built_from_entities() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    app.add_midi_clip();

    app.refresh_project_browser_entries();
    assert!(!app.layer_tree.is_empty());

    let ir_nodes: Vec<_> = app.layer_tree.iter()
        .filter(|n| n.kind == LayerNodeKind::Instrument)
        .collect();
    assert_eq!(ir_nodes.len(), 1);
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
    app.add_instrument_area();
    let ir_id = app.instrument_regions.keys().next().copied().unwrap();
    app.refresh_project_browser_entries();
    assert_eq!(app.layer_tree.len(), 1);

    app.selected = vec![HitTarget::InstrumentRegion(ir_id)];
    app.delete_selected();
    app.refresh_project_browser_entries();
    assert!(app.layer_tree.is_empty());
}

#[test]
fn test_move_node_up_down() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    let ir_id = app.instrument_regions.keys().next().copied().unwrap();
    app.selected.clear();

    // Add a second instrument
    app.add_instrument_area();
    let ir2_id = app.instrument_regions.keys().nth(1).copied().unwrap();
    app.refresh_project_browser_entries();

    assert_eq!(app.layer_tree.len(), 2);
    assert_eq!(app.layer_tree[0].entity_id, ir_id);
    assert_eq!(app.layer_tree[1].entity_id, ir2_id);

    // Move second instrument up
    assert!(layers::move_node_up(&mut app.layer_tree, ir2_id));
    assert_eq!(app.layer_tree[0].entity_id, ir2_id);
    assert_eq!(app.layer_tree[1].entity_id, ir_id);

    // Move it back down
    assert!(layers::move_node_down(&mut app.layer_tree, ir2_id));
    assert_eq!(app.layer_tree[0].entity_id, ir_id);
    assert_eq!(app.layer_tree[1].entity_id, ir2_id);

    // Can't move first up further
    assert!(!layers::move_node_up(&mut app.layer_tree, ir_id));
    // Can't move last down further
    assert!(!layers::move_node_down(&mut app.layer_tree, ir2_id));
}

#[test]
fn test_flatten_respects_expanded() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    app.refresh_project_browser_entries();

    // Expanded by default — should see instrument + midi child
    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instrument_regions, &app.midi_clips,
        &app.waveforms, &app.effect_regions, &app.plugin_blocks,
    );
    assert_eq!(rows.len(), 2);

    // Collapse the instrument node
    let ir_id = app.layer_tree[0].entity_id;
    layers::toggle_expanded(&mut app.layer_tree, ir_id);
    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instrument_regions, &app.midi_clips,
        &app.waveforms, &app.effect_regions, &app.plugin_blocks,
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
    let ir_id = app.instrument_regions.keys().next().copied().unwrap();
    assert_eq!(mc.instrument_region_id, Some(ir_id));
}

#[test]
fn test_flat_layer_row_color() {
    let mut app = App::new_headless();
    let wf_id = new_id();
    app.waveforms.insert(wf_id, make_waveform(100.0, 100.0));
    app.refresh_project_browser_entries();

    let rows = layers::flatten_tree(
        &app.layer_tree,
        &app.instrument_regions, &app.midi_clips,
        &app.waveforms, &app.effect_regions, &app.plugin_blocks,
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
    assert_eq!(app.instrument_regions.len(), 1);
    assert_eq!(app.midi_clips.len(), 1);

    let ir_id = app.instrument_regions.keys().next().copied().unwrap();
    app.selected = vec![HitTarget::InstrumentRegion(ir_id)];
    app.delete_selected();

    assert!(app.instrument_regions.is_empty());
    assert!(app.midi_clips.is_empty());
}
