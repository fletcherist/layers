mod models;
mod conversions;
#[cfg(feature = "native")]
mod local;
#[cfg(feature = "native")]
mod remote;
mod helpers;

pub use models::*;
pub use conversions::*;
#[cfg(feature = "native")]
pub use local::{Storage, default_base_path};
pub(crate) use local::StoredAudioData;
#[cfg(feature = "native")]
pub use remote::RemoteStorage;
#[allow(unused_imports)] // used by tests
pub use helpers::{f32_slice_to_u8, u8_slice_to_f32};

// ---------------------------------------------------------------------------
// ProjectStore trait — unified interface for local and remote storage
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
pub trait ProjectStore {
    fn save_project_state(&self, state: ProjectState);
    fn load_project_state(&self) -> Option<ProjectState>;
    fn save_audio(&self, waveform_id: &str, file_bytes: &[u8], extension: &str);
    fn load_audio(&self, waveform_id: &str) -> Option<(Vec<u8>, String)>;
    fn save_peaks(&self, waveform_id: &str, block_size: u64, left: &[f32], right: &[f32]);
    fn load_peaks(&self, waveform_id: &str) -> Option<(u64, Vec<f32>, Vec<f32>)>;
    fn clear_audio_and_peaks(&self);
}

// ---------------------------------------------------------------------------
// Shared runtime helper — safe to call from any thread
// ---------------------------------------------------------------------------

#[cfg(feature = "native")]
pub(crate) fn run_on_rt<F, T>(rt: &tokio::runtime::Runtime, future: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    rt.spawn(async move {
        let result = future.await;
        let _ = tx.send(result);
    });
    rx.recv()
        .expect("run_on_rt: runtime shut down while task was in flight")
}
