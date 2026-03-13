use crate::settings::{AdaptiveGridSize, FixedGrid, GridMode, Settings};
use crate::ui::palette::CommandAction;
use crate::InstanceRaw;

pub const CTX_MENU_WIDTH: f32 = 220.0;
pub const CTX_MENU_ITEM_HEIGHT: f32 = 32.0;
pub const CTX_MENU_SECTION_HEIGHT: f32 = 26.0;
pub const CTX_MENU_SEPARATOR_HEIGHT: f32 = 9.0;
pub const CTX_MENU_PADDING: f32 = 4.0;
pub const CTX_MENU_BORDER_RADIUS: f32 = 8.0;
pub const CTX_MENU_INLINE_HEIGHT: f32 = 28.0;
const INLINE_PILL_PAD_X: f32 = 7.0;
const INLINE_PILL_GAP: f32 = 2.0;
const INLINE_PILL_HEIGHT: f32 = 22.0;

#[derive(Clone, Copy, PartialEq)]
pub enum MenuContext {
    Canvas,
    Grid,
    Selection {
        has_waveforms: bool,
        has_effect_region: bool,
    },
    ComponentDef,
    ComponentInstance,
    BrowserEntry,
}

pub struct ContextMenuItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: CommandAction,
    pub checked: bool,
}

pub struct InlinePill {
    pub label: &'static str,
    pub action: CommandAction,
    pub active: bool,
}

pub enum ContextMenuEntry {
    Item(ContextMenuItem),
    Separator,
    SectionHeader(&'static str),
    InlineGroup(Vec<InlinePill>),
}

pub struct ContextMenu {
    pub position: [f32; 2],
    pub entries: Vec<ContextMenuEntry>,
    pub hovered_index: Option<usize>,
    pub context: MenuContext,
}

fn estimate_pill_width(label: &str, scale: f32) -> f32 {
    let font_size = 11.0 * scale;
    label.len() as f32 * font_size * 0.55 + INLINE_PILL_PAD_X * 2.0 * scale
}

fn grid_entries(settings: &Settings) -> Vec<ContextMenuEntry> {
    let mut entries = Vec::new();

    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Snap to Grid",
        shortcut: "⌘4",
        action: CommandAction::ToggleSnapToGrid,
        checked: settings.snap_to_grid,
    }));
    entries.push(ContextMenuEntry::Separator);

    let is_adaptive = matches!(settings.grid_mode, GridMode::Adaptive(_));

    entries.push(ContextMenuEntry::SectionHeader("Grid Mode:"));
    entries.push(ContextMenuEntry::InlineGroup(vec![
        InlinePill {
            label: "Adaptive",
            action: CommandAction::SetGridAdaptive(
                if let GridMode::Adaptive(s) = settings.grid_mode {
                    s
                } else {
                    AdaptiveGridSize::Medium
                },
            ),
            active: is_adaptive,
        },
        InlinePill {
            label: "Fixed",
            action: CommandAction::SetGridFixed(if let GridMode::Fixed(f) = settings.grid_mode {
                f
            } else {
                FixedGrid::Quarter
            }),
            active: !is_adaptive,
        },
    ]));

    entries.push(ContextMenuEntry::SectionHeader("Grid Size:"));
    if is_adaptive {
        let sizes = [
            AdaptiveGridSize::Widest,
            AdaptiveGridSize::Wide,
            AdaptiveGridSize::Medium,
            AdaptiveGridSize::Narrow,
            AdaptiveGridSize::Narrowest,
        ];
        entries.push(ContextMenuEntry::InlineGroup(
            sizes
                .iter()
                .map(|&s| InlinePill {
                    label: s.label(),
                    action: CommandAction::SetGridAdaptive(s),
                    active: matches!(settings.grid_mode, GridMode::Adaptive(cur) if cur == s),
                })
                .collect(),
        ));
    } else {
        let fine = [
            FixedGrid::Bars8,
            FixedGrid::Bars4,
            FixedGrid::Bars2,
            FixedGrid::Bar1,
        ];
        entries.push(ContextMenuEntry::InlineGroup(
            fine.iter()
                .map(|&f| InlinePill {
                    label: f.label(),
                    action: CommandAction::SetGridFixed(f),
                    active: matches!(settings.grid_mode, GridMode::Fixed(cur) if cur == f),
                })
                .collect(),
        ));
        let subdivisions = [
            FixedGrid::Half,
            FixedGrid::Quarter,
            FixedGrid::Eighth,
            FixedGrid::Sixteenth,
            FixedGrid::ThirtySecond,
        ];
        entries.push(ContextMenuEntry::InlineGroup(
            subdivisions
                .iter()
                .map(|&f| InlinePill {
                    label: f.label(),
                    action: CommandAction::SetGridFixed(f),
                    active: matches!(settings.grid_mode, GridMode::Fixed(cur) if cur == f),
                })
                .collect(),
        ));
    }

    entries.push(ContextMenuEntry::Separator);
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Narrow Grid",
        shortcut: "⌘1",
        action: CommandAction::NarrowGrid,
        checked: false,
    }));
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Widen Grid",
        shortcut: "⌘2",
        action: CommandAction::WidenGrid,
        checked: false,
    }));
    entries.push(ContextMenuEntry::Separator);
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Triplet Grid",
        shortcut: "⌘3",
        action: CommandAction::ToggleTripletGrid,
        checked: settings.triplet_grid,
    }));
    entries.push(ContextMenuEntry::Separator);
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: if settings.grid_enabled {
            "Disable Grid"
        } else {
            "Enable Grid"
        },
        shortcut: "",
        action: CommandAction::ToggleGrid,
        checked: false,
    }));

    entries
}

