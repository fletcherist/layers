use crate::InstanceRaw;

const WIN_W: f32 = 360.0;
const HEADER_H: f32 = 36.0;
const ROW_H: f32 = 32.0;
const SLIDER_TRACK_H: f32 = 4.0;
const SLIDER_THUMB_R: f32 = 6.0;
const PADDING: f32 = 14.0;
const LABEL_W: f32 = 120.0;
const BORDER_RADIUS: f32 = 8.0;
const MAX_VISIBLE_PARAMS: usize = 16;

pub struct ParamEntry {
    pub name: String,
    pub unit: String,
    pub value: f32,
    pub default: f32,
}

pub struct PluginEditorWindow {
    pub region_idx: usize,
    pub slot_idx: usize,
    pub plugin_name: String,
    pub params: Vec<ParamEntry>,
    pub dragging_slider: Option<usize>,
    pub scroll_offset: f32,
}

impl PluginEditorWindow {
    pub fn new(
        region_idx: usize,
        slot_idx: usize,
        plugin_name: String,
        params: Vec<ParamEntry>,
    ) -> Self {
        Self {
            region_idx,
            slot_idx,
            plugin_name,
            params,
            dragging_slider: None,
            scroll_offset: 0.0,
        }
    }

    fn win_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = WIN_W * scale;
        let visible = self.params.len().min(MAX_VISIBLE_PARAMS);
        let h = (HEADER_H + visible as f32 * ROW_H + PADDING) * scale;
        let x = (screen_w - w) * 0.5;
        let y = (screen_h - h) * 0.5;
        ([x, y], [w, h])
    }

    pub fn contains(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = self.win_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0] && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    fn slider_track_rect(
        &self,
        idx: usize,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, _) = self.win_rect(screen_w, screen_h, scale);
        let track_x = wp[0] + (PADDING + LABEL_W) * scale;
        let track_w = (WIN_W - PADDING * 2.0 - LABEL_W - 40.0) * scale;
        let track_y = wp[1]
            + HEADER_H * scale
            + idx as f32 * ROW_H * scale
            + (ROW_H * scale - SLIDER_TRACK_H * scale) * 0.5
            - self.scroll_offset;
        let track_h = SLIDER_TRACK_H * scale;
        ([track_x, track_y], [track_w, track_h])
    }

    pub fn slider_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let content_top = wp[1] + HEADER_H * scale;
        let content_bottom = wp[1] + ws[1];
        if mouse[1] < content_top || mouse[1] > content_bottom {
            return None;
        }
        for i in 0..self.params.len().min(MAX_VISIBLE_PARAMS) {
            let (tp, ts) = self.slider_track_rect(i, screen_w, screen_h, scale);
            if tp[1] < content_top - ROW_H * scale || tp[1] > content_bottom {
                continue;
            }
            let val = self.params[i].value;
            let thumb_x = tp[0] + val * ts[0];
            let thumb_cy = tp[1] + ts[1] * 0.5;
            let r = SLIDER_THUMB_R * scale + 4.0 * scale;
            let dx = mouse[0] - thumb_x;
            let dy = mouse[1] - thumb_cy;
            if dx * dx + dy * dy <= r * r {
                return Some(i);
            }
            if mouse[1] >= tp[1] - 4.0 * scale
                && mouse[1] <= tp[1] + ts[1] + 4.0 * scale
                && mouse[0] >= tp[0] - 2.0 * scale
                && mouse[0] <= tp[0] + ts[0] + 2.0 * scale
            {
                return Some(i);
            }
        }
        None
    }

    pub fn slider_drag(
        &mut self,
        idx: usize,
        mouse_x: f32,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> f32 {
        let (tp, ts) = self.slider_track_rect(idx, screen_w, screen_h, scale);
        let norm = ((mouse_x - tp[0]) / ts[0]).clamp(0.0, 1.0);
        if idx < self.params.len() {
            self.params[idx].value = norm;
        }
        norm
    }

    pub fn build_instances(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let br = BORDER_RADIUS * scale;

        // Backdrop
        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [screen_w, screen_h],
            color: [0.0, 0.0, 0.0, 0.40],
            border_radius: 0.0,
        });

        // Shadow
        let so = 8.0 * scale;
        out.push(InstanceRaw {
            position: [wp[0] + so, wp[1] + so],
            size: [ws[0] + 2.0 * scale, ws[1] + 2.0 * scale],
            color: [0.0, 0.0, 0.0, 0.40],
            border_radius: br,
        });

        // Window background
        out.push(InstanceRaw {
            position: wp,
            size: ws,
            color: [0.14, 0.13, 0.18, 0.98],
            border_radius: br,
        });

        // Header background
        out.push(InstanceRaw {
            position: wp,
            size: [ws[0], HEADER_H * scale],
            color: [0.18, 0.16, 0.24, 1.0],
            border_radius: br,
        });
        // Fill bottom corners of header
        out.push(InstanceRaw {
            position: [wp[0], wp[1] + HEADER_H * scale - br],
            size: [ws[0], br],
            color: [0.18, 0.16, 0.24, 1.0],
            border_radius: 0.0,
        });

        // Header divider
        out.push(InstanceRaw {
            position: [wp[0] + PADDING * scale, wp[1] + HEADER_H * scale],
            size: [ws[0] - PADDING * 2.0 * scale, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        let content_top = wp[1] + HEADER_H * scale;
        let content_bottom = wp[1] + ws[1];

        // Parameter rows
        for i in 0..self.params.len().min(MAX_VISIBLE_PARAMS) {
            let (tp, ts) = self.slider_track_rect(i, screen_w, screen_h, scale);
            if tp[1] + ts[1] < content_top || tp[1] > content_bottom {
                continue;
            }

            // Alternating row background
            if i % 2 == 1 {
                let row_y =
                    wp[1] + HEADER_H * scale + i as f32 * ROW_H * scale - self.scroll_offset;
                out.push(InstanceRaw {
                    position: [wp[0], row_y],
                    size: [ws[0], ROW_H * scale],
                    color: [1.0, 1.0, 1.0, 0.02],
                    border_radius: 0.0,
                });
            }

            // Slider track background
            out.push(InstanceRaw {
                position: tp,
                size: ts,
                color: [1.0, 1.0, 1.0, 0.10],
                border_radius: ts[1] * 0.5,
            });

            // Slider filled portion
            let val = self.params[i].value;
            out.push(InstanceRaw {
                position: tp,
                size: [ts[0] * val, ts[1]],
                color: [0.55, 0.28, 0.85, 0.60],
                border_radius: ts[1] * 0.5,
            });

            // Slider thumb
            let thumb_r = SLIDER_THUMB_R * scale;
            let thumb_x = tp[0] + val * ts[0] - thumb_r;
            let thumb_y = tp[1] + ts[1] * 0.5 - thumb_r;
            out.push(InstanceRaw {
                position: [thumb_x, thumb_y],
                size: [thumb_r * 2.0, thumb_r * 2.0],
                color: [0.70, 0.50, 1.00, 0.95],
                border_radius: thumb_r,
            });
        }

        out
    }
}

pub struct PluginEditorTextEntry {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub color: [u8; 4],
    pub weight: u16,
    pub max_width: f32,
}

impl PluginEditorWindow {
    pub fn get_text_entries(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<PluginEditorTextEntry> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);

        // Title
        let title_font = 12.0 * scale;
        let title_line = 16.0 * scale;
        out.push(PluginEditorTextEntry {
            text: self.plugin_name.clone(),
            x: wp[0] + PADDING * scale,
            y: wp[1] + (HEADER_H * scale - title_line) * 0.5,
            font_size: title_font,
            line_height: title_line,
            color: [230, 230, 240, 255],
            weight: 600,
            max_width: ws[0] - PADDING * 2.0 * scale,
        });

        let content_top = wp[1] + HEADER_H * scale;
        let content_bottom = wp[1] + ws[1];

        // Parameter labels and values
        let label_font = 11.0 * scale;
        let label_line = 15.0 * scale;
        for i in 0..self.params.len().min(MAX_VISIBLE_PARAMS) {
            let row_y = content_top + i as f32 * ROW_H * scale - self.scroll_offset;
            if row_y + ROW_H * scale < content_top || row_y > content_bottom {
                continue;
            }
            let param = &self.params[i];
            let text_y = row_y + (ROW_H * scale - label_line) * 0.5;

            // Parameter name
            out.push(PluginEditorTextEntry {
                text: param.name.clone(),
                x: wp[0] + PADDING * scale,
                y: text_y,
                font_size: label_font,
                line_height: label_line,
                color: [190, 190, 200, 255],
                weight: 400,
                max_width: LABEL_W * scale,
            });

            // Parameter value text
            let display_val = param.value * 100.0;
            let val_text = if param.unit.is_empty() {
                format!("{:.0}%", display_val)
            } else {
                format!("{:.0} {}", display_val, param.unit)
            };
            let (tp, ts) = self.slider_track_rect(i, screen_w, screen_h, scale);
            out.push(PluginEditorTextEntry {
                text: val_text,
                x: tp[0] + ts[0] + 6.0 * scale,
                y: text_y,
                font_size: label_font,
                line_height: label_line,
                color: [160, 160, 170, 255],
                weight: 400,
                max_width: 40.0 * scale,
            });
        }

        out
    }
}
