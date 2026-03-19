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

// --- Waveform palette (Ableton Live 10 track colors, 14 cols × 5 rows) ---
pub const WAVEFORM_COLOR_COLS: usize = 14;
pub const WAVEFORM_COLOR_ROWS: usize = 5;
pub const WAVEFORM_COLORS: &[[f32; 4]] = &[
    // Row 1 – vivid
    [0.992, 0.584, 0.655, 1.0], // #FD95A7
    [0.992, 0.643, 0.227, 1.0], // #FDA43A
    [0.796, 0.596, 0.204, 1.0], // #CB9834
    [0.969, 0.953, 0.518, 1.0], // #F7F384
    [0.753, 0.976, 0.196, 1.0], // #C0F932
    [0.196, 0.992, 0.259, 1.0], // #32FD42
    [0.224, 0.992, 0.667, 1.0], // #39FDAA
    [0.396, 0.996, 0.910, 1.0], // #65FEE8
    [0.553, 0.776, 0.992, 1.0], // #8DC6FD
    [0.337, 0.510, 0.882, 1.0], // #5682E1
    [0.576, 0.663, 0.988, 1.0], // #93A9FC
    [0.839, 0.439, 0.886, 1.0], // #D670E2
    [0.890, 0.337, 0.624, 1.0], // #E3569F
    [1.000, 1.000, 1.000, 1.0], // #FFFFFF
    // Row 2 – saturated
    [0.988, 0.224, 0.239, 1.0], // #FC393D
    [0.957, 0.424, 0.125, 1.0], // #F46C20
    [0.596, 0.443, 0.306, 1.0], // #98714E
    [0.996, 0.933, 0.290, 1.0], // #FEEE4A
    [0.545, 0.992, 0.439, 1.0], // #8BFD70
    [0.267, 0.757, 0.129, 1.0], // #44C121
    [0.118, 0.745, 0.686, 1.0], // #1EBEAF
    [0.192, 0.914, 0.992, 1.0], // #31E9FD
    [0.133, 0.647, 0.922, 1.0], // #22A5EB
    [0.071, 0.494, 0.745, 1.0], // #127EBE
    [0.533, 0.439, 0.882, 1.0], // #8870E1
    [0.710, 0.475, 0.769, 1.0], // #B579C4
    [0.992, 0.259, 0.824, 1.0], // #FD42D2
    [0.816, 0.816, 0.816, 1.0], // #D0D0D0
    // Row 3 – pastel
    [0.878, 0.408, 0.365, 1.0], // #E0685D
    [0.992, 0.639, 0.471, 1.0], // #FDA378
    [0.824, 0.675, 0.459, 1.0], // #D2AC75
    [0.929, 0.996, 0.698, 1.0], // #EDFEB2
    [0.824, 0.890, 0.612, 1.0], // #D2E39C
    [0.729, 0.812, 0.475, 1.0], // #BACF79
    [0.612, 0.765, 0.561, 1.0], // #9CC38F
    [0.835, 0.992, 0.886, 1.0], // #D5FDE2
    [0.808, 0.945, 0.973, 1.0], // #CEF1F8
    [0.725, 0.761, 0.886, 1.0], // #B9C2E2
    [0.804, 0.737, 0.890, 1.0], // #CDBCE3
    [0.682, 0.604, 0.890, 1.0], // #AE9AE3
    [0.898, 0.863, 0.882, 1.0], // #E5DCE1
    [0.663, 0.663, 0.663, 1.0], // #A9A9A9
    // Row 4 – muted
    [0.773, 0.573, 0.549, 1.0], // #C5928C
    [0.714, 0.510, 0.349, 1.0], // #B68259
    [0.596, 0.514, 0.420, 1.0], // #98836B
    [0.749, 0.725, 0.431, 1.0], // #BFB96E
    [0.651, 0.737, 0.145, 1.0], // #A6BC25
    [0.494, 0.686, 0.322, 1.0], // #7EAF52
    [0.541, 0.761, 0.729, 1.0], // #8AC2BA
    [0.612, 0.702, 0.765, 1.0], // #9CB3C3
    [0.525, 0.647, 0.757, 1.0], // #86A5C1
    [0.518, 0.580, 0.792, 1.0], // #8494CA
    [0.647, 0.588, 0.706, 1.0], // #A596B4
    [0.745, 0.627, 0.741, 1.0], // #BEA0BD
    [0.733, 0.447, 0.588, 1.0], // #BB7296
    [0.482, 0.482, 0.482, 1.0], // #7B7B7B
    // Row 5 – dark
    [0.678, 0.204, 0.212, 1.0], // #AD3436
    [0.655, 0.318, 0.208, 1.0], // #A75135
    [0.443, 0.310, 0.259, 1.0], // #714F42
    [0.855, 0.761, 0.161, 1.0], // #DAC229
    [0.522, 0.584, 0.169, 1.0], // #85952B
    [0.333, 0.620, 0.220, 1.0], // #559E38
    [0.106, 0.608, 0.557, 1.0], // #1B9B8E
    [0.149, 0.388, 0.514, 1.0], // #266383
    [0.106, 0.200, 0.576, 1.0], // #1B3393
    [0.192, 0.329, 0.627, 1.0], // #3154A0
    [0.384, 0.306, 0.671, 1.0], // #624EAB
    [0.635, 0.306, 0.671, 1.0], // #A24EAB
    [0.792, 0.196, 0.431, 1.0], // #CA326E
    [0.235, 0.235, 0.235, 1.0], // #3C3C3C
];

