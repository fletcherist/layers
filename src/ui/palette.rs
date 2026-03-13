use crate::InstanceRaw;

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
    ToggleGrid,
    SetGridAdaptive(AdaptiveGridSize),
    SetGridFixed(FixedGrid),
    NarrowGrid,
    WidenGrid,
    ToggleTripletGrid,
    TestToast,
    RevealInFinder,
    ReverseSample,
    SetSampleVolume,
    SplitSample,
    AddLoopArea,
    AddEffectsArea,
    AddRenderArea,
    SetSampleColor(usize),
}

#[derive(Clone, Copy, PartialEq)]
pub enum PaletteMode {
    Commands,
    VolumeFader,
    SampleVolumeFader,
}

pub const FADER_CONTENT_HEIGHT: f32 = 90.0;
pub const SAMPLE_FADER_CONTENT_HEIGHT: f32 = 220.0;
const FADER_TRACK_H: f32 = 6.0;
const FADER_THUMB_R: f32 = 9.0;
const FADER_MARGIN_TOP: f32 = 36.0;
const RMS_BAR_H: f32 = 4.0;
const RMS_MARGIN_TOP: f32 = 22.0;

const SAMPLE_FADER_TRACK_W: f32 = 6.0;
const SAMPLE_FADER_TRACK_H: f32 = 180.0;
const DB_MIN: f32 = -60.0;
const DB_MAX: f32 = 6.0;
const DB_RANGE: f32 = DB_MAX - DB_MIN; // 66.0

