//! Offline audio renderer — mixes group members into a stereo buffer, then encodes to WAV or MP3.

use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use crate::effects::PluginGuiHandle;
use crate::entity_id::EntityId;
use crate::grid::PIXELS_PER_SECOND;
use crate::ui::export_window::{ExportFormat, ExportProgress};
use crate::App;

const EXPORT_SAMPLE_RATE: u32 = 48000;
const EFFECT_BLOCK_SIZE: usize = 512;

/// Collect non-bypassed plugin handles from an effect chain by ID.
fn collect_chain_plugins(app: &App, chain_id: EntityId) -> Vec<Arc<Mutex<Option<PluginGuiHandle>>>> {
    let mut out = Vec::new();
    if let Some(chain) = app.effect_chains.get(&chain_id) {
        for slot in &chain.slots {
            if !slot.bypass {
                out.push(slot.gui.clone());
            }
        }
    }
    out
}

/// Collect non-bypassed plugin handles for a group's effect chain.
fn collect_group_plugins(app: &App, group_id: EntityId) -> Vec<Arc<Mutex<Option<PluginGuiHandle>>>> {
    if let Some(group) = app.groups.get(&group_id) {
        if let Some(chain_id) = group.effect_chain_id {
            return collect_chain_plugins(app, chain_id);
        }
    }
    Vec::new()
}

/// Collect non-bypassed plugin handles for the master bus effect chain.
fn collect_master_plugins(app: &App) -> Vec<Arc<Mutex<Option<PluginGuiHandle>>>> {
    if let Some(chain_id) = app.master.effect_chain_id {
        return collect_chain_plugins(app, chain_id);
    }
    Vec::new()
}

/// Process stereo buffers through a chain of effect plugins (block-by-block).
fn process_effect_chain(
    left: &mut [f32],
    right: &mut [f32],
    plugins: &[Arc<Mutex<Option<PluginGuiHandle>>>],
) {
    if plugins.is_empty() {
        return;
    }
    let total = left.len();
    let mut fx_buf_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_buf_r = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_out_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_out_r = vec![0.0f32; EFFECT_BLOCK_SIZE];

    for block_start in (0..total).step_by(EFFECT_BLOCK_SIZE) {
        let block_end = (block_start + EFFECT_BLOCK_SIZE).min(total);
        let block_len = block_end - block_start;
        fx_buf_l[..block_len].copy_from_slice(&left[block_start..block_end]);
        fx_buf_r[..block_len].copy_from_slice(&right[block_start..block_end]);

        #[allow(unused_mut)]
        let (mut src_l, mut src_r, mut dst_l, mut dst_r) = (
            &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r,
        );
        for plugin_arc in plugins {
            dst_l[..block_len].copy_from_slice(&src_l[..block_len]);
            dst_r[..block_len].copy_from_slice(&src_r[..block_len]);
            if let Ok(guard) = plugin_arc.try_lock() {
                if let Some(ref gui) = *guard {
                    let inputs: [&[f32]; 2] = [&src_l[..block_len], &src_r[..block_len]];
                    let mut outputs: [&mut [f32]; 2] = [&mut dst_l[..block_len], &mut dst_r[..block_len]];
                    gui.process(&inputs, &mut outputs, block_len);
                }
            }
            std::mem::swap(src_l, dst_l);
            std::mem::swap(src_r, dst_r);
        }
        left[block_start..block_end].copy_from_slice(&src_l[..block_len]);
        right[block_start..block_end].copy_from_slice(&src_r[..block_len]);
    }
}

/// Collected audio data that can be sent to a background thread for encoding.
struct MixedAudio {
    left: Vec<f32>,
    right: Vec<f32>,
    total_frames: usize,
}

