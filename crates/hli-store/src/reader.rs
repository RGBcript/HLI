//! Lector concurrente: conexión propia, sin escrituras.

use std::path::Path;
use std::sync::Mutex;

use hli_core::{canon::zero_hash, model::*, HliError, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use crate::writer::{row_to_branch, row_to_event};

pub struct Reader {
    conn: Mutex<Connection>,
    artifacts_dir: std::path::PathBuf,
}

impl Reader {
    pub fn open(db: &Path, artifacts_dir: std::path::PathBuf) -> Result<Self> {
        let conn = Connection::open(db)?;
        conn.pragma_update(None, "query_only", true)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        Ok(Self {
            conn: Mutex::new(conn),
            artifacts_dir,
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let c = self
            .conn
            .lock()
            .map_err(|_| HliError::internal("reader lock"))?;
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
                    config: serde_json::from_str(&r.get::<_, String>(5)?)
                        .unwrap_or(serde_json::json!({})),
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
                &format!(
                    "SELECT {} FROM events WHERE event_id = ?1",
                    crate::writer::EVENT_COLS
                ),
                [id],
                row_to_event,
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("event {id}")))
        })
    }

    /// Eventos visibles para una rama: su trayectoria ancestral materializada.
    pub fn visible_events(&self, branch_id: &str, revision: i64) -> Result<Vec<Event>> {
        self.with(|c| {
            let mut chain: Vec<(String, i64)> = Vec::new();
            let mut cur = branch_id.to_string();
            let mut limit = revision;
            loop {
                let (parent, fork): (Option<String>, Option<String>) = c
                    .query_row(
                        "SELECT parent_branch_id, fork_event_id FROM branches WHERE id = ?1",
                        [&cur],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?
                    .ok_or_else(|| HliError::not_found(&format!("branch {cur}")))?;
                chain.push((cur.clone(), limit));
                match parent {
                    Some(p) => {
                        let fseq: i64 = match fork {
                            Some(f) => c
                                .query_row("SELECT sequence FROM events WHERE event_id = ?1", [&f], |r| r.get(0))
                                .optional()?
                                .unwrap_or(i64::MAX),
                            None => i64::MAX,
                        };
                        limit = fseq;
                        cur = p;
                    }
                    None => break,
                }
            }

            let mut out: Vec<Event> = Vec::new();
            for (b, lim) in &chain {
                let mut stmt = c.prepare(&format!(
                    "SELECT {} FROM events WHERE branch_id = ?1 AND sequence <= ?2 ORDER BY sequence",
                    crate::writer::EVENT_COLS
                ))?;
                let rows = stmt.query_map(rusqlite::params![b, lim], row_to_event)?;
                for e in rows {
                    out.push(e?);
                }
            }

            // Eventos del thread que no pertenecen a ninguna rama (creación del thread).
            let thread_id: String = c.query_row(
                "SELECT thread_id FROM branches WHERE id = ?1",
                [branch_id],
                |r| r.get(0),
            )?;
            let mut stmt = c.prepare(&format!(
                "SELECT {} FROM events WHERE thread_id = ?1 AND branch_id IS NULL AND sequence <= ?2",
                crate::writer::EVENT_COLS
            ))?;
            let rows = stmt.query_map(rusqlite::params![thread_id, revision], row_to_event)?;
            for e in rows {
                out.push(e?);
            }

            out.sort_by_key(|e| e.sequence);
            out.dedup_by(|a, b| a.event_id == b.event_id);
            Ok(out)
        })
    }

    pub fn memory(&self, id: &str) -> Result<Memory> {
        self.with(|c| {
            c.query_row(
                &format!(
                    "SELECT {} FROM memories WHERE id = ?1",
                    crate::writer::MEMORY_COLS
                ),
                [id],
                crate::writer::row_to_memory,
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("memoria {id}")))
        })
    }

    pub fn memories(&self, workspace_id: &str) -> Result<Vec<Memory>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {} FROM memories WHERE workspace_id = ?1 ORDER BY created_at",
                crate::writer::MEMORY_COLS
            ))?;
            let rows = stmt.query_map([workspace_id], crate::writer::row_to_memory)?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn search_memories(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        self.with(|c| {
            let terms: Vec<String> = query
                .to_lowercase()
                .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
                .filter(|t| t.chars().count() >= 2)
                .map(|t| t.replace('"', ""))
                .take(12)
                .collect();
            if terms.is_empty() {
                return Ok(vec![]);
            }
            let match_expr = terms
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(" OR ");
            let mut stmt = c.prepare(
                "SELECT m.id, m.type, m.status, m.scope, m.content, bm25(memories_fts) AS score
                 FROM memories_fts f JOIN memories m ON m.id = f.mem_id
                 WHERE memories_fts MATCH ?1 AND f.workspace_id = ?2 AND m.status = 'active'
                   AND m.valid_from <= ?3 AND (m.valid_until IS NULL OR m.valid_until > ?3)
                 ORDER BY score LIMIT ?4",
            )?;
            let now = hli_core::time::now_iso();
            let rows = stmt.query_map(
                rusqlite::params![match_expr, workspace_id, now, limit as i64],
                |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, String>(0)?,
                        "type": r.get::<_, String>(1)?,
                        "status": r.get::<_, String>(2)?,
                        "scope": r.get::<_, String>(3)?,
                        "content": r.get::<_, String>(4)?,
                        "score": r.get::<_, f64>(5)?.abs()
                    }))
                },
            )?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn artifact(&self, id: &str) -> Result<crate::writer::ArtifactInfo> {
        self.with(|c| {
            let (id, sha, mime, bytes): (String, String, String, i64) = c
                .query_row(
                    "SELECT id, sha256, mime, bytes FROM artifacts WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?
                .ok_or_else(|| HliError::not_found(&format!("artifact {id}")))?;
            let path = self.artifacts_dir.join(&sha).to_string_lossy().to_string();
            Ok(crate::writer::ArtifactInfo {
                id,
                sha256: format!("sha256:{sha}"),
                mime,
                bytes,
                path,
            })
        })
    }

    pub fn workspace_of_thread(&self, thread_id: &str) -> Result<String> {
        self.with(|c| {
            c.query_row(
                "SELECT workspace_id FROM threads WHERE id = ?1",
                [thread_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("thread {thread_id}")))
        })
    }

    pub fn modules(&self) -> Result<Vec<serde_json::Value>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT name, version, path, manifest_json, checksum, installed_at FROM modules ORDER BY name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(serde_json::json!({
                    "name": r.get::<_, String>(0)?,
                    "version": r.get::<_, String>(1)?,
                    "path": r.get::<_, String>(2)?,
                    "manifest": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
                    "checksum": r.get::<_, String>(4)?,
                    "installed_at": r.get::<_, String>(5)?,
                }))
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn module(&self, name: &str) -> Result<serde_json::Value> {
        self.with(|c| {
            c.query_row(
                "SELECT name, version, path, manifest_json, checksum, installed_at FROM modules
                 WHERE name = ?1 ORDER BY installed_at DESC LIMIT 1",
                [name],
                |r| {
                    Ok(serde_json::json!({
                        "name": r.get::<_, String>(0)?,
                        "version": r.get::<_, String>(1)?,
                        "path": r.get::<_, String>(2)?,
                        "manifest": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
                        "checksum": r.get::<_, String>(4)?,
                        "installed_at": r.get::<_, String>(5)?,
                    }))
                },
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("módulo {name}")))
        })
    }

    pub fn mchs(&self) -> Result<Vec<serde_json::Value>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT i.name, i.profile, i.workspace_id, i.state, i.pid, i.autostart,
                        i.created_at, p.module_name, p.domain
                 FROM mch_instances i JOIN mch_profiles p ON p.name = i.profile
                 ORDER BY i.name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(serde_json::json!({
                    "name": r.get::<_, String>(0)?,
                    "profile": r.get::<_, String>(1)?,
                    "workspace_id": r.get::<_, Option<String>>(2)?,
                    "state": r.get::<_, String>(3)?,
                    "pid": r.get::<_, Option<i64>>(4)?,
                    "autostart": r.get::<_, i64>(5)? != 0,
                    "created_at": r.get::<_, String>(6)?,
                    "module": r.get::<_, String>(7)?,
                    "domain": r.get::<_, String>(8)?,
                }))
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn mch(&self, name: &str) -> Result<serde_json::Value> {
        self.with(|c| {
            c.query_row(
                "SELECT i.name, i.profile, i.workspace_id, i.state, i.pid, i.autostart,
                        i.created_at, p.module_name, p.domain, p.manifest_json
                 FROM mch_instances i JOIN mch_profiles p ON p.name = i.profile
                 WHERE i.name = ?1",
                [name],
                |r| {
                    Ok(serde_json::json!({
                        "name": r.get::<_, String>(0)?,
                        "profile": r.get::<_, String>(1)?,
                        "workspace_id": r.get::<_, Option<String>>(2)?,
                        "state": r.get::<_, String>(3)?,
                        "pid": r.get::<_, Option<i64>>(4)?,
                        "autostart": r.get::<_, i64>(5)? != 0,
                        "created_at": r.get::<_, String>(6)?,
                        "module": r.get::<_, String>(7)?,
                        "domain": r.get::<_, String>(8)?,
                        "manifest": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(9)?).unwrap_or(serde_json::json!({})),
                    }))
                },
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("MCH {name}")))
        })
    }

    pub fn last_provider(&self, thread_id: &str) -> Result<Option<String>> {
        self.with(|c| {
            let body: Option<String> = c
                .query_row(
                    "SELECT receipt_json FROM runs WHERE thread_id = ?1 AND receipt_json IS NOT NULL
                     ORDER BY started_at DESC LIMIT 1",
                    [thread_id],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(body
                .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                .and_then(|v| v["provider"].as_str().map(str::to_string)))
        })
    }

    pub fn slices_for_artifact(&self, artifact_id: &str) -> Result<Vec<(ArtifactSlice, Vec<u8>)>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, artifact_id, selector_json, media_type, slice_hash, reason, content
                 FROM artifact_slices WHERE artifact_id = ?1 ORDER BY created_at",
            )?;
            let rows = stmt.query_map([artifact_id], |r| {
                let selector: Value =
                    serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or(serde_json::json!({}));
                let slice = ArtifactSlice {
                    slice_id: r.get(0)?,
                    artifact_id: r.get(1)?,
                    artifact_hash: String::new(),
                    media_type: r.get(3)?,
                    selector,
                    slice_hash: r.get(4)?,
                    estimated_tokens: 0,
                    reason: r.get(5)?,
                };
                Ok((slice, r.get::<_, Vec<u8>>(6)?))
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn slice(&self, id: &str) -> Result<(ArtifactSlice, Vec<u8>)> {
        self.with(|c| {
            c.query_row(
                "SELECT id, artifact_id, selector_json, media_type, slice_hash, reason, content
                 FROM artifact_slices WHERE id = ?1",
                [id],
                |r| {
                    let selector: Value = serde_json::from_str(&r.get::<_, String>(2)?)
                        .unwrap_or(serde_json::json!({}));
                    Ok((
                        ArtifactSlice {
                            slice_id: r.get(0)?,
                            artifact_id: r.get(1)?,
                            artifact_hash: String::new(),
                            media_type: r.get(3)?,
                            selector,
                            slice_hash: r.get(4)?,
                            estimated_tokens: 0,
                            reason: r.get(5)?,
                        },
                        r.get::<_, Vec<u8>>(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("slice {id}")))
        })
    }

    pub fn run(&self, id: &str) -> Result<Run> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {} FROM runs WHERE id = ?1", crate::writer::RUN_COLS),
                [id],
                crate::writer::row_to_run,
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("run {id}")))
        })
    }

    pub fn runs(&self, workspace_id: &str) -> Result<Vec<Run>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {} FROM runs WHERE workspace_id = ?1 ORDER BY started_at",
                crate::writer::RUN_COLS
            ))?;
            let rows = stmt.query_map([workspace_id], crate::writer::row_to_run)?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn manifest_for_run(&self, run_id: &str) -> Result<ContextManifest> {
        self.with(|c| {
            c.query_row(
                "SELECT body_json FROM context_manifests WHERE run_id = ?1 ORDER BY created_at DESC LIMIT 1",
                [run_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|b| serde_json::from_str::<ContextManifest>(&b).ok())
            .ok_or_else(|| HliError::not_found(&format!("manifest del run {run_id}")))
        })
    }

    pub fn manifest(&self, id: &str) -> Result<ContextManifest> {
        self.with(|c| {
            c.query_row(
                "SELECT body_json FROM context_manifests WHERE id = ?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|b| serde_json::from_str::<ContextManifest>(&b).ok())
            .ok_or_else(|| HliError::not_found(&format!("manifest {id}")))
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
