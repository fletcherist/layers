use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

pub const PIXELS_PER_SECOND: f32 = 120.0;

#[derive(Clone)]
pub struct AudioClipData {
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub duration_secs: f32,
}

pub struct LoadedAudio {
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub peaks: Vec<f32>,
    pub duration_secs: f32,
    pub width: f32,
}

struct PlaybackClip {
    buffer: Arc<Vec<f32>>,
    source_sample_rate: u32,
    start_time_secs: f64,
    duration_secs: f64,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    playing: Arc<AtomicBool>,
    position_bits: Arc<AtomicU64>,
    clips: Arc<Mutex<Vec<PlaybackClip>>>,
    master_volume: Arc<AtomicU64>,
    rms_peak: Arc<AtomicU64>,
}

fn store_f64(atomic: &AtomicU64, value: f64) {
    atomic.store(value.to_bits(), Ordering::Relaxed);
}

fn load_f64(atomic: &AtomicU64) -> f64 {
    f64::from_bits(atomic.load(Ordering::Relaxed))
}

impl AudioEngine {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let config: cpal::StreamConfig = supported.into();

        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        println!("  Audio engine: {} Hz, {} channels", sample_rate, channels);

        let playing = Arc::new(AtomicBool::new(false));
        let position_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let clips: Arc<Mutex<Vec<PlaybackClip>>> = Arc::new(Mutex::new(Vec::new()));
        let master_volume = Arc::new(AtomicU64::new(1.0f64.to_bits()));
        let rms_peak = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let p = playing.clone();
        let pos = position_bits.clone();
        let c = clips.clone();
        let vol = master_volume.clone();
        let rms = rms_peak.clone();
        let sr = sample_rate as f64;

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

                    let frames = data.len() / channels;
                    let mut sum_sq = 0.0f64;
                    for i in 0..frames {
                        let t = current_time + i as f64 / sr;
                        let mut mix = 0.0f32;

                        for clip in clips_guard.iter() {
                            let clip_t = t - clip.start_time_secs;
                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                let source_idx = (clip_t * clip.source_sample_rate as f64) as usize;
                                if source_idx < clip.buffer.len() {
                                    mix += clip.buffer[source_idx];
                                }
                            }
                        }

                        let mixed = (mix * gain).clamp(-1.0, 1.0);
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
            playing,
            position_bits,
            clips,
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

    pub fn update_clips(&self, waveform_positions: &[[f32; 2]], audio_clips: &[AudioClipData]) {
        let mut clips = self.clips.lock().unwrap();
        clips.clear();
        for (pos, clip_data) in waveform_positions.iter().zip(audio_clips.iter()) {
            let start_secs = pos[0] as f64 / PIXELS_PER_SECOND as f64;
            clips.push(PlaybackClip {
                buffer: clip_data.samples.clone(),
                source_sample_rate: clip_data.sample_rate,
                start_time_secs: start_secs,
                duration_secs: clip_data.duration_secs as f64,
            });
        }
    }

    pub fn set_master_volume(&self, v: f32) {
        store_f64(&self.master_volume, v.clamp(0.0, 1.0) as f64);
    }

    pub fn master_volume(&self) -> f32 {
        load_f64(&self.master_volume) as f32
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

        let mono: Vec<f32> = if channels > 1 {
            interleaved
                .chunks(channels)
                .map(|ch| ch.iter().sum::<f32>() / ch.len() as f32)
                .collect()
        } else {
            interleaved
        };

        let duration_secs = mono.len() as f32 / sample_rate as f32;
        let width = duration_secs * PIXELS_PER_SECOND;

        let num_peaks = (width as usize).clamp(10, 4000);
        let chunk_size = (mono.len() / num_peaks).max(1);
        let peaks: Vec<f32> = mono
            .chunks(chunk_size)
            .map(|chunk| chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max))
            .collect();

        Some(LoadedAudio {
            samples: Arc::new(mono),
            sample_rate,
            peaks,
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

        let mono: Vec<f32> = if channels > 1 {
            interleaved
                .chunks(channels)
                .map(|ch| ch.iter().sum::<f32>() / ch.len() as f32)
                .collect()
        } else {
            interleaved
        };

        let duration_secs = mono.len() as f32 / sample_rate as f32;
        let width = duration_secs * PIXELS_PER_SECOND;

        let num_peaks = (width as usize).clamp(100, 4000);
        let chunk_size = (mono.len() / num_peaks).max(1);
        let peaks: Vec<f32> = mono
            .chunks(chunk_size)
            .map(|chunk| chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max))
            .collect();

        println!(
            "  Recording stopped: {:.1}s, {} samples",
            duration_secs,
            mono.len()
        );

        Some(LoadedAudio {
            samples: Arc::new(mono),
            sample_rate,
            peaks,
            duration_secs,
            width,
        })
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

    let mono: Vec<f32> = if channels > 1 {
        interleaved
            .chunks(channels)
            .map(|ch| ch.iter().sum::<f32>() / ch.len() as f32)
            .collect()
    } else {
        interleaved
    };

    let duration_secs = mono.len() as f32 / sample_rate as f32;
    let width = duration_secs * PIXELS_PER_SECOND;

    let num_peaks = (width as usize).clamp(100, 4000);
    let chunk_size = (mono.len() / num_peaks).max(1);
    let peaks: Vec<f32> = mono
        .chunks(chunk_size)
        .map(|chunk| chunk.iter().map(|s| s.abs()).fold(0.0f32, f32::max))
        .collect();

    Some(LoadedAudio {
        samples: Arc::new(mono),
        sample_rate,
        peaks,
        duration_secs,
        width,
    })
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
        _ => {}
    }
}
