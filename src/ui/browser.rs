use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

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
const SEARCH_BAR_HEIGHT: f32 = 36.0;
const SIDEBAR_WIDTH: f32 = 110.0;
const SIDEBAR_ITEM_HEIGHT: f32 = 26.0;
const SIDEBAR_SECTION_GAP: f32 = 18.0;
const INDENT_PX: f32 = 16.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const COLLAPSED_WIDTH: f32 = 20.0;
/// Vertical gap between the last category item and the "Places" section header inside the sidebar.
const PLACES_SECTION_GAP: f32 = 8.0;
/// Height of the "Places" label row inside the sidebar.
const PLACES_HEADER_HEIGHT: f32 = 20.0;
/// Row height for each entry in the places section of the sidebar.
const PLACES_ROW_HEIGHT: f32 = 24.0;
/// Height of the sample preview strip at the bottom of the browser.
pub const PREVIEW_STRIP_HEIGHT: f32 = 52.0;


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
            BrowserCategory::Effects => "Audio Effects",
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
    LayerNode { id: EntityId, kind: LayerNodeKind, has_children: bool, expanded: bool, color: [f32; 4], is_soloed: bool, is_muted: bool, is_monitoring: bool },
    Master,
    EmptyState,
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
const SEARCH_DEBOUNCE_MS: u64 = 150;

