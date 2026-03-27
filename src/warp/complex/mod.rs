//! Phase-vocoder-based time stretching with Identity Phase Locking.
//!
//! # Algorithm
//!
//! The algorithm is a phase vocoder (STFT → modify → ISTFT) enhanced with two
//! techniques that dramatically improve audio quality:
//!
//! ## 1. Identity Phase Locking (IPL) — Laroche & Dolson, 1999
//!
//! A naive phase vocoder propagates each frequency bin's phase independently.
//! This destroys the vertical phase coherence between harmonics of the same
//! source, producing a characteristic "phasiness" or metallic smearing.
//!
//! IPL fixes this by:
//! 1. Detecting **spectral peaks** (bins louder than their neighbors).
//! 2. Propagating phase normally only for peak bins.
//! 3. For every non-peak bin, **locking** its synthesis phase to the nearest
//!    peak: `φ_synth[k] = φ_synth[peak] + (φ_analysis[k] - φ_analysis[peak])`.
//!
//! This preserves the phase relationships within each partial's "region of
//! influence", eliminating phasiness while keeping frequency resolution.
//!
//! ## 2. Transient Detection with Phase Reset
//!
//! Phase vocoders smear transients (drum hits, plucks) across multiple
//! overlapping windows, making attacks sound soft and washy. The transient
//! detector monitors **spectral flux** (the sum of positive magnitude changes
//! between consecutive frames). When flux spikes above a running median, the
//! frame is classified as a transient and the synthesis phases are **reset** to
//! the analysis phases, bypassing propagation entirely. This preserves the
//! temporal sharpness of attacks.
//!
//! # Signal flow
//!
//! ```text
//! Input ──► Hann window ──► FFT ──► Extract |mag|, ∠phase
//!                                       │
//!                              ┌────────┴────────┐
//!                              │  Transient?      │
//!                              │  (spectral flux) │
//!                              └────────┬────────┘
//!                                 yes / │ \ no
//!                                 /     │    \
//!                     Reset phases     Phase propagation
//!                     to analysis      + IPL peak-locking
//!                                 \     │    /
//!                                  \    │   /
//!                              ┌────────┴────────┐
//!                              │ Reconstruct      │
//!                              │ spectrum from     │
//!                              │ |mag| + φ_synth   │
//!                              └────────┬────────┘
//!                                       │
//!                           IFFT ──► Hann window ──► Overlap-Add ──► Output
//! ```
//!
//! # Parameters
//!
//! | Parameter         | Default | Description                                  |
//! |-------------------|---------|----------------------------------------------|
//! | FFT size          | 4096    | Window size in samples. Larger = better       |
//! |                   |         | frequency resolution but more time smearing.  |
//! | Overlap           | 75%     | `hop = FFT_size / 4`. Standard for phase      |
//! |                   |         | vocoders; Hann² at 75% sums to ~1.5.         |
//! | Peak radius       | 2       | A bin is a peak if it exceeds all neighbors   |
//! |                   |         | within ±2 bins.                               |
//! | Transient thresh  | 2.0×    | Flux must exceed the running median by 2×.    |
//!
//! # Usage
//!
//! ```ignore
//! use crate::warp::complex::TimeStretcher;
//!
//! let mut stretcher = TimeStretcher::with_default_size();
//! stretcher.push_input(&audio_samples);
//!
//! let mut output = vec![0.0f32; desired_length];
//! let written = stretcher.pull_output(&mut output, 2.0); // 2× slower
//! ```
//!
//! # References
//!
//! - Laroche, J. & Dolson, M. (1999). "Improved Phase Vocoder Time-Scale
//!   Modification of Audio." *IEEE Trans. Speech and Audio Processing*, 7(3).
//! - Driedger, J. & Müller, M. (2014). "TSM Toolbox: MATLAB Implementations
//!   of Time-Scale Modification Algorithms." *Proc. DAFx-14*.

pub mod phase_vocoder;
pub mod transients;

use std::sync::Arc;

use phase_vocoder::PhaseVocoder;

const DEFAULT_FFT_SIZE: usize = 4096;

