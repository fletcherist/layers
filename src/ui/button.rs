use crate::gpu::TextEntry;
use crate::InstanceRaw;

/// Push a filled button background rectangle.
pub fn push_instance(
    out: &mut Vec<InstanceRaw>,
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    border_radius: f32,
) {
    out.push(InstanceRaw { position: pos, size, color, border_radius });
}

/// Push a centered button label.
pub fn push_text(
    out: &mut Vec<TextEntry>,
    pos: [f32; 2],
    size: [f32; 2],
    label: &str,
    font_size: f32,
    line_height: f32,
    color: [u8; 4],
    weight: u16,
) {
    out.push(TextEntry {
        text: label.to_string(),
        x: pos[0],
        y: pos[1] + (size[1] - line_height) * 0.5,
        font_size,
        line_height,
        color,
        weight,
        max_width: size[0],
        bounds: None,
        center: true,
    });
}
