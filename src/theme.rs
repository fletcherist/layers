// ---------------------------------------------------------------------------
// src/theme.rs — centralized color palette
// ---------------------------------------------------------------------------
// Every visual color used in the app lives here as a named constant so the
// entire color scheme can be changed in one place.
//
// Groups:
//   Backgrounds · Accents · Interactive · Transport · Scrollbars · RMS
//   Regions · Entity colors · Waveform palette · Helper

// --- Backgrounds ---
pub const BG_BASE: [f32; 4]    = [0.11, 0.11, 0.14, 1.0]; // main canvas, panels
pub const BG_SURFACE: [f32; 4] = [0.13, 0.13, 0.17, 1.0]; // headers, elevated panels
pub const BG_MENU: [f32; 4]    = [0.16, 0.16, 0.19, 1.0]; // context menu background
pub const BG_OVERLAY: [f32; 4] = [0.14, 0.14, 0.17, 0.98]; // palette, modal overlays

// --- Accents ---
pub const ACCENT: [f32; 4]       = [0.25, 0.55, 1.0, 1.0];  // primary blue
pub const ACCENT_MUTED: [f32; 4] = [0.25, 0.50, 0.90, 0.60]; // badges, pills
pub const ACCENT_FAINT: [f32; 4] = [0.25, 0.55, 1.0, 0.08]; // loop region fill

// --- Interactive ---
pub const HOVER: [f32; 4]     = [1.0, 1.0, 1.0, 0.06];
pub const SELECTION: [f32; 4] = [0.35, 0.65, 1.0, 0.8];

// --- Playhead & Transport ---
pub const PLAYHEAD: [f32; 4]      = [0.20, 0.80, 0.60, 0.9];
pub const RECORD_ACTIVE: [f32; 4] = [0.95, 0.20, 0.20, 1.0];
pub const RECORD_DIM: [f32; 4]    = [0.85, 0.25, 0.25, 0.9];

// --- Scrollbars ---
pub const SCROLLBAR_BG: [f32; 4]    = [1.0, 1.0, 1.0, 0.08];
pub const SCROLLBAR_THUMB: [f32; 4] = [1.0, 1.0, 1.0, 0.20];

// --- RMS meter ---
pub const RMS_LOW: [f32; 4]  = [0.45, 0.92, 0.55, 1.0]; // green
pub const RMS_MID: [f32; 4]  = [1.0, 0.85, 0.32, 1.0];  // yellow
pub const RMS_HIGH: [f32; 4] = [1.0, 0.35, 0.30, 1.0];  // red

// --- Browser-specific UI ---
pub const CHEVRON: [f32; 4]          = [1.0, 1.0, 1.0, 0.40];
pub const ADD_BTN_NORMAL: [f32; 4]   = [1.0, 1.0, 1.0, 0.50];
pub const ADD_BTN_HOVER: [f32; 4]    = [1.0, 1.0, 1.0, 0.80];
pub const BG_PLUGIN: [f32; 4]        = [0.10, 0.12, 0.16, 1.0];
pub const BG_PLUGIN_HEADER: [f32; 4] = [0.11, 0.14, 0.20, 1.0];

// --- Region colors (export / loop) ---
pub const EXPORT_FILL_COLOR: [f32; 4]       = [0.15, 0.70, 0.55, 0.10];
pub const EXPORT_BORDER_COLOR: [f32; 4]     = [0.20, 0.80, 0.60, 0.50];
pub const EXPORT_RENDER_PILL_COLOR: [f32; 4] = [0.15, 0.65, 0.50, 0.85];
pub const LOOP_FILL_COLOR: [f32; 4]         = [0.25, 0.55, 0.95, 0.08];
pub const LOOP_BORDER_COLOR: [f32; 4]       = [0.30, 0.60, 1.0, 0.50];
pub const LOOP_BADGE_COLOR: [f32; 4]        = [0.20, 0.50, 0.95, 0.85];

// --- Component entity ---
pub const COMPONENT_BORDER_COLOR: [f32; 4]  = [0.85, 0.55, 0.20, 0.50];
pub const COMPONENT_FILL_COLOR: [f32; 4]    = [0.85, 0.55, 0.20, 0.06];
pub const COMPONENT_BADGE_COLOR: [f32; 4]   = [0.85, 0.55, 0.20, 0.70];
pub const INSTANCE_FILL_COLOR: [f32; 4]     = [0.85, 0.55, 0.20, 0.04];
pub const INSTANCE_BORDER_COLOR: [f32; 4]   = [0.85, 0.55, 0.20, 0.30];
pub const LOCK_ICON_COLOR: [f32; 4]         = [0.85, 0.55, 0.20, 0.60];

// --- Effect entity ---
pub const EFFECT_BORDER_COLOR: [f32; 4]    = [0.25, 0.50, 0.90, 0.50];
pub const EFFECT_ACTIVE_BORDER: [f32; 4]   = [0.35, 0.60, 1.00, 0.70];
pub const PLUGIN_BLOCK_DEFAULT_COLOR: [f32; 4] = [0.25, 0.50, 0.90, 0.70];

// --- Instrument entity ---
pub const INSTRUMENT_BORDER_COLOR: [f32; 4] = [0.60, 0.30, 0.90, 0.50];
pub const INSTRUMENT_ACTIVE_BORDER: [f32; 4] = [0.70, 0.40, 1.00, 0.70];

// --- MIDI ---
pub const MIDI_CLIP_DEFAULT_COLOR: [f32; 4] = [0.60, 0.30, 0.90, 0.70];

// --- Waveform palette (16 slot color wheel) ---
pub const WAVEFORM_COLORS: &[[f32; 4]] = &[
    [1.00, 0.24, 0.19, 1.0], // red
    [1.00, 0.42, 0.24, 1.0], // orange-red
    [1.00, 0.58, 0.00, 1.0], // orange
    [1.00, 0.72, 0.00, 1.0], // amber
    [1.00, 0.84, 0.00, 1.0], // yellow
    [0.78, 0.90, 0.19, 1.0], // lime
    [0.30, 0.85, 0.39, 1.0], // green
    [0.19, 0.84, 0.55, 1.0], // mint
    [0.19, 0.78, 0.71, 1.0], // teal
    [0.19, 0.78, 0.90, 1.0], // cyan
    [0.35, 0.78, 0.98, 1.0], // sky blue
    [0.00, 0.48, 1.00, 1.0], // blue
    [0.35, 0.34, 0.84, 1.0], // indigo
    [0.69, 0.32, 0.87, 1.0], // violet
    [0.88, 0.25, 0.63, 1.0], // magenta
    [1.00, 0.18, 0.33, 1.0], // rose
];

// --- Helper ---
/// Return a copy of `c` with alpha replaced by `a`.
#[inline]
pub fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}
