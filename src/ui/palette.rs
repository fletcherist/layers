use crate::gpu::TextEntry;
use crate::InstanceRaw;
use crate::theme::{SCROLLBAR_BG, SCROLLBAR_THUMB, RMS_LOW, RMS_MID, RMS_HIGH};

pub const PALETTE_WIDTH: f32 = 520.0;
pub const PALETTE_INPUT_HEIGHT: f32 = 52.0;
pub const PALETTE_ITEM_HEIGHT: f32 = 38.0;
pub const PALETTE_SECTION_HEIGHT: f32 = 28.0;
pub const PALETTE_MAX_VISIBLE_ROWS: usize = 14;
pub const PALETTE_PADDING: f32 = 6.0;
pub const PALETTE_BORDER_RADIUS: f32 = 12.0;

use crate::settings::{AdaptiveGridSize, FixedGrid};

#[derive(Clone, Copy, PartialEq)]
pub enum CommandAction {
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    Undo,
    Redo,
    SaveProject,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleBrowser,
    AddFolderToBrowser,
    SetMasterVolume,
    CreateComponent,
    CreateInstance,
    GoToComponent,
    OpenSettings,
    RenameEffectRegion,
    RenameSample,
    ToggleSnapToGrid,
    ToggleVerticalSnap,
    ToggleGrid,
    SetGridAdaptive(AdaptiveGridSize),
    SetGridFixed(FixedGrid),
    NarrowGrid,
    WidenGrid,
    ToggleTripletGrid,
    TestToast,
    RevealInFinder,
    ReverseSample,
    SplitSample,
    AddLoopArea,
    AddEffectsArea,
    AddRenderArea,
    AddPlugin,
    SetSampleColor(usize),
    ToggleAutomation,
    AddVolumeAutomation,
    AddPanAutomation,
    AddMidiClip,
    AddTextNote,
    AddInstrument,
    SetMidiClipGridFixed(FixedGrid),
    SetMidiClipGridAdaptive(AdaptiveGridSize),
    ToggleMidiClipTripletGrid,
    NarrowMidiClipGrid,
    WidenMidiClipGrid,
    MoveLayerUp,
    MoveLayerDown,
    SetWarpOff,
    SetWarpRePitch,
    SetWarpSemitone,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PaletteMode {
    Commands,
    VolumeFader,
    PluginPicker,
    InstrumentPicker,
}

pub struct PluginPickerEntry {
    pub name: String,
    pub manufacturer: String,
    pub unique_id: String,
    pub is_instrument: bool,
}

pub const FADER_CONTENT_HEIGHT: f32 = 90.0;
const FADER_TRACK_H: f32 = 6.0;
const FADER_THUMB_R: f32 = 9.0;
const FADER_MARGIN_TOP: f32 = 36.0;
const RMS_BAR_H: f32 = 4.0;
const RMS_MARGIN_TOP: f32 = 22.0;

const DB_MIN: f32 = -60.0;
const DB_MAX: f32 = 6.0;
const DB_RANGE: f32 = DB_MAX - DB_MIN; // 66.0

pub fn gain_to_db(gain: f32) -> f32 {
    if gain < 0.00001 {
        -100.0
    } else {
        20.0 * gain.log10()
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

pub fn fader_pos_to_gain(pos: f32) -> f32 {
    if pos <= 0.005 {
        return 0.0;
    }
    let db = DB_MIN + pos.clamp(0.0, 1.0) * DB_RANGE;
    db_to_gain(db)
}

pub fn gain_to_fader_pos(gain: f32) -> f32 {
    if gain < 0.00001 {
        return 0.0;
    }
    let db = gain_to_db(gain);
    ((db - DB_MIN) / DB_RANGE).clamp(0.0, 1.0)
}

// --- Right-window gain fader (piecewise: +24 top, 0 at P_ZERO, −70 near-bottom, −∞ below) ---

pub const VOL_FADER_DB_MAX: f32 = 24.0;
pub const VOL_FADER_DB_BOTTOM: f32 = -70.0;
/// Fader position (0–1) where 0 dB sits.
pub const VOL_FADER_P_ZERO: f32 = 0.60;
/// Fader position where −70 dB sits; below this it hyperbolically approaches −∞.
pub const VOL_FADER_P_BOTTOM: f32 = 0.05;

pub fn vol_fader_pos_to_gain(pos: f32) -> f32 {
    let pos = pos.clamp(0.0, 1.0);
    // Mute gate: bottom 1% of travel
    if pos < 0.01 {
        return 0.0;
    }
    let db = if pos >= VOL_FADER_P_ZERO {
        // Upper segment: 0 → +24 dB
        (pos - VOL_FADER_P_ZERO) / (1.0 - VOL_FADER_P_ZERO) * VOL_FADER_DB_MAX
    } else if pos >= VOL_FADER_P_BOTTOM {
        // Middle segment: linear −70 → 0 dB
        VOL_FADER_DB_BOTTOM * (pos - VOL_FADER_P_ZERO) / (VOL_FADER_P_BOTTOM - VOL_FADER_P_ZERO)
    } else {
        // Lower segment: hyperbolic fast approach to −∞
        VOL_FADER_DB_BOTTOM * VOL_FADER_P_BOTTOM / pos
    };
    db_to_gain(db)
}

pub fn gain_to_vol_fader_pos(gain: f32) -> f32 {
    if gain < 0.00001 {
        return 0.0;
    }
    let db = gain_to_db(gain);
    if db >= 0.0 {
        // Upper segment
        (VOL_FADER_P_ZERO + db / VOL_FADER_DB_MAX * (1.0 - VOL_FADER_P_ZERO)).clamp(VOL_FADER_P_ZERO, 1.0)
    } else if db >= VOL_FADER_DB_BOTTOM {
        // Middle segment: pos = P_ZERO + db/DB_BOTTOM*(P_BOTTOM−P_ZERO)
        let pos = VOL_FADER_P_ZERO + db / VOL_FADER_DB_BOTTOM * (VOL_FADER_P_BOTTOM - VOL_FADER_P_ZERO);
        pos.clamp(VOL_FADER_P_BOTTOM, VOL_FADER_P_ZERO)
    } else {
        // Lower segment: pos = DB_BOTTOM*P_BOTTOM/db (hyperbolic inverse)
        let pos = VOL_FADER_DB_BOTTOM * VOL_FADER_P_BOTTOM / db;
        pos.clamp(0.0, VOL_FADER_P_BOTTOM)
    }
}

pub struct CommandDef {
    pub name: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
    pub action: CommandAction,
    pub dev_only: bool,
}

pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "Select All",
        shortcut: "⌘A",
        category: "Suggestions",
        action: CommandAction::SelectAll,
        dev_only: false,
    },
    CommandDef {
        name: "Copy",
        shortcut: "⌘C",
        category: "Edit",
        action: CommandAction::Copy,
        dev_only: false,
    },
    CommandDef {
        name: "Paste",
        shortcut: "⌘V",
        category: "Edit",
        action: CommandAction::Paste,
        dev_only: false,
    },
    CommandDef {
        name: "Delete",
        shortcut: "⌫",
        category: "Edit",
        action: CommandAction::Delete,
        dev_only: false,
    },
    CommandDef {
        name: "Undo",
        shortcut: "⌘Z",
        category: "Edit",
        action: CommandAction::Undo,
        dev_only: false,
    },
    CommandDef {
        name: "Redo",
        shortcut: "⇧⌘Z",
        category: "Edit",
        action: CommandAction::Redo,
        dev_only: false,
    },
    CommandDef {
        name: "Zoom In",
        shortcut: "⌘+",
        category: "View",
        action: CommandAction::ZoomIn,
        dev_only: false,
    },
    CommandDef {
        name: "Zoom Out",
        shortcut: "⌘−",
        category: "View",
        action: CommandAction::ZoomOut,
        dev_only: false,
    },
    CommandDef {
        name: "Reset Zoom",
        shortcut: "⌘0",
        category: "View",
        action: CommandAction::ResetZoom,
        dev_only: false,
    },
    CommandDef {
        name: "Toggle Sample Browser",
        shortcut: "⌘B",
        category: "View",
        action: CommandAction::ToggleBrowser,
        dev_only: false,
    },
    CommandDef {
        name: "Save Project",
        shortcut: "⌘S",
        category: "Project",
        action: CommandAction::SaveProject,
        dev_only: false,
    },
    CommandDef {
        name: "Add Folder to Browser",
        shortcut: "⇧⌘A",
        category: "Project",
        action: CommandAction::AddFolderToBrowser,
        dev_only: false,
    },
    CommandDef {
        name: "Set Master Volume",
        shortcut: "",
        category: "Audio",
        action: CommandAction::SetMasterVolume,
        dev_only: false,
    },
    CommandDef {
        name: "Open Settings",
        shortcut: "⌘,",
        category: "View",
        action: CommandAction::OpenSettings,
        dev_only: false,
    },
    CommandDef {
        name: "Reverse Sample",
        shortcut: "",
        category: "Audio",
        action: CommandAction::ReverseSample,
        dev_only: false,
    },
    CommandDef {
        name: "Split Sample",
        shortcut: "⌘E",
        category: "Audio",
        action: CommandAction::SplitSample,
        dev_only: false,
    },
    CommandDef {
        name: "Add Loop Area",
        shortcut: "⌘L",
        category: "Audio",
        action: CommandAction::AddLoopArea,
        dev_only: false,
    },
    CommandDef {
        name: "Add Effects Area",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddEffectsArea,
        dev_only: false,
    },
    CommandDef {
        name: "Add Plugin",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddPlugin,
        dev_only: false,
    },
    CommandDef {
        name: "Add Render Area",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddRenderArea,
        dev_only: false,
    },
    CommandDef {
        name: "Toggle Automation",
        shortcut: "⌘T",
        category: "View",
        action: CommandAction::ToggleAutomation,
        dev_only: false,
    },
    CommandDef {
        name: "Create Volume Automation",
        shortcut: "",
        category: "Sample",
        action: CommandAction::AddVolumeAutomation,
        dev_only: false,
    },
    CommandDef {
        name: "Create Pan Automation",
        shortcut: "",
        category: "Sample",
        action: CommandAction::AddPanAutomation,
        dev_only: false,
    },
    CommandDef {
        name: "Add MIDI Clip",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddMidiClip,
        dev_only: false,
    },
    CommandDef {
        name: "Add Text Note",
        shortcut: "",
        category: "Canvas",
        action: CommandAction::AddTextNote,
        dev_only: false,
    },
    CommandDef {
        name: "Add Instrument",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddInstrument,
        dev_only: false,
    },
    CommandDef {
        name: "Test Toast",
        shortcut: "",
        category: "Debug",
        action: CommandAction::TestToast,
        dev_only: true,
    },
];

#[derive(Clone)]
pub enum PaletteRow {
    Section(&'static str),
    Command(usize),
    Plugin(usize),
}

pub struct CommandPalette {
    pub search_text: String,
    pub rows: Vec<PaletteRow>,
    pub command_count: usize,
    pub selected_index: usize,
    pub scroll_row_offset: usize,
    pub mode: PaletteMode,
    pub fader_value: f32,
    pub fader_rms: f32,
    pub fader_dragging: bool,
    pub scroll_accumulator: f32,
    // Plugin picker state
    pub plugin_entries: Vec<PluginPickerEntry>,
    pub filtered_plugin_indices: Vec<usize>,
    pub plugin_selected_index: usize,
    pub plugin_scroll_offset: f32,
}

impl CommandPalette {
    pub fn new(dev_mode: bool) -> Self {
        let mut p = Self {
            search_text: String::new(),
            rows: Vec::new(),
            command_count: 0,
            selected_index: 0,
            scroll_row_offset: 0,
            mode: PaletteMode::Commands,
            fader_value: 1.0,
            fader_rms: 0.0,
            fader_dragging: false,
            scroll_accumulator: 0.0,
            plugin_entries: Vec::new(),
            filtered_plugin_indices: Vec::new(),
            plugin_selected_index: 0,
            plugin_scroll_offset: 0.0,
        };
        p.rebuild_rows(dev_mode);
        p
    }

