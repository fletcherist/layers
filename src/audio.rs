use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::path::Path;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::entity_id::EntityId;
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

pub use crate::grid::PIXELS_PER_SECOND;
pub use crate::ui::waveform::AudioClipData;

const DEFAULT_EFFECT_BLOCK_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// Lock-free SPSC ring buffer for input monitoring
// ---------------------------------------------------------------------------

pub struct MonitorRingBuffer {
    data: Box<[std::cell::UnsafeCell<f32>]>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

// SAFETY: Only one producer (input thread) and one consumer (output thread).
unsafe impl Send for MonitorRingBuffer {}
unsafe impl Sync for MonitorRingBuffer {}

impl MonitorRingBuffer {
    pub fn new(capacity: usize) -> Arc<Self> {
        let data = (0..capacity)
            .map(|_| std::cell::UnsafeCell::new(0.0f32))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Arc::new(Self {
            data,
            capacity,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        })
    }

    /// Push samples from the input (producer) thread.
    pub fn push(&self, samples: &[f32]) {
        let mut wp = self.write_pos.load(Ordering::Relaxed);
        for &s in samples {
            // SAFETY: single producer – only the input thread calls push.
            unsafe { *self.data[wp % self.capacity].get() = s };
            wp = wp.wrapping_add(1);
        }
        self.write_pos.store(wp, Ordering::Release);
    }

    /// Pop up to `out.len()` samples sequentially from the ring buffer.
    /// If more data is available than requested, the excess stays for the next read.
    /// Returns the number of samples actually read.
    pub fn pop(&self, out: &mut [f32]) -> usize {
        let wp = self.write_pos.load(Ordering::Acquire);
        let rp = self.read_pos.load(Ordering::Relaxed);
        let available = wp.wrapping_sub(rp).min(self.capacity);
        let to_read = available.min(out.len());
        let mut rp2 = rp;
        for item in out.iter_mut().take(to_read) {
            // SAFETY: single consumer – only the output thread calls pop.
            *item = unsafe { *self.data[rp2 % self.capacity].get() };
            rp2 = rp2.wrapping_add(1);
        }
        self.read_pos.store(rp2, Ordering::Release);
        to_read
    }
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
    entity_id: EntityId,
    buffer: Arc<Vec<f32>>,
    source_sample_rate: u32,
    effective_sample_rate: f64,
    start_time_secs: f64,
    duration_secs: f64,
    position_y: f32,
    height: f32,
    fade_in_secs: f64,
    fade_out_secs: f64,
    fade_in_curve: f32,
    fade_out_curve: f32,
    volume: f32,
    pan: f32,
    buffer_offset_secs: f64,
    volume_automation: Vec<(f32, f32)>,
    pan_automation: Vec<(f32, f32)>,
    chain_plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
    chain_latency_samples: u32,
    group_bus_index: Option<usize>,
    chain_bus_index: Option<usize>,
}

pub struct ChainBus {
    pub plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
    pub latency_samples: u32,
}

pub struct GroupBus {
    pub entity_id: EntityId,
    pub plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
    pub latency_samples: u32,
    pub volume: f32,
    pub pan: f32,
}

pub struct AudioEffectRegion {
    pub x_start_px: f32,
    pub x_end_px: f32,
    pub y_start: f32,
    pub y_end: f32,
    pub plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
}

pub struct AudioInstrument {
    pub id: EntityId,
    pub x_start_px: f32,
    pub x_end_px: f32,
    pub y_start: f32,
    pub y_end: f32,
    pub gui: Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>,
    pub midi_events: Vec<TimedMidiEvent>,
    pub volume: f32,
    pub pan: f32,
    pub chain_plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
    /// Total latency of synth + chain plugins, used to pre-send MIDI events
    pub total_latency_samples: u32,
    /// If this instrument belongs to a group, index into the group_buses vec.
    pub group_bus_index: Option<usize>,
}

/// Live computer-keyboard preview MIDI (drained once per output callback).
#[derive(Clone, Copy, Debug)]
pub enum KeyboardPreviewEvent {
    NoteOn {
        target: EntityId,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        target: EntityId,
        note: u8,
    },
}

pub struct KeyboardPreviewState {
    pub target: Option<EntityId>,
    pub pending: VecDeque<KeyboardPreviewEvent>,
}

/// One-shot sample preview for the browser audition feature.
pub struct PreviewClip {
    pub left: Arc<Vec<f32>>,
    pub right: Arc<Vec<f32>>,
    pub source_sample_rate: u32,
    pub position: f64, // fractional sample position (for resampling)
    pub playing: bool,
}

pub struct TimedMidiEvent {
    pub time_secs: f64,
    pub note: u8,
    pub velocity: u8,
    pub is_note_on: bool,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    device_name: String,
    sample_rate: u32,
    playing: Arc<AtomicBool>,
    position_bits: Arc<AtomicU64>,
    clips: Arc<Mutex<Vec<PlaybackClip>>>,
    effect_regions: Arc<Mutex<Vec<AudioEffectRegion>>>,
    instrument_regions: Arc<Mutex<Vec<AudioInstrument>>>,
    group_buses: Arc<Mutex<Vec<GroupBus>>>,
    chain_buses: Arc<Mutex<Vec<ChainBus>>>,
    master_volume: Arc<AtomicU64>,
    master_pan: Arc<AtomicU64>,
    master_bus_plugins: Arc<Mutex<Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>>>,
    rms_peak: Arc<AtomicU64>,
    per_entity_rms: Arc<Mutex<HashMap<EntityId, f64>>>,
    loop_enabled: Arc<AtomicBool>,
    loop_start_bits: Arc<AtomicU64>,
    loop_end_bits: Arc<AtomicU64>,
    metronome_enabled: Arc<AtomicBool>,
    bpm_bits: Arc<AtomicU64>,
    keyboard_preview: Arc<Mutex<KeyboardPreviewState>>,
    monitoring_enabled: Arc<AtomicBool>,
    monitor_ring: Arc<MonitorRingBuffer>,
    monitor_effect_plugins: Arc<Mutex<Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>>>,
    monitor_input_channels: Arc<AtomicUsize>,
    monitor_input_sample_rate: Arc<AtomicU64>,
    preview_clip: Arc<Mutex<Option<PreviewClip>>>,
    preview_playing: Arc<AtomicBool>,
    preview_position_bits: Arc<AtomicU64>,
}

fn store_f64(atomic: &AtomicU64, value: f64) {
    atomic.store(value.to_bits(), Ordering::Relaxed);
}

fn load_f64(atomic: &AtomicU64) -> f64 {
    f64::from_bits(atomic.load(Ordering::Relaxed))
}

#[inline]
fn apply_fade_curve_f32(t: f32, curve: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let exponent = 2.0f32.powf(-curve);
    t.powf(exponent)
}

#[inline]
fn clip_fade_gain(clip_t: f64, duration: f64, fade_in: f64, fade_out: f64, fade_in_curve: f32, fade_out_curve: f32) -> f32 {
    let mut g = 1.0f32;
    if fade_in > 0.0 && clip_t < fade_in {
        let t = (clip_t / fade_in) as f32;
        g = apply_fade_curve_f32(t, fade_in_curve);
    }
    let from_end = duration - clip_t;
    if fade_out > 0.0 && from_end < fade_out {
        let t = (from_end / fade_out) as f32;
        g = g.min(apply_fade_curve_f32(t, fade_out_curve));
    }
    g.clamp(0.0, 1.0)
}

/// Render a clip's dry audio into a pre-allocated stereo buffer, compensating for chain latency.
#[inline]
fn render_clip_dry(clip: &PlaybackClip, frames: usize, current_time: f64, sr: f64, buf: &mut [[f32; 2]]) {
    let latency_offset_secs = clip.chain_latency_samples as f64 / sr;
    buf[..frames].fill([0.0, 0.0]);
    for i in 0..frames {
        let t = current_time + i as f64 / sr;
        let clip_t = t - clip.start_time_secs + latency_offset_secs;
        if clip_t >= 0.0 && clip_t < clip.duration_secs + latency_offset_secs {
            let source_idx = ((clip_t + clip.buffer_offset_secs) * clip.effective_sample_rate) as usize;
            if source_idx < clip.buffer.len() {
                let fg = clip_fade_gain(clip_t, clip.duration_secs, clip.fade_in_secs, clip.fade_out_secs, clip.fade_in_curve, clip.fade_out_curve);
                let norm_t = if clip.duration_secs > 0.0 { (clip_t / clip.duration_secs) as f32 } else { 0.0 };
                let auto_vol = crate::automation::volume_value_to_gain(
                    crate::automation::interp_automation(norm_t, &clip.volume_automation, 0.5),
                );
                let auto_pan = crate::automation::interp_automation(norm_t, &clip.pan_automation, clip.pan);
                let sample = clip.buffer[source_idx] * fg * clip.volume * auto_vol;
                let pan_angle = auto_pan * std::f32::consts::FRAC_PI_2;
                buf[i] = [sample * pan_angle.cos(), sample * pan_angle.sin()];
            }
        }
    }
}

/// Process a clip's dry audio through its effect chain in-place, block by block.
#[inline]
fn process_clip_chain(
    clip: &PlaybackClip, frames: usize, effect_block_size: usize,
    clip_dry: &mut [[f32; 2]],
    fx_buf_l: &mut [f32], fx_buf_r: &mut [f32],
    fx_out_l: &mut [f32], fx_out_r: &mut [f32],
) {
    for block_start in (0..frames).step_by(effect_block_size) {
        let block_end = (block_start + effect_block_size).min(frames);
        let block_len = block_end - block_start;
        for j in 0..block_len {
            fx_buf_l[j] = clip_dry[block_start + j][0];
            fx_buf_r[j] = clip_dry[block_start + j][1];
        }
        #[allow(unused_mut)]
        let (mut src_l, mut src_r, mut dst_l, mut dst_r): (&mut [f32], &mut [f32], &mut [f32], &mut [f32]) =
            (fx_buf_l, fx_buf_r, fx_out_l, fx_out_r);
        for plugin_arc in &clip.chain_plugins {
            dst_l[..block_len].copy_from_slice(&src_l[..block_len]);
            dst_r[..block_len].copy_from_slice(&src_r[..block_len]);
            if let Ok(guard) = plugin_arc.try_lock() {
                if let Some(ref gui) = *guard {
                    let inputs: [&[f32]; 2] = [&src_l[..block_len], &src_r[..block_len]];
                    let mut outputs: [&mut [f32]; 2] = [&mut dst_l[..block_len], &mut dst_r[..block_len]];
                    gui.process(&inputs, &mut outputs, block_len);
                }
            }
            std::mem::swap(&mut src_l, &mut dst_l);
            std::mem::swap(&mut src_r, &mut dst_r);
        }
        for j in 0..block_len {
            clip_dry[block_start + j] = [src_l[j], src_r[j]];
        }
    }
}

impl AudioEngine {
    pub fn new() -> Option<Self> {
        Self::new_with_device(None, DEFAULT_EFFECT_BLOCK_SIZE)
    }

