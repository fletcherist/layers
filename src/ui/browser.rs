use std::collections::HashSet;
use std::path::PathBuf;

#[cfg(target_arch = "wasm32")]
use web_time::Instant as TimeInstant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant as TimeInstant;

use crate::InstanceRaw;
use crate::entity_id::EntityId;
use crate::layers::{FlatLayerRow, LayerNodeKind};

pub const DEFAULT_BROWSER_WIDTH: f32 = 320.0;
const MIN_BROWSER_WIDTH: f32 = 240.0;
const MAX_BROWSER_WIDTH: f32 = 700.0;
const RESIZE_HANDLE_PX: f32 = 5.0;
pub const ITEM_HEIGHT: f32 = 24.0;
pub const HEADER_HEIGHT: f32 = 36.0;
const SEARCH_BAR_HEIGHT: f32 = 32.0;
const SIDEBAR_WIDTH: f32 = 110.0;
const SIDEBAR_ITEM_HEIGHT: f32 = 26.0;
const SIDEBAR_SECTION_GAP: f32 = 18.0;
const INDENT_PX: f32 = 16.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const ADD_BUTTON_SIZE: f32 = 20.0;


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrowserCategory {
    Layers,
    Samples,
    Instruments,
    Effects,
}

pub const SIDEBAR_CATEGORIES: &[BrowserCategory] = &[
    BrowserCategory::Layers,
    BrowserCategory::Samples,
    BrowserCategory::Instruments,
    BrowserCategory::Effects,
];

impl BrowserCategory {
    pub fn label(self) -> &'static str {
        match self {
            BrowserCategory::Layers => "Layers",
            BrowserCategory::Samples => "Samples",
            BrowserCategory::Instruments => "Instruments",
            BrowserCategory::Effects => "Effects",
        }
    }
}

#[derive(Clone)]
pub enum EntryKind {
    Dir,
    File,
    PluginHeader,
    Plugin { unique_id: String, is_instrument: bool },
    ProjectInstrument { id: EntityId },
    LayerNode { id: EntityId, kind: LayerNodeKind, has_children: bool, expanded: bool, color: [f32; 4], is_soloed: bool, is_muted: bool },
}

#[derive(Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: EntryKind,
    pub depth: usize,
}

impl BrowserEntry {
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, EntryKind::Dir)
    }
}

#[derive(Clone)]
pub struct PluginEntry {
    pub unique_id: String,
    pub name: String,
    pub manufacturer: String,
    pub is_instrument: bool,
}

/// Cached file entry for fast search (avoids repeated filesystem walks).
#[derive(Clone)]
struct CachedFile {
    path: PathBuf,
    name: String,
    name_lower: String,
}

/// Maximum number of search results to display.
const MAX_SEARCH_RESULTS: usize = 500;

