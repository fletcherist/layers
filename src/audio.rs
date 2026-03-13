use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rack::traits::PluginInstance;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

pub const PIXELS_PER_SECOND: f32 = 120.0;

const EFFECT_BLOCK_SIZE: usize = 512;

#[derive(Clone)]
pub struct AudioClipData {
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub duration_secs: f32,
}

pub struct LoadedAudio {
    pub samples: Arc<Vec<f32>>,
    pub left_samples: Arc<Vec<f32>>,
    pub right_samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub duration_secs: f32,
    pub width: f32,
}

struct PlaybackClip {
    buffer: Arc<Vec<f32>>,
    source_sample_rate: u32,
    start_time_secs: f64,
    duration_secs: f64,
    position_y: f32,
    height: f32,
    fade_in_secs: f64,
    fade_out_secs: f64,
    volume: f32,
}

pub struct AudioEffectRegion {
    pub x_start_px: f32,
    pub x_end_px: f32,
    pub y_start: f32,
    pub y_end: f32,
    pub plugins: Vec<Arc<Mutex<Option<Box<dyn PluginInstance>>>>>,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    device_name: String,
    playing: Arc<AtomicBool>,
    position_bits: Arc<AtomicU64>,
    clips: Arc<Mutex<Vec<PlaybackClip>>>,
    effect_regions: Arc<Mutex<Vec<AudioEffectRegion>>>,
    master_volume: Arc<AtomicU64>,
    rms_peak: Arc<AtomicU64>,
}

fn store_f64(atomic: &AtomicU64, value: f64) {
    atomic.store(value.to_bits(), Ordering::Relaxed);
}

fn load_f64(atomic: &AtomicU64) -> f64 {
    f64::from_bits(atomic.load(Ordering::Relaxed))
}

#[inline]
fn clip_fade_gain(clip_t: f64, duration: f64, fade_in: f64, fade_out: f64) -> f32 {
    let mut g = 1.0f32;
    if fade_in > 0.0 && clip_t < fade_in {
        g = (clip_t / fade_in) as f32;
    }
    let from_end = duration - clip_t;
    if fade_out > 0.0 && from_end < fade_out {
        g = g.min((from_end / fade_out) as f32);
    }
    g.clamp(0.0, 1.0)
}

impl AudioEngine {
    pub fn new() -> Option<Self> {
        Self::new_with_device(None)
    }

