//! Core phase vocoder with Identity Phase Locking (IPL).
//!
//! This module implements a single STFT frame processor. It does not manage
//! buffering or overlap-add — that is handled by [`super::TimeStretcher`].
//!
//! # Key concepts
//!
//! - **Analysis hop** (`hop_a`): the step size when reading input frames.
//!   Fixed at `fft_size / 4` (75% overlap).
//! - **Synthesis hop** (`hop_s`): the step size when writing output frames.
//!   `hop_s = hop_a × stretch_ratio`. Passed per-frame to [`PhaseVocoder::process_frame`].
//! - **Phase propagation**: each bin's synthesis phase is advanced by the
//!   "true instantaneous frequency" scaled by the hop ratio:
//!   `φ_s[k] += (hop_s / hop_a) × (ω_k + princarg(Δφ - ω_k))`
//! - **IPL**: non-peak bins inherit their phase offset from the nearest peak.
//! - **Transient reset**: when the [`TransientDetector`] fires, synthesis
//!   phases are copied directly from analysis phases.

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;
use std::f32::consts::PI;
use std::sync::Arc;

use super::transients::TransientDetector;

const TWO_PI: f32 = 2.0 * PI;

/// Wrap angle into `[-PI, PI]`.
///
/// Also called "principal argument". Normalizes a phase angle so that
/// phase differences can be compared correctly across the `±PI` boundary.
#[inline]
fn princarg(phase: f32) -> f32 {
    phase - TWO_PI * (phase / TWO_PI).round()
}

/// Generate a Hann window of the given size.
///
/// The Hann window `w[n] = 0.5 × (1 - cos(2π n / (N-1)))` is used for both
/// analysis and synthesis. When squared (analysis × synthesis) at 75% overlap,
/// the windows sum to a constant ~1.5, which is compensated during output
/// normalization in [`super::TimeStretcher::pull_output`].
fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (TWO_PI * i as f32 / (size - 1) as f32).cos()))
        .collect()
}

/// Find spectral peaks: bins whose magnitude exceeds all neighbors within `radius`.
///
/// A bin `k` is a peak if `|X[k]| >= |X[j]|` for all `j` in `[k-radius, k+radius]`.
/// DC (bin 0) and Nyquist (last bin) are excluded. Bins with magnitude below
/// `1e-10` are never considered peaks (noise floor gate).
///
/// These peaks form the "anchor points" for Identity Phase Locking — each
/// peak represents a detected partial/harmonic, and surrounding bins are
/// phase-locked to their nearest peak.
fn find_peaks(magnitudes: &[f32], radius: usize) -> Vec<usize> {
    let len = magnitudes.len();
    let mut peaks = Vec::new();
    for i in 1..len.saturating_sub(1) {
        let lo = if i >= radius { i - radius } else { 0 };
        let hi = (i + radius + 1).min(len);
        let is_peak = (lo..hi).all(|j| j == i || magnitudes[i] >= magnitudes[j]);
        if is_peak && magnitudes[i] > 1e-10 {
            peaks.push(i);
        }
    }
    peaks
}

/// For each frequency bin, find the index of the nearest spectral peak.
///
/// Uses a single-pass sweep with a moving pointer into the sorted `peaks`
/// array. Each bin is assigned to whichever peak is closer; ties go to the
/// lower-index peak.
///
/// The returned vector maps `bin_index → peak_index`. For peak bins
/// themselves, `assignment[k] == k`.
fn assign_to_nearest_peak(num_bins: usize, peaks: &[usize]) -> Vec<usize> {
    let mut assignment = vec![0usize; num_bins];
    if peaks.is_empty() {
        return assignment;
    }
    let mut pi = 0usize;
    for bin in 0..num_bins {
        // Advance peak pointer if next peak is closer
        while pi + 1 < peaks.len() {
            let dist_cur = (bin as isize - peaks[pi] as isize).unsigned_abs();
            let dist_next = (bin as isize - peaks[pi + 1] as isize).unsigned_abs();
            if dist_next < dist_cur {
                pi += 1;
            } else {
                break;
            }
        }
        assignment[bin] = peaks[pi];
    }
    assignment
}

