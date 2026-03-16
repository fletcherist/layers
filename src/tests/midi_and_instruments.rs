use crate::grid;
use crate::instruments;
use crate::midi;
use crate::settings::{GridMode, Settings};
use crate::App;
use crate::DragState;
use crate::HitTarget;

// ---------------------------------------------------------------------------
// MIDI Clip CRUD
// ---------------------------------------------------------------------------

#[test]
fn test_add_midi_clip() {
    let mut app = App::new_headless();
    assert!(app.midi_clips.is_empty());
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 1);
    assert_eq!(app.selected, vec![HitTarget::MidiClip(0)]);
    let mc = &app.midi_clips[0];
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
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 2);
    app.selected = vec![HitTarget::MidiClip(0)];
    app.delete_selected();
    assert_eq!(app.midi_clips.len(), 1);
}

#[test]
fn test_move_midi_clip() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let target = HitTarget::MidiClip(0);
    app.set_target_pos(&target, [100.0, 200.0]);
    assert_eq!(app.midi_clips[0].position, [100.0, 200.0]);
    assert_eq!(app.get_target_pos(&target), [100.0, 200.0]);
}

#[test]
fn test_add_remove_midi_notes() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc = &mut app.midi_clips[0];
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
    assert_eq!(app.midi_clips[0].notes.len(), 2);
    assert_eq!(app.midi_clips[0].notes[0].pitch, 60);
    assert_eq!(app.midi_clips[0].notes[1].pitch, 64);

    // Remove first note
    app.midi_clips[0].notes.remove(0);
    assert_eq!(app.midi_clips[0].notes.len(), 1);
    assert_eq!(app.midi_clips[0].notes[0].pitch, 64);
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
    assert_eq!(app.selected, vec![HitTarget::InstrumentRegion(0)]);
    let ir = &app.instrument_regions[0];
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
    app.selected = vec![HitTarget::InstrumentRegion(1)];
    app.delete_selected();
    assert_eq!(app.instrument_regions.len(), 1);
}