    pub fn new_with_device(device_name: Option<&str>) -> Option<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) if name != "No Device" => {
                let found = host
                    .output_devices()
                    .ok()?
                    .find(|d| d.name().ok().as_deref() == Some(name))
                    .or_else(|| {
                        host.devices().ok()?.find(|d| {
                            d.name().ok().as_deref() == Some(name)
                                && d.default_output_config().is_ok()
                        })
                    });
                if found.is_none() {
                    println!(
                        "  Audio device '{}' not available as output, falling back to default",
                        name
                    );
                }
                found.or_else(|| host.default_output_device())
            }
            _ => host.default_output_device(),
        }?;
        let actual_device_name = device.name().unwrap_or_else(|_| "Unknown".into());
        println!("  Audio output device: {}", actual_device_name);
        let supported = device.default_output_config().ok()?;
        let config: cpal::StreamConfig = supported.into();

        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        println!("  Audio engine: {} Hz, {} channels", sample_rate, channels);

        let playing = Arc::new(AtomicBool::new(false));
        let position_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let clips: Arc<Mutex<Vec<PlaybackClip>>> = Arc::new(Mutex::new(Vec::new()));
        let effect_regions: Arc<Mutex<Vec<AudioEffectRegion>>> = Arc::new(Mutex::new(Vec::new()));
        let master_volume = Arc::new(AtomicU64::new(1.0f64.to_bits()));
        let rms_peak = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let p = playing.clone();
        let pos = position_bits.clone();
        let c = clips.clone();
        let er = effect_regions.clone();
        let vol = master_volume.clone();
        let rms = rms_peak.clone();
        let sr = sample_rate as f64;

        let mut fx_buf_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
        let mut fx_buf_r = vec![0.0f32; EFFECT_BLOCK_SIZE];
        let mut fx_out_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
        let mut fx_out_r = vec![0.0f32; EFFECT_BLOCK_SIZE];

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if !p.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        store_f64(&rms, 0.0);
                        return;
                    }

                    let current_time = load_f64(&pos);
                    let gain = load_f64(&vol) as f32;
                    let clips_guard = match c.try_lock() {
                        Ok(guard) => guard,
                        Err(_) => {
                            data.fill(0.0);
                            return;
                        }
                    };

                    let regions_guard = er.try_lock().ok();

                    let frames = data.len() / channels;
                    let mut sum_sq = 0.0f64;

                    // Mix all clips per-sample first (dry mix)
                    let mut dry_mix = vec![0.0f32; frames];
                    for i in 0..frames {
                        let t = current_time + i as f64 / sr;
                        let mut mix = 0.0f32;
                        for clip in clips_guard.iter() {
                            let clip_t = t - clip.start_time_secs;
                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                let source_idx = (clip_t * clip.source_sample_rate as f64) as usize;
                                if source_idx < clip.buffer.len() {
                                    let fg = clip_fade_gain(
                                        clip_t,
                                        clip.duration_secs,
                                        clip.fade_in_secs,
                                        clip.fade_out_secs,
                                    );
                                    mix += clip.buffer[source_idx] * fg * clip.volume;
                                }
                            }
                        }
                        dry_mix[i] = mix;
                    }

                    // Process through effect regions if any are active
                    if let Some(ref regions) = regions_guard {
                        if !regions.is_empty() {
                            for region in regions.iter() {
                                let region_start_secs =
                                    region.x_start_px as f64 / PIXELS_PER_SECOND as f64;
                                let region_end_secs =
                                    region.x_end_px as f64 / PIXELS_PER_SECOND as f64;

                                let any_overlap = clips_guard.iter().any(|clip| {
                                    let clip_y_end = clip.position_y + clip.height;
                                    clip.position_y < region.y_end && clip_y_end > region.y_start
                                });

                                if !any_overlap || region.plugins.is_empty() {
                                    continue;
                                }

                                // Process block-by-block through plugin chain
                                let mut offset = 0;
                                while offset < frames {
                                    let block_len = (frames - offset).min(EFFECT_BLOCK_SIZE);
                                    let t_start = current_time + offset as f64 / sr;
                                    let t_end = t_start + block_len as f64 / sr;
                                    let mid_t = (t_start + t_end) * 0.5;

                                    if mid_t < region_start_secs || mid_t > region_end_secs {
                                        offset += block_len;
                                        continue;
                                    }

                                    // Mix only clips that overlap this region spatially
                                    for j in 0..block_len {
                                        let t = current_time + (offset + j) as f64 / sr;
                                        let mut region_mix = 0.0f32;
                                        for clip in clips_guard.iter() {
                                            let clip_y_end = clip.position_y + clip.height;
                                            if clip.position_y >= region.y_end
                                                || clip_y_end <= region.y_start
                                            {
                                                continue;
                                            }
                                            let clip_t = t - clip.start_time_secs;
                                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                                let source_idx = (clip_t
                                                    * clip.source_sample_rate as f64)
                                                    as usize;
                                                if source_idx < clip.buffer.len() {
                                                    let fg = clip_fade_gain(
                                                        clip_t,
                                                        clip.duration_secs,
                                                        clip.fade_in_secs,
                                                        clip.fade_out_secs,
                                                    );
                                                    region_mix +=
                                                        clip.buffer[source_idx] * fg * clip.volume;
                                                }
                                            }
                                        }
                                        fx_buf_l[j] = region_mix;
                                        fx_buf_r[j] = region_mix;
                                    }

                                    #[allow(unused_mut)]
                                    let (mut src_l, mut src_r, mut dst_l, mut dst_r) = (
                                        &mut fx_buf_l,
                                        &mut fx_buf_r,
                                        &mut fx_out_l,
                                        &mut fx_out_r,
                                    );

                                    for plugin_mutex in &region.plugins {
                                        dst_l[..block_len].fill(0.0);
                                        dst_r[..block_len].fill(0.0);
                                        if let Ok(mut guard) = plugin_mutex.try_lock() {
                                            if let Some(ref mut plugin) = *guard {
                                                let inputs: [&[f32]; 2] =
                                                    [&src_l[..block_len], &src_r[..block_len]];
                                                let mut outputs: [&mut [f32]; 2] = [
                                                    &mut dst_l[..block_len],
                                                    &mut dst_r[..block_len],
                                                ];
                                                let _ = plugin.process(
                                                    &inputs,
                                                    &mut outputs,
                                                    block_len,
                                                );
                                            }
                                        }
                                        std::mem::swap(src_l, dst_l);
                                        std::mem::swap(src_r, dst_r);
                                    }

                                    // Replace dry mix for these frames with wet (mono from stereo)
                                    for j in 0..block_len {
                                        let wet = (src_l[j] + src_r[j]) * 0.5;
                                        let t = current_time + (offset + j) as f64 / sr;
                                        let mut overlap_dry = 0.0f32;
                                        for clip in clips_guard.iter() {
                                            let clip_y_end = clip.position_y + clip.height;
                                            if clip.position_y >= region.y_end
                                                || clip_y_end <= region.y_start
                                            {
                                                continue;
                                            }
                                            let clip_t = t - clip.start_time_secs;
                                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                                let source_idx = (clip_t
                                                    * clip.source_sample_rate as f64)
                                                    as usize;
                                                if source_idx < clip.buffer.len() {
                                                    let fg = clip_fade_gain(
                                                        clip_t,
                                                        clip.duration_secs,
                                                        clip.fade_in_secs,
                                                        clip.fade_out_secs,
                                                    );
                                                    overlap_dry +=
                                                        clip.buffer[source_idx] * fg * clip.volume;
                                                }
                                            }
                                        }
                                        dry_mix[offset + j] =
                                            dry_mix[offset + j] - overlap_dry + wet;
                                    }

                                    offset += block_len;
                                }
                            }
                        }
                    }

                    // Write final output
                    for i in 0..frames {
                        let mixed = (dry_mix[i] * gain).clamp(-1.0, 1.0);
                        sum_sq += (mixed as f64) * (mixed as f64);
                        let base = i * channels;
                        for ch in 0..channels {
                            data[base + ch] = mixed;
                        }
                    }

                    if frames > 0 {
                        let rms_val = (sum_sq / frames as f64).sqrt();
                        store_f64(&rms, rms_val);
                    }

                    let new_time = current_time + frames as f64 / sr;
                    store_f64(&pos, new_time);
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
            .ok()?;

        stream.play().ok()?;

        Some(Self {
            _stream: stream,
            device_name: actual_device_name,
            playing,
            position_bits,
            clips,
            effect_regions,
            master_volume,
            rms_peak,
        })
    }

    pub fn toggle_playback(&self) {
        let was = self.playing.load(Ordering::Relaxed);
        self.playing.store(!was, Ordering::Relaxed);
        if !was {
            println!("  Playback started");
        } else {
            println!("  Playback paused");
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn seek_to_seconds(&self, secs: f64) {
        store_f64(&self.position_bits, secs);
    }

    pub fn position_seconds(&self) -> f64 {
        load_f64(&self.position_bits)
    }

    pub fn update_clips(
        &self,
        waveform_positions: &[[f32; 2]],
        waveform_sizes: &[[f32; 2]],
        audio_clips: &[AudioClipData],
        fade_ins_px: &[f32],
        fade_outs_px: &[f32],
        volumes: &[f32],
    ) {
        let mut clips = self.clips.lock().unwrap();
        clips.clear();
        for (i, ((pos, size), clip_data)) in waveform_positions
            .iter()
            .zip(waveform_sizes.iter())
            .zip(audio_clips.iter())
            .enumerate()
        {
            let start_secs = pos[0] as f64 / PIXELS_PER_SECOND as f64;
            let fi = fade_ins_px.get(i).copied().unwrap_or(0.0);
            let fo = fade_outs_px.get(i).copied().unwrap_or(0.0);
            let vol = volumes.get(i).copied().unwrap_or(1.0);
            clips.push(PlaybackClip {
                buffer: clip_data.samples.clone(),
                source_sample_rate: clip_data.sample_rate,
                start_time_secs: start_secs,
                duration_secs: clip_data.duration_secs as f64,
                position_y: pos[1],
                height: size[1],
                fade_in_secs: (fi / PIXELS_PER_SECOND) as f64,
                fade_out_secs: (fo / PIXELS_PER_SECOND) as f64,
                volume: vol,
            });
        }
    }

    pub fn update_effect_regions(&self, regions: Vec<AudioEffectRegion>) {
        if let Ok(mut guard) = self.effect_regions.lock() {
            *guard = regions;
        }
    }

    pub fn set_master_volume(&self, v: f32) {
        store_f64(&self.master_volume, v.clamp(0.0, 1.0) as f64);
    }

    pub fn master_volume(&self) -> f32 {
        load_f64(&self.master_volume) as f32
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn rms_peak(&self) -> f32 {
        load_f64(&self.rms_peak) as f32
    }
}

pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: usize,
    recording: Arc<AtomicBool>,
}

impl AudioRecorder {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let supported = device.default_input_config().ok()?;
        let config: cpal::StreamConfig = supported.into();

        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;
        println!(
            "  Audio recorder: {} Hz, {} channels",
            sample_rate, channels
        );

        Some(Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate,
            channels,
            recording: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    pub fn start(&mut self) -> bool {
        if self.is_recording() {
            return false;
        }

        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => return false,
        };
        let supported = match device.default_input_config() {
            Ok(c) => c,
            Err(_) => return false,
        };
        let config: cpal::StreamConfig = supported.into();
        self.sample_rate = config.sample_rate.0;
        self.channels = config.channels as usize;

        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        self.buffer = buf.clone();
        let rec = self.recording.clone();

        let channels = self.channels;
        let stream = match device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !rec.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(mut guard) = buf.try_lock() {
                    guard.extend_from_slice(data);
                }
            },
            |err| eprintln!("Recording stream error: {}", err),
            None,
        ) {
            Ok(s) => s,
            Err(_) => return false,
        };

        if stream.play().is_err() {
            return false;
        }

        self.stream = Some(stream);
        self.recording.store(true, Ordering::Relaxed);
        println!(
            "  Recording started ({} ch, {} Hz)",
            channels, self.sample_rate
        );
        true
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn current_snapshot(&self) -> Option<LoadedAudio> {
        if !self.is_recording() {
            return None;
        }
        let interleaved = {
            let guard = self.buffer.try_lock().ok()?;
            guard.clone()
        };
        if interleaved.is_empty() {
            return None;
        }

        let channels = self.channels;
        let sample_rate = self.sample_rate;

        let (left, right, mono) = deinterleave_stereo(&interleaved, channels);

        let duration_secs = mono.len() as f32 / sample_rate as f32;
        let width = duration_secs * PIXELS_PER_SECOND;

        Some(LoadedAudio {
            samples: Arc::new(mono),
            left_samples: Arc::new(left),
            right_samples: Arc::new(right),
            sample_rate,
            duration_secs,
            width,
        })
    }

    pub fn stop(&mut self) -> Option<LoadedAudio> {
        if !self.is_recording() {
            return None;
        }
        self.recording.store(false, Ordering::Relaxed);
        self.stream = None;

        let interleaved = {
            let guard = self.buffer.lock().ok()?;
            guard.clone()
        };

        if interleaved.is_empty() {
            return None;
        }

        let channels = self.channels;
        let sample_rate = self.sample_rate;

        let (left, right, mono) = deinterleave_stereo(&interleaved, channels);

        let duration_secs = mono.len() as f32 / sample_rate as f32;
        let width = duration_secs * PIXELS_PER_SECOND;

        println!(
            "  Recording stopped: {:.1}s, {} samples",
            duration_secs,
            mono.len()
        );

        Some(LoadedAudio {
            samples: Arc::new(mono),
            left_samples: Arc::new(left),
            right_samples: Arc::new(right),
            sample_rate,
            duration_secs,
            width,
        })
    }
}

