use crate::palette::CommandAction;
use crate::InstanceRaw;

pub const CTX_MENU_WIDTH: f32 = 200.0;
pub const CTX_MENU_ITEM_HEIGHT: f32 = 32.0;
pub const CTX_MENU_SEPARATOR_HEIGHT: f32 = 9.0;
pub const CTX_MENU_PADDING: f32 = 4.0;
pub const CTX_MENU_BORDER_RADIUS: f32 = 8.0;

pub struct ContextMenuItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: CommandAction,
}

pub enum ContextMenuEntry {
    Item(ContextMenuItem),
    Separator,
}

pub struct ContextMenu {
    pub position: [f32; 2],
    pub entries: Vec<ContextMenuEntry>,
    pub hovered_index: Option<usize>,
}

impl ContextMenu {
    pub fn new(pos: [f32; 2]) -> Self {
        Self {
            position: pos,
            entries: vec![
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Copy",
                    shortcut: "⌘C",
                    action: CommandAction::Copy,
                }),
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Paste",
                    shortcut: "⌘V",
                    action: CommandAction::Paste,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Duplicate",
                    shortcut: "⌘D",
                    action: CommandAction::Duplicate,
                }),
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Delete",
                    shortcut: "⌫",
                    action: CommandAction::Delete,
                }),
                ContextMenuEntry::Separator,
                ContextMenuEntry::Item(ContextMenuItem {
                    label: "Select All",
                    shortcut: "⌘A",
                    action: CommandAction::SelectAll,
                }),
            ],
            hovered_index: None,
        }
    }

    pub fn content_height(&self, scale: f32) -> f32 {
        let mut h = 0.0;
        for entry in &self.entries {
            h += match entry {
                ContextMenuEntry::Item(_) => CTX_MENU_ITEM_HEIGHT * scale,
                ContextMenuEntry::Separator => CTX_MENU_SEPARATOR_HEIGHT * scale,
            };
        }
        h
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

    pub fn item_at(
        &self,
        pos: [f32; 2],
        screen_w: f32,
        screen_h: f32,
        scale: f32,
    ) -> Option<usize> {
        let (rp, _) = self.menu_rect(screen_w, screen_h, scale);
        let mut y = rp[1] + CTX_MENU_PADDING * scale;
        let mut item_i = 0;
        for entry in &self.entries {
            let rh = match entry {
                ContextMenuEntry::Item(_) => CTX_MENU_ITEM_HEIGHT * scale,
                ContextMenuEntry::Separator => CTX_MENU_SEPARATOR_HEIGHT * scale,
            };
            if pos[1] >= y && pos[1] < y + rh {
                return match entry {
                    ContextMenuEntry::Item(_) => Some(item_i),
                    ContextMenuEntry::Separator => None,
                };
            }
            if matches!(entry, ContextMenuEntry::Item(_)) {
                item_i += 1;
            }
            y += rh;
        }
        None
    }

    pub fn action_at(&self, index: usize) -> Option<CommandAction> {
        let mut item_i = 0;
        for entry in &self.entries {
            if let ContextMenuEntry::Item(item) = entry {
                if item_i == index {
                    return Some(item.action);
                }
                item_i += 1;
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

        // Shadow
        let so = 6.0 * scale;
        out.push(InstanceRaw {
            position: [pos[0] + so, pos[1] + so],
            size: [size[0] + 2.0 * scale, size[1] + 2.0 * scale],
            color: [0.0, 0.0, 0.0, 0.40],
            border_radius: CTX_MENU_BORDER_RADIUS * scale,
        });

        // Background
        out.push(InstanceRaw {
            position: pos,
            size,
            color: [0.16, 0.16, 0.19, 0.98],
            border_radius: CTX_MENU_BORDER_RADIUS * scale,
        });

        // Hover highlight + separators
        let mut y = pos[1] + pad;
        let mut item_i = 0;
        for entry in &self.entries {
            match entry {
                ContextMenuEntry::Item(_) => {
                    if Some(item_i) == self.hovered_index {
                        out.push(InstanceRaw {
                            position: [pos[0] + pad, y],
                            size: [size[0] - pad * 2.0, CTX_MENU_ITEM_HEIGHT * scale],
                            color: [0.26, 0.26, 0.32, 0.8],
                            border_radius: 5.0 * scale,
                        });
                    }
                    item_i += 1;
                    y += CTX_MENU_ITEM_HEIGHT * scale;
                }
                ContextMenuEntry::Separator => {
                    let sep_y = y + CTX_MENU_SEPARATOR_HEIGHT * scale * 0.5;
                    out.push(InstanceRaw {
                        position: [pos[0] + pad + 4.0 * scale, sep_y],
                        size: [size[0] - (pad + 4.0 * scale) * 2.0, 1.0 * scale],
                        color: [1.0, 1.0, 1.0, 0.08],
                        border_radius: 0.0,
                    });
                    y += CTX_MENU_SEPARATOR_HEIGHT * scale;
                }
            }
        }

        out
    }
}
