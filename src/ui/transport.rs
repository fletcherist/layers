use crate::InstanceRaw;
use crate::point_in_rect;

// ---------------------------------------------------------------------------
// Transport Panel (bottom-center playback status)
// ---------------------------------------------------------------------------

pub(crate) const TRANSPORT_WIDTH: f32 = 210.0;
const TRANSPORT_HEIGHT: f32 = 36.0;
const TRANSPORT_BOTTOM_MARGIN: f32 = 32.0;
const FX_BUTTON_WIDTH: f32 = 42.0;
const FX_BUTTON_GAP: f32 = 10.0;
const EXPORT_BUTTON_WIDTH: f32 = 50.0;
const EXPORT_BUTTON_GAP: f32 = 10.0;

pub(crate) struct TransportPanel;

impl TransportPanel {
    pub(crate) fn panel_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = TRANSPORT_WIDTH * scale;
        let h = TRANSPORT_HEIGHT * scale;
        let x = (screen_w - w) * 0.5;
        let y = screen_h - h - TRANSPORT_BOTTOM_MARGIN * scale;
        ([x, y], [w, h])
    }

    pub(crate) fn fx_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (tp_pos, tp_size) = Self::panel_rect(screen_w, screen_h, scale);
        let w = FX_BUTTON_WIDTH * scale;
        let h = tp_size[1];
        let x = tp_pos[0] - w - FX_BUTTON_GAP * scale;
        let y = tp_pos[1];
        ([x, y], [w, h])
    }

    pub(crate) fn hit_fx_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::fx_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn record_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);
        let btn_size = 24.0 * scale;
        let btn_x = pos[0] + size[0] - btn_size - 8.0 * scale;
        let btn_y = pos[1] + (size[1] - btn_size) * 0.5;
        ([btn_x, btn_y], [btn_size, btn_size])
    }

    pub(crate) fn build_instances(
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        is_playing: bool,
        is_recording: bool,
    ) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = Self::panel_rect(screen_w, screen_h, scale);

        // background pill
        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.12, 0.12, 0.16, 0.85],
            border_radius: size[1] * 0.5,
        });

        let icon_x = pos[0] + 14.0 * scale;
        let icon_cy = pos[1] + size[1] * 0.5;

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
                color: [0.95, 0.2, 0.2, 1.0],
                border_radius: 2.0 * scale,
            });
        } else {
            out.push(InstanceRaw {
                position: [dot_x, dot_y],
                size: [dot_diameter, dot_diameter],
                color: [0.85, 0.25, 0.25, 0.9],
                border_radius: dot_diameter * 0.5,
            });
        }

        out
    }

    pub(crate) fn build_fx_button_instances(screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = Self::fx_button_rect(screen_w, screen_h, scale);

        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.14, 0.12, 0.20, 0.85],
            border_radius: size[1] * 0.5,
        });

        // "FX" text approximation: F shape + X shape using small bars
        let cx = pos[0] + size[0] * 0.30;
        let cy = pos[1] + size[1] * 0.5;
        let bar = 2.0 * scale;

        // F: vertical bar
        let f_h = 10.0 * scale;
        out.push(InstanceRaw {
            position: [cx - 4.0 * scale, cy - f_h * 0.5],
            size: [bar, f_h],
            color: [0.70, 0.45, 1.00, 0.90],
            border_radius: 0.0,
        });
        // F: top horizontal
        out.push(InstanceRaw {
            position: [cx - 4.0 * scale, cy - f_h * 0.5],
            size: [6.0 * scale, bar],
            color: [0.70, 0.45, 1.00, 0.90],
            border_radius: 0.0,
        });
        // F: middle horizontal
        out.push(InstanceRaw {
            position: [cx - 4.0 * scale, cy - bar * 0.5],
            size: [5.0 * scale, bar],
            color: [0.70, 0.45, 1.00, 0.90],
            border_radius: 0.0,
        });

        // "+" icon
        let plus_cx = pos[0] + size[0] * 0.72;
        let plus_h = 8.0 * scale;
        let plus_w = 8.0 * scale;
        out.push(InstanceRaw {
            position: [plus_cx - plus_w * 0.5, cy - bar * 0.5],
            size: [plus_w, bar],
            color: [0.70, 0.45, 1.00, 0.70],
            border_radius: 0.0,
        });
        out.push(InstanceRaw {
            position: [plus_cx - bar * 0.5, cy - plus_h * 0.5],
            size: [bar, plus_h],
            color: [0.70, 0.45, 1.00, 0.70],
            border_radius: 0.0,
        });

        out
    }

    pub(crate) fn contains(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::panel_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn hit_record_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::record_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn export_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (tp_pos, tp_size) = Self::panel_rect(screen_w, screen_h, scale);
        let w = EXPORT_BUTTON_WIDTH * scale;
        let h = tp_size[1];
        let x = tp_pos[0] + tp_size[0] + EXPORT_BUTTON_GAP * scale;
        let y = tp_pos[1];
        ([x, y], [w, h])
    }

    pub(crate) fn hit_export_button(pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::export_button_rect(screen_w, screen_h, scale);
        point_in_rect(pos, rp, rs)
    }

    pub(crate) fn build_export_button_instances(screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = Self::export_button_rect(screen_w, screen_h, scale);

        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.10, 0.18, 0.16, 0.85],
            border_radius: size[1] * 0.5,
        });

        let cy = pos[1] + size[1] * 0.5;
        let bar = 2.0 * scale;

        // Arrow-out icon: vertical bar + arrowhead pointing right
        let icon_cx = pos[0] + size[0] * 0.38;
        let arrow_h = 10.0 * scale;
        // Vertical bar
        out.push(InstanceRaw {
            position: [icon_cx - bar * 0.5, cy - arrow_h * 0.5],
            size: [bar, arrow_h],
            color: [0.20, 0.75, 0.60, 0.90],
            border_radius: 0.0,
        });
        // Horizontal bar (arrow shaft)
        let shaft_w = 7.0 * scale;
        out.push(InstanceRaw {
            position: [icon_cx, cy - bar * 0.5],
            size: [shaft_w, bar],
            color: [0.20, 0.75, 0.60, 0.90],
            border_radius: 0.0,
        });
        // Arrowhead: small chevron using two bars
        let tip_x = icon_cx + shaft_w;
        let chev = 4.0 * scale;
        out.push(InstanceRaw {
            position: [tip_x - chev * 0.3, cy - chev * 0.5],
            size: [chev, bar],
            color: [0.20, 0.75, 0.60, 0.90],
            border_radius: 0.0,
        });
        out.push(InstanceRaw {
            position: [tip_x - chev * 0.3, cy + chev * 0.5 - bar],
            size: [chev, bar],
            color: [0.20, 0.75, 0.60, 0.90],
            border_radius: 0.0,
        });

        out
    }
}
