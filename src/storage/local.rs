use std::path::{Path, PathBuf};
use std::sync::Arc;

use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::types::{Bytes, SurrealValue};
use surrealdb::Surreal;

use super::models::*;
use super::{ProjectStore, run_on_rt};

// ---------------------------------------------------------------------------
// Audio stored as original encoded file (shared with remote.rs)
// ---------------------------------------------------------------------------

#[derive(Clone, SurrealValue)]
pub(crate) struct StoredAudioData {
    pub(crate) waveform_id: String,
    pub(crate) file_data: Bytes,
    pub(crate) extension: String,
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

pub struct Storage {
    rt: Arc<tokio::runtime::Runtime>,
    temp_projects_dir: PathBuf,
    index_db: Surreal<Db>,
    project_db: Option<Surreal<Db>>,
    current_project_path: Option<PathBuf>,
    is_temp: bool,
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn open_db(rt: &tokio::runtime::Runtime, path: &Path) -> Option<Surreal<Db>> {
    let path_str = path.to_str()?.to_string();
    run_on_rt(rt, async move {
        let db = Surreal::new::<RocksDb>(&path_str).await.ok()?;
        db.use_ns("layers").use_db("canvas").await.ok()?;
        Some(db)
    })
}

impl Storage {
    /// Opens the global index DB and prepares the temp projects directory.
    pub fn open(base_path: &Path) -> Option<Self> {
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(10 * 1024 * 1024)
                .build()
                .ok()?,
        );

        let temp_projects_dir = base_path.join("projects");
        std::fs::create_dir_all(&temp_projects_dir).ok()?;

        let index_db_path = base_path.join("index.db");
        let index_db = open_db(&rt, &index_db_path)?;

        log::info!("Storage opened at {:?}", base_path);
        Some(Storage {
            rt,
            temp_projects_dir,
            index_db,
            project_db: None,
            current_project_path: None,
            is_temp: false,
        })
    }

    // -----------------------------------------------------------------------
    // Project lifecycle (local-only)
    // -----------------------------------------------------------------------

    /// Opens a per-project DB inside `path/db/`.
    pub fn open_project(&mut self, path: &Path) -> bool {
        self.close_project();
        let db_path = path.join("db");
        std::fs::create_dir_all(&db_path).ok();
        match open_db(&self.rt, &db_path) {
            Some(db) => {
                self.project_db = Some(db);
                self.current_project_path = Some(path.to_path_buf());
                let key = path.to_string_lossy().to_string();
                let index_db = self.index_db.clone();
                self.is_temp = run_on_rt(&self.rt, async move {
                    let entry: Option<ProjectIndexEntry> =
                        index_db.select(("projects", &*key)).await.ok()?;
                    entry.map(|e| e.is_temp)
                })
                .unwrap_or(false);
                log::info!("Opened project DB at {:?} (temp={})", db_path, self.is_temp);
                true
            }
            None => {
                log::error!("Failed to open project DB at {:?}", db_path);
                false
            }
        }
    }

    /// Drops the current project DB.
    pub fn close_project(&mut self) {
        self.project_db = None;
        self.current_project_path = None;
        self.is_temp = false;
    }

    /// Creates a new temp project under `~/.layers/projects/tmp-<ts>/`.
    pub fn create_temp_project(&mut self) -> Option<PathBuf> {
        let ts = now_ts();
        let dir = self.temp_projects_dir.join(format!("tmp-{ts}"));
        std::fs::create_dir_all(&dir).ok()?;
        if !self.open_project(&dir) {
            return None;
        }
        self.is_temp = true;

        let entry = ProjectIndexEntry {
            name: "Untitled".to_string(),
            path: dir.to_string_lossy().to_string(),
            is_temp: true,
            created_at: ts,
            updated_at: ts,
        };
        let key = dir.to_string_lossy().to_string();
        self.upsert_index_entry(&key, entry);
        log::info!("Created temp project at {:?}", dir);
        Some(dir)
    }