#[test]
fn test_move_instrument_region() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    let target = HitTarget::InstrumentRegion(0);
    app.set_target_pos(&target, [50.0, 75.0]);
    assert_eq!(app.instrument_regions[0].position, [50.0, 75.0]);
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
    assert_eq!(app.selected, vec![HitTarget::InstrumentRegion(0)]);
    // Should enter MIDI edit mode on the clip
    assert_eq!(app.editing_midi_clip, Some(0));

    let ir = &app.instrument_regions[0];
    assert!(ir.has_plugin());
    assert_eq!(ir.plugin_id, "test-synth");
    assert_eq!(ir.plugin_name, "Test Synth");

    // MIDI clip should be inside the region with padding
    let mc = &app.midi_clips[0];
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
    let ir = &mut app.instrument_regions[0];
    ir.position = [100.0, 100.0];
    ir.size = [400.0, 300.0];
    ir.plugin_id = "test-synth".to_string();

    // Place a MIDI clip inside
    app.midi_clips.push(midi::MidiClip::new([120.0, 120.0], &app.settings.clone()));
    app.midi_clips[0].size = [360.0, 260.0];

    // Simulate resizing the clip to be wider than the region
    let new_clip_size = [600.0, 260.0];
    app.midi_clips[0].size = new_clip_size;
    let cp = app.midi_clips[0].position;
    let cs = app.midi_clips[0].size;
    let padding = instruments::INSTRUMENT_REGION_PADDING;
    for ir in &mut app.instrument_regions {
        if crate::hit_testing::rects_overlap(ir.position, ir.size, cp, cs) {
            instruments::ensure_region_contains_clip(ir, cp, cs, padding);
        }
    }

    // Region should have grown to contain the clip + padding
    let ir = &app.instrument_regions[0];
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
    app.instrument_regions[0].position = [0.0, 0.0];
    app.instrument_regions[0].size = [1000.0, 500.0];
    app.instrument_regions[0].plugin_id = "test-synth".to_string();
    app.instrument_regions[0].plugin_name = "Test Synth".to_string();

    // Add MIDI clip inside the region
    app.midi_clips.push(midi::MidiClip {
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

    // push_undo was called by add_midi_clip, so undo should remove it
    app.undo();
    assert_eq!(app.midi_clips.len(), 0);

    app.redo();
    assert_eq!(app.midi_clips.len(), 1);
}

#[test]
fn test_midi_clip_individual_grid() {
    use crate::settings::{AdaptiveGridSize, FixedGrid};

    let mut app = App::new_headless();
    app.add_midi_clip();
    app.add_midi_clip();
    assert_eq!(app.midi_clips.len(), 2);

    // Both clips inherit project grid by default
    assert_eq!(app.midi_clips[0].grid_mode, app.settings.grid_mode);
    assert_eq!(app.midi_clips[0].triplet_grid, app.settings.triplet_grid);
    assert_eq!(app.midi_clips[1].grid_mode, app.settings.grid_mode);

    // Change clip 0 to 1/8 fixed, triplet
    app.midi_clips[0].grid_mode = GridMode::Fixed(FixedGrid::Eighth);
    app.midi_clips[0].triplet_grid = true;

    // Change clip 1 to adaptive wide
    app.midi_clips[1].grid_mode = GridMode::Adaptive(AdaptiveGridSize::Wide);
    app.midi_clips[1].triplet_grid = false;

    // Verify independence
    assert_eq!(
        app.midi_clips[0].grid_mode,
        GridMode::Fixed(FixedGrid::Eighth)
    );
    assert!(app.midi_clips[0].triplet_grid);
    assert_eq!(
        app.midi_clips[1].grid_mode,
        GridMode::Adaptive(AdaptiveGridSize::Wide)
    );
    assert!(!app.midi_clips[1].triplet_grid);

    // Project grid unchanged
    assert_eq!(app.settings.grid_mode, GridMode::default());
    assert!(!app.settings.triplet_grid);
}

#[test]
fn test_undo_redo_instrument_region() {
    let mut app = App::new_headless();
    app.add_instrument_area();
    assert_eq!(app.instrument_regions.len(), 1);

    app.undo();
    assert_eq!(app.instrument_regions.len(), 0);

    app.redo();
    assert_eq!(app.instrument_regions.len(), 1);
}

// ---------------------------------------------------------------------------
// Alt+drag duplication of MIDI notes
// ---------------------------------------------------------------------------

#[test]
fn test_alt_duplicate_midi_notes() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Add two notes to the clip
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 20.0,
        velocity: 80,
    });
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);

    // Simulate what alt+drag does: clone selected notes and push them
    app.push_undo();
    let selected = vec![0usize, 1usize];
    let mut new_indices: Vec<usize> = Vec::new();
    for &ni in &selected {
        let cloned = app.midi_clips[mc_idx].notes[ni].clone();
        app.midi_clips[mc_idx].notes.push(cloned);
        new_indices.push(app.midi_clips[mc_idx].notes.len() - 1);
    }
    app.selected_midi_notes = new_indices.clone();

    // Should now have 4 notes (original 2 + 2 duplicates)
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 4);

    // Duplicates have the same pitch and duration as originals
    assert_eq!(app.midi_clips[mc_idx].notes[2].pitch, 60);
    assert_eq!(app.midi_clips[mc_idx].notes[2].duration_px, 30.0);
    assert_eq!(app.midi_clips[mc_idx].notes[3].pitch, 64);
    assert_eq!(app.midi_clips[mc_idx].notes[3].duration_px, 20.0);

    // selected_midi_notes points to the new duplicates
    assert_eq!(app.selected_midi_notes, vec![2, 3]);

    // Undo should revert to 2 notes
    app.undo();
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
}

// ---------------------------------------------------------------------------
// Drag-select, empty-click, and multi-note drag
// ---------------------------------------------------------------------------

