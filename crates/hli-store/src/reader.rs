//! Lector concurrente: conexión propia, sin escrituras.

use std::path::Path;
use std::sync::Mutex;

use hli_core::{
    canon::zero_hash,
    model::*,
    HliError, Result,
};
use rusqlite::{Connection, OptionalExtension};

use crate::writer::{row_to_branch, row_to_event};

pub struct Reader {
    conn: Mutex<Connection>,
}

impl Reader {
    pub fn open(db: &Path) -> Result<Self> {
        let conn = Connection::open(db)?;
        conn.pragma_update(None, "query_only", true)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let c = self.conn.lock().map_err(|_| HliError::internal("reader lock"))?;
        f(&c)
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, name, root_path, created_at, archived_at, config_json
                 FROM workspaces ORDER BY created_at",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    root_path: r.get(2)?,
                    created_at: r.get(3)?,
                    archived_at: r.get(4)?,
                    config: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(serde_json::json!({})),
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn workspace(&self, id: &str) -> Result<Workspace> {
        self.with(|c| {
            c.query_row(
                "SELECT id, name, root_path, created_at, archived_at, config_json
                 FROM workspaces WHERE id = ?1",
                [id],
                |r| {
                    Ok(Workspace {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        root_path: r.get(2)?,
                        created_at: r.get(3)?,
                        archived_at: r.get(4)?,
                        config: serde_json::from_str(&r.get::<_, String>(5)?)
                            .unwrap_or(serde_json::json!({})),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("workspace {id}")))
        })
    }

    pub fn threads(&self, workspace_id: &str) -> Result<Vec<Thread>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, workspace_id, title, created_at, archived_at FROM threads
                 WHERE workspace_id = ?1 ORDER BY created_at",
            )?;
            let rows = stmt.query_map([workspace_id], |r| {
                Ok(Thread {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    title: r.get(2)?,
                    created_at: r.get(3)?,
                    archived_at: r.get(4)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn branches(&self, thread_id: &str) -> Result<Vec<Branch>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, thread_id, name, parent_branch_id, fork_event_id, head_event_id,
                        head_sequence, snapshot_id, created_at
                 FROM branches WHERE thread_id = ?1 ORDER BY created_at",
            )?;
            let rows = stmt.query_map([thread_id], row_to_branch)?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn branch_state(&self, branch_id: &str) -> Result<BranchState> {
        self.with(|c| {
            let branch = c
                .query_row(
                    "SELECT id, thread_id, name, parent_branch_id, fork_event_id, head_event_id,
                            head_sequence, snapshot_id, created_at
                     FROM branches WHERE id = ?1",
                    [branch_id],
                    row_to_branch,
                )
                .optional()?
                .ok_or_else(|| HliError::not_found(&format!("branch {branch_id}")))?;

            let thread = c.query_row(
                "SELECT id, workspace_id, title, created_at, archived_at FROM threads WHERE id = ?1",
                [&branch.thread_id],
                |r| {
                    Ok(Thread {
                        id: r.get(0)?,
                        workspace_id: r.get(1)?,
                        title: r.get(2)?,
                        created_at: r.get(3)?,
                        archived_at: r.get(4)?,
                    })
                },
            )?;

            let workspace = c.query_row(
                "SELECT id, name, root_path, created_at, archived_at, config_json
                 FROM workspaces WHERE id = ?1",
                [&thread.workspace_id],
                |r| {
                    Ok(Workspace {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        root_path: r.get(2)?,
                        created_at: r.get(3)?,
                        archived_at: r.get(4)?,
                        config: serde_json::from_str(&r.get::<_, String>(5)?)
                            .unwrap_or(serde_json::json!({})),
                    })
                },
            )?;

            let head_event = match &branch.head_event_id {
                Some(id) => c
                    .query_row(
                        &format!("SELECT {} FROM events WHERE event_id = ?1", crate::writer::EVENT_COLS),
                        [id],
                        row_to_event,
                    )
                    .optional()?,
                None => None,
            };

            let event_count: i64 = c.query_row(
                "SELECT COUNT(*) FROM events WHERE thread_id = ?1 AND sequence <= ?2",
                rusqlite::params![branch.thread_id, branch.head_sequence],
                |r| r.get(0),
            )?;

            let chain_head: Option<String> = c
                .query_row(
                    "SELECT event_hash FROM events WHERE workspace_id = ?1 ORDER BY sequence DESC LIMIT 1",
                    [&workspace.id],
                    |r| r.get(0),
                )
                .optional()?;

            Ok(BranchState { branch, thread, workspace, head_event, event_count, chain_head })
        })
    }

    pub fn events(&self, workspace_id: &str, branch_id: Option<&str>) -> Result<Vec<Event>> {
        self.with(|c| {
            let sql = format!(
                "SELECT {} FROM events WHERE workspace_id = ?1
                 AND (?2 IS NULL OR branch_id = ?2) ORDER BY sequence",
                crate::writer::EVENT_COLS
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params![workspace_id, branch_id], row_to_event)?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn event(&self, id: &str) -> Result<Event> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {} FROM events WHERE event_id = ?1", crate::writer::EVENT_COLS),
                [id],
                row_to_event,
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("event {id}")))
        })
    }

    pub fn verify(&self, workspace_id: &str) -> Result<VerifyReport> {
        let events = self.events(workspace_id, None)?;
        let mut prev = zero_hash();
        let mut first_bad = None;
        for e in &events {
            if e.integrity.previous_hash != prev {
                first_bad = Some(BadEvent {
                    event_id: e.event_id.clone(),
                    sequence: e.sequence,
                    reason: "previous_hash no coincide".into(),
                });
                break;
            }
            if !e.verify() {
                first_bad = Some(BadEvent {
                    event_id: e.event_id.clone(),
                    sequence: e.sequence,
                    reason: "event_hash inválido".into(),
                });
                break;
            }
            prev = e.integrity.event_hash.clone();
        }
        Ok(VerifyReport {
            workspace_id: workspace_id.to_string(),
            checked: events.len() as i64,
            ok: first_bad.is_none(),
            chain_head: events.last().map(|e| e.integrity.event_hash.clone()),
            first_bad,
        })
    }

    pub fn artifacts(&self, workspace_id: &str) -> Result<Vec<serde_json::Value>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, sha256, mime, bytes, created_at FROM artifacts
                 WHERE workspace_id = ?1 ORDER BY created_at",
            )?;
            let rows = stmt.query_map([workspace_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, String>(0)?,
                    "sha256": r.get::<_, String>(1)?,
                    "mime": r.get::<_, String>(2)?,
                    "bytes": r.get::<_, i64>(3)?,
                    "created_at": r.get::<_, String>(4)?,
                }))
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }
}
