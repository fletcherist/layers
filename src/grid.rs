use crate::settings::{GridMode, Settings};

pub(crate) const DEFAULT_BPM: f32 = 120.0;
pub const PIXELS_PER_SECOND: f32 = 120.0;

pub(crate) fn pixels_per_beat(bpm: f32) -> f32 {
    PIXELS_PER_SECOND * 60.0 / bpm
}

/// Musical subdivision levels in beats: 32, 16, 8, 4, 2, 1, 1/2, 1/4, 1/8, 1/16, 1/32
pub(crate) const BEAT_SUBDIVISIONS: &[f32] = &[
    32.0, 16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 0.0625, 0.03125,
];

/// Returns (minor_spacing_world, beats_per_bar) for adaptive grid.
/// Picks the subdivision where screen-px spacing is closest to the target.
pub(crate) fn musical_grid_spacing(zoom: f32, target_px: f32, triplet: bool, bpm: f32) -> f32 {
    let ppb = pixels_per_beat(bpm);
    let triplet_mul = if triplet { 2.0 / 3.0 } else { 1.0 };
    let mut best = BEAT_SUBDIVISIONS[0] * ppb * triplet_mul;
    let mut best_diff = f32::MAX;
    for &subdiv in BEAT_SUBDIVISIONS {
        let world_spacing = subdiv * ppb * triplet_mul;
        let screen_spacing = world_spacing * zoom;
        let diff = (screen_spacing - target_px).abs();
        if diff < best_diff {
            best_diff = diff;
            best = world_spacing;
        }
    }
    best
}

pub(crate) fn grid_spacing_for_settings(settings: &Settings, zoom: f32, bpm: f32) -> f32 {
    match settings.grid_mode {
        GridMode::Adaptive(size) => {
            musical_grid_spacing(zoom, size.target_px(), settings.triplet_grid, bpm)
        }
        GridMode::Fixed(fg) => {
            let ppb = pixels_per_beat(bpm);
            let triplet_mul = if settings.triplet_grid {
                2.0 / 3.0
            } else {
                1.0
            };
            fg.beats() * ppb * triplet_mul
        }
    }
}

/// Snap a world-X coordinate to the nearest grid line.
pub(crate) fn snap_to_grid(world_x: f32, settings: &Settings, zoom: f32, bpm: f32) -> f32 {
    if !settings.grid_enabled || !settings.snap_to_grid {
        return world_x;
    }
    let spacing = grid_spacing_for_settings(settings, zoom, bpm);
    (world_x / spacing).round() * spacing
}

/// Snap a world-Y coordinate to the same musical grid used horizontally.
pub(crate) fn snap_to_vertical_grid(world_y: f32, settings: &Settings, zoom: f32, bpm: f32) -> f32 {
    if !settings.snap_to_vertical_grid {
        return world_y;
    }
    let spacing = grid_spacing_for_settings(settings, zoom, bpm);
    (world_y / spacing).round() * spacing
}

/// Grid spacing using a clip's own grid_mode / triplet_grid.
pub(crate) fn clip_grid_spacing(grid_mode: GridMode, triplet: bool, zoom: f32, bpm: f32) -> f32 {
    match grid_mode {
        GridMode::Adaptive(size) => musical_grid_spacing(zoom, size.target_px(), triplet, bpm),
        GridMode::Fixed(fg) => {
            let ppb = pixels_per_beat(bpm);
            let triplet_mul = if triplet { 2.0 / 3.0 } else { 1.0 };
            fg.beats() * ppb * triplet_mul
        }
    }
}

/// Snap using a clip's own grid settings (respects project snap_to_grid toggle).
pub(crate) fn snap_to_clip_grid(
    world_x: f32,
    settings: &Settings,
    grid_mode: GridMode,
    triplet: bool,
    zoom: f32,
    bpm: f32,
) -> f32 {
    if !settings.grid_enabled || !settings.snap_to_grid {
        return world_x;
    }
    let spacing = clip_grid_spacing(grid_mode, triplet, zoom, bpm);
    (world_x / spacing).round() * spacing
}
