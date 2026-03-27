use futures_util::StreamExt;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::types::Action;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::user::EphemeralMessage;
use crate::network::{NetworkMode, SharedConnectionState};
use crate::operations::CommittedOp;
use crate::user::{User, color_for_user_index};

// ---------------------------------------------------------------------------
// SurrealDB record types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SurrealValue)]
struct OpRecord {
    user_id: String,
    seq: u64,
    timestamp_ms: u64,
    op_json: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SurrealValue)]
struct PresenceRecord {
    user_id: String,
    name: String,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    color_a: f32,
    heartbeat_ms: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SurrealValue)]
struct EphemeralRecord {
    user_id: String,
    kind: String,
    payload_json: String,
    updated_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn presence_to_user(p: &PresenceRecord) -> User {
    User {
        id: p.user_id.parse().unwrap_or_default(),
        name: p.name.clone(),
        color: [p.color_r, p.color_g, p.color_b, p.color_a],
    }
}

fn committed_to_record(op: &CommittedOp) -> Option<OpRecord> {
    let op_json = match serde_json::to_string(&op.op) {
        Ok(json) => json,
        Err(e) => {
            log::error!("[SurrealClient] Failed to serialize op: {e}");
            return None;
        }
    };
    Some(OpRecord {
        user_id: op.user_id.to_string(),
        seq: op.seq,
        timestamp_ms: op.timestamp_ms,
        op_json,
    })
}

fn record_to_committed(rec: &OpRecord) -> Option<CommittedOp> {
    let op = serde_json::from_str(&rec.op_json).ok()?;
    let user_id = rec.user_id.parse().ok()?;
    Some(CommittedOp {
        op,
        user_id,
        timestamp_ms: rec.timestamp_ms,
        seq: rec.seq,
        before_selection: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Spawn the SurrealDB sync client
// ---------------------------------------------------------------------------

pub fn spawn_surreal_client(
    url: String,
    project_id: String,
    password: Option<String>,
    op_tx: mpsc::UnboundedSender<CommittedOp>,
    mut op_rx: mpsc::UnboundedReceiver<CommittedOp>,
    eph_tx: mpsc::UnboundedSender<EphemeralMessage>,
    mut eph_rx: mpsc::UnboundedReceiver<EphemeralMessage>,
    welcome_tx: tokio::sync::oneshot::Sender<User>,
    connection_state: SharedConnectionState,
    rt: &tokio::runtime::Runtime,
) -> JoinHandle<()> {
    rt.spawn(async move {
        // 1. Connect to SurrealDB
        let addr = url
            .trim_start_matches("ws://")
            .trim_start_matches("wss://");

        let db: Surreal<Client> = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Surreal::new::<Ws>(addr),
        )
        .await
        {
            Ok(Ok(db)) => db,
            Ok(Err(e)) => {
                log::error!("[SurrealClient] Connection error: {e}");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
            Err(_) => {
                log::error!("[SurrealClient] Connection timed out after 5s");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
        };

        // 2. Sign in if password provided
        if let Some(ref pass) = password {
            use surrealdb::opt::auth::Root;
            if let Err(e) = db.signin(Root { username: "root".to_string(), password: pass.clone() }).await {
                log::error!("[SurrealClient] Authentication failed: {e}");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
            log::info!("[SurrealClient] Authenticated as root");
        }

        // 3. Use namespace/database
        let db_name = format!("project_{project_id}");
        if let Err(e) = db.use_ns("layers").use_db(&db_name).await {
            log::error!("[SurrealClient] Failed to use ns/db: {e}");
            connection_state.set(NetworkMode::Disconnected);
            return;
        }
        log::info!("[SurrealClient] Connected to {url}, db={db_name}");

        // 2b. Ensure tables exist (SurrealDB requires them for LIVE SELECT)
        if let Err(e) = db
            .query(
                "DEFINE TABLE IF NOT EXISTS ops; \
                 DEFINE TABLE IF NOT EXISTS presence; \
                 DEFINE TABLE IF NOT EXISTS ephemeral;",
            )
            .await
        {
            log::error!("[SurrealClient] Failed to define tables: {e}");
            connection_state.set(NetworkMode::Disconnected);
            return;
        }

        // 3. Generate local user
        let user_id = uuid::Uuid::new_v4();
        let user_id_str = user_id.to_string();

        // Derive color from user_id hash (deterministic, no server round-trip needed)
        let color_idx: usize = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            user_id.hash(&mut hasher);
            hasher.finish() as usize
        };

        let color = color_for_user_index(color_idx);
        let name = format!("User {}", color_idx + 1);
        let user = User {
            id: user_id,
            name: name.clone(),
            color,
        };

        // 4. Upsert presence
        let presence = PresenceRecord {
            user_id: user_id_str.clone(),
            name: name.clone(),
            color_r: color[0],
            color_g: color[1],
            color_b: color[2],
            color_a: color[3],
            heartbeat_ms: now_ms(),
        };
        let _: Option<PresenceRecord> = db
            .upsert(("presence", &*user_id_str))
            .content(presence)
            .await
            .unwrap_or(None);

        // Send welcome
        connection_state.set(NetworkMode::Connected);
        let _ = welcome_tx.send(user.clone());
        log::info!(
            "[SurrealClient] Joined as {} ({})",
            user.name,
            user_id_str
        );

        // 5. Initial sync: replay all ops
        let existing_ops: Vec<OpRecord> = db
            .query("SELECT * FROM ops ORDER BY timestamp_ms ASC, seq ASC")
            .await
            .ok()
            .and_then(|mut r| r.take(0).ok())
            .unwrap_or_default();
        log::info!(
            "[SurrealClient] Replaying {} existing ops",
            existing_ops.len()
        );
        for rec in &existing_ops {
            if let Some(committed) = record_to_committed(rec) {
                let _ = op_tx.send(committed);
            }
        }

        // 6. Query existing presence → send UserJoined
        let existing_presence: Vec<PresenceRecord> = db
            .select("presence")
            .await
            .unwrap_or_default();
        for p in &existing_presence {
            if p.user_id != user_id_str {
                let _ = eph_tx.send(EphemeralMessage::UserJoined {
                    user: presence_to_user(p),
                });
            }
        }

        // 7. Start live queries
        let mut ops_stream = match db.select("ops").live().await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[SurrealClient] Failed to start ops live query: {e}");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
        };

        let mut presence_stream = match db.select("presence").live().await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[SurrealClient] Failed to start presence live query: {e}");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
        };

        let mut ephemeral_stream = match db.select("ephemeral").live().await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[SurrealClient] Failed to start ephemeral live query: {e}");
                connection_state.set(NetworkMode::Disconnected);
                return;
            }
        };

        // 8. Main select! loop
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                // Outbound: local ops → SurrealDB
                Some(committed) = op_rx.recv() => {
                    log::info!(
                        "[SurrealClient] outbound op: {} (seq={}, user={})",
                        committed.op.variant_name(),
                        committed.seq,
                        committed.user_id
                    );
                    let Some(record) = committed_to_record(&committed) else {
                        continue;
                    };
                    let result: Result<Option<OpRecord>, _> =
                        db.create("ops").content(record).await;
                    if let Err(e) = result {
                        log::warn!("[SurrealClient] Failed to create op record: {e}");
                    }
                }

                // Outbound: local ephemeral → SurrealDB
                Some(eph) = eph_rx.recv() => {
                    let (kind, payload_json, fire_and_forget) = match &eph {
                        EphemeralMessage::CursorMove { .. } => {
                            ("CursorMove".to_string(), serde_json::to_string(&eph).unwrap_or_default(), false)
                        }
                        EphemeralMessage::DragUpdate { .. } => {
                            ("DragUpdate".to_string(), serde_json::to_string(&eph).unwrap_or_default(), false)
                        }
                        EphemeralMessage::DragEnd { .. } => {
                            ("DragEnd".to_string(), serde_json::to_string(&eph).unwrap_or_default(), true)
                        }
                        EphemeralMessage::ViewportUpdate { .. } => {
                            ("ViewportUpdate".to_string(), serde_json::to_string(&eph).unwrap_or_default(), false)
                        }
                        EphemeralMessage::PlaybackUpdate { .. } => {
                            ("PlaybackUpdate".to_string(), serde_json::to_string(&eph).unwrap_or_default(), false)
                        }
                        // Don't send UserJoined/UserLeft through ephemeral table
                        _ => continue,
                    };
                    let record = EphemeralRecord {
                        user_id: user_id_str.clone(),
                        kind: kind.clone(),
                        payload_json,
                        updated_at_ms: now_ms(),
                    };
                    if fire_and_forget {
                        // Use create for one-shot messages so they aren't overwritten
                        let _: Result<Option<EphemeralRecord>, _> =
                            db.create("ephemeral").content(record).await;
                    } else {
                        // Use upsert keyed by user+kind so different message types
                        // from the same user don't overwrite each other
                        let record_key = format!("{}__{}", user_id_str, kind);
                        let _: Result<Option<EphemeralRecord>, _> =
                            db.upsert(("ephemeral", &*record_key)).content(record).await;
                    }
                }

                // Inbound: ops live query
                Some(notification) = ops_stream.next() => {
                    match notification {
                        Ok(notif) if notif.action == Action::Create => {
                            let rec: OpRecord = notif.data;
                            // Filter out our own ops
                            if rec.user_id == user_id_str {
                                continue;
                            }
                            if let Some(committed) = record_to_committed(&rec) {
                                log::info!(
                                    "[SurrealClient] inbound op: {} (seq={}, user={})",
                                    committed.op.variant_name(),
                                    committed.seq,
                                    committed.user_id
                                );
                                let _ = op_tx.send(committed);
                            }
                        }
                        Err(e) => {
                            log::error!("[SurrealClient] ops live query error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }

                // Inbound: presence live query
                Some(notification) = presence_stream.next() => {
                    match notification {
                        Ok(notif) => {
                            let rec: PresenceRecord = notif.data;
                            if rec.user_id == user_id_str {
                                continue;
                            }
                            match notif.action {
                                Action::Create => {
                                    let _ = eph_tx.send(EphemeralMessage::UserJoined {
                                        user: presence_to_user(&rec),
                                    });
                                }
                                Action::Delete => {
                                    if let Ok(uid) = rec.user_id.parse() {
                                        let _ = eph_tx.send(EphemeralMessage::UserLeft {
                                            user_id: uid,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            log::error!("[SurrealClient] presence live query error: {e}");
                            break;
                        }
                    }
                }

                // Inbound: ephemeral live query
                Some(notification) = ephemeral_stream.next() => {
                    match notification {
                        Ok(notif) if notif.action == Action::Update || notif.action == Action::Create => {
                            let rec: EphemeralRecord = notif.data;
                            if rec.user_id == user_id_str {
                                continue;
                            }
                            if let Ok(msg) = serde_json::from_str::<EphemeralMessage>(&rec.payload_json) {
                                let _ = eph_tx.send(msg);
                            }
                        }
                        Err(e) => {
                            log::error!("[SurrealClient] ephemeral live query error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }

                // Heartbeat: keep presence alive, gc stale entries
                _ = heartbeat.tick() => {
                    let presence = PresenceRecord {
                        user_id: user_id_str.clone(),
                        name: name.clone(),
                        color_r: color[0],
                        color_g: color[1],
                        color_b: color[2],
                        color_a: color[3],
                        heartbeat_ms: now_ms(),
                    };
                    let _: Result<Option<PresenceRecord>, _> =
                        db.upsert(("presence", &*user_id_str)).content(presence).await;

                    // GC stale presence (>15s without heartbeat)
                    let cutoff = now_ms().saturating_sub(15_000);
                    let _ = db
                        .query("DELETE presence WHERE heartbeat_ms < $cutoff")
                        .bind(("cutoff", cutoff))
                        .await;

                    // GC old fire-and-forget ephemeral records (>5s old)
                    let eph_cutoff = now_ms().saturating_sub(5_000);
                    let _ = db
                        .query("DELETE ephemeral WHERE updated_at_ms < $cutoff")
                        .bind(("cutoff", eph_cutoff))
                        .await;
                }
            }
        }

        // 9. Cleanup on exit
        log::info!("[SurrealClient] Cleaning up...");
        let _: Result<Option<PresenceRecord>, _> =
            db.delete(("presence", &*user_id_str)).await;
        // Delete all ephemeral records for this user (keyed by user__kind)
        let _ = db
            .query("DELETE ephemeral WHERE user_id = $uid")
            .bind(("uid", user_id_str.clone()))
            .await;
        connection_state.set(NetworkMode::Disconnected);
    })
}
