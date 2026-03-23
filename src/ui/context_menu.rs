use crate::gpu::TextEntry;
use crate::layers::LayerNodeKind;
use crate::settings::{AdaptiveGridSize, FixedGrid, GridMode, Settings};
use crate::ui::palette::CommandAction;
use crate::InstanceRaw;
use crate::theme::{WAVEFORM_COLORS, WAVEFORM_COLOR_COLS, WAVEFORM_COLOR_ROWS, SCROLLBAR_BG};

pub const CTX_MENU_WIDTH: f32 = 260.0;
pub const CTX_MENU_ITEM_HEIGHT: f32 = 26.0;
pub const CTX_MENU_SECTION_HEIGHT: f32 = 22.0;
pub const CTX_MENU_SEPARATOR_HEIGHT: f32 = 7.0;
pub const CTX_MENU_PADDING: f32 = 3.0;
pub const CTX_MENU_BORDER_RADIUS: f32 = 8.0;
pub const CTX_MENU_INLINE_HEIGHT: f32 = 24.0;
pub const CTX_MENU_SWATCH_HEIGHT: f32 = 17.0;
const INLINE_PILL_PAD_X: f32 = 7.0;
const INLINE_PILL_GAP: f32 = 2.0;
const INLINE_PILL_HEIGHT: f32 = 22.0;
const COLOR_SWATCH_SIZE: f32 = 15.0;
const COLOR_SWATCH_GAP: f32 = 2.0;
const COLOR_SWATCH_RING: f32 = 2.0;

#[derive(Clone, Copy, PartialEq)]
pub enum MenuContext {
    Canvas,
    Grid,
    Selection {
        has_waveforms: bool,
        has_effect_region: bool,
        current_waveform_color: Option<[f32; 4]>,
    },
    ComponentDef,
    ComponentInstance,
    BrowserEntry,
    LayerNode { kind: LayerNodeKind },
    MidiClipEdit {
        grid_mode: GridMode,
        triplet_grid: bool,
    },
    WarpModeSelect {
        current: crate::ui::waveform::WarpMode,
    },
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

#[derive(Clone)]
pub struct ColorSwatch {
    pub color: [f32; 4],
    pub action: CommandAction,
    pub active: bool,
}

pub enum ContextMenuEntry {
    Item(ContextMenuItem),
    Separator,
    SectionHeader(&'static str),
    InlineGroup(Vec<InlinePill>),
    ColorSwatchGroup(Vec<ColorSwatch>),
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
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Snap to Vertical Grid",
        shortcut: "",
        action: CommandAction::ToggleVerticalSnap,
        checked: settings.snap_to_vertical_grid,
    }));
    entries.push(ContextMenuEntry::Separator);

    entries.push(ContextMenuEntry::SectionHeader("Fixed Grid:"));
    let bars = [
        FixedGrid::Bars8,
        FixedGrid::Bars4,
        FixedGrid::Bars2,
        FixedGrid::Bar1,
    ];
    entries.push(ContextMenuEntry::InlineGroup(
        bars.iter()
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

    entries.push(ContextMenuEntry::SectionHeader("Adaptive Grid:"));
    let adaptive_row1 = [
        AdaptiveGridSize::Widest,
        AdaptiveGridSize::Wide,
        AdaptiveGridSize::Medium,
        AdaptiveGridSize::Narrow,
    ];
    entries.push(ContextMenuEntry::InlineGroup(
        adaptive_row1
            .iter()
            .map(|&s| InlinePill {
                label: s.label(),
                action: CommandAction::SetGridAdaptive(s),
                active: matches!(settings.grid_mode, GridMode::Adaptive(cur) if cur == s),
            })
            .collect(),
    ));
    entries.push(ContextMenuEntry::InlineGroup(vec![InlinePill {
        label: AdaptiveGridSize::Narrowest.label(),
        action: CommandAction::SetGridAdaptive(AdaptiveGridSize::Narrowest),
        active: matches!(
            settings.grid_mode,
            GridMode::Adaptive(AdaptiveGridSize::Narrowest)
        ),
    }]));

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

fn midi_clip_grid_entries(grid_mode: GridMode, triplet_grid: bool) -> Vec<ContextMenuEntry> {
    let mut entries = Vec::new();

    entries.push(ContextMenuEntry::SectionHeader("Clip Grid:"));

    entries.push(ContextMenuEntry::SectionHeader("Fixed:"));
    let bars = [
        FixedGrid::Bars8,
        FixedGrid::Bars4,
        FixedGrid::Bars2,
        FixedGrid::Bar1,
    ];
    entries.push(ContextMenuEntry::InlineGroup(
        bars.iter()
            .map(|&f| InlinePill {
                label: f.label(),
                action: CommandAction::SetMidiClipGridFixed(f),
                active: matches!(grid_mode, GridMode::Fixed(cur) if cur == f),
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
                action: CommandAction::SetMidiClipGridFixed(f),
                active: matches!(grid_mode, GridMode::Fixed(cur) if cur == f),
            })
            .collect(),
    ));

    entries.push(ContextMenuEntry::SectionHeader("Adaptive:"));
    let adaptive_row1 = [
        AdaptiveGridSize::Widest,
        AdaptiveGridSize::Wide,
        AdaptiveGridSize::Medium,
        AdaptiveGridSize::Narrow,
    ];
    entries.push(ContextMenuEntry::InlineGroup(
        adaptive_row1
            .iter()
            .map(|&s| InlinePill {
                label: s.label(),
                action: CommandAction::SetMidiClipGridAdaptive(s),
                active: matches!(grid_mode, GridMode::Adaptive(cur) if cur == s),
            })
            .collect(),
    ));
    entries.push(ContextMenuEntry::InlineGroup(vec![InlinePill {
        label: AdaptiveGridSize::Narrowest.label(),
        action: CommandAction::SetMidiClipGridAdaptive(AdaptiveGridSize::Narrowest),
        active: matches!(grid_mode, GridMode::Adaptive(AdaptiveGridSize::Narrowest)),
    }]));

    entries.push(ContextMenuEntry::Separator);
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Narrow Grid",
        shortcut: "",
        action: CommandAction::NarrowMidiClipGrid,
        checked: false,
    }));
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Widen Grid",
        shortcut: "",
        action: CommandAction::WidenMidiClipGrid,
        checked: false,
    }));
    entries.push(ContextMenuEntry::Separator);
    entries.push(ContextMenuEntry::Item(ContextMenuItem {
        label: "Triplet Grid",
        shortcut: "",
        action: CommandAction::ToggleMidiClipTripletGrid,
        checked: triplet_grid,
    }));

    entries
}

