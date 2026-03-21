use crate::entity_id::EntityId;
use crate::ui::hit_testing::point_in_rect;
use crate::settings::{GridMode, Settings};
use crate::{push_border, Camera, InstanceRaw};

// ---------------------------------------------------------------------------
// MIDI data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MidiNote {
    pub pitch: u8,        // MIDI note 0-127
    pub start_px: f32,    // relative to clip left edge, in pixels
    pub duration_px: f32, // in pixels
    pub velocity: u8,     // 0-127, default 100
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MidiClip {
    pub position: [f32; 2],
    pub size: [f32; 2], // [width, height]
    pub color: [f32; 4],
    pub notes: Vec<MidiNote>,
    pub pitch_range: (u8, u8), // (low, high) e.g. (48, 84) = C3-C6
    pub grid_mode: GridMode,
    pub triplet_grid: bool,
    pub velocity_lane_height: f32,
}

pub const MIDI_CLIP_DEFAULT_HEIGHT: f32 = 540.0;
pub const MIDI_CLIP_DEFAULT_BARS: u32 = 4;
pub const MIDI_CLIP_DEFAULT_SIZE: [f32; 2] = [480.0, 540.0];
pub use crate::theme::MIDI_CLIP_DEFAULT_COLOR;
pub const MIDI_CLIP_DEFAULT_PITCH_RANGE: (u8, u8) = (12, 109); // C0-C8
pub const DEFAULT_NOTE_DURATION_PX: f32 = 30.0;
pub const VELOCITY_LANE_HEIGHT: f32 = 40.0;
pub const VELOCITY_LANE_MIN_HEIGHT: f32 = 20.0;
pub const VELOCITY_LANE_MAX_HEIGHT: f32 = 150.0;
const VELOCITY_LANE_DIVIDER: f32 = 1.0;
const VELOCITY_DIVIDER_HIT_PX: f32 = 8.0;

impl MidiClip {
    pub fn new(position: [f32; 2], settings: &Settings) -> Self {
        Self {
            position,
            size: MIDI_CLIP_DEFAULT_SIZE,
            color: MIDI_CLIP_DEFAULT_COLOR,
            notes: Vec::new(),
            pitch_range: MIDI_CLIP_DEFAULT_PITCH_RANGE,
            grid_mode: settings.grid_mode,
            triplet_grid: settings.triplet_grid,
            velocity_lane_height: VELOCITY_LANE_HEIGHT,
        }
    }

    pub fn note_area_height(&self, editing: bool) -> f32 {
        // TODO: refactor velocity lane rendering before re-enabling
        // if editing {
        //     (self.size[1] - self.velocity_lane_height).max(20.0)
        // } else {
        //     self.size[1]
        // }
        let _ = editing;
        self.size[1]
    }

    pub fn velocity_lane_top(&self) -> f32 {
        self.position[1] + self.note_area_height(true)
    }

    pub fn note_height(&self) -> f32 {
        self.note_height_editing(false)
    }

    pub fn note_height_editing(&self, editing: bool) -> f32 {
        let range = (self.pitch_range.1 - self.pitch_range.0) as f32;
        if range > 0.0 {
            self.note_area_height(editing) / range
        } else {
            self.note_area_height(editing)
        }
    }

    /// Convert a pitch to Y position (world coords) within this clip.
    pub fn pitch_to_y(&self, pitch: u8) -> f32 {
        self.pitch_to_y_editing(pitch, false)
    }

    pub fn pitch_to_y_editing(&self, pitch: u8, editing: bool) -> f32 {
        let nh = self.note_height_editing(editing);
        let area_h = self.note_area_height(editing);
        self.position[1] + area_h - (pitch as f32 - self.pitch_range.0 as f32) * nh - nh
    }

    /// Convert a world Y position to pitch.
    pub fn y_to_pitch(&self, y: f32) -> u8 {
        self.y_to_pitch_editing(y, false)
    }