fn deinterleave_stereo(interleaved: &[f32], channels: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    if channels >= 2 {
        let frame_count = interleaved.len() / channels;
        let mut left = Vec::with_capacity(frame_count);
        let mut right = Vec::with_capacity(frame_count);
        let mut mono = Vec::with_capacity(frame_count);
        for frame in interleaved.chunks(channels) {
            let l = frame[0];
            let r = frame[1];
            left.push(l);
            right.push(r);
            mono.push((l + r) * 0.5);
        }
        (left, right, mono)
    } else {
        (
            interleaved.to_vec(),
            interleaved.to_vec(),
            interleaved.to_vec(),
        )
    }
}

pub fn load_audio_file(path: &Path) -> Option<LoadedAudio> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &Default::default(), &Default::default())
        .ok()?;

    let mut format = probed.format;
    let track = format.default_track()?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .ok()?;

    let sample_rate = track.codec_params.sample_rate?;
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);

    let mut interleaved = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if let Ok(buffer) = decoder.decode(&packet) {
            decode_buffer(&buffer, &mut interleaved);
        }
    }

    if interleaved.is_empty() {
        return None;
    }

    let (left, right, mono) = deinterleave_stereo(&interleaved, channels);

    let duration_secs = mono.len() as f32 / sample_rate as f32;
    let width = duration_secs * PIXELS_PER_SECOND;

    Some(LoadedAudio {
        samples: Arc::new(mono),
        left_samples: Arc::new(left),
        right_samples: Arc::new(right),
        sample_rate,
        duration_secs,
        width,
    })
}