    pub fn new_with_device(device_name: Option<&str>, effect_block_size: usize) -> Option<Self> {
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
        let instrument_regions: Arc<Mutex<Vec<AudioInstrument>>> = Arc::new(Mutex::new(Vec::new()));
        let group_buses: Arc<Mutex<Vec<GroupBus>>> = Arc::new(Mutex::new(Vec::new()));
        let chain_buses: Arc<Mutex<Vec<ChainBus>>> = Arc::new(Mutex::new(Vec::new()));
        let master_volume = Arc::new(AtomicU64::new(1.0f64.to_bits()));
        let master_pan = Arc::new(AtomicU64::new(0.5f64.to_bits()));
        let master_bus_plugins: Arc<Mutex<Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>>> = Arc::new(Mutex::new(Vec::new()));
        let rms_peak = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let per_entity_rms: Arc<Mutex<HashMap<EntityId, f64>>> = Arc::new(Mutex::new(HashMap::new()));
        let loop_enabled = Arc::new(AtomicBool::new(false));
        let loop_start_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let loop_end_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));
        let metronome_enabled = Arc::new(AtomicBool::new(false));
        let bpm_bits = Arc::new(AtomicU64::new(120.0f64.to_bits()));
        let keyboard_preview: Arc<Mutex<KeyboardPreviewState>> = Arc::new(Mutex::new(KeyboardPreviewState {
            target: None,
            pending: VecDeque::new(),
        }));
        let monitoring_enabled = Arc::new(AtomicBool::new(false));
        let monitor_ring = MonitorRingBuffer::new(8192);
        let monitor_effect_plugins: Arc<Mutex<Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let monitor_input_channels = Arc::new(AtomicUsize::new(1));
        let monitor_input_sample_rate = Arc::new(AtomicU64::new(sample_rate as u64));
        let preview_clip: Arc<Mutex<Option<PreviewClip>>> = Arc::new(Mutex::new(None));
        let preview_playing = Arc::new(AtomicBool::new(false));
        let preview_position_bits = Arc::new(AtomicU64::new(0.0f64.to_bits()));

        let p = playing.clone();
        let pos = position_bits.clone();
        let c = clips.clone();
        let er = effect_regions.clone();
        let inst_r = instrument_regions.clone();
        let gb = group_buses.clone();
        let cb = chain_buses.clone();
        let kb_preview = keyboard_preview.clone();
        let vol = master_volume.clone();
        let m_pan = master_pan.clone();
        let m_plugins = master_bus_plugins.clone();
        let rms = rms_peak.clone();
        let per_ent_rms = per_entity_rms.clone();
        let lp_en = loop_enabled.clone();
        let lp_s = loop_start_bits.clone();
        let lp_e = loop_end_bits.clone();
        let met_en = metronome_enabled.clone();
        let met_bpm = bpm_bits.clone();
        let mon_en = monitoring_enabled.clone();
        let mon_ring_c = monitor_ring.clone();
        let mon_fx = monitor_effect_plugins.clone();
        let mon_in_ch = monitor_input_channels.clone();
        let mon_in_sr = monitor_input_sample_rate.clone();
        let preview_c = preview_clip.clone();
        let preview_p = preview_playing.clone();
        let preview_pos = preview_position_bits.clone();
        let sr = sample_rate as f64;

        let mut fx_buf_l = vec![0.0f32; effect_block_size];
        let mut fx_buf_r = vec![0.0f32; effect_block_size];
        let mut fx_out_l = vec![0.0f32; effect_block_size];
        let mut fx_out_r = vec![0.0f32; effect_block_size];
        let mut inst_out_l = vec![0.0f32; effect_block_size];
        let mut inst_out_r = vec![0.0f32; effect_block_size];
        let mut mon_raw = vec![0.0f32; 8192];
        let mut mon_fx_l = vec![0.0f32; 4096];
        let mut mon_fx_r = vec![0.0f32; 4096];
        let mut mon_fx_out_l = vec![0.0f32; effect_block_size];
        let mut mon_fx_out_r = vec![0.0f32; effect_block_size];

        let initial_mix_capacity: usize = 8192;
        let mut mix_capacity = initial_mix_capacity;
        let mut dry_mix = vec![[0.0f32; 2]; initial_mix_capacity];
        let mut clip_dry = vec![[0.0f32; 2]; initial_mix_capacity];
        let mut group_bus_l = vec![0.0f32; initial_mix_capacity];
        let mut group_bus_r = vec![0.0f32; initial_mix_capacity];
        let mut chain_bus_l = vec![0.0f32; initial_mix_capacity];
        let mut chain_bus_r = vec![0.0f32; initial_mix_capacity];
        let mut kb_batch_buf: Vec<KeyboardPreviewEvent> = Vec::with_capacity(64);
        let mut silent_buf = vec![0.0f32; effect_block_size];

        let mut entity_rms_local: HashMap<EntityId, f64> = HashMap::new();
        let mut mon_debug_counter: u32 = 0;
        let mut was_playing = false;
        let mut met_phase: f64 = 0.0;
        let mut met_samples_left: u32 = 0;
        let mut met_click_total: u32 = 0;
        let mut met_freq: f64 = 1000.0;
        let mut met_last_beat: i64 = -1;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let is_playing = p.load(Ordering::Relaxed);

