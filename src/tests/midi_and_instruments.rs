use crate::entity_id::{EntityId, new_id};
use crate::grid;
use crate::instruments;
use crate::midi;
use crate::settings::{GridMode, Settings};
use crate::ui::palette::CommandAction;
use crate::App;
use crate::DragState;
use crate::HitTarget;
use winit::keyboard::ModifiersState;

/// Helper: get the first MIDI clip id from selected
fn first_selected_mc(app: &App) -> Option<EntityId> {
    app.selected.iter().find_map(|t| match t {
        HitTarget::MidiClip(id) => Some(*id),
        _ => None,
    })
}

/// Helper: get the first instrument region id from selected
fn first_selected_ir(app: &App) -> Option<EntityId> {
    app.selected.iter().find_map(|t| match t {
        HitTarget::InstrumentRegion(id) => Some(*id),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// MIDI Clip CRUD
// ---------------------------------------------------------------------------

#[test]
fn test_add_midi_clip() {
    let mut app = App::new_headless();
    assert!(app.midi_clips.is_empty());
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 1);
    let mc_id = first_selected_mc(&app).expect("should have selected midi clip");
    let mc = app.midi_clips.get(&mc_id).unwrap();
    let ppb = grid::pixels_per_beat(app.bpm);
    let expected_width = ppb * 4.0 * midi::MIDI_CLIP_DEFAULT_BARS as f32;
    assert_eq!(mc.size[0], expected_width);
    assert_eq!(mc.size[1], midi::MIDI_CLIP_DEFAULT_HEIGHT);
    assert_eq!(mc.pitch_range, midi::MIDI_CLIP_DEFAULT_PITCH_RANGE);
    assert!(mc.notes.is_empty());
}

#[test]
fn test_delete_midi_clip() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc0_id = first_selected_mc(&app).unwrap();
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 2);
    app.selected = vec![HitTarget::MidiClip(mc0_id)];
    app.delete_selected();
    assert_eq!(app.midi_clips.len(), 1);
}

#[test]
fn test_move_midi_clip() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_id = first_selected_mc(&app).unwrap();
    let target = HitTarget::MidiClip(mc_id);
    app.set_target_pos(&target, [100.0, 200.0]);
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().position, [100.0, 200.0]);
    assert_eq!(app.get_target_pos(&target), [100.0, 200.0]);
}

#[test]
fn test_add_remove_midi_notes() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_id = first_selected_mc(&app).unwrap();
    {
        let mc = app.midi_clips.get_mut(&mc_id).unwrap();
        mc.notes.push(midi::MidiNote {
            pitch: 60,
            start_px: 0.0,
            duration_px: 30.0,
            velocity: 100,
        });
        mc.notes.push(midi::MidiNote {
            pitch: 64,
            start_px: 30.0,
            duration_px: 30.0,
            velocity: 80,
        });
    }
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().notes.len(), 2);
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().notes[0].pitch, 60);
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().notes[1].pitch, 64);

    // Remove first note
    app.midi_clips.get_mut(&mc_id).unwrap().notes.remove(0);
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().notes.len(), 1);
    assert_eq!(app.midi_clips.get(&mc_id).unwrap().notes[0].pitch, 64);
}

#[test]
fn test_midi_clip_pitch_to_y_and_back() {
    let mc = midi::MidiClip::new([0.0, 0.0], &Settings::default());
    // Round-trip: pitch -> y -> pitch
    for pitch in mc.pitch_range.0..mc.pitch_range.1 {
        let y = mc.pitch_to_y(pitch);
        let back = mc.y_to_pitch(y + mc.note_height() * 0.5); // center of note
        assert_eq!(back, pitch, "Round-trip failed for pitch {}", pitch);
    }
}

// ---------------------------------------------------------------------------
// Instrument Region
// ---------------------------------------------------------------------------

#[test]
fn test_add_instrument_region() {
    let mut app = App::new_headless();
    assert!(app.instrument_regions.is_empty());
    app.add_instrument_area();
    assert_eq!(app.instrument_regions.len(), 1);
    let ir_id = first_selected_ir(&app).expect("should have selected instrument region");
    let ir = app.instrument_regions.get(&ir_id).unwrap();
    assert_eq!(
        ir.size,
        [
            instruments::INSTRUMENT_REGION_DEFAULT_WIDTH,
            instruments::INSTRUMENT_REGION_DEFAULT_HEIGHT
        ]
    );
    assert!(!ir.has_plugin());
}

#[test]
fn test_delete_instrument_region() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    app.add_instrument_area();
    assert_eq!(app.instrument_regions.len(), 2);
    let ir_id = first_selected_ir(&app).unwrap();
    app.selected = vec![HitTarget::InstrumentRegion(ir_id)];
    app.delete_selected();
    assert_eq!(app.instrument_regions.len(), 1);
}

