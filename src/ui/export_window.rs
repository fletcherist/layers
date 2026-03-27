use crate::settings::Settings;
use crate::entity_id::EntityId;
use crate::InstanceRaw;
use crate::gpu::TextEntry;
use crate::ui::dropdown;

// ---------------------------------------------------------------------------
// Export window UI
// ---------------------------------------------------------------------------

const WIN_WIDTH: f32 = 360.0;
const WIN_HEIGHT: f32 = 220.0;
const BORDER_RADIUS: f32 = 12.0;
const PADDING: f32 = 20.0;
const BUTTON_HEIGHT: f32 = 32.0;
const BUTTON_WIDTH: f32 = 100.0;
const PROGRESS_BAR_HEIGHT: f32 = 6.0;
const DROPDOWN_WIDTH: f32 = 220.0;
const DROPDOWN_HEIGHT: f32 = 28.0;

const FORMAT_LABELS: &[&str] = &["WAV", "MP3"];

#[derive(Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Wav,
    Mp3,
}

impl ExportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Wav => "WAV",
            Self::Mp3 => "MP3",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ExportState {
    /// User is selecting format options
    Idle,
    /// Export is in progress
    Exporting,
}

pub struct ExportWindow {
    pub group_id: EntityId,
    pub group_name: String,
    pub format: ExportFormat,
    pub state: ExportState,
    pub progress: f32,
    /// Receiver for progress updates from background export thread
    pub progress_rx: Option<std::sync::mpsc::Receiver<ExportProgress>>,
    // Dropdown state
    pub format_dropdown_open: bool,
    pub hovered_dropdown_item: Option<usize>,
    // Hover states
    pub hovered_export_btn: bool,
}

pub enum ExportProgress {
    Progress(f32),
    Done(Result<(), String>),
}

