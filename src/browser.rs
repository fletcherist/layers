use std::collections::HashSet;
use std::path::PathBuf;

use crate::InstanceRaw;

const DEFAULT_BROWSER_WIDTH: f32 = 260.0;
const MIN_BROWSER_WIDTH: f32 = 150.0;
const MAX_BROWSER_WIDTH: f32 = 600.0;
const RESIZE_HANDLE_PX: f32 = 5.0;
const ITEM_HEIGHT: f32 = 24.0;
pub const HEADER_HEIGHT: f32 = 36.0;
const INDENT_PX: f32 = 16.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const ADD_BUTTON_SIZE: f32 = 20.0;

const BG_COLOR: [f32; 4] = [0.11, 0.11, 0.14, 1.0];
const HEADER_BG: [f32; 4] = [0.13, 0.13, 0.17, 1.0];
const HOVER_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.06];
const SCROLLBAR_BG: [f32; 4] = [1.0, 1.0, 1.0, 0.08];
const SCROLLBAR_THUMB: [f32; 4] = [1.0, 1.0, 1.0, 0.20];
const CHEVRON_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.40];
const ADD_BTN_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.50];
const ADD_BTN_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.80];

#[derive(Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

pub struct SampleBrowser {
    pub root_folders: Vec<PathBuf>,
    pub expanded: HashSet<PathBuf>,
    pub entries: Vec<BrowserEntry>,
    pub scroll_offset: f32,
    pub scroll_velocity: f32,
    pub hovered_entry: Option<usize>,
    pub visible: bool,
    pub add_button_hovered: bool,
    pub width: f32,
    pub resize_hovered: bool,
    pub text_dirty: bool,
    pub cached_text: Vec<BrowserTextEntry>,
    pub text_generation: u64,
    cached_screen_h: f32,
    cached_scale: f32,
    last_scroll_screen_h: f32,
    last_scroll_scale: f32,
}

impl SampleBrowser {
    pub fn new() -> Self {
        Self {
            root_folders: Vec::new(),
            expanded: HashSet::new(),
            entries: Vec::new(),
            scroll_offset: 0.0,
            scroll_velocity: 0.0,
            hovered_entry: None,
            visible: false,
            add_button_hovered: false,
            width: DEFAULT_BROWSER_WIDTH,
            resize_hovered: false,
            text_dirty: true,
            cached_text: Vec::new(),
            text_generation: 0,
            cached_screen_h: 0.0,
            cached_scale: 0.0,
            last_scroll_screen_h: 0.0,
            last_scroll_scale: 0.0,
        }
    }

    pub fn from_folders(folders: Vec<PathBuf>) -> Self {
        let mut browser = Self::new();
        browser.visible = !folders.is_empty();
        for f in folders {
            if f.is_dir() {
                browser.expanded.insert(f.clone());
                browser.root_folders.push(f);
            }
        }
        browser.rebuild_entries();
        browser
    }

    pub fn from_state(folders: Vec<PathBuf>, expanded: HashSet<PathBuf>, visible: bool) -> Self {
        let mut browser = Self::new();
        browser.visible = visible;
        browser.expanded = expanded;
        for f in folders {
            if f.is_dir() {
                browser.root_folders.push(f);
            }
        }
        browser.rebuild_entries();
        browser
    }

    pub fn add_folder(&mut self, path: PathBuf) {
        if self.root_folders.contains(&path) {
            return;
        }
        self.expanded.insert(path.clone());
        self.root_folders.push(path);
        self.rebuild_entries();
    }

    pub fn remove_folder(&mut self, index: usize) {
        if index < self.root_folders.len() {
            let removed = self.root_folders.remove(index);
            self.expanded.remove(&removed);
            self.rebuild_entries();
        }
    }

    pub fn toggle_expand(&mut self, entry_idx: usize) {
        if let Some(entry) = self.entries.get(entry_idx) {
            if entry.is_dir {
                let path = entry.path.clone();
                if self.expanded.contains(&path) {
                    self.expanded.remove(&path);
                } else {
                    self.expanded.insert(path);
                }
                self.rebuild_entries();
            }
        }
    }

