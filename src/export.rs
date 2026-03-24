//! Offline audio renderer — mixes group members into a WAV file.

use std::path::Path;
use crate::entity_id::EntityId;
use crate::grid::PIXELS_PER_SECOND;
use crate::App;

const EXPORT_SAMPLE_RATE: u32 = 48000;

/// Export a group's audio content to a WAV file.
/// Mixes all waveform members within the group's time range.
pub(crate) fn export_group_wav(app: &App, group_id: EntityId, path: &Path) -> Result<(), String> {
    let group = app.groups.get(&group_id)
        .ok_or_else(|| "Group not found".to_string())?;

    let start_sec = group.position[0] as f64 / PIXELS_PER_SECOND as f64;
    let end_sec = (group.position[0] + group.size[0]) as f64 / PIXELS_PER_SECOND as f64;
    let duration_sec = end_sec - start_sec;
    if duration_sec <= 0.0 {
        return Err("Group has zero duration".to_string());
    }

    let total_frames = (duration_sec * EXPORT_SAMPLE_RATE as f64) as usize;
    let mut left_buf = vec![0.0f32; total_frames];
    let mut right_buf = vec![0.0f32; total_frames];

    // Mix each waveform member
    for mid in &group.member_ids {
        let wf = match app.waveforms.get(mid) {
            Some(w) if !w.disabled => w,
            _ => continue,
        };
        let clip = match app.audio_clips.get(mid) {
            Some(c) => c,
            None => continue,
        };
        if clip.samples.is_empty() {
            continue;
        }

        let wf_start_sec = wf.position[0] as f64 / PIXELS_PER_SECOND as f64;
        let wf_end_sec = (wf.position[0] + wf.size[0]) as f64 / PIXELS_PER_SECOND as f64;

        // Clip to export range
        let mix_start_sec = wf_start_sec.max(start_sec);
        let mix_end_sec = wf_end_sec.min(end_sec);
        if mix_start_sec >= mix_end_sec {
            continue;
        }

        let volume = wf.volume;
        let pan = wf.pan; // 0.0 = left, 0.5 = center, 1.0 = right
        let left_gain = volume * (1.0 - pan).min(1.0) * 2.0_f32.min(1.0 + (1.0 - pan));
        let right_gain = volume * pan.min(1.0) * 2.0_f32.min(1.0 + pan);

        // Simple equal-power-ish pan
        let left_gain = volume * (std::f32::consts::FRAC_PI_2 * (1.0 - pan)).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;
        let right_gain = volume * (std::f32::consts::FRAC_PI_2 * pan).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;

        let src_rate = clip.sample_rate as f64;
        let src_len = clip.samples.len();
        let left_samples = &wf.audio.left_samples;
        let right_samples = &wf.audio.right_samples;
        let has_stereo = !left_samples.is_empty() && !right_samples.is_empty();

        // Sample offset (for trimmed clips)
        let offset_sec = wf.sample_offset_px as f64 / PIXELS_PER_SECOND as f64;

        let wf_width_sec = wf.size[0] as f64 / PIXELS_PER_SECOND as f64;
        let fade_in_sec = wf.fade_in_px as f64 / PIXELS_PER_SECOND as f64;
        let fade_out_sec = wf.fade_out_px as f64 / PIXELS_PER_SECOND as f64;

        for frame in 0..total_frames {
            let t_sec = start_sec + frame as f64 / EXPORT_SAMPLE_RATE as f64;
            if t_sec < mix_start_sec || t_sec >= mix_end_sec {
                continue;
            }

            // Position within the waveform clip (0 = clip start)
            let clip_t = t_sec - wf_start_sec;

            // Source sample position (accounting for offset)
            let src_t = offset_sec + clip_t;
            let src_idx = (src_t * src_rate) as usize;

            let (l_sample, r_sample) = if has_stereo && src_idx < left_samples.len() && src_idx < right_samples.len() {
                (left_samples[src_idx], right_samples[src_idx])
            } else if src_idx < src_len {
                let mono = clip.samples[src_idx];
                (mono, mono)
            } else {
                continue;
            };

            // Apply fade in/out
            let mut fade = 1.0f32;
            if fade_in_sec > 0.0 && clip_t < fade_in_sec {
                fade *= (clip_t / fade_in_sec) as f32;
            }
            if fade_out_sec > 0.0 && clip_t > wf_width_sec - fade_out_sec {
                let fade_pos = (wf_width_sec - clip_t) / fade_out_sec;
                fade *= fade_pos as f32;
            }
            fade = fade.clamp(0.0, 1.0);

            left_buf[frame] += l_sample * left_gain * fade;
            right_buf[frame] += r_sample * right_gain * fade;
        }
    }

    // Clamp and write WAV
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: EXPORT_SAMPLE_RATE,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV file: {}", e))?;

    let scale = (1 << 23) as f32 - 1.0; // 24-bit range
    for i in 0..total_frames {
        let l = (left_buf[i].clamp(-1.0, 1.0) * scale) as i32;
        let r = (right_buf[i].clamp(-1.0, 1.0) * scale) as i32;
        writer.write_sample(l).map_err(|e| format!("Write error: {}", e))?;
        writer.write_sample(r).map_err(|e| format!("Write error: {}", e))?;
    }

    writer.finalize().map_err(|e| format!("Failed to finalize WAV: {}", e))?;
    Ok(())
}
