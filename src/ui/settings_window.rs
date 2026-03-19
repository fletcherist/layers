use crate::settings::Settings;
#[cfg(feature = "native")]
use crate::settings::{available_driver_types, available_input_devices, available_output_devices};
use crate::InstanceRaw;

// ---------------------------------------------------------------------------
// Settings window UI
// ---------------------------------------------------------------------------

const WIN_WIDTH: f32 = 620.0;
const WIN_HEIGHT: f32 = 400.0;
const SIDEBAR_WIDTH: f32 = 180.0;
const BORDER_RADIUS: f32 = 12.0;
const SECTION_HEADER_HEIGHT: f32 = 36.0;
const ROW_HEIGHT: f32 = 38.0;
const SLIDER_TRACK_H: f32 = 5.0;
const SLIDER_THUMB_R: f32 = 7.0;
const SLIDER_WIDTH: f32 = 180.0;
const VALUE_WIDTH: f32 = 60.0;
const ROW_LABEL_X: f32 = 24.0;
const SLIDER_RIGHT_PAD: f32 = 24.0;
const DROPDOWN_WIDTH: f32 = 220.0;
const DROPDOWN_HEIGHT: f32 = 28.0;
const DROPDOWN_RIGHT_PAD: f32 = 24.0;
const DROPDOWN_ITEM_HEIGHT: f32 = 26.0;
const AUDIO_DROPDOWN_COUNT: usize = 3;
const THEME_PRESETS: &[&str] = &["Default", "Ableton"];

#[derive(Clone, Copy, PartialEq)]
pub enum SettingsCategory {
    ThemeAndColors,
    Audio,
    Developer,
}

impl SettingsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ThemeAndColors => "Theme & Colors",
            Self::Audio => "Audio",
            Self::Developer => "Developer",
        }
    }
}

pub const CATEGORIES: &[SettingsCategory] = &[
    SettingsCategory::ThemeAndColors,
    SettingsCategory::Audio,
    SettingsCategory::Developer,
];


struct SliderDef {
    label: &'static str,
    min: f32,
    max: f32,
    unit: &'static str,
    display_scale: f32,
}

const SLIDERS: &[SliderDef] = &[
    SliderDef {
        label: "Grid Line Intensity",
        min: 0.0,
        max: 1.0,
        unit: "%",
        display_scale: 100.0,
    },
    SliderDef {
        label: "Brightness",
        min: 0.0,
        max: 2.0,
        unit: "%",
        display_scale: 100.0,
    },
    SliderDef {
        label: "Color Intensity",
        min: 0.0,
        max: 1.0,
        unit: "%",
        display_scale: 100.0,
    },
    SliderDef {
        label: "Primary Color",
        min: 0.0,
        max: 360.0,
        unit: "°",
        display_scale: 1.0,
    },
];

