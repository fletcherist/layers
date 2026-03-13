use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

use crate::audio::PIXELS_PER_SECOND;
use crate::{push_border, Camera, InstanceRaw};

const PEAK_BLOCK_SIZE: usize = 256;

#[derive(Clone)]
pub struct WaveformPeaks {
    pub block_size: usize,
    pub peaks: Vec<f32>,
}

impl WaveformPeaks {
    pub fn build(samples: &[f32]) -> Self {
        let block_size = PEAK_BLOCK_SIZE;
        let num_blocks = (samples.len() + block_size - 1) / block_size;
        let mut peaks = Vec::with_capacity(num_blocks);
        for i in 0..num_blocks {
            let start = i * block_size;
            let end = (start + block_size).min(samples.len());
            let peak = samples[start..end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max);
            peaks.push(peak);
        }
        WaveformPeaks { block_size, peaks }
    }

    pub fn empty() -> Self {
        WaveformPeaks {
            block_size: PEAK_BLOCK_SIZE,
            peaks: Vec::new(),
        }
    }

    pub fn from_raw(block_size: usize, peaks: Vec<f32>) -> Self {
        WaveformPeaks { block_size, peaks }
    }

    pub fn peak_in_range(&self, sample_start: usize, sample_end: usize) -> f32 {
        if self.peaks.is_empty() || sample_start >= sample_end {
            return 0.0;
        }
        let block_start = sample_start / self.block_size;
        let block_end = (sample_end + self.block_size - 1) / self.block_size;
        let block_end = block_end.min(self.peaks.len());
        if block_start >= block_end {
            return 0.0;
        }
        self.peaks[block_start..block_end]
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
    }
}

#[derive(Clone)]
pub struct AudioData {
    pub left_samples: Arc<Vec<f32>>,
    pub right_samples: Arc<Vec<f32>>,
    pub left_peaks: Arc<WaveformPeaks>,
    pub right_peaks: Arc<WaveformPeaks>,
    pub sample_rate: u32,
    pub filename: String,
}

#[derive(Clone)]
pub struct WaveformView {
    pub audio: Arc<AudioData>,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
    pub fade_in_px: f32,
    pub fade_out_px: f32,
}


#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct WaveformVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub edge: f32,
}

const FADE_CURVE_SEGMENTS: usize = 24;
pub const FADE_HANDLE_SIZE: f32 = 8.0;

const SAMPLES_PER_PX_THRESHOLD: f32 = 4.0;

fn fade_gain_at(x_in_clip: f32, clip_width: f32, fade_in_px: f32, fade_out_px: f32) -> f32 {
    let mut g = 1.0f32;
    if fade_in_px > 0.0 && x_in_clip < fade_in_px {
        g = (x_in_clip / fade_in_px).clamp(0.0, 1.0);
    }
    let x_from_end = clip_width - x_in_clip;
    if fade_out_px > 0.0 && x_from_end < fade_out_px {
        g = g.min((x_from_end / fade_out_px).clamp(0.0, 1.0));
    }
    g
}