#[test]
fn test_move_instrument_region() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    let ir_id = first_selected_ir(&app).unwrap();
    let target = HitTarget::InstrumentRegion(ir_id);
    app.set_target_pos(&target, [50.0, 75.0]);
    assert_eq!(app.instrument_regions.get(&ir_id).unwrap().position, [50.0, 75.0]);
}

#[test]
fn test_add_instrument_one_step() {
    let mut app = App::new_headless();
    assert!(app.instrument_regions.is_empty());
    assert!(app.midi_clips.is_empty());

    // Single-step: add_instrument creates region + MIDI clip with plugin assigned
    app.add_instrument("test-synth", "Test Synth");
    assert_eq!(app.instrument_regions.len(), 1);
    assert_eq!(app.midi_clips.len(), 1);
    let ir_id = first_selected_ir(&app).expect("should select instrument region");
    // Should enter MIDI edit mode on the clip
    let mc_id = *app.midi_clips.keys().next().unwrap();
    assert_eq!(app.editing_midi_clip, Some(mc_id));

    let ir = app.instrument_regions.get(&ir_id).unwrap();
    assert!(ir.has_plugin());
    assert_eq!(ir.plugin_id, "test-synth");
    assert_eq!(ir.plugin_name, "Test Synth");

    // MIDI clip should be inside the region with padding
    let mc = app.midi_clips.get(&mc_id).unwrap();
    let padding = instruments::INSTRUMENT_REGION_PADDING;
    assert!((mc.position[0] - (ir.position[0] + padding)).abs() < 0.01);
    assert!((mc.position[1] - (ir.position[1] + padding)).abs() < 0.01);
    // Region should be clip + 2*padding
    assert!((ir.size[0] - (mc.size[0] + padding * 2.0)).abs() < 0.01);
    assert!((ir.size[1] - (mc.size[1] + padding * 2.0)).abs() < 0.01);
}

#[test]
fn test_instrument_region_auto_extends_on_clip_resize() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    let ir_id = first_selected_ir(&app).unwrap();
    {
        let ir = app.instrument_regions.get_mut(&ir_id).unwrap();
        ir.position = [100.0, 100.0];
        ir.size = [400.0, 300.0];
        ir.plugin_id = "test-synth".to_string();
    }

    // Place a MIDI clip inside
    let mc_id = new_id();
    app.midi_clips.insert(mc_id, midi::MidiClip::new([120.0, 120.0], &app.settings.clone()));
    app.midi_clips.get_mut(&mc_id).unwrap().size = [360.0, 260.0];

    // Simulate resizing the clip to be wider than the region
    let new_clip_size = [600.0, 260.0];
    app.midi_clips.get_mut(&mc_id).unwrap().size = new_clip_size;
    let cp = app.midi_clips.get(&mc_id).unwrap().position;
    let cs = app.midi_clips.get(&mc_id).unwrap().size;
    let padding = instruments::INSTRUMENT_REGION_PADDING;
    for (_, ir) in &mut app.instrument_regions {
        if crate::ui::hit_testing::rects_overlap(ir.position, ir.size, cp, cs) {
            instruments::ensure_region_contains_clip(ir, cp, cs, padding);
        }
    }

    // Region should have grown to contain the clip + padding
    let ir = app.instrument_regions.get(&ir_id).unwrap();
    assert!(ir.position[0] + ir.size[0] >= cp[0] + cs[0] + padding);
    assert!(ir.position[1] + ir.size[1] >= cp[1] + cs[1] + padding);
}

// ---------------------------------------------------------------------------
// MIDI Audio Sync
// ---------------------------------------------------------------------------

#[test]
fn test_sync_instrument_regions_produces_events() {
    let mut app = App::new_headless();

    // Add instrument region
    app.add_instrument_area();
    let ir_id = first_selected_ir(&app).unwrap();
    {
        let ir = app.instrument_regions.get_mut(&ir_id).unwrap();
        ir.position = [0.0, 0.0];
        ir.size = [1000.0, 500.0];
        ir.plugin_id = "test-synth".to_string();
        ir.plugin_name = "Test Synth".to_string();
    }

    // Add MIDI clip inside the region
    let mc_id = new_id();
    app.midi_clips.insert(mc_id, midi::MidiClip {
        position: [100.0, 100.0],
        size: [200.0, 150.0],
        color: midi::MIDI_CLIP_DEFAULT_COLOR,
        notes: vec![
            midi::MidiNote {
                pitch: 60,
                start_px: 0.0,
                duration_px: 60.0,
                velocity: 100,
            },
            midi::MidiNote {
                pitch: 64,
                start_px: 60.0,
                duration_px: 30.0,
                velocity: 80,
            },
        ],
        pitch_range: (48, 84),
        grid_mode: GridMode::default(),
        triplet_grid: false,
        velocity_lane_height: midi::VELOCITY_LANE_HEIGHT,
    });

    // No audio engine in headless, but sync_instrument_regions shouldn't panic
    app.sync_instrument_regions();
    // Just verify it doesn't crash - actual audio processing requires AudioEngine
}