impl ExportWindow {
    pub fn new(group_id: EntityId, group_name: String) -> Self {
        Self {
            group_id,
            group_name,
            format: ExportFormat::Wav,
            state: ExportState::Idle,
            progress: 0.0,
            progress_rx: None,
            format_dropdown_open: false,
            hovered_dropdown_item: None,
            hovered_export_btn: false,
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
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0] && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    /// Returns the popup rect when the format dropdown is open, for GPU text clipping.
    pub fn open_dropdown_popup_rect(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<([f32; 2], [f32; 2])> {
        if !self.format_dropdown_open {
            return None;
        }
        let (dp, ds) = self.format_dropdown_rect(screen_w, screen_h, scale);
        Some(dropdown::popup_rect(dp, ds, FORMAT_LABELS.len(), scale))
    }

    /// Poll progress from background export thread. Returns true if export just completed.
    pub fn poll_progress(&mut self) -> Option<Result<(), String>> {
        let rx = self.progress_rx.as_ref()?;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                ExportProgress::Progress(p) => {
                    self.progress = p;
                }
                ExportProgress::Done(result) => {
                    self.progress_rx = None;
                    return Some(result);
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Hit testing
    // -----------------------------------------------------------------------

    fn format_dropdown_rect(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, _ws) = self.win_rect(screen_w, screen_h, scale);
        let dp = [wp[0] + PADDING * scale, wp[1] + 50.0 * scale];
        let ds = [DROPDOWN_WIDTH * scale, DROPDOWN_HEIGHT * scale];
        (dp, ds)
    }

    fn selected_format_idx(&self) -> usize {
        match self.format {
            ExportFormat::Wav => 0,
            ExportFormat::Mp3 => 1,
        }
    }

    fn format_from_idx(idx: usize) -> ExportFormat {
        match idx {
            1 => ExportFormat::Mp3,
            _ => ExportFormat::Wav,
        }
    }

    pub fn handle_format_click(
        &mut self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        let (dp, ds) = self.format_dropdown_rect(screen_w, screen_h, scale);
        match dropdown::handle_click(pos, dp, ds, FORMAT_LABELS.len(), scale, self.format_dropdown_open) {
            dropdown::ClickResult::Selected(idx) => {
                self.format = Self::format_from_idx(idx);
                self.format_dropdown_open = false;
                true
            }
            dropdown::ClickResult::Toggled(open) => {
                self.format_dropdown_open = open;
                true
            }
            dropdown::ClickResult::None => false,
        }
    }

    fn export_button_rect(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let btn_w = BUTTON_WIDTH * scale;
        let btn_h = BUTTON_HEIGHT * scale;
        let x = wp[0] + ws[0] - PADDING * scale - btn_w;
        let y = wp[1] + ws[1] - PADDING * scale - btn_h;
        ([x, y], [btn_w, btn_h])
    }

    pub fn hit_test_export_button(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if self.state != ExportState::Idle {
            return false;
        }
        let (bp, bs) = self.export_button_rect(screen_w, screen_h, scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0]
            && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    // -----------------------------------------------------------------------
    // Hover
    // -----------------------------------------------------------------------

    pub fn update_hover(&mut self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) {
        if self.state != ExportState::Idle {
            self.hovered_dropdown_item = None;
            self.hovered_export_btn = false;
            return;
        }
        if self.format_dropdown_open {
            let (dp, ds) = self.format_dropdown_rect(screen_w, screen_h, scale);
            self.hovered_dropdown_item =
                dropdown::hit_test_popup_item(pos, dp, ds, FORMAT_LABELS.len(), scale);
        } else {
            self.hovered_dropdown_item = None;
        }
        self.hovered_export_btn = self.hit_test_export_button(pos, screen_w, screen_h, scale);
    }

    // -----------------------------------------------------------------------
    // Instance rendering (build_instances)
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
            color: t.bg_window,
            border_radius: br,
        });

        match self.state {
            ExportState::Idle => {
                self.build_idle_instances(&mut out, settings, screen_w, screen_h, scale);
            }
            ExportState::Exporting => {
                self.build_exporting_instances(&mut out, settings, screen_w, screen_h, scale);
            }
        }

        out
    }

    fn build_idle_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) {
        let t = &settings.theme;

        // Format dropdown
        let (dp, ds) = self.format_dropdown_rect(screen_w, screen_h, scale);
        dropdown::render_button(out, dp, ds, scale, t);
        if self.format_dropdown_open {
            dropdown::render_popup(
                out,
                dp,
                ds,
                FORMAT_LABELS.len(),
                self.selected_format_idx(),
                self.hovered_dropdown_item,
                scale,
                t,
            );
        }

        // Export button
        let (ebp, ebs) = self.export_button_rect(screen_w, screen_h, scale);
        let btn_color = if self.hovered_export_btn {
            t.accent
        } else {
            crate::theme::with_alpha(t.accent, 0.8)
        };
        out.push(InstanceRaw {
            position: ebp,
            size: ebs,
            color: btn_color,
            border_radius: 6.0 * scale,
        });
    }

    fn build_exporting_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let t = &settings.theme;

        // Progress bar background
        let bar_w = ws[0] - PADDING * 2.0 * scale;
        let bar_h = PROGRESS_BAR_HEIGHT * scale;
        let bar_x = wp[0] + PADDING * scale;
        let bar_y = wp[1] + ws[1] * 0.5 - bar_h * 0.5 + 10.0 * scale;

        out.push(InstanceRaw {
            position: [bar_x, bar_y],
            size: [bar_w, bar_h],
            color: crate::theme::with_alpha(t.bg_elevated, 0.8),
            border_radius: bar_h * 0.5,
        });

