use surrealdb::types::SurrealValue;

use crate::{push_border, Camera, InstanceRaw};

#[derive(Clone, PartialEq, SurrealValue)]
pub struct WaveformObject {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
    pub peaks: Vec<f32>,
    pub filename: String,
}

const DESIRED_BAR_SCREEN_PX: f32 = 3.5;

pub fn build_waveform_instances(
    wf: &WaveformObject,
    camera: &Camera,
    world_left: f32,
    world_right: f32,
    is_hovered: bool,
    is_selected: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let bg_color = [
        wf.color[0] * 0.15,
        wf.color[1] * 0.15,
        wf.color[2] * 0.15,
        0.92,
    ];
    out.push(InstanceRaw {
        position: wf.position,
        size: wf.size,
        color: bg_color,
        border_radius: wf.border_radius,
    });

    if is_hovered && !is_selected {
        let bw = 2.0 / camera.zoom;
        let bc = [wf.color[0], wf.color[1], wf.color[2], 0.6];
        push_border(&mut out, wf.position, wf.size, bw, bc);
    }

    let center_line_h = 1.0 / camera.zoom;
    out.push(InstanceRaw {
        position: [
            wf.position[0],
            wf.position[1] + wf.size[1] * 0.5 - center_line_h * 0.5,
        ],
        size: [wf.size[0], center_line_h],
        color: [1.0, 1.0, 1.0, 0.08],
        border_radius: 0.0,
    });

    if !wf.peaks.is_empty() {
        let num_peaks = wf.peaks.len();
        let col_width = wf.size[0] / num_peaks as f32;

        let screen_wf_width = wf.size[0] * camera.zoom;
        let target_bars = (screen_wf_width / DESIRED_BAR_SCREEN_PX).round().max(1.0) as usize;
        let step = (num_peaks / target_bars).max(1);

        let effective_col = col_width * step as f32;

        let start_raw =
            ((world_left - wf.position[0]) / effective_col).floor().max(0.0) as usize * step;
        let end_raw = ((world_right - wf.position[0]) / effective_col)
            .ceil()
            .max(0.0) as usize
            * step;
        let start = start_raw.min(num_peaks);
        let end = end_raw.min(num_peaks);

        let mut peak_color = wf.color;
        if is_hovered || is_selected {
            peak_color[0] = (peak_color[0] + 0.1).min(1.0);
            peak_color[1] = (peak_color[1] + 0.1).min(1.0);
            peak_color[2] = (peak_color[2] + 0.1).min(1.0);
        }

        let padding = wf.size[1] * 0.08;
        let drawable_h = wf.size[1] - padding * 2.0;

        let bar_w = effective_col * 0.82;

        let mut j = start;
        while j < end {
            let chunk_end = (j + step).min(num_peaks);
            let peak = if step > 1 {
                wf.peaks[j..chunk_end]
                    .iter()
                    .fold(0.0f32, |a, &b| a.max(b))
            } else {
                wf.peaks[j]
            };

            let bar_h = (peak * drawable_h).max(1.0 / camera.zoom);
            let bar_y = wf.position[1] + padding + (drawable_h - bar_h) * 0.5;
            let bar_x = wf.position[0] + j as f32 * col_width;

            out.push(InstanceRaw {
                position: [bar_x, bar_y],
                size: [bar_w, bar_h],
                color: peak_color,
                border_radius: if bar_w > 2.0 / camera.zoom {
                    1.0 / camera.zoom
                } else {
                    0.0
                },
            });

            j += step;
        }
    }

    out
}
