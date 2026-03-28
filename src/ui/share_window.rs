use crate::settings::Settings;
use crate::ui::text_input::{TextInput, TextInputConfig};
use crate::InstanceRaw;
use crate::gpu::TextEntry;

// ---------------------------------------------------------------------------
// Share window UI
// ---------------------------------------------------------------------------

const WIN_WIDTH: f32 = 420.0;
const WIN_HEIGHT: f32 = 200.0;
const BORDER_RADIUS: f32 = 12.0;
const PADDING: f32 = 20.0;
const URL_BOX_HEIGHT: f32 = 34.0;
const BTN_WIDTH: f32 = 120.0;
const BTN_HEIGHT: f32 = 32.0;
const COPIED_DURATION: f32 = 2.0;
const PROGRESS_BAR_HEIGHT: f32 = 6.0;

/// Width of the prefix text "https://layers.audio/projects/" at scale 1.0
const PREFIX_DISPLAY_WIDTH: f32 = 178.0;

const URL_PREFIX: &str = "https://layers.audio/projects/";

#[derive(Clone, PartialEq)]
pub enum ShareUploadState {
    Uploading,
    Done,
    Failed(String),
}

pub struct ShareWindow {
    pub id_input: TextInput,
    pub upload_state: ShareUploadState,
    pub progress: f32,
    pub progress_label: String,
    pub copy_hovered: bool,
    pub copied: bool,
    pub copied_timer: f32,
}

impl ShareWindow {
    pub fn new(share_id: String) -> Self {
        let config = TextInputConfig {
            multiline: false,
            allow_spaces: false,
            placeholder: String::new(),
        };
        Self {
            id_input: TextInput::with_text(share_id, config),
            upload_state: ShareUploadState::Uploading,
            progress: 0.0,
            progress_label: "Uploading…".to_string(),
            copy_hovered: false,
            copied: false,
            copied_timer: 0.0,
        }
    }

    pub fn full_url(&self) -> String {
        format!("{}{}", URL_PREFIX, self.id_input.text())
    }

    pub fn tick(&mut self, dt: f32) {
        if self.copied {
            self.copied_timer -= dt;
            if self.copied_timer <= 0.0 {
                self.copied = false;
                self.copied_timer = 0.0;
            }
        }
    }