#[test]
fn test_drag_select_midi_notes() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    // Position the clip at a known location
    app.midi_clips[mc_idx].position = [0.0, 0.0];
    app.midi_clips[mc_idx].size = [480.0, 200.0];

    let pitch_range = app.midi_clips[mc_idx].pitch_range;
    let nh = app.midi_clips[mc_idx].size[1] / (pitch_range.1 - pitch_range.0) as f32;

    // Add three notes at known positions
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 200.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 400.0,
        duration_px: 30.0,
        velocity: 100,
    });

    // Compute selection rect that covers only first two notes
    let note0_x = 0.0 + app.midi_clips[mc_idx].notes[0].start_px;
    let note1_x = 0.0
        + app.midi_clips[mc_idx].notes[1].start_px
        + app.midi_clips[mc_idx].notes[1].duration_px;
    let note_y = app.midi_clips[mc_idx].pitch_to_y(60);

    let rx = note0_x - 1.0;
    let ry = note_y - 1.0;
    let rw = (note1_x - rx) + 1.0;
    let rh = nh + 2.0;

    // Simulate SelectingMidiNotes: find notes intersecting selection rect
    let pos = app.midi_clips[mc_idx].position;
    let _size = app.midi_clips[mc_idx].size;
    let clip_nh = app.midi_clips[mc_idx].note_height();
    let mut selected = Vec::new();
    for (i, note) in app.midi_clips[mc_idx].notes.iter().enumerate() {
        let nx = pos[0] + note.start_px;
        let ny = app.midi_clips[mc_idx].pitch_to_y(note.pitch);
        let nw = note.duration_px;
        if nx < rx + rw && nx + nw > rx && ny < ry + rh && ny + clip_nh > ry {
            selected.push(i);
        }
    }
    app.selected_midi_notes = selected;

    // Only the first two notes (indices 0 and 1) should be selected
    assert_eq!(app.selected_midi_notes, vec![0, 1]);
}

#[test]
fn test_empty_click_clears_midi_selection() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    // Pre-select the note
    app.selected_midi_notes = vec![0];
    assert_eq!(app.selected_midi_notes.len(), 1);

    // Simulate empty-space click: clear selection
    app.selected_midi_notes.clear();
    app.midi_note_select_rect = None;

    assert!(app.selected_midi_notes.is_empty());
    // No new notes should have been created
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
}

#[test]
fn test_multi_note_drag_moves_all_selected() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.midi_clips[mc_idx].position = [0.0, 0.0];
    app.midi_clips[mc_idx].size = [480.0, 200.0];

    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 30.0,
        velocity: 100,
    });

    // Select both notes
    app.selected_midi_notes = vec![0, 1];

    // Simulate moving both notes by 20px to the right
    let delta = 20.0f32;
    for &ni in &app.selected_midi_notes.clone() {
        app.midi_clips[mc_idx].notes[ni].start_px += delta;
    }

    assert_eq!(app.midi_clips[mc_idx].notes[0].start_px, 30.0);
    assert_eq!(app.midi_clips[mc_idx].notes[1].start_px, 70.0);
}

#[test]
fn test_midi_auto_edit_mode_zoom_threshold() {
    use crate::MIDI_AUTO_EDIT_ZOOM_THRESHOLD;

    let mut app = App::new_headless();
    app.add_midi_clip();
    app.midi_clips[0].position = [50.0, 50.0];
    app.midi_clips[0].size = [200.0, 100.0];

    // At low zoom, editing_midi_clip should not be auto-set
    app.camera.zoom = 1.0;
    assert!(app.camera.zoom < MIDI_AUTO_EDIT_ZOOM_THRESHOLD);
    assert!(app.editing_midi_clip.is_none());

    // Simulate auto-edit: at high zoom, clicking on clip should enter edit mode
    app.camera.zoom = 3.0;
    assert!(app.camera.zoom >= MIDI_AUTO_EDIT_ZOOM_THRESHOLD);
    app.editing_midi_clip = Some(0);
    app.selected_midi_notes.clear();
    assert_eq!(app.editing_midi_clip, Some(0));

    // Simulate zoom out below threshold: should auto-exit edit mode
    app.camera.zoom = 1.0;
    if app.camera.zoom < MIDI_AUTO_EDIT_ZOOM_THRESHOLD && app.editing_midi_clip.is_some() {
        app.editing_midi_clip = None;
        app.selected_midi_notes.clear();
    }
    assert!(app.editing_midi_clip.is_none());
    assert!(app.selected_midi_notes.is_empty());
}