                    // Send all-notes-off to every instrument on play→stop transition
                    if was_playing && !is_playing {
                        met_last_beat = -1;
                        if let Ok(mut g) = kb_preview.try_lock() {
                            g.pending.clear();
                        }
                        if let Ok(inst_guard) = inst_r.try_lock() {
                            for region in inst_guard.iter() {
                                if let Ok(gui_guard) = region.gui.try_lock() {
                                    if let Some(ref gui) = *gui_guard {
                                        for note in 0..128u8 {
                                            gui.send_midi_note_off(note, 0, 0, 0);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    was_playing = is_playing;

                    // Even when stopped, process instruments for GUI keyboard preview
                    let has_instruments = inst_r.try_lock()
                        .ok()
                        .map_or(false, |g| !g.is_empty());

                    let mon_active = mon_en.load(Ordering::Relaxed);
                    let preview_active = preview_p.load(Ordering::Relaxed);
                    if !is_playing && !has_instruments && !mon_active && !preview_active {
                        data.fill(0.0);
                        store_f64(&rms, 0.0);
                        if let Ok(mut m) = per_ent_rms.try_lock() { m.clear(); }
                        return;
                    }

                    let current_time = load_f64(&pos);
                    let gain = load_f64(&vol) as f32;
                    let frames = data.len() / channels;
                    let mut sum_sq = 0.0f64;

                    let clips_guard = c.try_lock().ok();
                    let regions_guard = er.try_lock().ok();

                    let clips_ref: &[PlaybackClip] = clips_guard
                        .as_ref()
                        .map(|g| g.as_slice())
                        .unwrap_or(&[]);

                    if frames > mix_capacity {
                        mix_capacity = frames;
                        dry_mix.resize(mix_capacity, [0.0f32; 2]);
                        clip_dry.resize(mix_capacity, [0.0f32; 2]);
                        group_bus_l.resize(mix_capacity, 0.0f32);
                        group_bus_r.resize(mix_capacity, 0.0f32);
                        chain_bus_l.resize(mix_capacity, 0.0f32);
                        chain_bus_r.resize(mix_capacity, 0.0f32);
                    }
                    dry_mix[..frames].fill([0.0, 0.0]);

                    // Per-entity RMS accumulators
                    entity_rms_local.clear();
                    let mut clip_sum_sq: Vec<f64> = vec![0.0; clips_ref.len()];

                    // Drain keyboard preview events before instrument processing
                    kb_batch_buf.clear();
                    if let Ok(mut g) = kb_preview.try_lock() {
                        kb_batch_buf.extend(g.pending.drain(..));
                    }

                    // Count group buses for per-bus instrument accumulators
                    let num_group_buses = gb.try_lock().map(|g| g.len()).unwrap_or(0);
                    // Per-bus L/R accumulators for grouped instrument output.
                    // Indexed as inst_per_bus_l[bus_idx * frames + sample].
                    let inst_per_bus_total = num_group_buses * frames;
                    let mut inst_per_bus_l: Vec<f32> = vec![0.0f32; inst_per_bus_total];
                    let mut inst_per_bus_r: Vec<f32> = vec![0.0f32; inst_per_bus_total];

                    // Process instrument regions (MIDI → VST3 → audio, additive)
                    // Always process — instruments need continuous process() for GUI keyboard preview
                    if let Ok(inst_guard) = inst_r.try_lock() {
                        for region in inst_guard.iter() {
                            let mut inst_sum_sq = 0.0f64;
                            let region_start_secs = region.x_start_px as f64 / PIXELS_PER_SECOND as f64;
                            let region_end_secs = region.x_end_px as f64 / PIXELS_PER_SECOND as f64;
                            // Pre-send MIDI by total latency so output aligns with timeline
                            let inst_latency_secs = region.total_latency_samples as f64 / sr;

                            let mut offset = 0;
                            while offset < frames {
                                let block_len = (frames - offset).min(effect_block_size);
                                let t_start = current_time + offset as f64 / sr;
                                let t_end = t_start + block_len as f64 / sr;
                                let in_region = is_playing
                                    && t_end > region_start_secs - inst_latency_secs
                                    && t_start < region_end_secs;

                                if let Ok(gui_guard) = region.gui.try_lock() {
                                    if let Some(ref gui) = *gui_guard {
                                        // Send scheduled MIDI events only when playing within region
                                        if in_region {
                                            for ev in &region.midi_events {
                                                // Shift MIDI events earlier by latency so output aligns
                                                let adjusted_time = ev.time_secs - inst_latency_secs;
                                                if adjusted_time >= t_start && adjusted_time < t_end {
                                                    let so = ((adjusted_time - t_start) * sr) as i32;
                                                    if ev.is_note_on {
                                                        gui.send_midi_note_on(ev.note, ev.velocity, 0, so);
                                                    } else {
                                                        gui.send_midi_note_off(ev.note, 0, 0, so);
                                                    }
                                                }
                                            }
                                        }

                                        if offset == 0 {
                                            for ev in &kb_batch_buf {
                                                match *ev {
                                                    KeyboardPreviewEvent::NoteOn {
                                                        target,
                                                        note,
                                                        velocity,
                                                    } if target == region.id => {
                                                        gui.send_midi_note_on(note, velocity, 0, 0);
                                                    }
                                                    KeyboardPreviewEvent::NoteOff { target, note }
                                                        if target == region.id =>
                                                    {
                                                        gui.send_midi_note_off(note, 0, 0, 0);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }

                                        // Always call process() — needed for GUI keyboard + sustain
                                        inst_out_l[..block_len].fill(0.0);
                                        inst_out_r[..block_len].fill(0.0);

                                        let in_ch = gui.audio_input_channels();
                                        silent_buf[..block_len].fill(0.0);
                                        let silent_ref: &[f32] = &silent_buf[..block_len];
                                        const MAX_INST_IN_CH: usize = 32;
                                        let capped_ch = in_ch.min(MAX_INST_IN_CH);
                                        let inst_inputs_arr: [&[f32]; MAX_INST_IN_CH] = [silent_ref; MAX_INST_IN_CH];
                                        let inputs = &inst_inputs_arr[..capped_ch];
                                        let mut outputs: [&mut [f32]; 2] = [
                                            &mut inst_out_l[..block_len],
                                            &mut inst_out_r[..block_len],
                                        ];

                                        gui.process(inputs, &mut outputs, block_len);

                                        // Process through instrument's own effect chain (excludes group FX)
                                        if !region.chain_plugins.is_empty() {
                                            fx_buf_l[..block_len].copy_from_slice(&inst_out_l[..block_len]);
                                            fx_buf_r[..block_len].copy_from_slice(&inst_out_r[..block_len]);
                                            #[allow(unused_mut)]
                                            let (mut sl, mut sr_buf, mut dl, mut dr) = (
                                                &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r,
                                            );
                                            for plugin_arc in &region.chain_plugins {
                                                dl[..block_len].copy_from_slice(&sl[..block_len]);
                                                dr[..block_len].copy_from_slice(&sr_buf[..block_len]);
                                                if let Ok(g2) = plugin_arc.try_lock() {
                                                    if let Some(ref fx_gui) = *g2 {
                                                        let ins: [&[f32]; 2] = [&sl[..block_len], &sr_buf[..block_len]];
                                                        let mut outs: [&mut [f32]; 2] = [&mut dl[..block_len], &mut dr[..block_len]];
                                                        fx_gui.process(&ins, &mut outs, block_len);
                                                    }
                                                }
                                                std::mem::swap(sl, dl);
                                                std::mem::swap(sr_buf, dr);
                                            }
                                            inst_out_l[..block_len].copy_from_slice(&sl[..block_len]);
                                            inst_out_r[..block_len].copy_from_slice(&sr_buf[..block_len]);
                                        }

                                        // Apply instrument volume/pan
                                        let iv = region.volume;
                                        let ip = region.pan.clamp(0.0, 1.0);
                                        let il_mul = (2.0 * (1.0 - ip)).min(1.0) * iv;
                                        let ir_mul = (2.0 * ip).min(1.0) * iv;

                                        // Route: grouped instruments go to per-bus accumulator when
                                        // playing (group FX applied later in Pass 4); otherwise dry_mix.
                                        let route_to_bus = is_playing
                                            && region.group_bus_index.filter(|&i| i < num_group_buses).is_some();
                                        if route_to_bus {
                                            let gbi = region.group_bus_index.unwrap();
                                            let base = gbi * frames + offset;
                                            for j in 0..block_len {
                                                let il = inst_out_l[j] * il_mul;
                                                let ir = inst_out_r[j] * ir_mul;
                                                inst_per_bus_l[base + j] += il;
                                                inst_per_bus_r[base + j] += ir;
                                                let mono = ((il + ir) * 0.5) as f64;
                                                inst_sum_sq += mono * mono;
                                            }
                                        } else {
                                            for j in 0..block_len {
                                                let il = inst_out_l[j] * il_mul;
                                                let ir = inst_out_r[j] * ir_mul;
                                                dry_mix[offset + j][0] += il;
                                                dry_mix[offset + j][1] += ir;
                                                let mono = ((il + ir) * 0.5) as f64;
                                                inst_sum_sq += mono * mono;
                                            }
                                        }
                                    }
                                }

                                offset += block_len;
                            }
                            if frames > 0 {
                                entity_rms_local.insert(region.id, (inst_sum_sq / frames as f64).sqrt());
                            }
                        }
                    }

                    if is_playing {
                    // Pass 1: clips without FX and without group → dry_mix
                    for i in 0..frames {
                        let t = current_time + i as f64 / sr;
                        let mut mix_l = 0.0f32;
                        let mut mix_r = 0.0f32;
                        for (ci, clip) in clips_ref.iter().enumerate() {
                            if !clip.chain_plugins.is_empty() || clip.group_bus_index.is_some() || clip.chain_bus_index.is_some() {
                                continue;
                            }
                            let clip_t = t - clip.start_time_secs;
                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                let source_idx = ((clip_t + clip.buffer_offset_secs) * clip.effective_sample_rate) as usize;
                                if source_idx < clip.buffer.len() {
                                    let fg = clip_fade_gain(
                                        clip_t,
                                        clip.duration_secs,
                                        clip.fade_in_secs,
                                        clip.fade_out_secs,
                                        clip.fade_in_curve,
                                        clip.fade_out_curve,
                                    );
                                    let norm_t = if clip.duration_secs > 0.0 {
                                        (clip_t / clip.duration_secs) as f32
                                    } else {
                                        0.0
                                    };
                                    let auto_vol = crate::automation::volume_value_to_gain(
                                        crate::automation::interp_automation(
                                            norm_t, &clip.volume_automation, 0.5,
                                        ),
                                    );
                                    let auto_pan = crate::automation::interp_automation(
                                        norm_t, &clip.pan_automation, clip.pan,
                                    );
                                    let sample = clip.buffer[source_idx] * fg * clip.volume * auto_vol;
                                    // Constant-power panning
                                    let pan_angle = auto_pan * std::f32::consts::FRAC_PI_2;
                                    let pan_l = pan_angle.cos();
                                    let pan_r = pan_angle.sin();
                                    mix_l += sample * pan_l;
                                    mix_r += sample * pan_r;
                                    let mono = (sample * pan_l + sample * pan_r) * 0.5;
                                    clip_sum_sq[ci] += (mono as f64) * (mono as f64);
                                }
                            }
                        }
                        dry_mix[i] = [mix_l, mix_r];
                    }

                    // Pass 2: clips with clip-level FX but no group and no chain bus → clip FX → dry_mix
                    for (ci, clip) in clips_ref.iter().enumerate() {
                        if clip.chain_plugins.is_empty() || clip.group_bus_index.is_some() || clip.chain_bus_index.is_some() {
                            continue;
                        }
                        render_clip_dry(clip, frames, current_time, sr, &mut clip_dry);
                        process_clip_chain(clip, frames, effect_block_size, &mut clip_dry,
                            &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r);
                        let mut csq = 0.0f64;
                        for j in 0..frames {
                            dry_mix[j][0] += clip_dry[j][0];
                            dry_mix[j][1] += clip_dry[j][1];
                            let mono = ((clip_dry[j][0] + clip_dry[j][1]) * 0.5) as f64;
                            csq += mono * mono;
                        }
                        clip_sum_sq[ci] += csq;
                    }

                    // Pass 2.5: chain bus processing — shared effect chains process once on summed audio
                    if let Ok(cb_guard) = cb.try_lock() {
                        if !cb_guard.is_empty() {
                            for (bus_idx, bus) in cb_guard.iter().enumerate() {
                                chain_bus_l[..frames].fill(0.0);
                                chain_bus_r[..frames].fill(0.0);

                                // Sum dry audio from all clips assigned to this chain bus (ungrouped only)
                                for (ci, clip) in clips_ref.iter().enumerate() {
                                    if clip.chain_bus_index != Some(bus_idx) { continue; }
                                    if clip.group_bus_index.is_some() { continue; }
                                    render_clip_dry(clip, frames, current_time, sr, &mut clip_dry);
                                    let mut csq = 0.0f64;
                                    for j in 0..frames {
                                        chain_bus_l[j] += clip_dry[j][0];
                                        chain_bus_r[j] += clip_dry[j][1];
                                        let mono = ((clip_dry[j][0] + clip_dry[j][1]) * 0.5) as f64;
                                        csq += mono * mono;
                                    }
                                    clip_sum_sq[ci] += csq;
                                }

                                // Process chain bus through plugins (block-by-block)
                                if !bus.plugins.is_empty() {
                                    for block_start in (0..frames).step_by(effect_block_size) {
                                        let block_end = (block_start + effect_block_size).min(frames);
                                        let block_len = block_end - block_start;
                                        fx_buf_l[..block_len].copy_from_slice(&chain_bus_l[block_start..block_end]);
                                        fx_buf_r[..block_len].copy_from_slice(&chain_bus_r[block_start..block_end]);
                                        #[allow(unused_mut)]
                                        let (mut src_l, mut src_r, mut dst_l, mut dst_r) = (
                                            &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r,
                                        );
                                        for plugin_arc in &bus.plugins {
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
                                        for j in 0..block_len {
                                            chain_bus_l[block_start + j] = src_l[j];
                                            chain_bus_r[block_start + j] = src_r[j];
                                        }
                                    }
                                }

                                // Mix chain bus result into dry_mix
                                for j in 0..frames {
                                    dry_mix[j][0] += chain_bus_l[j];
                                    dry_mix[j][1] += chain_bus_r[j];
                                }
                            }
                        }
                    }

                    // Pass 3: grouped clips → optional clip FX → group bus
                    // Pass 4: group FX on each bus → dry_mix
                    if let Ok(buses_guard) = gb.try_lock() {
                        if !buses_guard.is_empty() {
                            for (bus_idx, bus) in buses_guard.iter().enumerate() {
                                group_bus_l[..frames].fill(0.0);
                                group_bus_r[..frames].fill(0.0);

                                for (ci, clip) in clips_ref.iter().enumerate() {
                                    if clip.group_bus_index != Some(bus_idx) {
                                        continue;
                                    }
                                    if clip.chain_plugins.is_empty() {
                                        // No clip-level FX: render dry directly into group bus
                                        let latency_offset_secs = clip.chain_latency_samples as f64 / sr;
                                        for i in 0..frames {
                                            let t = current_time + i as f64 / sr;
                                            let clip_t = t - clip.start_time_secs + latency_offset_secs;
                                            if clip_t >= 0.0 && clip_t < clip.duration_secs + latency_offset_secs {
                                                let source_idx = ((clip_t + clip.buffer_offset_secs) * clip.effective_sample_rate) as usize;
                                                if source_idx < clip.buffer.len() {
                                                    let fg = clip_fade_gain(clip_t, clip.duration_secs, clip.fade_in_secs, clip.fade_out_secs, clip.fade_in_curve, clip.fade_out_curve);
                                                    let norm_t = if clip.duration_secs > 0.0 { (clip_t / clip.duration_secs) as f32 } else { 0.0 };
                                                    let auto_vol = crate::automation::volume_value_to_gain(crate::automation::interp_automation(norm_t, &clip.volume_automation, 0.5));
                                                    let auto_pan = crate::automation::interp_automation(norm_t, &clip.pan_automation, clip.pan);
                                                    let sample = clip.buffer[source_idx] * fg * clip.volume * auto_vol;
                                                    let pan_angle = auto_pan * std::f32::consts::FRAC_PI_2;
                                                    let sl = sample * pan_angle.cos();
                                                    let sr_s = sample * pan_angle.sin();
                                                    group_bus_l[i] += sl;
                                                    group_bus_r[i] += sr_s;
                                                    let mono = ((sl + sr_s) * 0.5) as f64;
                                                    clip_sum_sq[ci] += mono * mono;
                                                }
                                            }
                                        }
                                    } else {
                                        // Clip-level FX: render dry → clip FX → group bus
                                        render_clip_dry(clip, frames, current_time, sr, &mut clip_dry);
                                        process_clip_chain(clip, frames, effect_block_size, &mut clip_dry,
                                            &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r);
                                        let mut csq = 0.0f64;
                                        for j in 0..frames {
                                            group_bus_l[j] += clip_dry[j][0];
                                            group_bus_r[j] += clip_dry[j][1];
                                            let mono = ((clip_dry[j][0] + clip_dry[j][1]) * 0.5) as f64;
                                            csq += mono * mono;
                                        }
                                        clip_sum_sq[ci] += csq;
                                    }
                                }

                                // Add grouped instrument output to the bus
                                if bus_idx < num_group_buses {
                                    let base = bus_idx * frames;
                                    for j in 0..frames {
                                        group_bus_l[j] += inst_per_bus_l[base + j];
                                        group_bus_r[j] += inst_per_bus_r[base + j];
                                    }
                                }

                                // Pass 4: process group bus through group FX
                                if !bus.plugins.is_empty() {
                                    for block_start in (0..frames).step_by(effect_block_size) {
                                        let block_end = (block_start + effect_block_size).min(frames);
                                        let block_len = block_end - block_start;
                                        fx_buf_l[..block_len].copy_from_slice(&group_bus_l[block_start..block_end]);
                                        fx_buf_r[..block_len].copy_from_slice(&group_bus_r[block_start..block_end]);
                                        #[allow(unused_mut)]
                                        let (mut src_l, mut src_r, mut dst_l, mut dst_r) = (
                                            &mut fx_buf_l, &mut fx_buf_r, &mut fx_out_l, &mut fx_out_r,
                                        );
                                        for plugin_arc in &bus.plugins {
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
                                        for j in 0..block_len {
                                            group_bus_l[block_start + j] = src_l[j];
                                            group_bus_r[block_start + j] = src_r[j];
                                        }
                                    }
                                }

                                // Apply group-level volume and stereo balance (linear law)
                                let gv = bus.volume;
                                let gp = bus.pan.clamp(0.0, 1.0);
                                let l_mul = (2.0 * (1.0 - gp)).min(1.0) * gv;
                                let r_mul = (2.0 * gp).min(1.0) * gv;
                                let mut grp_sq = 0.0f64;
                                for j in 0..frames {
                                    let gl = group_bus_l[j] * l_mul;
                                    let gr = group_bus_r[j] * r_mul;
                                    dry_mix[j][0] += gl;
                                    dry_mix[j][1] += gr;
                                    let mono = ((gl + gr) * 0.5) as f64;
                                    grp_sq += mono * mono;
                                }
                                if frames > 0 {
                                    entity_rms_local.insert(bus.entity_id, (grp_sq / frames as f64).sqrt());
                                }
                            }
                        }
                    }

                    // Process through effect regions if any are active
                    if let Some(ref regions) = regions_guard {
                        if !regions.is_empty() {
                            for region in regions.iter() {
                                let region_start_secs =
                                    region.x_start_px as f64 / PIXELS_PER_SECOND as f64;
                                let region_end_secs =
                                    region.x_end_px as f64 / PIXELS_PER_SECOND as f64;

                                let any_overlap = clips_ref.iter().any(|clip| {
                                    let clip_y_end = clip.position_y + clip.height;
                                    clip.position_y < region.y_end && clip_y_end > region.y_start
                                });

                                if !any_overlap || region.plugins.is_empty() {
                                    continue;
                                }

                                // Process block-by-block through plugin chain
                                let mut offset = 0;
                                while offset < frames {
                                    let block_len = (frames - offset).min(effect_block_size);
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
                                        for clip in clips_ref.iter() {
                                            let clip_y_end = clip.position_y + clip.height;
                                            if clip.position_y >= region.y_end
                                                || clip_y_end <= region.y_start
                                            {
                                                continue;
                                            }
                                            let clip_t = t - clip.start_time_secs;
                                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                                let source_idx = ((clip_t + clip.buffer_offset_secs)
                                                    * clip.source_sample_rate as f64)
                                                    as usize;
                                                if source_idx < clip.buffer.len() {
                                                    let fg = clip_fade_gain(
                                                        clip_t,
                                                        clip.duration_secs,
                                                        clip.fade_in_secs,
                                                        clip.fade_out_secs,
                                                        clip.fade_in_curve,
                                                        clip.fade_out_curve,
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
                                        dst_l[..block_len].copy_from_slice(&src_l[..block_len]);
                                        dst_r[..block_len].copy_from_slice(&src_r[..block_len]);
                                        if let Ok(guard) = plugin_mutex.try_lock() {
                                            if let Some(ref gui) = *guard {
                                                let inputs: [&[f32]; 2] =
                                                    [&src_l[..block_len], &src_r[..block_len]];
                                                let mut outputs: [&mut [f32]; 2] = [
                                                    &mut dst_l[..block_len],
                                                    &mut dst_r[..block_len],
                                                ];
                                                gui.process(
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
                                        for clip in clips_ref.iter() {
                                            let clip_y_end = clip.position_y + clip.height;
                                            if clip.position_y >= region.y_end
                                                || clip_y_end <= region.y_start
                                            {
                                                continue;
                                            }
                                            let clip_t = t - clip.start_time_secs;
                                            if clip_t >= 0.0 && clip_t < clip.duration_secs {
                                                let source_idx = ((clip_t + clip.buffer_offset_secs)
                                                    * clip.source_sample_rate as f64)
                                                    as usize;
                                                if source_idx < clip.buffer.len() {
                                                    let fg = clip_fade_gain(
                                                        clip_t,
                                                        clip.duration_secs,
                                                        clip.fade_in_secs,
                                                        clip.fade_out_secs,
                                                        clip.fade_in_curve,
                                                        clip.fade_out_curve,
                                                    );
                                                    overlap_dry +=
                                                        clip.buffer[source_idx] * fg * clip.volume;
                                                }
                                            }
                                        }
                                        let mono = (dry_mix[offset + j][0] + dry_mix[offset + j][1]) * 0.5;
                                        let new_mono = mono - overlap_dry + wet;
                                        dry_mix[offset + j] = [new_mono, new_mono];
                                    }

                                    offset += block_len;
                                }
                            }
                        }
                    }

                    } // end if is_playing (clips + effects)

                    // Metronome click synthesis
                    if is_playing && met_en.load(Ordering::Relaxed) {
                        let bpm = load_f64(&met_bpm);
                        if bpm > 0.0 {
                            let beat_dur = 60.0 / bpm;
                            for i in 0..frames {
                                let t = current_time + i as f64 / sr;
                                let beat_idx = (t / beat_dur).floor() as i64;
                                if beat_idx < met_last_beat {
                                    met_last_beat = beat_idx - 1;
                                }
                                if beat_idx > met_last_beat {
                                    met_last_beat = beat_idx;
                                    // Only click if we're very close to the beat boundary;
                                    // suppresses the first click after play/seek mid-beat
                                    let beat_start = beat_idx as f64 * beat_dur;
                                    if t - beat_start < 0.001 {
                                        let is_downbeat = beat_idx.rem_euclid(4) == 0;
                                        met_freq = if is_downbeat { 1000.0 } else { 800.0 };
                                        let dur_secs = if is_downbeat { 0.020 } else { 0.015 };
                                        met_click_total = (dur_secs * sr) as u32;
                                        met_samples_left = met_click_total;
                                        met_phase = 0.0;
                                    }
                                }
                                if met_samples_left > 0 {
                                    let progress = 1.0 - (met_samples_left as f32 / met_click_total as f32);
                                    let envelope = (-progress * 5.0_f32).exp();
                                    let sine = (met_phase * std::f64::consts::TAU).sin() as f32;
                                    let click = sine * envelope * 0.5;
                                    dry_mix[i][0] += click;
                                    dry_mix[i][1] += click;
                                    met_phase += met_freq / sr;
                                    met_samples_left -= 1;
                                }
                            }
                        }
                    }

                    // Input monitoring: mix live mic input into output
                    // Handles sample rate conversion (input may differ from output)
                    if mon_en.load(Ordering::Relaxed) {
                        let in_ch = mon_in_ch.load(Ordering::Relaxed).max(1);
                        let in_sr = mon_in_sr.load(Ordering::Relaxed).max(1) as f64;
                        let out_sr = sr;

                        // How many input samples we need to produce `frames` output frames
                        let ratio = in_sr / out_sr;
                        let in_frames_needed = ((frames as f64) * ratio).ceil() as usize + 1;
                        let in_samples_needed = in_frames_needed * in_ch;
                        let pop_len = in_samples_needed.min(mon_raw.len());
                        let got = mon_ring_c.pop(&mut mon_raw[..pop_len]);
                        let in_frames_got = got / in_ch;

                        // Deinterleave input into L/R
                        for j in 0..in_frames_got {
                            if in_ch >= 2 {
                                mon_fx_l[j] = mon_raw[j * in_ch];
                                mon_fx_r[j] = mon_raw[j * in_ch + 1];
                            } else {
                                mon_fx_l[j] = mon_raw[j];
                                mon_fx_r[j] = mon_raw[j];
                            }
                        }

                        // Resample from input rate to output rate via linear interpolation
                        // and write into dry_mix
                        if in_frames_got > 1 {
                            for i in 0..frames {
                                let src_pos = i as f64 * ratio;
                                let idx = src_pos as usize;
                                if idx + 1 >= in_frames_got { break; }
                                let frac = src_pos - idx as f64;
                                let frac = frac as f32;
                                let l = mon_fx_l[idx] + (mon_fx_l[idx + 1] - mon_fx_l[idx]) * frac;
                                let r = mon_fx_r[idx] + (mon_fx_r[idx + 1] - mon_fx_r[idx]) * frac;
                                dry_mix[i][0] += l;
                                dry_mix[i][1] += r;
                            }
                        }
                    }

                    // Mix browser preview clip into dry_mix
                    if preview_active {
                        if let Ok(mut preview_guard) = preview_c.try_lock() {
                            if let Some(ref mut pv) = *preview_guard {
                                if pv.playing {
                                    let ratio = pv.source_sample_rate as f64 / sr;
                                    let total_samples = pv.left.len();
                                    for i in 0..frames {
                                        let idx = pv.position as usize;
                                        if idx >= total_samples {
                                            pv.playing = false;
                                            preview_p.store(false, Ordering::Relaxed);
                                            break;
                                        }
                                        let next = (idx + 1).min(total_samples - 1);
                                        let frac = (pv.position - idx as f64) as f32;
                                        let l = pv.left[idx] + (pv.left[next] - pv.left[idx]) * frac;
                                        let r = if !pv.right.is_empty() {
                                            let ri = idx.min(pv.right.len() - 1);
                                            let rn = (ri + 1).min(pv.right.len() - 1);
                                            pv.right[ri] + (pv.right[rn] - pv.right[ri]) * frac
                                        } else {
                                            l
                                        };
                                        dry_mix[i][0] += l;
                                        dry_mix[i][1] += r;
                                        pv.position += ratio;
                                    }
                                    let norm = if total_samples > 0 {
                                        pv.position / total_samples as f64
                                    } else {
                                        1.0
                                    };
                                    store_f64(&preview_pos, norm.min(1.0));
                                }
                            }
                        }
                    }

                    // --- Master bus pass: apply master volume + pan ---
                    let master_pan_val = load_f64(&m_pan) as f32;
                    let m_l_mul = gain * (2.0 * (1.0 - master_pan_val)).min(1.0);
                    let m_r_mul = gain * (2.0 * master_pan_val).min(1.0);

                    // Process master bus effect chain (if any plugins)
                    if let Ok(plugins) = m_plugins.try_lock() {
                        if !plugins.is_empty() {
                            let block = frames;
                            let mut src_l = vec![0.0f32; block];
                            let mut src_r = vec![0.0f32; block];
                            let mut dst_l = vec![0.0f32; block];
                            let mut dst_r = vec![0.0f32; block];
                            for i in 0..block {
                                src_l[i] = dry_mix[i][0];
                                src_r[i] = dry_mix[i][1];
                            }
                            for plug_arc in plugins.iter() {
                                dst_l[..block].copy_from_slice(&src_l[..block]);
                                dst_r[..block].copy_from_slice(&src_r[..block]);
                                if let Ok(guard) = plug_arc.try_lock() {
                                    if let Some(ref gui) = *guard {
                                        let inputs: [&[f32]; 2] = [&src_l[..block], &src_r[..block]];
                                        let mut outputs: [&mut [f32]; 2] = [&mut dst_l[..block], &mut dst_r[..block]];
                                        gui.process(&inputs, &mut outputs, block);
                                    }
                                }
                                std::mem::swap(&mut src_l, &mut dst_l);
                                std::mem::swap(&mut src_r, &mut dst_r);
                            }
                            for i in 0..block {
                                dry_mix[i][0] = src_l[i];
                                dry_mix[i][1] = src_r[i];
                            }
                        }
                    }

                    // Write final output (stereo)
                    for i in 0..frames {
                        let base = i * channels;
                        if channels >= 2 {
                            let l = (dry_mix[i][0] * m_l_mul).clamp(-1.0, 1.0);
                            let r = (dry_mix[i][1] * m_r_mul).clamp(-1.0, 1.0);
                            data[base] = l;
                            data[base + 1] = r;
                            let mono = (l + r) * 0.5;
                            sum_sq += (mono as f64) * (mono as f64);
                            for ch in 2..channels {
                                data[base + ch] = mono;
                            }
                        } else {
                            let mono = ((dry_mix[i][0] + dry_mix[i][1]) * 0.5 * m_l_mul).clamp(-1.0, 1.0);
                            data[base] = mono;
                            sum_sq += (mono as f64) * (mono as f64);
                        }
                    }

                    if frames > 0 {
                        let rms_val = (sum_sq / frames as f64).sqrt();
                        store_f64(&rms, rms_val);

                        // Per-clip RMS
                        for (ci, clip) in clips_ref.iter().enumerate() {
                            if clip_sum_sq[ci] > 0.0 {
                                entity_rms_local.insert(clip.entity_id, (clip_sum_sq[ci] / frames as f64).sqrt());
                            }
                        }
                        // Write per-entity RMS to shared map
                        if let Ok(mut m) = per_ent_rms.try_lock() {
                            std::mem::swap(&mut *m, &mut entity_rms_local);
                        }
                    }

                    if is_playing {
                        let mut new_time = current_time + frames as f64 / sr;
                        if lp_en.load(Ordering::Relaxed) {
                            let ls = load_f64(&lp_s);
                            let le = load_f64(&lp_e);
                            if le > ls && current_time >= ls && current_time < le && new_time >= le {
                                let len = le - ls;
                                new_time = ls + (new_time - le).rem_euclid(len);
                            }
                        }
                        store_f64(&pos, new_time);
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
            .ok()?;

        stream.play().ok()?;

        Some(Self {
            _stream: stream,
            device_name: actual_device_name,
            sample_rate,
            playing,
            position_bits,
            clips,
            effect_regions,
            instrument_regions,
            group_buses,
            chain_buses,
            master_volume,
            master_pan,
            master_bus_plugins,
            rms_peak,
            per_entity_rms,
            loop_enabled,
            loop_start_bits,
            loop_end_bits,
            metronome_enabled,
            bpm_bits,
            keyboard_preview,
            monitoring_enabled,
            monitor_ring,
            monitor_effect_plugins,
            monitor_input_channels,
            monitor_input_sample_rate,
            preview_clip,
            preview_playing,
            preview_position_bits,
        })
    }

    pub fn set_keyboard_preview_target(&self, id: Option<EntityId>) {
        if let Ok(mut g) = self.keyboard_preview.lock() {
            g.target = id;
        }
    }

    pub fn keyboard_preview_note_on(&self, target: EntityId, note: u8, velocity: u8) {
        if let Ok(mut g) = self.keyboard_preview.lock() {
            g.pending.push_back(KeyboardPreviewEvent::NoteOn {
                target,
                note,
                velocity,
            });
        }
    }

    pub fn keyboard_preview_note_off(&self, target: EntityId, note: u8) {
        if let Ok(mut g) = self.keyboard_preview.lock() {
            g.pending
                .push_back(KeyboardPreviewEvent::NoteOff { target, note });
        }
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

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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

    pub fn play_preview(&self, left: Arc<Vec<f32>>, right: Arc<Vec<f32>>, sample_rate: u32) {
        if let Ok(mut g) = self.preview_clip.lock() {
            *g = Some(PreviewClip {
                left,
                right,
                source_sample_rate: sample_rate,
                position: 0.0,
                playing: true,
            });
        }
        self.preview_playing.store(true, Ordering::Relaxed);
        store_f64(&self.preview_position_bits, 0.0);
    }

    pub fn stop_preview(&self) {
        if let Ok(mut g) = self.preview_clip.lock() {
            if let Some(ref mut clip) = *g {
                clip.playing = false;
            }
        }
        self.preview_playing.store(false, Ordering::Relaxed);
    }

    pub fn is_preview_playing(&self) -> bool {
        self.preview_playing.load(Ordering::Relaxed)
    }

    pub fn preview_position(&self) -> f64 {
        load_f64(&self.preview_position_bits)
    }

    pub fn seek_preview(&self, normalized: f64) {
        if let Ok(mut g) = self.preview_clip.lock() {
            if let Some(ref mut clip) = *g {
                let total = clip.left.len() as f64;
                clip.position = (normalized * total).clamp(0.0, total);
                clip.playing = true;
            }
        }
        self.preview_playing.store(true, Ordering::Relaxed);
        store_f64(&self.preview_position_bits, normalized.clamp(0.0, 1.0));
    }

    pub fn update_clips(
        &self,
        waveform_positions: &[[f32; 2]],
        waveform_sizes: &[[f32; 2]],
        audio_clips: &[AudioClipData],
        fade_ins_px: &[f32],
        fade_outs_px: &[f32],
        fade_in_curves: &[f32],
        fade_out_curves: &[f32],
        volumes: &[f32],
        pans: &[f32],
        sample_offsets_px: &[f32],
        volume_automations: &[Vec<(f32, f32)>],
        pan_automations: &[Vec<(f32, f32)>],
        warp_modes: &[u8],
        sample_bpms: &[f32],
        project_bpm: f32,
        pitch_semitones: &[f32],
        chain_plugins_per_clip: &[Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>],
        chain_latencies: &[u32],
        group_bus_indices: &[Option<usize>],
        chain_bus_indices: &[Option<usize>],
        entity_ids: &[EntityId],
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
            let fi_curve = fade_in_curves.get(i).copied().unwrap_or(0.0);
            let fo_curve = fade_out_curves.get(i).copied().unwrap_or(0.0);
            let vol = volumes.get(i).copied().unwrap_or(1.0);
            let pan = pans.get(i).copied().unwrap_or(0.5);
            let offset_px = sample_offsets_px.get(i).copied().unwrap_or(0.0);
            let offset_secs = offset_px as f64 / PIXELS_PER_SECOND as f64;
            let visible_duration = size[0] as f64 / PIXELS_PER_SECOND as f64;
            let vol_auto = volume_automations.get(i).cloned().unwrap_or_default();
            let pan_auto = pan_automations.get(i).cloned().unwrap_or_default();
            let warp = warp_modes.get(i).copied().unwrap_or(0);
            let sample_bpm = sample_bpms.get(i).copied().unwrap_or(120.0);
            let pitch = pitch_semitones.get(i).copied().unwrap_or(0.0);
            let effective_rate = match warp {
                1 => clip_data.sample_rate as f64 * (sample_bpm as f64 / project_bpm as f64),
                2 => clip_data.sample_rate as f64 * 2.0_f64.powf(pitch as f64 / 12.0),
                _ => clip_data.sample_rate as f64,
            };
            clips.push(PlaybackClip {
                entity_id: entity_ids.get(i).copied().unwrap_or(EntityId::nil()),
                buffer: clip_data.samples.clone(),
                source_sample_rate: clip_data.sample_rate,
                effective_sample_rate: effective_rate,
                start_time_secs: start_secs,
                duration_secs: visible_duration,
                position_y: pos[1],
                height: size[1],
                fade_in_secs: (fi / PIXELS_PER_SECOND) as f64,
                fade_out_secs: (fo / PIXELS_PER_SECOND) as f64,
                fade_in_curve: fi_curve,
                fade_out_curve: fo_curve,
                volume: vol,
                pan,
                buffer_offset_secs: offset_secs,
                volume_automation: vol_auto,
                pan_automation: pan_auto,
                chain_plugins: chain_plugins_per_clip.get(i).cloned().unwrap_or_default(),
                chain_latency_samples: chain_latencies.get(i).copied().unwrap_or(0),
                group_bus_index: group_bus_indices.get(i).copied().flatten(),
                chain_bus_index: chain_bus_indices.get(i).copied().flatten(),
            });
        }
    }

    pub fn update_group_buses(&self, buses: Vec<GroupBus>) {
        if let Ok(mut guard) = self.group_buses.lock() {
            *guard = buses;
        }
    }

    pub fn update_chain_buses(&self, buses: Vec<ChainBus>) {
        if let Ok(mut guard) = self.chain_buses.lock() {
            *guard = buses;
        }
    }

    pub fn update_master_bus(
        &self,
        plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
        volume: f32,
        pan: f32,
    ) {
        self.set_master_volume(volume);
        store_f64(&self.master_pan, pan as f64);
        if let Ok(mut guard) = self.master_bus_plugins.lock() {
            *guard = plugins;
        }
    }

    pub fn update_effect_regions(&self, regions: Vec<AudioEffectRegion>) {
        if let Ok(mut guard) = self.effect_regions.lock() {
            *guard = regions;
        }
    }

    pub fn update_instruments(&self, regions: Vec<AudioInstrument>) {
        if let Ok(mut guard) = self.instrument_regions.lock() {
            *guard = regions;
        }
    }

    pub fn set_loop_region(&self, start_secs: f64, end_secs: f64) {
        store_f64(&self.loop_start_bits, start_secs);
        store_f64(&self.loop_end_bits, end_secs);
    }

    pub fn set_loop_enabled(&self, enabled: bool) {
        self.loop_enabled.store(enabled, Ordering::Relaxed);
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

    pub fn entity_rms(&self, id: EntityId) -> f32 {
        self.per_entity_rms
            .lock()
            .ok()
            .and_then(|m| m.get(&id).copied())
            .unwrap_or(0.0) as f32
    }

    pub fn set_metronome_enabled(&self, enabled: bool) {
        self.metronome_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_bpm(&self, bpm: f32) {
        store_f64(&self.bpm_bits, bpm as f64);
    }

    pub fn monitor_ring(&self) -> Arc<MonitorRingBuffer> {
        self.monitor_ring.clone()
    }

    pub fn monitoring_enabled_flag(&self) -> Arc<AtomicBool> {
        self.monitoring_enabled.clone()
    }

    pub fn monitor_input_channels_flag(&self) -> Arc<AtomicUsize> {
        self.monitor_input_channels.clone()
    }

    pub fn monitor_input_sample_rate_flag(&self) -> Arc<AtomicU64> {
        self.monitor_input_sample_rate.clone()
    }

    pub fn set_monitoring_enabled(&self, enabled: bool) {
        self.monitoring_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn update_monitor_effects(
        &self,
        plugins: Vec<Arc<Mutex<Option<crate::effects::PluginGuiHandle>>>>,
    ) {
        if let Ok(mut guard) = self.monitor_effect_plugins.lock() {
            *guard = plugins;
        }
    }
}

pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: usize,
    recording: Arc<AtomicBool>,
    monitoring: Arc<AtomicBool>,
    monitor_ring: Option<Arc<MonitorRingBuffer>>,
    monitor_input_channels: Option<Arc<AtomicUsize>>,
    monitor_input_sample_rate: Option<Arc<AtomicU64>>,
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
            monitoring: Arc::new(AtomicBool::new(false)),
            monitor_ring: None,
            monitor_input_channels: None,
            monitor_input_sample_rate: None,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    pub fn set_monitor_ring(&mut self, ring: Arc<MonitorRingBuffer>, flag: Arc<AtomicBool>, input_channels: Arc<AtomicUsize>, input_sample_rate: Arc<AtomicU64>) {
        self.monitor_ring = Some(ring);
        self.monitoring = flag;
        self.monitor_input_channels = Some(input_channels);
        self.monitor_input_sample_rate = Some(input_sample_rate);
    }

    /// Ensure the CPAL input stream is running (needed for recording or monitoring).
    fn ensure_stream(&mut self) -> bool {
        if self.stream.is_some() {
            return true;
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

        // Update engine's knowledge of input channel count and sample rate
        if let Some(ref ch_flag) = self.monitor_input_channels {
            ch_flag.store(self.channels, Ordering::Relaxed);
        }
        if let Some(ref sr_flag) = self.monitor_input_sample_rate {
            sr_flag.store(self.sample_rate as u64, Ordering::Relaxed);
        }

        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        self.buffer = buf.clone();
        let rec = self.recording.clone();
        let mon = self.monitoring.clone();
        let mon_ring = self.monitor_ring.clone();

        let stream = match device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Push to recording buffer when recording
                if rec.load(Ordering::Relaxed) {
                    if let Ok(mut guard) = buf.try_lock() {
                        guard.extend_from_slice(data);
                    }
                }
                // Push to monitoring ring buffer when monitoring
                if mon.load(Ordering::Relaxed) {
                    if let Some(ref ring) = mon_ring {
                        ring.push(data);
                    }
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
        true
    }

    /// Drop the input stream if neither recording nor monitoring need it.
    fn maybe_drop_stream(&mut self) {
        if !self.is_recording() && !self.monitoring.load(Ordering::Relaxed) {
            self.stream = None;
        }
    }

    pub fn set_monitoring(&mut self, enabled: bool) {
        println!("  set_monitoring({}) ring={} flag={}", enabled, self.monitor_ring.is_some(), self.monitoring.load(Ordering::Relaxed));
        if enabled {
            let ok = self.ensure_stream();
            println!("  ensure_stream -> {} stream={}", ok, self.stream.is_some());
            println!("  INPUT: {} Hz, {} ch", self.sample_rate, self.channels);
        } else {
            self.maybe_drop_stream();
        }
    }

    pub fn start(&mut self) -> bool {
        if self.is_recording() {
            return false;
        }

        if !self.ensure_stream() {
            return false;
        }

        // Reset the recording buffer
        if let Ok(mut guard) = self.buffer.lock() {
            guard.clear();
        }

        self.recording.store(true, Ordering::Relaxed);
        println!(
            "  Recording started ({} ch, {} Hz)",
            self.channels, self.sample_rate
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
        self.maybe_drop_stream();

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

/// Quickly probe an audio file's duration from its header metadata
/// without decoding the full stream. Returns `(duration_secs, width_px)`.
pub fn probe_audio_duration(path: &Path) -> Option<(f32, f32)> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &Default::default(), &Default::default())
        .ok()?;
    let track = probed.format.default_track()?;
    let sample_rate = track.codec_params.sample_rate? as f64;
    let n_frames = track.codec_params.n_frames? as f64;
    let dur = (n_frames / sample_rate) as f32;
    Some((dur, dur * PIXELS_PER_SECOND))
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

pub fn load_audio_from_bytes(file_bytes: &[u8], extension: &str) -> Option<LoadedAudio> {
    let cursor = std::io::Cursor::new(file_bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    hint.with_extension(extension);

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

/// Encode stereo PCM samples as WAV bytes in memory.
pub fn encode_wav_bytes(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = left.len().min(right.len());
    let num_channels: u16 = 2;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = (num_samples * num_channels as usize * (bits_per_sample as usize / 8)) as u32;

    let mut buf = Vec::with_capacity(44 + data_size as usize);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for i in 0..num_samples {
        let l = (left[i].clamp(-1.0, 1.0) * 32767.0) as i16;
        let r = (right[i].clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&l.to_le_bytes());
        buf.extend_from_slice(&r.to_le_bytes());
    }
    buf
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
    pub fade_in_curve: f32,
    pub fade_out_curve: f32,
    pub volume: f32,
    pub buffer_offset_secs: f64,
    pub warp_mode: u8,
    pub sample_bpm: f32,
    pub project_bpm: f32,
    pub pitch_semitones: f32,
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
                let effective_rate = match clip.warp_mode {
                    1 => clip.source_sample_rate as f64 * (clip.sample_bpm as f64 / clip.project_bpm as f64),
                    2 => clip.source_sample_rate as f64 * 2.0_f64.powf(clip.pitch_semitones as f64 / 12.0),
                    _ => clip.source_sample_rate as f64,
                };
                let source_idx = ((clip_t + clip.buffer_offset_secs) * effective_rate) as usize;
                if source_idx < clip.buffer.len() {
                    let fg = clip_fade_gain(
                        clip_t,
                        clip.duration_secs,
                        clip.fade_in_secs,
                        clip.fade_out_secs,
                        clip.fade_in_curve,
                        clip.fade_out_curve,
                    );
                    mix += clip.buffer[source_idx] * fg * clip.volume;
                }
            }
        }
        dry_mix[i] = mix;
    }

    let effect_block_size = DEFAULT_EFFECT_BLOCK_SIZE;
    let mut fx_buf_l = vec![0.0f32; effect_block_size];
    let mut fx_buf_r = vec![0.0f32; effect_block_size];
    let mut fx_out_l = vec![0.0f32; effect_block_size];
    let mut fx_out_r = vec![0.0f32; effect_block_size];

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
            let block_len = (total_frames - offset).min(effect_block_size);
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
                        let source_idx = ((clip_t + clip.buffer_offset_secs) * clip.source_sample_rate as f64) as usize;
                        if source_idx < clip.buffer.len() {
                            let fg = clip_fade_gain(
                                clip_t,
                                clip.duration_secs,
                                clip.fade_in_secs,
                                clip.fade_out_secs,
                                clip.fade_in_curve,
                                clip.fade_out_curve,
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
                dst_l[..block_len].copy_from_slice(&src_l[..block_len]);
                dst_r[..block_len].copy_from_slice(&src_r[..block_len]);
                if let Ok(guard) = plugin_mutex.try_lock() {
                    if let Some(ref gui) = *guard {
                        let inputs: Vec<&[f32]> = vec![&src_l[..block_len], &src_r[..block_len]];
                        let mut outputs: Vec<&mut [f32]> =
                            vec![&mut dst_l[..block_len], &mut dst_r[..block_len]];
                        gui.process(&inputs, &mut outputs, block_len);
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
                        let source_idx = ((clip_t + clip.buffer_offset_secs) * clip.source_sample_rate as f64) as usize;
                        if source_idx < clip.buffer.len() {
                            let fg = clip_fade_gain(
                                clip_t,
                                clip.duration_secs,
                                clip.fade_in_secs,
                                clip.fade_out_secs,
                                clip.fade_in_curve,
                                clip.fade_out_curve,
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