/// Single-channel time stretcher built on a phase vocoder with IPL.
///
/// Manages the input/output buffering and overlap-add synthesis around
/// the [`PhaseVocoder`]. Analysis frames are read at `hop_a` spacing from
/// the input buffer, processed, and written at `hop_s = hop_a × ratio`
/// spacing into the output buffer using overlap-add.
///
/// # Buffer management
///
/// Internally maintains two linear buffers:
/// - **Input buffer**: samples pushed via [`push_input`](Self::push_input)
///   are appended here. Consumed input is periodically compacted.
/// - **Output buffer**: synthesis frames are overlap-added here. Finalized
///   samples (those before `output_write_pos` that won't receive more OLA
///   contributions) are read out via [`pull_output`](Self::pull_output).
///
/// Both buffers are compacted when the read position exceeds `8 × fft_size`
/// to prevent unbounded memory growth during long playback.
pub struct TimeStretcher {
    vocoder: PhaseVocoder,
    fft_size: usize,
    hop_a: usize,

    input_buffer: Vec<f32>,
    input_pos: usize,

    output_buffer: Vec<f32>,
    output_write_pos: usize, // next synthesis frame starts here
    output_read_pos: usize,  // caller has consumed up to here
}

impl TimeStretcher {
    pub fn new(fft_size: usize) -> Self {
        Self {
            vocoder: PhaseVocoder::new(fft_size),
            fft_size,
            hop_a: fft_size / 4,
            input_buffer: Vec::new(),
            input_pos: 0,
            output_buffer: Vec::new(),
            output_write_pos: 0,
            output_read_pos: 0,
        }
    }

    pub fn with_default_size() -> Self {
        Self::new(DEFAULT_FFT_SIZE)
    }

    /// Append input samples for processing.
    pub fn push_input(&mut self, samples: &[f32]) {
        self.input_buffer.extend_from_slice(samples);
    }

    /// Process available input and pull stretched output.
    ///
    /// `ratio` > 1.0 means slower (output longer than input).
    /// `ratio` < 1.0 means faster (output shorter than input).
    ///
    /// Returns the number of samples actually written to `output`.
    pub fn pull_output(&mut self, output: &mut [f32], ratio: f64) -> usize {
        let hop_s = ((self.hop_a as f64) * ratio).round().max(1.0) as usize;

        // Process frames until we have enough output or exhaust input
        while self.available_output() < output.len() {
            if self.input_pos + self.fft_size > self.input_buffer.len() {
                break; // need more input
            }

            let frame = &self.input_buffer[self.input_pos..self.input_pos + self.fft_size];
            let result = self.vocoder.process_frame(frame, hop_s);

            // Ensure output buffer is large enough for overlap-add
            let needed = self.output_write_pos + self.fft_size;
            if self.output_buffer.len() < needed {
                self.output_buffer.resize(needed, 0.0);
            }

            // Overlap-add synthesis frame
            for (i, &s) in result.iter().enumerate() {
                self.output_buffer[self.output_write_pos + i] += s;
            }

            self.input_pos += self.hop_a;
            self.output_write_pos += hop_s;
        }

        // Copy finalized output to caller
        // Samples in [read_pos, write_pos) are finalized (no more OLA contributions)
        let available = self.available_output().min(output.len());

        // Hann^2 at 75% overlap sums to ~1.5; normalize
        let norm = 1.0 / 1.5;
        for i in 0..available {
            output[i] = self.output_buffer[self.output_read_pos + i] * norm;
        }
        self.output_read_pos += available;

        // Compact buffers periodically to prevent unbounded growth
        self.compact_buffers();

        available
    }

    /// Reset all state (call on seek).
    pub fn reset(&mut self) {
        self.vocoder.reset();
        self.input_buffer.clear();
        self.input_pos = 0;
        self.output_buffer.clear();
        self.output_write_pos = 0;
        self.output_read_pos = 0;
    }

    /// Number of input samples needed before the next frame can be processed.
    pub fn input_needed(&self) -> usize {
        let available = self.input_buffer.len().saturating_sub(self.input_pos);
        if available >= self.fft_size {
            0
        } else {
            self.fft_size - available
        }
    }