/// Single-channel phase vocoder with Identity Phase Locking (IPL).
///
/// Stateful processor: maintains synthesis phase and previous analysis phase
/// across frames. Must be fed consecutive, overlapping analysis frames from
/// the same audio stream. Call [`reset`](Self::reset) when seeking.
///
/// # Frame processing
///
/// Each call to [`process_frame`](Self::process_frame) performs:
///
/// 1. **Analysis window** (Hann) applied to the input.
/// 2. **Forward FFT** → complex spectrum of `fft_size/2 + 1` bins.
/// 3. **Magnitude/phase extraction** from complex bins.
/// 4. **Transient check** via [`TransientDetector`].
/// 5. **Phase computation**:
///    - First frame or transient → copy analysis phases directly.
///    - Otherwise → propagate + IPL lock (see module docs).
/// 6. **Spectrum reconstruction** from magnitudes + synthesis phases.
/// 7. **Inverse FFT** → time domain, normalized by `1/N`.
/// 8. **Synthesis window** (Hann) applied to the output.
///
/// The caller is responsible for overlap-add of the returned frames.
pub struct PhaseVocoder {
    pub fft_size: usize,
    pub hop_a: usize,
    pub spectrum_len: usize,

    fft_forward: Arc<dyn realfft::RealToComplex<f32>>,
    fft_inverse: Arc<dyn realfft::ComplexToReal<f32>>,
    window: Vec<f32>,

    prev_analysis_phase: Vec<f32>,
    synthesis_phase: Vec<f32>,

    transient_detector: TransientDetector,

    // Reusable scratch buffers
    scratch_fwd: Vec<Complex<f32>>,
    scratch_inv: Vec<Complex<f32>>,
    spectrum: Vec<Complex<f32>>,
    time_buf: Vec<f32>,

    has_prev_frame: bool,
}

impl PhaseVocoder {
    pub fn new(fft_size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(fft_size);
        let fft_inverse = planner.plan_fft_inverse(fft_size);
        let spectrum_len = fft_size / 2 + 1;

        let scratch_fwd = fft_forward.make_scratch_vec();
        let scratch_inv = fft_inverse.make_scratch_vec();
        let spectrum = fft_forward.make_output_vec();
        let time_buf = fft_inverse.make_output_vec();

        Self {
            fft_size,
            hop_a: fft_size / 4,
            spectrum_len,
            fft_forward,
            fft_inverse,
            window: hann_window(fft_size),
            prev_analysis_phase: vec![0.0; spectrum_len],
            synthesis_phase: vec![0.0; spectrum_len],
            transient_detector: TransientDetector::new(spectrum_len),
            scratch_fwd,
            scratch_inv,
            spectrum,
            time_buf,
            has_prev_frame: false,
        }
    }

    /// Process a single frame of `fft_size` input samples.
    ///
    /// `hop_s` is the synthesis hop size (determines stretch ratio together with `hop_a`).
    /// Returns synthesized time-domain frame of `fft_size` samples (windowed, ready for overlap-add).
    pub fn process_frame(&mut self, input_frame: &[f32], hop_s: usize) -> &[f32] {
        debug_assert_eq!(input_frame.len(), self.fft_size);

        // Apply analysis window
        let mut windowed: Vec<f32> = input_frame
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| s * w)
            .collect();

        // Forward FFT
        self.fft_forward
            .process_with_scratch(&mut windowed, &mut self.spectrum, &mut self.scratch_fwd)
            .unwrap();

        // Extract magnitudes and phases
        let mut magnitudes = Vec::with_capacity(self.spectrum_len);
        let mut analysis_phases = Vec::with_capacity(self.spectrum_len);
        for bin in &self.spectrum {
            magnitudes.push((bin.re * bin.re + bin.im * bin.im).sqrt());
            analysis_phases.push(bin.im.atan2(bin.re));
        }

        let is_transient = self.transient_detector.is_transient(&magnitudes);

        if !self.has_prev_frame {
            // First frame: copy analysis phases directly
            self.synthesis_phase.copy_from_slice(&analysis_phases);
            self.has_prev_frame = true;
        } else if is_transient {
            // Transient: reset to analysis phases (preserve attack clarity)
            self.synthesis_phase.copy_from_slice(&analysis_phases);
        } else {
            // --- Phase propagation + IPL ---
            let hop_a = self.hop_a as f32;
            let hop_s_f = hop_s as f32;
            let fft_size_f = self.fft_size as f32;

            // Step 1: standard phase vocoder propagation for all bins
            let mut propagated = vec![0.0f32; self.spectrum_len];
            for k in 0..self.spectrum_len {
                let omega_k = TWO_PI * hop_a * k as f32 / fft_size_f;
                let dp = princarg(analysis_phases[k] - self.prev_analysis_phase[k] - omega_k);
                let true_freq = omega_k + dp;
                propagated[k] = self.synthesis_phase[k] + (hop_s_f / hop_a) * true_freq;
            }

            // Step 2: identity phase locking
            let peaks = find_peaks(&magnitudes, 2);
            if !peaks.is_empty() {
                let assignments = assign_to_nearest_peak(self.spectrum_len, &peaks);
                for k in 0..self.spectrum_len {
                    let p = assignments[k];
                    if p == k {
                        // Peak bin: use propagated phase
                        self.synthesis_phase[k] = propagated[k];
                    } else {
                        // Non-peak: lock to nearest peak
                        self.synthesis_phase[k] = propagated[p]
                            + (analysis_phases[k] - analysis_phases[p]);
                    }
                }
            } else {
                self.synthesis_phase.copy_from_slice(&propagated);
            }
        }