/// Search debounce delay in milliseconds.
const SEARCH_DEBOUNCE_MS: u64 = 50;

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
    pub cached_text: Vec<TextEntry>,
    pub text_generation: u64,
    cached_screen_h: f32,
    cached_scale: f32,
    cached_text_primary_r: f32,
    last_scroll_screen_h: f32,
    last_scroll_scale: f32,
    pub plugins: Vec<PluginEntry>,
    pub instruments: Vec<PluginEntry>,
    pub plugins_expanded: bool,
    pub instruments_expanded: bool,
    pub active_category: BrowserCategory,
    pub hovered_sidebar: Option<usize>,
    /// Flattened layer tree rows for the Layers tab.
    pub layer_rows: Vec<FlatLayerRow>,
    /// Inline rename state for a layer row: (entity_id, kind, current_text).
    pub editing_browser_name: Option<(crate::entity_id::EntityId, LayerNodeKind, String)>,
    /// Current search query string.
    pub search_query: String,
    /// Whether the search bar is focused (accepting keyboard input).
    pub search_focused: bool,
    /// Cached flat file index for fast sample search.
    file_index: Vec<CachedFile>,
    /// Whether the file index needs rebuilding (root folders changed).
    file_index_dirty: bool,
    /// When set, a search rebuild is pending and should fire after this deadline.
    search_debounce_deadline: Option<TimeInstant>,
    /// Whether the search clear (X) button is hovered.
    pub search_clear_hovered: bool,
    /// Drop indicator for layer reorder drag: (flat_row_index, depth, is_inside_group).
    pub layer_drop_indicator: Option<(usize, usize, bool)>,
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
            visible: true,
            add_button_hovered: false,
            width: DEFAULT_BROWSER_WIDTH,
            resize_hovered: false,
            text_dirty: true,
            cached_text: Vec::new(),
            text_generation: 0,
            cached_screen_h: 0.0,
            cached_scale: 0.0,
            cached_text_primary_r: -1.0,
            last_scroll_screen_h: 0.0,
            last_scroll_scale: 0.0,
            plugins: Vec::new(),
            instruments: Vec::new(),
            plugins_expanded: true,
            instruments_expanded: true,
            active_category: BrowserCategory::Samples,
            hovered_sidebar: None,
            layer_rows: Vec::new(),
            editing_browser_name: None,
            search_query: String::new(),
            search_focused: false,
            file_index: Vec::new(),
            file_index_dirty: true,
            search_debounce_deadline: None,
            search_clear_hovered: false,
            layer_drop_indicator: None,
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

    /// Restore saved width, clamping to at least DEFAULT_BROWSER_WIDTH.
    pub fn restore_width(&mut self, saved: f32) {
        self.width = saved.max(DEFAULT_BROWSER_WIDTH);
    }

    pub fn add_folder(&mut self, path: PathBuf) {
        if self.root_folders.contains(&path) {
            return;
        }
        self.expanded.insert(path.clone());
        self.root_folders.push(path);
        self.file_index_dirty = true;
        self.rebuild_entries();
    }

    pub fn remove_folder(&mut self, index: usize) {
        if index < self.root_folders.len() {
            let removed = self.root_folders.remove(index);
            self.expanded.remove(&removed);
            self.file_index_dirty = true;
            self.rebuild_entries();
        }
    }

    /// Rebuild the flat file index by walking all root folders once.
    /// Called lazily when a sample search is first performed after folders change.
    fn ensure_file_index(&mut self) {
        if !self.file_index_dirty {
            return;
        }
        self.file_index.clear();
        for root in &self.root_folders.clone() {
            Self::index_walk_dir(&mut self.file_index, root);
        }
        self.file_index_dirty = false;
    }

    fn index_walk_dir(index: &mut Vec<CachedFile>, dir: &std::path::Path) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for item in read.flatten() {
            let path = item.path();
            let fname = item.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                Self::index_walk_dir(index, &path);
            } else {
                let name_lower = fname.to_lowercase();
                index.push(CachedFile {
                    path,
                    name: fname,
                    name_lower,
                });
            }
        }
    }

    pub fn toggle_expand(&mut self, entry_idx: usize) {
        if let Some(entry) = self.entries.get(entry_idx) {
            match &entry.kind {
                EntryKind::Dir => {
                    let path = entry.path.clone();
                    if self.expanded.contains(&path) {
                        self.expanded.remove(&path);
                    } else {
                        self.expanded.insert(path);
                    }
                    self.rebuild_entries();
                }
                EntryKind::PluginHeader => {
                    if entry.name == "INSTRUMENTS" {
                        self.instruments_expanded = !self.instruments_expanded;
                    } else {
                        self.plugins_expanded = !self.plugins_expanded;
                    }
                    self.rebuild_entries();
                }
                _ => {}
            }
        }
    }

    pub fn is_expanded(&self, path: &PathBuf) -> bool {
        self.expanded.contains(path)
    }

    pub fn set_plugins(&mut self, effects: Vec<PluginEntry>, instruments: Vec<PluginEntry>) {
        self.plugins = effects;
        self.instruments = instruments;
        self.rebuild_entries();
    }

    pub fn rebuild_entries(&mut self) {
        self.entries.clear();
        let query = self.search_query.clone();
        let searching = !query.is_empty();
        match self.active_category {
            BrowserCategory::Layers => {
                for row in &self.layer_rows {
                    if searching && !fuzzy_match(&row.label, &query) {
                        continue;
                    }
                    self.entries.push(BrowserEntry {
                        path: PathBuf::new(),
                        name: row.label.clone(),
                        kind: EntryKind::LayerNode {
                            id: row.entity_id,
                            kind: row.kind,
                            has_children: row.has_children,
                            expanded: row.expanded,
                            color: row.color,
                            is_soloed: row.is_soloed,
                            is_muted: row.is_muted,
                        },
                        depth: if searching { 0 } else { row.depth },
                    });
                }
            }
            BrowserCategory::Samples => {
                if searching {
                    self.ensure_file_index();
                    let query_lower = query.to_lowercase();
                    for cached in &self.file_index {
                        if fuzzy_match_lowered(&cached.name_lower, &query_lower) {
                            self.entries.push(BrowserEntry {
                                path: cached.path.clone(),
                                name: cached.name.clone(),
                                kind: EntryKind::File,
                                depth: 0,
                            });
                            if self.entries.len() >= MAX_SEARCH_RESULTS {
                                break;
                            }
                        }
                    }
                } else {
                    for root in &self.root_folders {
                        walk_dir(&mut self.entries, &self.expanded, root, 0);
                    }
                }
            }
            BrowserCategory::Instruments => {
                for inst in &self.instruments {
                    if searching && !fuzzy_match(&inst.name, &query) {
                        continue;
                    }
                    self.entries.push(BrowserEntry {
                        path: PathBuf::new(),
                        name: inst.name.clone(),
                        kind: EntryKind::Plugin {
                            unique_id: inst.unique_id.clone(),
                            is_instrument: true,
                        },
                        depth: 0,
                    });
                }
            }
            BrowserCategory::Effects => {
                for plug in &self.plugins {
                    if searching && !fuzzy_match(&plug.name, &query) {
                        continue;
                    }
                    self.entries.push(BrowserEntry {
                        path: PathBuf::new(),
                        name: plug.name.clone(),
                        kind: EntryKind::Plugin {
                            unique_id: plug.unique_id.clone(),
                            is_instrument: false,
                        },
                        depth: 0,
                    });
                }
            }
        }
        self.clamp_scroll();
        self.text_dirty = true;
    }

    fn sidebar_width(&self, scale: f32) -> f32 {
        SIDEBAR_WIDTH * scale
    }

    pub(crate) fn content_x(&self, scale: f32) -> f32 {
        self.sidebar_width(scale)
    }

    pub(crate) fn content_width(&self, scale: f32) -> f32 {
        self.panel_width(scale) - self.sidebar_width(scale)
    }

    fn content_height(&self, scale: f32) -> f32 {
        self.entries.len() as f32 * ITEM_HEIGHT * scale
    }

    pub(crate) fn content_top(&self, scale: f32) -> f32 {
        (HEADER_HEIGHT + SEARCH_BAR_HEIGHT) * scale
    }

    fn visible_height(&self, screen_h: f32, scale: f32) -> f32 {
        screen_h - self.content_top(scale)
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

    /// Schedule a debounced search rebuild. Call this instead of `rebuild_entries()`
    /// when the search query changes from keyboard input.
    pub fn schedule_search_rebuild(&mut self) {
        self.search_debounce_deadline =
            Some(TimeInstant::now() + std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS));
    }

    /// Flush pending debounced search if the deadline has passed.
    /// Returns true if a rebuild was performed (caller should request redraw).
    pub fn tick_search_debounce(&mut self) -> bool {
        if let Some(deadline) = self.search_debounce_deadline {
            if TimeInstant::now() >= deadline {
                self.search_debounce_deadline = None;
                self.rebuild_entries();
                self.text_dirty = true;
                return true;
            }
        }
        false
    }

    /// Returns true if a search rebuild is pending (debounce timer active).
    pub fn is_search_pending(&self) -> bool {
        self.search_debounce_deadline.is_some()
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
        let row_y = HEADER_HEIGHT * scale;
        let margin = (SEARCH_BAR_HEIGHT * scale - sz) * 0.5;
        let x = w - margin - sz;
        let y = row_y + margin;
        ([x, y], [sz, sz])
    }

    /// Rect for the search clear (X) button, inside the search input on the right.
    fn clear_button_rect(&self, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = self.panel_width(scale);
        let margin = 6.0 * scale;
        let right_pad = if self.active_category == BrowserCategory::Samples {
            (ADD_BUTTON_SIZE + 10.0) * scale
        } else {
            margin
        };
        let sb_x = margin;
        let row_y = HEADER_HEIGHT * scale;
        let search_bar_h = SEARCH_BAR_HEIGHT * scale;
        let sb_y = row_y + margin;
        let sb_h = search_bar_h - 2.0 * margin;
        let sb_w_inner = w - sb_x - right_pad;
        let btn_sz = 16.0 * scale;
        let btn_x = sb_x + sb_w_inner - btn_sz - 2.0 * scale;
        let btn_y = sb_y + (sb_h - btn_sz) * 0.5;
        ([btn_x, btn_y], [btn_sz, btn_sz])
    }

    pub fn hit_clear_button(&self, pos: [f32; 2], scale: f32) -> bool {
        if self.search_query.is_empty() {
            return false;
        }
        let (bp, bs) = self.clear_button_rect(scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0] && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    pub fn get_search_clear_icon_entry(&self, theme: &crate::theme::RuntimeTheme, scale: f32) -> Option<crate::gpu::IconEntry> {
        if self.search_query.is_empty() {
            return None;
        }
        let (bp, bs) = self.clear_button_rect(scale);
        let icon_size = bs[1];
        let color = if self.search_clear_hovered {
            crate::theme::RuntimeTheme::text_u8(theme.text_primary, 220)
        } else {
            crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 160)
        };
        Some(crate::gpu::IconEntry {
            codepoint: crate::icons::CLOSE,
            x: bp[0],
            y: bp[1],
            size: icon_size,
            color,
        })
    }

    pub fn hit_add_button(&self, pos: [f32; 2], scale: f32) -> bool {
        if self.active_category != BrowserCategory::Samples {
            return false;
        }
        let (bp, bs) = self.add_button_rect(scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0] && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    pub fn hit_search_bar(&self, pos: [f32; 2], scale: f32) -> bool {
        let top = HEADER_HEIGHT * scale;
        let bottom = top + SEARCH_BAR_HEIGHT * scale;
        pos[1] >= top
            && pos[1] < bottom
            && pos[0] >= 0.0
            && pos[0] < self.panel_width(scale)
            && !self.hit_add_button(pos, scale)
            && !self.hit_clear_button(pos, scale)
    }

    /// Returns which content-area entry the position is over, if any.
    /// Only considers positions in the content area (right of sidebar).
    pub fn item_at(&self, pos: [f32; 2], _screen_h: f32, scale: f32) -> Option<usize> {
        let top = self.content_top(scale);
        let cx = self.content_x(scale);
        if pos[1] < top || pos[0] < cx {
            return None;
        }
        let y = pos[1] - top + self.scroll_offset;
        let idx = (y / (ITEM_HEIGHT * scale)) as usize;
        if idx < self.entries.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Returns which sidebar category was clicked, if any.
    pub fn hit_sidebar(&self, pos: [f32; 2], scale: f32) -> Option<BrowserCategory> {
        let top = self.content_top(scale);
        let sb_w = self.sidebar_width(scale);
        if pos[0] >= sb_w || pos[1] < top {
            return None;
        }
        let y = pos[1] - top;
        // "Library" label gap
        let section_gap = SIDEBAR_SECTION_GAP * scale;
        let item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let content_y = y - section_gap;
        if content_y < 0.0 {
            return None;
        }
        let idx = (content_y / item_h) as usize;
        SIDEBAR_CATEGORIES.get(idx).copied()
    }

    /// Returns sidebar hover index (0-based into SIDEBAR_CATEGORIES).
    pub fn sidebar_item_at(&self, pos: [f32; 2], scale: f32) -> Option<usize> {
        let top = self.content_top(scale);
        let sb_w = self.sidebar_width(scale);
        if pos[0] >= sb_w || pos[1] < top {
            return None;
        }
        let y = pos[1] - top;
        let section_gap = SIDEBAR_SECTION_GAP * scale;
        let item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let content_y = y - section_gap;
        if content_y < 0.0 {
            return None;
        }
        let idx = (content_y / item_h) as usize;
        if idx < SIDEBAR_CATEGORIES.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_h: f32, scale: f32) {
        self.resize_hovered = self.hit_resize_handle(pos, scale);
        self.add_button_hovered = self.hit_add_button(pos, scale);
        self.search_clear_hovered = self.hit_clear_button(pos, scale);
        self.hovered_sidebar = self.sidebar_item_at(pos, scale);
        self.hovered_entry = if self.resize_hovered || self.hovered_sidebar.is_some() {
            None
        } else {
            self.item_at(pos, screen_h, scale)
        };
    }

    pub fn build_instances(&self, settings: &crate::settings::Settings, _screen_w: f32, screen_h: f32, scale: f32, selected_ids: &std::collections::HashSet<crate::entity_id::EntityId>) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let sb_w = self.sidebar_width(scale);
        let cx = self.content_x(scale);
        let item_h = ITEM_HEIGHT * scale;

        // --- Full panel background ---
        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [w, screen_h],
            color: settings.theme.bg_base,
            border_radius: 0.0,
        });

        // --- Header ---
        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [w, header_h],
            color: settings.theme.bg_surface,
            border_radius: 0.0,
        });
        // Separator under header
        out.push(InstanceRaw {
            position: [0.0, header_h - 1.0 * scale],
            size: [w, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.07],
            border_radius: 0.0,
        });

        // --- Search bar row (below header) ---
        {
            let search_bar_h = SEARCH_BAR_HEIGHT * scale;
            let row_y = header_h;
            // Search bar row background
            out.push(InstanceRaw {
                position: [0.0, row_y],
                size: [w, search_bar_h],
                color: settings.theme.bg_surface,
                border_radius: 0.0,
            });
            // Separator under search bar row
            out.push(InstanceRaw {
                position: [0.0, row_y + search_bar_h - 1.0 * scale],
                size: [w, 1.0 * scale],
                color: [1.0, 1.0, 1.0, 0.07],
                border_radius: 0.0,
            });
            let margin = 6.0 * scale;
            let right_pad = if self.active_category == BrowserCategory::Samples {
                (ADD_BUTTON_SIZE + 10.0) * scale
            } else {
                margin
            };
            let sb_x = margin;
            let sb_y = row_y + margin;
            let sb_w_inner = w - sb_x - right_pad;
            let sb_h = search_bar_h - 2.0 * margin;
            let bar_color = if self.search_focused {
                [
                    settings.theme.accent[0] * 0.15 + 0.05,
                    settings.theme.accent[1] * 0.15 + 0.05,
                    settings.theme.accent[2] * 0.15 + 0.05,
                    1.0,
                ]
            } else {
                crate::theme::with_alpha(settings.theme.shadow, settings.theme.shadow[3] * 0.6)
            };
            out.push(InstanceRaw {
                position: [sb_x, sb_y],
                size: [sb_w_inner, sb_h],
                color: bar_color,
                border_radius: 4.0 * scale,
            });
            // Focused border
            if self.search_focused {
                let border = 1.0 * scale;
                out.push(InstanceRaw {
                    position: [sb_x, sb_y],
                    size: [sb_w_inner, border],
                    color: [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.5],
                    border_radius: 0.0,
                });
                out.push(InstanceRaw {
                    position: [sb_x, sb_y + sb_h - border],
                    size: [sb_w_inner, border],
                    color: [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.5],
                    border_radius: 0.0,
                });
                out.push(InstanceRaw {
                    position: [sb_x, sb_y],
                    size: [border, sb_h],
                    color: [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.5],
                    border_radius: 0.0,
                });
                out.push(InstanceRaw {
                    position: [sb_x + sb_w_inner - border, sb_y],
                    size: [border, sb_h],
                    color: [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.5],
                    border_radius: 0.0,
                });
            }

            // Search clear (X) button hover highlight
            if !self.search_query.is_empty() && self.search_clear_hovered {
                let (cp, cs) = self.clear_button_rect(scale);
                out.push(InstanceRaw {
                    position: cp,
                    size: cs,
                    color: settings.theme.item_hover,
                    border_radius: cs[0] * 0.5,
                });
            }
        }

        // "+" add folder button — only on Samples category
        if self.active_category == BrowserCategory::Samples {
            let (bp, bs) = self.add_button_rect(scale);
            let btn_color = if self.add_button_hovered {
                crate::theme::with_alpha(settings.theme.text_primary, 0.80)
            } else {
                crate::theme::with_alpha(settings.theme.text_primary, 0.50)
            };
            let bar_h = 2.0 * scale;
            let bar_w = bs[0] * 0.6;
            out.push(InstanceRaw {
                position: [bp[0] + (bs[0] - bar_w) * 0.5, bp[1] + (bs[1] - bar_h) * 0.5],
                size: [bar_w, bar_h],
                color: btn_color,
                border_radius: 0.0,
            });
            out.push(InstanceRaw {
                position: [bp[0] + (bs[0] - bar_h) * 0.5, bp[1] + (bs[1] - bar_w) * 0.5],
                size: [bar_h, bar_w],
                color: btn_color,
                border_radius: 0.0,
            });
        }

        // --- Sidebar background (slightly darker) ---
        let ct = self.content_top(scale);
        out.push(InstanceRaw {
            position: [0.0, ct],
            size: [sb_w, screen_h - ct],
            color: [
                settings.theme.bg_base[0] * 0.9,
                settings.theme.bg_base[1] * 0.9,
                settings.theme.bg_base[2] * 0.9,
                1.0,
            ],
            border_radius: 0.0,
        });

        // --- Sidebar items ---
        let sb_item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let section_gap = SIDEBAR_SECTION_GAP * scale;

        for (i, cat) in SIDEBAR_CATEGORIES.iter().enumerate() {
            let y = ct + section_gap + i as f32 * sb_item_h;

            // Active highlight
            if *cat == self.active_category {
                out.push(InstanceRaw {
                    position: [0.0, y],
                    size: [sb_w, sb_item_h],
                    color: [
                        settings.theme.accent[0],
                        settings.theme.accent[1],
                        settings.theme.accent[2],
                        0.12,
                    ],
                    border_radius: 0.0,
                });
                // Left accent bar
                out.push(InstanceRaw {
                    position: [0.0, y],
                    size: [2.5 * scale, sb_item_h],
                    color: settings.theme.accent,
                    border_radius: 0.0,
                });
            } else if self.hovered_sidebar == Some(i) {
                out.push(InstanceRaw {
                    position: [0.0, y],
                    size: [sb_w, sb_item_h],
                    color: settings.theme.item_hover,
                    border_radius: 0.0,
                });
            }
        }

        // --- Vertical separator between sidebar and content ---
        out.push(InstanceRaw {
            position: [sb_w - 1.0 * scale, ct],
            size: [1.0 * scale, screen_h - ct],
            color: [1.0, 1.0, 1.0, 0.07],
            border_radius: 0.0,
        });

        // --- Right edge separator ---
        out.push(InstanceRaw {
            position: [w - 1.0 * scale, 0.0],
            size: [1.0 * scale, screen_h],
            color: [1.0, 1.0, 1.0, 0.07],
            border_radius: 0.0,
        });

        // --- Content area items ---
        let visible_h = self.visible_height(screen_h, scale);
        let first_visible = (self.scroll_offset / item_h) as usize;
        let last_visible = ((self.scroll_offset + visible_h) / item_h).ceil() as usize;
        let last_visible = last_visible.min(self.entries.len());
        let content_w = self.content_width(scale);

        for i in first_visible..last_visible {
            let entry = &self.entries[i];
            let y = ct + i as f32 * item_h - self.scroll_offset;

            if y + item_h < ct || y > screen_h {
                continue;
            }

            match &entry.kind {
                EntryKind::PluginHeader => {
                    // Section separator
                    out.push(InstanceRaw {
                        position: [cx, y],
                        size: [content_w, 1.0 * scale],
                        color: [1.0, 1.0, 1.0, 0.07],
                        border_radius: 0.0,
                    });
                    // Section header background
                    out.push(InstanceRaw {
                        position: [cx, y],
                        size: [content_w, item_h],
                        color: settings.theme.bg_plugin_header,
                        border_radius: 0.0,
                    });
                    // Badge
                    let badge_w = 18.0 * scale;
                    let badge_h = 12.0 * scale;
                    let badge_x = cx + 8.0 * scale;
                    let badge_y = y + (item_h - badge_h) * 0.5;
                    out.push(InstanceRaw {
                        position: [badge_x, badge_y],
                        size: [badge_w, badge_h],
                        color: settings.theme.accent_muted,
                        border_radius: 2.0 * scale,
                    });
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }
                }
                EntryKind::Plugin { is_instrument, .. } => {
                    // Plugin row background
                    out.push(InstanceRaw {
                        position: [cx, y],
                        size: [content_w, item_h],
                        color: settings.theme.bg_plugin,
                        border_radius: 0.0,
                    });
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }
                    // Category dot
                    let dot_sz = 5.0 * scale;
                    let dot_x = cx + 12.0 * scale;
                    let dot_y = y + (item_h - dot_sz) * 0.5;
                    let dot_color = if *is_instrument {
                        settings.theme.pill_instrument
                    } else {
                        settings.theme.pill_effect
                    };
                    out.push(InstanceRaw {
                        position: [dot_x, dot_y],
                        size: [dot_sz, dot_sz],
                        color: dot_color,
                        border_radius: dot_sz * 0.5,
                    });
                }
                EntryKind::ProjectInstrument { .. } | EntryKind::LayerNode { .. } => {
                    let indent = entry.depth as f32 * INDENT_PX * scale;
                    out.push(InstanceRaw {
                        position: [cx, y],
                        size: [content_w, item_h],
                        color: settings.theme.bg_plugin,
                        border_radius: 0.0,
                    });
                    let entry_entity_id = match &entry.kind {
                        EntryKind::LayerNode { id, .. } => Some(*id),
                        EntryKind::ProjectInstrument { id } => Some(*id),
                        _ => None,
                    };
                    if entry_entity_id.map_or(false, |id| selected_ids.contains(&id)) {
                        let a = settings.theme.accent;
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: [a[0], a[1], a[2], 0.22],
                            border_radius: 0.0,
                        });
                    }
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }
                    // Chevron for expandable nodes (not instruments — always expanded)
                    if let EntryKind::LayerNode { has_children: true, expanded, kind, .. } = &entry.kind {
                    if !matches!(kind, LayerNodeKind::Instrument) {
                        let chev_sz = 8.0 * scale;
                        let chev_x = cx + indent + 8.0 * scale + chev_sz * 0.5;
                        let cy_mid = y + item_h * 0.5;
                        if *expanded {
                            let bar_w = chev_sz * 0.7;
                            let bar_h = 1.5 * scale;
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w * 0.5, cy_mid - bar_h],
                                size: [bar_w, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w * 0.25, cy_mid],
                                size: [bar_w * 0.5, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                        } else {
                            let bar_w = 1.5 * scale;
                            let bar_h = chev_sz * 0.7;
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w, cy_mid - bar_h * 0.5],
                                size: [bar_w, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                            out.push(InstanceRaw {
                                position: [chev_x, cy_mid - bar_h * 0.25],
                                size: [bar_w, bar_h * 0.5],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                        }
                    }
                    }
                    // Category dot
                    let dot_sz = 5.0 * scale;
                    let dot_offset = indent + 20.0 * scale;
                    let dot_x = cx + dot_offset;
                    let dot_y = y + (item_h - dot_sz) * 0.5;
                    let dot_color = match &entry.kind {
                        EntryKind::LayerNode { kind, color, .. } => match kind {
                            LayerNodeKind::Instrument => settings.theme.pill_instrument,
                            LayerNodeKind::TextNote => settings.theme.category_dot,
                            LayerNodeKind::Group => settings.theme.component_border_color,
                            _ => *color,
                        },
                        _ => settings.theme.pill_instrument,
                    };
                    out.push(InstanceRaw {
                        position: [dot_x, dot_y],
                        size: [dot_sz, dot_sz],
                        color: dot_color,
                        border_radius: dot_sz * 0.5,
                    });
                    // Solo/Mute buttons (right-aligned) — only for Waveform, Instrument, Group
                    if let EntryKind::LayerNode { kind, is_soloed, is_muted, .. } = &entry.kind {
                        if matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Instrument | LayerNodeKind::Group) {
                            let row_right = cx + content_w;
                            let row_cy = y + item_h * 0.5;
                            let layout = super::solo_mute::layout_right_aligned(row_right, row_cy, scale);
                            let is_hovered = self.hovered_entry == Some(i);
                            out.extend(super::solo_mute::build_instances(&layout, *is_soloed, *is_muted, is_hovered, &settings.theme, scale));
                        }
                    }
                }
                EntryKind::Dir | EntryKind::File => {
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }

                    let indent = entry.depth as f32 * INDENT_PX * scale + 8.0 * scale;

                    // Chevron for directories
                    if entry.is_dir() {
                        let chev_sz = 8.0 * scale;
                        let chev_x = cx + indent + chev_sz * 0.5;
                        let cy = y + item_h * 0.5;

                        if self.is_expanded(&entry.path) {
                            let bar_w = chev_sz * 0.7;
                            let bar_h = 1.5 * scale;
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w * 0.5, cy - bar_h],
                                size: [bar_w, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w * 0.25, cy],
                                size: [bar_w * 0.5, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                        } else {
                            let bar_w = 1.5 * scale;
                            let bar_h = chev_sz * 0.7;
                            out.push(InstanceRaw {
                                position: [chev_x - bar_w, cy - bar_h * 0.5],
                                size: [bar_w, bar_h],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                            out.push(InstanceRaw {
                                position: [chev_x, cy - bar_h * 0.25],
                                size: [bar_w, bar_h * 0.5],
                                color: crate::theme::with_alpha(settings.theme.text_primary, 0.40),
                                border_radius: 0.0,
                            });
                        }
                    }
                }
            }
        }

        // --- Layer reorder drop indicator ---
        if let Some((indicator_row, indicator_depth, is_inside)) = self.layer_drop_indicator {
            let indent = indicator_depth as f32 * INDENT_PX * scale;
            if is_inside {
                // Highlight the group row with accent tint
                let row_y = ct + indicator_row as f32 * item_h - self.scroll_offset;
                if row_y + item_h > ct && row_y < screen_h {
                    out.push(InstanceRaw {
                        position: [cx + indent, row_y],
                        size: [content_w - indent, item_h],
                        color: [settings.theme.accent[0], settings.theme.accent[1], settings.theme.accent[2], 0.15],
                        border_radius: 2.0 * scale,
                    });
                }
            } else {
                // Horizontal insertion line between rows
                let line_y = ct + indicator_row as f32 * item_h - self.scroll_offset;
                let line_h = 2.0 * scale;
                if line_y > ct - line_h && line_y < screen_h {
                    out.push(InstanceRaw {
                        position: [cx + indent, line_y - line_h * 0.5],
                        size: [content_w - indent, line_h],
                        color: settings.theme.accent,
                        border_radius: 1.0 * scale,
                    });
                    // Small dot at left end
                    let dot = 6.0 * scale;
                    out.push(InstanceRaw {
                        position: [cx + indent - dot * 0.5, line_y - dot * 0.5],
                        size: [dot, dot],
                        color: settings.theme.accent,
                        border_radius: dot * 0.5,
                    });
                }
            }
        }

        // --- Scrollbar (in content area) ---
        let content_h = self.content_height(scale);
        if content_h > visible_h {
            let sb_x = w - SCROLLBAR_WIDTH * scale - 2.0 * scale;
            let sb_track_h = visible_h;

            out.push(InstanceRaw {
                position: [sb_x, ct],
                size: [SCROLLBAR_WIDTH * scale, sb_track_h],
                color: crate::theme::with_alpha(settings.theme.text_primary, 0.08),
                border_radius: 3.0 * scale,
            });

            let ratio = visible_h / content_h;
            let thumb_h = (ratio * sb_track_h).max(20.0 * scale);
            let scroll_ratio = if self.max_scroll(screen_h, scale) > 0.0 {
                self.scroll_offset / self.max_scroll(screen_h, scale)
            } else {
                0.0
            };
            let thumb_y = ct + scroll_ratio * (sb_track_h - thumb_h);

            out.push(InstanceRaw {
                position: [sb_x, thumb_y],
                size: [SCROLLBAR_WIDTH * scale, thumb_h],
                color: crate::theme::with_alpha(settings.theme.text_primary, 0.20),
                border_radius: 3.0 * scale,
            });
        }

        out
    }

    pub fn get_text_entries(&mut self, theme: &crate::theme::RuntimeTheme, screen_h: f32, scale: f32) -> &[TextEntry] {
        if self.text_dirty
            || (self.cached_screen_h - screen_h).abs() > 0.5
            || (self.cached_scale - scale).abs() > 0.001
            || (self.cached_text_primary_r - theme.text_primary[0]).abs() > 0.001
        {
            self.cached_text = self.build_text_entries(theme, screen_h, scale);
            self.cached_screen_h = screen_h;
            self.cached_scale = scale;
            self.cached_text_primary_r = theme.text_primary[0];
            self.text_dirty = false;
            self.text_generation += 1;
        }
        &self.cached_text
    }

    fn build_text_entries(&self, theme: &crate::theme::RuntimeTheme, _screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let sb_w = self.sidebar_width(scale);
        let cx = self.content_x(scale);
        let item_h = ITEM_HEIGHT * scale;

        // --- Search bar text (second row, below header) ---
        {
            let search_bar_h = SEARCH_BAR_HEIGHT * scale;
            let row_y = header_h;
            let margin = 6.0 * scale;
            let right_pad = if self.active_category == BrowserCategory::Samples {
                (ADD_BUTTON_SIZE + 10.0) * scale
            } else {
                margin
            };
            let sb_x = margin;
            let sb_w_inner = w - sb_x - right_pad;
            let font_sz = 11.0 * scale;
            let line_h = 14.0 * scale;
            let text_x = sb_x + 8.0 * scale;
            let text_y = row_y + (search_bar_h - line_h) * 0.5;
            let (text, color) = if self.search_focused || !self.search_query.is_empty() {
                let display = format!("{}|", self.search_query);
                (display, crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255))
            } else {
                ("Search...".to_string(), crate::theme::RuntimeTheme::text_u8(theme.text_dim, 160))
            };
            out.push(TextEntry {
                text,
                x: text_x,
                y: text_y,
                font_size: font_sz,
                line_height: line_h,
                max_width: sb_w_inner - if self.search_query.is_empty() { 16.0 } else { 32.0 } * scale,
                color,
                weight: 400,
                bounds: Some([sb_x, row_y, sb_x + sb_w_inner, row_y + search_bar_h]),
                center: false,
            });
        }

        let ct = self.content_top(scale);

        // --- Sidebar category labels (fixed, not scrolled) ---
        let sb_item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let section_gap = SIDEBAR_SECTION_GAP * scale;
        let font_sz = 12.0 * scale;
        let line_h = 15.0 * scale;

        for (i, cat) in SIDEBAR_CATEGORIES.iter().enumerate() {
            let y = ct + section_gap + i as f32 * sb_item_h;
            let is_active = *cat == self.active_category;
            let color = if is_active {
                crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255)
            } else {
                crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 200)
            };
            out.push(TextEntry {
                text: cat.label().to_string(),
                x: 12.0 * scale,
                y: y + (sb_item_h - line_h) * 0.5,
                font_size: font_sz,
                line_height: line_h,
                max_width: sb_w - 16.0 * scale,
                color,
                weight: if is_active { 600 } else { 400 },
                bounds: Some([0.0, 0.0, 0.0, 0.0]),
                center: false,
            });
        }

        // --- Content area entries ---
        for (i, entry) in self.entries.iter().enumerate() {
            let base_y = ct + i as f32 * item_h;

            match &entry.kind {
                EntryKind::PluginHeader => {
                    out.push(TextEntry {
                        text: entry.name.clone(),
                        x: cx + 30.0 * scale,
                        y: base_y + (item_h - 12.0 * scale) * 0.5,
                        font_size: 10.0 * scale,
                        line_height: 12.0 * scale,
                        max_width: w * 0.6,
                        color: crate::theme::RuntimeTheme::text_u8(theme.text_dim, 200),
                        weight: 600,
                        bounds: None,
                        center: false,
                    });
                }
                EntryKind::Plugin { is_instrument, .. } => {
                    let text_x = cx + 22.0 * scale;
                    let font_sz = 12.0 * scale;
                    let line_h = 16.0 * scale;
                    let color = if *is_instrument {
                        crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230)
                    } else {
                        crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 240)
                    };
                    out.push(TextEntry {
                        text: entry.name.clone(),
                        x: text_x,
                        y: base_y + (item_h - line_h) * 0.5,
                        font_size: font_sz,
                        line_height: line_h,
                        max_width: w - text_x - 12.0 * scale,
                        color,
                        weight: 400,
                        bounds: None,
                        center: false,
                    });
                }
                EntryKind::ProjectInstrument { .. } | EntryKind::LayerNode { .. } => {
                    let indent = entry.depth as f32 * INDENT_PX * scale;
                    let text_offset = indent + 28.0 * scale;
                    let text_x = cx + text_offset;
                    let font_sz = 12.0 * scale;
                    let line_h = 16.0 * scale;
                    let color = match &entry.kind {
                        EntryKind::LayerNode { kind, .. } => match kind {
                            LayerNodeKind::Instrument => crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230),
                            LayerNodeKind::MidiClip => crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 230),
                            LayerNodeKind::Waveform => crate::theme::RuntimeTheme::text_u8(theme.text_primary, 240),
                            LayerNodeKind::TextNote => crate::theme::RuntimeTheme::text_u8(theme.text_dim, 255),
                            LayerNodeKind::Group => crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230),
                        },
                        _ => crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230),
                    };
                    let entry_id = match &entry.kind {
                        EntryKind::LayerNode { id, .. } => Some(*id),
                        EntryKind::ProjectInstrument { id } => Some(*id),
                        _ => None,
                    };
                    let (display_text, display_color) = match (entry_id, &self.editing_browser_name) {
                        (Some(eid), Some((edit_id, _, text))) if eid == *edit_id => {
                            (format!("{}|", text), [255u8, 255, 255, 255])
                        }
                        _ => (entry.name.clone(), color),
                    };
                    out.push(TextEntry {
                        text: display_text,
                        x: text_x,
                        y: base_y + (item_h - line_h) * 0.5,
                        font_size: font_sz,
                        line_height: line_h,
                        max_width: w - text_x - 12.0 * scale,
                        color: display_color,
                        weight: 400,
                        bounds: None,
                        center: false,
                    });
                    // Solo/Mute button labels
                    if let EntryKind::LayerNode { kind, is_soloed, is_muted, .. } = &entry.kind {
                        if matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Instrument | LayerNodeKind::Group) {
                            let row_right = cx + self.content_width(scale);
                            let row_cy = base_y + item_h * 0.5;
                            let layout = super::solo_mute::layout_right_aligned(row_right, row_cy, scale);
                            out.extend(super::solo_mute::build_text_entries(&layout, *is_soloed, *is_muted, theme, scale));
                        }
                    }
                }
                EntryKind::Dir | EntryKind::File => {
                    let indent = entry.depth as f32 * INDENT_PX * scale + 8.0 * scale;
                    let text_x = cx + indent + 18.0 * scale;
                    let font_sz = 13.0 * scale;
                    let line_h = 18.0 * scale;

                    let (color, weight) = if entry.is_dir() {
                        (crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255), 400u16)
                    } else {
                        (crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 255), 400u16)
                    };

                    out.push(TextEntry {
                        text: entry.name.clone(),
                        x: text_x,
                        y: base_y + (item_h - line_h) * 0.5,
                        font_size: font_sz,
                        line_height: line_h,
                        max_width: w - text_x - 12.0 * scale,
                        color,
                        weight,
                        bounds: None,
                        center: false,
                    });
                }
            }
        }

        out
    }
}

