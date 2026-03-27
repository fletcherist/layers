use crate::gpu::InstanceRaw;
use crate::theme::RuntimeTheme;

pub const SM_BTN_W: f32 = 16.0;
pub const SM_BTN_H: f32 = 14.0;
pub const SM_GAP: f32 = 2.0;
pub const SM_MARGIN: f32 = 4.0;

/// Layout info for solo/mute button pair.
pub struct SoloMuteLayout {
    pub s_pos: [f32; 2],
    pub s_size: [f32; 2],
    pub m_pos: [f32; 2],
    pub m_size: [f32; 2],
}

/// Compute positions for S and M buttons.
/// `x` is the right edge of the container, `y` is the vertical center of the row.
pub fn layout_right_aligned(right_edge: f32, cy: f32, scale: f32) -> SoloMuteLayout {
    let bw = SM_BTN_W * scale;
    let bh = SM_BTN_H * scale;
    let m_x = right_edge - bw - SM_MARGIN * scale;
    let s_x = m_x - bw - SM_GAP * scale;
    let by = cy - bh * 0.5;
    SoloMuteLayout {
        s_pos: [s_x, by],
        s_size: [bw, bh],
        m_pos: [m_x, by],
        m_size: [bw, bh],
    }
}

/// Compute positions for S and M buttons centered at a given position.
pub fn layout_centered(cx: f32, cy: f32, scale: f32) -> SoloMuteLayout {
    let bw = SM_BTN_W * scale;
    let bh = SM_BTN_H * scale;
    let total_w = bw * 2.0 + SM_GAP * scale;
    let s_x = cx - total_w * 0.5;
    let m_x = s_x + bw + SM_GAP * scale;
    let by = cy - bh * 0.5;
    SoloMuteLayout {
        s_pos: [s_x, by],
        s_size: [bw, bh],
        m_pos: [m_x, by],
        m_size: [bw, bh],
    }
}

pub fn hit_test(layout: &SoloMuteLayout, pos: [f32; 2]) -> SoloMuteHit {
    if pos[0] >= layout.s_pos[0] && pos[0] <= layout.s_pos[0] + layout.s_size[0]
        && pos[1] >= layout.s_pos[1] && pos[1] <= layout.s_pos[1] + layout.s_size[1]
    {
        return SoloMuteHit::Solo;
    }
    if pos[0] >= layout.m_pos[0] && pos[0] <= layout.m_pos[0] + layout.m_size[0]
        && pos[1] >= layout.m_pos[1] && pos[1] <= layout.m_pos[1] + layout.m_size[1]
    {
        return SoloMuteHit::Mute;
    }
    SoloMuteHit::None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoloMuteHit {
    Solo,
    Mute,
    None,
}

/// Build GPU instances for the S and M button backgrounds.
pub fn build_instances(
    layout: &SoloMuteLayout,
    is_soloed: bool,
    is_muted: bool,
    is_hovered: bool,
    visible: bool,
    theme: &RuntimeTheme,
    scale: f32,
) -> Vec<InstanceRaw> {
    if !visible {
        return Vec::new();
    }
    let mut out = Vec::new();
    let br = 2.0 * scale;

    // S button background
    let s_bg = if is_soloed {
        [0.85, 0.75, 0.15, 1.0]
    } else if is_hovered {
        crate::theme::with_alpha(theme.text_dim, 0.15)
    } else {
        [0.0; 4]
    };
    if s_bg[3] > 0.0 {
        out.push(InstanceRaw {
            position: layout.s_pos,
            size: layout.s_size,
            color: s_bg,
            border_radius: br,
        });
    }

    // M button background
    let m_bg = if is_muted {
        [0.85, 0.30, 0.20, 1.0]
    } else if is_hovered {
        crate::theme::with_alpha(theme.text_dim, 0.15)
    } else {
        [0.0; 4]
    };
    if m_bg[3] > 0.0 {
        out.push(InstanceRaw {
            position: layout.m_pos,
            size: layout.m_size,
            color: m_bg,
            border_radius: br,
        });
    }

    out
}

/// Build text entries for the S and M button labels.
pub fn build_text_entries(
    layout: &SoloMuteLayout,
    is_soloed: bool,
    is_muted: bool,
    visible: bool,
    theme: &RuntimeTheme,
    scale: f32,
) -> Vec<crate::gpu::TextEntry> {
    if !visible {
        return Vec::new();
    }
    let btn_font = 9.0 * scale;
    let btn_line = 11.0 * scale;

    let s_color = if is_soloed {
        [30u8, 30, 30, 255]
    } else {
        RuntimeTheme::text_u8(theme.text_dim, 140)
    };
    let m_color = if is_muted {
        [255u8, 255, 255, 255]
    } else {
        RuntimeTheme::text_u8(theme.text_dim, 140)
    };

    vec![
        crate::gpu::TextEntry {
            text: "S".to_string(),
            x: layout.s_pos[0],
            y: layout.s_pos[1] + (layout.s_size[1] - btn_line) * 0.5,
            font_size: btn_font,
            line_height: btn_line,
            max_width: layout.s_size[0],
            color: s_color,
            weight: 700,
            bounds: None,
            center: true,
        },
        crate::gpu::TextEntry {
            text: "M".to_string(),
            x: layout.m_pos[0],
            y: layout.m_pos[1] + (layout.m_size[1] - btn_line) * 0.5,
            font_size: btn_font,
            line_height: btn_line,
            max_width: layout.m_size[0],
            color: m_color,
            weight: 700,
            bounds: None,
            center: true,
        },
    ]
}