// --- Helper ---
/// Return a copy of `c` with alpha replaced by `a`.
#[inline]
pub fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

// ---------------------------------------------------------------------------
// Runtime theme (hue-parameterized)
// ---------------------------------------------------------------------------

const PRIMARY_HUE: f32 = 216.0;
const LIGHTNESS_OFFSET: f32 = 0.0;
const OFFSET_COMPONENT: f32 = 30.0;

/// Convert HSL + alpha to a linear [f32; 4] colour.
/// h in [0, 360), s and l in [0, 1], a in [0, 1].
pub fn hsl(h: f32, s: f32, l: f32, a: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c * 0.5;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [r1 + m, g1 + m, b1 + m, a]
}

pub fn wrap_hue(h: f32) -> f32 {
    ((h % 360.0) + 360.0) % 360.0
}

#[derive(Clone)]
pub struct RuntimeTheme {
    pub bg_base: [f32; 4],
    pub bg_surface: [f32; 4],
    pub bg_menu: [f32; 4],
    pub bg_overlay: [f32; 4],
    pub bg_elevated: [f32; 4],
    pub bg_input: [f32; 4],
    pub bg_dropdown: [f32; 4],
    pub bg_panel: [f32; 4],
    pub bg_window: [f32; 4],
    pub bg_sidebar: [f32; 4],
    pub bg_window_header: [f32; 4],
    pub bg_plugin: [f32; 4],
    pub bg_plugin_header: [f32; 4],
    pub accent: [f32; 4],
    pub accent_muted: [f32; 4],
    pub accent_faint: [f32; 4],
    pub selection: [f32; 4],
    pub border_subtle: [f32; 4],
    pub item_hover: [f32; 4],
    pub item_active: [f32; 4],
    pub option_highlight: [f32; 4],
    pub pill_active: [f32; 4],
    pub pill_inactive: [f32; 4],
    pub slider_fill: [f32; 4],
    pub knob_inactive: [f32; 4],
    pub drop_zone_fill: [f32; 4],
    pub drop_zone_border: [f32; 4],
    pub select_rect_fill: [f32; 4],
    pub select_rect_border: [f32; 4],
    pub select_outline: [f32; 4],
    pub loop_fill_color: [f32; 4],
    pub loop_border_color: [f32; 4],
    pub loop_badge_color: [f32; 4],
    pub export_fill_color: [f32; 4],
    pub export_border_color: [f32; 4],
    pub export_render_pill_color: [f32; 4],
    pub component_border_color: [f32; 4],
    pub component_fill_color: [f32; 4],
    pub component_badge_color: [f32; 4],
    pub instance_fill_color: [f32; 4],
    pub instance_border_color: [f32; 4],
    pub lock_icon_color: [f32; 4],
    pub effect_border_color: [f32; 4],
    pub effect_active_border: [f32; 4],
    pub plugin_block_default_color: [f32; 4],
    pub instrument_border_color: [f32; 4],
    pub instrument_active_border: [f32; 4],
    pub midi_clip_default_color: [f32; 4],
    pub playhead: [f32; 4],
    pub category_dot: [f32; 4],
    pub pill_instrument: [f32; 4],
    pub pill_effect: [f32; 4],
}