pub fn gain_to_db(gain: f32) -> f32 {
    if gain < 0.00001 { -100.0 } else { 20.0 * gain.log10() }
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
        name: "Set Sample Volume",
        shortcut: "",
        category: "Audio",
        action: CommandAction::SetSampleVolume,
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
        name: "Add Render Area",
        shortcut: "",
        category: "Audio",
        action: CommandAction::AddRenderArea,
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
    pub fader_target_waveform: Option<usize>,
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
            fader_target_waveform: None,
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

        if self.command_count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= self.command_count {
            self.selected_index = self.command_count - 1;
        }
        self.scroll_row_offset = 0;
        self.ensure_selected_visible();
    }

    pub fn update_filter(&mut self, dev_mode: bool) {
        self.rebuild_rows(dev_mode);
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
            if let PaletteRow::Command(_) = row {
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

    pub fn visible_command_offset(&self) -> usize {
        let mut count = 0;
        for row in &self.rows[..self.scroll_row_offset] {
            if matches!(row, PaletteRow::Command(_)) {
                count += 1;
            }
        }
        count
    }

    pub fn selected_action(&self) -> Option<CommandAction> {
        let mut cmd_i = 0;
        for row in &self.rows {
            if let PaletteRow::Command(ci) = row {
                if cmd_i == self.selected_index {
                    return Some(COMMANDS[*ci].action);
                }
                cmd_i += 1;
            }
        }
        None
    }

    pub fn visible_rows(&self) -> &[PaletteRow] {
        if matches!(
            self.mode,
            PaletteMode::VolumeFader | PaletteMode::SampleVolumeFader
        ) {
            return &[];
        }
        let start = self.scroll_row_offset.min(self.rows.len());
        let end = (start + PALETTE_MAX_VISIBLE_ROWS).min(self.rows.len());
        &self.rows[start..end]
    }

    pub fn content_height(&self, scale: f32) -> f32 {
        if self.mode == PaletteMode::SampleVolumeFader {
            return SAMPLE_FADER_CONTENT_HEIGHT * scale;
        }
        if self.mode == PaletteMode::VolumeFader {
            return FADER_CONTENT_HEIGHT * scale;
        }
        let mut h = 0.0;
        for row in self.visible_rows() {
            h += match row {
                PaletteRow::Section(_) => PALETTE_SECTION_HEIGHT * scale,
                PaletteRow::Command(_) => PALETTE_ITEM_HEIGHT * scale,
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
    pub fn item_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        if matches!(
            self.mode,
            PaletteMode::VolumeFader | PaletteMode::SampleVolumeFader
        ) {
            return None;
        }
        let (rp, _) = self.palette_rect(screen_w, screen_h, scale);
        let list_top = rp[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;
        let base_cmd = self.visible_command_offset();
        let mut y = list_top;
        let mut cmd_i = 0;
        for row in self.visible_rows() {
            let rh = match row {
                PaletteRow::Section(_) => PALETTE_SECTION_HEIGHT * scale,
                PaletteRow::Command(_) => PALETTE_ITEM_HEIGHT * scale,
            };
            if pos[1] >= y && pos[1] < y + rh {
                return match row {
                    PaletteRow::Section(_) => None,
                    PaletteRow::Command(_) => Some(base_cmd + cmd_i),
                };
            }
            if matches!(row, PaletteRow::Command(_)) {
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

    pub fn sample_fader_track_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let (ppos, psize) = self.palette_rect(screen_w, screen_h, scale);
        let track_w = SAMPLE_FADER_TRACK_W * scale;
        let track_h = SAMPLE_FADER_TRACK_H * scale;
        let cx = ppos[0] + psize[0] * 0.5 - track_w * 0.5;
        let top = ppos[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale + 20.0 * scale;
        ([cx, top], [track_w, track_h])
    }

    pub fn fader_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if !matches!(
            self.mode,
            PaletteMode::VolumeFader | PaletteMode::SampleVolumeFader
        ) {
            return false;
        }
        if self.mode == PaletteMode::SampleVolumeFader {
            let (tp, ts) = self.sample_fader_track_rect(screen_w, screen_h, scale);
            let fader_pos = gain_to_fader_pos(self.fader_value);
            let thumb_cx = tp[0] + ts[0] * 0.5;
            let thumb_y = tp[1] + ts[1] * (1.0 - fader_pos);
            let r = FADER_THUMB_R * scale + 4.0 * scale;
            let dx = mouse[0] - thumb_cx;
            let dy = mouse[1] - thumb_y;
            return dx * dx + dy * dy <= r * r;
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

    pub fn sample_fader_drag(&mut self, mouse_y: f32, screen_w: f32, screen_h: f32, scale: f32) {
        let (tp, ts) = self.sample_fader_track_rect(screen_w, screen_h, scale);
        let pos = 1.0 - ((mouse_y - tp[1]) / ts[1]).clamp(0.0, 1.0);
        self.fader_value = fader_pos_to_gain(pos);
    }

    pub fn build_instances(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
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
            color: [0.14, 0.14, 0.17, 0.98],
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
                        PaletteRow::Command(_) => {
                            if base_cmd + cmd_i == self.selected_index {
                                out.push(InstanceRaw {
                                    position: [pos[0] + margin, y],
                                    size: [size[0] - margin * 2.0, PALETTE_ITEM_HEIGHT * scale],
                                    color: [0.26, 0.26, 0.32, 0.8],
                                    border_radius: 6.0 * scale,
                                });
                            }
                            cmd_i += 1;
                            y += PALETTE_ITEM_HEIGHT * scale;
                        }
                    }
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
                        [1.0, 0.35, 0.30, 1.0]
                    } else if self.fader_rms > 0.5 {
                        [1.0, 0.85, 0.32, 1.0]
                    } else {
                        [0.45, 0.92, 0.55, 1.0]
                    };
                    out.push(InstanceRaw {
                        position: [tp[0], rms_y],
                        size: [rms_w, rms_h],
                        color: rms_color,
                        border_radius: rms_h * 0.5,
                    });
                }
            }
            PaletteMode::SampleVolumeFader => {
                let (tp, ts) = self.sample_fader_track_rect(screen_w, screen_h, scale);
                let fader_pos = gain_to_fader_pos(self.fader_value);

                // Vertical track background
                out.push(InstanceRaw {
                    position: tp,
                    size: ts,
                    color: [0.25, 0.25, 0.30, 1.0],
                    border_radius: ts[0] * 0.5,
                });

                // Filled portion from bottom up
                let fill_h = fader_pos * ts[1];
                if fill_h > 0.5 {
                    let fill_y = tp[1] + ts[1] - fill_h;
                    out.push(InstanceRaw {
                        position: [tp[0], fill_y],
                        size: [ts[0], fill_h],
                        color: [0.40, 0.72, 1.00, 1.0],
                        border_radius: ts[0] * 0.5,
                    });
                }

                // 0 dB reference line
                let zero_db_pos = (0.0 - DB_MIN) / DB_RANGE;
                let zero_db_y = tp[1] + ts[1] * (1.0 - zero_db_pos);
                let mark_w = 20.0 * scale;
                out.push(InstanceRaw {
                    position: [tp[0] - mark_w * 0.5 + ts[0] * 0.5 - mark_w * 0.5, zero_db_y - 0.5 * scale],
                    size: [mark_w, 1.0 * scale],
                    color: [1.0, 1.0, 1.0, 0.15],
                    border_radius: 0.0,
                });
                out.push(InstanceRaw {
                    position: [tp[0] + ts[0] + 4.0 * scale, zero_db_y - 0.5 * scale],
                    size: [mark_w, 1.0 * scale],
                    color: [1.0, 1.0, 1.0, 0.15],
                    border_radius: 0.0,
                });

                // Thumb
                let thumb_r = FADER_THUMB_R * scale;
                let thumb_y = tp[1] + ts[1] * (1.0 - fader_pos) - thumb_r;
                let thumb_cx = tp[0] + ts[0] * 0.5 - thumb_r;
                out.push(InstanceRaw {
                    position: [thumb_cx, thumb_y],
                    size: [thumb_r * 2.0, thumb_r * 2.0],
                    color: [1.0, 1.0, 1.0, 0.95],
                    border_radius: thumb_r,
                });
            }
        }

        out
    }
}
