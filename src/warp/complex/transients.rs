//! Spectral-flux-based transient (onset) detection.
//!
//! Transients — drum hits, plucks, consonants — are the Achilles' heel of
//! phase vocoders. Because the STFT spreads each transient across multiple
//! overlapping analysis windows, the resynthesized attack gets smeared in
//! time, sounding soft and "washy".
//!
//! The fix: detect transient frames and **reset the synthesis phases** to the
//! original analysis phases, bypassing phase propagation for that frame. This
//! preserves the temporal sharpness of the attack at the cost of a brief
//! phase discontinuity (which is masked by the transient energy itself).
//!
//! # Detection method
//!
//! **Spectral flux** measures how much the spectrum changed between two frames:
//!
//! ```text
//! flux[n] = Σ_k max(0, |X[n][k]| - |X[n-1][k]|)
//! ```
//!
//! Only *positive* changes are summed (half-wave rectification), because we
//! care about energy appearing, not disappearing. The flux is compared against
//! a **rolling median** of recent flux values. If the current flux exceeds the
//! median by a threshold factor (default 2×), it is classified as a transient.
//!
//! The rolling median (rather than mean) makes the detector robust to gradual
//! spectral evolution (e.g. a chord change), which raises the mean but not the
//! median by as much.

use std::collections::VecDeque;

/// Spectral-flux-based transient detector.
///
/// Maintains a rolling window of recent flux values and fires when the
/// current flux exceeds the running median by [`threshold`](Self::threshold)×.
///
/// # Edge cases
///
/// - **First frame**: no previous magnitudes to compare against → never fires.
/// - **Fewer than 3 frames of history**: insufficient data for median → never fires.
/// - **Near-zero baseline** (median < 1e-6): uses an absolute flux threshold
///   of 0.1 instead of the relative test, to avoid false positives from
///   numerical noise in silence and to correctly detect the first onset after
///   silence.
pub struct TransientDetector {
    prev_magnitudes: Vec<f32>,
    flux_history: VecDeque<f32>,
    median_window: usize,
    threshold: f32,
    has_prev: bool,
}

impl TransientDetector {
    pub fn new(spectrum_len: usize) -> Self {
        Self {
            prev_magnitudes: vec![0.0; spectrum_len],
            flux_history: VecDeque::with_capacity(16),
            median_window: 8,
            threshold: 2.0,
            has_prev: false,
        }
    }

    /// Returns true if the current frame contains a transient onset.
    pub fn is_transient(&mut self, magnitudes: &[f32]) -> bool {
        if !self.has_prev {
            self.prev_magnitudes.copy_from_slice(magnitudes);
            self.has_prev = true;
            return false;
        }

        // Positive spectral flux: sum of magnitude increases only
        let flux: f32 = magnitudes
            .iter()
            .zip(self.prev_magnitudes.iter())
            .map(|(&cur, &prev)| (cur - prev).max(0.0))
            .sum();

        self.prev_magnitudes.copy_from_slice(magnitudes);

        // Maintain a rolling window for median computation
        if self.flux_history.len() >= self.median_window {
            self.flux_history.pop_front();
        }
        self.flux_history.push_back(flux);

        if self.flux_history.len() < 3 {
            return false;
        }

        let median = rolling_median(&self.flux_history);
        if median < 1e-6 {
            // Near-zero baseline: use absolute threshold
            flux > 0.1
        } else {
            flux > median * self.threshold
        }
    }

    pub fn reset(&mut self) {
        self.prev_magnitudes.fill(0.0);
        self.flux_history.clear();
        self.has_prev = false;
    }
}

fn rolling_median(values: &VecDeque<f32>) -> f32 {
    let mut sorted: Vec<f32> = values.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) * 0.5
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_no_transient() {
        let spectrum_len = 64;
        let mut det = TransientDetector::new(spectrum_len);
        let silent = vec![0.0f32; spectrum_len];
        for _ in 0..10 {
            assert!(!det.is_transient(&silent));
        }
    }

    #[test]
    fn test_impulse_detected() {
        let spectrum_len = 64;
        let mut det = TransientDetector::new(spectrum_len);
        // Use a small non-zero baseline so median is > 1e-6
        let quiet = vec![0.01f32; spectrum_len];
        let loud = vec![10.0f32; spectrum_len];

        // Feed several quiet frames to build up history with non-zero median
        for _ in 0..10 {
            det.is_transient(&quiet);
        }

        // Sudden loud frame should trigger transient
        assert!(det.is_transient(&loud));
    }

    #[test]
    fn test_steady_state_no_transient() {
        let spectrum_len = 64;
        let mut det = TransientDetector::new(spectrum_len);
        let steady = vec![5.0f32; spectrum_len];

        // After initial ramp-up, steady state should not trigger
        for _ in 0..20 {
            det.is_transient(&steady);
        }
        assert!(!det.is_transient(&steady));
    }

    #[test]
    fn test_reset() {
        let spectrum_len = 64;
        let mut det = TransientDetector::new(spectrum_len);
        let frame = vec![5.0f32; spectrum_len];
        det.is_transient(&frame);
        det.reset();
        assert!(!det.has_prev);
        assert!(det.flux_history.is_empty());
    }
}
