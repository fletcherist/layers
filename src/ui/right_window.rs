use crate::entity_id::EntityId;
use crate::gpu::TextEntry;
use crate::InstanceRaw;
use crate::ui::palette::{gain_to_db, gain_to_vol_fader_pos, vol_fader_pos_to_gain,
    VOL_FADER_DB_MAX, VOL_FADER_DB_BOTTOM, VOL_FADER_P_ZERO, VOL_FADER_P_BOTTOM};
use crate::ui::value_entry::ValueEntry;
use crate::ui::waveform::WarpMode;

pub const RIGHT_WINDOW_WIDTH: f32 = 200.0;
const HEADER_HEIGHT: f32 = 36.0;
const KNOB_R: f32 = 22.0;
const KNOB_DOT_R: f32 = 2.5;
const KNOB_INDICATOR_R: f32 = 3.5;
const ARC_DOTS: usize = 30;

const FADER_TRACK_W: f32 = 4.0;
const FADER_TRACK_HEIGHT: f32 = 160.0;
const FADER_TOP_OFFSET: f32 = 32.0;

const PAN_KNOB_Y_OFFSET: f32 = 264.0;
const REVERSE_BUTTON_Y_OFFSET: f32 = 326.0;
const PITCH_KNOB_Y_OFFSET: f32 = 368.0;


pub struct VolFaderLayout {
    pub track_pos: [f32; 2],
    pub track_size: [f32; 2],
    pub center_x: f32,
    /// "Gain" label Y (top of label text)
    pub label_y: f32,
    /// Triangle indicator X (left edge of ▶)
    pub triangle_x: f32,
    /// Scale labels X (left edge of "24", "0", "70")
    pub scale_labels_x: f32,
    /// dB value text Y (top of text)
    pub db_text_y: f32,
    /// dB value text rect (for hit testing)
    pub db_text_rect: ([f32; 2], [f32; 2]),
    /// Focus bracket bounds
    pub bracket_x0: f32,
    pub bracket_x1: f32,
    pub bracket_y0: f32,
    pub bracket_y1: f32,
    /// Tick mark X offset (right edge of tick gap, left of track)
    pub tick_x_offset: f32,
}

pub struct PanKnobLayout {
    pub center: [f32; 2],
    pub radius: f32,
    pub label_y: f32,
    pub value_y: f32,
    pub bracket_x0: f32,
    pub bracket_x1: f32,
    pub bracket_y0: f32,
    pub bracket_y1: f32,
}

pub struct PitchTextLayout {
    pub label_pos: [f32; 2],
    pub value_pos: [f32; 2],
    pub text_rect: ([f32; 2], [f32; 2]),
    pub bracket_x0: f32,
    pub bracket_x1: f32,
    pub bracket_y0: f32,
    pub bracket_y1: f32,
}

pub struct SampleBpmTextLayout {
    pub label_pos: [f32; 2],
    pub value_pos: [f32; 2],
    pub text_rect: ([f32; 2], [f32; 2]),
    pub bracket_x0: f32,
    pub bracket_x1: f32,
    pub bracket_y0: f32,
    pub bracket_y1: f32,
}

pub struct RightWindow {
    pub waveform_id: EntityId,
    pub volume: f32,
    pub pan: f32,
    pub warp_mode: WarpMode,
    pub sample_bpm: f32,
    pub pitch_semitones: f32,
    pub is_reversed: bool,
    pub vol_dragging: bool,
    pub pan_dragging: bool,
    pub sample_bpm_dragging: bool,
    pub pitch_dragging: bool,
    pub drag_start_y: f32,
    pub drag_start_value: f32,
    pub vol_entry: ValueEntry,
    pub sample_bpm_entry: ValueEntry,
    pub pitch_entry: ValueEntry,
    pub vol_fader_focused: bool,
    pub pan_knob_focused: bool,
    pub pitch_focused: bool,
    pub sample_bpm_focused: bool,
}