    pub fn y_to_pitch_editing(&self, y: f32, editing: bool) -> u8 {
        let nh = self.note_height_editing(editing);
        let area_h = self.note_area_height(editing);
        let relative = self.position[1] + area_h - y;
        let pitch = (relative / nh) as u8 + self.pitch_range.0;
        pitch.clamp(self.pitch_range.0, self.pitch_range.1 - 1)
    }

    /// Convert a world X position to start_px (relative to clip left).
    pub fn x_to_start_px(&self, x: f32) -> f32 {
        (x - self.position[0]).max(0.0)
    }

    pub fn contains(&self, world_pos: [f32; 2]) -> bool {
        point_in_rect(world_pos, self.position, self.size)
    }

    /// After moving/resizing notes, resolve overlaps on the same pitch.
    /// For each active note, crops the tail of any other same-pitch note that
    /// overlaps, and deletes notes that are fully covered.
    /// Returns updated active indices (accounting for deletions).
    pub fn resolve_note_overlaps(&mut self, active_indices: &[usize]) -> Vec<usize> {
        use std::collections::HashSet;
        let active_set: HashSet<usize> = active_indices.iter().copied().collect();
        let mut to_delete: HashSet<usize> = HashSet::new();

        for &ai in active_indices {
            if ai >= self.notes.len() {
                continue;
            }
            let a_pitch = self.notes[ai].pitch;
            let a_start = self.notes[ai].start_px;
            let a_end = a_start + self.notes[ai].duration_px;

            for j in 0..self.notes.len() {
                if j == ai || active_set.contains(&j) || to_delete.contains(&j) {
                    continue;
                }
                let other = &self.notes[j];
                if other.pitch != a_pitch {
                    continue;
                }
                let o_start = other.start_px;
                let o_end = o_start + other.duration_px;

                if o_start >= a_start && o_end <= a_end {
                    to_delete.insert(j);
                    continue;
                }

                // Other note's tail extends past active note's start → crop tail
                if o_start < a_start && o_end > a_start {
                    self.notes[j].duration_px = a_start - o_start;
                    if self.notes[j].duration_px < 10.0 {
                        to_delete.insert(j);
                    }
                }

                // Active note's tail extends into other note's head → delete
                if o_start >= a_start && o_start < a_end && o_end > a_end {
                    to_delete.insert(j);
                }
            }
        }

        let mut delete_sorted: Vec<usize> = to_delete.into_iter().collect();
        delete_sorted.sort_unstable();

        let new_active: Vec<usize> = active_indices
            .iter()
            .map(|&i| {
                let shift = delete_sorted.iter().filter(|&&d| d < i).count();
                i - shift
            })
            .collect();

        for &i in delete_sorted.iter().rev() {
            self.notes.remove(i);
        }

        new_active
    }
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiNoteHitZone {
    Body,
    LeftEdge,
    RightEdge,
    VelocityBar,
}

const NOTE_EDGE_HIT_PX: f32 = 20.0;

pub fn hit_test_midi_note(
    clip: &MidiClip,
    world_pos: [f32; 2],
    camera: &Camera,
) -> Option<(usize, MidiNoteHitZone)> {
    hit_test_midi_note_editing(clip, world_pos, camera, false)
}

pub fn hit_test_midi_note_editing(
    clip: &MidiClip,
    world_pos: [f32; 2],
    camera: &Camera,
    editing: bool,
) -> Option<(usize, MidiNoteHitZone)> {
    let nh = clip.note_height_editing(editing);
    let edge_margin = NOTE_EDGE_HIT_PX / camera.zoom;

    // When editing, only hit-test in the note area (above velocity lane)
    if editing && world_pos[1] >= clip.velocity_lane_top() {
        return None;
    }

    for (i, note) in clip.notes.iter().enumerate().rev() {
        let nx = clip.position[0] + note.start_px;
        let ny = clip.pitch_to_y_editing(note.pitch, editing);
        let nw = note.duration_px;
        if point_in_rect(world_pos, [nx, ny], [nw, nh]) {
            if (world_pos[0] - (nx + nw)).abs() < edge_margin {
                return Some((i, MidiNoteHitZone::RightEdge));
            }
            if (world_pos[0] - nx).abs() < edge_margin {
                return Some((i, MidiNoteHitZone::LeftEdge));
            }
            return Some((i, MidiNoteHitZone::Body));
        }
    }
    None
}

/// Hit-test the velocity lane divider for resizing. Returns true if near the divider.
pub fn hit_test_velocity_divider(
    clip: &MidiClip,
    world_pos: [f32; 2],
    camera: &Camera,
) -> bool {
    let lane_top = clip.velocity_lane_top();
    let margin = VELOCITY_DIVIDER_HIT_PX / camera.zoom;
    let in_x = world_pos[0] >= clip.position[0]
        && world_pos[0] <= clip.position[0] + clip.size[0];
    let near_divider = (world_pos[1] - lane_top).abs() < margin;
    in_x && near_divider
}

/// Hit-test the velocity lane area. Returns the note index if a bar is hit.
pub fn hit_test_velocity_bar(
    clip: &MidiClip,
    world_pos: [f32; 2],
    _camera: &Camera,
) -> Option<usize> {
    let lane_top = clip.velocity_lane_top();
    let lane_bottom = clip.position[1] + clip.size[1];
    if world_pos[1] < lane_top || world_pos[1] > lane_bottom {
        return None;
    }
    if world_pos[0] < clip.position[0] || world_pos[0] > clip.position[0] + clip.size[0] {
        return None;
    }

    // Check each note's bar (reverse order for topmost-first)
    for (i, note) in clip.notes.iter().enumerate().rev() {
        let nx = clip.position[0] + note.start_px;
        let nw = note.duration_px;
        if world_pos[0] >= nx && world_pos[0] <= nx + nw {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn build_midi_clip_instances(
    clip: &MidiClip,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    editing: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    // Background rectangle
    let mut bg_color = [clip.color[0], clip.color[1], clip.color[2], 0.15];
    if is_hovered && !is_selected {
        bg_color[3] = 0.22;
    }
    out.push(InstanceRaw {
        position: clip.position,
        size: clip.size,
        color: bg_color,
        border_radius: 4.0 / camera.zoom,
    });

    // Border
    let bw = if is_selected { 2.5 } else { 1.5 } / camera.zoom;
    let mut bc = clip.color;
    bc[3] = 0.60;
    if is_hovered && !is_selected {
        bc[3] = 0.75;
    }
    push_border(&mut out, clip.position, clip.size, bw, bc);

    // Pitch grid lines (for black keys) - use note area only when editing
    let nh = clip.note_height_editing(editing);
    for pitch in clip.pitch_range.0..clip.pitch_range.1 {
        let y = clip.pitch_to_y_editing(pitch, editing);
        let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
        if is_black {
            out.push(InstanceRaw {
                position: [clip.position[0], y],
                size: [clip.size[0], nh],
                color: [0.0, 0.0, 0.0, 0.10],
                border_radius: 0.0,
            });
        }
        if pitch % 12 == 0 {
            let line_h = 1.0 / camera.zoom;
            out.push(InstanceRaw {
                position: [clip.position[0], y + nh - line_h * 0.5],
                size: [clip.size[0], line_h],
                color: [1.0, 1.0, 1.0, 0.12],
                border_radius: 0.0,
            });
        }
    }

    out
}

pub fn build_midi_note_instances(
    clip: &MidiClip,
    camera: &Camera,
    selected_notes: &[usize],
    editing: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();
    let nh = clip.note_height_editing(editing);

    for (i, note) in clip.notes.iter().enumerate() {
        let nx = clip.position[0] + note.start_px;
        let ny = clip.pitch_to_y_editing(note.pitch, editing);
        let nw = note.duration_px;

        let vel_factor = note.velocity as f32 / 127.0;
        let brightness = 0.5 + vel_factor * 0.5;
        let note_color = [
            clip.color[0] * brightness,
            clip.color[1] * brightness,
            (clip.color[2] * brightness).min(1.0),
            0.85,
        ];

        out.push(InstanceRaw {
            position: [nx, ny],
            size: [nw, nh.max(2.0 / camera.zoom)],
            color: note_color,
            border_radius: 2.0 / camera.zoom,
        });

        let border_bw = 1.0 / camera.zoom;
        push_border(&mut out, [nx, ny], [nw, nh], border_bw, [1.0, 1.0, 1.0, 0.35]);

        if selected_notes.contains(&i) {
            let sel_bw = 1.5 / camera.zoom;
            push_border(&mut out, [nx, ny], [nw, nh], sel_bw, [1.0, 1.0, 1.0, 0.8]);
        }
    }

    out
}

pub fn build_velocity_lane_instances(
    clip: &MidiClip,
    camera: &Camera,
    selected_notes: &[usize],
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();
    let lane_top = clip.velocity_lane_top();
    let lane_height = clip.velocity_lane_height;
    let lane_bottom = lane_top + lane_height;
    let clip_x = clip.position[0];
    let clip_w = clip.size[0];

    // Lane background
    out.push(InstanceRaw {
        position: [clip_x, lane_top],
        size: [clip_w, lane_height],
        color: [0.0, 0.0, 0.0, 0.20],
        border_radius: 0.0,
    });

    // Divider line between note area and velocity lane
    let div_h = VELOCITY_LANE_DIVIDER / camera.zoom;
    out.push(InstanceRaw {
        position: [clip_x, lane_top - div_h * 0.5],
        size: [clip_w, div_h],
        color: [1.0, 1.0, 1.0, 0.25],
        border_radius: 0.0,
    });

    let stem_w = 2.0 / camera.zoom;
    let dot_radius = (2.0 + 1.5 * camera.zoom).min(lane_height * 0.15);
    let padding = 2.0 / camera.zoom;
    let usable_height = lane_height - padding - dot_radius;

    for (i, note) in clip.notes.iter().enumerate() {
        let note_cx = clip_x + note.start_px + note.duration_px * 0.5;
        let vel_factor = note.velocity as f32 / 127.0;
        let stem_height = vel_factor * usable_height;
        let stem_top = lane_bottom - padding - stem_height;
        let is_selected = selected_notes.contains(&i);

        let brightness = 0.5 + vel_factor * 0.5;
        let color = [
            clip.color[0] * brightness,
            clip.color[1] * brightness,
            (clip.color[2] * brightness).min(1.0),
            if is_selected { 1.0 } else { 0.75 },
        ];

        // Stem line
        out.push(InstanceRaw {
            position: [note_cx - stem_w * 0.5, stem_top],
            size: [stem_w, stem_height],
            color,
            border_radius: 0.0,
        });

        // Circle at top of stem
        let dot_size = dot_radius * 2.0;
        out.push(InstanceRaw {
            position: [note_cx - dot_radius, stem_top - dot_radius],
            size: [dot_size, dot_size],
            color,
            border_radius: dot_radius,
        });

        if is_selected {
            let ring_w = 1.0 / camera.zoom;
            let ring_size = dot_size + ring_w * 2.0;
            push_border(
                &mut out,
                [note_cx - dot_radius - ring_w, stem_top - dot_radius - ring_w],
                [ring_size, ring_size],
                ring_w,
                [1.0, 1.0, 1.0, 0.8],
            );
        }
    }

    out
}

/// Get note name from MIDI pitch (for piano roll labels)
pub fn note_name(pitch: u8) -> String {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (pitch / 12) as i32 - 1;
    let name = names[(pitch % 12) as usize];
    format!("{}{}", name, octave)
}