impl RuntimeTheme {
    pub fn from_hue_with_settings(h: f32, color_intensity: f32, brightness: f32) -> Self {
        let s_mult = 0.05 + color_intensity * 0.95;
        let lo = (brightness - 1.0) * 0.15;
        // Helper: apply saturation multiplier and lightness offset, preserving alpha
        let c = |hue: f32, s: f32, l: f32, a: f32| -> [f32; 4] {
            hsl(hue, s * s_mult, (l + lo).clamp(0.05, 0.95), a)
        };
        let accent = c(h, 1.0, 0.625, 1.0);
        let ch = wrap_hue(h + OFFSET_COMPONENT);
        Self {
            bg_base:          c(h, 0.12, 0.125, 1.0),
            bg_surface:       c(h, 0.12, 0.145, 1.0),
            bg_menu:          c(h, 0.10, 0.165, 1.0),
            bg_overlay:       c(h, 0.10, 0.155, 0.98),
            bg_elevated:      c(h, 0.14, 0.175, 1.0),
            bg_input:         c(h, 0.14, 0.165, 1.0),
            bg_dropdown:      c(h, 0.12, 0.170, 1.0),
            bg_panel:         c(h, 0.10, 0.145, 0.85),
            bg_window:        c(h, 0.13, 0.155, 0.98),
            bg_sidebar:       c(h, 0.12, 0.135, 1.0),
            bg_window_header: c(h, 0.16, 0.185, 1.0),
            bg_plugin:        c(h, 0.20, 0.120, 1.0),
            bg_plugin_header: c(h, 0.22, 0.140, 1.0),
            accent,
            accent_muted:     c(h, 0.70, 0.560, 0.60),
            accent_faint:     c(h, 1.00, 0.625, 0.08),
            selection:        c(h, 0.65, 0.600, 0.80),
            border_subtle:    c(h, 0.15, 0.300, 0.12),
            item_hover:       [1.0, 1.0, 1.0, 0.06],
            item_active:      c(h, 0.50, 0.300, 0.25),
            option_highlight: c(h, 0.50, 0.250, 0.30),
            pill_active:      c(h, 0.65, 0.500, 0.85),
            pill_inactive:    c(h, 0.15, 0.350, 0.40),
            slider_fill:      c(h, 0.65, 0.550, 0.80),
            knob_inactive:    c(h, 0.20, 0.350, 0.60),
            drop_zone_fill:   c(h, 0.65, 0.580, 0.10),
            drop_zone_border: c(h, 0.65, 0.620, 0.60),
            select_rect_fill: c(h, 0.65, 0.580, 0.08),
            select_rect_border: c(h, 0.60, 0.600, 0.50),
            select_outline:   c(h, 0.65, 0.600, 0.70),
            loop_fill_color:  c(h, 0.80, 0.580, 0.08),
            loop_border_color: c(h, 0.75, 0.620, 0.50),
            loop_badge_color: c(h, 0.75, 0.570, 0.85),
            export_fill_color:       c(wrap_hue(h + 150.0), 0.70, 0.550, 0.10),
            export_border_color:     c(wrap_hue(h + 150.0), 0.75, 0.600, 0.50),
            export_render_pill_color: c(wrap_hue(h + 150.0), 0.65, 0.500, 0.85),
            component_border_color:  c(ch, 0.75, 0.525, 0.50),
            component_fill_color:    c(ch, 0.75, 0.525, 0.06),
            component_badge_color:   c(ch, 0.75, 0.525, 0.70),
            instance_fill_color:     c(ch, 0.75, 0.525, 0.04),
            instance_border_color:   c(ch, 0.75, 0.525, 0.30),
            lock_icon_color:         c(ch, 0.75, 0.525, 0.60),
            effect_border_color:     c(h, 0.65, 0.560, 0.50),
            effect_active_border:    c(h, 0.70, 0.600, 0.70),
            plugin_block_default_color: c(h, 0.65, 0.560, 0.70),
            instrument_border_color: c(wrap_hue(h + 60.0), 0.65, 0.580, 0.50),
            instrument_active_border: c(wrap_hue(h + 60.0), 0.70, 0.620, 0.70),
            midi_clip_default_color: c(wrap_hue(h + 60.0), 0.65, 0.580, 0.70),
            playhead:          c(wrap_hue(h + 160.0), 0.70, 0.580, 0.90),
            category_dot:      c(h, 0.65, 0.580, 0.70),
            pill_instrument:   c(wrap_hue(h + 60.0), 0.65, 0.520, 0.85),
            pill_effect:       c(h, 0.65, 0.520, 0.85),
        }
    }