    /// Moves/copies the current project folder to `dest`, reopens DB there.
    pub fn save_project_to(&mut self, dest: &Path) -> bool {
        let src = match &self.current_project_path {
            Some(p) => p.clone(),
            None => return false,
        };
        if src == dest {
            return true;
        }

        // Close current DB so RocksDB lock is released
        let was_temp = self.is_temp;
        self.project_db = None;

        // Create dest and copy contents
        if let Err(e) = copy_dir_all(&src, dest) {
            log::error!("Failed to copy project to {:?}: {e}", dest);
            // Try to reopen at old location
            self.open_project(&src);
            self.is_temp = was_temp;
            return false;
        }

        // Remove old temp dir
        if was_temp {
            let _ = std::fs::remove_dir_all(&src);
            self.delete_index_entry(&src.to_string_lossy());
        }

        // Reopen at new location
        if !self.open_project(dest) {
            log::error!("Failed to reopen project at {:?}", dest);
            return false;
        }
        self.is_temp = false;

        let entry = ProjectIndexEntry {
            name: String::new(),
            path: dest.to_string_lossy().to_string(),
            is_temp: false,
            created_at: now_ts(),
            updated_at: now_ts(),
        };
        let key = dest.to_string_lossy().to_string();
        self.upsert_index_entry(&key, entry);
        true
    }

    pub fn is_temp_project(&self) -> bool {
        self.is_temp
    }

    pub fn current_project_path(&self) -> Option<&Path> {
        self.current_project_path.as_deref()
    }

    // -----------------------------------------------------------------------
    // project.json (local-only)
    // -----------------------------------------------------------------------

