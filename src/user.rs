use crate::entity_id::EntityId;

pub type UserId = EntityId;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub color: [f32; 4],
}

/// Preview of a remote user's in-progress drag operation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DragPreview {
    MovingEntities {
        /// (target, position, size)
        targets: Vec<(crate::HitTarget, [f32; 2], [f32; 2])>,
    },
    ResizingEntity {
        target: crate::HitTarget,
        new_position: [f32; 2],
        new_size: [f32; 2],
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RemoteViewport {
    pub position: [f32; 2],
    pub zoom: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RemotePlaybackState {
    pub is_playing: bool,
    pub position_seconds: f64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RemoteUserState {
    pub user: User,
    pub cursor_world: Option<[f32; 2]>,
    pub drag_preview: Option<DragPreview>,
    pub online: bool,
    pub viewport: Option<RemoteViewport>,
    pub playback: Option<RemotePlaybackState>,
}

/// Pre-defined colors for remote user cursors.
pub const USER_COLORS: &[[f32; 4]] = &[
    [0.35, 0.78, 0.98, 1.0], // sky blue
    [0.30, 0.85, 0.39, 1.0], // green
    [1.00, 0.58, 0.00, 1.0], // orange
    [0.88, 0.25, 0.63, 1.0], // magenta
    [0.69, 0.32, 0.87, 1.0], // violet
    [1.00, 0.84, 0.00, 1.0], // yellow
    [0.19, 0.84, 0.55, 1.0], // mint
    [1.00, 0.24, 0.19, 1.0], // red
];

pub fn color_for_user_index(idx: usize) -> [f32; 4] {
    USER_COLORS[idx % USER_COLORS.len()]
}

/// Ephemeral messages that are NOT persisted — used for cursor sync, presence, etc.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum EphemeralMessage {
    CursorMove {
        user_id: UserId,
        position: [f32; 2],
    },
    DragUpdate {
        user_id: UserId,
        preview: DragPreview,
    },
    DragEnd {
        user_id: UserId,
    },
    UserJoined {
        user: User,
    },
    UserLeft {
        user_id: UserId,
    },
    ViewportUpdate {
        user_id: UserId,
        position: [f32; 2],
        zoom: f32,
    },
    PlaybackUpdate {
        user_id: UserId,
        is_playing: bool,
        position_seconds: f64,
        timestamp_ms: u64,
    },
}
