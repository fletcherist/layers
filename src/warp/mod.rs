//! Audio time-stretching algorithms.
//!
//! This module provides pitch-preserving time-stretching: changing the duration
//! of an audio signal without altering its pitch. This is the core technology
//! behind the "Complex" warp mode in the DAW, analogous to Ableton Live's
//! Complex/Complex Pro or Logic Pro's Flex Time.
//!
//! # Available algorithms
//!
//! - [`complex`] — Phase vocoder with Identity Phase Locking (IPL).
//!   High-quality algorithm suitable for polyphonic material (full mixes,
//!   drums + melody, etc.). Preserves transients via spectral flux detection.
//!
//! # Architecture
//!
//! ```text
//! Source audio ──► TimeStretcher ──► Stretched audio
//!                    │
//!                    ├── PhaseVocoder (STFT + phase propagation + IPL)
//!                    └── TransientDetector (spectral flux onset detection)
//! ```
//!
//! The stretcher is designed for both real-time (audio callback) and offline use.
//! In real-time mode, feed samples incrementally via [`TimeStretcher::push_input`]
//! and pull stretched output via [`TimeStretcher::pull_output`]. The stretch ratio
//! can be changed at any time.

pub mod complex;

pub use complex::{StereoTimeStretcher, TimeStretcher};