    pub fn win_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = WIN_WIDTH * scale;
        let h = WIN_HEIGHT * scale;
        let x = (screen_w - w) * 0.5;
        let y = (screen_h - h) * 0.5;
        ([x, y], [w, h])
    }

    pub fn contains(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = self.win_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0]
            && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    fn url_box_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (wp, _) = self.win_rect(screen_w, screen_h, scale);
        let pad = PADDING * scale;
        // Place URL box below the progress area
        let y_offset = if self.upload_state == ShareUploadState::Uploading {
            80.0
        } else {
            36.0
        };
        let x = wp[0] + pad;
        let y = wp[1] + y_offset * scale;
        let w = WIN_WIDTH * scale - pad * 2.0;
        let h = URL_BOX_HEIGHT * scale;
        ([x, y], [w, h])
    }

    fn progress_bar_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let bar_w = ws[0] - PADDING * 2.0 * scale;
        let bar_h = PROGRESS_BAR_HEIGHT * scale;
        let bar_x = wp[0] + PADDING * scale;
        let bar_y = wp[1] + 56.0 * scale;
        ([bar_x, bar_y], [bar_w, bar_h])
    }

    fn copy_button_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let btn_w = BTN_WIDTH * scale;
        let btn_h = BTN_HEIGHT * scale;
        let x = wp[0] + (ws[0] - btn_w) * 0.5;
        let y = wp[1] + ws[1] - PADDING * scale - btn_h;
        ([x, y], [btn_w, btn_h])
    }

    pub fn hit_copy_button(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        if self.upload_state == ShareUploadState::Uploading {
            return false;
        }
        let (bp, bs) = self.copy_button_rect(screen_w, screen_h, scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0]
            && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) {
        self.copy_hovered = self.hit_copy_button(pos, screen_w, screen_h, scale);
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    pub fn build_instances(
        &self,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let br = BORDER_RADIUS * scale;
        let t = &settings.theme;

        // Full-screen backdrop
        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [screen_w, screen_h],
            color: t.shadow_strong,
            border_radius: 0.0,
        });

        // Shadow
        let so = 10.0 * scale;
        out.push(InstanceRaw {
            position: [wp[0] + so, wp[1] + so],
            size: [ws[0] + 2.0 * scale, ws[1] + 2.0 * scale],
            color: t.shadow,
            border_radius: br,
        });

        // Window background
        out.push(InstanceRaw {
            position: wp,
            size: ws,
            color: t.bg_base,
            border_radius: br,
        });

        // URL box background
        let (ubp, ubs) = self.url_box_rect(screen_w, screen_h, scale);
        out.push(InstanceRaw {
            position: ubp,
            size: ubs,
            color: t.bg_input,
            border_radius: 6.0 * scale,
        });

        // Progress bar (only during upload)
        if self.upload_state == ShareUploadState::Uploading {
            let (bp, bs) = self.progress_bar_rect(screen_w, screen_h, scale);

            // Bar background
            out.push(InstanceRaw {
                position: bp,
                size: bs,
                color: crate::theme::with_alpha(t.bg_elevated, 0.8),
                border_radius: bs[1] * 0.5,
            });

            // Bar fill
            let fill_w = bs[0] * self.progress.clamp(0.0, 1.0);
            if fill_w > 0.5 {
                out.push(InstanceRaw {
                    position: bp,
                    size: [fill_w, bs[1]],
                    color: t.accent,
                    border_radius: bs[1] * 0.5,
                });
            }
        }

        // Copy button
        let (cbp, cbs) = self.copy_button_rect(screen_w, screen_h, scale);
        let btn_color = if self.upload_state == ShareUploadState::Uploading {
            crate::theme::with_alpha(t.accent, 0.3)
        } else if self.copy_hovered {
            t.accent
        } else {
            crate::theme::with_alpha(t.accent, 0.8)
        };
        out.push(InstanceRaw {
            position: cbp,
            size: cbs,
            color: btn_color,
            border_radius: 6.0 * scale,
        });

        out
    }

    pub fn get_text_entries(
        &self,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (wp, _ws) = self.win_rect(screen_w, screen_h, scale);
        let t = &settings.theme;

        let win_bounds = {
            let (wp2, ws2) = self.win_rect(screen_w, screen_h, scale);
            Some([wp2[0], wp2[1], wp2[0] + ws2[0], wp2[1] + ws2[1]])
        };

        // Title
        out.push(TextEntry {
            text: "Share project".to_string(),
            x: wp[0] + PADDING * scale,
            y: wp[1] + 12.0 * scale,
            font_size: 13.0 * scale,
            line_height: 18.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_primary, 255),
            weight: 600,
            max_width: (WIN_WIDTH - PADDING * 2.0) * scale,
            bounds: win_bounds,
            center: false,
        });

        // Progress text (only during upload)
        if self.upload_state == ShareUploadState::Uploading {
            // Label
            out.push(TextEntry {
                text: self.progress_label.clone(),
                x: wp[0] + PADDING * scale,
                y: wp[1] + 38.0 * scale,
                font_size: 11.0 * scale,
                line_height: 14.0 * scale,
                color: crate::theme::RuntimeTheme::text_u8(t.text_primary, 220),
                weight: 400,
                max_width: 300.0 * scale,
                bounds: win_bounds,
                center: false,
            });

            // Percentage
            let pct = (self.progress * 100.0) as i32;
            out.push(TextEntry {
                text: format!("{}%", pct),
                x: wp[0] + (WIN_WIDTH - PADDING) * scale - 40.0 * scale,
                y: wp[1] + 38.0 * scale,
                font_size: 11.0 * scale,
                line_height: 14.0 * scale,
                color: crate::theme::RuntimeTheme::text_u8(t.text_secondary, 200),
                weight: 400,
                max_width: 60.0 * scale,
                bounds: win_bounds,
                center: false,
            });
        }

        // URL box: prefix (static) + editable ID
        let (ubp, ubs) = self.url_box_rect(screen_w, screen_h, scale);
        let text_y = ubp[1] + (ubs[1] - 14.0 * scale) * 0.5;
        let url_bounds = Some([ubp[0], ubp[1], ubp[0] + ubs[0], ubp[1] + ubs[1]]);

        // Prefix text (non-editable, dim)
        out.push(TextEntry {
            text: URL_PREFIX.to_string(),
            x: ubp[0] + 8.0 * scale,
            y: text_y,
            font_size: 11.0 * scale,
            line_height: 14.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_dim, 160),
            weight: 400,
            max_width: PREFIX_DISPLAY_WIDTH * scale,
            bounds: url_bounds,
            center: false,
        });

        // Editable ID text (with cursor when not uploading)
        let id_display = if self.upload_state == ShareUploadState::Uploading {
            self.id_input.text().to_string()
        } else {
            self.id_input.display_text()
        };
        let prefix_offset = PREFIX_DISPLAY_WIDTH * scale;
        out.push(TextEntry {
            text: id_display,
            x: ubp[0] + 8.0 * scale + prefix_offset,
            y: text_y,
            font_size: 11.0 * scale,
            line_height: 14.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_primary, 255),
            weight: 400,
            max_width: ubs[0] - 16.0 * scale - prefix_offset,
            bounds: url_bounds,
            center: false,
        });

        // Copy button label
        let (cbp, cbs) = self.copy_button_rect(screen_w, screen_h, scale);
        let btn_label = if self.copied {
            "Copied!"
        } else if self.upload_state == ShareUploadState::Uploading {
            "Uploading…"
        } else {
            "Copy link"
        };
        out.push(TextEntry {
            text: btn_label.to_string(),
            x: cbp[0],
            y: cbp[1] + (cbs[1] - 14.0 * scale) * 0.5,
            font_size: 12.0 * scale,
            line_height: 14.0 * scale,
            color: [255, 255, 255, 255],
            weight: 500,
            max_width: cbs[0],
            bounds: win_bounds,
            center: true,
        });

        out
    }
}
