use rand::Rng;
use realfft::RealFftPlanner;
use std::f32::consts::PI;

fn next_power_of_two(n: usize) -> usize {
    let mut v = n;
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (size - 1) as f32).cos()))
        .collect()
}

fn paulstretch_channel(
    samples: &[f32],
    sample_rate: u32,
    stretch_factor: f32,
    window_size_secs: f32,
    shared_phases: &[Vec<f32>],
) -> Vec<f32> {
    let window_size = next_power_of_two((sample_rate as f32 * window_size_secs) as usize).max(64);
    let hop_out = window_size / 2;
    // hop_out / hop_in = stretch_factor, so hop_in = hop_out / stretch_factor
    let hop_in = hop_out as f64 / stretch_factor as f64;

    let window = hann_window(window_size);

    let num_frames = (samples.len() as f64 / hop_in).ceil() as usize + 1;
    let output_len = num_frames * hop_out + window_size;
    let mut output = vec![0.0f32; output_len];

    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(window_size);
    let ifft = planner.plan_fft_inverse(window_size);

    let spectrum_len = window_size / 2 + 1;
    let mut spectrum = fft.make_output_vec();
    let mut scratch_fwd = fft.make_scratch_vec();
    let mut scratch_inv = ifft.make_scratch_vec();

    let mut input_pos: f64 = 0.0;
    let mut output_pos: usize = 0;
    let mut frame_idx: usize = 0;

    while (input_pos as usize) < samples.len() {
        let start = input_pos as usize;
        let mut frame = vec![0.0f32; window_size];
        let copy_len = (samples.len() - start).min(window_size);
        frame[..copy_len].copy_from_slice(&samples[start..start + copy_len]);

        // Apply analysis window
        for (s, w) in frame.iter_mut().zip(window.iter()) {
            *s *= w;
        }

        // Forward FFT
        fft.process_with_scratch(&mut frame, &mut spectrum, &mut scratch_fwd)
            .unwrap();

        // Extract magnitudes, apply shared random phases
        let phases = &shared_phases[frame_idx.min(shared_phases.len() - 1)];
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let mag = (bin.re * bin.re + bin.im * bin.im).sqrt();
            if i == 0 || i == spectrum_len - 1 {
                // DC and Nyquist bins must be real
                bin.re = mag;
                bin.im = 0.0;
            } else {
                let phase = phases[i];
                bin.re = mag * phase.cos();
                bin.im = mag * phase.sin();
            }
        }

        // Inverse FFT
        let mut time_buf = ifft.make_output_vec();
        ifft.process_with_scratch(&mut spectrum, &mut time_buf, &mut scratch_inv)
            .unwrap();

        // Normalize IFFT output (realfft scales by N)
        let norm = 1.0 / window_size as f32;
        for s in time_buf.iter_mut() {
            *s *= norm;
        }

        // Apply synthesis window and overlap-add
        let end = (output_pos + window_size).min(output.len());
        for i in output_pos..end {
            let j = i - output_pos;
            output[i] += time_buf[j] * window[j];
        }

        input_pos += hop_in;
        output_pos += hop_out;
        frame_idx += 1;

        if output_pos + window_size > output.len() {
            break;
        }
    }

    output
}

/// Process stereo audio with the Paul Stretch algorithm.
/// Uses the same random phases for both channels to preserve stereo image.
pub fn paulstretch_stereo(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    stretch_factor: f32,
    window_size_secs: f32,
) -> (Vec<f32>, Vec<f32>) {
    let window_size = next_power_of_two((sample_rate as f32 * window_size_secs) as usize).max(64);
    let hop_out = window_size / 2;
    let hop_in = hop_out as f64 / stretch_factor as f64;
    let spectrum_len = window_size / 2 + 1;
    let input_len = left.len().max(right.len());

    // Pre-generate shared random phases for all frames
    let num_frames = (input_len as f64 / hop_in).ceil() as usize + 2;
    let mut rng = rand::thread_rng();
    let shared_phases: Vec<Vec<f32>> = (0..num_frames)
        .map(|_| (0..spectrum_len).map(|_| rng.gen_range(0.0..2.0 * PI)).collect())
        .collect();

    let stretched_left = paulstretch_channel(left, sample_rate, stretch_factor, window_size_secs, &shared_phases);
    let stretched_right = paulstretch_channel(right, sample_rate, stretch_factor, window_size_secs, &shared_phases);

    // Ensure both channels are the same length
    let len = stretched_left.len().min(stretched_right.len());
    (stretched_left[..len].to_vec(), stretched_right[..len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paulstretch_output_length() {
        let sample_rate = 44100;
        let duration_secs = 1.0;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;

        // Generate a simple sine wave
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        let stretch_factor = 8.0;
        let (out_l, out_r) = paulstretch_stereo(
            &samples, &samples, sample_rate, stretch_factor, 0.25,
        );

        // Output should be approximately stretch_factor times longer
        let expected_min = (num_samples as f32 * stretch_factor * 0.8) as usize;
        let expected_max = (num_samples as f32 * stretch_factor * 1.3) as usize;
        assert!(
            out_l.len() >= expected_min && out_l.len() <= expected_max,
            "output len {} not in expected range [{}, {}]",
            out_l.len(), expected_min, expected_max,
        );
        assert_eq!(out_l.len(), out_r.len());
    }

    #[test]
    fn test_paulstretch_stereo_preserves_image() {
        let sample_rate = 44100;
        let num_samples = 44100; // 1 second

        let left: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / sample_rate as f32).sin())
            .collect();
        let right: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * PI * 880.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        let (out_l, out_r) = paulstretch_stereo(&left, &right, sample_rate, 4.0, 0.25);

        // Channels should be different (different input frequencies)
        let diff: f32 = out_l.iter().zip(out_r.iter())
            .map(|(l, r)| (l - r).abs())
            .sum::<f32>() / out_l.len() as f32;
        assert!(diff > 0.0001, "channels should differ: avg diff = {}", diff);
    }

    #[test]
    fn test_paulstretch_no_nans() {
        let sample_rate = 44100;
        let samples: Vec<f32> = (0..22050)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        let (out_l, out_r) = paulstretch_stereo(&samples, &samples, sample_rate, 8.0, 0.25);

        assert!(!out_l.iter().any(|s| s.is_nan()), "output contains NaN");
        assert!(!out_r.iter().any(|s| s.is_nan()), "output contains NaN");
    }
}