pub fn build_waveform_instances(
    wf: &WaveformView,
    camera: &Camera,
    _world_left: f32,
    _world_right: f32,
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
    let br = (wf.border_radius).min(6.0 / camera.zoom);
    out.push(InstanceRaw {
        position: wf.position,
        size: wf.size,
        color: bg_color,
        border_radius: br,
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

    let has_fade_in = wf.fade_in_px > 0.0;
    let has_fade_out = wf.fade_out_px > 0.0;

    if has_fade_in {
        out.push(InstanceRaw {
            position: wf.position,
            size: [wf.fade_in_px, wf.size[1]],
            color: [0.0, 0.0, 0.0, 0.25],
            border_radius: 0.0,
        });
    }

    if has_fade_out {
        let fo_x = wf.position[0] + wf.size[0] - wf.fade_out_px;
        out.push(InstanceRaw {
            position: [fo_x, wf.position[1]],
            size: [wf.fade_out_px, wf.size[1]],
            color: [0.0, 0.0, 0.0, 0.25],
            border_radius: 0.0,
        });
    }

    // Fade curve lines
    let line_w = 1.5 / camera.zoom;
    let curve_color = [wf.color[0], wf.color[1], wf.color[2], 0.8];

    if has_fade_in {
        let x0 = wf.position[0];
        let y_bot = wf.position[1] + wf.size[1];
        let x1 = wf.position[0] + wf.fade_in_px;
        let y_top = wf.position[1];
        for seg in 0..FADE_CURVE_SEGMENTS {
            let t0 = seg as f32 / FADE_CURVE_SEGMENTS as f32;
            let t1 = (seg + 1) as f32 / FADE_CURVE_SEGMENTS as f32;
            let sx = x0 + (x1 - x0) * t0;
            let sy = y_bot + (y_top - y_bot) * t0;
            let ex = x0 + (x1 - x0) * t1;
            let ey = y_bot + (y_top - y_bot) * t1;
            let seg_len = ((ex - sx).powi(2) + (ey - sy).powi(2)).sqrt();
            out.push(InstanceRaw {
                position: [sx, sy.min(ey)],
                size: [seg_len.max(line_w), (ey - sy).abs().max(line_w)],
                color: curve_color,
                border_radius: 0.0,
            });
        }
    }

    if has_fade_out {
        let x0 = wf.position[0] + wf.size[0] - wf.fade_out_px;
        let y_top = wf.position[1];
        let x1 = wf.position[0] + wf.size[0];
        let y_bot = wf.position[1] + wf.size[1];
        for seg in 0..FADE_CURVE_SEGMENTS {
            let t0 = seg as f32 / FADE_CURVE_SEGMENTS as f32;
            let t1 = (seg + 1) as f32 / FADE_CURVE_SEGMENTS as f32;
            let sx = x0 + (x1 - x0) * t0;
            let sy = y_top + (y_bot - y_top) * t0;
            let ex = x0 + (x1 - x0) * t1;
            let ey = y_top + (y_bot - y_top) * t1;
            let seg_len = ((ex - sx).powi(2) + (ey - sy).powi(2)).sqrt();
            out.push(InstanceRaw {
                position: [sx, sy.min(ey)],
                size: [seg_len.max(line_w), (ey - sy).abs().max(line_w)],
                color: curve_color,
                border_radius: 0.0,
            });
        }
    }

    if is_hovered || is_selected {
        let handle_sz = FADE_HANDLE_SIZE / camera.zoom;
        let handle_color = [1.0, 1.0, 1.0, 0.9];
        let handle_br = 2.0 / camera.zoom;

        let fi_hx = wf.position[0] + wf.fade_in_px - handle_sz * 0.5;
        let fi_hy = wf.position[1] - handle_sz * 0.5;
        out.push(InstanceRaw {
            position: [fi_hx, fi_hy],
            size: [handle_sz, handle_sz],
            color: handle_color,
            border_radius: handle_br,
        });

        let fo_hx = wf.position[0] + wf.size[0] - wf.fade_out_px - handle_sz * 0.5;
        let fo_hy = wf.position[1] - handle_sz * 0.5;
        out.push(InstanceRaw {
            position: [fo_hx, fo_hy],
            size: [handle_sz, handle_sz],
            color: handle_color,
            border_radius: handle_br,
        });
    }

    out
}

fn channel_triangles(
    samples: &[f32],
    peaks: &WaveformPeaks,
    sample_rate: u32,
    wf_pos: [f32; 2],
    wf_size: [f32; 2],
    center_y: f32,
    half_h: f32,
    direction: f32,
    color: [f32; 4],
    camera: &Camera,
    world_left: f32,
    world_right: f32,
    fade_in_px: f32,
    fade_out_px: f32,
) -> Vec<WaveformVertex> {
    let mut verts = Vec::new();
    if samples.is_empty() || wf_size[0] <= 0.0 {
        return verts;
    }

    let samples_per_px = sample_rate as f32 / (PIXELS_PER_SECOND * camera.zoom);
    let desired_screen_px = 2.0;
    let world_step = desired_screen_px / camera.zoom;

    let vis_left = world_left.max(wf_pos[0]);
    let vis_right = world_right.min(wf_pos[0] + wf_size[0]);
    if vis_left >= vis_right {
        return verts;
    }

    let num_columns = ((vis_right - vis_left) / world_step).ceil() as usize + 1;
    let num_columns = num_columns.min(8192);

    let feather = 0.8 / camera.zoom;

    if samples_per_px > SAMPLES_PER_PX_THRESHOLD {
        // Zoomed out: compute min/max peak per column
        let mut prev_x = vis_left;
        let mut prev_amp = 0.0f32;
        let mut first = true;

        for col in 0..=num_columns {
            let wx = vis_left + col as f32 * world_step;
            let wx = wx.min(vis_right);

            let t = ((wx - wf_pos[0]) / wf_size[0]).clamp(0.0, 1.0);
            let sample_center = (t * samples.len() as f32) as usize;
            let half_window = (samples_per_px * world_step * camera.zoom * 0.5) as usize;
            let half_window = half_window.max(1);
            let s_start = sample_center.saturating_sub(half_window).min(samples.len());
            let s_end = (sample_center + half_window).min(samples.len());

            let peak = peaks.peak_in_range(s_start, s_end);

            let x_in_clip = wx - wf_pos[0];
            let fg = fade_gain_at(x_in_clip, wf_size[0], fade_in_px, fade_out_px);
            let amp = peak * fg;

            if first {
                prev_x = wx;
                prev_amp = amp;
                first = false;
                continue;
            }

            push_filled_quad(
                &mut verts, prev_x, prev_amp, wx, amp, center_y, half_h, direction, feather, color,
            );

            prev_x = wx;
            prev_amp = amp;
        }
    } else {
        // Zoomed in: draw individual samples as connected waveform
        let world_per_sample = 1.0 / (sample_rate as f32 / PIXELS_PER_SECOND);

        let vis_start_sample = (((vis_left - wf_pos[0]) / wf_size[0]) * samples.len() as f32)
            .floor()
            .max(0.0) as usize;
        let vis_end_sample = (((vis_right - wf_pos[0]) / wf_size[0]) * samples.len() as f32)
            .ceil()
            .max(0.0) as usize;
        let vis_start_sample = vis_start_sample.min(samples.len());
        let vis_end_sample = vis_end_sample.min(samples.len());

        if vis_end_sample <= vis_start_sample {
            return verts;
        }

        let mut prev_x = wf_pos[0] + vis_start_sample as f32 * world_per_sample;
        let x_in_clip = prev_x - wf_pos[0];
        let fg = fade_gain_at(x_in_clip, wf_size[0], fade_in_px, fade_out_px);
        let mut prev_val = samples[vis_start_sample] * fg;

        for si in (vis_start_sample + 1)..vis_end_sample {
            let wx = wf_pos[0] + si as f32 * world_per_sample;
            let x_in_clip = wx - wf_pos[0];
            let fg = fade_gain_at(x_in_clip, wf_size[0], fade_in_px, fade_out_px);
            let val = samples[si] * fg;

            push_wave_quad(
                &mut verts, prev_x, prev_val, wx, val, center_y, half_h, direction, feather, color,
            );

            prev_x = wx;
            prev_val = val;
        }
    }

    verts
}

fn push_filled_quad(
    verts: &mut Vec<WaveformVertex>,
    x0: f32,
    amp0: f32,
    x1: f32,
    amp1: f32,
    center_y: f32,
    half_h: f32,
    direction: f32,
    feather: f32,
    color: [f32; 4],
) {
    let y0_inner = center_y;
    let y0_outer = center_y + direction * amp0.clamp(0.0, 1.0) * half_h;
    let y1_inner = center_y;
    let y1_outer = center_y + direction * amp1.clamp(0.0, 1.0) * half_h;

    let y0_feather = y0_outer + direction * feather;
    let y1_feather = y1_outer + direction * feather;

    // Main filled quad (2 triangles)
    let v_inner0 = WaveformVertex {
        position: [x0, y0_inner],
        color,
        edge: 0.0,
    };
    let v_outer0 = WaveformVertex {
        position: [x0, y0_outer],
        color,
        edge: 0.0,
    };
    let v_inner1 = WaveformVertex {
        position: [x1, y1_inner],
        color,
        edge: 0.0,
    };
    let v_outer1 = WaveformVertex {
        position: [x1, y1_outer],
        color,
        edge: 0.0,
    };

    verts.push(v_inner0);
    verts.push(v_outer0);
    verts.push(v_inner1);

    verts.push(v_inner1);
    verts.push(v_outer0);
    verts.push(v_outer1);

    // AA feather edge (2 triangles)
    let feather_color = [color[0], color[1], color[2], 0.0];
    let v_feather0 = WaveformVertex {
        position: [x0, y0_feather],
        color: feather_color,
        edge: 1.0,
    };
    let v_feather1 = WaveformVertex {
        position: [x1, y1_feather],
        color: feather_color,
        edge: 1.0,
    };

    verts.push(v_outer0);
    verts.push(v_feather0);
    verts.push(v_outer1);

    verts.push(v_outer1);
    verts.push(v_feather0);
    verts.push(v_feather1);
}

fn push_wave_quad(
    verts: &mut Vec<WaveformVertex>,
    x0: f32,
    val0: f32,
    x1: f32,
    val1: f32,
    center_y: f32,
    half_h: f32,
    direction: f32,
    feather: f32,
    color: [f32; 4],
) {
    // val can be negative for true waveform, direction determines which half
    let y0_wave = center_y + direction * val0 * half_h;
    let y1_wave = center_y + direction * val1 * half_h;
    let y0_center = center_y;
    let y1_center = center_y;

    let y0_top = y0_wave.min(y0_center);
    let y0_bot = y0_wave.max(y0_center);
    let y1_top = y1_wave.min(y1_center);
    let y1_bot = y1_wave.max(y1_center);

    let v_top0 = WaveformVertex {
        position: [x0, y0_top],
        color,
        edge: 0.0,
    };
    let v_bot0 = WaveformVertex {
        position: [x0, y0_bot],
        color,
        edge: 0.0,
    };
    let v_top1 = WaveformVertex {
        position: [x1, y1_top],
        color,
        edge: 0.0,
    };
    let v_bot1 = WaveformVertex {
        position: [x1, y1_bot],
        color,
        edge: 0.0,
    };

    verts.push(v_top0);
    verts.push(v_bot0);
    verts.push(v_top1);

    verts.push(v_top1);
    verts.push(v_bot0);
    verts.push(v_bot1);

    // Feather on the outer edge (the wave side, not center)
    let feather_color = [color[0], color[1], color[2], 0.0];
    let y0_f_top = y0_top - feather;
    let y0_f_bot = y0_bot + feather;
    let y1_f_top = y1_top - feather;
    let y1_f_bot = y1_bot + feather;

    // Top feather
    let vf_top0 = WaveformVertex {
        position: [x0, y0_f_top],
        color: feather_color,
        edge: 1.0,
    };
    let vf_top1 = WaveformVertex {
        position: [x1, y1_f_top],
        color: feather_color,
        edge: 1.0,
    };
    verts.push(vf_top0);
    verts.push(v_top0);
    verts.push(vf_top1);
    verts.push(vf_top1);
    verts.push(v_top0);
    verts.push(v_top1);

    // Bottom feather
    let vf_bot0 = WaveformVertex {
        position: [x0, y0_f_bot],
        color: feather_color,
        edge: 1.0,
    };
    let vf_bot1 = WaveformVertex {
        position: [x1, y1_f_bot],
        color: feather_color,
        edge: 1.0,
    };
    verts.push(v_bot0);
    verts.push(vf_bot0);
    verts.push(v_bot1);
    verts.push(v_bot1);
    verts.push(vf_bot0);
    verts.push(vf_bot1);
}

pub fn build_waveform_triangles(
    wf: &WaveformView,
    camera: &Camera,
    world_left: f32,
    world_right: f32,
    is_hovered: bool,
    is_selected: bool,
) -> Vec<WaveformVertex> {
    if wf.audio.left_samples.is_empty() && wf.audio.right_samples.is_empty() {
        return Vec::new();
    }

    let mut peak_color = wf.color;
    if is_hovered || is_selected {
        peak_color[0] = (peak_color[0] + 0.1).min(1.0);
        peak_color[1] = (peak_color[1] + 0.1).min(1.0);
        peak_color[2] = (peak_color[2] + 0.1).min(1.0);
    }

    let padding = wf.size[1] * 0.06;
    let center_y = wf.position[1] + wf.size[1] * 0.5;
    let half_h = (wf.size[1] * 0.5) - padding;

    let mut all_verts = Vec::new();

    all_verts.extend(channel_triangles(
        &wf.audio.left_samples,
        &wf.audio.left_peaks,
        wf.audio.sample_rate,
        wf.position,
        wf.size,
        center_y,
        half_h,
        -1.0,
        peak_color,
        camera,
        world_left,
        world_right,
        wf.fade_in_px,
        wf.fade_out_px,
    ));

    all_verts.extend(channel_triangles(
        &wf.audio.right_samples,
        &wf.audio.right_peaks,
        wf.audio.sample_rate,
        wf.position,
        wf.size,
        center_y,
        half_h,
        1.0,
        peak_color,
        camera,
        world_left,
        world_right,
        wf.fade_in_px,
        wf.fade_out_px,
    ));

    all_verts
}