#[test]
fn test_undo_redo_midi_clip() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 1);

    // push_op was called by add_midi_clip, so undo_op should remove it
    app.undo_op();
    assert_eq!(app.midi_clips.len(), 0);

    app.redo_op();
    assert_eq!(app.midi_clips.len(), 1);
}

#[test]
fn test_midi_clip_individual_grid() {
    use crate::settings::{AdaptiveGridSize, FixedGrid};

    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc0_id = first_selected_mc(&app).unwrap();
    app.add_midi_clip();
    let mc1_id = first_selected_mc(&app).unwrap();
    assert_eq!(app.midi_clips.len(), 2);

    // Both clips inherit project grid by default
    assert_eq!(app.midi_clips.get(&mc0_id).unwrap().grid_mode, app.settings.grid_mode);
    assert_eq!(app.midi_clips.get(&mc0_id).unwrap().triplet_grid, app.settings.triplet_grid);
    assert_eq!(app.midi_clips.get(&mc1_id).unwrap().grid_mode, app.settings.grid_mode);

    // Change clip 0 to 1/8 fixed, triplet
    {
        let mc0 = app.midi_clips.get_mut(&mc0_id).unwrap();
        mc0.grid_mode = GridMode::Fixed(FixedGrid::Eighth);
        mc0.triplet_grid = true;
    }

    // Change clip 1 to adaptive wide
    {
        let mc1 = app.midi_clips.get_mut(&mc1_id).unwrap();
        mc1.grid_mode = GridMode::Adaptive(AdaptiveGridSize::Wide);
        mc1.triplet_grid = false;
    }

    // Verify independence
    assert_eq!(
        app.midi_clips.get(&mc0_id).unwrap().grid_mode,
        GridMode::Fixed(FixedGrid::Eighth)
    );
    assert!(app.midi_clips.get(&mc0_id).unwrap().triplet_grid);
    assert_eq!(
        app.midi_clips.get(&mc1_id).unwrap().grid_mode,
        GridMode::Adaptive(AdaptiveGridSize::Wide)
    );
    assert!(!app.midi_clips.get(&mc1_id).unwrap().triplet_grid);

    // Project grid unchanged
    assert_eq!(app.settings.grid_mode, GridMode::default());
    assert!(!app.settings.triplet_grid);
}

#[test]
fn test_undo_redo_instrument_region() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    assert_eq!(app.instrument_regions.len(), 1);

    app.undo_op();
    assert_eq!(app.instrument_regions.len(), 0);

    app.redo_op();
    assert_eq!(app.instrument_regions.len(), 1);
}

#[cfg(feature = "native")]
#[test]
fn test_computer_keyboard_state_and_project_browser() {
    use crate::midi_keyboard;
    use crate::ui::browser::BrowserCategory;

    let mut app = App::new_headless();
    app.add_instrument_area();
    let ir_id = first_selected_ir(&app).unwrap();

    app.sync_keyboard_instrument_from_selection();
    assert_eq!(app.keyboard_instrument_id, Some(ir_id));

    app.selected.clear();
    app.sync_keyboard_instrument_from_selection();
    assert_eq!(app.keyboard_instrument_id, None);

    app.computer_keyboard_armed = true;
    app.computer_keyboard_velocity = 72;
    assert_eq!(midi_keyboard::adjust_velocity(100, -8), 92);

    assert!(midi_keyboard::with_octave_offset(120, 1).is_none());
    assert_eq!(midi_keyboard::with_octave_offset(60, 3), Some(96));

    app.sample_browser.active_category = BrowserCategory::Project;
    app.execute_command(CommandAction::ToggleBrowser);
    assert_eq!(app.sample_browser.entries.len(), 1);

    app.focus_instrument_region(ir_id);
    assert!(app
        .selected
        .iter()
        .any(|t| matches!(t, HitTarget::InstrumentRegion(id) if *id == ir_id)));

    app.selected.clear();
    app.sync_keyboard_instrument_from_selection();
    app.sync_computer_keyboard_to_engine();
    assert_eq!(app.keyboard_instrument_id, None);

    app.add_instrument_area();
    app.execute_command(CommandAction::ToggleBrowser);
    app.execute_command(CommandAction::ToggleBrowser);
    assert_eq!(app.sample_browser.active_category, BrowserCategory::Project);
    assert_eq!(app.sample_browser.entries.len(), 2);
}