/// Mix all waveform members of a group into stereo buffers.
/// This must run on the main thread (needs access to App).
fn mix_group(app: &App, group_id: EntityId) -> Result<MixedAudio, String> {
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

        let mix_start_sec = wf_start_sec.max(start_sec);
        let mix_end_sec = wf_end_sec.min(end_sec);
        if mix_start_sec >= mix_end_sec {
            continue;
        }

        let volume = wf.volume;
        let pan = wf.pan;
        let _left_gain_unused = volume * (1.0 - pan).min(1.0) * 2.0_f32.min(1.0 + (1.0 - pan));
        let _right_gain_unused = volume * pan.min(1.0) * 2.0_f32.min(1.0 + pan);

        // Simple equal-power-ish pan
        let left_gain = volume * (std::f32::consts::FRAC_PI_2 * (1.0 - pan)).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;
        let right_gain = volume * (std::f32::consts::FRAC_PI_2 * pan).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;

        let src_rate = clip.sample_rate as f64;
        let src_len = clip.samples.len();
        let left_samples = &wf.audio.left_samples;
        let right_samples = &wf.audio.right_samples;
        let has_stereo = !left_samples.is_empty() && !right_samples.is_empty();

        let offset_sec = wf.sample_offset_px as f64 / PIXELS_PER_SECOND as f64;
        let wf_width_sec = wf.size[0] as f64 / PIXELS_PER_SECOND as f64;
        let fade_in_sec = wf.fade_in_px as f64 / PIXELS_PER_SECOND as f64;
        let fade_out_sec = wf.fade_out_px as f64 / PIXELS_PER_SECOND as f64;

        for frame in 0..total_frames {
            let t_sec = start_sec + frame as f64 / EXPORT_SAMPLE_RATE as f64;
            if t_sec < mix_start_sec || t_sec >= mix_end_sec {
                continue;
            }

            let clip_t = t_sec - wf_start_sec;
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

    // Process through group effect chain (matches playback path in audio.rs)
    let plugins = collect_group_plugins(app, group_id);
    process_effect_chain(&mut left_buf, &mut right_buf, &plugins);

    // Apply group-level volume and stereo balance (same linear law as live engine)
    let gv = group.volume;
    let gp = group.pan.clamp(0.0, 1.0);
    let l_mul = (2.0 * (1.0 - gp)).min(1.0) * gv;
    let r_mul = (2.0 * gp).min(1.0) * gv;
    if (l_mul - 1.0).abs() > f32::EPSILON || (r_mul - 1.0).abs() > f32::EPSILON {
        for i in 0..total_frames {
            left_buf[i] *= l_mul;
            right_buf[i] *= r_mul;
        }
    }

    Ok(MixedAudio { left: left_buf, right: right_buf, total_frames })
}

/// Encode mixed audio to WAV.
fn encode_wav(audio: &MixedAudio, path: &Path, progress_tx: &mpsc::Sender<ExportProgress>) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: EXPORT_SAMPLE_RATE,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV file: {}", e))?;

    let scale = (1 << 23) as f32 - 1.0;
    let report_interval = (audio.total_frames / 100).max(1);
    for i in 0..audio.total_frames {
        let l = (audio.left[i].clamp(-1.0, 1.0) * scale) as i32;
        let r = (audio.right[i].clamp(-1.0, 1.0) * scale) as i32;
        writer.write_sample(l).map_err(|e| format!("Write error: {}", e))?;
        writer.write_sample(r).map_err(|e| format!("Write error: {}", e))?;

        if i % report_interval == 0 {
            let _ = progress_tx.send(ExportProgress::Progress(i as f32 / audio.total_frames as f32));
        }
    }

    writer.finalize().map_err(|e| format!("Failed to finalize WAV: {}", e))?;
    Ok(())
}

/// Mix and start encoding on a background thread. Returns a progress receiver.
pub(crate) fn start_export(
    app: &App,
    group_id: EntityId,
    path: std::path::PathBuf,
    format: ExportFormat,
) -> Result<mpsc::Receiver<ExportProgress>, String> {
    // Mix on main thread (needs App access)
    let audio = mix_group(app, group_id)?;

    let (tx, rx) = mpsc::channel();

    // Encode on background thread
    std::thread::spawn(move || {
        let result = match format {
            ExportFormat::Wav => encode_wav(&audio, &path, &tx),
            ExportFormat::Mp3 => {
                // MP3 export not yet available — fall back to WAV
                encode_wav(&audio, &path, &tx)
            }
        };
        let _ = tx.send(ExportProgress::Done(result));
    });

    Ok(rx)
}

