use crate::settings::{Settings, BUFFER_SIZE_OPTIONS};
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
const AUDIO_DROPDOWN_COUNT: usize = 3;
const THEME_PRESETS: &[&str] = &["Default", "Ableton", "Light"];

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
    pub hovered_dropdown_item: Option<usize>,
    pub dragging_slider: Option<usize>,
    pub open_dropdown: Option<usize>,
    pub cached_driver_types: Vec<String>,
    pub cached_input_devices: Vec<String>,
    pub cached_output_devices: Vec<String>,
    pub cached_buffer_sizes: Vec<String>,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            active_category: SettingsCategory::ThemeAndColors,
            hovered_category: None,
            hovered_dropdown_item: None,
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
            cached_buffer_sizes: BUFFER_SIZE_OPTIONS.iter().map(|s| s.to_string()).collect(),
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

    /// Returns the popup rect ([pos], [size]) when a dropdown is open, or None.
    pub fn open_dropdown_popup_rect(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<([f32; 2], [f32; 2])> {
        let dd_idx = self.open_dropdown?;
        let count = self.dropdown_option_count(dd_idx);
        if count == 0 {
            return None;
        }
        let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
        Some(super::dropdown::popup_rect(dp, ds, count, scale))
    }

    // -----------------------------------------------------------------------
    // Dropdown helpers (shared across all categories)
    // -----------------------------------------------------------------------

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

    /// Returns the number of options for the given dropdown index.
    fn dropdown_option_count(&self, dd_idx: usize) -> usize {
        if self.active_category == SettingsCategory::ThemeAndColors && dd_idx == 0 {
            THEME_PRESETS.len()
        } else if self.active_category == SettingsCategory::Developer && dd_idx == 0 {
            Self::dev_mode_options().len()
        } else if dd_idx == 3 {
            Self::auto_clip_fades_options().len()
        } else {
            self.dropdown_options(dd_idx).len()
        }
    }

    fn dropdown_options(&self, idx: usize) -> &[String] {
        match idx {
            0 => &self.cached_driver_types,
            1 => &self.cached_input_devices,
            2 => &self.cached_output_devices,
            4 => &self.cached_buffer_sizes,
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
            4 => { settings.buffer_size = value.parse().unwrap_or(512); }
            _ => {}
        }
    }

    fn dev_mode_options() -> &'static [&'static str] {
        &["Production", "Development"]
    }

    fn auto_clip_fades_options() -> &'static [&'static str] {
        &["On", "Off"]
    }

    /// Render a row separator line.
    fn render_row_separator(
        out: &mut Vec<InstanceRaw>,
        content_x: f32,
        content_w: f32,
        y: f32,
        scale: f32,
        t: &crate::theme::RuntimeTheme,
    ) {
        out.push(InstanceRaw {
            position: [content_x + 16.0 * scale, y - 0.5 * scale],
            size: [content_w - 32.0 * scale, 1.0 * scale],
            color: crate::theme::with_alpha(t.divider, t.divider[3] * 0.67),
            border_radius: 0.0,
        });
    }

    // -----------------------------------------------------------------------
    // Hit testing (delegates to shared dropdown component)
    // -----------------------------------------------------------------------

    fn dropdown_button_hit_test(
        &self,
        mouse: [f32; 2],
        row_idx: usize,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        let (dp, ds) = self.dropdown_rect(row_idx, screen_w, screen_h, scale);
        super::dropdown::hit_test_button(mouse, dp, ds)
    }

    pub fn dropdown_item_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let dd_idx = self.open_dropdown?;
        let count = self.dropdown_option_count(dd_idx);
        let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
        super::dropdown::hit_test_popup_item(mouse, dp, ds, count, scale)
    }

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
            if self.dropdown_button_hit_test(mouse, i, screen_w, screen_h, scale) {
                return Some(i);
            }
        }
        None
    }

    fn handle_dropdown_click(
        &mut self,
        mouse: [f32; 2],
        dd_idx: usize,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
        let count = self.dropdown_option_count(dd_idx);
        let is_open = self.open_dropdown == Some(dd_idx);
        match super::dropdown::handle_click(mouse, dp, ds, count, scale, is_open) {
            super::dropdown::ClickResult::Selected(idx) => {
                self.open_dropdown = None;
                Some(idx)
            }
            super::dropdown::ClickResult::Toggled(new_open) => {
                self.open_dropdown = if new_open { Some(dd_idx) } else { None };
                None
            }
            super::dropdown::ClickResult::None => None,
        }
    }

    // -----------------------------------------------------------------------
    // Hover
    // -----------------------------------------------------------------------

    pub fn update_hover(&mut self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) {
        self.hovered_category = self.category_at(pos, screen_w, screen_h, scale);
        self.hovered_dropdown_item = self.dropdown_item_hit_test(pos, screen_w, screen_h, scale);
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

    // -----------------------------------------------------------------------
    // Sliders
    // -----------------------------------------------------------------------

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

    fn reset_theme_button_rect(
        &self,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);
        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let content_w = ws[0] - SIDEBAR_WIDTH * scale;
        let btn_w = 120.0 * scale;
        let btn_h = DROPDOWN_HEIGHT * scale;
        let btn_x = content_x + content_w - DROPDOWN_RIGHT_PAD * scale - btn_w;
        let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + 2.0 * ROW_HEIGHT * scale;
        let btn_y = row_y + (ROW_HEIGHT * scale - btn_h) * 0.5;
        ([btn_x, btn_y], [btn_w, btn_h])
    }

    // -----------------------------------------------------------------------
    // Click handlers (per category)
    // -----------------------------------------------------------------------

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

        if let Some(idx) = self.handle_dropdown_click(mouse, 0, screen_w, screen_h, scale) {
            if idx < THEME_PRESETS.len() {
                let chosen = THEME_PRESETS[idx];
                settings.theme_preset = chosen.to_string();
                settings.theme = match chosen {
                    "Ableton" => crate::theme::RuntimeTheme::from_preset_ableton(),
                    "Light"   => crate::theme::RuntimeTheme::from_preset_light(settings.primary_hue),
                    _         => crate::theme::RuntimeTheme::from_hue_with_settings(
                        settings.primary_hue,
                        settings.color_intensity,
                        settings.brightness,
                    ),
                };
            }
            return true;
        }

        // If button was toggled, handle_dropdown_click already set state
        if self.open_dropdown == Some(0) || self.dropdown_button_hit_test(mouse, 0, screen_w, screen_h, scale) {
            return true;
        }

        // Click elsewhere closes the dropdown
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            return true;
        }

        false
    }

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

        // Auto Clip Fades (row 3)
        if let Some(idx) = self.handle_dropdown_click(mouse, 3, screen_w, screen_h, scale) {
            settings.auto_clip_fades = idx == 0;
            return true;
        }
        if self.dropdown_button_hit_test(mouse, 3, screen_w, screen_h, scale) {
            return true;
        }

        // Buffer Size (row 4)
        if let Some(idx) = self.handle_dropdown_click(mouse, 4, screen_w, screen_h, scale) {
            let options = self.dropdown_options(4);
            if idx < options.len() {
                Self::set_dropdown_value(settings, 4, options[idx].clone());
            }
            return true;
        }
        if self.dropdown_button_hit_test(mouse, 4, screen_w, screen_h, scale) {
            return true;
        }

        // Generic audio dropdowns (rows 0-2)
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

        if let Some(dd_idx) = self.dropdown_hit_test(mouse, screen_w, screen_h, scale) {
            if self.open_dropdown == Some(dd_idx) {
                self.open_dropdown = None;
            } else {
                self.open_dropdown = Some(dd_idx);
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

        if let Some(idx) = self.handle_dropdown_click(mouse, 0, screen_w, screen_h, scale) {
            settings.dev_mode = idx == 1;
            return true;
        }
        if self.dropdown_button_hit_test(mouse, 0, screen_w, screen_h, scale) {
            return true;
        }

        // Reset Theme button
        let (btn_pos, btn_size) = self.reset_theme_button_rect(screen_w, screen_h, scale);
        if mouse[0] >= btn_pos[0]
            && mouse[0] <= btn_pos[0] + btn_size[0]
            && mouse[1] >= btn_pos[1]
            && mouse[1] <= btn_pos[1] + btn_size[1]
        {
            self.open_dropdown = None;
            settings.reset_theme_to_defaults();
            return true;
        }

        // Click elsewhere closes dropdown
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            return true;
        }

        false
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
            color: t.divider,
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
            color: t.divider,
            border_radius: 0.0,
        });

        match self.active_category {
            SettingsCategory::ThemeAndColors => {
                // Sliders first so dropdown popup renders on top
                if settings.theme_preset == "Default" {
                    self.build_slider_instances(
                        &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                    );
                }
                self.build_theme_dropdown_instances(
                    &mut out, settings, screen_w, screen_h, scale, content_x, content_w, wp, t,
                );
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
                color: crate::theme::with_alpha(t.text_primary, 0.95),
                border_radius: thumb_r,
            });

            let row_bottom =
                wp[1] + SECTION_HEADER_HEIGHT * scale + (i as f32 + 2.0) * ROW_HEIGHT * scale;
            if i < SLIDERS.len() - 1 {
                Self::render_row_separator(out, content_x, content_w, row_bottom, scale, t);
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
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
        super::dropdown::render_button(out, dp, ds, scale, t);

        // Row separator below theme row
        let row_bottom = wp[1] + SECTION_HEADER_HEIGHT * scale + ROW_HEIGHT * scale;
        Self::render_row_separator(out, content_x, content_w, row_bottom, scale, t);

        // Popup
        if self.active_category == SettingsCategory::ThemeAndColors {
            if let Some(0) = self.open_dropdown {
                let selected = THEME_PRESETS.iter().position(|p| *p == settings.theme_preset).unwrap_or(0);
                super::dropdown::render_popup(out, dp, ds, THEME_PRESETS.len(), selected, self.hovered_dropdown_item, scale, t);
            }
        }
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
        // Audio dropdowns (rows 0-2)
        for i in 0..AUDIO_DROPDOWN_COUNT {
            let (dp, ds) = self.dropdown_rect(i, screen_w, screen_h, scale);
            super::dropdown::render_button(out, dp, ds, scale, t);

            if i < AUDIO_DROPDOWN_COUNT - 1 {
                let row_bottom =
                    wp[1] + SECTION_HEADER_HEIGHT * scale + (i as f32 + 1.0) * ROW_HEIGHT * scale;
                Self::render_row_separator(out, content_x, content_w, row_bottom, scale, t);
            }
        }

        // Row 3: Auto Clip Fades
        {
            let (dp, ds) = self.dropdown_rect(3, screen_w, screen_h, scale);
            let row_top = wp[1] + SECTION_HEADER_HEIGHT * scale + 3.0 * ROW_HEIGHT * scale;
            Self::render_row_separator(out, content_x, content_w, row_top, scale, t);
            super::dropdown::render_button(out, dp, ds, scale, t);
        }

        // Row 4: Buffer Size
        {
            let (dp, ds) = self.dropdown_rect(4, screen_w, screen_h, scale);
            let row_top = wp[1] + SECTION_HEADER_HEIGHT * scale + 4.0 * ROW_HEIGHT * scale;
            Self::render_row_separator(out, content_x, content_w, row_top, scale, t);
            super::dropdown::render_button(out, dp, ds, scale, t);
        }

        // Open dropdown popup
        if let Some(dd_idx) = self.open_dropdown {
            let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
            if dd_idx == 3 {
                let options = Self::auto_clip_fades_options();
                let selected = if settings.auto_clip_fades { 0 } else { 1 };
                super::dropdown::render_popup(out, dp, ds, options.len(), selected, self.hovered_dropdown_item, scale, t);
            } else {
                let options = self.dropdown_options(dd_idx);
                if !options.is_empty() {
                    let current_buf;
                    let current = if dd_idx == 4 {
                        current_buf = settings.buffer_size.to_string();
                        current_buf.as_str()
                    } else {
                        Self::dropdown_current(settings, dd_idx)
                    };
                    let selected = options.iter().position(|o| o == current).unwrap_or(0);
                    super::dropdown::render_popup(out, dp, ds, options.len(), selected, self.hovered_dropdown_item, scale, t);
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
        let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
        super::dropdown::render_button(out, dp, ds, scale, t);

        // Popup
        if let Some(0) = self.open_dropdown {
            let selected = if settings.dev_mode { 1 } else { 0 };
            super::dropdown::render_popup(out, dp, ds, Self::dev_mode_options().len(), selected, self.hovered_dropdown_item, scale, t);
        }

        // Reset Theme button (row 2)
        let (btn_pos, btn_size) = self.reset_theme_button_rect(screen_w, screen_h, scale);
        out.push(InstanceRaw {
            position: btn_pos,
            size: btn_size,
            color: t.accent,
            border_radius: 6.0 * scale,
        });
    }
}

// ---------------------------------------------------------------------------
// Text entries (for glyphon rendering in gpu.rs)
// ---------------------------------------------------------------------------

use crate::gpu::TextEntry;

impl SettingsWindow {
    /// Helper to push a row label text entry.
    fn push_row_label(
        out: &mut Vec<TextEntry>,
        text: &str,
        content_x: f32,
        row_y: f32,
        scale: f32,
        settings: &Settings,
    ) {
        let label_font = 13.0 * scale;
        let label_line = 18.0 * scale;
        out.push(TextEntry {
            text: text.to_string(),
            x: content_x + ROW_LABEL_X * scale,
            y: row_y + (ROW_HEIGHT * scale - label_line) * 0.5,
            font_size: label_font,
            line_height: label_line,
            color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_primary, 255),
            weight: 400,
            max_width: 300.0 * scale,
            bounds: None,
            center: false,
        });
    }


    pub fn get_text_entries(
        &self,
        settings: &Settings,
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (wp, ws) = self.win_rect(screen_w, screen_h, scale);

        // Window title
        let title_font = 13.0 * scale;
        let title_line = 18.0 * scale;
        out.push(TextEntry {
            text: "Settings".to_string(),
            x: wp[0] + ws[0] * 0.5 - 24.0 * scale,
            y: wp[1] - title_line - 6.0 * scale,
            font_size: title_font,
            line_height: title_line,
            color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_primary, 255),
            weight: 600,
            max_width: 300.0 * scale,
            bounds: None,
            center: false,
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
                crate::theme::RuntimeTheme::text_u8(settings.theme.text_primary, 255)
            } else {
                crate::theme::RuntimeTheme::text_u8(settings.theme.text_secondary, 255)
            };
            out.push(TextEntry {
                text: cat.label().to_string(),
                x: wp[0] + 18.0 * scale,
                y,
                font_size: cat_font,
                line_height: cat_line,
                color,
                weight: if is_active { 600 } else { 400 },
                max_width: 300.0 * scale,
                bounds: None,
                center: false,
            });
        }

        let content_x = wp[0] + SIDEBAR_WIDTH * scale;
        let section_font = 11.0 * scale;
        let section_line = 15.0 * scale;

        match self.active_category {
            SettingsCategory::ThemeAndColors => {
                // Section header
                out.push(TextEntry {
                    text: "Customization".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_dim, 200),
                    weight: 600,
                    max_width: 300.0 * scale,
                    bounds: None,
                    center: false,
                });

                let content_w = ws[0] - SIDEBAR_WIDTH * scale;

                // Row 0: Theme preset
                let theme_row_y = wp[1] + SECTION_HEADER_HEIGHT * scale;
                Self::push_row_label(&mut out, "Theme", content_x, theme_row_y, scale, settings);
                let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
                super::dropdown::render_value_text(&mut out, &settings.theme_preset, dp, ds, scale, &settings.theme);

                // Theme preset popup text
                if let Some(0) = self.open_dropdown {
                    let selected = THEME_PRESETS.iter().position(|p| *p == settings.theme_preset).unwrap_or(0);
                    super::dropdown::build_popup_text(&mut out, THEME_PRESETS, selected, dp, ds, scale, &settings.theme);
                }

                // Slider rows (only when Default preset)
                if settings.theme_preset == "Default" {
                    let value_font = 12.0 * scale;
                    let value_line = 16.0 * scale;
                    for (i, def) in SLIDERS.iter().enumerate() {
                        let row_y = wp[1]
                            + SECTION_HEADER_HEIGHT * scale
                            + (i + 1) as f32 * ROW_HEIGHT * scale;

                        Self::push_row_label(&mut out, def.label, content_x, row_y, scale, settings);

                        let val = Self::slider_value(settings, i);
                        let display = (val * def.display_scale) as i32;
                        let val_text = format!("{} {}", display, def.unit);
                        let val_x = content_x + content_w - SLIDER_RIGHT_PAD * scale
                            - VALUE_WIDTH * scale
                            + 8.0 * scale;
                        out.push(TextEntry {
                            text: val_text,
                            x: val_x,
                            y: row_y + (ROW_HEIGHT * scale - value_line) * 0.5,
                            font_size: value_font,
                            line_height: value_line,
                            color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_secondary, 255),
                            weight: 400,
                            max_width: 300.0 * scale,
                            bounds: None,
                            center: false,
                        });
                    }
                }
            }
            SettingsCategory::Audio => {
                // Section header
                out.push(TextEntry {
                    text: "Audio Device".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_dim, 200),
                    weight: 600,
                    max_width: 300.0 * scale,
                    bounds: None,
                    center: false,
                });

                let labels = ["Driver Type", "Audio Input Device", "Audio Output Device"];

                for i in 0..AUDIO_DROPDOWN_COUNT {
                    let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + i as f32 * ROW_HEIGHT * scale;
                    Self::push_row_label(&mut out, labels[i], content_x, row_y, scale, settings);

                    let current = Self::dropdown_current(settings, i);
                    let (dp, ds) = self.dropdown_rect(i, screen_w, screen_h, scale);
                    super::dropdown::render_value_text(&mut out, current, dp, ds, scale, &settings.theme);
                }

                // Row 3: Auto Clip Fades
                {
                    let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + 3.0 * ROW_HEIGHT * scale;
                    Self::push_row_label(&mut out, "Auto Clip Fades", content_x, row_y, scale, settings);
                    let current_text = if settings.auto_clip_fades { "On" } else { "Off" };
                    let (dp3, ds3) = self.dropdown_rect(3, screen_w, screen_h, scale);
                    super::dropdown::render_value_text(&mut out, current_text, dp3, ds3, scale, &settings.theme);
                }

                // Row 4: Buffer Size
                {
                    let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale + 4.0 * ROW_HEIGHT * scale;
                    Self::push_row_label(&mut out, "Buffer Size", content_x, row_y, scale, settings);
                    let current_text = settings.buffer_size.to_string();
                    let (dp4, ds4) = self.dropdown_rect(4, screen_w, screen_h, scale);
                    super::dropdown::render_value_text(&mut out, &current_text, dp4, ds4, scale, &settings.theme);
                }

                // Popup item text
                if let Some(dd_idx) = self.open_dropdown {
                    let (dp, ds) = self.dropdown_rect(dd_idx, screen_w, screen_h, scale);
                    if dd_idx == 3 {
                        let options = Self::auto_clip_fades_options();
                        let selected = if settings.auto_clip_fades { 0 } else { 1 };
                        super::dropdown::build_popup_text(&mut out, options, selected, dp, ds, scale, &settings.theme);
                    } else {
                        let options = self.dropdown_options(dd_idx);
                        if !options.is_empty() {
                            let current_buf;
                            let current = if dd_idx == 4 {
                                current_buf = settings.buffer_size.to_string();
                                current_buf.as_str()
                            } else {
                                Self::dropdown_current(settings, dd_idx)
                            };
                            let selected = options.iter().position(|o| o == current).unwrap_or(0);
                            let labels: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
                            super::dropdown::build_popup_text(&mut out, &labels, selected, dp, ds, scale, &settings.theme);
                        }
                    }
                }
            }
            SettingsCategory::Developer => {
                // Section header
                out.push(TextEntry {
                    text: "Developer".to_string(),
                    x: content_x + ROW_LABEL_X * scale,
                    y: wp[1] + (SECTION_HEADER_HEIGHT * scale - section_line) * 0.5,
                    font_size: section_font,
                    line_height: section_line,
                    color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_dim, 200),
                    weight: 600,
                    max_width: 300.0 * scale,
                    bounds: None,
                    center: false,
                });

                let dd_font = 12.0 * scale;
                let dd_line = 16.0 * scale;

                // Row 0: Mode
                let row_y = wp[1] + SECTION_HEADER_HEIGHT * scale;
                Self::push_row_label(&mut out, "Mode", content_x, row_y, scale, settings);
                let current_text = if settings.dev_mode { "Development" } else { "Production" };
                let (dp, ds) = self.dropdown_rect(0, screen_w, screen_h, scale);
                super::dropdown::render_value_text(&mut out, current_text, dp, ds, scale, &settings.theme);

                // Row 1: Build version
                let build_row_y = row_y + ROW_HEIGHT * scale;
                Self::push_row_label(&mut out, "Build", content_x, build_row_y, scale, settings);
                let build_version = std::fs::read_to_string("build_version")
                    .unwrap_or_else(|_| "0".to_string());
                let build_version = build_version.trim();
                out.push(TextEntry {
                    text: format!("#{}", build_version),
                    x: dp[0] + 10.0 * scale,
                    y: build_row_y + (ROW_HEIGHT * scale - dd_line) * 0.5,
                    font_size: dd_font,
                    line_height: dd_line,
                    color: crate::theme::RuntimeTheme::text_u8(settings.theme.text_dim, 200),
                    weight: 400,
                    max_width: 300.0 * scale,
                    bounds: None,
                    center: false,
                });

                // Row 2: Reset Theme
                let reset_row_y = row_y + 2.0 * ROW_HEIGHT * scale;
                Self::push_row_label(&mut out, "Theme", content_x, reset_row_y, scale, settings);

                let (btn_pos, btn_size) = self.reset_theme_button_rect(screen_w, screen_h, scale);
                out.push(TextEntry {
                    text: "Reset to Defaults".to_string(),
                    x: btn_pos[0] + btn_size[0] * 0.5,
                    y: btn_pos[1] + (btn_size[1] - dd_line) * 0.5,
                    font_size: dd_font,
                    line_height: dd_line,
                    color: [255, 255, 255, 255],
                    weight: 600,
                    max_width: btn_size[0],
                    bounds: None,
                    center: true,
                });

                // Popup item text
                if let Some(0) = self.open_dropdown {
                    let options = Self::dev_mode_options();
                    let selected = if settings.dev_mode { 1 } else { 0 };
                    super::dropdown::build_popup_text(&mut out, options, selected, dp, ds, scale, &settings.theme);
                }
            }
        }

        out
    }
}
