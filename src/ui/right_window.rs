use crate::entity_id::EntityId;
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
const PITCH_KNOB_Y_OFFSET: f32 = 348.0;


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

pub struct RightWindow {
    pub waveform_id: EntityId,
    pub volume: f32,
    pub pan: f32,
    pub warp_mode: WarpMode,
    pub sample_bpm: f32,
    pub pitch_semitones: f32,
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
        let c = Self::pan_knob_center(screen_w, screen_h, scale);
        let r = (KNOB_R + 8.0) * scale;
        let dx = pos[0] - c[0];
        let dy = pos[1] - c[1];
        dx * dx + dy * dy <= r * r
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
        let pc = Self::pan_knob_center(screen_w, screen_h, scale);
        Self::push_knob(&mut out, pc[0], pc[1], self.pan, scale, &settings.theme);

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

    pub fn pan_knob_center_pub(screen_w: f32, screen_h: f32, scale: f32) -> [f32; 2] {
        Self::pan_knob_center(screen_w, screen_h, scale)
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
}
