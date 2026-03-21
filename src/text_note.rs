use crate::entity_id::EntityId;

pub const DEFAULT_SIZE: [f32; 2] = [200.0, 120.0];
pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const DEFAULT_COLOR: [f32; 4] = [0.15, 0.15, 0.18, 1.0];
pub const DEFAULT_TEXT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const DEFAULT_BORDER_RADIUS: f32 = 6.0;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextNote {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
    pub text: String,
    pub font_size: f32,
    pub text_color: [f32; 4],
}

impl TextNote {
    pub fn new(position: [f32; 2]) -> Self {
        Self {
            position,
            size: DEFAULT_SIZE,
            color: DEFAULT_COLOR,
            border_radius: DEFAULT_BORDER_RADIUS,
            text: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            text_color: DEFAULT_TEXT_COLOR,
        }
    }
}

/// Transient editing state — not serialized.
pub struct TextNoteEditState {
    pub note_id: EntityId,
    pub text: String,
    pub before_text: String,
    pub cursor: usize,
}