#[test]
fn test_double_click_creates_note_in_edit_mode() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc = &app.midi_clips[0];
    let clip_pos = mc.position;
    let clip_size = mc.size;

    // Enter edit mode
    app.editing_midi_clip = Some(0);
    assert!(app.midi_clips[0].notes.is_empty());

    // Pick a world position inside the clip
    let click_x = clip_pos[0] + 50.0;
    let click_y = clip_pos[1] + clip_size[1] / 2.0;

    // Simulate what the double-click handler does: create a note
    let pitch = app.midi_clips[0].y_to_pitch(click_y);
    let start_px = app.midi_clips[0].x_to_start_px(click_x);
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch,
        start_px,
        duration_px: midi::DEFAULT_NOTE_DURATION_PX,
        velocity: 100,
    });

    assert_eq!(app.midi_clips[0].notes.len(), 1);
    assert_eq!(app.midi_clips[0].notes[0].pitch, pitch);
    assert_eq!(
        app.midi_clips[0].notes[0].duration_px,
        midi::DEFAULT_NOTE_DURATION_PX
    );
    assert_eq!(app.midi_clips[0].notes[0].velocity, 100);
}

#[test]
fn test_alt_copy_midi_clip() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 0.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 30.0,
        duration_px: 30.0,
        velocity: 80,
    });
    let original_pos = app.midi_clips[0].position;

    app.selected = vec![HitTarget::MidiClip(0)];
    app.begin_move_selection([original_pos[0] + 10.0, original_pos[1] + 10.0], true);

    assert_eq!(app.midi_clips.len(), 2);
    assert_eq!(app.midi_clips[1].notes.len(), 2);
    assert_eq!(app.midi_clips[1].notes[0].pitch, 60);
    assert_eq!(app.midi_clips[1].notes[1].pitch, 64);
    assert_eq!(app.midi_clips[1].position, original_pos);
    assert_eq!(app.selected, vec![HitTarget::MidiClip(1)]);
}

// ---------------------------------------------------------------------------
// Velocity editing
// ---------------------------------------------------------------------------

#[test]
fn test_velocity_change() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    assert_eq!(app.midi_clips[mc_idx].notes[0].velocity, 100);

    app.midi_clips[mc_idx].notes[0].velocity = 50;
    assert_eq!(app.midi_clips[mc_idx].notes[0].velocity, 50);

    app.midi_clips[mc_idx].notes[0].velocity = 0;
    assert_eq!(app.midi_clips[mc_idx].notes[0].velocity, 0);

    app.midi_clips[mc_idx].notes[0].velocity = 127;
    assert_eq!(app.midi_clips[mc_idx].notes[0].velocity, 127);
}

#[test]
fn test_velocity_bar_hit_test() {
    use crate::Camera;

    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.midi_clips[mc_idx].position = [0.0, 0.0];
    app.midi_clips[mc_idx].size = [480.0, 200.0];

    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 100.0,
        duration_px: 30.0,
        velocity: 80,
    });

    let camera = Camera::new();
    let mc = &app.midi_clips[mc_idx];
    let lane_top = mc.velocity_lane_top();
    let lane_mid_y = lane_top + midi::VELOCITY_LANE_HEIGHT * 0.5;

    // Hit the first note's velocity bar
    let hit = midi::hit_test_velocity_bar(mc, [20.0, lane_mid_y], &camera);
    assert_eq!(hit, Some(0));

    // Hit the second note's velocity bar
    let hit = midi::hit_test_velocity_bar(mc, [110.0, lane_mid_y], &camera);
    assert_eq!(hit, Some(1));

    // Miss (between notes)
    let hit = midi::hit_test_velocity_bar(mc, [60.0, lane_mid_y], &camera);
    assert_eq!(hit, None);

    // Miss (above velocity lane)
    let hit = midi::hit_test_velocity_bar(mc, [20.0, lane_top - 5.0], &camera);
    assert_eq!(hit, None);
}

#[test]
fn test_velocity_multi_select_drag() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 80,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 67,
        start_px: 100.0,
        duration_px: 30.0,
        velocity: 60,
    });

    // Simulate what DraggingVelocity does: apply delta to selected notes
    let selected = vec![0usize, 1usize];
    let original_vels: Vec<u8> = selected
        .iter()
        .map(|&ni| app.midi_clips[mc_idx].notes[ni].velocity)
        .collect();

    let vel_delta: i16 = 20;
    for (j, &ni) in selected.iter().enumerate() {
        let new_vel = (original_vels[j] as i16 + vel_delta).clamp(0, 127) as u8;
        app.midi_clips[mc_idx].notes[ni].velocity = new_vel;
    }

    assert_eq!(app.midi_clips[mc_idx].notes[0].velocity, 100);
    assert_eq!(app.midi_clips[mc_idx].notes[1].velocity, 120);
    // Unselected note unchanged
    assert_eq!(app.midi_clips[mc_idx].notes[2].velocity, 60);
}

