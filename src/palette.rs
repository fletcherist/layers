use crate::InstanceRaw;

pub const PALETTE_WIDTH: f32 = 520.0;
pub const PALETTE_INPUT_HEIGHT: f32 = 52.0;
pub const PALETTE_ITEM_HEIGHT: f32 = 38.0;
pub const PALETTE_SECTION_HEIGHT: f32 = 28.0;
pub const PALETTE_MAX_VISIBLE_ROWS: usize = 14;
pub const PALETTE_PADDING: f32 = 6.0;
pub const PALETTE_BORDER_RADIUS: f32 = 12.0;

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
}

#[derive(Clone, Copy, PartialEq)]
pub enum PaletteMode {
    Commands,
    VolumeFader,
}

pub const FADER_CONTENT_HEIGHT: f32 = 90.0;
const FADER_TRACK_H: f32 = 6.0;
const FADER_THUMB_R: f32 = 9.0;
const FADER_MARGIN_TOP: f32 = 36.0;
const RMS_BAR_H: f32 = 4.0;
const RMS_MARGIN_TOP: f32 = 22.0;

pub struct CommandDef {
    pub name: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
    pub action: CommandAction,
}

pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "Select All",
        shortcut: "⌘A",
        category: "Suggestions",
        action: CommandAction::SelectAll,
    },
    CommandDef {
        name: "Copy",
        shortcut: "⌘C",
        category: "Edit",
        action: CommandAction::Copy,
    },
    CommandDef {
        name: "Paste",
        shortcut: "⌘V",
        category: "Edit",
        action: CommandAction::Paste,
    },
    CommandDef {
        name: "Delete",
        shortcut: "⌫",
        category: "Edit",
        action: CommandAction::Delete,
    },
    CommandDef {
        name: "Undo",
        shortcut: "⌘Z",
        category: "Edit",
        action: CommandAction::Undo,
    },
    CommandDef {
        name: "Redo",
        shortcut: "⇧⌘Z",
        category: "Edit",
        action: CommandAction::Redo,
    },
    CommandDef {
        name: "Zoom In",
        shortcut: "⌘+",
        category: "View",
        action: CommandAction::ZoomIn,
    },
    CommandDef {
        name: "Zoom Out",
        shortcut: "⌘−",
        category: "View",
        action: CommandAction::ZoomOut,
    },
    CommandDef {
        name: "Reset Zoom",
        shortcut: "⌘0",
        category: "View",
        action: CommandAction::ResetZoom,
    },
    CommandDef {
        name: "Toggle Sample Browser",
        shortcut: "⌘B",
        category: "View",
        action: CommandAction::ToggleBrowser,
    },
    CommandDef {
        name: "Save Project",
        shortcut: "⌘S",
        category: "Project",
        action: CommandAction::SaveProject,
    },
    CommandDef {
        name: "Add Folder to Browser",
        shortcut: "⇧⌘A",
        category: "Project",
        action: CommandAction::AddFolderToBrowser,
    },
    CommandDef {
        name: "Set Master Volume",
        shortcut: "",
        category: "Audio",
        action: CommandAction::SetMasterVolume,
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
    pub mode: PaletteMode,
    pub fader_value: f32,
    pub fader_rms: f32,
    pub fader_dragging: bool,
}

impl CommandPalette {
    pub fn new() -> Self {
        let mut p = Self {
            search_text: String::new(),
            rows: Vec::new(),
            command_count: 0,
            selected_index: 0,
            mode: PaletteMode::Commands,
            fader_value: 1.0,
            fader_rms: 0.0,
            fader_dragging: false,
        };
        p.rebuild_rows();
        p
    }

    fn rebuild_rows(&mut self) {
        let query = self.search_text.to_lowercase();
        let is_searching = !query.is_empty();

        let matched: Vec<usize> = COMMANDS
            .iter()
            .enumerate()
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
    }

    pub fn update_filter(&mut self) {
        self.rebuild_rows();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.command_count == 0 {
            return;
        }
        let len = self.command_count as i32;
        self.selected_index = ((self.selected_index as i32 + delta).rem_euclid(len)) as usize;
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
        if self.mode == PaletteMode::VolumeFader {
            return &[];
        }
        let n = self.rows.len().min(PALETTE_MAX_VISIBLE_ROWS);
        &self.rows[..n]
    }

    pub fn content_height(&self, scale: f32) -> f32 {
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

    /// Returns the command-relative index (not the row index) if mouse is on a command row.
    pub fn item_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        if self.mode == PaletteMode::VolumeFader {
            return None;
        }
        let (rp, _) = self.palette_rect(screen_w, screen_h, scale);
        let list_top = rp[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;
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
                    PaletteRow::Command(_) => Some(cmd_i),
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

    pub fn fader_hit_test(
        &self,
        mouse: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> bool {
        if self.mode != PaletteMode::VolumeFader {
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
                let mut cmd_i = 0;
                for row in self.visible_rows() {
                    match row {
                        PaletteRow::Section(_) => {
                            y += PALETTE_SECTION_HEIGHT * scale;
                        }
                        PaletteRow::Command(_) => {
                            if cmd_i == self.selected_index {
                                out.push(InstanceRaw {
                                    position: [pos[0] + margin, y],
                                    size: [
                                        size[0] - margin * 2.0,
                                        PALETTE_ITEM_HEIGHT * scale,
                                    ],
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

                // Fader track background
                out.push(InstanceRaw {
                    position: tp,
                    size: ts,
                    color: [0.25, 0.25, 0.30, 1.0],
                    border_radius: ts[1] * 0.5,
                });

                // Fader filled portion
                let fill_w = self.fader_value * ts[0];
                if fill_w > 0.5 {
                    out.push(InstanceRaw {
                        position: tp,
                        size: [fill_w, ts[1]],
                        color: [0.40, 0.72, 1.00, 1.0],
                        border_radius: ts[1] * 0.5,
                    });
                }

                // Thumb
                let thumb_r = FADER_THUMB_R * scale;
                let thumb_x = tp[0] + fill_w - thumb_r;
                let thumb_cy = tp[1] + ts[1] * 0.5 - thumb_r;
                out.push(InstanceRaw {
                    position: [thumb_x, thumb_cy],
                    size: [thumb_r * 2.0, thumb_r * 2.0],
                    color: [1.0, 1.0, 1.0, 0.95],
                    border_radius: thumb_r,
                });

                // RMS bar background
                let rms_y = tp[1] + ts[1] + RMS_MARGIN_TOP * scale;
                let rms_h = RMS_BAR_H * scale;
                out.push(InstanceRaw {
                    position: [tp[0], rms_y],
                    size: [ts[0], rms_h],
                    color: [0.20, 0.20, 0.25, 1.0],
                    border_radius: rms_h * 0.5,
                });

                // RMS bar filled
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
        }

        out
    }
}