pub struct ExportClip {
    pub buffer: Arc<Vec<f32>>,
    pub source_sample_rate: u32,
    pub start_time_secs: f64,
    pub duration_secs: f64,
    pub position_y: f32,
    pub height: f32,
    pub fade_in_secs: f64,
    pub fade_out_secs: f64,
    pub volume: f32,
}

pub fn render_to_wav(
    path: &std::path::Path,
    start_secs: f64,
    end_secs: f64,
    y_start: f32,
    y_end: f32,
    clips: &[ExportClip],
    effect_regions: &[AudioEffectRegion],
) -> Result<(), String> {
    let sample_rate = 48000u32;
    let sr = sample_rate as f64;
    let total_frames = ((end_secs - start_secs) * sr) as usize;

    if total_frames == 0 {
        return Err("Zero-length export region".into());
    }

    println!(
        "  Rendering {:.2}s ({} samples) at {} Hz...",
        end_secs - start_secs,
        total_frames,
        sample_rate
    );

    let mut dry_mix = vec![0.0f32; total_frames];

    for i in 0..total_frames {
        let t = start_secs + i as f64 / sr;
        let mut mix = 0.0f32;
        for clip in clips {
            let clip_y_end = clip.position_y + clip.height;
            if clip.position_y >= y_end || clip_y_end <= y_start {
                continue;
            }
            let clip_t = t - clip.start_time_secs;
            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                let source_idx = (clip_t * clip.source_sample_rate as f64) as usize;
                if source_idx < clip.buffer.len() {
                    let fg = clip_fade_gain(
                        clip_t,
                        clip.duration_secs,
                        clip.fade_in_secs,
                        clip.fade_out_secs,
                    );
                    mix += clip.buffer[source_idx] * fg * clip.volume;
                }
            }
        }
        dry_mix[i] = mix;
    }

    let mut fx_buf_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_buf_r = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_out_l = vec![0.0f32; EFFECT_BLOCK_SIZE];
    let mut fx_out_r = vec![0.0f32; EFFECT_BLOCK_SIZE];

    for region in effect_regions {
        let region_start_secs = region.x_start_px as f64 / PIXELS_PER_SECOND as f64;
        let region_end_secs = region.x_end_px as f64 / PIXELS_PER_SECOND as f64;

        let any_overlap = clips.iter().any(|clip| {
            let clip_y_end = clip.position_y + clip.height;
            clip.position_y < region.y_end && clip_y_end > region.y_start
        });

        if !any_overlap || region.plugins.is_empty() {
            continue;
        }

        let mut offset = 0;
        while offset < total_frames {
            let block_len = (total_frames - offset).min(EFFECT_BLOCK_SIZE);
            let t_start = start_secs + offset as f64 / sr;
            let t_end = t_start + block_len as f64 / sr;
            let mid_t = (t_start + t_end) * 0.5;

            if mid_t < region_start_secs || mid_t > region_end_secs {
                offset += block_len;
                continue;
            }

            for j in 0..block_len {
                let t = start_secs + (offset + j) as f64 / sr;
                let mut region_mix = 0.0f32;
                for clip in clips {
                    let clip_y_end = clip.position_y + clip.height;
                    if clip.position_y >= region.y_end || clip_y_end <= region.y_start {
                        continue;
                    }
                    let clip_t = t - clip.start_time_secs;
                    if clip_t >= 0.0 && clip_t < clip.duration_secs {
                        let source_idx = (clip_t * clip.source_sample_rate as f64) as usize;
                        if source_idx < clip.buffer.len() {
                            let fg = clip_fade_gain(
                                clip_t,
                                clip.duration_secs,
                                clip.fade_in_secs,
                                clip.fade_out_secs,
                            );
                            region_mix += clip.buffer[source_idx] * fg * clip.volume;
                        }
                    }
                }
                fx_buf_l[j] = region_mix;
                fx_buf_r[j] = region_mix;
            }

            #[allow(unused_mut)]
            let (mut src_l, mut src_r, mut dst_l, mut dst_r) =
                (&mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r);

            for plugin_mutex in &region.plugins {
                dst_l[..block_len].fill(0.0);
                dst_r[..block_len].fill(0.0);
                if let Ok(mut guard) = plugin_mutex.try_lock() {
                    if let Some(ref mut plugin) = *guard {
                        let inputs: [&[f32]; 2] = [&src_l[..block_len], &src_r[..block_len]];
                        let mut outputs: [&mut [f32]; 2] =
                            [&mut dst_l[..block_len], &mut dst_r[..block_len]];
                        let _ = plugin.process(&inputs, &mut outputs, block_len);
                    }
                }
                std::mem::swap(src_l, dst_l);
                std::mem::swap(src_r, dst_r);
            }

            for j in 0..block_len {
                let wet = (src_l[j] + src_r[j]) * 0.5;
                let t = start_secs + (offset + j) as f64 / sr;
                let mut overlap_dry = 0.0f32;
                for clip in clips {
                    let clip_y_end = clip.position_y + clip.height;
                    if clip.position_y >= region.y_end || clip_y_end <= region.y_start {
                        continue;
                    }
                    let clip_t = t - clip.start_time_secs;
                    if clip_t >= 0.0 && clip_t < clip.duration_secs {
                        let source_idx = (clip_t * clip.source_sample_rate as f64) as usize;
                        if source_idx < clip.buffer.len() {
                            let fg = clip_fade_gain(
                                clip_t,
                                clip.duration_secs,
                                clip.fade_in_secs,
                                clip.fade_out_secs,
                            );
                            overlap_dry += clip.buffer[source_idx] * fg * clip.volume;
                        }
                    }
                }
                dry_mix[offset + j] = dry_mix[offset + j] - overlap_dry + wet;
            }

            offset += block_len;
        }
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV file: {}", e))?;

    for &sample in &dry_mix {
        writer
            .write_sample(sample.clamp(-1.0, 1.0))
            .map_err(|e| format!("Failed to write sample: {}", e))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

    println!(
        "  WAV export complete: {} frames, {:.2}s",
        total_frames,
        total_frames as f64 / sr
    );

    Ok(())
}

fn decode_buffer(buffer: &AudioBufferRef, out: &mut Vec<f32>) {
    match buffer {
        AudioBufferRef::F32(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push(plane[i]);
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push(plane[i] as f32 / i32::MAX as f32);
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push(plane[i] as f32 / i16::MAX as f32);
                }
            }
        }
        AudioBufferRef::U8(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push((plane[i] as f32 - 128.0) / 128.0);
                }
            }
        }
        AudioBufferRef::S24(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push(plane[i].inner() as f32 / 8388607.0);
                }
            }
        }
        AudioBufferRef::U24(buf) => {
            let planes = buf.planes();
            let planes = planes.planes();
            if planes.is_empty() {
                return;
            }
            for i in 0..planes[0].len() {
                for plane in planes.iter() {
                    out.push((plane[i].inner() as f32 - 8388608.0) / 8388608.0);
                }
            }
        }
        _ => {}
    }
}
