use crate::gpu::{InstanceRaw, TextEntry};
use crate::TimeInstant;

const TOOLTIP_DELAY_MS: u128 = 300;
const TOOLTIP_FONT_SIZE: f32 = 11.0;
const TOOLTIP_PADDING_H: f32 = 8.0;
const TOOLTIP_PADDING_V: f32 = 4.0;
const TOOLTIP_GAP: f32 = 6.0;
const TOOLTIP_BORDER_RADIUS: f32 = 6.0;

pub(crate) struct TooltipState {
    current_target: Option<String>,
    text: String,
    target_rect: ([f32; 2], [f32; 2]),
    hover_start: TimeInstant,
    visible: bool,
}

impl TooltipState {
    pub(crate) fn new() -> Self {
        Self {
            current_target: None,
            text: String::new(),
            target_rect: ([0.0; 2], [0.0; 2]),
            hover_start: TimeInstant::now(),
            visible: false,
        }
    }

    pub(crate) fn set_target(&mut self, id: &str, text: &str, rect: ([f32; 2], [f32; 2])) {
        if self.current_target.as_deref() != Some(id) {
            self.current_target = Some(id.to_string());
            self.text = text.to_string();
            self.target_rect = rect;
            self.hover_start = TimeInstant::now();
            self.visible = false;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.current_target = None;
        self.visible = false;
    }

    pub(crate) fn tick(&mut self) {
        if self.current_target.is_some() && !self.visible {
            if self.hover_start.elapsed().as_millis() >= TOOLTIP_DELAY_MS {
                self.visible = true;
            }
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.current_target.is_some() && !self.visible
    }

    pub(crate) fn build_instances(&self, scale: f32, theme: &crate::theme::RuntimeTheme) -> Vec<InstanceRaw> {
        if !self.visible || self.current_target.is_none() {
            return Vec::new();
        }
        let font_size = TOOLTIP_FONT_SIZE * scale;
        let char_w = font_size * 0.55;
        let text_w = char_w * self.text.len() as f32;
        let pad_h = TOOLTIP_PADDING_H * scale;
        let pad_v = TOOLTIP_PADDING_V * scale;
        let pill_w = text_w + pad_h * 2.0;
        let pill_h = font_size + pad_v * 2.0;
        let gap = TOOLTIP_GAP * scale;

        let (tpos, tsize) = self.target_rect;
        let pill_x = tpos[0] + (tsize[0] - pill_w) * 0.5;
        let pill_y = tpos[1] - pill_h - gap;

        vec![InstanceRaw {
            position: [pill_x, pill_y],
            size: [pill_w, pill_h],
            color: theme.tooltip_bg,
            border_radius: TOOLTIP_BORDER_RADIUS * scale, shadow_blur: 0.0,
        }]
    }

    pub(crate) fn build_text_entries(&self, scale: f32, theme: &crate::theme::RuntimeTheme) -> Vec<TextEntry> {
        if !self.visible || self.current_target.is_none() {
            return Vec::new();
        }
        let font_size = TOOLTIP_FONT_SIZE * scale;
        let char_w = font_size * 0.55;
        let text_w = char_w * self.text.len() as f32;
        let pad_h = TOOLTIP_PADDING_H * scale;
        let pad_v = TOOLTIP_PADDING_V * scale;
        let pill_w = text_w + pad_h * 2.0;
        let pill_h = font_size + pad_v * 2.0;
        let gap = TOOLTIP_GAP * scale;

        let (tpos, tsize) = self.target_rect;
        let pill_x = tpos[0] + (tsize[0] - pill_w) * 0.5;
        let pill_y = tpos[1] - pill_h - gap;

        let line_height = font_size * 1.3;

        vec![TextEntry {
            text: self.text.clone(),
            x: pill_x + pad_h,
            y: pill_y + (pill_h - line_height) * 0.5,
            font_size,
            line_height,
            max_width: pill_w,
            color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, 240),
            weight: 400,
            bounds: None,
            center: false,
        }]
    }

    #[cfg(test)]
    pub(crate) fn force_elapsed(&mut self) {
        self.hover_start = TimeInstant::now() - std::time::Duration::from_secs(1);
    }
}