    /// Number of output samples ready to read.
    fn available_output(&self) -> usize {
        self.output_write_pos.saturating_sub(self.output_read_pos)
    }

    fn compact_buffers(&mut self) {
        let threshold = self.fft_size * 8;
        if self.output_read_pos > threshold {
            self.output_buffer.drain(..self.output_read_pos);
            self.output_write_pos -= self.output_read_pos;
            self.output_read_pos = 0;
        }
        if self.input_pos > threshold {
            self.input_buffer.drain(..self.input_pos);
            self.input_pos = 0;
        }
    }
}

/// Stereo time stretcher for the audio engine.
///
/// Wraps two independent [`TimeStretcher`] instances (one per channel) and
/// owns references to the source audio. Automatically feeds source samples
/// to the internal stretchers as output is requested.
///
/// # Lifecycle in the audio engine
///
/// 1. **Created** in `update_clips()` on the main thread when a clip's warp
///    mode is set to Complex. Receives `Arc<Vec<f32>>` references to the
///    clip's stereo audio data plus the initial stretch ratio.
/// 2. **Salvaged** across `update_clips()` calls: when the clip list is
///    rebuilt (e.g. on tempo change), the stretcher is extracted from the
///    old `PlaybackClip`, its ratio is updated via [`set_ratio`](Self::set_ratio),
///    and it is re-attached to the new clip. This preserves the phase
///    vocoder's internal state for seamless playback.
/// 3. **Used** in the audio callback via [`process`](Self::process), which
///    feeds source audio and pulls stretched output.
/// 4. **Reset** on seek via [`reset`](Self::reset), which clears all internal
///    state and repositions the source read cursor.
pub struct StereoTimeStretcher {
    left: TimeStretcher,
    right: TimeStretcher,

    source_l: Arc<Vec<f32>>,
    source_r: Arc<Vec<f32>>,
    source_position: usize,
    ratio: f64,
    fft_size: usize,
    hop_a: usize,
}