/// Mix ALL waveforms in the project into stereo buffers (for Main Layer export).
fn mix_all(app: &App) -> Result<MixedAudio, String> {
    // Find the time span across all waveforms
    let mut earliest = f64::MAX;
    let mut latest = f64::MIN;
    for wf in app.waveforms.values() {
        if wf.disabled { continue; }
        let wf_start = wf.position[0] as f64 / PIXELS_PER_SECOND as f64;
        let wf_end = (wf.position[0] + wf.size[0]) as f64 / PIXELS_PER_SECOND as f64;
        if wf_start < earliest { earliest = wf_start; }
        if wf_end > latest { latest = wf_end; }
    }
    if earliest >= latest || earliest == f64::MAX {
        return Err("No audio to export".to_string());
    }

    let start_sec = earliest;
    let end_sec = latest;
    let duration_sec = end_sec - start_sec;
    let total_frames = (duration_sec * EXPORT_SAMPLE_RATE as f64) as usize;
    let mut left_buf = vec![0.0f32; total_frames];
    let mut right_buf = vec![0.0f32; total_frames];

    // Collect group membership: waveform_id → group_id
    let mut wf_group: std::collections::HashMap<EntityId, EntityId> = std::collections::HashMap::new();
    for (gid, g) in &app.groups {
        for mid in &g.member_ids {
            wf_group.insert(*mid, *gid);
        }
    }

    // Per-group accumulation buffers for effect chain processing
    let mut group_bufs: std::collections::HashMap<EntityId, (Vec<f32>, Vec<f32>)> = std::collections::HashMap::new();
    for &gid in app.groups.keys() {
        let plugins = collect_group_plugins(app, gid);
        if !plugins.is_empty() {
            group_bufs.insert(gid, (vec![0.0f32; total_frames], vec![0.0f32; total_frames]));
        }
    }

    for (&wf_id, wf) in &app.waveforms {
        if wf.disabled { continue; }
        let clip = match app.audio_clips.get(&wf_id) {
            Some(c) => c,
            None => continue,
        };
        if clip.samples.is_empty() { continue; }

        let wf_start_sec = wf.position[0] as f64 / PIXELS_PER_SECOND as f64;
        let wf_end_sec = (wf.position[0] + wf.size[0]) as f64 / PIXELS_PER_SECOND as f64;
        let mix_start_sec = wf_start_sec.max(start_sec);
        let mix_end_sec = wf_end_sec.min(end_sec);
        if mix_start_sec >= mix_end_sec { continue; }

        let volume = wf.volume;
        let pan = wf.pan;
        let left_gain = volume * (std::f32::consts::FRAC_PI_2 * (1.0 - pan)).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;
        let right_gain = volume * (std::f32::consts::FRAC_PI_2 * pan).cos().max(0.0).min(1.0) * std::f32::consts::SQRT_2;

        let src_rate = clip.sample_rate as f64;
        let src_len = clip.samples.len();
        let left_samples = &wf.audio.left_samples;
        let right_samples = &wf.audio.right_samples;
        let has_stereo = !left_samples.is_empty() && !right_samples.is_empty();
        let offset_sec = wf.sample_offset_px as f64 / PIXELS_PER_SECOND as f64;
        let wf_width_sec = wf.size[0] as f64 / PIXELS_PER_SECOND as f64;
        let fade_in_sec = wf.fade_in_px as f64 / PIXELS_PER_SECOND as f64;
        let fade_out_sec = wf.fade_out_px as f64 / PIXELS_PER_SECOND as f64;

        // Determine where to accumulate: group bus (if group has FX) or final output
        let has_group_fx = wf_group.get(&wf_id)
            .map_or(false, |gid| group_bufs.contains_key(gid));

        for frame in 0..total_frames {
            let t_sec = start_sec + frame as f64 / EXPORT_SAMPLE_RATE as f64;
            if t_sec < mix_start_sec || t_sec >= mix_end_sec { continue; }

            let clip_t = t_sec - wf_start_sec;
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

            let mut fade = 1.0f32;
            if fade_in_sec > 0.0 && clip_t < fade_in_sec {
                fade *= (clip_t / fade_in_sec) as f32;
            }
            if fade_out_sec > 0.0 && clip_t > wf_width_sec - fade_out_sec {
                let fade_pos = (wf_width_sec - clip_t) / fade_out_sec;
                fade *= fade_pos as f32;
            }
            fade = fade.clamp(0.0, 1.0);

            if has_group_fx {
                // Accumulate into group bus (waveform gain only, group vol/pan applied after FX)
                let gid = wf_group[&wf_id];
                let (ref mut gl, ref mut gr) = group_bufs.get_mut(&gid).unwrap();
                gl[frame] += l_sample * left_gain * fade;
                gr[frame] += r_sample * right_gain * fade;
            } else {
                // No group FX — mix directly with group vol/pan applied inline
                let (g_l_mul, g_r_mul) = if let Some(gid) = wf_group.get(&wf_id) {
                    if let Some(g) = app.groups.get(gid) {
                        let gp = g.pan.clamp(0.0, 1.0);
                        ((2.0 * (1.0 - gp)).min(1.0) * g.volume, (2.0 * gp).min(1.0) * g.volume)
                    } else {
                        (1.0, 1.0)
                    }
                } else {
                    (1.0, 1.0)
                };
                left_buf[frame] += l_sample * left_gain * fade * g_l_mul;
                right_buf[frame] += r_sample * right_gain * fade * g_r_mul;
            }
        }
    }

    // Process each group's accumulated audio through its effect chain, then mix into output
    for (&gid, (ref mut gl, ref mut gr)) in &mut group_bufs {
        let plugins = collect_group_plugins(app, gid);
        process_effect_chain(gl, gr, &plugins);

        // Apply group volume and pan after effects
        let group = &app.groups[&gid];
        let gp = group.pan.clamp(0.0, 1.0);
        let l_mul = (2.0 * (1.0 - gp)).min(1.0) * group.volume;
        let r_mul = (2.0 * gp).min(1.0) * group.volume;
        for i in 0..total_frames {
            left_buf[i] += gl[i] * l_mul;
            right_buf[i] += gr[i] * r_mul;
        }
    }

    // Process through master bus effect chain
    let master_plugins = collect_master_plugins(app);
    process_effect_chain(&mut left_buf, &mut right_buf, &master_plugins);

    // Apply main layer volume and pan
    let mv = app.master.volume;
    let mp = app.master.pan.clamp(0.0, 1.0);
    let ml_mul = (2.0 * (1.0 - mp)).min(1.0) * mv;
    let mr_mul = (2.0 * mp).min(1.0) * mv;
    if (ml_mul - 1.0).abs() > f32::EPSILON || (mr_mul - 1.0).abs() > f32::EPSILON {
        for i in 0..total_frames {
            left_buf[i] *= ml_mul;
            right_buf[i] *= mr_mul;
        }
    }

    Ok(MixedAudio { left: left_buf, right: right_buf, total_frames })
}

/// Mix all project audio (Main Layer) and start encoding on a background thread.
pub(crate) fn start_export_main(
    app: &App,
    path: std::path::PathBuf,
    format: ExportFormat,
) -> Result<mpsc::Receiver<ExportProgress>, String> {
    let audio = mix_all(app)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match format {
            ExportFormat::Wav => encode_wav(&audio, &path, &tx),
            ExportFormat::Mp3 => encode_wav(&audio, &path, &tx),
        };
        let _ = tx.send(ExportProgress::Done(result));
    });
    Ok(rx)
}

/// Legacy synchronous export (kept for backward compatibility with tests).
pub(crate) fn export_group_wav(app: &App, group_id: EntityId, path: &Path) -> Result<(), String> {
    let audio = mix_group(app, group_id)?;
    let (tx, _rx) = mpsc::channel();
    encode_wav(&audio, path, &tx)
}