fn walk_dir(
    entries: &mut Vec<BrowserEntry>,
    expanded: &HashSet<PathBuf>,
    dir: &PathBuf,
    depth: usize,
) {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

    entries.push(BrowserEntry {
        path: dir.clone(),
        name,
        kind: EntryKind::Dir,
        depth,
    });

    if !expanded.contains(dir) {
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
            walk_dir(entries, expanded, &path, depth + 1);
        } else {
            entries.push(BrowserEntry {
                path,
                name: fname,
                kind: EntryKind::File,
                depth: depth + 1,
            });
        }
    }
}

/// Fuzzy match: returns true if all characters of `needle` appear in `haystack`
/// in order (case-insensitive). Empty needle always matches.
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut h_chars = haystack.chars();
    'outer: for nc in needle.chars() {
        let nc_lo = nc.to_lowercase().next().unwrap_or(nc);
        loop {
            match h_chars.next() {
                Some(hc) => {
                    if hc.to_lowercase().next().unwrap_or(hc) == nc_lo {
                        continue 'outer;
                    }
                }
                None => return false,
            }
        }
    }
    true
}

/// Fuzzy match for pre-lowercased strings (no per-char lowercase conversion).
fn fuzzy_match_lowered(haystack_lower: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let mut h_chars = haystack_lower.chars();
    'outer: for nc in needle_lower.chars() {
        loop {
            match h_chars.next() {
                Some(hc) => {
                    if hc == nc {
                        continue 'outer;
                    }
                }
                None => return false,
            }
        }
    }
    true
}


use crate::gpu::TextEntry;