impl StereoTimeStretcher {
    pub fn new(source_l: Arc<Vec<f32>>, source_r: Arc<Vec<f32>>, ratio: f64) -> Self {
        let fft_size = DEFAULT_FFT_SIZE;
        Self {
            left: TimeStretcher::new(fft_size),
            right: TimeStretcher::new(fft_size),
            source_l,
            source_r,
            source_position: 0,
            ratio,
            fft_size,
            hop_a: fft_size / 4,
        }
    }

    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio;
    }

    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Produce stretched stereo output. Feeds source audio automatically.
    /// Returns the number of frames actually produced.
    pub fn process(&mut self, out_l: &mut [f32], out_r: &mut [f32], frames: usize) -> usize {
        let frames = frames.min(out_l.len()).min(out_r.len());

        // Feed source audio until both channels have enough input
        self.feed_source(frames);

        let n_l = self.left.pull_output(&mut out_l[..frames], self.ratio);
        let n_r = self.right.pull_output(&mut out_r[..frames], self.ratio);
        n_l.min(n_r)
    }

    /// Feed enough source audio to produce approximately `needed_output` samples.
    fn feed_source(&mut self, needed_output: usize) {
        let src_len = self.source_l.len().min(self.source_r.len());
        // Estimate input needed: output = input * ratio, so input = output / ratio
        // Add extra for FFT window overlap
        let input_estimate = (needed_output as f64 / self.ratio).ceil() as usize + self.fft_size * 2;

        let target = (self.source_position + input_estimate).min(src_len);
        if self.source_position < target {
            let chunk_l = &self.source_l[self.source_position..target];
            let chunk_r = &self.source_r[self.source_position..target];
            self.left.push_input(chunk_l);
            self.right.push_input(chunk_r);
            self.source_position = target;
        }
    }

    /// Reset the stretcher and pre-fill the overlap-add pipeline.
    ///
    /// Starts reading source audio from `prefill_len` samples BEFORE
    /// `source_position`, processes those frames, and discards the output.
    /// This ensures the overlap-add sum is at full amplitude when real
    /// output begins — without this, the first ~fft_size output samples
    /// would ramp up from silence (Hann window startup), cutting off attacks.
    pub fn reset(&mut self, source_position: usize) {
        self.left.reset();
        self.right.reset();

        let src_len = self.source_l.len().min(self.source_r.len());
        // Pre-fill with fft_size samples before the target position
        // so the overlap-add has enough frames to reach full amplitude
        let prefill_len = self.fft_size;
        let prefill_start = source_position.saturating_sub(prefill_len);
        self.source_position = prefill_start;

        if prefill_start < src_len {
            let end = source_position.min(src_len);
            if end > prefill_start {
                let chunk_l = &self.source_l[prefill_start..end];
                let chunk_r = &self.source_r[prefill_start..end];
                self.left.push_input(chunk_l);
                self.right.push_input(chunk_r);
                self.source_position = end;

                // Process the pre-fill frames and discard output
                let discard_len = ((end - prefill_start) as f64 * self.ratio) as usize + self.fft_size;
                let mut discard = vec![0.0f32; discard_len];
                self.left.pull_output(&mut discard, self.ratio);
                self.right.pull_output(&mut discard, self.ratio);
            }
        }
    }

    pub fn source_position(&self) -> usize {
        self.source_position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn generate_sine(freq: f32, sample_rate: f32, num_samples: usize) -> Vec<f32> {
        (0..num_samples)
            .map(|i| (2.0 * PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    #[test]
    fn test_time_stretcher_2x() {
        let sr = 44100.0;
        let freq = 440.0;
        let duration_samples = 44100; // 1 second
        let sine = generate_sine(freq, sr, duration_samples);

        let mut stretcher = TimeStretcher::new(2048);
        stretcher.push_input(&sine);

        let ratio = 2.0;
        let expected_len = (duration_samples as f64 * ratio) as usize;
        let mut output = vec![0.0f32; expected_len + 8192];
        let written = stretcher.pull_output(&mut output, ratio);

        // Should produce roughly 2x the input length (minus some edge effects)
        assert!(
            written > (expected_len as f64 * 0.7) as usize,
            "expected ~{} samples, got {}",
            expected_len,
            written
        );

        // Output should not be silence
        let energy: f32 = output[..written].iter().map(|s| s * s).sum();
        assert!(energy > 1.0, "output is too quiet, energy = {}", energy);
    }

    #[test]
    fn test_time_stretcher_half() {
        let sr = 44100.0;
        let freq = 440.0;
        let duration_samples = 44100;
        let sine = generate_sine(freq, sr, duration_samples);

        let mut stretcher = TimeStretcher::new(2048);
        stretcher.push_input(&sine);

        let ratio = 0.5;
        let expected_len = (duration_samples as f64 * ratio) as usize;
        let mut output = vec![0.0f32; expected_len + 8192];
        let written = stretcher.pull_output(&mut output, ratio);

        assert!(
            written > (expected_len as f64 * 0.7) as usize,
            "expected ~{} samples, got {}",
            expected_len,
            written
        );
    }

    #[test]
    fn test_stereo_stretcher() {
        let sr = 44100.0;
        let n = 44100;
        let left = generate_sine(440.0, sr, n);
        let right = generate_sine(880.0, sr, n);

        let mut stretcher = StereoTimeStretcher::new(
            Arc::new(left),
            Arc::new(right),
            1.5,
        );

        let frames = 44100;
        let mut out_l = vec![0.0f32; frames];
        let mut out_r = vec![0.0f32; frames];
        let written = stretcher.process(&mut out_l, &mut out_r, frames);

        assert!(written > 0, "should produce output");

        // L and R should be different (different input frequencies)
        let diff: f32 = out_l[..written]
            .iter()
            .zip(out_r[..written].iter())
            .map(|(l, r)| (l - r).abs())
            .sum::<f32>()
            / written as f32;
        assert!(diff > 0.001, "channels should differ, avg diff = {}", diff);
    }

    #[test]
    fn test_pitch_preservation() {
        // Stretch a sine wave at 2x, measure zero-crossing rate
        // If pitch is preserved, zero-crossing rate should be similar
        let sr = 44100.0;
        let freq = 440.0;
        let n = 44100;
        let sine = generate_sine(freq, sr, n);

        let mut stretcher = TimeStretcher::new(2048);
        stretcher.push_input(&sine);

        let mut output = vec![0.0f32; n * 3];
        let written = stretcher.pull_output(&mut output, 2.0);

        // Count zero crossings in original
        let orig_zc = count_zero_crossings(&sine[1024..n - 1024]);
        // Count zero crossings in output (skip edges)
        let skip = 2048;
        let out_zc = count_zero_crossings(&output[skip..written.saturating_sub(skip)]);

        // Zero-crossing rate (per sample) should be similar
        let orig_rate = orig_zc as f64 / (n - 2048) as f64;
        let out_rate = out_zc as f64 / (written - 2 * skip) as f64;

        let ratio = out_rate / orig_rate;
        assert!(
            ratio > 0.7 && ratio < 1.4,
            "pitch should be preserved: orig_rate={}, out_rate={}, ratio={}",
            orig_rate,
            out_rate,
            ratio
        );
    }

    fn count_zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count()
    }

    #[test]
    fn test_reset() {
        let mut stretcher = TimeStretcher::new(1024);
        stretcher.push_input(&vec![1.0; 4096]);
        let mut out = vec![0.0; 1024];
        stretcher.pull_output(&mut out, 1.0);
        stretcher.reset();
        assert_eq!(stretcher.input_needed(), 1024);
        assert_eq!(stretcher.available_output(), 0);
    }

    /// Simulate the real audio callback: small blocks (256 frames), called repeatedly.
    /// The StereoTimeStretcher must produce non-zero output after startup.
    #[test]
    fn test_stereo_stretcher_small_blocks() {
        let sr = 44100.0;
        let n = 44100 * 2; // 2 seconds of audio
        let left = generate_sine(440.0, sr, n);
        let right = generate_sine(440.0, sr, n);

        let mut stretcher = StereoTimeStretcher::new(
            Arc::new(left),
            Arc::new(right),
            1.0, // unity ratio
        );

        let block_size = 256; // typical audio callback size
        let mut out_l = vec![0.0f32; block_size];
        let mut out_r = vec![0.0f32; block_size];
        let mut total_energy = 0.0f64;
        let mut total_produced = 0usize;

        // Simulate 100 callbacks (~0.58 seconds)
        for cb in 0..100 {
            out_l.fill(0.0);
            out_r.fill(0.0);
            let produced = stretcher.process(&mut out_l, &mut out_r, block_size);
            total_produced += produced;
            let mut block_energy = 0.0f64;
            for i in 0..produced {
                block_energy += (out_l[i] as f64) * (out_l[i] as f64);
                block_energy += (out_r[i] as f64) * (out_r[i] as f64);
            }
            total_energy += block_energy;

            // After warmup (~fft_size samples = ~16 callbacks at 256),
            // every block should have non-zero energy
            if cb > 20 {
                assert!(
                    block_energy > 0.01,
                    "callback {} should have energy, got {} (produced={})",
                    cb, block_energy, produced
                );
            }
        }

        assert!(
            total_produced > 20000,
            "should produce substantial output, got {} samples",
            total_produced
        );
        assert!(
            total_energy > 100.0,
            "output should have significant energy, got {}",
            total_energy
        );
    }

    /// Test that feed_source doesn't starve on subsequent calls.
    #[test]
    fn test_feed_source_continuous() {
        let sr = 44100.0;
        let n = 44100 * 4;
        let sine = generate_sine(440.0, sr, n);

        let mut stretcher = StereoTimeStretcher::new(
            Arc::new(sine.clone()),
            Arc::new(sine),
            1.0,
        );

        let block = 512;
        let mut out_l = vec![0.0f32; block];
        let mut out_r = vec![0.0f32; block];

        // Run many iterations — source should not run out for 4 seconds of audio
        let mut zero_blocks = 0;
        for _ in 0..300 {
            out_l.fill(0.0);
            out_r.fill(0.0);
            let produced = stretcher.process(&mut out_l, &mut out_r, block);
            if produced == 0 {
                zero_blocks += 1;
            }
        }
        // Allow a few zero blocks at the very start or end, but not many
        assert!(
            zero_blocks < 5,
            "too many empty blocks: {} out of 300",
            zero_blocks
        );
    }
}