    pub fn write_project_json(&self, name: &str) {
        let path = match &self.current_project_path {
            Some(p) => p.join("project.json"),
            None => return,
        };
        let meta = ProjectMeta {
            name: name.to_string(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&meta) {
            if let Err(e) = std::fs::write(&path, json) {
                log::error!("Failed to write project.json: {e}");
            }
        }
    }

    pub fn read_project_json(path: &Path) -> Option<ProjectMeta> {
        let json_path = path.join("project.json");
        let contents = std::fs::read_to_string(&json_path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    // -----------------------------------------------------------------------
    // Local-only save helpers (project.json + index updates)
    // -----------------------------------------------------------------------

    pub fn save_and_index_project(&self, state: ProjectState) {
        self.write_project_json(&state.name);
        self.save_project_state(state);

        if let Some(path) = &self.current_project_path {
            let key = path.to_string_lossy().to_string();
            self.update_index_timestamp(&key);
        }
    }

    // -----------------------------------------------------------------------
    // Global index (local-only)
    // -----------------------------------------------------------------------

    pub fn list_projects(&self) -> Vec<ProjectIndexEntry> {
        let index_db = self.index_db.clone();
        let mut entries = run_on_rt(&self.rt, async move {
            let entries: Vec<ProjectIndexEntry> =
                index_db.select("projects").await.ok()?;
            Some(entries)
        })
        .unwrap_or_default();
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        entries
    }

    pub fn delete_project(&mut self, path: &str) {
        self.delete_index_entry(path);
        let _ = std::fs::remove_dir_all(path);
        log::info!("Deleted project at {path}");
    }

    pub fn update_index_name(&self, path: &str, name: &str) {
        let index_db = self.index_db.clone();
        let path = path.to_string();
        let name = name.to_string();
        run_on_rt(&self.rt, async move {
            let existing: Option<ProjectIndexEntry> =
                index_db.select(("projects", &*path)).await.ok()?;
            if let Some(mut entry) = existing {
                entry.name = name;
                entry.updated_at = now_ts();
                let _: Option<ProjectIndexEntry> = index_db
                    .upsert(("projects", &*path))
                    .content(entry)
                    .await
                    .ok()?;
            }
            Some(())
        });
    }

    // -----------------------------------------------------------------------
    // Index helpers (private)
    // -----------------------------------------------------------------------

    fn upsert_index_entry(&self, key: &str, entry: ProjectIndexEntry) {
        let index_db = self.index_db.clone();
        let key = key.to_string();
        let _ = run_on_rt(&self.rt, async move {
            let _: Option<ProjectIndexEntry> = index_db
                .upsert(("projects", &*key))
                .content(entry)
                .await?;
            Ok::<(), surrealdb::Error>(())
        });
    }

    fn delete_index_entry(&self, key: &str) {
        let index_db = self.index_db.clone();
        let key = key.to_string();
        let _ = run_on_rt(&self.rt, async move {
            let _: Option<ProjectIndexEntry> = index_db.delete(("projects", &*key)).await?;
            Ok::<(), surrealdb::Error>(())
        });
    }

    fn update_index_timestamp(&self, key: &str) {
        let index_db = self.index_db.clone();
        let key = key.to_string();
        run_on_rt(&self.rt, async move {
            let existing: Option<ProjectIndexEntry> =
                index_db.select(("projects", &*key)).await.ok()?;
            if let Some(mut entry) = existing {
                entry.updated_at = now_ts();
                let _: Option<ProjectIndexEntry> = index_db
                    .upsert(("projects", &*key))
                    .content(entry)
                    .await
                    .ok()?;
            }
            Some(())
        });
    }
}

// ---------------------------------------------------------------------------
// ProjectStore implementation
// ---------------------------------------------------------------------------

impl ProjectStore for Storage {
    fn save_project_state(&self, state: ProjectState) {
        let db = match &self.project_db {
            Some(db) => db.clone(),
            None => return,
        };
        let result = run_on_rt(&self.rt, async move {
            let _: Option<ProjectState> = db.upsert(("state", "main")).content(state).await?;
            Ok::<(), surrealdb::Error>(())
        });
        if let Err(e) = result {
            log::error!("Failed to save project state: {e}");
        }
    }

    fn load_project_state(&self) -> Option<ProjectState> {
        let db = self.project_db.as_ref()?.clone();
        run_on_rt(&self.rt, async move {
            let state: Option<ProjectState> = db.select(("state", "main")).await.ok()?;
            state
        })
    }

    fn save_audio(&self, waveform_id: &str, file_bytes: &[u8], extension: &str) {
        let db = match &self.project_db {
            Some(db) => db.clone(),
            None => return,
        };
        let data = StoredAudioData {
            waveform_id: waveform_id.to_string(),
            file_data: Bytes::from(file_bytes.to_vec()),
            extension: extension.to_string(),
        };
        let wf_id = waveform_id.to_string();
        let result = run_on_rt(&self.rt, async move {
            let _: Option<StoredAudioData> =
                db.upsert(("audio", &*wf_id)).content(data).await?;
            Ok::<(), surrealdb::Error>(())
        });
        if let Err(e) = result {
            log::error!("Failed to save audio data for waveform {waveform_id}: {e}");
        }
    }

    fn load_audio(&self, waveform_id: &str) -> Option<(Vec<u8>, String)> {
        let db = self.project_db.as_ref()?.clone();
        let wf_id = waveform_id.to_string();
        run_on_rt(&self.rt, async move {
            let data: Option<StoredAudioData> =
                db.select(("audio", &*wf_id)).await.ok()?;
            data
        })
        .map(|d| (d.file_data.into_inner().to_vec(), d.extension))
    }

    fn save_peaks(&self, waveform_id: &str, block_size: u64, left: &[f32], right: &[f32]) {
        let db = match &self.project_db {
            Some(db) => db.clone(),
            None => return,
        };
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
            log::error!("Failed to save peaks for waveform {waveform_id}: {e}");
        }
    }

    fn load_peaks(&self, waveform_id: &str) -> Option<(u64, Vec<f32>, Vec<f32>)> {
        let db = self.project_db.as_ref()?.clone();
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
        let db = match &self.project_db {
            Some(db) => db.clone(),
            None => return,
        };
        let _ = run_on_rt(&self.rt, async move {
            let _: Vec<StoredAudioData> = db.delete("audio").await?;
            let _: Vec<StoredPeaks> = db.delete("peaks").await?;
            Ok::<(), surrealdb::Error>(())
        });
    }
}

// Peak quantization: re-export from helpers
use super::helpers::{peaks_f32_to_u8, peaks_u8_to_f32};

// ---------------------------------------------------------------------------
// Utility: recursive dir copy
// ---------------------------------------------------------------------------

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Default base path
// ---------------------------------------------------------------------------

pub fn default_base_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".layers")
}