impl RightWindow {
    pub fn panel_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = RIGHT_WINDOW_WIDTH * scale;
        let h = screen_h;
        ([screen_w - w, 0.0], [w, h])
    }

    fn vol_fader_rects(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);
        let panel_cx = pp[0] + ps[0] * 0.5;
        let track_pos = [
            panel_cx - FADER_TRACK_W * 0.5 * scale,
            pp[1] + (HEADER_HEIGHT + FADER_TOP_OFFSET) * scale,
        ];
        let track_size = [FADER_TRACK_W * scale, FADER_TRACK_HEIGHT * scale];
        (track_pos, track_size)
    }

    fn vol_fader_thumb_y(fader_pos: f32, track_pos: [f32; 2], track_h: f32) -> f32 {
        track_pos[1] + (1.0 - fader_pos) * track_h
    }

    fn pan_knob_center(screen_w: f32, screen_h: f32, scale: f32) -> [f32; 2] {
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);
        [pp[0] + ps[0] * 0.5, pp[1] + HEADER_HEIGHT * scale + PAN_KNOB_Y_OFFSET * scale]
    }

    fn warp_mode_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);
        let cx = pp[0] + ps[0] * 0.5;
        let y = pp[1] + HEADER_HEIGHT * scale + PITCH_KNOB_Y_OFFSET * scale;
        let w = 80.0 * scale;
        let h = 24.0 * scale;
        ([cx - w * 0.5, y - h * 0.5], [w, h])
    }

    fn reverse_button_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);
        let cx = pp[0] + ps[0] * 0.5;
        let y = pp[1] + HEADER_HEIGHT * scale + REVERSE_BUTTON_Y_OFFSET * scale;
        let w = 80.0 * scale;
        let h = 24.0 * scale;
        ([cx - w * 0.5, y - h * 0.5], [w, h])
    }

    fn warp_mode_selector_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);
        let cx = pp[0] + ps[0] * 0.5;
        let (btn_pos, btn_size) = Self::warp_mode_button_rect(screen_w, screen_h, scale);
        let y = btn_pos[1] + btn_size[1] + 4.0 * scale;
        let w = 90.0 * scale;
        let h = 22.0 * scale;
        ([cx - w * 0.5, y], [w, h])
    }

    pub fn warp_mode_selector_rect_pub(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        Self::warp_mode_selector_rect(screen_w, screen_h, scale)
    }

    fn warp_param_text_rect(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (pp, _ps) = Self::panel_rect(screen_w, screen_h, scale);
        let (sel_pos, sel_size) = Self::warp_mode_selector_rect(screen_w, screen_h, scale);
        let rw_w = RIGHT_WINDOW_WIDTH * scale;
        let text_y = sel_pos[1] + sel_size[1] + 4.0 * scale;
        ([pp[0], text_y], [rw_w, 40.0 * scale])
    }

    pub fn warp_param_text_rect_pub(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        Self::warp_param_text_rect(screen_w, screen_h, scale)
    }

    pub fn hit_test_vol_knob(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (track_pos, track_size) = Self::vol_fader_rects(screen_w, screen_h, scale);
        let fader_pos_val = gain_to_vol_fader_pos(self.volume);
        let thumb_y = Self::vol_fader_thumb_y(fader_pos_val, track_pos, track_size[1]);
        // Rectangular hit zone: covers triangle + track, 20px tall around thumb
        let hit_x = track_pos[0] - 18.0 * scale;
        let hit_w = track_size[0] + 22.0 * scale;
        let hit_h = 18.0 * scale;
        pos[0] >= hit_x && pos[0] <= hit_x + hit_w
            && pos[1] >= thumb_y - hit_h * 0.5 && pos[1] <= thumb_y + hit_h * 0.5
    }

    pub fn hit_test_vol_track(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (track_pos, track_size) = Self::vol_fader_rects(screen_w, screen_h, scale);
        let margin = 12.0 * scale;
        pos[0] >= track_pos[0] - margin && pos[0] <= track_pos[0] + track_size[0] + margin
            && pos[1] >= track_pos[1] && pos[1] <= track_pos[1] + track_size[1]
    }

    pub fn hit_test_pan_knob(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let layout = Self::pan_knob_layout(screen_w, screen_h, scale);
        let r = layout.radius + 8.0 * scale;
        let dx = pos[0] - layout.center[0];
        let dy = pos[1] - layout.center[1];
        dx * dx + dy * dy <= r * r
    }

    pub fn hit_test_reverse_button(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::reverse_button_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    pub fn hit_test_warp_mode_button(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = Self::warp_mode_button_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    pub fn hit_test_warp_mode_selector(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        if self.warp_mode == WarpMode::Off { return false; }
        let (rp, rs) = Self::warp_mode_selector_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    pub fn hit_test_sample_bpm_text(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        if self.warp_mode != WarpMode::RePitch { return false; }
        let (rp, rs) = Self::warp_param_text_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    pub fn hit_test_pitch_text(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        if self.warp_mode != WarpMode::Semitone { return false; }
        let (rp, rs) = Self::warp_param_text_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    pub fn vol_fader_layout(screen_w: f32, screen_h: f32, scale: f32) -> VolFaderLayout {
        let (pp, _ps) = Self::panel_rect(screen_w, screen_h, scale);
        let (track_pos, track_size) = Self::vol_fader_rects(screen_w, screen_h, scale);
        let center_x = track_pos[0] + track_size[0] * 0.5;
        let rw_w = RIGHT_WINDOW_WIDTH * scale;
        let db_text_y = track_pos[1] + track_size[1] + 4.0 * scale;
        VolFaderLayout {
            track_pos,
            track_size,
            center_x,
            label_y: track_pos[1] - 18.0 * scale,
            triangle_x: track_pos[0] - 14.0 * scale,
            scale_labels_x: track_pos[0] + track_size[0] + 11.0 * scale,
            db_text_y,
            db_text_rect: ([pp[0], db_text_y], [rw_w, 20.0 * scale]),
            bracket_x0: center_x - 20.0 * scale,
            bracket_x1: center_x + 20.0 * scale,
            bracket_y0: track_pos[1] - 22.0 * scale,
            bracket_y1: track_pos[1] + track_size[1] + 30.0 * scale,
            tick_x_offset: track_pos[0],
        }
    }

    pub fn pan_knob_layout(screen_w: f32, screen_h: f32, scale: f32) -> PanKnobLayout {
        let center = Self::pan_knob_center(screen_w, screen_h, scale);
        let radius = KNOB_R * scale;
        let label_y = center[1] - radius - 18.0 * scale;
        let value_y = center[1] + radius + 4.0 * scale;
        PanKnobLayout {
            center,
            radius,
            label_y,
            value_y,
            bracket_x0: center[0] - 30.0 * scale,
            bracket_x1: center[0] + 30.0 * scale,
            bracket_y0: label_y - 4.0 * scale,
            bracket_y1: value_y + 18.0 * scale,
        }
    }

    pub fn pitch_text_layout(screen_w: f32, screen_h: f32, scale: f32) -> PitchTextLayout {
        let (pp, _ps) = Self::panel_rect(screen_w, screen_h, scale);
        let (text_pos, text_size) = Self::warp_param_text_rect(screen_w, screen_h, scale);
        let rw_w = RIGHT_WINDOW_WIDTH * scale;
        let cx = pp[0] + rw_w * 0.5;
        // label_line=15, val_line=16 in gpu.rs; total visible text ~31px
        let content_h = 31.0 * scale;
        // Text is left-aligned at text_pos[0] (panel left edge); center brackets on text area
        let text_cx = text_pos[0] + rw_w * 0.5;
        PitchTextLayout {
            label_pos: [cx, text_pos[1]],
            value_pos: [cx, text_pos[1] + 15.0 * scale],
            text_rect: (text_pos, text_size),
            bracket_x0: text_pos[0] + 4.0 * scale,
            bracket_x1: text_pos[0] + rw_w - 4.0 * scale,
            bracket_y0: text_pos[1] - 2.0 * scale,
            bracket_y1: text_pos[1] + content_h + 4.0 * scale,
        }
    }

    pub fn sample_bpm_text_layout(screen_w: f32, screen_h: f32, scale: f32) -> SampleBpmTextLayout {
        let (pp, _ps) = Self::panel_rect(screen_w, screen_h, scale);
        let (text_pos, text_size) = Self::warp_param_text_rect(screen_w, screen_h, scale);
        let rw_w = RIGHT_WINDOW_WIDTH * scale;
        let cx = pp[0] + rw_w * 0.5;
        let content_h = 31.0 * scale;
        SampleBpmTextLayout {
            label_pos: [cx, text_pos[1]],
            value_pos: [cx, text_pos[1] + 15.0 * scale],
            text_rect: (text_pos, text_size),
            bracket_x0: text_pos[0] + 4.0 * scale,
            bracket_x1: text_pos[0] + rw_w - 4.0 * scale,
            bracket_y0: text_pos[1] - 2.0 * scale,
            bracket_y1: text_pos[1] + content_h + 4.0 * scale,
        }
    }

    pub fn warp_mode_button_rect_pub(screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        Self::warp_mode_button_rect(screen_w, screen_h, scale)
    }


    pub fn hit_test_vol_text(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let layout = Self::vol_fader_layout(screen_w, screen_h, scale);
        let (rp, rs) = layout.db_text_rect;
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }



    fn value_to_angle(v: f32) -> f32 {
        // 135° (10-o'clock) to 45° (2-o'clock), 270° sweep, center (0.5) = 12 o'clock
        let deg = 135.0 + v.clamp(0.0, 1.0) * 270.0;
        deg.to_radians()
    }

    fn push_knob(out: &mut Vec<InstanceRaw>, cx: f32, cy: f32, value: f32, scale: f32, theme: &crate::theme::RuntimeTheme) {
        let kr = KNOB_R * scale;
        let dot_r = KNOB_DOT_R * scale;
        let ind_r = KNOB_INDICATOR_R * scale;

        // Knob background circle
        out.push(InstanceRaw {
            position: [cx - kr, cy - kr],
            size: [kr * 2.0, kr * 2.0],
            color: [0.18, 0.18, 0.22, 1.0],
            border_radius: kr,
        });

        // Arc dots
        for i in 0..ARC_DOTS {
            let t = i as f32 / ARC_DOTS as f32;
            let angle = Self::value_to_angle(t);
            let arc_r = (KNOB_R - 6.0) * scale;
            let dx = angle.cos() * arc_r;
            let dy = angle.sin() * arc_r;
            let color = if (t - 0.5) * (value - 0.5) > 0.0
                && (t - 0.5).abs() <= (value - 0.5).abs()
            {
                theme.accent
            } else {
                theme.knob_inactive
            };
            out.push(InstanceRaw {
                position: [cx + dx - dot_r, cy + dy - dot_r],
                size: [dot_r * 2.0, dot_r * 2.0],
                color,
                border_radius: dot_r,
            });
        }

        // Indicator dot
        let ind_angle = Self::value_to_angle(value);
        let ind_arc_r = (KNOB_R - 6.0) * scale;
        let idx = ind_angle.cos() * ind_arc_r;
        let idy = ind_angle.sin() * ind_arc_r;
        out.push(InstanceRaw {
            position: [cx + idx - ind_r, cy + idy - ind_r],
            size: [ind_r * 2.0, ind_r * 2.0],
            color: [1.0, 1.0, 1.0, 0.95],
            border_radius: ind_r,
        });
    }

    pub fn build_instances(&self, settings: &crate::settings::Settings, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pp, ps) = Self::panel_rect(screen_w, screen_h, scale);

        // Left separator line
        out.push(InstanceRaw {
            position: [pp[0], pp[1]],
            size: [1.0 * scale, ps[1]],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        // Panel background
        out.push(InstanceRaw {
            position: [pp[0] + 1.0 * scale, pp[1]],
            size: [ps[0] - 1.0 * scale, ps[1]],
            color: settings.theme.bg_base,
            border_radius: 0.0,
        });

        // Header background
        out.push(InstanceRaw {
            position: [pp[0] + 1.0 * scale, pp[1]],
            size: [ps[0] - 1.0 * scale, HEADER_HEIGHT * scale],
            color: settings.theme.bg_surface,
            border_radius: 0.0,
        });

        // Header divider
        out.push(InstanceRaw {
            position: [pp[0] + 1.0 * scale, pp[1] + HEADER_HEIGHT * scale],
            size: [ps[0] - 1.0 * scale, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        // Volume fader
        let vol_pos = gain_to_vol_fader_pos(self.volume);
        let layout = Self::vol_fader_layout(screen_w, screen_h, scale);
        let track_pos = layout.track_pos;
        let track_size = layout.track_size;
        let thumb_y = Self::vol_fader_thumb_y(vol_pos, track_pos, track_size[1]);

        // Track background
        out.push(InstanceRaw {
            position: track_pos,
            size: track_size,
            color: [0.2, 0.2, 0.25, 1.0],
            border_radius: FADER_TRACK_W * 0.5 * scale,
        });

        // Focus corner brackets — enclose Gain label, fader track, ticks, scale labels, dB value
        if self.vol_fader_focused {
            let bracket_len = 10.0 * scale;
            let thick = 1.0 * scale;
            let color = [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.7];
            let x0 = layout.bracket_x0;
            let x1 = layout.bracket_x1;
            let y0 = layout.bracket_y0;
            let y1 = layout.bracket_y1;
            // Top-left
            out.push(InstanceRaw { position: [x0, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Top-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-left
            out.push(InstanceRaw { position: [x0, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
        }

        // Fill anchored at 0 dB: extends up when volume > 0 dB, down when volume < 0 dB
        let y_zero = Self::vol_fader_thumb_y(VOL_FADER_P_ZERO, track_pos, track_size[1]);
        let fill_top = thumb_y.min(y_zero);
        let fill_bot = thumb_y.max(y_zero);
        let fill_h = fill_bot - fill_top;
        if fill_h > 0.5 {
            out.push(InstanceRaw {
                position: [track_pos[0], fill_top],
                size: [track_size[0], fill_h],
                color: settings.theme.accent,
                border_radius: FADER_TRACK_W * 0.5 * scale,
            });
        }

        // Tick marks to the right of the track (standard mixer scale)
        // Major ticks (6px) at: +24, 0, -70. Minor ticks (3px) every 6 dB in between.
        let tick_db_values: &[(f32, bool)] = &[
            (24.0, true), (18.0, false), (12.0, false), (6.0, false),
            (0.0, true),
            (-6.0, false), (-12.0, false), (-18.0, false), (-24.0, false),
            (-30.0, false), (-36.0, false), (-42.0, false), (-48.0, false),
            (-54.0, false), (-60.0, false),
            (-70.0, true),
        ];
        for &(db, major) in tick_db_values {
            let fpos = if db <= VOL_FADER_DB_BOTTOM {
                VOL_FADER_P_BOTTOM
            } else {
                gain_to_vol_fader_pos(crate::ui::palette::db_to_gain(db))
            };
            let ty = Self::vol_fader_thumb_y(fpos, track_pos, track_size[1]);
            let tick_w = if major { 6.0 } else { 3.0 };
            // Ticks extend leftward from the track left edge
            let tick_x = layout.tick_x_offset - (tick_w + 3.0) * scale;
            out.push(InstanceRaw {
                position: [tick_x, ty - 0.5 * scale],
                size: [tick_w * scale, 1.0 * scale],
                color: [0.6, 0.6, 0.65, 0.7],
                border_radius: 0.0,
            });
        }

        // No thumb circle — triangle indicator is rendered as text in gpu.rs

        // Pan knob
        let pan_layout = Self::pan_knob_layout(screen_w, screen_h, scale);
        Self::push_knob(&mut out, pan_layout.center[0], pan_layout.center[1], self.pan, scale, &settings.theme);

        // Pan knob focus brackets
        if self.pan_knob_focused {
            let bracket_len = 10.0 * scale;
            let thick = 1.0 * scale;
            let color = [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.7];
            let x0 = pan_layout.bracket_x0;
            let x1 = pan_layout.bracket_x1;
            let y0 = pan_layout.bracket_y0;
            let y1 = pan_layout.bracket_y1;
            // Top-left
            out.push(InstanceRaw { position: [x0, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Top-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-left
            out.push(InstanceRaw { position: [x0, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
        }

        // Reverse button
        let (rev_pos, rev_size) = Self::reverse_button_rect(screen_w, screen_h, scale);
        let rev_color = if self.is_reversed { settings.theme.accent } else { [0.2, 0.2, 0.25, 1.0] };
        out.push(InstanceRaw {
            position: rev_pos,
            size: rev_size,
            color: rev_color,
            border_radius: 4.0 * scale,
        });

        // Warp toggle button
        let (btn_pos, btn_size) = Self::warp_mode_button_rect(screen_w, screen_h, scale);
        let warp_on = self.warp_mode != WarpMode::Off;
        let btn_color = if warp_on { settings.theme.accent } else { [0.2, 0.2, 0.25, 1.0] };
        out.push(InstanceRaw {
            position: btn_pos,
            size: btn_size,
            color: btn_color,
            border_radius: 4.0 * scale,
        });

        // Warp mode selector (only when warp is on)
        if warp_on {
            let (sel_pos, sel_size) = Self::warp_mode_selector_rect(screen_w, screen_h, scale);
            out.push(InstanceRaw {
                position: sel_pos,
                size: sel_size,
                color: [0.16, 0.16, 0.20, 1.0],
                border_radius: 4.0 * scale,
            });
        }

        // Sample BPM text focus brackets
        if self.sample_bpm_focused && self.warp_mode == WarpMode::RePitch {
            let sl = Self::sample_bpm_text_layout(screen_w, screen_h, scale);
            let bracket_len = 10.0 * scale;
            let thick = 1.0 * scale;
            let color = [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.7];
            let x0 = sl.bracket_x0;
            let x1 = sl.bracket_x1;
            let y0 = sl.bracket_y0;
            let y1 = sl.bracket_y1;
            // Top-left
            out.push(InstanceRaw { position: [x0, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Top-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-left
            out.push(InstanceRaw { position: [x0, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
        }

        // Pitch text focus brackets
        if self.pitch_focused && self.warp_mode == WarpMode::Semitone {
            let pl = Self::pitch_text_layout(screen_w, screen_h, scale);
            let bracket_len = 10.0 * scale;
            let thick = 1.0 * scale;
            let color = [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.7];
            let x0 = pl.bracket_x0;
            let x1 = pl.bracket_x1;
            let y0 = pl.bracket_y0;
            let y1 = pl.bracket_y1;
            // Top-left
            out.push(InstanceRaw { position: [x0, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Top-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y0], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y0], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-left
            out.push(InstanceRaw { position: [x0, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x0, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
            // Bottom-right
            out.push(InstanceRaw { position: [x1 - bracket_len, y1 - thick], size: [bracket_len, thick], color, border_radius: 0.0 });
            out.push(InstanceRaw { position: [x1 - thick, y1 - bracket_len], size: [thick, bracket_len], color, border_radius: 0.0 });
        }

        out
    }

    /// Format volume value as dB string
    pub fn vol_text(&self) -> String {
        if self.volume < 0.00001 {
            "-inf".to_string()
        } else {
            let db = gain_to_db(self.volume);
            if db >= 0.0 {
                format!("+{:.1} dB", db)
            } else {
                format!("{:.1} dB", db)
            }
        }
    }

    /// Format pan value as string
    pub fn pan_text(&self) -> String {
        let p = self.pan;
        if (p - 0.5).abs() < 0.01 {
            "C".to_string()
        } else if p < 0.5 {
            let pct = ((0.5 - p) * 200.0).round() as u32;
            format!("L {}%", pct)
        } else {
            let pct = ((p - 0.5) * 200.0).round() as u32;
            format!("R {}%", pct)
        }
    }

    /// Format warp toggle button text
    pub fn warp_button_text(&self) -> &'static str {
        if self.warp_mode == WarpMode::Off { "OFF" } else { "ON" }
    }

    /// Format warp mode selector text
    pub fn warp_mode_selector_text(&self) -> &'static str {
        match self.warp_mode {
            WarpMode::RePitch => "REPITCH",
            WarpMode::Semitone | WarpMode::Off => "SEMITONE",
        }
    }

    /// Format pitch value as semitones string
    pub fn pitch_text(&self) -> String {
        let p = self.pitch_semitones;
        if p.abs() < 0.05 {
            "0 st".to_string()
        } else {
            let rounded = p.round() as i32;
            if rounded > 0 {
                format!("+{} st", rounded)
            } else {
                format!("{} st", rounded)
            }
        }
    }

    /// Format sample BPM as display string
    pub fn sample_bpm_text(&self) -> String {
        format!("{:.1}", self.sample_bpm)
    }

    /// Compute new volume from drag delta (up = increase)
    pub fn drag_vol_delta(drag_start_y: f32, mouse_y: f32, drag_start_value: f32, scale: f32) -> f32 {
        let delta = (drag_start_y - mouse_y) / (200.0 * scale);
        let new_pos = (drag_start_value + delta).clamp(0.0, 1.0);
        vol_fader_pos_to_gain(new_pos)
    }

    /// Compute new pan from drag delta (up = increase)
    pub fn drag_pan_delta(drag_start_y: f32, mouse_y: f32, drag_start_value: f32, scale: f32) -> f32 {
        let delta = (drag_start_y - mouse_y) / (200.0 * scale);
        (drag_start_value + delta).clamp(0.0, 1.0)
    }

    /// Compute new sample BPM from drag delta (up = increase)
    pub fn drag_sample_bpm_delta(drag_start_y: f32, mouse_y: f32, drag_start_value: f32, scale: f32) -> f32 {
        let delta = (drag_start_y - mouse_y) / (2.0 * scale);
        (drag_start_value + delta).clamp(20.0, 999.0)
    }

    /// Compute new pitch semitones from drag delta (up = increase)
    pub fn drag_pitch_delta(drag_start_y: f32, mouse_y: f32, drag_start_value: f32, scale: f32) -> f32 {
        let delta = (drag_start_y - mouse_y) / (8.0 * scale);
        (drag_start_value + delta).clamp(-24.0, 24.0)
    }

    pub fn get_text_entries(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (pp, _) = Self::panel_rect(screen_w, screen_h, scale);
        let layout = Self::vol_fader_layout(screen_w, screen_h, scale);
        let fader_pos = layout.track_pos;
        let fader_size = layout.track_size;
        let pan_layout = Self::pan_knob_layout(screen_w, screen_h, scale);
        let header_font = 10.0 * scale;
        let header_line = 14.0 * scale;
        let label_font = 11.0 * scale;
        let label_line = 15.0 * scale;
        let val_font = 12.0 * scale;
        let val_line = 16.0 * scale;
        let rw_w = RIGHT_WINDOW_WIDTH * scale;

        // "INSPECTOR" header label
        out.push(TextEntry {
            text: "INSPECTOR".to_string(),
            x: pp[0] + 12.0 * scale,
            y: 11.0 * scale,
            font_size: header_font,
            line_height: header_line,
            max_width: rw_w,
            color: [120, 140, 170, 200],
            weight: 400,
            bounds: None,
            center: false,
        });

        // Fader geometry helpers
        let vol_fader_pos_val = gain_to_vol_fader_pos(self.volume);
        let thumb_y = fader_pos[1] + (1.0 - vol_fader_pos_val) * fader_size[1];

        // "Gain" label (above fader top, centered)
        out.push(TextEntry {
            text: "Gain".to_string(),
            x: pp[0],
            y: layout.label_y,
            font_size: label_font,
            line_height: label_line,
            max_width: rw_w,
            color: [140, 140, 150, 180],
            weight: 400,
            bounds: None,
            center: true,
        });

        // Triangle indicator (▶) to the left of the track at thumb position
        let tri_font = 10.0 * scale;
        let tri_line = 12.0 * scale;
        out.push(TextEntry {
            text: "▶".to_string(),
            x: layout.triangle_x,
            y: thumb_y - tri_line * 0.5,
            font_size: tri_font,
            line_height: tri_line,
            max_width: 16.0 * scale,
            color: [220, 220, 230, 230],
            weight: 400,
            bounds: None,
            center: false,
        });

        // Scale labels to the right of tick marks
        let scale_font = 9.0 * scale;
        let scale_line = 11.0 * scale;
        let label_x = layout.scale_labels_x;
        let label_bounds = Some([label_x, 0.0, label_x + 30.0 * scale, screen_h]);

        // "+24" at fader top
        out.push(TextEntry {
            text: "24".to_string(),
            x: label_x,
            y: fader_pos[1] - scale_line * 0.5,
            font_size: scale_font,
            line_height: scale_line,
            max_width: 30.0 * scale,
            color: [140, 140, 150, 160],
            weight: 400,
            bounds: label_bounds,
            center: false,
        });

        // "0" at 0 dB position
        let y_zero = fader_pos[1] + (1.0 - VOL_FADER_P_ZERO) * fader_size[1];
        out.push(TextEntry {
            text: "0".to_string(),
            x: label_x,
            y: y_zero - scale_line * 0.5,
            font_size: scale_font,
            line_height: scale_line,
            max_width: 30.0 * scale,
            color: [140, 140, 150, 160],
            weight: 400,
            bounds: label_bounds,
            center: false,
        });

        // "70" at -70 dB position (near bottom)
        let y_70 = fader_pos[1] + (1.0 - VOL_FADER_P_BOTTOM) * fader_size[1];
        out.push(TextEntry {
            text: "70".to_string(),
            x: label_x,
            y: y_70 - scale_line * 0.5,
            font_size: scale_font,
            line_height: scale_line,
            max_width: 30.0 * scale,
            color: [140, 140, 150, 160],
            weight: 400,
            bounds: label_bounds,
            center: false,
        });

        // dB value below fader — centered on the fader track
        let vol_idle = self.vol_text();
        let vol_display = self.vol_entry.display(&vol_idle);
        let vol_alpha: u8 = if self.vol_entry.is_editing() { 255 } else { 220 };
        out.push(TextEntry {
            text: vol_display.to_string(),
            x: pp[0],
            y: layout.db_text_y,
            font_size: val_font,
            line_height: val_line,
            max_width: rw_w,
            color: [200, 200, 210, vol_alpha],
            weight: 400,
            bounds: None,
            center: true,
        });

        // PAN label — centered at the knob center
        out.push(TextEntry {
            text: "Pan".to_string(),
            x: pp[0],
            y: pan_layout.label_y,
            font_size: label_font,
            line_height: label_line,
            max_width: rw_w,
            color: [140, 140, 150, 180],
            weight: 400,
            bounds: None,
            center: true,
        });

        // PAN value — below the knob
        let pan_text = self.pan_text();
        out.push(TextEntry {
            text: pan_text,
            x: pp[0],
            y: pan_layout.value_y,
            font_size: val_font,
            line_height: val_line,
            max_width: rw_w,
            color: [200, 200, 210, 220],
            weight: 400,
            bounds: None,
            center: true,
        });

        // Reverse button text
        let (rev_pos, rev_size) = Self::reverse_button_rect(screen_w, screen_h, scale);
        out.push(TextEntry {
            text: "Reverse".to_string(),
            x: rev_pos[0],
            y: rev_pos[1] + (rev_size[1] - val_line) * 0.5,
            font_size: val_font,
            line_height: val_line,
            max_width: rev_size[0],
            color: [220, 220, 230, 240],
            weight: 400,
            bounds: None,
            center: false,
        });

        // WARP label
        let (btn_pos, btn_size) = Self::warp_mode_button_rect_pub(screen_w, screen_h, scale);
        out.push(TextEntry {
            text: "WARP".to_string(),
            x: btn_pos[0] + btn_size[0] * 0.5 - rw_w * 0.5,
            y: btn_pos[1] - 18.0 * scale,
            font_size: label_font,
            line_height: label_line,
            max_width: rw_w,
            color: [140, 140, 150, 180],
            weight: 400,
            bounds: None,
            center: false,
        });

        // WARP toggle text (centered on button)
        let warp_text = self.warp_button_text();
        out.push(TextEntry {
            text: warp_text.to_string(),
            x: btn_pos[0],
            y: btn_pos[1] + (btn_size[1] - val_line) * 0.5,
            font_size: val_font,
            line_height: val_line,
            max_width: btn_size[0],
            color: [220, 220, 230, 240],
            weight: 400,
            bounds: None,
            center: false,
        });

        let warp_on = self.warp_mode != WarpMode::Off;
        if warp_on {
            // Mode selector text (centered on selector rect)
            let (sel_pos, sel_size) = Self::warp_mode_selector_rect_pub(screen_w, screen_h, scale);
            let mode_text = self.warp_mode_selector_text();
            out.push(TextEntry {
                text: mode_text.to_string(),
                x: sel_pos[0],
                y: sel_pos[1] + (sel_size[1] - val_line) * 0.5,
                font_size: val_font,
                line_height: val_line,
                max_width: sel_size[0],
                color: [200, 200, 210, 220],
                weight: 400,
                bounds: None,
            center: false,
            });

            // Mode-specific param label + value
            let (param_pos, _param_size) = Self::warp_param_text_rect_pub(screen_w, screen_h, scale);
            if self.warp_mode == WarpMode::RePitch {
                out.push(TextEntry {
                    text: "SAMPLE BPM".to_string(),
                    x: param_pos[0],
                    y: param_pos[1],
                    font_size: label_font,
                    line_height: label_line,
                    max_width: rw_w,
                    color: [140, 140, 150, 180],
                    weight: 400,
                    bounds: None,
                    center: true,
                });

                let sbpm_idle = self.sample_bpm_text();
                let sbpm_display = self.sample_bpm_entry.display(&sbpm_idle);
                let sbpm_alpha: u8 = if self.sample_bpm_entry.is_editing() { 255 } else { 220 };
                out.push(TextEntry {
                    text: sbpm_display.to_string(),
                    x: param_pos[0],
                    y: param_pos[1] + label_line,
                    font_size: val_font,
                    line_height: val_line,
                    max_width: rw_w,
                    color: [200, 200, 210, sbpm_alpha],
                    weight: 400,
                    bounds: None,
                    center: true,
                });
            } else if self.warp_mode == WarpMode::Semitone {
                out.push(TextEntry {
                    text: "PITCH".to_string(),
                    x: param_pos[0],
                    y: param_pos[1],
                    font_size: label_font,
                    line_height: label_line,
                    max_width: rw_w,
                    color: [140, 140, 150, 180],
                    weight: 400,
                    bounds: None,
                    center: true,
                });

                let pitch_idle = self.pitch_text();
                let pitch_display = self.pitch_entry.display(&pitch_idle);
                let pitch_alpha: u8 = if self.pitch_entry.is_editing() { 255 } else { 220 };
                out.push(TextEntry {
                    text: pitch_display.to_string(),
                    x: param_pos[0],
                    y: param_pos[1] + label_line,
                    font_size: val_font,
                    line_height: val_line,
                    max_width: rw_w,
                    color: [200, 200, 210, pitch_alpha],
                    weight: 400,
                    bounds: None,
                    center: true,
                });
            }
        }

        out
    }
}