    fn rebuild_rows(&mut self, dev_mode: bool) {
        let query = self.search_text.to_lowercase();
        let is_searching = !query.is_empty();

        let matched: Vec<usize> = COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, cmd)| dev_mode || !cmd.dev_only)
            .filter(|(_, cmd)| !is_searching || cmd.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        self.rows.clear();
        self.command_count = 0;

        if is_searching {
            for &i in &matched {
                self.rows.push(PaletteRow::Command(i));
                self.command_count += 1;
            }
        } else {
            let mut last_cat = "";
            for &i in &matched {
                let cat = COMMANDS[i].category;
                if cat != last_cat {
                    self.rows.push(PaletteRow::Section(cat));
                    last_cat = cat;
                }
                self.rows.push(PaletteRow::Command(i));
                self.command_count += 1;
            }
        }

        // Append matching (or all) plugins: instruments first, then effects
        // Search also matches the type label ("instrument" / "effect")
        let plugin_matches = |e: &PluginPickerEntry| -> bool {
            if !is_searching {
                return true;
            }
            let label = if e.is_instrument { "instrument" } else { "effect" };
            e.name.to_lowercase().contains(&query)
                || e.manufacturer.to_lowercase().contains(&query)
                || label.contains(&query)
        };
        let instruments: Vec<usize> = self
            .plugin_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_instrument)
            .filter(|(_, e)| plugin_matches(e))
            .map(|(i, _)| i)
            .collect();
        let effects: Vec<usize> = self
            .plugin_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_instrument)
            .filter(|(_, e)| plugin_matches(e))
            .map(|(i, _)| i)
            .collect();
        if !instruments.is_empty() {
            self.rows.push(PaletteRow::Section("Instruments"));
            for i in instruments {
                self.rows.push(PaletteRow::Plugin(i));
                self.command_count += 1;
            }
        }
        if !effects.is_empty() {
            self.rows.push(PaletteRow::Section("Effects"));
            for i in effects {
                self.rows.push(PaletteRow::Plugin(i));
                self.command_count += 1;
            }
        }

