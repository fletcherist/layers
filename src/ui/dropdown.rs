//! Reusable dropdown component: rendering, hit-testing, and text helpers.

use crate::gpu::TextEntry;
use crate::theme::{self, RuntimeTheme};
use crate::InstanceRaw;

pub(crate) const ITEM_HEIGHT: f32 = 26.0;

/// Result of `handle_click`.
pub(crate) enum ClickResult {
    /// A popup item was selected (index).
    Selected(usize),
    /// The button was toggled (new open state).
    Toggled(bool),
    /// Click was outside both button and popup.
    None,
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render a dropdown button (border + background + arrow indicator).
pub(crate) fn render_button(
    out: &mut Vec<InstanceRaw>,
    dp: [f32; 2],
    ds: [f32; 2],
    scale: f32,
    t: &RuntimeTheme,
) {
    let dd_br = 4.0 * scale;
    out.push(InstanceRaw {
        position: [dp[0] - 1.0, dp[1] - 1.0],
        size: [ds[0] + 2.0, ds[1] + 2.0],
        color: t.bg_window_header,
        border_radius: dd_br + 1.0,
    });
    out.push(InstanceRaw {
        position: dp,
        size: ds,
        color: t.bg_input,
        border_radius: dd_br,
    });
    let arrow_size = 10.0 * scale;
    let arrow_x = dp[0] + ds[0] - 18.0 * scale;
    let arrow_y = dp[1] + (ds[1] - arrow_size) * 0.5;
    out.push(InstanceRaw {
        position: [arrow_x, arrow_y],
        size: [arrow_size, arrow_size],
        color: theme::with_alpha(t.text_primary, 0.3),
        border_radius: arrow_size * 0.5,
    });
}

/// Render a dropdown popup (shadow + border + background + item highlights).
pub(crate) fn render_popup(
    out: &mut Vec<InstanceRaw>,
    dp: [f32; 2],
    ds: [f32; 2],
    item_count: usize,
    selected_idx: usize,
    hovered_item: Option<usize>,
    scale: f32,
    t: &RuntimeTheme,
) {
    let item_h = ITEM_HEIGHT * scale;
    let popup_h = item_count as f32 * item_h;
    let popup_y = dp[1] + ds[1] + 2.0 * scale;
    let popup_br = 6.0 * scale;

    // Shadow
    out.push(InstanceRaw {
        position: [dp[0] + 4.0 * scale, popup_y + 4.0 * scale],
        size: [ds[0], popup_h],
        color: t.shadow_strong,
        border_radius: popup_br,
    });
    // Border
    out.push(InstanceRaw {
        position: [dp[0] - 1.0, popup_y - 1.0],
        size: [ds[0] + 2.0, popup_h + 2.0],
        color: t.bg_window_header,
        border_radius: popup_br + 1.0,
    });
    // Background
    out.push(InstanceRaw {
        position: [dp[0], popup_y],
        size: [ds[0], popup_h],
        color: t.bg_menu,
        border_radius: popup_br,
    });
    // Item highlights
    for j in 0..item_count {
        let iy = popup_y + j as f32 * item_h;
        if j == selected_idx {
            out.push(InstanceRaw {
                position: [dp[0] + 4.0 * scale, iy + 2.0 * scale],
                size: [ds[0] - 8.0 * scale, item_h - 4.0 * scale],
                color: t.option_highlight,
                border_radius: 4.0 * scale,
            });
        } else if hovered_item == Some(j) {
            out.push(InstanceRaw {
                position: [dp[0] + 4.0 * scale, iy + 2.0 * scale],
                size: [ds[0] - 8.0 * scale, item_h - 4.0 * scale],
                color: t.item_hover,
                border_radius: 4.0 * scale,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

/// Returns true if `mouse` is inside the dropdown button rect.
pub(crate) fn hit_test_button(mouse: [f32; 2], dp: [f32; 2], ds: [f32; 2]) -> bool {
    mouse[0] >= dp[0]
        && mouse[0] <= dp[0] + ds[0]
        && mouse[1] >= dp[1]
        && mouse[1] <= dp[1] + ds[1]
}

/// Returns which popup item is under `mouse`, or None.
pub(crate) fn hit_test_popup_item(
    mouse: [f32; 2],
    dp: [f32; 2],
    ds: [f32; 2],
    item_count: usize,
    scale: f32,
) -> Option<usize> {
    if item_count == 0 {
        return None;
    }
    let item_h = ITEM_HEIGHT * scale;
    let popup_y = dp[1] + ds[1] + 2.0 * scale;
    let popup_h = item_count as f32 * item_h;

    if mouse[0] >= dp[0]
        && mouse[0] <= dp[0] + ds[0]
        && mouse[1] >= popup_y
        && mouse[1] <= popup_y + popup_h
    {
        let rel = mouse[1] - popup_y;
        let idx = (rel / item_h) as usize;
        if idx < item_count {
            return Some(idx);
        }
    }
    None
}

/// Handle a click on a dropdown (button or popup).
/// `is_open` is the current open state of this dropdown.
/// Returns a `ClickResult` describing what happened.
pub(crate) fn handle_click(
    mouse: [f32; 2],
    dp: [f32; 2],
    ds: [f32; 2],
    item_count: usize,
    scale: f32,
    is_open: bool,
) -> ClickResult {
    // Check popup item first (only if open)
    if is_open {
        if let Some(idx) = hit_test_popup_item(mouse, dp, ds, item_count, scale) {
            return ClickResult::Selected(idx);
        }
    }
    // Check button
    if hit_test_button(mouse, dp, ds) {
        return ClickResult::Toggled(!is_open);
    }
    ClickResult::None
}

/// Returns the popup rect `([pos], [size])` for GPU text clipping.
pub(crate) fn popup_rect(
    dp: [f32; 2],
    ds: [f32; 2],
    item_count: usize,
    scale: f32,
) -> ([f32; 2], [f32; 2]) {
    let item_h = ITEM_HEIGHT * scale;
    let popup_h = item_count as f32 * item_h;
    let popup_y = dp[1] + ds[1] + 2.0 * scale;
    ([dp[0], popup_y], [ds[0], popup_h])
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Build text entries for popup items.
pub(crate) fn build_popup_text(
    out: &mut Vec<TextEntry>,
    labels: &[&str],
    selected_idx: usize,
    dp: [f32; 2],
    ds: [f32; 2],
    scale: f32,
    t: &RuntimeTheme,
) {
    let dd_font = 12.0 * scale;
    let dd_line = 16.0 * scale;
    let item_h = ITEM_HEIGHT * scale;
    let popup_y = dp[1] + ds[1] + 2.0 * scale;

    for (j, label) in labels.iter().enumerate() {
        let iy = popup_y + j as f32 * item_h;
        let is_selected = j == selected_idx;
        out.push(TextEntry {
            text: label.to_string(),
            x: dp[0] + 12.0 * scale,
            y: iy + (item_h - dd_line) * 0.5,
            font_size: dd_font,
            line_height: dd_line,
            color: RuntimeTheme::text_u8(
                if is_selected { t.text_primary } else { t.text_secondary },
                255,
            ),
            weight: if is_selected { 600 } else { 400 },
            max_width: 300.0 * scale,
            bounds: Some([0.0, 0.0, 0.0, 0.0]),
            center: false,
        });
    }
}

/// Render the current dropdown value text inside the button.
pub(crate) fn render_value_text(
    out: &mut Vec<TextEntry>,
    text: &str,
    dp: [f32; 2],
    ds: [f32; 2],
    scale: f32,
    t: &RuntimeTheme,
) {
    let dd_font = 12.0 * scale;
    let dd_line = 16.0 * scale;
    out.push(TextEntry {
        text: text.to_string(),
        x: dp[0] + 10.0 * scale,
        y: dp[1] + (ds[1] - dd_line) * 0.5,
        font_size: dd_font,
        line_height: dd_line,
        color: RuntimeTheme::text_u8(t.text_primary, 255),
        weight: 400,
        max_width: 300.0 * scale,
        bounds: None,
        center: false,
    });
}