fn entry_height(entry: &ContextMenuEntry, scale: f32) -> f32 {
    match entry {
        ContextMenuEntry::Item(_) => CTX_MENU_ITEM_HEIGHT * scale,
        ContextMenuEntry::Separator => CTX_MENU_SEPARATOR_HEIGHT * scale,
        ContextMenuEntry::SectionHeader(_) => CTX_MENU_SECTION_HEIGHT * scale,
        ContextMenuEntry::InlineGroup(_) => CTX_MENU_INLINE_HEIGHT * scale,
        ContextMenuEntry::ColorSwatchGroup(_) => CTX_MENU_SWATCH_HEIGHT * scale,
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
                current_waveform_color,
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
                if has_waveforms {
                    fn colors_match(a: [f32; 4], b: [f32; 4]) -> bool {
                        (a[0] - b[0]).abs() < 0.01
                            && (a[1] - b[1]).abs() < 0.01
                            && (a[2] - b[2]).abs() < 0.01
                    }
                    let all_swatches: Vec<ColorSwatch> = WAVEFORM_COLORS
                        .iter()
                        .enumerate()
                        .map(|(i, &c)| ColorSwatch {
                            color: c,
                            action: CommandAction::SetSampleColor(i),
                            active: current_waveform_color
                                .map_or(false, |cur| colors_match(cur, c)),
                        })
                        .collect();
                    entries.push(ContextMenuEntry::SectionHeader("Color:"));
                    for row in 0..WAVEFORM_COLOR_ROWS {
                        let start = row * WAVEFORM_COLOR_COLS;
                        let end = (start + WAVEFORM_COLOR_COLS).min(all_swatches.len());
                        entries.push(ContextMenuEntry::ColorSwatchGroup(
                            all_swatches[start..end].to_vec(),
                        ));
                    }
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
                        label: "Split Here",
                        shortcut: "⌘E",
                        action: CommandAction::SplitSample,
                        checked: false,
                    }));
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
                    label: "Add Text Note",
                    shortcut: "",
                    action: CommandAction::AddTextNote,
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
            MenuContext::MidiClipEdit {
                grid_mode,
                triplet_grid,
            } => midi_clip_grid_entries(grid_mode, triplet_grid),
            MenuContext::BrowserEntry => vec![ContextMenuEntry::Item(ContextMenuItem {
                label: "Reveal in Finder",
                shortcut: "⌥⌘R",
                action: CommandAction::RevealInFinder,
                checked: false,
            })],
            MenuContext::LayerNode { kind } => {
                let mut entries = vec![];
                match kind {
                    LayerNodeKind::Waveform => {
                        entries.push(ContextMenuEntry::Item(ContextMenuItem {
                            label: "Rename",
                            shortcut: "",
                            action: CommandAction::RenameSample,
                            checked: false,
                        }));
                        entries.push(ContextMenuEntry::Separator);
                    }
                    LayerNodeKind::EffectRegion => {
                        entries.push(ContextMenuEntry::Item(ContextMenuItem {
                            label: "Rename",
                            shortcut: "",
                            action: CommandAction::RenameEffectRegion,
                            checked: false,
                        }));
                        entries.push(ContextMenuEntry::Separator);
                    }
                    LayerNodeKind::Instrument => {
                        entries.push(ContextMenuEntry::Item(ContextMenuItem {
                            label: "Open",
                            shortcut: "",
                            action: CommandAction::OpenInstrumentGui,
                            checked: false,
                        }));
                        entries.push(ContextMenuEntry::Separator);
                    }
                    _ => {}
                }
                entries.push(ContextMenuEntry::Item(ContextMenuItem {
                    label: "Delete",
                    shortcut: "⌫",
                    action: CommandAction::Delete,
                    checked: false,
                }));
                entries
            }
            MenuContext::WarpModeSelect { current } => {
                use crate::ui::waveform::WarpMode;
                vec![
                    ContextMenuEntry::Item(ContextMenuItem {
                        label: "Semitone",
                        shortcut: "",
                        action: CommandAction::SetWarpSemitone,
                        checked: current == WarpMode::Semitone,
                    }),
                    ContextMenuEntry::Item(ContextMenuItem {
                        label: "Re-Pitch",
                        shortcut: "",
                        action: CommandAction::SetWarpRePitch,
                        checked: current == WarpMode::RePitch,
                    }),
                ]
            }
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
                    ContextMenuEntry::ColorSwatchGroup(swatches) => {
                        let sz = COLOR_SWATCH_SIZE * scale;
                        let swatch_y = y + (rh - sz) * 0.5;
                        if pos[1] < swatch_y || pos[1] > swatch_y + sz {
                            return None;
                        }
                        let mut px = rp[0] + pad + 4.0 * scale;
                        for (si, _) in swatches.iter().enumerate() {
                            if pos[0] >= px && pos[0] < px + sz {
                                return Some(item_i + si);
                            }
                            px += sz + COLOR_SWATCH_GAP * scale;
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
            match entry {
                ContextMenuEntry::Item(_) => item_i += 1,
                ContextMenuEntry::InlineGroup(pills) => item_i += pills.len(),
                ContextMenuEntry::ColorSwatchGroup(swatches) => item_i += swatches.len(),
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
                ContextMenuEntry::ColorSwatchGroup(swatches) => {
                    for swatch in swatches {
                        if item_i == index {
                            return Some(swatch.action);
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

    pub fn build_instances(&self, settings: &crate::settings::Settings, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let (pos, size) = self.menu_rect(screen_w, screen_h, scale);
        let pad = CTX_MENU_PADDING * scale;

        let so = 6.0 * scale;
        out.push(InstanceRaw {
            position: [pos[0] + so, pos[1] + so],
            size: [size[0] + 2.0 * scale, size[1] + 2.0 * scale],
            color: settings.theme.shadow,
            border_radius: CTX_MENU_BORDER_RADIUS * scale,
        });

        out.push(InstanceRaw {
            position: pos,
            size,
            color: settings.theme.bg_menu,
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
                            color: settings.theme.option_highlight,
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
                            color: crate::theme::with_alpha(settings.theme.text_primary, 0.9),
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
                        color: SCROLLBAR_BG,
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
                                color: crate::theme::with_alpha(settings.theme.pill_active, 0.95),
                                border_radius: pill_r,
                            });
                        } else if is_hovered {
                            out.push(InstanceRaw {
                                position: [px, pill_y],
                                size: [pw, pill_h],
                                color: crate::theme::with_alpha(settings.theme.pill_inactive, 0.7),
                                border_radius: pill_r,
                            });
                        }
                        px += pw + INLINE_PILL_GAP * scale;
                    }
                    item_i += pills.len();
                }
                ContextMenuEntry::ColorSwatchGroup(swatches) => {
                    let sz = COLOR_SWATCH_SIZE * scale;
                    let r = 2.0 * scale;
                    let swatch_y = y + (rh - sz) * 0.5;
                    let mut px = pos[0] + pad + 4.0 * scale;
                    for (si, swatch) in swatches.iter().enumerate() {
                        let is_hovered = Some(item_i + si) == self.hovered_index;
                        if swatch.active || is_hovered {
                            let ring = COLOR_SWATCH_RING * scale;
                            out.push(InstanceRaw {
                                position: [px - ring, swatch_y - ring],
                                size: [sz + ring * 2.0, sz + ring * 2.0],
                                color: [1.0, 1.0, 1.0, if swatch.active { 0.9 } else { 0.4 }],
                                border_radius: r + ring,
                            });
                        }
                        out.push(InstanceRaw {
                            position: [px, swatch_y],
                            size: [sz, sz],
                            color: swatch.color,
                            border_radius: r,
                        });
                        px += sz + COLOR_SWATCH_GAP * scale;
                    }
                    item_i += swatches.len();
                }
            }
            y += rh;
        }

        out
    }

    pub fn get_text_entries(&self, theme: &crate::theme::RuntimeTheme, screen_w: f32, screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let (mpos, _msize) = self.menu_rect(screen_w, screen_h, scale);
        let pad = CTX_MENU_PADDING * scale;
        let label_font = 13.0 * scale;
        let label_line = 18.0 * scale;
        let shortcut_font = 12.0 * scale;
        let shortcut_line = 17.0 * scale;
        let section_font = 11.0 * scale;
        let section_line = 15.0 * scale;
        let has_any_checked = self
            .entries
            .iter()
            .any(|e| matches!(e, ContextMenuEntry::Item(it) if it.checked));
        let check_indent = if has_any_checked { 16.0 * scale } else { 0.0 };

        let mut y = mpos[1] + pad;
        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Item(item) => {
                    out.push(TextEntry {
                        text: item.label.to_string(),
                        x: mpos[0] + pad + 10.0 * scale + check_indent,
                        y: y + (CTX_MENU_ITEM_HEIGHT * scale - label_line) * 0.5,
                        font_size: label_font,
                        line_height: label_line,
                        max_width: CTX_MENU_WIDTH * scale * 0.55,
                        color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, 255),
                        weight: 400,
                        bounds: None,
                center: false,
                    });

                    if !item.shortcut.is_empty() {
                        out.push(TextEntry {
                            text: item.shortcut.to_string(),
                            x: mpos[0] + CTX_MENU_WIDTH * scale - pad - 50.0 * scale,
                            y: y + (CTX_MENU_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                            font_size: shortcut_font,
                            line_height: shortcut_line,
                            max_width: 60.0 * scale,
                            color: crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 220),
                            weight: 400,
                            bounds: None,
                center: false,
                        });
                    }

                    y += CTX_MENU_ITEM_HEIGHT * scale;
                }
                ContextMenuEntry::Separator => {
                    y += CTX_MENU_SEPARATOR_HEIGHT * scale;
                }
                ContextMenuEntry::SectionHeader(label) => {
                    out.push(TextEntry {
                        text: label.to_string(),
                        x: mpos[0] + pad + 10.0 * scale,
                        y: y + (CTX_MENU_SECTION_HEIGHT * scale - section_line) * 0.5,
                        font_size: section_font,
                        line_height: section_line,
                        max_width: CTX_MENU_WIDTH * scale * 0.8,
                        color: crate::theme::RuntimeTheme::text_u8(theme.text_secondary, 200),
                        weight: 400,
                        bounds: None,
                center: false,
                    });
                    y += CTX_MENU_SECTION_HEIGHT * scale;
                }
                ContextMenuEntry::InlineGroup(pills) => {
                    let row_h = CTX_MENU_INLINE_HEIGHT * scale;
                    let pill_h = 22.0 * scale;
                    let pill_pad_x = 7.0 * scale;
                    let pill_gap = 2.0 * scale;
                    let pill_font = 11.0 * scale;
                    let pill_line = 15.0 * scale;
                    let pill_y = y + (row_h - pill_h) * 0.5;
                    let mut px = mpos[0] + pad + 4.0 * scale;
                    for pill in pills {
                        let pw = pill.label.len() as f32 * pill_font * 0.55 + pill_pad_x * 2.0;
                        let alpha: u8 = if pill.active { 240 } else { 160 };
                        out.push(TextEntry {
                            text: pill.label.to_string(),
                            x: px + pill_pad_x,
                            y: pill_y + (pill_h - pill_line) * 0.5,
                            font_size: pill_font,
                            line_height: pill_line,
                            max_width: pw,
                            color: crate::theme::RuntimeTheme::text_u8(theme.text_primary, alpha),
                            weight: 400,
                            bounds: None,
                center: false,
                        });
                        px += pw + pill_gap;
                    }
                    y += row_h;
                }
                ContextMenuEntry::ColorSwatchGroup(_) => {
                    y += CTX_MENU_SWATCH_HEIGHT * scale;
                }
            }
        }

        out
    }
}