fn entry_height(entry: &ContextMenuEntry, scale: f32) -> f32 {
    match entry {
        ContextMenuEntry::Item(_) => CTX_MENU_ITEM_HEIGHT * scale,
        ContextMenuEntry::Separator => CTX_MENU_SEPARATOR_HEIGHT * scale,
        ContextMenuEntry::SectionHeader(_) => CTX_MENU_SECTION_HEIGHT * scale,
        ContextMenuEntry::InlineGroup(_) => CTX_MENU_INLINE_HEIGHT * scale,
    }
}

impl ContextMenu {
    pub fn new(pos: [f32; 2], context: MenuContext, settings: &Settings) -> Self {
        let entries = match context {
            MenuContext::ComponentInstance => vec![
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Go to Component",
                    shortcut: "",
                    action: CommandAction::GoToComponent,
                    checked: false,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Copy",
                    shortcut: "⌘C",
                    action: CommandAction::Copy,
                    checked: false,
                }),
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Duplicate",
                    shortcut: "⌘D",
                    action: CommandAction::Duplicate,
                    checked: false,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Delete",
                    shortcut: "⌫",
                    action: CommandAction::Delete,
                    checked: false,
                }),
            ],
            MenuContext::ComponentDef => vec![
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Create Instance",
                    shortcut: "",
                    action: CommandAction::CreateInstance,
                    checked: false,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Duplicate",
                    shortcut: "⌘D",
                    action: CommandAction::Duplicate,
                    checked: false,
                }),
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Delete",
                    shortcut: "⌫",
                    action: CommandAction::Delete,
                    checked: false,
                }),
            ],
            MenuContext::Selection {
                has_waveforms,
                has_effect_region,
            } => {
                let mut entries = vec![];
                if has_effect_region {
                    entries.push(ContextMenuEntry::Item(ContextMenuItem {
                        label: "Rename",
                        shortcut: "⌘R",
                        action: CommandAction::RenameEffectRegion,
                        checked: false,
                    }));
                    entries.push(ContextMenuEntry::Separator);
                } else if has_waveforms {
                    entries.push(ContextMenuEntry::Item(ContextMenuItem {
                        label: "Rename",
                        shortcut: "⌘R",
                        action: CommandAction::RenameSample,
                        checked: false,
                    }));
                    entries.push(ContextMenuEntry::Separator);
                }
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Copy",
                    shortcut: "⌘C",
                    action: CommandAction::Copy,
                    checked: false,
                }));
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Paste",
                    shortcut: "⌘V",
                    action: CommandAction::Paste,
                    checked: false,
                }));
                entries.push(ContextMenuEntry::Separator);
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Duplicate",
                    shortcut: "⌘D",
                    action: CommandAction::Duplicate,
                    checked: false,
                }));
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Delete",
                    shortcut: "⌫",
                    action: CommandAction::Delete,
                    checked: false,
                }));
                if has_waveforms {
                    entries.push(ContextMenuEntry::Separator);
                    entries.push(ContextMenuEntry::Item(ContextMenuItem {
                        label: "Create Component",
                        shortcut: "",
                        action: CommandAction::CreateComponent,
                        checked: false,
                    }));
                }
                entries.push(ContextMenuEntry::Separator);
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Select All",
                    shortcut: "⌘A",
                    action: CommandAction::SelectAll,
                    checked: false,
                }));
                entries
            }
            MenuContext::Canvas => vec![
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Paste",
                    shortcut: "⌘V",
                    action: CommandAction::Paste,
                    checked: false,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Select All",
                    shortcut: "⌘A",
                    action: CommandAction::SelectAll,
                    checked: false,
                }),
            ],
            MenuContext::Grid => grid_entries(settings),
            MenuContext::BrowserEntry => vec![ContextMenuEntry::Item(ContextMenuItem {
                label: "Reveal in Finder",
                shortcut: "⌥⌘R",
                action: CommandAction::RevealInFinder,
                checked: false,
            })],
        };
        Self {
            position: pos,
            entries,
            hovered_index: None,
            context,
        }
    }

    pub fn content_height(&self, scale: f32) -> f32 {
        self.entries.iter().map(|e| entry_height(e, scale)).sum()
    }

    pub fn menu_rect(&self, screen_w: f32, screen_h: f32, scale: f32) -> ([f32; 2], [f32; 2]) {
        let w = CTX_MENU_WIDTH * scale;
        let h = self.content_height(scale) + CTX_MENU_PADDING * 2.0 * scale;
        let mut x = self.position[0];
        let mut y = self.position[1];
        if x + w > screen_w {
            x = screen_w - w;
        }
        if y + h > screen_h {
            y = screen_h - h;
        }
        ([x, y], [w, h])
    }

    pub fn contains(&self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) -> bool {
        let (rp, rs) = self.menu_rect(screen_w, screen_h, scale);
        pos[0] >= rp[0] && pos[0] <= rp[0] + rs[0] && pos[1] >= rp[1] && pos[1] <= rp[1] + rs[1]
    }

    /// Returns a flat item index for the entry under `pos`.
    /// InlineGroup pills each count as one item.
    pub fn item_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let (rp, _rs) = self.menu_rect(screen_w, screen_h, scale);
        let pad = CTX_MENU_PADDING * scale;
        let mut y = rp[1] + pad;
        let mut item_i = 0;
        for entry in &self.entries {
            let rh = entry_height(entry, scale);
            if pos[1] >= y && pos[1] < y + rh {
                match entry {
                    ContextMenuEntry::Item(_) => return Some(item_i),
                    ContextMenuEntry::InlineGroup(pills) => {
                        let pill_h = INLINE_PILL_HEIGHT * scale;
                        let pill_y = y + (rh - pill_h) * 0.5;
                        if pos[1] < pill_y || pos[1] > pill_y + pill_h {
                            return None;
                        }
                        let mut px = rp[0] + pad + 4.0 * scale;
                        for (pi, pill) in pills.iter().enumerate() {
                            let pw = estimate_pill_width(pill.label, scale);
                            if pos[0] >= px && pos[0] < px + pw {
                                return Some(item_i + pi);
                            }
                            px += pw + INLINE_PILL_GAP * scale;
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
            match entry {
                ContextMenuEntry::Item(_) => item_i += 1,
                ContextMenuEntry::InlineGroup(pills) => item_i += pills.len(),
                _ => {}
            }
            y += rh;
        }
        None
    }

    pub fn action_at(&self, index: usize) -> Option<CommandAction> {
        let mut item_i = 0;
        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Item(item) => {
                    if item_i == index {
                        return Some(item.action);
                    }
                    item_i += 1;
                }
                ContextMenuEntry::InlineGroup(pills) => {
                    for pill in pills {
                        if item_i == index {
                            return Some(pill.action);
                        }
                        item_i += 1;
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn update_hover(&mut self, pos: [f32; 2], screen_w: f32, screen_h: f32, scale: f32) {
        self.hovered_index = self.item_at(pos, screen_w, screen_h, scale);
    }

    pub fn build_instances(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = self.menu_rect(screen_w, screen_h, scale);
        let pad = CTX_MENU_PADDING * scale;

        let so = 6.0 * scale;
        out.push(InstanceRaw {
            position: [pos[0] + so, pos[1] + so],
            size: [size[0] + 2.0 * scale, size[1] + 2.0 * scale],
            color: [0.0, 0.0, 0.0, 0.40],
            border_radius: CTX_MENU_BORDER_RADIUS * scale,
        });

        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.16, 0.16, 0.19, 1.0],
            border_radius: CTX_MENU_BORDER_RADIUS * scale,
        });

        let mut y = pos[1] + pad;
        let mut item_i = 0;
        for entry in &self.entries {
            let rh = entry_height(entry, scale);
            match entry {
                ContextMenuEntry::Item(item) => {
                    if Some(item_i) == self.hovered_index {
                        out.push(InstanceRaw {
                            position: [pos[0] + pad, y],
                            size: [size[0] - pad * 2.0, CTX_MENU_ITEM_HEIGHT * scale],
                            color: [0.26, 0.26, 0.32, 0.8],
                            border_radius: 5.0 * scale,
                        });
                    }
                    if item.checked {
                        let check_sz = 4.0 * scale;
                        let cx = pos[0] + pad + 5.0 * scale;
                        let cy = y + CTX_MENU_ITEM_HEIGHT * scale * 0.5 - check_sz * 0.5;
                        out.push(InstanceRaw {
                            position: [cx, cy],
                            size: [check_sz, check_sz],
                            color: [0.9, 0.9, 0.95, 0.9],
                            border_radius: check_sz * 0.5,
                        });
                    }
                    item_i += 1;
                }
                ContextMenuEntry::Separator => {
                    let sep_y = y + CTX_MENU_SEPARATOR_HEIGHT * scale * 0.5;
                    out.push(InstanceRaw {
                        position: [pos[0] + pad + 4.0 * scale, sep_y],
                        size: [size[0] - (pad + 4.0 * scale) * 2.0, 1.0 * scale],
                        color: [1.0, 1.0, 1.0, 0.08],
                        border_radius: 0.0,
                    });
                }
                ContextMenuEntry::SectionHeader(_) => {}
                ContextMenuEntry::InlineGroup(pills) => {
                    let pill_h = INLINE_PILL_HEIGHT * scale;
                    let pill_r = pill_h * 0.5;
                    let pill_y = y + (rh - pill_h) * 0.5;
                    let mut px = pos[0] + pad + 4.0 * scale;
                    for (pi, pill) in pills.iter().enumerate() {
                        let pw = estimate_pill_width(pill.label, scale);
                        let is_hovered = Some(item_i + pi) == self.hovered_index;
                        if pill.active {
                            out.push(InstanceRaw {
                                position: [px, pill_y],
                                size: [pw, pill_h],
                                color: [0.32, 0.32, 0.40, 0.95],
                                border_radius: pill_r,
                            });
                        } else if is_hovered {
                            out.push(InstanceRaw {
                                position: [px, pill_y],
                                size: [pw, pill_h],
                                color: [0.24, 0.24, 0.30, 0.7],
                                border_radius: pill_r,
                            });
                        }
                        px += pw + INLINE_PILL_GAP * scale;
                    }
                    item_i += pills.len();
                }
            }
            y += rh;
        }

        out
    }
}
