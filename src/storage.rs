use std::path::{Path, PathBuf};

use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;

use crate::{CanvasObject, WaveformObject};

#[derive(SurrealValue)]
pub struct ProjectState {
    pub name: String,
    pub camera_position: [f32; 2],
    pub camera_zoom: f32,
    pub objects: Vec<CanvasObject>,
    pub waveforms: Vec<WaveformObject>,
    pub browser_folders: Vec<String>,
    pub browser_width: f32,
    pub browser_visible: bool,
    pub browser_expanded: Vec<String>,
}

#[derive(Clone, Debug, SurrealValue)]
pub struct ProjectListEntry {
    pub project_id: String,
    pub name: String,
}

pub struct Storage {
    db: Surreal<Db>,
    rt: tokio::runtime::Runtime,
    db_path: PathBuf,
}

impl Storage {
    pub fn open(path: &Path) -> Option<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(10 * 1024 * 1024)
            .build()
            .ok()?;

        let db = rt.block_on(async {
            let db = Surreal::new::<RocksDb>(path.to_str()?).await.ok()?;
            db.use_ns("layers").use_db("canvas").await.ok()?;
            Some(db)
        })?;

        log::info!("SurrealDB opened at {:?}", path);
        Some(Storage {
            db,
            rt,
            db_path: path.to_path_buf(),
        })
    }

    pub fn save(&self, project_id: &str, state: ProjectState) {
        let result = self.rt.block_on(async {
            let meta = ProjectListEntry {
                project_id: project_id.to_string(),
                name: state.name.clone(),
            };
            let _: Option<ProjectListEntry> = self
                .db
                .upsert(("project_meta", project_id))
                .content(meta)
                .await?;
            let _: Option<ProjectState> = self
                .db
                .upsert(("project", project_id))
                .content(state)
                .await?;
            Ok::<(), surrealdb::Error>(())
        });
        match result {
            Ok(()) => log::info!("Project '{}' saved to {:?}", project_id, self.db_path),
            Err(e) => log::error!("Failed to save project: {e}"),
        }
    }

    pub fn load(&self, project_id: &str) -> Option<ProjectState> {
        self.rt.block_on(async {
            let state: Option<ProjectState> = self.db.select(("project", project_id)).await.ok()?;
            state
        })
    }

    pub fn list_projects(&self) -> Vec<ProjectListEntry> {
        self.rt
            .block_on(async {
                let entries: Vec<ProjectListEntry> = self.db.select("project_meta").await.ok()?;
                Some(entries)
            })
            .unwrap_or_default()
    }

    pub fn delete_project(&self, project_id: &str) {
        let result = self.rt.block_on(async {
            let _: Option<ProjectListEntry> = self.db.delete(("project_meta", project_id)).await?;
            let _: Option<ProjectState> = self.db.delete(("project", project_id)).await?;
            Ok::<(), surrealdb::Error>(())
        });
        match result {
            Ok(()) => log::info!("Project '{}' deleted", project_id),
            Err(e) => log::error!("Failed to delete project '{}': {e}", project_id),
        }
    }
}

pub fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".layers")
        .join("project.db")
}