        if self.command_count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= self.command_count {
            self.selected_index = self.command_count - 1;
        }
        self.scroll_row_offset = 0;
        self.ensure_selected_visible();
    }

    pub fn update_filter(&mut self, dev_mode: bool) {
        if matches!(self.mode, PaletteMode::PluginPicker | PaletteMode::InstrumentPicker) {
            self.rebuild_plugin_filter();
            return;
        }
        self.rebuild_rows(dev_mode);
    }

    pub fn set_plugin_entries(&mut self, entries: Vec<PluginPickerEntry>) {
        self.plugin_entries = entries;
        self.rebuild_plugin_filter();
    }

    fn rebuild_plugin_filter(&mut self) {
        let query = self.search_text.to_lowercase();
        self.filtered_plugin_indices = self
            .plugin_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                query.is_empty()
                    || e.name.to_lowercase().contains(&query)
                    || e.manufacturer.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        let count = self.filtered_plugin_indices.len();
        if count == 0 {
            self.plugin_selected_index = 0;
        } else if self.plugin_selected_index >= count {
            self.plugin_selected_index = count - 1;
        }
        self.plugin_scroll_offset = 0.0;
        // Note: ensure_plugin_selected_visible needs scale, but after filter reset
        // selection is at top so scroll is already correct.
    }

    pub fn plugin_content_height(&self, scale: f32) -> f32 {
        self.filtered_plugin_indices.len() as f32 * PALETTE_ITEM_HEIGHT * scale
    }

    pub fn plugin_visible_height(&self, scale: f32) -> f32 {
        let max_h = PALETTE_MAX_VISIBLE_ROWS as f32 * PALETTE_ITEM_HEIGHT * scale;
        self.plugin_content_height(scale).min(max_h)
    }

    pub fn plugin_max_scroll(&self, scale: f32) -> f32 {
        (self.plugin_content_height(scale) - self.plugin_visible_height(scale)).max(0.0)
    }

    pub fn clamp_plugin_scroll(&mut self, scale: f32) {
        self.plugin_scroll_offset = self
            .plugin_scroll_offset
            .clamp(0.0, self.plugin_max_scroll(scale));
    }

    pub fn move_plugin_selection(&mut self, delta: i32, scale: f32) {
        let count = self.filtered_plugin_indices.len();
        if count == 0 {
            return;
        }
        self.plugin_selected_index =
            ((self.plugin_selected_index as i32 + delta).rem_euclid(count as i32)) as usize;
        self.ensure_plugin_selected_visible(scale);
    }

    fn ensure_plugin_selected_visible(&mut self, scale: f32) {
        let item_h = PALETTE_ITEM_HEIGHT * scale;
        let sel_top = self.plugin_selected_index as f32 * item_h;
        let sel_bottom = sel_top + item_h;
        let visible_h = self.plugin_visible_height(scale);

        if sel_top < self.plugin_scroll_offset {
            self.plugin_scroll_offset = sel_top;
        }
        if sel_bottom > self.plugin_scroll_offset + visible_h {
            self.plugin_scroll_offset = sel_bottom - visible_h;
        }
        self.clamp_plugin_scroll(scale);
    }

    pub fn scroll_plugin_by(&mut self, delta_px: f32, scale: f32) {
        self.plugin_scroll_offset += delta_px;
        self.clamp_plugin_scroll(scale);
    }

    pub fn visible_plugin_entries(&self, scale: f32) -> &[usize] {
        let item_h = PALETTE_ITEM_HEIGHT * scale;
        if item_h <= 0.0 {
            return &[];
        }
        let start = (self.plugin_scroll_offset / item_h).floor() as usize;
        let start = start.min(self.filtered_plugin_indices.len());
        let end = (start + PALETTE_MAX_VISIBLE_ROWS + 1).min(self.filtered_plugin_indices.len());
        &self.filtered_plugin_indices[start..end]
    }

    /// Returns the pixel Y offset of the first visible row relative to the list top.
    /// This is the fractional part that makes scrolling smooth.
    pub fn plugin_scroll_y_offset(&self, scale: f32) -> f32 {
        let item_h = PALETTE_ITEM_HEIGHT * scale;
        if item_h <= 0.0 {
            return 0.0;
        }
        self.plugin_scroll_offset % item_h
    }

    pub fn selected_plugin(&self) -> Option<&PluginPickerEntry> {
        let idx = *self
            .filtered_plugin_indices
            .get(self.plugin_selected_index)?;
        self.plugin_entries.get(idx)
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.command_count == 0 {
            return;
        }
        let len = self.command_count as i32;
        self.selected_index = ((self.selected_index as i32 + delta).rem_euclid(len)) as usize;
        self.ensure_selected_visible();
    }

    fn row_index_for_selected(&self) -> Option<usize> {
        let mut cmd_i = 0;
        for (ri, row) in self.rows.iter().enumerate() {
            if matches!(row, PaletteRow::Command(_) | PaletteRow::Plugin(_)) {
                if cmd_i == self.selected_index {
                    return Some(ri);
                }
                cmd_i += 1;
            }
        }
        None
    }

    fn ensure_selected_visible(&mut self) {
        let Some(sel_row) = self.row_index_for_selected() else {
            return;
        };
        if sel_row < self.scroll_row_offset {
            self.scroll_row_offset = sel_row;
        }
        let end = self.scroll_row_offset + PALETTE_MAX_VISIBLE_ROWS;
        if sel_row >= end {
            self.scroll_row_offset = sel_row + 1 - PALETTE_MAX_VISIBLE_ROWS;
        }
        self.clamp_scroll_offset();
    }

    fn total_row_height(&self, scale: f32) -> f32 {
        self.rows
            .iter()
            .map(|r| match r {
                PaletteRow::Section(_) => PALETTE_SECTION_HEIGHT * scale,
                PaletteRow::Command(_) | PaletteRow::Plugin(_) => PALETTE_ITEM_HEIGHT * scale,
            })
            .sum()
    }

    fn clamp_scroll_offset(&mut self) {
        let max = self.rows.len().saturating_sub(PALETTE_MAX_VISIBLE_ROWS);
        if self.scroll_row_offset > max {
            self.scroll_row_offset = max;
        }
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if self.rows.len() <= PALETTE_MAX_VISIBLE_ROWS {
            return;
        }
        let max = self.rows.len() - PALETTE_MAX_VISIBLE_ROWS;
        let new = (self.scroll_row_offset as i32 + delta).clamp(0, max as i32);
        self.scroll_row_offset = new as usize;
    }

    pub fn scroll_by_pixels(&mut self, pixels: f32, scale: f32) {
        self.scroll_accumulator += pixels;
        let row_h = PALETTE_ITEM_HEIGHT * scale;
        let lines = (self.scroll_accumulator / row_h) as i32;
        if lines != 0 {
            self.scroll_accumulator -= lines as f32 * row_h;
            self.scroll_by(lines);
        }
    }

    pub fn visible_command_offset(&self) -> usize {
        let mut count = 0;
        for row in &self.rows[..self.scroll_row_offset] {
            if matches!(row, PaletteRow::Command(_) | PaletteRow::Plugin(_)) {
                count += 1;
            }
        }
        count
    }

    pub fn selected_action(&self) -> Option<CommandAction> {
        let mut cmd_i = 0;
        for row in &self.rows {
            match row {
                PaletteRow::Command(ci) => {
                    if cmd_i == self.selected_index {
                        return Some(COMMANDS[*ci].action);
                    }
                    cmd_i += 1;
                }
                PaletteRow::Plugin(_) => {
                    // Plugin rows count toward selected_index but aren't command actions
                    cmd_i += 1;
                }
                PaletteRow::Section(_) => {}
            }
        }
        None
    }

    /// Returns the selected plugin entry if a Plugin row is selected in Commands mode.
    pub fn selected_inline_plugin(&self) -> Option<&PluginPickerEntry> {
        let mut cmd_i = 0;
        for row in &self.rows {
            match row {
                PaletteRow::Plugin(pi) => {
                    if cmd_i == self.selected_index {
                        return self.plugin_entries.get(*pi);
                    }
                    cmd_i += 1;
                }
                PaletteRow::Command(_) => {
                    cmd_i += 1;
                }
                PaletteRow::Section(_) => {}
            }
        }
        None
    }

    pub fn visible_rows(&self) -> &[PaletteRow] {
        if !matches!(self.mode, PaletteMode::Commands) {
            return &[];
        }
        let start = self.scroll_row_offset.min(self.rows.len());
        let end = (start + PALETTE_MAX_VISIBLE_ROWS).min(self.rows.len());
        &self.rows[start..end]
    }

    pub fn content_height(&self, scale: f32) -> f32 {
        if self.mode == PaletteMode::VolumeFader {
            return FADER_CONTENT_HEIGHT * scale;
        }
        if matches!(self.mode, PaletteMode::PluginPicker | PaletteMode::InstrumentPicker) {
            return self.plugin_visible_height(scale);
        }
        let mut h = 0.0;
        for row in self.visible_rows() {
            h += match row {
                PaletteRow::Section(_) => PALETTE_SECTION_HEIGHT * scale,
                PaletteRow::Command(_) | PaletteRow::Plugin(_) => PALETTE_ITEM_HEIGHT * scale,
            };
        }
        h
    }

    pub fn total_height(&self, scale: f32) -> f32 {
        let content = self.content_height(scale);
        let divider = if content > 0.0 { 1.0 * scale } else { 0.0 };
        PALETTE_INPUT_HEIGHT * scale + divider + content + PALETTE_PADDING * scale
    }

    pub fn palette_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = PALETTE_WIDTH * scale;
        let h = self.total_height(scale);
        let x = (screen_w - w) * 0.5;
        let y = screen_h * 0.16;
        ([x, y], [w, h])
    }

    pub fn contains(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = self.palette_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0] && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    /// Returns the global command-relative index if mouse is on a command row.
    /// In PluginPicker mode, returns the index into filtered_plugin_indices.
    pub fn item_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        if matches!(self.mode, PaletteMode::VolumeFader) {
            return None;
        }
        let (rp, _) = self.palette_rect(screen_w, screen_h, scale);
        let list_top = rp[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;

        if matches!(self.mode, PaletteMode::PluginPicker | PaletteMode::InstrumentPicker) {
            let item_h = PALETTE_ITEM_HEIGHT * scale;
            if item_h <= 0.0 {
                return None;
            }
            let y_offset = self.plugin_scroll_y_offset(scale);
            let visible = self.visible_plugin_entries(scale);
            let first_row = (self.plugin_scroll_offset / item_h).floor() as usize;
            for (i, _) in visible.iter().enumerate() {
                let y = list_top + i as f32 * item_h - y_offset;
                if pos[1] >= y && pos[1] < y + item_h {
                    return Some(first_row + i);
                }
            }
            return None;
        }

        let base_cmd = self.visible_command_offset();
        let mut y = list_top;
        let mut cmd_i = 0;
        for row in self.visible_rows() {
            let rh = match row {
                PaletteRow::Section(_) => PALETTE_SECTION_HEIGHT * scale,
                PaletteRow::Command(_) | PaletteRow::Plugin(_) => PALETTE_ITEM_HEIGHT * scale,
            };
            if pos[1] >= y && pos[1] < y + rh {
                return match row {
                    PaletteRow::Section(_) => None,
                    PaletteRow::Command(_) | PaletteRow::Plugin(_) => Some(base_cmd + cmd_i),
                };
            }
            if matches!(row, PaletteRow::Command(_) | PaletteRow::Plugin(_)) {
                cmd_i += 1;
            }
            y += rh;
        }
        None
    }

    fn fader_track_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (ppos, psize) = self.palette_rect(screen_w, screen_h, scale);
        let margin = PALETTE_PADDING * scale;
        let pad = 16.0 * scale;
        let track_y =
            ppos[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale + FADER_MARGIN_TOP * scale;
        let track_w = psize[0] - margin * 2.0 - pad * 2.0;
        (
            [ppos[0] + margin + pad, track_y],
            [track_w, FADER_TRACK_H * scale],
        )
    }

    pub fn fader_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if !matches!(self.mode, PaletteMode::VolumeFader) {
            return false;
        }
        let (tp, ts) = self.fader_track_rect(screen_w, screen_h, scale);
        let thumb_x = tp[0] + self.fader_value * ts[0];
        let thumb_cy = tp[1] + ts[1] * 0.5;
        let r = FADER_THUMB_R * scale + 4.0 * scale;
        let dx = mouse[0] - thumb_x;
        let dy = mouse[1] - thumb_cy;
        dx * dx + dy * dy <= r * r
    }

    pub fn fader_drag(&mut self, mouse_x: f32, screen_w: f32, screen_h: f32, scale: f32) {
        let (tp, ts) = self.fader_track_rect(screen_w, screen_h, scale);
        self.fader_value = ((mouse_x - tp[0]) / ts[0]).clamp(0.0, 1.0);
    }

    pub fn build_instances(&self, settings: &crate::settings::Settings, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = self.palette_rect(screen_w, screen_h, scale);
        let margin = PALETTE_PADDING * scale;

        // Full-screen backdrop
        out.push(InstanceRaw {
            position: [0.0, 0.0],
            size: [screen_w, screen_h],
            color: [0.0, 0.0, 0.0, 0.45],
            border_radius: 0.0,
        });

        // Shadow
        let so = 8.0 * scale;
        out.push(InstanceRaw {
            position: [pos[0] + so, pos[1] + so],
            size: [size[0] + 2.0 * scale, size[1] + 2.0 * scale],
            color: [0.0, 0.0, 0.0, 0.45],
            border_radius: PALETTE_BORDER_RADIUS * scale,
        });

        // Main background
        out.push(InstanceRaw {
            position: pos,
            size,
            color: settings.theme.bg_overlay,
            border_radius: PALETTE_BORDER_RADIUS * scale,
        });

        // Search field background
        let sf_margin = 8.0 * scale;
        out.push(InstanceRaw {
            position: [pos[0] + sf_margin, pos[1] + sf_margin],
            size: [
                size[0] - sf_margin * 2.0,
                PALETTE_INPUT_HEIGHT * scale - sf_margin * 2.0,
            ],
            color: [0.20, 0.20, 0.25, 1.0],
            border_radius: 8.0 * scale,
        });

        // Search icon (small circle to hint at magnifying glass)
        let icon_r = 7.0 * scale;
        out.push(InstanceRaw {
            position: [
                pos[0] + sf_margin + 10.0 * scale,
                pos[1] + (PALETTE_INPUT_HEIGHT * scale - icon_r * 2.0) * 0.5,
            ],
            size: [icon_r * 2.0, icon_r * 2.0],
            color: [0.45, 0.45, 0.52, 0.7],
            border_radius: icon_r,
        });
        // Inner circle cutout
        let inner_r = 4.5 * scale;
        out.push(InstanceRaw {
            position: [
                pos[0] + sf_margin + 10.0 * scale + (icon_r - inner_r),
                pos[1] + (PALETTE_INPUT_HEIGHT * scale - inner_r * 2.0) * 0.5,
            ],
            size: [inner_r * 2.0, inner_r * 2.0],
            color: [0.20, 0.20, 0.25, 1.0],
            border_radius: inner_r,
        });

        let list_top = pos[1] + PALETTE_INPUT_HEIGHT * scale;

        // Divider
        out.push(InstanceRaw {
            position: [pos[0] + margin, list_top],
            size: [size[0] - margin * 2.0, 1.0 * scale],
            color: [1.0, 1.0, 1.0, 0.06],
            border_radius: 0.0,
        });

        match self.mode {
            PaletteMode::Commands => {
                let mut y = list_top + 1.0 * scale;
                let base_cmd = self.visible_command_offset();
                let mut cmd_i = 0;
                for row in self.visible_rows() {
                    match row {
                        PaletteRow::Section(_) => {
                            y += PALETTE_SECTION_HEIGHT * scale;
                        }
                        PaletteRow::Command(_) | PaletteRow::Plugin(_) => {
                            if base_cmd + cmd_i == self.selected_index {
                                out.push(InstanceRaw {
                                    position: [pos[0] + margin, y],
                                    size: [size[0] - margin * 2.0, PALETTE_ITEM_HEIGHT * scale],
                                    color: [0.26, 0.26, 0.32, 0.8],
                                    border_radius: 6.0 * scale,
                                });
                            }
                            // Label pill for plugin rows
                            if let PaletteRow::Plugin(pi) = row {
                                let entry = &self.plugin_entries[*pi];
                                let is_inst = entry.is_instrument;
                                let pill_w = if is_inst { 72.0 } else { 44.0 };
                                let pill_h = 20.0 * scale;
                                let pill_x = pos[0] + size[0] - margin - (pill_w + 10.0) * scale;
                                let pill_y = y + (PALETTE_ITEM_HEIGHT * scale - pill_h) * 0.5;
                                let border_color = if is_inst {
                                    [0.39, 0.63, 1.0, 0.25]
                                } else {
                                    [1.0, 0.67, 0.31, 0.25]
                                };
                                out.push(InstanceRaw {
                                    position: [pill_x, pill_y],
                                    size: [pill_w * scale, pill_h],
                                    color: border_color,
                                    border_radius: 4.0 * scale,
                                });
                            }
                            cmd_i += 1;
                            y += PALETTE_ITEM_HEIGHT * scale;
                        }
                    }
                }

                // Scrollbar
                let total_h = self.total_row_height(scale);
                let visible_h = self.content_height(scale);
                if total_h > visible_h && visible_h > 0.0 {
                    let sb_w = 6.0 * scale;
                    let sb_x = pos[0] + size[0] - margin - sb_w;
                    let track_top = list_top + 1.0 * scale;
                    let track_h = visible_h;

                    out.push(InstanceRaw {
                        position: [sb_x, track_top],
                        size: [sb_w, track_h],
                        color: SCROLLBAR_BG,
                        border_radius: 3.0 * scale,
                    });

                    let ratio = visible_h / total_h;
                    let thumb_h = (ratio * track_h).max(20.0 * scale);
                    let max_offset = self.rows.len().saturating_sub(PALETTE_MAX_VISIBLE_ROWS);
                    let scroll_ratio = if max_offset > 0 {
                        self.scroll_row_offset as f32 / max_offset as f32
                    } else {
                        0.0
                    };
                    let thumb_y = track_top + scroll_ratio * (track_h - thumb_h);

                    out.push(InstanceRaw {
                        position: [sb_x, thumb_y],
                        size: [sb_w, thumb_h],
                        color: SCROLLBAR_THUMB,
                        border_radius: 3.0 * scale,
                    });
                }
            }
            PaletteMode::VolumeFader => {
                let (tp, ts) = self.fader_track_rect(screen_w, screen_h, scale);

                out.push(InstanceRaw {
                    position: tp,
                    size: ts,
                    color: [0.25, 0.25, 0.30, 1.0],
                    border_radius: ts[1] * 0.5,
                });

                let fill_w = self.fader_value * ts[0];
                if fill_w > 0.5 {
                    out.push(InstanceRaw {
                        position: tp,
                        size: [fill_w, ts[1]],
                        color: [0.40, 0.72, 1.00, 1.0],
                        border_radius: ts[1] * 0.5,
                    });
                }

                let thumb_r = FADER_THUMB_R * scale;
                let thumb_x = tp[0] + fill_w - thumb_r;
                let thumb_cy = tp[1] + ts[1] * 0.5 - thumb_r;
                out.push(InstanceRaw {
                    position: [thumb_x, thumb_cy],
                    size: [thumb_r * 2.0, thumb_r * 2.0],
                    color: [1.0, 1.0, 1.0, 0.95],
                    border_radius: thumb_r,
                });

                let rms_y = tp[1] + ts[1] + RMS_MARGIN_TOP * scale;
                let rms_h = RMS_BAR_H * scale;
                out.push(InstanceRaw {
                    position: [tp[0], rms_y],
                    size: [ts[0], rms_h],
                    color: [0.20, 0.20, 0.25, 1.0],
                    border_radius: rms_h * 0.5,
                });

                let rms_w = (self.fader_rms.clamp(0.0, 1.0) * ts[0]).max(0.0);
                if rms_w > 0.5 {
                    let rms_color = if self.fader_rms > 0.8 {
                        RMS_HIGH
                    } else if self.fader_rms > 0.5 {
                        RMS_MID
                    } else {
                        RMS_LOW
                    };
                    out.push(InstanceRaw {
                        position: [tp[0], rms_y],
                        size: [rms_w, rms_h],
                        color: rms_color,
                        border_radius: rms_h * 0.5,
                    });
                }
            }
            PaletteMode::PluginPicker | PaletteMode::InstrumentPicker => {
                let item_h = PALETTE_ITEM_HEIGHT * scale;
                let y_offset = self.plugin_scroll_y_offset(scale);
                let first_row = if item_h > 0.0 {
                    (self.plugin_scroll_offset / item_h).floor() as usize
                } else {
                    0
                };
                let visible = self.visible_plugin_entries(scale);
                let mut y = list_top + 1.0 * scale - y_offset;
                for (i, _) in visible.iter().enumerate() {
                    let abs_idx = first_row + i;
                    if abs_idx == self.plugin_selected_index {
                        out.push(InstanceRaw {
                            position: [pos[0] + margin, y],
                            size: [size[0] - margin * 2.0, item_h],
                            color: [0.26, 0.26, 0.32, 0.8],
                            border_radius: 6.0 * scale,
                        });
                    }
                    y += item_h;
                }

                // Scrollbar
                let content_h = self.plugin_content_height(scale);
                let visible_h = self.plugin_visible_height(scale);
                if content_h > visible_h {
                    let sb_w = 6.0 * scale;
                    let sb_x = pos[0] + size[0] - margin - sb_w;
                    let track_top = list_top + 1.0 * scale;
                    let track_h = visible_h;

                    // Track
                    out.push(InstanceRaw {
                        position: [sb_x, track_top],
                        size: [sb_w, track_h],
                        color: SCROLLBAR_BG,
                        border_radius: 3.0 * scale,
                    });

                    // Thumb
                    let ratio = visible_h / content_h;
                    let thumb_h = (ratio * track_h).max(20.0 * scale);
                    let max_scroll = self.plugin_max_scroll(scale);
                    let scroll_ratio = if max_scroll > 0.0 {
                        self.plugin_scroll_offset / max_scroll
                    } else {
                        0.0
                    };
                    let thumb_y = track_top + scroll_ratio * (track_h - thumb_h);

                    out.push(InstanceRaw {
                        position: [sb_x, thumb_y],
                        size: [sb_w, thumb_h],
                        color: SCROLLBAR_THUMB,
                        border_radius: 3.0 * scale,
                    });
                }
            }
        }

        out
    }

    pub fn get_text_entries(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (ppos, _psize) = self.palette_rect(screen_w, screen_h, scale);
        let margin = PALETTE_PADDING * scale;
        let list_top = ppos[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;

        // Search input text (or placeholder)
        let (display_text, search_color) = match self.mode {
            PaletteMode::VolumeFader => ("Master Volume", [235, 235, 240, 255]),
            PaletteMode::PluginPicker | PaletteMode::InstrumentPicker if self.search_text.is_empty() => {
                ("Search plugins...", [140, 140, 150, 160])
            }
            _ if self.search_text.is_empty() => {
                ("Search", [140, 140, 150, 160])
            }
            _ => (self.search_text.as_str(), [235, 235, 240, 255]),
        };
        let sfont = 15.0 * scale;
        let sline = 22.0 * scale;
        out.push(TextEntry {
            text: display_text.to_string(),
            x: ppos[0] + 36.0 * scale,
            y: ppos[1] + (PALETTE_INPUT_HEIGHT * scale - sline) * 0.5,
            font_size: sfont,
            line_height: sline,
            max_width: PALETTE_WIDTH * scale - 60.0 * scale,
            color: search_color,
            weight: 400,
            bounds: None,
                center: false,
        });

        match self.mode {
            PaletteMode::VolumeFader => {
                let pad = 16.0 * scale;
                let track_y = list_top + 36.0 * scale;
                let track_h = 6.0 * scale;
                let rms_y = track_y + track_h + 22.0 * scale;

                let pct = (self.fader_value * 100.0) as u32;
                let vol_text = format!("{}%", pct);
                let label_font = 13.0 * scale;
                let label_line = 18.0 * scale;
                out.push(TextEntry {
                    text: vol_text,
                    x: ppos[0] + margin + pad,
                    y: list_top + 14.0 * scale,
                    font_size: label_font,
                    line_height: label_line,
                    max_width: PALETTE_WIDTH * scale - margin * 2.0,
                    color: [200, 200, 210, 220],
                    weight: 400,
                    bounds: None,
                center: false,
                });

                let db_val = if self.fader_rms > 0.0001 {
                    20.0 * self.fader_rms.log10()
                } else {
                    -60.0
                };
                let rms_text = format!("RMS: {:.1} dB", db_val);
                let small_font = 11.0 * scale;
                let small_line = 15.0 * scale;
                out.push(TextEntry {
                    text: rms_text,
                    x: ppos[0] + margin + pad,
                    y: rms_y + 8.0 * scale,
                    font_size: small_font,
                    line_height: small_line,
                    max_width: PALETTE_WIDTH * scale - margin * 2.0,
                    color: [140, 140, 150, 180],
                    weight: 400,
                    bounds: None,
                center: false,
                });
            }
            PaletteMode::Commands => {
                let sect_font = 11.0 * scale;
                let sect_line = 16.0 * scale;
                let ifont = 13.5 * scale;
                let iline = 20.0 * scale;
                let shortcut_font = 12.0 * scale;
                let shortcut_line = 17.0 * scale;

                let mut y = list_top;
                for row in self.visible_rows() {
                    match row {
                        PaletteRow::Section(label) => {
                            out.push(TextEntry {
                                text: label.to_string(),
                                x: ppos[0] + margin + 12.0 * scale,
                                y: y + (PALETTE_SECTION_HEIGHT * scale - sect_line) * 0.5 + 2.0 * scale,
                                font_size: sect_font,
                                line_height: sect_line,
                                max_width: PALETTE_WIDTH * scale - margin * 4.0,
                                color: [120, 140, 170, 200],
                                weight: 400,
                                bounds: None,
                center: false,
                            });
                            y += PALETTE_SECTION_HEIGHT * scale;
                        }
                        PaletteRow::Command(ci) => {
                            let cmd = &COMMANDS[*ci];

                            out.push(TextEntry {
                                text: cmd.name.to_string(),
                                x: ppos[0] + margin + 12.0 * scale,
                                y: y + (PALETTE_ITEM_HEIGHT * scale - iline) * 0.5,
                                font_size: ifont,
                                line_height: iline,
                                max_width: PALETTE_WIDTH * scale * 0.65,
                                color: [215, 215, 222, 255],
                                weight: 400,
                                bounds: None,
                center: false,
                            });

                            if !cmd.shortcut.is_empty() {
                                out.push(TextEntry {
                                    text: cmd.shortcut.to_string(),
                                    x: ppos[0] + PALETTE_WIDTH * scale - margin - 70.0 * scale,
                                    y: y + (PALETTE_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                                    font_size: shortcut_font,
                                    line_height: shortcut_line,
                                    max_width: 80.0 * scale,
                                    color: [120, 120, 135, 180],
                                    weight: 400,
                                    bounds: None,
                center: false,
                                });
                            }

                            y += PALETTE_ITEM_HEIGHT * scale;
                        }
                        PaletteRow::Plugin(pi) => {
                            let entry = &self.plugin_entries[*pi];

                            // Plugin name
                            out.push(TextEntry {
                                text: entry.name.clone(),
                                x: ppos[0] + margin + 12.0 * scale,
                                y: y + (PALETTE_ITEM_HEIGHT * scale - iline) * 0.5,
                                font_size: ifont,
                                line_height: iline,
                                max_width: PALETTE_WIDTH * scale * 0.55,
                                color: [215, 215, 222, 255],
                                weight: 400,
                                bounds: None,
                center: false,
                            });

                            // Type label pill: "Instrument" or "Effect"
                            let label = if entry.is_instrument { "Instrument" } else { "Effect" };
                            let color = if entry.is_instrument {
                                [100, 160, 255, 220]
                            } else {
                                [255, 170, 80, 220]
                            };
                            let label_font = 10.5 * scale;
                            let label_line = 14.0 * scale;
                            let pill_w = if entry.is_instrument { 72.0 } else { 44.0 };
                            let pill_h = 20.0 * scale;
                            let pill_x = ppos[0] + PALETTE_WIDTH * scale - margin - (pill_w + 10.0) * scale;
                            let pill_y = y + (PALETTE_ITEM_HEIGHT * scale - pill_h) * 0.5;
                            out.push(TextEntry {
                                text: label.to_string(),
                                x: pill_x + (pill_w * scale - pill_w * scale * 0.9) * 0.5,
                                y: pill_y + (pill_h - label_line) * 0.5,
                                font_size: label_font,
                                line_height: label_line,
                                max_width: pill_w * scale,
                                color,
                                weight: 400,
                                bounds: None,
                center: false,
                            });

                            y += PALETTE_ITEM_HEIGHT * scale;
                        }
                    }
                }
            }
            PaletteMode::PluginPicker | PaletteMode::InstrumentPicker => {
                let ifont = 13.5 * scale;
                let iline = 20.0 * scale;
                let mfont = 11.0 * scale;
                let mline = 16.0 * scale;

                let y_offset = self.plugin_scroll_y_offset(scale);
                let mut y = list_top - y_offset;
                for &entry_idx in self.visible_plugin_entries(scale) {
                    if let Some(entry) = self.plugin_entries.get(entry_idx) {
                        // Plugin name
                        out.push(TextEntry {
                            text: entry.name.clone(),
                            x: ppos[0] + margin + 12.0 * scale,
                            y: y + (PALETTE_ITEM_HEIGHT * scale - iline) * 0.5,
                            font_size: ifont,
                            line_height: iline,
                            max_width: PALETTE_WIDTH * scale * 0.65,
                            color: [215, 215, 222, 255],
                            weight: 400,
                            bounds: None,
                center: false,
                        });

                        // Manufacturer (right-aligned, dimmer)
                        if !entry.manufacturer.is_empty() {
                            out.push(TextEntry {
                                text: entry.manufacturer.clone(),
                                x: ppos[0] + PALETTE_WIDTH * scale - margin - 130.0 * scale,
                                y: y + (PALETTE_ITEM_HEIGHT * scale - mline) * 0.5,
                                font_size: mfont,
                                line_height: mline,
                                max_width: 140.0 * scale,
                                color: [120, 120, 135, 180],
                                weight: 400,
                                bounds: None,
                center: false,
                            });
                        }

                        y += PALETTE_ITEM_HEIGHT * scale;
                    }
                }
            }
        }

        out
    }
}
