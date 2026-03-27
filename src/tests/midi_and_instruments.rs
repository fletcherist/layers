use crate::entity_id::{EntityId, new_id};
use crate::grid;
use crate::midi;
use crate::settings::{GridMode, Settings};
use crate::ui::palette::CommandAction;
use crate::App;
use crate::HitTarget;

/// Helper: get the first MIDI clip id from selected
fn first_selected_mc(app: &App) -> Option<EntityId> {
    app.selected.iter().find_map(|t| match t {
        HitTarget::MidiClip(id) => Some(*id),
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

#[test]
fn test_add_instrument_one_step() {
    let mut app = App::new_headless();
    assert!(app.midi_clips.is_empty());
    assert!(app.instruments.is_empty());

    // Single-step: add_instrument creates instrument + MIDI clip with plugin assigned
    app.add_instrument("test-synth", "Test Synth");
    assert_eq!(app.instruments.len(), 1);
    assert_eq!(app.midi_clips.len(), 1);

    // Should select the MIDI clip (not the instrument region)
    let mc_id = *app.midi_clips.keys().next().unwrap();
    assert!(app.selected.contains(&HitTarget::MidiClip(mc_id)));
    // MIDI editor should NOT auto-open so keyboard playing works immediately
    assert_eq!(app.editing_midi_clip, None);

    // Instrument should have plugin assigned
    let inst_id = *app.instruments.keys().next().unwrap();
    let inst = app.instruments.get(&inst_id).unwrap();
    assert!(inst.has_plugin());
    assert_eq!(inst.plugin_id, "test-synth");
    assert_eq!(inst.plugin_name, "Test Synth");

    // MIDI clip should reference the instrument
    let mc = app.midi_clips.get(&mc_id).unwrap();
    assert_eq!(mc.instrument_id, Some(inst_id));

    // Keyboard should target the instrument and be ready to play immediately
    assert_eq!(app.keyboard_instrument_id, Some(inst_id));
    assert!(app.computer_keyboard_armed);
    // No modal/editor blocking keyboard: editing_midi_clip must be None
    assert!(app.editing_midi_clip.is_none());
}

// ---------------------------------------------------------------------------
// MIDI Audio Sync
// ---------------------------------------------------------------------------

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


#[cfg(feature = "native")]
#[test]
fn test_computer_keyboard_state_and_project_browser() {
    use crate::midi_keyboard;
    use crate::ui::browser::BrowserCategory;

    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Selecting MIDI clip should set keyboard to its instrument
    app.selected = vec![HitTarget::MidiClip(mc_id)];
    app.sync_keyboard_instrument_from_selection();
    assert_eq!(app.keyboard_instrument_id, Some(inst_id));

    app.selected.clear();
    app.sync_keyboard_instrument_from_selection();
    assert_eq!(app.keyboard_instrument_id, None);

    app.computer_keyboard_armed = true;
    app.computer_keyboard_velocity = 72;
    assert_eq!(midi_keyboard::adjust_velocity(100, -8), 92);

    assert!(midi_keyboard::with_octave_offset(120, 1).is_none());
    assert_eq!(midi_keyboard::with_octave_offset(60, 3), Some(96));

    app.sample_browser.active_category = BrowserCategory::Layers;
    // Browser starts visible; close then re-open to trigger entry refresh
    app.execute_command(CommandAction::ToggleBrowser);
    app.execute_command(CommandAction::ToggleBrowser);
    // 1 instrument + 1 midi clip child = 2 entries when expanded
    assert!(app.sample_browser.entries.len() >= 1);

    app.selected.clear();
    app.sync_keyboard_instrument_from_selection();
    app.sync_computer_keyboard_to_engine();
    assert_eq!(app.keyboard_instrument_id, None);

    app.add_instrument("test-synth-2", "TestSynth2");
    // Close and re-open to trigger entry refresh
    app.execute_command(CommandAction::ToggleBrowser);
    app.execute_command(CommandAction::ToggleBrowser);
    assert_eq!(app.sample_browser.active_category, BrowserCategory::Layers);
    // 2 instruments with clips
    assert!(app.sample_browser.entries.len() >= 2);
}

// ---------------------------------------------------------------------------
// Instrument volume/pan defaults
// ---------------------------------------------------------------------------

#[test]
fn test_instrument_default_volume_pan() {
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    assert_eq!(app.instruments.len(), 1);
    let inst = app.instruments.values().next().unwrap();
    assert!((inst.volume - 1.0).abs() < f32::EPSILON, "default volume should be 1.0");
    assert!((inst.pan - 0.5).abs() < f32::EPSILON, "default pan should be 0.5");
    assert!(inst.effect_chain_id.is_none(), "default effect_chain_id should be None");
}

#[test]
fn test_instrument_right_window_opens_on_instrument_click() {
    use crate::ui::right_window::RightWindowTarget;
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();

    // Open right window for instrument
    app.update_right_window_for_instrument(inst_id);
    let rw = app.right_window.as_ref().expect("right window should open for instrument");
    assert!(rw.is_instrument());
    assert_eq!(rw.target_id(), inst_id);
    assert!((rw.volume - 1.0).abs() < f32::EPSILON);
    assert!((rw.pan - 0.5).abs() < f32::EPSILON);
}

#[test]
fn test_midi_clip_selection_opens_instrument_right_window() {
    use crate::ui::right_window::RightWindowTarget;
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();
    let mc_id = *app.midi_clips.keys().next().unwrap();

    // Select MIDI clip → should open instrument right window
    app.selected.clear();
    app.selected.push(HitTarget::MidiClip(mc_id));
    app.update_right_window();
    let rw = app.right_window.as_ref().expect("right window should open via midi clip");
    assert!(rw.is_instrument());
    assert_eq!(rw.target_id(), inst_id);
}

#[test]
fn test_instrument_volume_undo_redo() {
    use crate::instruments::InstrumentSnapshot;
    let mut app = App::new_headless();
    app.add_instrument("test-synth", "TestSynth");
    let inst_id = *app.instruments.keys().next().unwrap();

    // Change volume via operation
    let inst = app.instruments.get(&inst_id).unwrap();
    let before = InstrumentSnapshot {
        name: inst.name.clone(), plugin_id: inst.plugin_id.clone(),
        plugin_name: inst.plugin_name.clone(), plugin_path: inst.plugin_path.clone(),
        volume: inst.volume, pan: inst.pan, effect_chain_id: inst.effect_chain_id, disabled: inst.disabled,
    };
    let after = InstrumentSnapshot { volume: 0.5, ..before.clone() };
    app.instruments.get_mut(&inst_id).unwrap().volume = 0.5;
    app.push_op(crate::operations::Operation::UpdateInstrument { id: inst_id, before: before.clone(), after });

    assert!((app.instruments.get(&inst_id).unwrap().volume - 0.5).abs() < f32::EPSILON);

    // Undo
    app.undo_op();
    assert!((app.instruments.get(&inst_id).unwrap().volume - 1.0).abs() < f32::EPSILON, "undo should restore volume");

    // Redo
    app.redo_op();
    assert!((app.instruments.get(&inst_id).unwrap().volume - 0.5).abs() < f32::EPSILON, "redo should reapply volume");
}

#[test]
fn test_instrument_selection_in_layers_panel() {
    let mut app = App::new_headless();

    // Add two instruments
    app.add_instrument("synth-a", "SynthA");
    let inst_a = *app.instruments.keys().next().unwrap();
    app.add_instrument("synth-b", "SynthB");
    let inst_b = *app.instruments.keys().last().unwrap();

    // Simulate clicking instrument A in layers panel
    app.selected.clear();
    app.selected.push(HitTarget::Instrument(inst_a));
    app.keyboard_instrument_id = Some(inst_a);
    app.computer_keyboard_armed = true;

    assert_eq!(app.selected.len(), 1);
    assert_eq!(app.selected[0], HitTarget::Instrument(inst_a));
    assert_eq!(app.keyboard_instrument_id, Some(inst_a));
    assert!(app.computer_keyboard_armed);

    // Switch to instrument B
    app.selected.clear();
    app.selected.push(HitTarget::Instrument(inst_b));
    app.keyboard_instrument_id = Some(inst_b);

    assert_eq!(app.selected.len(), 1);
    assert_eq!(app.selected[0], HitTarget::Instrument(inst_b));
    assert_eq!(app.keyboard_instrument_id, Some(inst_b));
    assert!(app.computer_keyboard_armed);
}