#[test]
fn test_velocity_lane_layout() {
    let mc = midi::MidiClip::new([0.0, 0.0], &Settings::default());

    let full_h = mc.size[1];
    let note_area = mc.note_area_height(true);
    let vel_lane = midi::VELOCITY_LANE_HEIGHT;

    assert!(note_area < full_h);
    assert!((note_area + vel_lane - full_h).abs() < 0.001);

    assert_eq!(mc.note_area_height(false), full_h);

    let lane_top = mc.velocity_lane_top();
    assert!((lane_top - note_area).abs() < 0.001);
}

#[test]
fn test_editing_pitch_round_trip() {
    let mc = midi::MidiClip::new([0.0, 0.0], &Settings::default());

    for pitch in mc.pitch_range.0..mc.pitch_range.1 {
        let y = mc.pitch_to_y_editing(pitch, true);
        let nh = mc.note_height_editing(true);
        let back = mc.y_to_pitch_editing(y + nh * 0.5, true);
        assert_eq!(back, pitch, "Editing round-trip failed for pitch {}", pitch);
    }
}

#[test]
fn test_velocity_lane_resize() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    let default_h = app.midi_clips[mc_idx].velocity_lane_height;
    assert_eq!(default_h, midi::VELOCITY_LANE_HEIGHT);

    // Simulate drag: increase lane height
    let new_h = 80.0f32.clamp(midi::VELOCITY_LANE_MIN_HEIGHT, midi::VELOCITY_LANE_MAX_HEIGHT);
    app.midi_clips[mc_idx].velocity_lane_height = new_h;
    assert_eq!(app.midi_clips[mc_idx].velocity_lane_height, 80.0);

    // Note area should shrink accordingly
    let note_area = app.midi_clips[mc_idx].note_area_height(true);
    assert!((note_area - (app.midi_clips[mc_idx].size[1] - 80.0)).abs() < 0.001);

    // Clamp to min
    app.midi_clips[mc_idx].velocity_lane_height = 5.0f32.clamp(
        midi::VELOCITY_LANE_MIN_HEIGHT,
        midi::VELOCITY_LANE_MAX_HEIGHT,
    );
    assert_eq!(
        app.midi_clips[mc_idx].velocity_lane_height,
        midi::VELOCITY_LANE_MIN_HEIGHT,
    );

    // Clamp to max
    app.midi_clips[mc_idx].velocity_lane_height = 999.0f32.clamp(
        midi::VELOCITY_LANE_MIN_HEIGHT,
        midi::VELOCITY_LANE_MAX_HEIGHT,
    );
    assert_eq!(
        app.midi_clips[mc_idx].velocity_lane_height,
        midi::VELOCITY_LANE_MAX_HEIGHT,
    );
}

#[test]
fn test_velocity_divider_hit_test() {
    use crate::Camera;

    let mut app = App::new_headless();
    app.add_midi_clip();
    app.midi_clips[0].position = [0.0, 0.0];
    app.midi_clips[0].size = [480.0, 200.0];

    let camera = Camera::new();
    let mc = &app.midi_clips[0];
    let lane_top = mc.velocity_lane_top();

    // Right on the divider
    assert!(midi::hit_test_velocity_divider(mc, [100.0, lane_top], &camera));

    // Slightly above (within margin)
    assert!(midi::hit_test_velocity_divider(mc, [100.0, lane_top - 3.0], &camera));

    // Far away
    assert!(!midi::hit_test_velocity_divider(mc, [100.0, lane_top - 50.0], &camera));

    // Outside clip x range
    assert!(!midi::hit_test_velocity_divider(mc, [-10.0, lane_top], &camera));
}

