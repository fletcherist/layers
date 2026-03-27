pub fn f32_slice_to_u8(data: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(data).to_vec()
}

pub fn u8_slice_to_f32(data: &[u8]) -> Vec<f32> {
    if data.len() % 4 != 0 {
        return Vec::new();
    }
    data.chunks_exact(4)
        .map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

// ---------------------------------------------------------------------------
// Peak quantization helpers (used by both local and remote storage)
// ---------------------------------------------------------------------------

pub(crate) fn peaks_f32_to_u8(peaks: &[f32]) -> Vec<u8> {
    peaks.iter().map(|&p| (p.clamp(0.0, 1.0) * 255.0) as u8).collect()
}

pub(crate) fn peaks_u8_to_f32(peaks: &[u8]) -> Vec<f32> {
    peaks.iter().map(|&b| b as f32 / 255.0).collect()
}