        self.prev_analysis_phase.copy_from_slice(&analysis_phases);

        // Reconstruct spectrum from magnitudes + synthesis phases
        for (i, bin) in self.spectrum.iter_mut().enumerate() {
            let mag = magnitudes[i];
            let phase = self.synthesis_phase[i];
            if i == 0 || i == self.spectrum_len - 1 {
                // DC and Nyquist: keep real
                bin.re = mag * phase.cos();
                bin.im = 0.0;
            } else {
                bin.re = mag * phase.cos();
                bin.im = mag * phase.sin();
            }
        }

        // Inverse FFT
        self.fft_inverse
            .process_with_scratch(&mut self.spectrum, &mut self.time_buf, &mut self.scratch_inv)
            .unwrap();

        // Normalize IFFT (realfft scales by N)
        let norm = 1.0 / self.fft_size as f32;
        for s in self.time_buf.iter_mut() {
            *s *= norm;
        }

        // Apply synthesis window
        for (s, w) in self.time_buf.iter_mut().zip(self.window.iter()) {
            *s *= w;
        }

        &self.time_buf
    }

    pub fn reset(&mut self) {
        self.prev_analysis_phase.fill(0.0);
        self.synthesis_phase.fill(0.0);
        self.transient_detector.reset();
        self.has_prev_frame = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_princarg() {
        assert!((princarg(0.0)).abs() < 1e-6);
        // PI wraps to either PI or -PI; both are valid
        assert!((princarg(PI)).abs() - PI < 1e-5);
        assert!((princarg(-PI)).abs() - PI < 1e-5);
        // 3*PI wraps to ~PI (or -PI)
        assert!((princarg(3.0 * PI)).abs() - PI < 1e-4);
        assert!((princarg(-3.0 * PI)).abs() - PI < 1e-4);
        // 2*PI wraps to ~0
        assert!((princarg(2.0 * PI)).abs() < 1e-4);
    }

    #[test]
    fn test_find_peaks() {
        let mags = vec![0.0, 0.5, 1.0, 0.5, 0.0, 0.3, 0.8, 0.3, 0.0];
        let peaks = find_peaks(&mags, 2);
        assert!(peaks.contains(&2));
        assert!(peaks.contains(&6));
    }

    #[test]
    fn test_assign_to_nearest_peak() {
        let peaks = vec![2, 6];
        let assignments = assign_to_nearest_peak(9, &peaks);
        assert_eq!(assignments[0], 2);
        assert_eq!(assignments[1], 2);
        assert_eq!(assignments[2], 2);
        assert_eq!(assignments[3], 2);
        // Bin 4 is equidistant (dist 2 from both) — algorithm keeps first peak
        assert_eq!(assignments[4], 2);
        assert_eq!(assignments[5], 6);
        assert_eq!(assignments[6], 6);
        assert_eq!(assignments[7], 6);
        assert_eq!(assignments[8], 6);
    }

    #[test]
    fn test_process_frame_no_panic() {
        let fft_size = 1024;
        let mut pv = PhaseVocoder::new(fft_size);
        let input = vec![0.0f32; fft_size];
        let _ = pv.process_frame(&input, fft_size / 4);
        let _ = pv.process_frame(&input, fft_size / 4);
    }

    #[test]
    fn test_sine_wave_unity_stretch() {
        // Stretch ratio 1.0 (hop_s == hop_a): output should approximate input
        let fft_size = 1024;
        let hop = fft_size / 4;
        let sr = 44100.0f32;
        let freq = 440.0f32;

        // Generate a sine wave
        let num_samples = fft_size * 4;
        let sine: Vec<f32> = (0..num_samples)
            .map(|i| (TWO_PI * freq * i as f32 / sr).sin())
            .collect();

        let mut pv = PhaseVocoder::new(fft_size);
        let mut output = vec![0.0f32; num_samples + fft_size];

        let mut in_pos = 0usize;
        let mut out_pos = 0usize;
        while in_pos + fft_size <= sine.len() {
            let frame = &sine[in_pos..in_pos + fft_size];
            let result = pv.process_frame(frame, hop);
            let end = (out_pos + fft_size).min(output.len());
            for i in out_pos..end {
                output[i] += result[i - out_pos];
            }
            in_pos += hop;
            out_pos += hop;
        }

        // Check output is not silence
        let energy: f32 = output.iter().map(|s| s * s).sum();
        assert!(energy > 0.1, "output should not be silent, energy = {}", energy);
    }
}