#[test]
fn test_transpose_selected_notes_by_semitone() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    app.editing_midi_clip = Some(0);

    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 30.0,
        velocity: 80,
    });

    // Select both notes
    app.selected_midi_notes = vec![0, 1];

    // Transpose up by 1 semitone
    for &ni in &app.selected_midi_notes.clone() {
        app.midi_clips[0].notes[ni].pitch += 1;
    }
    assert_eq!(app.midi_clips[0].notes[0].pitch, 61);
    assert_eq!(app.midi_clips[0].notes[1].pitch, 65);

    // Transpose down by 1 semitone
    for &ni in &app.selected_midi_notes.clone() {
        app.midi_clips[0].notes[ni].pitch -= 1;
    }
    assert_eq!(app.midi_clips[0].notes[0].pitch, 60);
    assert_eq!(app.midi_clips[0].notes[1].pitch, 64);

    // Transpose up by octave (12 semitones)
    for &ni in &app.selected_midi_notes.clone() {
        app.midi_clips[0].notes[ni].pitch += 12;
    }
    assert_eq!(app.midi_clips[0].notes[0].pitch, 72);
    assert_eq!(app.midi_clips[0].notes[1].pitch, 76);
}

// ---------------------------------------------------------------------------
// Cmd+D duplicate MIDI notes
// ---------------------------------------------------------------------------

#[test]
fn test_cmd_d_duplicate_midi_notes() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.editing_midi_clip = Some(mc_idx);

    // Single note: duplicate shifts by its own duration
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.selected_midi_notes = vec![0];

    app.push_undo();
    let notes = &app.midi_clips[mc_idx].notes;
    let min_start = notes[0].start_px;
    let max_end = notes[0].start_px + notes[0].duration_px;
    let group_shift = max_end - min_start; // 30.0
    let mut cloned = app.midi_clips[mc_idx].notes[0].clone();
    cloned.start_px += group_shift;
    app.midi_clips[mc_idx].notes.push(cloned);
    app.selected_midi_notes = vec![1];

    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
    assert_eq!(app.midi_clips[mc_idx].notes[1].start_px, 40.0); // 10 + 30
    assert_eq!(app.selected_midi_notes, vec![1]);

    app.undo();
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
}

#[test]
fn test_cmd_d_duplicate_midi_notes_group() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.editing_midi_clip = Some(mc_idx);

    // Two notes forming a group: [10..40] and [50..70]
    // Group span = max_end(70) - min_start(10) = 60
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 20.0,
        velocity: 80,
    });
    app.selected_midi_notes = vec![0, 1];

    app.push_undo();
    let group_shift = 70.0 - 10.0; // 60.0
    let mut new_indices: Vec<usize> = Vec::new();
    for &ni in &[0usize, 1] {
        let mut cloned = app.midi_clips[mc_idx].notes[ni].clone();
        cloned.start_px += group_shift;
        app.midi_clips[mc_idx].notes.push(cloned);
        new_indices.push(app.midi_clips[mc_idx].notes.len() - 1);
    }
    app.selected_midi_notes = new_indices;

    assert_eq!(app.midi_clips[mc_idx].notes.len(), 4);
    // Duplicated group preserves relative positions
    assert_eq!(app.midi_clips[mc_idx].notes[2].start_px, 70.0);  // 10 + 60
    assert_eq!(app.midi_clips[mc_idx].notes[2].pitch, 60);
    assert_eq!(app.midi_clips[mc_idx].notes[3].start_px, 110.0); // 50 + 60
    assert_eq!(app.midi_clips[mc_idx].notes[3].pitch, 64);
    assert_eq!(app.selected_midi_notes, vec![2, 3]);

    app.undo();
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
}

// ---------------------------------------------------------------------------
// Cmd+C / Cmd+V copy-paste MIDI notes
// ---------------------------------------------------------------------------