pub struct SampleBrowser {
    pub root_folders: Vec<PathBuf>,
    pub expanded: HashSet<PathBuf>,
    pub entries: Vec<BrowserEntry>,
    pub scroll_offset: f32,
    pub scroll_velocity: f32,
    pub hovered_entry: Option<usize>,
    pub visible: bool,
    pub width: f32,
    pub resize_hovered: bool,
    pub text_dirty: bool,
    /// When true, only the search-bar text entry (index 0) needs updating (cursor blink).
    pub cursor_text_dirty: bool,
    pub cached_text: Vec<TextEntry>,
    pub text_generation: u64,
    /// Incremented when only the search-bar cursor text entry changes (fast-path for GPU).
    pub cursor_text_generation: u64,
    /// Per-frame S/M/I text overlay for the hovered entry (cheap, max 3 entries).
    pub hover_sm_text: Vec<TextEntry>,
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
    pub editing_browser_name: Option<(crate::entity_id::EntityId, LayerNodeKind, crate::ui::text_input::TextInput)>,
    /// Search bar input state.
    pub search_input: crate::ui::text_input::TextInput,
    /// Whether the search bar is focused (accepting keyboard input).
    pub search_focused: bool,
    /// Cached flat file index for fast sample search.
    file_index: Vec<CachedFile>,
    /// Whether the file index needs rebuilding (root folders changed).
    file_index_dirty: bool,
    /// Receiver for background file index build.
    file_index_receiver: Option<mpsc::Receiver<Vec<CachedFile>>>,
    /// Whether a background file index build is in progress.
    file_index_building: bool,
    /// When set, a search rebuild is pending and should fire after this deadline.
    search_debounce_deadline: Option<TimeInstant>,
    /// Whether the search clear (X) button is hovered.
    pub search_clear_hovered: bool,
    /// Drop indicator for layer reorder drag: (flat_row_index, depth, is_inside_group).
    pub layer_drop_indicator: Option<(usize, usize, bool)>,
    /// Whether the browser toggle button (≡ in header or collapsed strip) is hovered.
    pub toggle_hovered: bool,
    /// Index of the currently selected place (root folder) in the Places column.
    pub selected_place: usize,
    /// Which place row (0-based into root_folders) is hovered; None if none.
    pub hovered_place: Option<usize>,
    /// Whether the "Add Folder…" row at the bottom of the places column is hovered.
    pub places_add_hovered: bool,
    /// Audio data for the currently previewed sample (waveform display).
    pub preview_audio: Option<Arc<super::waveform::AudioData>>,
    /// Path of the currently previewed sample.
    pub preview_path: Option<PathBuf>,
    /// Whether auto-preview (headphones) is enabled — click plays the sample.
    pub auto_preview: bool,
    /// Whether the headphones toggle button is hovered.
    pub preview_toggle_hovered: bool,
    /// Index of the selected entry in the Samples tab (for preview highlight).
    pub selected_entry: Option<usize>,
    /// Whether the Master ("Main") entry is currently selected.
    pub master_selected: bool,
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
            width: DEFAULT_BROWSER_WIDTH,
            resize_hovered: false,
            text_dirty: true,
            cursor_text_dirty: false,
            cached_text: Vec::new(),
            text_generation: 0,
            cursor_text_generation: 0,
            hover_sm_text: Vec::new(),
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
            search_input: crate::ui::text_input::TextInput::new(crate::ui::text_input::TextInputConfig::default()),
            search_focused: false,
            file_index: Vec::new(),
            file_index_dirty: true,
            file_index_receiver: None,
            file_index_building: false,
            search_debounce_deadline: None,
            search_clear_hovered: false,
            layer_drop_indicator: None,
            toggle_hovered: false,
            selected_place: 0,
            hovered_place: None,
            places_add_hovered: false,
            preview_audio: None,
            preview_path: None,
            auto_preview: true,
            preview_toggle_hovered: false,
            selected_entry: None,
            master_selected: false,
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
        self.selected_place = self.root_folders.len() - 1;
        self.file_index_dirty = true;
        self.rebuild_entries();
    }

    pub fn remove_folder(&mut self, index: usize) {
        if index < self.root_folders.len() {
            let removed = self.root_folders.remove(index);
            self.expanded.remove(&removed);
            if self.selected_place >= self.root_folders.len() && !self.root_folders.is_empty() {
                self.selected_place = self.root_folders.len() - 1;
            } else if self.root_folders.is_empty() {
                self.selected_place = 0;
            }
            self.file_index_dirty = true;
            self.rebuild_entries();
        }
    }

    /// Poll for a completed background file index build.
    /// Returns true if the index was updated (caller should request redraw).
    pub fn tick_file_index(&mut self) -> bool {
        if let Some(rx) = &self.file_index_receiver {
            if let Ok(index) = rx.try_recv() {
                self.file_index = index;
                self.file_index_dirty = false;
                self.file_index_building = false;
                self.file_index_receiver = None;
                // Re-run search with the new index.
                if !self.search_input.text.is_empty() {
                    self.rebuild_entries();
                }
                return true;
            }
        }
        false
    }

    /// Ensure the file index is available. If dirty, kicks off a background
    /// thread to walk directories without blocking the UI.
    fn ensure_file_index(&mut self) {
        if !self.file_index_dirty || self.file_index_building {
            return;
        }
        self.file_index.clear();
        self.file_index_building = true;
        let roots = self.root_folders.clone();
        let (tx, rx) = mpsc::channel();
        self.file_index_receiver = Some(rx);
        std::thread::spawn(move || {
            let mut index = Vec::new();
            for root in &roots {
                index_walk_dir(&mut index, root);
            }
            let _ = tx.send(index);
        });
    }

    /// Returns true if a background file index build is in progress.
    pub fn is_file_index_building(&self) -> bool {
        self.file_index_building
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
        let query = self.search_input.text.clone();
        let searching = !query.is_empty();
        let query_lower = query.to_lowercase();
        match self.active_category {
            BrowserCategory::Layers => {
                // Pinned "Main" layer row at the top
                if !searching {
                    self.entries.push(BrowserEntry {
                        path: PathBuf::new(),
                        name: "Main".to_string(),
                        kind: EntryKind::Master,
                        depth: 0,
                    });
                }
                for row in &self.layer_rows {
                    if searching && !fuzzy_match_lowered(&row.label.to_lowercase(), &query_lower) {
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
                            is_monitoring: row.is_monitoring,
                        },
                        depth: if searching { 0 } else { row.depth },
                    });
                }
                if self.layer_rows.is_empty() && !searching {
                    self.entries.push(BrowserEntry {
                        path: PathBuf::new(),
                        name: String::new(),
                        kind: EntryKind::EmptyState,
                        depth: 1,
                    });
                }
            }
            BrowserCategory::Samples => {
                if searching && query.len() >= 2 {
                    self.ensure_file_index();
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
                } else if let Some(root) = self.root_folders.get(self.selected_place) {
                    let root = root.clone();
                    walk_dir(&mut self.entries, &self.expanded, &root, 0);
                }
            }
            BrowserCategory::Instruments => {
                for inst in &self.instruments {
                    if searching && !fuzzy_match_lowered(&inst.name.to_lowercase(), &query_lower) {
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
                    if searching && !fuzzy_match_lowered(&plug.name.to_lowercase(), &query_lower) {
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

    /// Left edge of the file-tree pane. Places now live in the sidebar, so this is just content_x.
    pub(crate) fn tree_content_x(&self, scale: f32) -> f32 {
        self.content_x(scale)
    }

    fn tree_content_width(&self, scale: f32) -> f32 {
        self.panel_width(scale) - self.content_x(scale)
    }

    pub(crate) fn content_width(&self, scale: f32) -> f32 {
        self.panel_width(scale) - self.sidebar_width(scale)
    }

    /// Y coordinate where the Places section starts inside the sidebar (below all category rows).
    fn places_section_y(&self, scale: f32) -> f32 {
        let ct = self.content_top(scale);
        let section_gap = SIDEBAR_SECTION_GAP * scale;
        let item_h = SIDEBAR_ITEM_HEIGHT * scale;
        ct + section_gap + SIDEBAR_CATEGORIES.len() as f32 * item_h + PLACES_SECTION_GAP * scale
    }

    fn content_height(&self, scale: f32) -> f32 {
        self.entries.len() as f32 * ITEM_HEIGHT * scale
    }

    pub(crate) fn content_top(&self, scale: f32) -> f32 {
        (HEADER_HEIGHT + SEARCH_BAR_HEIGHT) * scale
    }

    /// Returns the tooltip anchor rect `(pos, size)` if the mouse is hovering the group icon
    /// of the currently hovered entry (Layers tab, Group rows only).
    pub fn hovered_group_icon_rect(&self, mouse_pos: [f32; 2], scale: f32) -> Option<([f32; 2], [f32; 2])> {
        let idx = self.hovered_entry?;
        let entry = self.entries.get(idx)?;
        if !matches!(entry.kind, EntryKind::LayerNode { kind: LayerNodeKind::Group, .. }) {
            return None;
        }
        let item_h = ITEM_HEIGHT * scale;
        let ct = self.content_top(scale);
        let y = ct + idx as f32 * item_h - self.scroll_offset;
        let indent = entry.depth as f32 * INDENT_PX * scale;
        let cx = self.content_x(scale);
        let icon_sz = 10.0 * scale;
        let icon_x = cx + indent + 20.0 * scale - icon_sz * 0.5;
        let icon_y = y + (item_h - icon_sz) * 0.5;
        let hit_pad = 4.0 * scale;
        if mouse_pos[0] >= icon_x - hit_pad && mouse_pos[0] <= icon_x + icon_sz + hit_pad
            && mouse_pos[1] >= icon_y - hit_pad && mouse_pos[1] <= icon_y + icon_sz + hit_pad
        {
            Some(([icon_x, icon_y], [icon_sz, icon_sz]))
        } else {
            None
        }
    }

    fn visible_height(&self, screen_h: f32, scale: f32) -> f32 {
        let preview_h = if self.preview_audio.is_some() {
            PREVIEW_STRIP_HEIGHT * scale
        } else {
            0.0
        };
        screen_h - self.content_top(scale) - preview_h
    }

    #[cfg(test)]
    pub fn visible_height_for_test(&self, screen_h: f32, scale: f32) -> f32 {
        self.visible_height(screen_h, scale)
    }

    pub fn preview_strip_rect(&self, screen_h: f32, scale: f32) -> [f32; 4] {
        let strip_h = PREVIEW_STRIP_HEIGHT * scale;
        let w = self.width * scale;
        [0.0, screen_h - strip_h, w, strip_h]
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

    /// Scroll to ensure the given entry index is visible in the content area.
    pub fn scroll_to_entry(&mut self, idx: usize, screen_h: f32, scale: f32) {
        let item_h = ITEM_HEIGHT * scale;
        let entry_top = idx as f32 * item_h;
        let entry_bot = entry_top + item_h;
        let vis_h = self.visible_height(screen_h, scale);

        if entry_top < self.scroll_offset {
            self.scroll_offset = entry_top;
        } else if entry_bot > self.scroll_offset + vis_h {
            self.scroll_offset = entry_bot - vis_h;
        }
        self.clamp_scroll_for_screen(screen_h, scale);
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
        if self.visible {
            self.width * scale
        } else {
            COLLAPSED_WIDTH * scale
        }
    }

    pub fn hit_resize_handle(&self, pos: [f32; 2], scale: f32) -> bool {
        if !self.visible {
            return false;
        }
        let edge = self.panel_width(scale);
        let zone = RESIZE_HANDLE_PX * scale;
        pos[0] >= edge - zone && pos[0] <= edge + zone
    }

    /// Hit test for the browser toggle button:
    /// - When visible: the `◄` button in the search bar row (left side).
    /// - When collapsed: the entire 16px strip.
    pub fn hit_toggle_button(&self, pos: [f32; 2], scale: f32) -> bool {
        if self.visible {
            false
        } else {
            pos[0] >= 0.0 && pos[0] <= COLLAPSED_WIDTH * scale
        }
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

    /// Rect for the search clear (X) button, inside the search input on the right.
    fn clear_button_rect(&self, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = self.panel_width(scale);
        let margin = 6.0 * scale;
        let sb_x = margin;
        let row_y = HEADER_HEIGHT * scale;
        let search_bar_h = SEARCH_BAR_HEIGHT * scale;
        let sb_y = row_y + margin;
        let sb_h = search_bar_h - 2.0 * margin;
        let sb_w_inner = w - sb_x - margin;
        let btn_sz = 16.0 * scale;
        let btn_x = sb_x + sb_w_inner - btn_sz - 2.0 * scale;
        let btn_y = sb_y + (sb_h - btn_sz) * 0.5;
        ([btn_x, btn_y], [btn_sz, btn_sz])
    }

    pub fn hit_clear_button(&self, pos: [f32; 2], scale: f32) -> bool {
        if self.search_input.text.is_empty() {
            return false;
        }
        let (bp, bs) = self.clear_button_rect(scale);
        pos[0] >= bp[0] && pos[0] <= bp[0] + bs[0] && pos[1] >= bp[1] && pos[1] <= bp[1] + bs[1]
    }

    /// Returns the index into `root_folders` for the place row at `pos`, or `None`.
    pub fn hit_place_row(&self, pos: [f32; 2], scale: f32) -> Option<usize> {
        if self.active_category != BrowserCategory::Samples {
            return None;
        }
        let sb_w = self.sidebar_width(scale);
        if pos[0] < 0.0 || pos[0] >= sb_w {
            return None;
        }
        let places_y = self.places_section_y(scale);
        let header_h = PLACES_HEADER_HEIGHT * scale;
        let row_h = PLACES_ROW_HEIGHT * scale;
        let y = pos[1] - places_y - header_h;
        if y < 0.0 {
            return None;
        }
        let idx = (y / row_h) as usize;
        if idx < self.root_folders.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Returns true if `pos` is over the "Add Folder…" row in the sidebar.
    pub fn hit_places_add(&self, pos: [f32; 2], scale: f32) -> bool {
        if self.active_category != BrowserCategory::Samples {
            return false;
        }
        let sb_w = self.sidebar_width(scale);
        if pos[0] < 0.0 || pos[0] >= sb_w {
            return false;
        }
        let places_y = self.places_section_y(scale);
        let header_h = PLACES_HEADER_HEIGHT * scale;
        let row_h = PLACES_ROW_HEIGHT * scale;
        let add_row_y = places_y + header_h + self.root_folders.len() as f32 * row_h;
        pos[1] >= add_row_y && pos[1] < add_row_y + row_h
    }

    pub fn hit_search_bar(&self, pos: [f32; 2], scale: f32) -> bool {
        let top = HEADER_HEIGHT * scale;
        let bottom = top + SEARCH_BAR_HEIGHT * scale;
        let left = 6.0 * scale;
        pos[1] >= top
            && pos[1] < bottom
            && pos[0] >= left
            && pos[0] < self.panel_width(scale)
            && !self.hit_clear_button(pos, scale)
    }

    pub fn get_search_clear_icon_entry(&self, theme: &crate::theme::RuntimeTheme, scale: f32) -> Option<crate::gpu::IconEntry> {
        if self.search_input.text.is_empty() {
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

    /// Icon entry for the browser toggle button (only shown when collapsed).
    pub fn get_toggle_icon_entry(&self, theme: &crate::theme::RuntimeTheme, scale: f32, screen_h: f32) -> Option<crate::gpu::IconEntry> {
        if self.visible {
            return None;
        }
        let icon_size = 14.0 * scale;
        let color = if self.toggle_hovered {
            crate::theme::RuntimeTheme::text_u8(theme.text_primary, 200)
        } else {
            crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 130)
        };
        let strip_w = COLLAPSED_WIDTH * scale;
        let x = (strip_w - icon_size) * 0.5;
        let y = (screen_h - icon_size) * 0.5;
        Some(crate::gpu::IconEntry {
            codepoint: crate::icons::CHEVRON_RIGHT,
            x,
            y,
            size: icon_size,
            color,
        })
    }

    /// Icon entries for Places in the sidebar: a folder icon per place row + folder-add for "Add Folder…".
    pub fn get_places_icon_entries(&self, theme: &crate::theme::RuntimeTheme, scale: f32) -> Vec<crate::gpu::IconEntry> {
        if self.active_category != BrowserCategory::Samples || !self.visible {
            return Vec::new();
        }
        let mut out = Vec::new();
        let places_y = self.places_section_y(scale);
        let header_h = PLACES_HEADER_HEIGHT * scale;
        let row_h = PLACES_ROW_HEIGHT * scale;
        let icon_sz = 12.0 * scale;
        let icon_x = 8.0 * scale;

        for (i, _) in self.root_folders.iter().enumerate() {
            let row_y = places_y + header_h + i as f32 * row_h;
            let icon_y = row_y + (row_h - icon_sz) * 0.5;
            let color = if i == self.selected_place {
                crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230)
            } else if self.hovered_place == Some(i) {
                crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 200)
            } else {
                crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 160)
            };
            out.push(crate::gpu::IconEntry {
                codepoint: crate::icons::FOLDER,
                x: icon_x,
                y: icon_y,
                size: icon_sz,
                color,
            });
        }

        // "Add Folder…" row icon
        let add_row_y = places_y + header_h + self.root_folders.len() as f32 * row_h;
        let icon_y = add_row_y + (row_h - icon_sz) * 0.5;
        let add_color = if self.places_add_hovered {
            crate::theme::RuntimeTheme::text_u8(theme.text_primary, 200)
        } else {
            crate::theme::RuntimeTheme::text_u8(theme.text_dim, 140)
        };
        out.push(crate::gpu::IconEntry {
            codepoint: crate::icons::CREATE_NEW_FOLDER,
            x: icon_x,
            y: icon_y,
            size: icon_sz,
            color: add_color,
        });

        out
    }

    /// Returns which content-area entry the position is over, if any.
    /// Only considers positions in the tree pane (right of sidebar + places column).
    pub fn item_at(&self, pos: [f32; 2], _screen_h: f32, scale: f32) -> Option<usize> {
        let top = self.content_top(scale);
        let cx = self.tree_content_x(scale);
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
        let item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let idx = (y / item_h) as usize;
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
        let item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let idx = (y / item_h) as usize;
        if idx < SIDEBAR_CATEGORIES.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_h: f32, scale: f32) {
        self.toggle_hovered = if self.visible { false } else { self.hit_toggle_button(pos, scale) };
        if !self.visible {
            return;
        }
        self.resize_hovered = self.hit_resize_handle(pos, scale);
        self.search_clear_hovered = self.hit_clear_button(pos, scale);
        self.hovered_sidebar = self.sidebar_item_at(pos, scale);
        self.hovered_place = self.hit_place_row(pos, scale);
        self.places_add_hovered = self.hit_places_add(pos, scale);
        let new_hovered = if self.resize_hovered || self.hovered_sidebar.is_some()
            || self.hovered_place.is_some() || self.places_add_hovered
        {
            None
        } else {
            self.item_at(pos, screen_h, scale)
        };
        if new_hovered != self.hovered_entry {
            self.hovered_entry = new_hovered;
            self.rebuild_hover_sm_text(scale);
        }
        // Preview toggle hover
        if self.preview_audio.is_some() {
            let [bx, by, bw, bh] = self.preview_toggle_rect(screen_h, scale);
            self.preview_toggle_hovered = pos[0] >= bx && pos[0] <= bx + bw
                && pos[1] >= by && pos[1] <= by + bh;
        } else {
            self.preview_toggle_hovered = false;
        }
    }

    /// Rebuild the small hover-only S/M/I text overlay for the current hovered entry.
    fn rebuild_hover_sm_text(&mut self, scale: f32) {
        self.hover_sm_text.clear();
        let i = match self.hovered_entry {
            Some(idx) => idx,
            None => return,
        };
        let entry = match self.entries.get(i) {
            Some(e) => e,
            None => return,
        };
        if let EntryKind::LayerNode { kind, is_soloed, is_muted, is_monitoring, .. } = &entry.kind {
            if !matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Instrument | LayerNodeKind::Group) {
                return;
            }
            // Skip if already persistently visible (handled by cached text)
            if *is_soloed || *is_muted || *is_monitoring {
                return;
            }
            let ct = self.content_top(scale);
            let item_h = ITEM_HEIGHT * scale;
            let cx = self.content_x(scale);
            let row_right = cx + self.content_width(scale);
            let row_cy = ct + i as f32 * item_h + item_h * 0.5;
            let layout = super::solo_mute::layout_right_aligned(row_right, row_cy, scale);
            let show_mon = matches!(kind, LayerNodeKind::Group | LayerNodeKind::Instrument);
            let theme = crate::theme::RuntimeTheme::default();
            self.hover_sm_text = super::solo_mute::build_text_entries(
                &layout, false, false, false, true, show_mon, &theme, scale,
            );
        }
    }

    pub fn build_instances(&self, settings: &crate::settings::Settings, _screen_w: f32, screen_h: f32, scale: f32, selected_ids: &std::collections::HashSet<crate::entity_id::EntityId>) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);

        // --- Collapsed strip ---
        if !self.visible {
            out.push(InstanceRaw {
                position: [0.0, 0.0],
                size: [w, screen_h],
                color: settings.theme.bg_surface,
                border_radius: 0.0,
            });
            if self.toggle_hovered {
                out.push(InstanceRaw {
                    position: [0.0, 0.0],
                    size: [w, screen_h],
                    color: [1.0, 1.0, 1.0, 0.06],
                    border_radius: 0.0,
                });
            }
            out.push(InstanceRaw {
                position: [w - 1.0 * scale, 0.0],
                size: [1.0 * scale, screen_h],
                color: [1.0, 1.0, 1.0, 0.07],
                border_radius: 0.0,
            });
            return out;
        }

        let header_h = HEADER_HEIGHT * scale;
        let sb_w = self.sidebar_width(scale);
        let item_h = ITEM_HEIGHT * scale;

        // cx and content_w span the full tree pane (right of sidebar — no separate places column)
        let cx = self.content_x(scale);
        let content_w = self.tree_content_width(scale);

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
            color: settings.theme.bg_base,
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
                color: settings.theme.bg_base,
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
            let sb_x = margin;
            let sb_y = row_y + margin;
            let sb_w_inner = w - sb_x - margin;
            let sb_h = search_bar_h - 2.0 * margin;
            let bar_color = crate::theme::with_alpha(settings.theme.shadow, settings.theme.shadow[3] * 0.6);
            out.push(InstanceRaw {
                position: [sb_x, sb_y],
                size: [sb_w_inner, sb_h],
                color: bar_color,
                border_radius: 6.0 * scale,
            });


            // Search clear (X) button hover highlight
            if !self.search_input.text.is_empty() && self.search_clear_hovered {
                let (cp, cs) = self.clear_button_rect(scale);
                out.push(InstanceRaw {
                    position: cp,
                    size: cs,
                    color: settings.theme.item_hover,
                    border_radius: cs[0] * 0.5,
                });
            }
        }

        // --- Sidebar background (slightly darker) ---
        let ct = self.content_top(scale);
        out.push(InstanceRaw {
            position: [0.0, ct],
            size: [sb_w, screen_h - ct],
            color: settings.theme.bg_base,
            border_radius: 0.0,
        });

        // --- Sidebar items (category tabs) ---
        let sb_item_h = SIDEBAR_ITEM_HEIGHT * scale;

        for (i, cat) in SIDEBAR_CATEGORIES.iter().enumerate() {
            let y = ct + i as f32 * sb_item_h;

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

        // --- Places section inside sidebar (Samples category only) ---
        if self.active_category == BrowserCategory::Samples {
            let places_y = self.places_section_y(scale);
            let ph = PLACES_HEADER_HEIGHT * scale;
            let row_h = PLACES_ROW_HEIGHT * scale;

            // Thin separator above "Places" label
            out.push(InstanceRaw {
                position: [8.0 * scale, places_y - 5.0 * scale],
                size: [sb_w - 16.0 * scale, 1.0 * scale],
                color: [1.0, 1.0, 1.0, 0.07],
                border_radius: 0.0,
            });

            for (i, _) in self.root_folders.iter().enumerate() {
                let row_y = places_y + ph + i as f32 * row_h;
                if i == self.selected_place {
                    out.push(InstanceRaw {
                        position: [0.0, row_y],
                        size: [sb_w, row_h],
                        color: [
                            settings.theme.accent[0],
                            settings.theme.accent[1],
                            settings.theme.accent[2],
                            0.12,
                        ],
                        border_radius: 0.0,
                    });
                    out.push(InstanceRaw {
                        position: [0.0, row_y],
                        size: [2.5 * scale, row_h],
                        color: settings.theme.accent,
                        border_radius: 0.0,
                    });
                } else if self.hovered_place == Some(i) {
                    out.push(InstanceRaw {
                        position: [0.0, row_y],
                        size: [sb_w, row_h],
                        color: settings.theme.item_hover,
                        border_radius: 0.0,
                    });
                }
            }

            // "Add Folder…" row hover
            let add_row_y = places_y + ph + self.root_folders.len() as f32 * row_h;
            if self.places_add_hovered {
                out.push(InstanceRaw {
                    position: [0.0, add_row_y],
                    size: [sb_w, row_h],
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

        for i in first_visible..last_visible {
            let entry = &self.entries[i];
            let y = ct + i as f32 * item_h - self.scroll_offset;

            if y + item_h <= ct || y > screen_h {
                continue;
            }

            // Clamp row geometry to content area top (same clipping as text)
            let clip_y = y.max(ct);
            let clip_h = (y + item_h - clip_y).max(0.0);

            match &entry.kind {
                EntryKind::PluginHeader => {
                    // Section separator
                    out.push(InstanceRaw {
                        position: [cx, clip_y],
                        size: [content_w, 1.0 * scale],
                        color: [1.0, 1.0, 1.0, 0.07],
                        border_radius: 0.0,
                    });
                    // Section header background
                    out.push(InstanceRaw {
                        position: [cx, clip_y],
                        size: [content_w, clip_h],
                        color: settings.theme.bg_base,
                        border_radius: 0.0,
                    });
                    // Badge
                    let badge_w = 18.0 * scale;
                    let badge_h = 12.0 * scale;
                    let badge_x = cx + 8.0 * scale;
                    let badge_y = y + (item_h - badge_h) * 0.5;
                    if badge_y >= ct {
                        out.push(InstanceRaw {
                            position: [badge_x, badge_y],
                            size: [badge_w, badge_h],
                            color: settings.theme.accent_muted,
                            border_radius: 2.0 * scale,
                        });
                    }
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, clip_y],
                            size: [content_w, clip_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }
                }
                EntryKind::Plugin { is_instrument, .. } => {
                    // Plugin row background
                    out.push(InstanceRaw {
                        position: [cx, clip_y],
                        size: [content_w, clip_h],
                        color: settings.theme.bg_base,
                        border_radius: 0.0,
                    });
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, clip_y],
                            size: [content_w, clip_h],
                            color: settings.theme.item_hover,
                            border_radius: 0.0,
                        });
                    }
                }
                EntryKind::EmptyState => {}
                EntryKind::ProjectInstrument { .. } | EntryKind::LayerNode { .. } | EntryKind::Master => {
                    let indent = entry.depth as f32 * INDENT_PX * scale;
                    out.push(InstanceRaw {
                        position: [cx, clip_y],
                        size: [content_w, clip_h],
                        color: settings.theme.bg_base,
                        border_radius: 0.0,
                    });
                    let entry_entity_id = match &entry.kind {
                        EntryKind::LayerNode { id, .. } => Some(*id),
                        EntryKind::ProjectInstrument { id } => Some(*id),
                        _ => None,
                    };
                    let is_selected = entry_entity_id.map_or(false, |id| selected_ids.contains(&id))
                        || (matches!(entry.kind, EntryKind::Master) && self.master_selected);
                    if is_selected {
                        let a = settings.theme.accent;
                        out.push(InstanceRaw {
                            position: [cx, clip_y],
                            size: [content_w, clip_h],
                            color: [a[0], a[1], a[2], 0.22],
                            border_radius: 0.0,
                        });
                    }
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, clip_y],
                            size: [content_w, clip_h],
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
                        if cy_mid >= ct {
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
                    }
                    // Group icon (dashed rectangle, Figma-style)
                    if let EntryKind::LayerNode { kind: LayerNodeKind::Group, .. } = &entry.kind {
                        let icon_sz = 10.0 * scale;
                        let icon_x = cx + indent + 20.0 * scale - icon_sz * 0.5;
                        let icon_y = y + (item_h - icon_sz) * 0.5;
                        if icon_y >= ct {
                            let bar_t = 1.5 * scale;
                            let dash = icon_sz * 0.4;
                            let col = crate::theme::with_alpha(settings.theme.text_primary, 0.50);
                            // Top-left corner
                            out.push(InstanceRaw { position: [icon_x, icon_y], size: [dash, bar_t], color: col, border_radius: 0.0 });
                            out.push(InstanceRaw { position: [icon_x, icon_y], size: [bar_t, dash], color: col, border_radius: 0.0 });
                            // Top-right corner
                            out.push(InstanceRaw { position: [icon_x + icon_sz - dash, icon_y], size: [dash, bar_t], color: col, border_radius: 0.0 });
                            out.push(InstanceRaw { position: [icon_x + icon_sz - bar_t, icon_y], size: [bar_t, dash], color: col, border_radius: 0.0 });
                            // Bottom-left corner
                            out.push(InstanceRaw { position: [icon_x, icon_y + icon_sz - bar_t], size: [dash, bar_t], color: col, border_radius: 0.0 });
                            out.push(InstanceRaw { position: [icon_x, icon_y + icon_sz - dash], size: [bar_t, dash], color: col, border_radius: 0.0 });
                            // Bottom-right corner
                            out.push(InstanceRaw { position: [icon_x + icon_sz - dash, icon_y + icon_sz - bar_t], size: [dash, bar_t], color: col, border_radius: 0.0 });
                            out.push(InstanceRaw { position: [icon_x + icon_sz - bar_t, icon_y + icon_sz - dash], size: [bar_t, dash], color: col, border_radius: 0.0 });
                        }
                    }
                    // Solo/Mute buttons (right-aligned) — only for Waveform, Instrument, Group
                    if let EntryKind::LayerNode { kind, is_soloed, is_muted, is_monitoring, .. } = &entry.kind {
                        if matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Instrument | LayerNodeKind::Group) {
                            let row_right = cx + content_w;
                            let row_cy = y + item_h * 0.5;
                            let layout = super::solo_mute::layout_right_aligned(row_right, row_cy, scale);
                            let is_hovered = self.hovered_entry == Some(i);
                            let show_mon = matches!(kind, LayerNodeKind::Group | LayerNodeKind::Instrument);
                            let visible = *is_soloed || *is_muted || *is_monitoring || is_hovered;
                            out.extend(super::solo_mute::build_instances(&layout, *is_soloed, *is_muted, *is_monitoring, is_hovered, visible, show_mon, &settings.theme, scale));
                        }
                    }
                }
                EntryKind::Dir | EntryKind::File => {
                    // Selected highlight (like Layers tab)
                    if self.selected_entry == Some(i) {
                        let a = settings.theme.accent;
                        out.push(InstanceRaw {
                            position: [cx, y],
                            size: [content_w, item_h],
                            color: [a[0], a[1], a[2], 0.22],
                            border_radius: 0.0,
                        });
                    }
                    // Hover
                    if self.hovered_entry == Some(i) {
                        out.push(InstanceRaw {
                            position: [cx, clip_y],
                            size: [content_w, clip_h],
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

                        if cy >= ct {
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

        // --- Preview strip at bottom ---
        if self.preview_audio.is_some() {
            let [strip_x, strip_y, strip_w, strip_h] = self.preview_strip_rect(screen_h, scale);

            // Strip background
            out.push(InstanceRaw {
                position: [strip_x, strip_y],
                size: [strip_w, strip_h],
                color: settings.theme.bg_base,
                border_radius: 0.0,
            });

            // Separator line above strip
            out.push(InstanceRaw {
                position: [strip_x, strip_y],
                size: [strip_w, 1.0 * scale],
                color: crate::theme::with_alpha(settings.theme.text_primary, 0.07),
                border_radius: 0.0,
            });

            // Headphones toggle button (left side)
            let btn_size = 20.0 * scale;
            let btn_x = strip_x + 8.0 * scale;
            let btn_y = strip_y + (strip_h - btn_size) * 0.5;
            let btn_radius = btn_size * 0.5;
            let btn_color = if self.auto_preview {
                settings.theme.accent
            } else {
                crate::theme::with_alpha(settings.theme.text_primary, 0.15)
            };
            out.push(InstanceRaw {
                position: [btn_x, btn_y],
                size: [btn_size, btn_size],
                color: btn_color,
                border_radius: btn_radius,
            });
            if self.preview_toggle_hovered {
                out.push(InstanceRaw {
                    position: [btn_x, btn_y],
                    size: [btn_size, btn_size],
                    color: [1.0, 1.0, 1.0, 0.08],
                    border_radius: btn_radius,
                });
            }

            // Waveform background rect (right of button, full height with padding)
            let wf_x = btn_x + btn_size + 8.0 * scale;
            let wf_w = strip_w - (wf_x - strip_x) - 8.0 * scale;
            let wf_h = strip_h - 12.0 * scale;
            let wf_y = strip_y + 6.0 * scale;
            out.push(InstanceRaw {
                position: [wf_x, wf_y],
                size: [wf_w, wf_h],
                color: [0.0, 0.0, 0.0, 0.3],
                border_radius: 2.0 * scale,
            });
        }

        out
    }

    /// Returns the rect [x, y, w, h] of the preview waveform area (inside the strip).
    pub fn preview_waveform_rect(&self, screen_h: f32, scale: f32) -> [f32; 4] {
        let [strip_x, strip_y, strip_w, strip_h] = self.preview_strip_rect(screen_h, scale);
        let btn_size = 20.0 * scale;
        let btn_x = strip_x + 8.0 * scale;
        let wf_x = btn_x + btn_size + 8.0 * scale;
        let wf_w = strip_w - (wf_x - strip_x) - 8.0 * scale;
        let wf_h = strip_h - 12.0 * scale;
        let wf_y = strip_y + 6.0 * scale;
        [wf_x, wf_y, wf_w, wf_h]
    }

    /// Returns the rect [x, y, w, h] of the headphones toggle button.
    pub fn preview_toggle_rect(&self, screen_h: f32, scale: f32) -> [f32; 4] {
        let [strip_x, strip_y, _, strip_h] = self.preview_strip_rect(screen_h, scale);
        let btn_size = 20.0 * scale;
        let btn_x = strip_x + 8.0 * scale;
        let btn_y = strip_y + (strip_h - btn_size) * 0.5;
        [btn_x, btn_y, btn_size, btn_size]
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
            self.cursor_text_dirty = false;
            self.text_generation += 1;
        } else if self.cursor_text_dirty && !self.cached_text.is_empty() {
            // Fast path: only rebuild the search bar text entry (index 0)
            // instead of regenerating 300+ entries for a cursor blink.
            self.cached_text[0] = self.build_search_bar_text_entry(theme, scale);
            self.cursor_text_dirty = false;
            self.cursor_text_generation += 1;
        }
        &self.cached_text
    }

    fn build_search_bar_text_entry(&self, theme: &crate::theme::RuntimeTheme, scale: f32) -> TextEntry {
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let search_bar_h = SEARCH_BAR_HEIGHT * scale;
        let row_y = header_h;
        let margin = 6.0 * scale;
        let sb_x = margin;
        let sb_w_inner = w - sb_x - margin;
        let font_sz = 11.0 * scale;
        let line_h = 14.0 * scale;
        let text_x = sb_x + 8.0 * scale;
        let text_y = row_y + (search_bar_h - line_h) * 0.5;
        let (text, color) = if self.search_focused || !self.search_input.text.is_empty() {
            let display = if self.search_focused {
                self.search_input.display_text()
            } else {
                self.search_input.text.clone()
            };
            (display, crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255))
        } else {
            ("Search...".to_string(), crate::theme::RuntimeTheme::text_u8(theme.text_dim, 160))
        };
        TextEntry {
            text,
            x: text_x,
            y: text_y,
            font_size: font_sz,
            line_height: line_h,
            max_width: sb_w_inner - if self.search_input.text.is_empty() { 16.0 } else { 32.0 } * scale,
            color,
            weight: 400,
            bounds: Some([sb_x, row_y, sb_x + sb_w_inner, row_y + search_bar_h]),
            center: false,
        }
    }

    fn build_text_entries(&self, theme: &crate::theme::RuntimeTheme, _screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let w = self.panel_width(scale);
        let header_h = HEADER_HEIGHT * scale;
        let sb_w = self.sidebar_width(scale);
        let item_h = ITEM_HEIGHT * scale;

        // cx is the tree-pane left edge (right of sidebar)
        let cx = self.content_x(scale);

        // --- Search bar text (second row, below header) ---
        out.push(self.build_search_bar_text_entry(theme, scale));

        let ct = self.content_top(scale);

        // --- Sidebar: category labels then Places section (not scrolled) ---
        let sb_item_h = SIDEBAR_ITEM_HEIGHT * scale;
        let font_sz = 12.0 * scale;
        let line_h = 15.0 * scale;

        for (i, cat) in SIDEBAR_CATEGORIES.iter().enumerate() {
            let y = ct + i as f32 * sb_item_h;
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

        // --- Places section inside sidebar (Samples only, not scrolled) ---
        if self.active_category == BrowserCategory::Samples {
            let places_y = self.places_section_y(scale);
            let ph = PLACES_HEADER_HEIGHT * scale;
            let row_h = PLACES_ROW_HEIGHT * scale;
            let icon_w = 22.0 * scale; // space reserved for folder icon

            // "Places" header label
            out.push(TextEntry {
                text: "Places".to_string(),
                x: 8.0 * scale,
                y: places_y + (ph - 9.0 * scale) * 0.5,
                font_size: 9.0 * scale,
                line_height: 9.0 * scale,
                max_width: sb_w - 10.0 * scale,
                color: crate::theme::RuntimeTheme::text_u8(theme.text_dim, 150),
                weight: 600,
                bounds: Some([0.0, 0.0, 0.0, 0.0]),
                center: false,
            });

            for (i, root) in self.root_folders.iter().enumerate() {
                let row_y = places_y + ph + i as f32 * row_h;
                let name = root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.to_string_lossy().to_string());
                let label_line_h = 13.0 * scale;
                let color = if i == self.selected_place {
                    crate::theme::RuntimeTheme::text_u8(theme.text_primary, 240)
                } else if self.hovered_place == Some(i) {
                    crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 210)
                } else {
                    crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 180)
                };
                out.push(TextEntry {
                    text: name,
                    x: icon_w,
                    y: row_y + (row_h - label_line_h) * 0.5,
                    font_size: 12.0 * scale,
                    line_height: label_line_h,
                    max_width: sb_w - icon_w - 4.0 * scale,
                    color,
                    weight: if i == self.selected_place { 600 } else { 400 },
                    bounds: Some([0.0, 0.0, 0.0, 0.0]),
                    center: false,
                });
            }

            // "Add Folder…" row
            let add_row_y = places_y + ph + self.root_folders.len() as f32 * row_h;
            let label_line_h = 13.0 * scale;
            let add_color = if self.places_add_hovered {
                crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 210)
            } else {
                crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 180)
            };
            out.push(TextEntry {
                text: "Add Folder…".to_string(),
                x: icon_w,
                y: add_row_y + (row_h - label_line_h) * 0.5,
                font_size: 12.0 * scale,
                line_height: label_line_h,
                max_width: sb_w - icon_w - 4.0 * scale,
                color: add_color,
                weight: 400,
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
                    let text_x = cx + 14.0 * scale;
                    let font_sz = 12.0 * scale;
                    let line_h = 16.0 * scale;
                    let color = crate::theme::RuntimeTheme::text_u8(theme.text_primary, 230);
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
                EntryKind::EmptyState => {
                    let font_sz = 12.0 * scale;
                    let line_h = 16.0 * scale;
                    out.push(TextEntry {
                        text: "Nothing here yet".to_string(),
                        x: cx,
                        y: base_y + (item_h - line_h) * 0.5,
                        font_size: font_sz,
                        line_height: line_h,
                        max_width: w - cx - 8.0 * scale,
                        color: crate::theme::RuntimeTheme::text_u8(theme.text_dim, 160),
                        weight: 400,
                        bounds: None,
                        center: true,
                    });
                }
                EntryKind::ProjectInstrument { .. } | EntryKind::LayerNode { .. } | EntryKind::Master => {
                    let indent = entry.depth as f32 * INDENT_PX * scale;
                    let dot_offset = if matches!(entry.kind, EntryKind::Master) { 12.0 }
                        else if matches!(entry.kind, EntryKind::LayerNode { kind: LayerNodeKind::Group, .. }) { 34.0 }
                        else { 28.0 };
                    let text_offset = indent + dot_offset * scale;
                    let text_x = cx + text_offset;
                    let font_sz = 12.0 * scale;
                    let line_h = 16.0 * scale;
                    let color = match &entry.kind {
                        EntryKind::Master => crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255),
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
                        (Some(eid), Some((edit_id, _, input))) if eid == *edit_id => {
                            (input.display_text(), [255u8, 255, 255, 255])
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
                        bounds: Some([cx, base_y, w, base_y + item_h]),
                        center: false,
                    });
                    // Solo/Mute button labels (persistent state only; hover text is in hover_sm_text)
                    if let EntryKind::LayerNode { kind, is_soloed, is_muted, is_monitoring, .. } = &entry.kind {
                        if matches!(kind, LayerNodeKind::Waveform | LayerNodeKind::Instrument | LayerNodeKind::Group) {
                            let visible = *is_soloed || *is_muted || *is_monitoring;
                            if visible {
                                let row_right = cx + self.content_width(scale);
                                let row_cy = base_y + item_h * 0.5;
                                let layout = super::solo_mute::layout_right_aligned(row_right, row_cy, scale);
                                let show_mon = matches!(kind, LayerNodeKind::Group | LayerNodeKind::Instrument);
                                out.extend(super::solo_mute::build_text_entries(&layout, *is_soloed, *is_muted, *is_monitoring, true, show_mon, theme, scale));
                            }
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

        // --- Preview strip text ---
        if let Some(ref audio) = self.preview_audio {
            let [strip_x, strip_y, strip_w, strip_h] = self.preview_strip_rect(_screen_h, scale);
            let strip_bounds = Some([strip_x, strip_y, strip_x + strip_w, strip_y + strip_h]);

            let [btn_x, btn_y, btn_size, _] = self.preview_toggle_rect(_screen_h, scale);
            let font_sz = 10.0 * scale;
            let line_h = 12.0 * scale;
            // Headphones icon "🎧" on the toggle button — centered
            out.push(TextEntry {
                text: "🎧".to_string(),
                x: btn_x,
                y: btn_y + (btn_size - line_h) * 0.5,
                font_size: font_sz,
                line_height: line_h,
                max_width: btn_size,
                color: [255, 255, 255, if self.auto_preview { 255 } else { 140 }],
                weight: 400,
                bounds: strip_bounds,
                center: true,
            });

            // Filename label (inside waveform, top-left with padding — like canvas)
            let [wf_x, wf_y, wf_w, _] = self.preview_waveform_rect(_screen_h, scale);
            out.push(TextEntry {
                text: audio.filename.clone(),
                x: wf_x + 6.0 * scale,
                y: wf_y + 4.0 * scale,
                font_size: 9.0 * scale,
                line_height: 11.0 * scale,
                max_width: wf_w - 12.0 * scale,
                color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, 200),
                weight: 400,
                bounds: strip_bounds,
                center: false,
            });
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


/// Recursively walk a directory tree and collect file entries for the search index.
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
            index_walk_dir(index, &path);
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

use crate::gpu::TextEntry;