pub struct SettingsWindow {
    pub active_category: SettingsCategory,
    pub hovered_category: Option<usize>,
    pub dragging_slider: Option<usize>,
    pub open_dropdown: Option<usize>,
    pub cached_driver_types: Vec<String>,
    pub cached_input_devices: Vec<String>,
    pub cached_output_devices: Vec<String>,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            active_category: SettingsCategory::ThemeAndColors,
            hovered_category: None,
            dragging_slider: None,
            open_dropdown: None,
            #[cfg(feature = "native")]
            cached_driver_types: available_driver_types(),
            #[cfg(not(feature = "native"))]
            cached_driver_types: vec!["Web Audio".to_string()],
            #[cfg(feature = "native")]
            cached_input_devices: available_input_devices(),
            #[cfg(not(feature = "native"))]
            cached_input_devices: vec!["No Device".to_string()],
            #[cfg(feature = "native")]
            cached_output_devices: available_output_devices(),
            #[cfg(not(feature = "native"))]
            cached_output_devices: vec!["No Device".to_string()],
        }
    }

    fn win_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
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

    fn slider_value(settings: &Settings, idx: usize) -> f32 {
        match idx {
            0 => settings.grid_line_intensity,
            1 => settings.brightness,
            2 => settings.color_intensity,
            3 => settings.primary_hue,
            _ => 0.0,
        }
    }

    fn set_slider_value(settings: &mut Settings, idx: usize, val: f32) {
        let def = &SLIDERS[idx];
        let clamped = val.clamp(def.min, def.max);
        match idx {
            0 => settings.grid_line_intensity = clamped,
            1 => {
                settings.brightness = clamped;
                settings.theme = crate::theme::RuntimeTheme::from_hue_with_settings(settings.primary_hue, settings.color_intensity, clamped);
            }
            2 => {
                settings.color_intensity = clamped;
                settings.theme = crate::theme::RuntimeTheme::from_hue_with_settings(settings.primary_hue, clamped, settings.brightness);
            }
            3 => {
                settings.primary_hue = clamped;
                settings.theme = crate::theme::RuntimeTheme::from_hue_with_settings(clamped, settings.color_intensity, settings.brightness);
            }
            _ => {}
        }
    }

    fn slider_track_rect(
        &self,
        slider_idx: usize,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let content_w = ws[0] - SIDEBAR_WIDTH * scale;

        let track_w = SLIDER_WIDTH * scale;
        let track_h = SLIDER_TRACK_H * scale;
        let track_x =
            content_x + content_w - SLIDER_RIGHT_PAD * scale - VALUE_WIDTH * scale - track_w;
        let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + (slider_idx + 1) as f32 * ROW_HEIGHT * scale;
        let track_y = row_y + (ROW_HEIGHT * scale - track_h) * 0.5;
        ([track_x, track_y], [track_w, track_h])
    }

    pub fn slider_hit_test(
        &self,
        mouse: [f32; 2],
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        if settings.theme_preset != "Default" {
            return None;
        }
        for i in 0..SLIDERS.len() {
            let (tp, ts) = self.slider_track_rect(i, screen_w, screen_h, scale);
            let val = Self::slider_value(settings, i);
            let def = &SLIDERS[i];
            let norm = (val - def.min) / (def.max - def.min);
            let thumb_x = tp[0] + norm * ts[0];
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
        &self,
        slider_idx: usize,
        mouse_x: f32,
        settings: &mut Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) {
        let (tp, ts) = self.slider_track_rect(slider_idx, screen_w, screen_h, scale);
        let def = &SLIDERS[slider_idx];
        let norm = ((mouse_x - tp[0]) / ts[0]).clamp(0.0, 1.0);
        let val = def.min + norm * (def.max - def.min);
        Self::set_slider_value(settings, slider_idx, val);
    }

    pub fn category_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let (wp, _ws) = self.win_rect(screen_w, screen_h, scale);
        let sidebar_x = wp[0];
        let sidebar_w = SIDEBAR_WIDTH * scale;
        if pos[0] < sidebar_x || pos[0] > sidebar_x + sidebar_w {
            return None;
        }
        let item_h = ROW_HEIGHT * scale;
        let top = wp[1] + 12.0 * scale;
        for i in 0..CATEGORIES.len() {
            let y = top + i as f32 * item_h;
            if pos[1] >= y && pos[1] < y + item_h {
                return Some(i);
            }
        }
        None
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) {
        self.hovered_category = self.category_at(pos, screen_w, screen_h, scale);
    }

    fn dropdown_rect(
        &self,
        row_idx: usize,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let content_w = ws[0] - SIDEBAR_WIDTH * scale;
        let dd_w = DROPDOWN_WIDTH * scale;
        let dd_h = DROPDOWN_HEIGHT * scale;
        let dd_x = content_x + content_w - DROPDOWN_RIGHT_PAD * scale - dd_w;
        let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + row_idx as f32 * ROW_HEIGHT * scale;
        let dd_y = row_y + (ROW_HEIGHT * scale - dd_h) * 0.5;
        ([dd_x, dd_y], [dd_w, dd_h])
    }

    fn dropdown_options(&self, idx: usize) -> &[String] {
        match idx {
            0 => &self.cached_driver_types,
            1 => &self.cached_input_devices,
            2 => &self.cached_output_devices,
            _ => &[],
        }
    }

    fn dropdown_current<'a>(settings: &'a Settings, idx: usize) -> &'a str {
        match idx {
            0 => &settings.audio_driver_type,
            1 => &settings.audio_input_device,
            2 => &settings.audio_output_device,
            _ => "",
        }
    }

    pub fn set_dropdown_value(settings: &mut Settings, idx: usize, value: String) {
        match idx {
            0 => settings.audio_driver_type = value,
            1 => settings.audio_input_device = value,
            2 => settings.audio_output_device = value,
            _ => {}
        }
    }

    /// Returns which dropdown row (0..3) was clicked, if any.
    pub fn dropdown_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        if self.active_category != SettingsCategory::Audio {
            return None;
        }
        for i in 0..AUDIO_DROPDOWN_COUNT {
            let (dp, ds) = self.dropdown_rect(i, screen_w, screen_h, scale);
            if mouse[0] >= dp[0]
                && mouse[0] <= dp[0] + ds[0]
                && mouse[1] >= dp[1]
                && mouse[1] <= dp[1] + ds[1]
            {
                return Some(i);
            }
        }
        None
    }

    /// When a dropdown popup is open, returns the index of the item under the mouse.
    pub fn dropdown_item_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let dd_idx = self.open_dropdown?;
        let options = self.dropdown_options(dd_idx);
        if options.is_empty() {
            return None;
        }
        let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
        let item_h = DROPDOWN_ITEM_HEIGHT * scale;
        let popup_y = dp[1] + ds[1] + 2.0 * scale;
        let popup_h = options.len() as f32 * item_h;

        if mouse[0] >= dp[0]
            && mouse[0] <= dp[0] + ds[0]
            && mouse[1] >= popup_y
            && mouse[1] <= popup_y + popup_h
        {
            let rel = mouse[1] - popup_y;
            let idx = (rel / item_h) as usize;
            if idx < options.len() {
                return Some(idx);
            }
        }
        None
    }

    /// Handle a click inside the Audio panel. Returns true if the click was consumed.
    pub fn handle_audio_click(
        &mut self,
        mouse: [f32; 2],
        settings: &mut Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if self.active_category != SettingsCategory::Audio {
            return false;
        }

        // Row 3 (Auto Clip Fades) — check popup and button before generic hit tests
        if self.open_dropdown == Some(3) {
            let (dp, ds) = self.dropdown_rect(3, screen_w, screen_h, scale);
            let options = Self::auto_clip_fades_options();
            let item_h = DROPDOWN_ITEM_HEIGHT * scale;
            let popup_y = dp[1] + ds[1] + 2.0 * scale;
            let popup_h = options.len() as f32 * item_h;
            if mouse[0] >= dp[0]
                && mouse[0] <= dp[0] + ds[0]
                && mouse[1] >= popup_y
                && mouse[1] <= popup_y + popup_h
            {
                let rel = mouse[1] - popup_y;
                let idx = (rel / item_h) as usize;
                if idx < options.len() {
                    settings.auto_clip_fades = idx == 0;
                    self.open_dropdown = None;
                    return true;
                }
            }
        }
        {
            let (dp, ds) = self.dropdown_rect(3, screen_w, screen_h, scale);
            if mouse[0] >= dp[0]
                && mouse[0] <= dp[0] + ds[0]
                && mouse[1] >= dp[1]
                && mouse[1] <= dp[1] + ds[1]
            {
                if self.open_dropdown == Some(3) {
                    self.open_dropdown = None;
                } else {
                    self.open_dropdown = Some(3);
                }
                return true;
            }
        }

        // First check if click is on an open dropdown item
        if self.open_dropdown.is_some() {
            if let Some(item_idx) = self.dropdown_item_hit_test(mouse, screen_w, screen_h, scale) {
                let dd_idx = self.open_dropdown.unwrap();
                let options = self.dropdown_options(dd_idx);
                let value = options[item_idx].clone();
                Self::set_dropdown_value(settings, dd_idx, value);
                self.open_dropdown = None;
                return true;
            }
        }

        // Check if click is on a dropdown button
        if let Some(dd_idx) = self.dropdown_hit_test(mouse, screen_w, screen_h, scale) {
            if self.open_dropdown == Some(dd_idx) {
                self.open_dropdown = None;
            } else {
                self.open_dropdown = Some(dd_idx);
            }
            return true;
        }

        // Click elsewhere in the panel closes the dropdown
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            return true;
        }

        false
    }

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
            color: [0.0, 0.0, 0.0, 0.50],
            border_radius: 0.0,
        });

        // Shadow
        let so = 10.0 * scale;
        out.push(InstanceRaw {
            position: [wp[0] + so, wp[1] + so],
            size: [ws[0] + 2.0 * scale, ws[1] + 2.0 * scale],
            color: [0.0, 0.0, 0.0, 0.45],
            border_radius: br,
        });

        // Window background
        out.push(InstanceRaw {
            position: wp,
            size: ws,
            color: t.bg_window,
            border_radius: br,
        });

        // Sidebar background
        out.push(InstanceRaw {
            position: wp,
            size: [SIDEBAR_WIDTH * scale, ws[1]],
            color: t.bg_sidebar,
            border_radius: br,
        });
        // Fill right side of sidebar (cover rounded corner at top-right of sidebar)
        out.push(InstanceRaw {
            position: [wp[0] + SIDEBAR_WIDTH * scale - br, wp[1]],
            size: [br, ws[1]],
            color: t.bg_sidebar,
            border_radius: 0.0,
        });

        // Sidebar divider
        out.push(InstanceRaw {
            position: [wp[0] + SIDEBAR_WIDTH * scale, wp[1] + 8.0 * scale],
            size: [1.0 * scale, ws[1] - 16.0 * scale],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        // Sidebar category items
        let item_h = ROW_HEIGHT * scale;
        let top = wp[1] + 12.0 * scale;
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let y = top + i as f32 * item_h;
            let is_active = *cat == self.active_category;
            let is_hovered = self.hovered_category == Some(i);
            if is_active {
                out.push(InstanceRaw {
                    position: [wp[0] + 6.0 * scale, y],
                    size: [SIDEBAR_WIDTH * scale - 12.0 * scale, item_h],
                    color: t.bg_elevated,
                    border_radius: 6.0 * scale,
                });
            } else if is_hovered {
                out.push(InstanceRaw {
                    position: [wp[0] + 6.0 * scale, y],
                    size: [SIDEBAR_WIDTH * scale - 12.0 * scale, item_h],
                    color: t.item_hover,
                    border_radius: 6.0 * scale,
                });
            }
        }

        // --- Right panel content ---
        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let content_w = ws[0] - SIDEBAR_WIDTH * scale;

        // Section header line
        let header_y = wp[1] + SECTION_HEADER_HEIGHT * scale;
        out.push(InstanceRaw {
            position: [content_x + 16.0 * scale, header_y - 1.0 * scale],
            size: [content_w - 32.0 * scale, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        match self.active_category {
            SettingsCategory::ThemeAndColors => {
                self.build_theme_dropdown_instances(
                    &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                );
                if settings.theme_preset == "Default" {
                    self.build_slider_instances(
                        &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                    );
                }
            }
            SettingsCategory::Audio => {
                self.build_audio_instances(
                    &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                );
            }
            SettingsCategory::Developer => {
                self.build_developer_instances(
                    &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                );
            }
        }

        out
    }

    fn build_slider_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        content_x: f32,
        content_w: f32,
        wp: [f32; 2],
        t: &crate::theme::RuntimeTheme,
    ) {
        for i in 0..SLIDERS.len() {
            let def = &SLIDERS[i];
            let val = Self::slider_value(settings, i);
            let norm = (val - def.min) / (def.max - def.min);

            let (tp, ts) = self.slider_track_rect(i, screen_w, screen_h, scale);

            // For the Primary Color slider (idx 3), draw a color swatch before the track
            if i == 3 {
                let swatch_sz = 14.0 * scale;
                let swatch_x = tp[0] - swatch_sz - 8.0 * scale;
                let swatch_y = tp[1] + ts[1] * 0.5 - swatch_sz * 0.5;
                out.push(InstanceRaw {
                    position: [swatch_x, swatch_y],
                    size: [swatch_sz, swatch_sz],
                    color: t.accent,
                    border_radius: swatch_sz * 0.5,
                });
            }

            out.push(InstanceRaw {
                position: tp,
                size: ts,
                color: t.knob_inactive,
                border_radius: ts[1] * 0.5,
            });

            let fill_w = norm * ts[0];
            if fill_w > 0.5 {
                out.push(InstanceRaw {
                    position: tp,
                    size: [fill_w, ts[1]],
                    color: t.slider_fill,
                    border_radius: ts[1] * 0.5,
                });
            }

            let thumb_r = SLIDER_THUMB_R * scale;
            let thumb_x = tp[0] + fill_w - thumb_r;
            let thumb_cy = tp[1] + ts[1] * 0.5 - thumb_r;
            out.push(InstanceRaw {
                position: [thumb_x, thumb_cy],
                size: [thumb_r * 2.0, thumb_r * 2.0],
                color: [1.0, 1.0, 1.0, 0.95],
                border_radius: thumb_r,
            });

            let row_bottom =
                wp[1] + SECTION_HEADER_HEIGHT * scale + (i as f32 + 2.0) * ROW_HEIGHT * scale;
            if i < SLIDERS.len() - 1 {
                out.push(InstanceRaw {
                    position: [content_x + 16.0 * scale, row_bottom - 0.5 * scale],
                    size: [content_w - 32.0 * scale, 1.0 * scale],
                    color: [1.0, 1.0, 1.0, 0.04],
                    border_radius: 0.0,
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_theme_dropdown_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        content_x: f32,
        content_w: f32,
        wp: [f32; 2],
        t: &crate::theme::RuntimeTheme,
    ) {
        let dd_br = 4.0 * scale;
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);

        // Dropdown border
        out.push(InstanceRaw {
            position: [dp[0] - 1.0, dp[1] - 1.0],
            size: [ds[0] + 2.0, ds[1] + 2.0],
            color: t.bg_window_header,
            border_radius: dd_br + 1.0,
        });

        // Dropdown background
        out.push(InstanceRaw {
            position: dp,
            size: ds,
            color: t.bg_input,
            border_radius: dd_br,
        });

        // Arrow indicator
        let arrow_size = 6.0 * scale;
        let arrow_x = dp[0] + ds[0] - 14.0 * scale;
        let arrow_y = dp[1] + (ds[1] - arrow_size) * 0.5;
        out.push(InstanceRaw {
            position: [arrow_x, arrow_y],
            size: [arrow_size, arrow_size],
            color: [1.0, 1.0, 1.0, 0.3],
            border_radius: arrow_size * 0.5,
        });

        // Row separator below theme row
        let row_bottom = wp[1] + SECTION_HEADER_HEIGHT * scale + ROW_HEIGHT * scale;
        out.push(InstanceRaw {
            position: [content_x + 16.0 * scale, row_bottom - 0.5 * scale],
            size: [content_w - 32.0 * scale, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.04],
            border_radius: 0.0,
        });

        // Popup
        if self.active_category == SettingsCategory::ThemeAndColors {
            if let Some(0) = self.open_dropdown {
                let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                let popup_h = THEME_PRESETS.len() as f32 * item_h;
                let popup_y = dp[1] + ds[1] + 2.0 * scale;
                let popup_br = 6.0 * scale;

                out.push(InstanceRaw {
                    position: [dp[0] + 4.0 * scale, popup_y + 4.0 * scale],
                    size: [ds[0], popup_h],
                    color: [0.0, 0.0, 0.0, 0.5],
                    border_radius: popup_br,
                });
                out.push(InstanceRaw {
                    position: [dp[0] - 1.0, popup_y - 1.0],
                    size: [ds[0] + 2.0, popup_h + 2.0],
                    color: t.bg_window_header,
                    border_radius: popup_br + 1.0,
                });
                out.push(InstanceRaw {
                    position: [dp[0], popup_y],
                    size: [ds[0], popup_h],
                    color: t.bg_menu,
                    border_radius: popup_br,
                });

                for (j, preset) in THEME_PRESETS.iter().enumerate() {
                    let iy = popup_y + j as f32 * item_h;
                    if *preset == settings.theme_preset {
                        out.push(InstanceRaw {
                            position: [dp[0] + 4.0 * scale, iy + 2.0 * scale],
                            size: [ds[0] - 8.0 * scale, item_h - 4.0 * scale],
                            color: t.option_highlight,
                            border_radius: 4.0 * scale,
                        });
                    }
                }
            }
        }
    }

    pub fn handle_theme_panel_click(
        &mut self,
        mouse: [f32; 2],
        settings: &mut Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if self.active_category != SettingsCategory::ThemeAndColors {
            return false;
        }

        // Check if popup is open and click is on an item
        if self.open_dropdown == Some(0) {
            let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
            let item_h = DROPDOWN_ITEM_HEIGHT * scale;
            let popup_y = dp[1] + ds[1] + 2.0 * scale;
            let popup_h = THEME_PRESETS.len() as f32 * item_h;

            if mouse[0] >= dp[0]
                && mouse[0] <= dp[0] + ds[0]
                && mouse[1] >= popup_y
                && mouse[1] <= popup_y + popup_h
            {
                let rel = mouse[1] - popup_y;
                let idx = (rel / item_h) as usize;
                if idx < THEME_PRESETS.len() {
                    let chosen = THEME_PRESETS[idx];
                    settings.theme_preset = chosen.to_string();
                    settings.theme = if chosen == "Ableton" {
                        crate::theme::RuntimeTheme::from_preset_ableton()
                    } else {
                        crate::theme::RuntimeTheme::from_hue_with_settings(
                            settings.primary_hue,
                            settings.color_intensity,
                            settings.brightness,
                        )
                    };
                    self.open_dropdown = None;
                    return true;
                }
            }
        }

        // Check if click is on the dropdown button
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
        if mouse[0] >= dp[0]
            && mouse[0] <= dp[0] + ds[0]
            && mouse[1] >= dp[1]
            && mouse[1] <= dp[1] + ds[1]
        {
            if self.open_dropdown == Some(0) {
                self.open_dropdown = None;
            } else {
                self.open_dropdown = Some(0);
            }
            return true;
        }

        // Click elsewhere closes the dropdown
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            return true;
        }

        false
    }

    #[allow(clippy::too_many_arguments)]
    fn build_audio_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        content_x: f32,
        content_w: f32,
        wp: [f32; 2],
        t: &crate::theme::RuntimeTheme,
    ) {
        let dd_br = 4.0 * scale;

        for i in 0..AUDIO_DROPDOWN_COUNT {
            let (dp, ds) = self.dropdown_rect(i, screen_w, screen_h, scale);

            // Dropdown border (slightly larger rect behind)
            out.push(InstanceRaw {
                position: [dp[0] - 1.0, dp[1] - 1.0],
                size: [ds[0] + 2.0, ds[1] + 2.0],
                color: t.bg_window_header,
                border_radius: dd_br + 1.0,
            });

            // Dropdown background (fully opaque)
            out.push(InstanceRaw {
                position: dp,
                size: ds,
                color: t.bg_input,
                border_radius: dd_br,
            });

            // Small arrow indicator on the right side of dropdown
            let arrow_size = 6.0 * scale;
            let arrow_x = dp[0] + ds[0] - 14.0 * scale;
            let arrow_y = dp[1] + (ds[1] - arrow_size) * 0.5;
            out.push(InstanceRaw {
                position: [arrow_x, arrow_y],
                size: [arrow_size, arrow_size],
                color: [1.0, 1.0, 1.0, 0.3],
                border_radius: arrow_size * 0.5,
            });

            // Row separator
            let row_bottom =
                wp[1] + SECTION_HEADER_HEIGHT * scale + (i as f32 + 1.0) * ROW_HEIGHT * scale;
            if i < AUDIO_DROPDOWN_COUNT - 1 {
                out.push(InstanceRaw {
                    position: [content_x + 16.0 * scale, row_bottom - 0.5 * scale],
                    size: [content_w - 32.0 * scale, 1.0 * scale],
                    color: [1.0, 1.0, 1.0, 0.04],
                    border_radius: 0.0,
                });
            }
        }

        // Row 3: Auto Clip Fades dropdown
        {
            let (dp, ds) = self.dropdown_rect(3, screen_w, screen_h, scale);

            // Separator above row 3
            let row_top =
                wp[1] + SECTION_HEADER_HEIGHT * scale + 3.0 * ROW_HEIGHT * scale;
            out.push(InstanceRaw {
                position: [content_x + 16.0 * scale, row_top - 0.5 * scale],
                size: [content_w - 32.0 * scale, 1.0 * scale],
                color: [1.0, 1.0, 1.0, 0.04],
                border_radius: 0.0,
            });

            // Dropdown border
            out.push(InstanceRaw {
                position: [dp[0] - 1.0, dp[1] - 1.0],
                size: [ds[0] + 2.0, ds[1] + 2.0],
                color: t.bg_window_header,
                border_radius: dd_br + 1.0,
            });

            // Dropdown background
            out.push(InstanceRaw {
                position: dp,
                size: ds,
                color: t.bg_input,
                border_radius: dd_br,
            });

            // Arrow indicator
            let arrow_size = 6.0 * scale;
            let arrow_x = dp[0] + ds[0] - 14.0 * scale;
            let arrow_y = dp[1] + (ds[1] - arrow_size) * 0.5;
            out.push(InstanceRaw {
                position: [arrow_x, arrow_y],
                size: [arrow_size, arrow_size],
                color: [1.0, 1.0, 1.0, 0.3],
                border_radius: arrow_size * 0.5,
            });
        }

        // Open dropdown popup
        if let Some(dd_idx) = self.open_dropdown {
            let options = self.dropdown_options(dd_idx);
            if !options.is_empty() {
                let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
                let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                let popup_h = options.len() as f32 * item_h;
                let popup_y = dp[1] + ds[1] + 2.0 * scale;
                let popup_br = 6.0 * scale;

                // Popup shadow
                out.push(InstanceRaw {
                    position: [dp[0] + 4.0 * scale, popup_y + 4.0 * scale],
                    size: [ds[0], popup_h],
                    color: [0.0, 0.0, 0.0, 0.5],
                    border_radius: popup_br,
                });

                // Popup border (slightly larger rect behind)
                out.push(InstanceRaw {
                    position: [dp[0] - 1.0, popup_y - 1.0],
                    size: [ds[0] + 2.0, popup_h + 2.0],
                    color: t.bg_window_header,
                    border_radius: popup_br + 1.0,
                });

                // Popup background (fully opaque)
                out.push(InstanceRaw {
                    position: [dp[0], popup_y],
                    size: [ds[0], popup_h],
                    color: t.bg_menu,
                    border_radius: popup_br,
                });

                let current = Self::dropdown_current(settings, dd_idx);
                for (j, opt) in options.iter().enumerate() {
                    let iy = popup_y + j as f32 * item_h;
                    if opt == current {
                        out.push(InstanceRaw {
                            position: [dp[0] + 4.0 * scale, iy + 2.0 * scale],
                            size: [ds[0] - 8.0 * scale, item_h - 4.0 * scale],
                            color: t.option_highlight,
                            border_radius: 4.0 * scale,
                        });
                    }
                }
            } else if dd_idx == 3 {
                // Auto Clip Fades popup
                let (dp3, ds3) = self.dropdown_rect(3, screen_w, screen_h, scale);
                let options = Self::auto_clip_fades_options();
                let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                let popup_h = options.len() as f32 * item_h;
                let popup_y = dp3[1] + ds3[1] + 2.0 * scale;
                let popup_br = 6.0 * scale;

                out.push(InstanceRaw {
                    position: [dp3[0] + 4.0 * scale, popup_y + 4.0 * scale],
                    size: [ds3[0], popup_h],
                    color: [0.0, 0.0, 0.0, 0.5],
                    border_radius: popup_br,
                });
                out.push(InstanceRaw {
                    position: [dp3[0] - 1.0, popup_y - 1.0],
                    size: [ds3[0] + 2.0, popup_h + 2.0],
                    color: t.bg_window_header,
                    border_radius: popup_br + 1.0,
                });
                out.push(InstanceRaw {
                    position: [dp3[0], popup_y],
                    size: [ds3[0], popup_h],
                    color: t.bg_menu,
                    border_radius: popup_br,
                });

                let current_idx: usize = if settings.auto_clip_fades { 0 } else { 1 };
                for (j, _opt) in options.iter().enumerate() {
                    let iy = popup_y + j as f32 * item_h;
                    if j == current_idx {
                        out.push(InstanceRaw {
                            position: [dp3[0] + 4.0 * scale, iy + 2.0 * scale],
                            size: [ds3[0] - 8.0 * scale, item_h - 4.0 * scale],
                            color: t.option_highlight,
                            border_radius: 4.0 * scale,
                        });
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_developer_instances(
        &self,
        out: &mut Vec<InstanceRaw>,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
        _content_x: f32,
        _content_w: f32,
        _wp: [f32; 2],
        t: &crate::theme::RuntimeTheme,
    ) {
        let dd_br = 4.0 * scale;
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);

        // Dropdown border
        out.push(InstanceRaw {
            position: [dp[0] - 1.0, dp[1] - 1.0],
            size: [ds[0] + 2.0, ds[1] + 2.0],
            color: t.bg_window_header,
            border_radius: dd_br + 1.0,
        });

        // Dropdown background
        out.push(InstanceRaw {
            position: dp,
            size: ds,
            color: t.bg_input,
            border_radius: dd_br,
        });

        // Arrow indicator
        let arrow_size = 6.0 * scale;
        let arrow_x = dp[0] + ds[0] - 14.0 * scale;
        let arrow_y = dp[1] + (ds[1] - arrow_size) * 0.5;
        out.push(InstanceRaw {
            position: [arrow_x, arrow_y],
            size: [arrow_size, arrow_size],
            color: [1.0, 1.0, 1.0, 0.3],
            border_radius: arrow_size * 0.5,
        });

        // Open dropdown popup
        if let Some(0) = self.open_dropdown {
            let options = Self::dev_mode_options();
            let item_h = DROPDOWN_ITEM_HEIGHT * scale;
            let popup_h = options.len() as f32 * item_h;
            let popup_y = dp[1] + ds[1] + 2.0 * scale;
            let popup_br = 6.0 * scale;

            // Popup shadow
            out.push(InstanceRaw {
                position: [dp[0] + 4.0 * scale, popup_y + 4.0 * scale],
                size: [ds[0], popup_h],
                color: [0.0, 0.0, 0.0, 0.5],
                border_radius: popup_br,
            });

            // Popup border
            out.push(InstanceRaw {
                position: [dp[0] - 1.0, popup_y - 1.0],
                size: [ds[0] + 2.0, popup_h + 2.0],
                color: t.bg_window_header,
                border_radius: popup_br + 1.0,
            });

            // Popup background
            out.push(InstanceRaw {
                position: [dp[0], popup_y],
                size: [ds[0], popup_h],
                color: t.bg_menu,
                border_radius: popup_br,
            });

            let current_idx: usize = if settings.dev_mode { 1 } else { 0 };
            for (j, _opt) in options.iter().enumerate() {
                let iy = popup_y + j as f32 * item_h;
                if j == current_idx {
                    out.push(InstanceRaw {
                        position: [dp[0] + 4.0 * scale, iy + 2.0 * scale],
                        size: [ds[0] - 8.0 * scale, item_h - 4.0 * scale],
                        color: t.option_highlight,
                        border_radius: 4.0 * scale,
                    });
                }
            }
        }
    }

    fn dev_mode_options() -> &'static [&'static str] {
        &["Production", "Development"]
    }

    fn auto_clip_fades_options() -> &'static [&'static str] {
        &["On", "Off"]
    }

    pub fn handle_developer_click(
        &mut self,
        mouse: [f32; 2],
        settings: &mut Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if self.active_category != SettingsCategory::Developer {
            return false;
        }

        // Check if click is on open dropdown item
        if self.open_dropdown == Some(0) {
            let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
            let options = Self::dev_mode_options();
            let item_h = DROPDOWN_ITEM_HEIGHT * scale;
            let popup_y = dp[1] + ds[1] + 2.0 * scale;
            let popup_h = options.len() as f32 * item_h;

            if mouse[0] >= dp[0]
                && mouse[0] <= dp[0] + ds[0]
                && mouse[1] >= popup_y
                && mouse[1] <= popup_y + popup_h
            {
                let rel = mouse[1] - popup_y;
                let idx = (rel / item_h) as usize;
                if idx < options.len() {
                    settings.dev_mode = idx == 1;
                    self.open_dropdown = None;
                    return true;
                }
            }
        }

        // Check if click is on dropdown button
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
        if mouse[0] >= dp[0]
            && mouse[0] <= dp[0] + ds[0]
            && mouse[1] >= dp[1]
            && mouse[1] <= dp[1] + ds[1]
        {
            if self.open_dropdown == Some(0) {
                self.open_dropdown = None;
            } else {
                self.open_dropdown = Some(0);
            }
            return true;
        }

        // Click elsewhere closes dropdown
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            return true;
        }

        false
    }
}

// ---------------------------------------------------------------------------
// Text entries (for glyphon rendering in main.rs)
// ---------------------------------------------------------------------------

pub struct SettingsTextEntry {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub color: [u8; 4],
    pub weight: u16,
}

impl SettingsWindow {
    pub fn get_text_entries(
        &self,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<SettingsTextEntry> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);

        // Window title
        let title_font = 13.0 * scale;
        let title_line = 18.0 * scale;
        out.push(SettingsTextEntry {
            text: "Settings".to_string(),
            x: wp[0] + ws[0] * 0.5 - 24.0 * scale,
            y: wp[1] - title_line - 6.0 * scale,
            font_size: title_font,
            line_height: title_line,
            color: [210, 210, 218, 255],
            weight: 600,
        });

        // Sidebar categories
        let item_h = ROW_HEIGHT * scale;
        let top = wp[1] + 12.0 * scale;
        let cat_font = 13.0 * scale;
        let cat_line = 18.0 * scale;
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let y = top + i as f32 * item_h + (item_h - cat_line) * 0.5;
            let is_active = *cat == self.active_category;
            let color = if is_active {
                [240, 240, 245, 255]
            } else {
                [170, 170, 180, 255]
            };
            out.push(SettingsTextEntry {
                text: cat.label().to_string(),
                x: wp[0] + 18.0 * scale,
                y,
                font_size: cat_font,
                line_height: cat_line,
                color,
                weight: if is_active { 600 } else { 400 },
            });
        }

        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let section_font = 11.0 * scale;
        let section_line = 15.0 * scale;

        match self.active_category {
            SettingsCategory::ThemeAndColors => {
                out.push(SettingsTextEntry {
                    text: "Customization".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: [140, 140, 150, 200],
                    weight: 600,
                });

                let label_font = 13.0 * scale;
                let label_line = 18.0 * scale;
                let dd_font = 12.0 * scale;
                let dd_line = 16.0 * scale;
                let value_font = 12.0 * scale;
                let value_line = 16.0 * scale;
                let content_w = ws[0] - SIDEBAR_WIDTH * scale;

                // Row 0: Theme preset dropdown
                let theme_row_y = wp[1] + SECTION_HEADER_HEIGHT * scale;
                out.push(SettingsTextEntry {
                    text: "Theme".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: theme_row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                    font_size: label_font,
                    line_height: label_line,
                    color: [210, 210, 218, 255],
                    weight: 400,
                });
                let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
                out.push(SettingsTextEntry {
                    text: settings.theme_preset.clone(),
                    x: dp[0] + 10.0 * scale,
                    y: dp[1] + (ds[1] - dd_line) * 0.5,
                    font_size: dd_font,
                    line_height: dd_line,
                    color: [210, 210, 218, 255],
                    weight: 400,
                });

                // Theme preset popup text
                if let Some(0) = self.open_dropdown {
                    let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                    let popup_y = dp[1] + ds[1] + 2.0 * scale;
                    for (j, preset) in THEME_PRESETS.iter().enumerate() {
                        let iy = popup_y + j as f32 * item_h;
                        let is_selected = *preset == settings.theme_preset;
                        out.push(SettingsTextEntry {
                            text: preset.to_string(),
                            x: dp[0] + 12.0 * scale,
                            y: iy + (item_h - dd_line) * 0.5,
                            font_size: dd_font,
                            line_height: dd_line,
                            color: if is_selected { [240, 240, 255, 255] } else { [200, 200, 210, 255] },
                            weight: if is_selected { 600 } else { 400 },
                        });
                    }
                }

                // Rows 1..N: sliders (only when Default preset)
                if settings.theme_preset == "Default" {
                    for (i, def) in SLIDERS.iter().enumerate() {
                        let row_y = wp[1]
                            + SECTION_HEADER_HEIGHT * scale
                            + (i + 1) as f32 * ROW_HEIGHT * scale;

                        out.push(SettingsTextEntry {
                            text: def.label.to_string(),
                            x: content_x + ROW_LABEL_X * scale,
                            y: row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                            font_size: label_font,
                            line_height: label_line,
                            color: [210, 210, 218, 255],
                            weight: 400,
                        });

                        let val = Self::slider_value(settings, i);
                        let display = (val * def.display_scale) as i32;
                        let val_text = format!("{} {}", display, def.unit);
                        let val_x = content_x + content_w - SLIDER_RIGHT_PAD * scale
                            - VALUE_WIDTH * scale
                            + 8.0 * scale;
                        out.push(SettingsTextEntry {
                            text: val_text,
                            x: val_x,
                            y: row_y + (ROW_HEIGHT * scale - value_line) * 0.5,
                            font_size: value_font,
                            line_height: value_line,
                            color: [170, 170, 180, 255],
                            weight: 400,
                        });
                    }
                }
            }
            SettingsCategory::Audio => {
                out.push(SettingsTextEntry {
                    text: "Audio Device".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: [140, 140, 150, 200],
                    weight: 600,
                });

                let label_font = 13.0 * scale;
                let label_line = 18.0 * scale;
                let dd_font = 12.0 * scale;
                let dd_line = 16.0 * scale;
                let labels = ["Driver Type", "Audio Input Device", "Audio Output Device"];

                for i in 0..AUDIO_DROPDOWN_COUNT {
                    let row_y =
                        wp[1] + SECTION_HEADER_HEIGHT * scale + i as f32 * ROW_HEIGHT * scale;

                    out.push(SettingsTextEntry {
                        text: labels[i].to_string(),
                        x: content_x + ROW_LABEL_X * scale,
                        y: row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                        font_size: label_font,
                        line_height: label_line,
                        color: [210, 210, 218, 255],
                        weight: 400,
                    });

                    let current = Self::dropdown_current(settings, i);
                    let (dp, ds) = self.dropdown_rect(i, screen_w, screen_h, scale);
                    out.push(SettingsTextEntry {
                        text: current.to_string(),
                        x: dp[0] + 10.0 * scale,
                        y: dp[1] + (ds[1] - dd_line) * 0.5,
                        font_size: dd_font,
                        line_height: dd_line,
                        color: [210, 210, 218, 255],
                        weight: 400,
                    });
                }

                // Row 3: Auto Clip Fades
                {
                    let row_y = wp[1]
                        + SECTION_HEADER_HEIGHT * scale
                        + 3.0 * ROW_HEIGHT * scale;
                    out.push(SettingsTextEntry {
                        text: "Auto Clip Fades".to_string(),
                        x: content_x + ROW_LABEL_X * scale,
                        y: row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                        font_size: label_font,
                        line_height: label_line,
                        color: [210, 210, 218, 255],
                        weight: 400,
                    });

                    let current_text = if settings.auto_clip_fades { "On" } else { "Off" };
                    let (dp3, ds3) = self.dropdown_rect(3, screen_w, screen_h, scale);
                    out.push(SettingsTextEntry {
                        text: current_text.to_string(),
                        x: dp3[0] + 10.0 * scale,
                        y: dp3[1] + (ds3[1] - dd_line) * 0.5,
                        font_size: dd_font,
                        line_height: dd_line,
                        color: [210, 210, 218, 255],
                        weight: 400,
                    });
                }

                // Popup item text
                if let Some(dd_idx) = self.open_dropdown {
                    let options = self.dropdown_options(dd_idx);
                    if !options.is_empty() {
                        let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
                        let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                        let popup_y = dp[1] + ds[1] + 2.0 * scale;
                        let current = Self::dropdown_current(settings, dd_idx);

                        for (j, opt) in options.iter().enumerate() {
                            let iy = popup_y + j as f32 * item_h;
                            let is_selected = opt == current;
                            out.push(SettingsTextEntry {
                                text: opt.clone(),
                                x: dp[0] + 12.0 * scale,
                                y: iy + (item_h - dd_line) * 0.5,
                                font_size: dd_font,
                                line_height: dd_line,
                                color: if is_selected {
                                    [240, 240, 255, 255]
                                } else {
                                    [200, 200, 210, 255]
                                },
                                weight: if is_selected { 600 } else { 400 },
                            });
                        }
                    } else if dd_idx == 3 {
                        // Auto Clip Fades popup text
                        let options = Self::auto_clip_fades_options();
                        let (dp, ds) = self.dropdown_rect(3, screen_w, screen_h, scale);
                        let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                        let popup_y = dp[1] + ds[1] + 2.0 * scale;
                        let current_idx: usize = if settings.auto_clip_fades { 0 } else { 1 };

                        for (j, opt) in options.iter().enumerate() {
                            let iy = popup_y + j as f32 * item_h;
                            let is_selected = j == current_idx;
                            out.push(SettingsTextEntry {
                                text: opt.to_string(),
                                x: dp[0] + 12.0 * scale,
                                y: iy + (item_h - dd_line) * 0.5,
                                font_size: dd_font,
                                line_height: dd_line,
                                color: if is_selected {
                                    [240, 240, 255, 255]
                                } else {
                                    [200, 200, 210, 255]
                                },
                                weight: if is_selected { 600 } else { 400 },
                            });
                        }
                    }
                }
            }
            SettingsCategory::Developer => {
                out.push(SettingsTextEntry {
                    text: "Developer".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: [140, 140, 150, 200],
                    weight: 600,
                });

                let label_font = 13.0 * scale;
                let label_line = 18.0 * scale;
                let dd_font = 12.0 * scale;
                let dd_line = 16.0 * scale;

                let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale;
                out.push(SettingsTextEntry {
                    text: "Mode".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                    font_size: label_font,
                    line_height: label_line,
                    color: [210, 210, 218, 255],
                    weight: 400,
                });

                let current_text = if settings.dev_mode { "Development" } else { "Production" };
                let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
                out.push(SettingsTextEntry {
                    text: current_text.to_string(),
                    x: dp[0] + 10.0 * scale,
                    y: dp[1] + (ds[1] - dd_line) * 0.5,
                    font_size: dd_font,
                    line_height: dd_line,
                    color: [210, 210, 218, 255],
                    weight: 400,
                });

                // Build version
                let build_row_y = row_y + ROW_HEIGHT * scale;
                out.push(SettingsTextEntry {
                    text: "Build".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: build_row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
                    font_size: label_font,
                    line_height: label_line,
                    color: [210, 210, 218, 255],
                    weight: 400,
                });
                let build_version = std::fs::read_to_string("build_version")
                    .unwrap_or_else(|_| "0".to_string());
                let build_version = build_version.trim();
                out.push(SettingsTextEntry {
                    text: format!("#{}", build_version),
                    x: dp[0] + 10.0 * scale,
                    y: build_row_y + (ROW_HEIGHT * scale - dd_line) * 0.5,
                    font_size: dd_font,
                    line_height: dd_line,
                    color: [140, 140, 150, 200],
                    weight: 400,
                });

                // Popup item text
                if let Some(0) = self.open_dropdown {
                    let options = Self::dev_mode_options();
                    let item_h = DROPDOWN_ITEM_HEIGHT * scale;
                    let popup_y = dp[1] + ds[1] + 2.0 * scale;
                    let current_idx: usize = if settings.dev_mode { 1 } else { 0 };

                    for (j, opt) in options.iter().enumerate() {
                        let iy = popup_y + j as f32 * item_h;
                        let is_selected = j == current_idx;
                        out.push(SettingsTextEntry {
                            text: opt.to_string(),
                            x: dp[0] + 12.0 * scale,
                            y: iy + (item_h - dd_line) * 0.5,
                            font_size: dd_font,
                            line_height: dd_line,
                            color: if is_selected {
                                [240, 240, 255, 255]
                            } else {
                                [200, 200, 210, 255]
                            },
                            weight: if is_selected { 600 } else { 400 },
                        });
                    }
                }
            }
        }

        out
    }
}