    pub fn from_hue(h: f32) -> Self {
        Self::from_hue_with_settings(h, 1.0, 1.0)
    }

    pub fn from_preset_ableton() -> Self {
        let bg       = [0.314, 0.298, 0.275, 1.0];
        let surface  = [0.345, 0.329, 0.306, 1.0];
        let menu     = [0.361, 0.345, 0.322, 1.0];
        let elevated = [0.396, 0.380, 0.357, 1.0];
        let input    = [0.290, 0.278, 0.255, 1.0];
        let sidebar  = [0.267, 0.255, 0.235, 1.0];
        let header   = [0.420, 0.404, 0.380, 1.0];
        let ac       = [0.902, 0.667, 0.176, 1.0];
        Self {
            bg_base: bg,
            bg_surface: surface,
            bg_menu: menu,
            bg_overlay: [menu[0], menu[1], menu[2], 0.98],
            bg_elevated: elevated,
            bg_input: input,
            bg_dropdown: menu,
            bg_panel: [surface[0], surface[1], surface[2], 0.85],
            bg_window: [surface[0], surface[1], surface[2], 0.98],
            bg_sidebar: sidebar,
            bg_window_header: header,
            bg_plugin: [input[0] - 0.02, input[1] - 0.02, input[2] - 0.02, 1.0],
            bg_plugin_header: input,
            accent: ac,
            accent_muted:  [ac[0], ac[1], ac[2], 0.60],
            accent_faint:  [ac[0], ac[1], ac[2], 0.08],
            selection:     [ac[0], ac[1], ac[2], 0.80],
            border_subtle: [1.0, 1.0, 1.0, 0.08],
            item_hover:    [1.0, 1.0, 1.0, 0.06],
            item_active:   [ac[0], ac[1], ac[2], 0.20],
            option_highlight: [ac[0], ac[1], ac[2], 0.30],
            pill_active:   [ac[0], ac[1], ac[2], 0.85],
            pill_inactive: [1.0, 1.0, 1.0, 0.25],
            slider_fill:   [ac[0], ac[1], ac[2], 0.80],
            knob_inactive: [1.0, 1.0, 1.0, 0.30],
            drop_zone_fill:   [ac[0], ac[1], ac[2], 0.10],
            drop_zone_border: [ac[0], ac[1], ac[2], 0.60],
            select_rect_fill:   [ac[0], ac[1], ac[2], 0.08],
            select_rect_border: [ac[0], ac[1], ac[2], 0.50],
            select_outline:     [ac[0], ac[1], ac[2], 0.70],
            loop_fill_color:   [ac[0], ac[1], ac[2], 0.08],
            loop_border_color: [ac[0], ac[1], ac[2], 0.50],
            loop_badge_color:  [ac[0], ac[1], ac[2], 0.85],
            export_fill_color:        [0.20, 0.75, 0.55, 0.10],
            export_border_color:      [0.20, 0.75, 0.55, 0.50],
            export_render_pill_color: [0.20, 0.75, 0.55, 0.85],
            component_border_color: [0.85, 0.55, 0.20, 0.50],
            component_fill_color:   [0.85, 0.55, 0.20, 0.06],
            component_badge_color:  [0.85, 0.55, 0.20, 0.70],
            instance_fill_color:    [0.85, 0.55, 0.20, 0.04],
            instance_border_color:  [0.85, 0.55, 0.20, 0.30],
            lock_icon_color:        [0.85, 0.55, 0.20, 0.60],
            effect_border_color:     [ac[0], ac[1], ac[2], 0.50],
            effect_active_border:    [ac[0], ac[1], ac[2], 0.70],
            plugin_block_default_color: [ac[0], ac[1], ac[2], 0.70],
            instrument_border_color:  [0.60, 0.30, 0.90, 0.50],
            instrument_active_border: [0.70, 0.40, 1.00, 0.70],
            midi_clip_default_color:  [0.60, 0.30, 0.90, 0.70],
            playhead:        [0.30, 0.85, 0.50, 0.90],
            category_dot:    [ac[0], ac[1], ac[2], 0.70],
            pill_instrument: [0.60, 0.30, 0.90, 0.85],
            pill_effect:     [ac[0], ac[1], ac[2], 0.85],
        }
    }
}

impl Default for RuntimeTheme {
    fn default() -> Self { Self::from_hue(PRIMARY_HUE) }
}