        // Progress bar fill
        let fill_w = bar_w * self.progress.clamp(0.0, 1.0);
        if fill_w > 0.5 {
            out.push(InstanceRaw {
                position: [bar_x, bar_y],
                size: [fill_w, bar_h],
                color: t.accent,
                border_radius: bar_h * 0.5,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Text entries
    // -----------------------------------------------------------------------

    pub fn get_text_entries(
        &self,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let t = &settings.theme;

        // Clip rect for text inside the window
        let win_bounds = Some([wp[0], wp[1], wp[0] + ws[0], wp[1] + ws[1]]);

        // Window title (above window, no clip)
        let title_font = 13.0 * scale;
        let title_line = 18.0 * scale;
        out.push(TextEntry {
            text: "Export".to_string(),
            x: wp[0] + ws[0] * 0.5 - 18.0 * scale,
            y: wp[1] - title_line - 6.0 * scale,
            font_size: title_font,
            line_height: title_line,
            color: crate::theme::RuntimeTheme::text_u8(t.text_primary, 255),
            weight: 600,
            max_width: 300.0 * scale,
            bounds: None,
            center: false,
        });

        match self.state {
            ExportState::Idle => {
                self.build_idle_text(&mut out, settings, screen_w, screen_h, scale, win_bounds);
            }
            ExportState::Exporting => {
                self.build_exporting_text(&mut out, settings, screen_w, screen_h, scale, win_bounds);
            }
        }

        out
    }

    fn build_idle_text(
        &self,
        out: &mut Vec<TextEntry>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        win_bounds: Option<[f32; 4]>,
    ) {
        let (wp, _ws) = self.win_rect(screen_w, screen_h, scale);
        let t = &settings.theme;
        let label_font = 12.0 * scale;
        let label_line = 16.0 * scale;

        // "Format" label
        out.push(TextEntry {
            text: "Format".to_string(),
            x: wp[0] + PADDING * scale,
            y: wp[1] + 20.0 * scale,
            font_size: 11.0 * scale,
            line_height: 15.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_dim, 200),
            weight: 600,
            max_width: 300.0 * scale,
            bounds: win_bounds,
            center: false,
        });

        // Format dropdown value text
        let (dp, ds) = self.format_dropdown_rect(screen_w, screen_h, scale);
        dropdown::render_value_text(out, self.format.label(), dp, ds, scale, t);

        // Format dropdown popup text
        if self.format_dropdown_open {
            dropdown::build_popup_text(
                out,
                FORMAT_LABELS,
                self.selected_format_idx(),
                dp,
                ds,
                scale,
                t,
            );
        }

        // Format description
        let desc = match self.format {
            ExportFormat::Wav => "Lossless, 24-bit, 48 kHz",
            ExportFormat::Mp3 => "Compressed, 320 kbps, 48 kHz",
        };
        out.push(TextEntry {
            text: desc.to_string(),
            x: wp[0] + PADDING * scale,
            y: wp[1] + 86.0 * scale,
            font_size: 11.0 * scale,
            line_height: 14.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_dim, 160),
            weight: 400,
            max_width: 300.0 * scale,
            bounds: win_bounds,
            center: false,
        });

        // Export button text
        let (ebp, ebs) = self.export_button_rect(screen_w, screen_h, scale);
        out.push(TextEntry {
            text: "Export".to_string(),
            x: ebp[0] + ebs[0] * 0.5 - 16.0 * scale,
            y: ebp[1] + (ebs[1] - label_line) * 0.5,
            font_size: label_font,
            line_height: label_line,
            color: crate::theme::RuntimeTheme::text_u8(t.text_primary, 255),
            weight: 600,
            max_width: ebs[0],
            bounds: win_bounds,
            center: false,
        });
    }

    fn build_exporting_text(
        &self,
        out: &mut Vec<TextEntry>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        win_bounds: Option<[f32; 4]>,
    ) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let t = &settings.theme;

        // "Rendering audio..." label
        out.push(TextEntry {
            text: "Rendering audio...".to_string(),
            x: wp[0] + PADDING * scale,
            y: wp[1] + ws[1] * 0.5 - 24.0 * scale,
            font_size: 12.0 * scale,
            line_height: 16.0 * scale,
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
            x: wp[0] + ws[0] - PADDING * scale - 40.0 * scale,
            y: wp[1] + ws[1] * 0.5 - 24.0 * scale,
            font_size: 12.0 * scale,
            line_height: 16.0 * scale,
            color: crate::theme::RuntimeTheme::text_u8(t.text_secondary, 200),
            weight: 400,
            max_width: 60.0 * scale,
            bounds: win_bounds,
            center: false,
        });
    }
}
