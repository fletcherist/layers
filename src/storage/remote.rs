use std::sync::Arc;

use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::types::Bytes;
use surrealdb::Surreal;

use super::helpers::{peaks_f32_to_u8, peaks_u8_to_f32};
use super::local::StoredAudioData;
use super::models::*;
use super::{ProjectStore, run_on_rt};

pub struct RemoteStorage {
    db: Surreal<Client>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl RemoteStorage {
    pub fn connect(url: &str, password: Option<&str>, rt: Arc<tokio::runtime::Runtime>) -> Option<Self> {
        let addr = url
            .trim_start_matches("ws://")
            .trim_start_matches("wss://");
        println!("[RemoteStorage] Connecting to SurrealDB at {addr}...");
        let db = run_on_rt(&rt, {
            let addr = addr.to_string();
            let password = password.map(|s| s.to_string());
            async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    Surreal::new::<Ws>(&addr),
                ).await;
                let db = match result {
                    Ok(Ok(db)) => db,
                    Ok(Err(e)) => {
                        eprintln!("[RemoteStorage] Connection error: {e}");
                        return None;
                    }
                    Err(_) => {
                        eprintln!("[RemoteStorage] Connection timed out after 5s");
                        return None;
                    }
                };
                if let Some(ref pass) = password {
                    use surrealdb::opt::auth::Root;
                    if let Err(e) = db.signin(Root { username: "root".to_string(), password: pass.clone() }).await {
                        eprintln!("[RemoteStorage] Authentication failed: {e}");
                        return None;
                    }
                    println!("[RemoteStorage] Authenticated as root");
                }
                db.use_ns("layers").use_db("meta").await.ok()?;
                Some(db)
            }
        })?;
        println!("[RemoteStorage] Connected to SurrealDB at {url}");
        Some(RemoteStorage { db, rt })
    }

    pub fn use_project(&self, project_id: &str) {
        let db_name = format!("project_{project_id}");
        let db = self.db.clone();
        let result = run_on_rt(&self.rt, async move {
            db.use_ns("layers").use_db(&db_name).await
        });
        if let Err(e) = result {
            log::error!("[RemoteStorage] Failed to switch to project DB: {e}");
        } else {
            log::info!("[RemoteStorage] Using project DB: project_{project_id}");
        }
    }
}

// ---------------------------------------------------------------------------
// ProjectStore implementation
// ---------------------------------------------------------------------------

impl ProjectStore for RemoteStorage {
    fn save_project_state(&self, state: ProjectState) {
        let db = self.db.clone();
        let result = run_on_rt(&self.rt, async move {
            let _: Option<ProjectState> = db.upsert(("state", "main")).content(state).await?;
            Ok::<(), surrealdb::Error>(())
        });
        if let Err(e) = result {
            log::error!("[RemoteStorage] Failed to save project state: {e}");
        }
    }

    fn load_project_state(&self) -> Option<ProjectState> {
        let db = self.db.clone();
        run_on_rt(&self.rt, async move {
            let state: Option<ProjectState> = db.select(("state", "main")).await.ok()?;
            state
        })
    }

    fn save_audio(&self, waveform_id: &str, file_bytes: &[u8], extension: &str) {
        let byte_len = file_bytes.len();
        let mb = byte_len as f64 / (1024.0 * 1024.0);
        println!(
            "[RemoteStorage] save_audio: wf={waveform_id} ext={extension} size={mb:.2} MB"
        );
        let data = StoredAudioData {
            waveform_id: waveform_id.to_string(),
            file_data: Bytes::from(file_bytes.to_vec()),
            extension: extension.to_string(),
        };
        let db = self.db.clone();
        let wf_id = waveform_id.to_string();
        let result: Result<(), surrealdb::Error> = run_on_rt(&self.rt, async move {
            let _: Option<StoredAudioData> =
                db.upsert(("audio", &*wf_id)).content(data).await?;
            Ok(())
        });
        match &result {
            Ok(()) => println!("[RemoteStorage] save_audio OK for {waveform_id}"),
            Err(e) => eprintln!("[RemoteStorage] save_audio FAILED for {waveform_id}: {e}"),
        }
    }

    fn load_audio(&self, waveform_id: &str) -> Option<(Vec<u8>, String)> {
        println!("[RemoteStorage] load_audio: wf={waveform_id}");
        let db = self.db.clone();
        let wf_id = waveform_id.to_string();
        let result = run_on_rt(&self.rt, async move {
            let data: Option<StoredAudioData> =
                db.select(("audio", &*wf_id)).await.ok()?;
            data
        });
        let found = result.is_some();
        println!("[RemoteStorage] load_audio: wf={waveform_id} found={found}");
        result.map(|r| (r.file_data.into_inner().to_vec(), r.extension))
    }

    fn save_peaks(&self, waveform_id: &str, block_size: u64, left: &[f32], right: &[f32]) {
        let db = self.db.clone();
        let data = StoredPeaks {
            waveform_id: waveform_id.to_string(),
            block_size,
            left_peaks: peaks_f32_to_u8(left),
            right_peaks: peaks_f32_to_u8(right),
        };
        let wf_id = waveform_id.to_string();
        let result = run_on_rt(&self.rt, async move {
            let _: Option<StoredPeaks> =
                db.upsert(("peaks", &*wf_id)).content(data).await?;
            Ok::<(), surrealdb::Error>(())
        });
        if let Err(e) = result {
            log::error!("[RemoteStorage] Failed to save peaks for {waveform_id}: {e}");
        }
    }

    fn load_peaks(&self, waveform_id: &str) -> Option<(u64, Vec<f32>, Vec<f32>)> {
        let db = self.db.clone();
        let wf_id = waveform_id.to_string();
        let stored = run_on_rt(&self.rt, async move {
            let data: Option<StoredPeaks> =
                db.select(("peaks", &*wf_id)).await.ok()?;
            data
        })?;
        Some((
            stored.block_size,
            peaks_u8_to_f32(&stored.left_peaks),
            peaks_u8_to_f32(&stored.right_peaks),
        ))
    }

    fn clear_audio_and_peaks(&self) {
        let db = self.db.clone();
        let _ = run_on_rt(&self.rt, async move {
            let _: Vec<StoredAudioData> = db.delete("audio").await?;
            let _: Vec<StoredPeaks> = db.delete("peaks").await?;
            Ok::<(), surrealdb::Error>(())
        });
    }
}
