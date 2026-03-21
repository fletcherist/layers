use crate::InstanceRaw;
use crate::gpu::TextEntry;
use crate::ui::hit_testing::point_in_rect;
use crate::theme::{RECORD_ACTIVE, RECORD_DIM};

// ---------------------------------------------------------------------------
// Transport Panel (bottom-center playback status)
// ---------------------------------------------------------------------------

pub(crate) const TRANSPORT_WIDTH: f32 = 280.0;
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
        let btn_x = pos[0] + size[0] - btn_size - 8.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn build_instances(
        settings: &crate::settings::Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        is_playing: bool,
        is_recording: bool,
        metronome_enabled: bool,
        computer_keyboard_armed: bool,
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

        // metronome dot
        let met_d = 8.0 * scale;
        let icon_cy = pos[1] + size[1] * 0.5;
        let met_x = pos[0] + 10.0 * scale;
        let met_y = icon_cy - met_d * 0.5;
        let met_color = if metronome_enabled {
            settings.theme.accent
        } else {
            [1.0, 1.0, 1.0, 0.25]
        };
        out.push(InstanceRaw {
            position: [met_x, met_y],
            size: [met_d, met_d],
            color: met_color,
            border_radius: met_d * 0.5,
        });

        // Computer keyboard (mini piano keys)
        let kb_btn = 20.0 * scale;
        let kb_x = pos[0] + 26.0 * scale;
        let kb_y = icon_cy - kb_btn * 0.5;
        let kb_on = computer_keyboard_armed;
        let kb_alpha = if kb_on { 0.95 } else { 0.35 };
        let key_w = 4.5 * scale;
        let key_h = kb_btn * 0.72;
        let key_gap = 1.0 * scale;
        for k in 0..3usize {
            let kx = kb_x + 3.0 * scale + (key_w + key_gap) * k as f32;
            out.push(InstanceRaw {
                position: [kx, kb_y + kb_btn * 0.14],
                size: [key_w, key_h],
                color: [1.0, 1.0, 1.0, kb_alpha],
                border_radius: 0.8 * scale,
            });
        }

        let icon_x = pos[0] + 52.0 * scale;

        if is_playing {
            let bar_w = 3.0 * scale;
            let bar_h = 12.0 * scale;
            let gap = 4.0 * scale;
            out.push(InstanceRaw {
                position: [icon_x, icon_cy - bar_h * 0.5],
                size: [bar_w, bar_h],
                color: [1.0, 1.0, 1.0, 0.9],
                border_radius: 1.0 * scale,
            });
            out.push(InstanceRaw {
                position: [icon_x + bar_w + gap, icon_cy - bar_h * 0.5],
                size: [bar_w, bar_h],
                color: [1.0, 1.0, 1.0, 0.9],
                border_radius: 1.0 * scale,
            });
        } else {
            let tri_w = 10.0 * scale;
            let tri_h = 12.0 * scale;
            let steps = (tri_h * 3.0).ceil() as usize;
            let step_h = tri_h / steps as f32;
            let min_w = 1.5 * scale;
            for i in 0..steps {
                let t = (i as f32 + 0.5) / steps as f32;
                let w = (tri_w * (1.0 - (2.0 * t - 1.0).abs())).max(min_w);
                let sy = icon_cy - tri_h * 0.5 + i as f32 * step_h;
                out.push(InstanceRaw {
                    position: [icon_x, sy],
                    size: [w, step_h + 0.5],
                    color: [1.0, 1.0, 1.0, 0.9],
                    border_radius: min_w * 0.5,
                });
            }
        }

        // record button: red circle (brighter when recording)
        let (rbtn_pos, rbtn_size) = Self::record_button_rect(screen_w, screen_h, scale);
        let dot_diameter = 12.0 * scale;
        let dot_x = rbtn_pos[0] + (rbtn_size[0] - dot_diameter) * 0.5;
        let dot_y = rbtn_pos[1] + (rbtn_size[1] - dot_diameter) * 0.5;

        if is_recording {
            // stop icon: rounded red square
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
        let x = tp_pos[0] + tp_size[0] - 80.0 * scale;
        let w = rbtn_pos[0] - x;
        ([x, tp_pos[1]], [w, tp_size[1]])
    }

    pub(crate) fn hit_bpm(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::bpm_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn get_text_entries(
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
            color: [220, 220, 230, 220],
            weight: 400,
            bounds: None,
                center: false,
        });

        // BPM display
        let bpm_str = if let Some(text) = editing_bpm {
            format!("{}|", text)
        } else {
            format!("{} bpm", bpm as u32)
        };
        let alpha = if editing_bpm.is_some() { 255 } else { 220 };
        out.push(TextEntry {
            text: bpm_str,
            x: tp_pos[0] + tp_size[0] - 80.0 * scale,
            y: tp_pos[1] + (tp_size[1] - tline) * 0.5,
            font_size: tfont,
            line_height: tline,
            max_width: 80.0 * scale,
            color: [220, 220, 230, alpha],
            weight: 400,
            bounds: None,
                center: false,
        });

        out
    }
}
