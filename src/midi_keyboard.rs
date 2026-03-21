//! Computer-keyboard → MIDI note mapping (Ableton-style: A row = C major octave + sharps on WETYU).

use winit::keyboard::{KeyCode, PhysicalKey};

pub const DEFAULT_VELOCITY: u8 = 100;
pub const VELOCITY_STEP: u8 = 8;
pub const OCTAVE_OFFSET_MAX: i8 = 3;

/// White keys A–K and black keys W E T Y U: MIDI note numbers before octave shift (C4 = 60 on A).
pub fn piano_key_midi_before_octave(physical: &PhysicalKey) -> Option<u8> {
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    match code {
        KeyCode::KeyA => Some(60),
        KeyCode::KeyS => Some(62),
        KeyCode::KeyD => Some(64),
        KeyCode::KeyF => Some(65),
        KeyCode::KeyG => Some(67),
        KeyCode::KeyH => Some(69),
        KeyCode::KeyJ => Some(71),
        KeyCode::KeyK => Some(72),
        KeyCode::KeyW => Some(61),
        KeyCode::KeyE => Some(63),
        KeyCode::KeyT => Some(66),
        KeyCode::KeyY => Some(68),
        KeyCode::KeyU => Some(70),
        _ => None,
    }
}

/// Apply octave offset (Z/X) in steps of ±1 octave, clamped to valid MIDI.
pub fn with_octave_offset(base: u8, octave_offset: i8) -> Option<u8> {
    let o = octave_offset.clamp(-OCTAVE_OFFSET_MAX, OCTAVE_OFFSET_MAX);
    let n = base as i16 + (o as i16) * 12;
    if (0..=127).contains(&n) {
        Some(n as u8)
    } else {
        None
    }
}

pub fn adjust_velocity(v: u8, delta: i16) -> u8 {
    (v as i16 + delta).clamp(1, 127) as u8
}
