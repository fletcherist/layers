use crate::InstanceRaw;
use crate::gpu::{IconEntry, TextEntry};
use crate::ui::hit_testing::point_in_rect;
use crate::theme::{RECORD_ACTIVE, RECORD_DIM};
use crate::icons;

// ---------------------------------------------------------------------------
// Transport Panel (bottom-center playback status)
// ---------------------------------------------------------------------------

pub(crate) const TRANSPORT_WIDTH: f32 = 310.0;
const TRANSPORT_HEIGHT: f32 = 36.0;
const TRANSPORT_BOTTOM_MARGIN: f32 = 32.0;

pub(crate) struct TransportPanel;

impl TransportPanel {
    pub(crate) fn panel_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = TRANSPORT_WIDTH * scale;
        let h = TRANSPORT_HEIGHT * scale;
        let x = (screen_w - w) * 0.5;
        let y = screen_h - h - TRANSPORT_BOTTOM_MARGIN * scale;
        ([x, y], [w, h])
    }

    pub(crate) fn record_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 24.0 * scale;
        let btn_x = pos[0] + size[0] - btn_size - 38.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn monitor_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 20.0 * scale;
        let btn_x = pos[0] + size[0] - btn_size - 8.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn hit_monitor_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::monitor_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn build_instances(
        settings: &crate::settings::Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        _is_playing: bool,
        is_recording: bool,
        _metronome_enabled: bool,
        _computer_keyboard_armed: bool,
        _input_monitoring: bool,
    ) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);

        // background pill
        out.push(InstanceRaw {
            position: pos,
            size,
            color: settings.theme.bg_panel,
            border_radius: size[1] * 0.5,
        });

        // record button: red circle / stop square (keep as geometry for semantic color)
        let (rbtn_pos, rbtn_size) = Self::record_button_rect(screen_w, screen_h, scale);
        let dot_diameter = 12.0 * scale;
        let dot_x = rbtn_pos[0] + (rbtn_size[0] - dot_diameter) * 0.5;
        let dot_y = rbtn_pos[1] + (rbtn_size[1] - dot_diameter) * 0.5;

        if is_recording {
            let sq = 10.0 * scale;
            let sq_x = rbtn_pos[0] + (rbtn_size[0] - sq) * 0.5;
            let sq_y = rbtn_pos[1] + (rbtn_size[1] - sq) * 0.5;
            out.push(InstanceRaw {
                position: [sq_x, sq_y],
                size: [sq, sq],
                color: RECORD_ACTIVE,
                border_radius: 2.0 * scale,
            });
        } else {
            out.push(InstanceRaw {
                position: [dot_x, dot_y],
                size: [dot_diameter, dot_diameter],
                color: RECORD_DIM,
                border_radius: dot_diameter * 0.5,
            });
        }

        out
    }

    pub(crate) fn get_icon_entries(
        settings: &crate::settings::Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        is_playing: bool,
        _is_recording: bool,
        metronome_enabled: bool,
        computer_keyboard_armed: bool,
        input_monitoring: bool,
    ) -> Vec<IconEntry> {
        let mut out = Vec::new();

        fn center_icon(btn_pos: [f32; 2], btn_size: [f32; 2], icon_size: f32) -> (f32, f32) {
            let x = btn_pos[0] + (btn_size[0] - icon_size) * 0.5;
            let y = btn_pos[1] + (btn_size[1] - icon_size) * 0.5;
            (x, y)
        }

        let small_icon = 16.0 * scale;
        let play_icon = 20.0 * scale;

        // Metronome (timer icon)
        let (met_pos, met_size) = Self::metronome_button_rect(screen_w, screen_h, scale);
        let (mx, my) = center_icon(met_pos, met_size, small_icon);
        let acc = settings.theme.accent;
        let met_color = if metronome_enabled {
            [(acc[0] * 255.0) as u8, (acc[1] * 255.0) as u8, (acc[2] * 255.0) as u8, (acc[3] * 255.0) as u8]
        } else {
            [255, 255, 255, 64]
        };
        out.push(IconEntry { codepoint: icons::TIMER, x: mx, y: my, size: small_icon, color: met_color });

        // Computer keyboard armed (keyboard icon)
        let (kb_pos, kb_size) = Self::computer_keyboard_button_rect(screen_w, screen_h, scale);
        let (kx, ky) = center_icon(kb_pos, kb_size, small_icon);
        let kb_alpha = if computer_keyboard_armed { 242 } else { 89 };
        out.push(IconEntry { codepoint: icons::KEYBOARD, x: kx, y: ky, size: small_icon, color: [255, 255, 255, kb_alpha] });

        // Play / Pause
        let (pp_pos, pp_size) = Self::play_pause_rect(screen_w, screen_h, scale);
        let (px, py) = center_icon(pp_pos, pp_size, play_icon);
        let play_codepoint = if is_playing { icons::PAUSE } else { icons::PLAY_ARROW };
        out.push(IconEntry { codepoint: play_codepoint, x: px, y: py, size: play_icon, color: [255, 255, 255, 230] });

        // Input monitor (headphones icon)
        let (mon_pos, mon_size) = Self::monitor_button_rect(screen_w, screen_h, scale);
        let (hx, hy) = center_icon(mon_pos, mon_size, small_icon);
        let mon_color = if input_monitoring { [77, 217, 128, 242] } else { [255, 255, 255, 77] };
        out.push(IconEntry { codepoint: icons::HEADPHONES, x: hx, y: hy, size: small_icon, color: mon_color });

        out
    }

    pub(crate) fn metronome_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 20.0 * scale;
        let btn_x = pos[0] + 4.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn hit_metronome_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::metronome_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn computer_keyboard_button_rect(
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 20.0 * scale;
        let btn_x = pos[0] + 26.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn hit_computer_keyboard_button(
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        let (rp, rs) = Self::computer_keyboard_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn play_pause_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 24.0 * scale;
        let btn_x = pos[0] + 50.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn hit_play_pause(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::play_pause_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn contains(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::panel_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn hit_record_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::record_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn bpm_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (tp_pos, tp_size) = Self::panel_rect(screen_w, screen_h, scale);
        let (rbtn_pos, _) = Self::record_button_rect(screen_w, screen_h, scale);
        let max_w = 60.0 * scale;
        let gap = 8.0 * scale;
        let x = rbtn_pos[0] - gap - max_w;
        ([x, tp_pos[1]], [max_w, tp_size[1]])
    }

    pub(crate) fn hit_bpm(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::bpm_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn get_text_entries(
        theme: &crate::theme::RuntimeTheme,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        playback_position: f64,
        bpm: f32,
        editing_bpm: Option<&str>,
    ) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (tp_pos, tp_size) = Self::panel_rect(screen_w, screen_h, scale);
        let tfont = 13.0 * scale;
        let tline = 18.0 * scale;

        // Time display
        let time_str = crate::format_playback_time(playback_position);
        out.push(TextEntry {
            text: time_str,
            x: tp_pos[0] + 74.0 * scale,
            y: tp_pos[1] + (tp_size[1] - tline) * 0.5,
            font_size: tfont,
            line_height: tline,
            max_width: TRANSPORT_WIDTH * scale * 0.6,
            color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, 220),
            weight: 400,
            bounds: None,
                center: false,
        });

        // BPM display - anchored to left of record button with a gap
        let bpm_str = if let Some(text) = editing_bpm {
            format!("{}|", text)
        } else {
            format!("{} bpm", bpm as u32)
        };
        let alpha = if editing_bpm.is_some() { 255 } else { 220 };
        let (rbtn_pos, _) = Self::record_button_rect(screen_w, screen_h, scale);
        let bpm_max_width = 60.0 * scale;
        let bpm_x = rbtn_pos[0] - 8.0 * scale - bpm_max_width;
        out.push(TextEntry {
            text: bpm_str,
            x: bpm_x,
            y: tp_pos[1] + (tp_size[1] - tline) * 0.5,
            font_size: tfont,
            line_height: tline,
            max_width: bpm_max_width,
            color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, alpha),
            weight: 400,
            bounds: None,
                center: false,
        });

        out
    }
}