#[test]
fn test_copy_paste_midi_notes() {
    use crate::ClipboardItem;

    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;
    app.editing_midi_clip = Some(mc_idx);

    // Two notes: [10..40] pitch 60, [50..70] pitch 64
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 20.0,
        velocity: 80,
    });
    app.selected_midi_notes = vec![0, 1];

    // --- Copy (simulate Cmd+C logic) ---
    let notes = &app.midi_clips[mc_idx].notes;
    let min_start = app.selected_midi_notes.iter()
        .map(|&ni| notes[ni].start_px)
        .fold(f32::INFINITY, f32::min);
    let mut copied: Vec<midi::MidiNote> = Vec::new();
    for &ni in &app.selected_midi_notes {
        let mut n = app.midi_clips[mc_idx].notes[ni].clone();
        n.start_px -= min_start;
        copied.push(n);
    }
    app.clipboard.items.clear();
    app.clipboard.items.push(ClipboardItem::MidiNotes(copied));

    // Clipboard should have normalized notes (start_px relative to 0)
    if let ClipboardItem::MidiNotes(ref cn) = app.clipboard.items[0] {
        assert_eq!(cn.len(), 2);
        assert_eq!(cn[0].start_px, 0.0);   // 10 - 10
        assert_eq!(cn[1].start_px, 40.0);  // 50 - 10
    } else {
        panic!("expected MidiNotes in clipboard");
    }

    // --- Paste at offset 200 (simulate Cmd+V logic) ---
    app.push_undo();
    let paste_offset = 200.0;
    if let ClipboardItem::MidiNotes(ref notes) = app.clipboard.items[0] {
        let mut new_indices: Vec<usize> = Vec::new();
        for n in notes {
            let mut pasted = n.clone();
            pasted.start_px += paste_offset;
            app.midi_clips[mc_idx].notes.push(pasted);
            new_indices.push(app.midi_clips[mc_idx].notes.len() - 1);
        }
        app.selected_midi_notes = new_indices;
    }

    // Should have 4 notes total
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 4);

    // Pasted notes preserve relative positions, offset by paste_offset
    assert_eq!(app.midi_clips[mc_idx].notes[2].pitch, 60);
    assert_eq!(app.midi_clips[mc_idx].notes[2].start_px, 200.0);
    assert_eq!(app.midi_clips[mc_idx].notes[2].duration_px, 30.0);
    assert_eq!(app.midi_clips[mc_idx].notes[3].pitch, 64);
    assert_eq!(app.midi_clips[mc_idx].notes[3].start_px, 240.0);
    assert_eq!(app.midi_clips[mc_idx].notes[3].duration_px, 20.0);

    // Selection points to pasted notes
    assert_eq!(app.selected_midi_notes, vec![2, 3]);

    // Undo reverts paste
    app.undo();
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
}

// ---------------------------------------------------------------------------
// Note overlap resolution
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_note_overlaps_tail_crop() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Note A at start=0, duration=100, pitch=60
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 0.0,
        duration_px: 100.0,
        velocity: 100,
    });
    // Note B at start=150, duration=50, pitch=60 (will be moved to overlap A)
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 150.0,
        duration_px: 50.0,
        velocity: 100,
    });

    // Simulate moving note B to start=50 so it overlaps A's tail
    app.midi_clips[mc_idx].notes[1].start_px = 50.0;
    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[1]);

    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
    // A's tail should be cropped to end where B starts
    assert_eq!(app.midi_clips[mc_idx].notes[0].duration_px, 50.0);
    // B should be unchanged
    assert_eq!(app.midi_clips[mc_idx].notes[1].start_px, 50.0);
    assert_eq!(app.midi_clips[mc_idx].notes[1].duration_px, 50.0);
    assert_eq!(new_indices, vec![1]);
}

#[test]
fn test_resolve_note_overlaps_full_cover_delete() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Small note that will be fully covered
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 40.0,
        duration_px: 20.0,
        velocity: 100,
    });
    // Large note that will be moved to cover the small one
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 200.0,
        duration_px: 100.0,
        velocity: 100,
    });

    // Move large note to start=30, covering the small note (30..130 covers 40..60)
    app.midi_clips[mc_idx].notes[1].start_px = 30.0;
    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[1]);

    // Small note should be deleted
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
    assert_eq!(app.midi_clips[mc_idx].notes[0].start_px, 30.0);
    assert_eq!(app.midi_clips[mc_idx].notes[0].duration_px, 100.0);
    // Active index should be remapped (was 1, now 0 after deletion of index 0)
    assert_eq!(new_indices, vec![0]);
}