    pub fn is_expanded(&self, path: &PathBuf) -> bool {
        self.expanded.contains(path)
    }

    pub fn rebuild_entries(&mut self) {
        self.entries.clear();
        for root in &self.root_folders.clone() {
            self.walk_dir(root, 0);
        }
        self.clamp_scroll();
        self.text_dirty = true;
    }

    fn walk_dir(&mut self, dir: &PathBuf, depth: usize) {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.to_string_lossy().to_string());

        self.entries.push(BrowserEntry {
            path: dir.clone(),
            name,
            is_dir: true,
            depth,
        });

        if !self.expanded.contains(dir) {
            return;
        }

        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };

        let mut children: Vec<(bool, String, PathBuf)> = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            let is_dir = path.is_dir();
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') {
                continue;
            }
            children.push((is_dir, fname, path));
        }

        children.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });

        for (is_dir, fname, path) in children {
            if is_dir {
                self.walk_dir(&path, depth + 1);
            } else {
                self.entries.push(BrowserEntry {
                    path,
                    name: fname,
                    is_dir: false,
                    depth: depth + 1,
                });
            }
        }
    }

    fn content_height(&self, scale: f32) -> f32 {
        self.entries.len() as f32 * ITEM_HEIGHT * scale
    }

    fn visible_height(&self, screen_h: f32, scale: f32) -> f32 {
        screen_h - HEADER_HEIGHT * scale
    }

    fn max_scroll(&self, screen_h: f32, scale: f32) -> f32 {
        (self.content_height(scale) - self.visible_height(screen_h, scale)).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_offset = self.scroll_offset.max(0.0);
    }

    pub fn clamp_scroll_for_screen(&mut self, screen_h: f32, scale: f32) {
        let max = self.max_scroll(screen_h, scale);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max);
    }

    /// Trackpad: apply delta directly (OS provides momentum)
    pub fn scroll_direct(&mut self, delta: f32, screen_h: f32, scale: f32) {
        self.scroll_offset -= delta;
        self.scroll_velocity = 0.0;
        self.clamp_scroll_for_screen(screen_h, scale);
    }

    /// Mouse wheel: accumulate velocity for smooth deceleration
    pub fn scroll(&mut self, delta: f32, screen_h: f32, scale: f32) {
        self.scroll_velocity += -delta;
        self.last_scroll_screen_h = screen_h;
        self.last_scroll_scale = scale;
    }

    /// Advance smooth scroll animation. Returns true if still animating.
    pub fn tick_scroll(&mut self) -> bool {
        if self.scroll_velocity.abs() < 0.5 {
            self.scroll_velocity = 0.0;
            return false;
        }
        self.scroll_offset += self.scroll_velocity;
        self.scroll_velocity *= 0.85;
        if self.last_scroll_screen_h > 0.0 {
            self.clamp_scroll_for_screen(self.last_scroll_screen_h, self.last_scroll_scale);
        }
        true
    }

    pub fn is_scroll_animating(&self) -> bool {
        self.scroll_velocity.abs() >= 0.5
    }

    pub fn panel_width(&self, scale: f32) -> f32 {
        self.width * scale
    }

    pub fn hit_resize_handle(&self, pos: [f32; 2], scale: f32) -> bool {
        let edge = self.panel_width(scale);
        let zone = RESIZE_HANDLE_PX * scale;
        pos[0] >= edge - zone && pos[0] <= edge + zone
    }

    pub fn set_width_from_screen(&mut self, screen_x: f32, scale: f32) {
        let w = (screen_x / scale).clamp(MIN_BROWSER_WIDTH, MAX_BROWSER_WIDTH);
        self.width = w;
        self.text_dirty = true;
    }

    pub fn contains(&self, pos: [f32; 2], _screen_h: f32, scale: f32) -> bool {
        let edge = self.panel_width(scale);
        let zone = RESIZE_HANDLE_PX * scale;
        pos[0] >= 0.0 && pos[0] <= edge + zone
    }

    pub fn add_button_rect(&self, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = self.panel_width(scale);
        let sz = ADD_BUTTON_SIZE * scale;
        let margin = (HEADER_HEIGHT * scale - sz) * 0.5;
        let x = w - margin - sz;
        let y = margin;
        ([x, y], [sz, sz])
    }

    pub fn hit_add_button(&self, pos: [f32; 2], scale: f32) -> bool {
        let (bp, bs) = self.add_button_rect(scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0] && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    pub fn item_at(&self, pos: [f32; 2], _screen_h: f32, scale: f32) -> Option<usize> {
        let header_h = HEADER_HEIGHT * scale;
        if pos[1] < header_h {
            return None;
        }
        let y = pos[1] - header_h + self.scroll_offset;
        let idx = (y / (ITEM_HEIGHT * scale)) as usize;
        if idx < self.entries.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_h: f32, scale: f32) {
        self.resize_hovered = self.hit_resize_handle(pos, scale);
        self.add_button_hovered = self.hit_add_button(pos, scale);
        self.hovered_entry = if self.resize_hovered {
            None
        } else {
            self.item_at(pos, screen_h, scale)
        };
    }

    pub fn build_instances(&self, _screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let item_h = ITEM_HEIGHT * scale;

        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [w, screen_h],
            color: BG_COLOR,
            border_radius: 0.0,
        });

        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [w, header_h],
            color: HEADER_BG,
            border_radius: 0.0,
        });

        // Separator under header
        out.push(InstanceRaw {
            position: [0.0, header_h - 1.0 * scale],
            size: [w, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.07],
            border_radius: 0.0,
        });

        // "+" add folder button
        let (bp, bs) = self.add_button_rect(scale);
        let btn_color = if self.add_button_hovered {
            ADD_BTN_HOVER
        } else {
            ADD_BTN_COLOR
        };
        // Horizontal bar of the +
        let bar_h = 2.0 * scale;
        let bar_w = bs[0] * 0.6;
        out.push(InstanceRaw {
            position: [bp[0] + (bs[0] - bar_w) * 0.5, bp[1] + (bs[1] - bar_h) * 0.5],
            size: [bar_w, bar_h],
            color: btn_color,
            border_radius: 0.0,
        });
        // Vertical bar of the +
        out.push(InstanceRaw {
            position: [bp[0] + (bs[0] - bar_h) * 0.5, bp[1] + (bs[1] - bar_w) * 0.5],
            size: [bar_h, bar_w],
            color: btn_color,
            border_radius: 0.0,
        });

        // Right edge separator
        out.push(InstanceRaw {
            position: [w - 1.0 * scale, 0.0],
            size: [1.0 * scale, screen_h],
            color: [1.0, 1.0, 1.0, 0.07],
            border_radius: 0.0,
        });

        let visible_h = self.visible_height(screen_h, scale);
        let first_visible = (self.scroll_offset / item_h) as usize;
        let last_visible = ((self.scroll_offset + visible_h) / item_h).ceil() as usize;
        let last_visible = last_visible.min(self.entries.len());

        for i in first_visible..last_visible {
            let entry = &self.entries[i];
            let y = header_h + i as f32 * item_h - self.scroll_offset;

            if y + item_h < header_h || y > screen_h {
                continue;
            }

            // Hover highlight
            if self.hovered_entry == Some(i) {
                out.push(InstanceRaw {
                    position: [0.0, y],
                    size: [w, item_h],
                    color: HOVER_COLOR,
                    border_radius: 0.0,
                });
            }

            let indent = entry.depth as f32 * INDENT_PX * scale + 8.0 * scale;

            // Chevron for directories
            if entry.is_dir {
                let chev_sz = 6.0 * scale;
                let cx = indent + chev_sz * 0.5;
                let cy = y + item_h * 0.5;

                if self.is_expanded(&entry.path) {
                    // Down-pointing chevron (two small bars forming a V)
                    let bar_w = chev_sz * 0.7;
                    let bar_h = 1.5 * scale;
                    out.push(InstanceRaw {
                        position: [cx - bar_w * 0.5, cy - bar_h],
                        size: [bar_w, bar_h],
                        color: CHEVRON_COLOR,
                        border_radius: 0.0,
                    });
                    out.push(InstanceRaw {
                        position: [cx - bar_w * 0.25, cy],
                        size: [bar_w * 0.5, bar_h],
                        color: CHEVRON_COLOR,
                        border_radius: 0.0,
                    });
                } else {
                    // Right-pointing chevron
                    let bar_w = 1.5 * scale;
                    let bar_h = chev_sz * 0.7;
                    out.push(InstanceRaw {
                        position: [cx - bar_w, cy - bar_h * 0.5],
                        size: [bar_w, bar_h],
                        color: CHEVRON_COLOR,
                        border_radius: 0.0,
                    });
                    out.push(InstanceRaw {
                        position: [cx, cy - bar_h * 0.25],
                        size: [bar_w, bar_h * 0.5],
                        color: CHEVRON_COLOR,
                        border_radius: 0.0,
                    });
                }
            }
        }

        // Scrollbar
        let content_h = self.content_height(scale);
        if content_h > visible_h {
            let sb_x = w - SCROLLBAR_WIDTH * scale - 2.0 * scale;
            let sb_track_h = visible_h;

            out.push(InstanceRaw {
                position: [sb_x, header_h],
                size: [SCROLLBAR_WIDTH * scale, sb_track_h],
                color: SCROLLBAR_BG,
                border_radius: 3.0 * scale,
            });

            let ratio = visible_h / content_h;
            let thumb_h = (ratio * sb_track_h).max(20.0 * scale);
            let scroll_ratio = if self.max_scroll(screen_h, scale) > 0.0 {
                self.scroll_offset / self.max_scroll(screen_h, scale)
            } else {
                0.0
            };
            let thumb_y = header_h + scroll_ratio * (sb_track_h - thumb_h);

            out.push(InstanceRaw {
                position: [sb_x, thumb_y],
                size: [SCROLLBAR_WIDTH * scale, thumb_h],
                color: SCROLLBAR_THUMB,
                border_radius: 3.0 * scale,
            });
        }

        out
    }

    pub fn get_text_entries(&mut self, screen_h: f32, scale: f32) -> &[BrowserTextEntry] {
        if self.text_dirty
            || (self.cached_screen_h - screen_h).abs() > 0.5
            || (self.cached_scale - scale).abs() > 0.001
        {
            self.cached_text = self.build_text_entries(screen_h, scale);
            self.cached_screen_h = screen_h;
            self.cached_scale = scale;
            self.text_dirty = false;
            self.text_generation += 1;
        }
        &self.cached_text
    }

    fn build_text_entries(&self, _screen_h: f32, scale: f32) -> Vec<BrowserTextEntry> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let item_h = ITEM_HEIGHT * scale;

        out.push(BrowserTextEntry {
            text: "EXPLORER".to_string(),
            x: 12.0 * scale,
            base_y: (header_h - 14.0 * scale) * 0.5,
            font_size: 11.0 * scale,
            line_height: 14.0 * scale,
            max_width: w * 0.6,
            color: [150, 150, 160, 200],
            weight: 600,
            is_header: true,
        });

        for (i, entry) in self.entries.iter().enumerate() {
            let base_y = header_h + i as f32 * item_h;
            let indent = entry.depth as f32 * INDENT_PX * scale + 8.0 * scale;
            let text_x = indent + 18.0 * scale;
            let font_sz = 13.0 * scale;
            let line_h = 18.0 * scale;

            let (color, weight) = if entry.is_dir {
                ([210, 210, 218, 255], 400u16)
            } else {
                ([170, 170, 180, 255], 400u16)
            };

            out.push(BrowserTextEntry {
                text: entry.name.clone(),
                x: text_x,
                base_y: base_y + (item_h - line_h) * 0.5,
                font_size: font_sz,
                line_height: line_h,
                max_width: w - text_x - 12.0 * scale,
                color,
                weight,
                is_header: false,
            });
        }

        out
    }
}

pub struct BrowserTextEntry {
    pub text: String,
    pub x: f32,
    pub base_y: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub max_width: f32,
    pub color: [u8; 4],
    pub weight: u16,
    pub is_header: bool,
}