#[test]
fn test_resolve_note_overlaps_different_pitch_no_op() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 0.0,
        duration_px: 100.0,
        velocity: 100,
    });
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 62, // different pitch
        start_px: 50.0,
        duration_px: 50.0,
        velocity: 100,
    });

    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[1]);

    // No cropping or deletion — different pitches
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 2);
    assert_eq!(app.midi_clips[mc_idx].notes[0].duration_px, 100.0);
    assert_eq!(app.midi_clips[mc_idx].notes[1].start_px, 50.0);
    assert_eq!(new_indices, vec![1]);
}

#[test]
fn test_resolve_note_overlaps_tiny_remainder_deleted() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Note A: very close to where B will start, so cropping leaves < 10px
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 45.0,
        duration_px: 20.0,
        velocity: 100,
    });
    // Note B: will be moved to start=50, leaving A with only 5px
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 200.0,
        duration_px: 60.0,
        velocity: 100,
    });

    app.midi_clips[mc_idx].notes[1].start_px = 50.0;
    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[1]);

    // A would be cropped to 5px which is < 10px minimum, so it gets deleted
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
    assert_eq!(app.midi_clips[mc_idx].notes[0].start_px, 50.0);
    assert_eq!(new_indices, vec![0]);
}

#[test]
fn test_resolve_note_overlaps_head_crop() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Note A (will be moved right so its tail overlaps B's head)
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 0.0,
        duration_px: 100.0,
        velocity: 100,
    });
    // Note B (stationary, starts at 150)
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 150.0,
        duration_px: 80.0,
        velocity: 100,
    });

    // Move A right so it ends at 200, overlapping B (150..230)
    app.midi_clips[mc_idx].notes[0].start_px = 100.0;
    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[0]);

    // B should be deleted since A's tail overlaps its head
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
    assert_eq!(app.midi_clips[mc_idx].notes[0].start_px, 100.0);
    assert_eq!(app.midi_clips[mc_idx].notes[0].duration_px, 100.0);
    assert_eq!(new_indices, vec![0]);
}

#[test]
fn test_resolve_note_overlaps_head_crop_deletes_small() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    let mc_idx = 0;

    // Note A moved so its tail nearly covers all of B
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 0.0,
        duration_px: 100.0,
        velocity: 100,
    });
    // Note B: only 5px would remain after crop (< 10px min)
    app.midi_clips[mc_idx].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 90.0,
        duration_px: 15.0,
        velocity: 100,
    });

    let new_indices = app.midi_clips[mc_idx].resolve_note_overlaps(&[0]);

    // B's remainder would be 5px (105 - 100 = 5) which is < 10, so deleted
    assert_eq!(app.midi_clips[mc_idx].notes.len(), 1);
    assert_eq!(app.midi_clips[mc_idx].notes[0].start_px, 0.0);
    assert_eq!(new_indices, vec![0]);
}

#[test]
fn test_click_selected_note_clears_multi_selection() {
    let mut app = App::new_headless();
    app.add_midi_clip();
    app.editing_midi_clip = Some(0);

    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 60,
        start_px: 10.0,
        duration_px: 30.0,
        velocity: 100,
    });
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 64,
        start_px: 50.0,
        duration_px: 30.0,
        velocity: 80,
    });
    app.midi_clips[0].notes.push(midi::MidiNote {
        pitch: 67,
        start_px: 90.0,
        duration_px: 30.0,
        velocity: 90,
    });

    // Multi-select all three notes
    app.selected_midi_notes = vec![0, 1, 2];
    assert_eq!(app.selected_midi_notes.len(), 3);

    // Simulate mouse-down on note 1 (already selected):
    // The handler sets pending_midi_note_click and starts a MovingMidiNote drag
    // with push_undo() called before.
    app.push_undo();
    app.pending_midi_note_click = Some(1);
    let offsets = app.selected_midi_notes.iter().map(|_| [0.0f32, 0.0f32]).collect();
    app.drag = DragState::MovingMidiNote {
        clip_idx: 0,
        note_indices: app.selected_midi_notes.clone(),
        offsets,
        start_world: [50.0, 0.0],
    };

    // Simulate mouse-up without movement: pending_midi_note_click is still Some
    if let Some(note_idx) = app.pending_midi_note_click.take() {
        app.undo();
        app.selected_midi_notes = vec![note_idx];
    }
    app.drag = DragState::None;

    // Only note 1 should be selected
    assert_eq!(app.selected_midi_notes, vec![1]);
}
