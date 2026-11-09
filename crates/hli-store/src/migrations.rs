//! Migraciones versionadas y transaccionales del esquema SQLite.
//!
//! Cada paso se aplica dentro de su propia transacción y registra su versión en
//! `schema_migrations`. Una migración fallida deja la base en la versión
//! anterior, sin estado parcial.

use std::path::{Path, PathBuf};

use hli_core::{HliError, Result, time::now_iso};
use rusqlite::{Connection, Transaction};

pub const DATABASE_SCHEMA_VERSION: i64 = hli_core::versions::DATABASE_SCHEMA_VERSION;

pub struct Step {
    pub version: i64,
    pub name: &'static str,
    apply: fn(&Transaction<'_>) -> Result<()>,
}

pub struct Pending {
    pub version: i64,
    pub name: &'static str,
}

#[derive(Debug)]
pub struct ApplyReport {
    pub from_version: i64,
    pub to_version: i64,
    pub applied: Vec<i64>,
}

pub struct Status {
    pub db_exists: bool,
    pub current_version: i64,
    pub target_version: i64,
    pub pending: Vec<Pending>,
    pub compatible: bool,
    pub backup_required: bool,
}

pub struct VerifyReport {
    pub current_version: i64,
    pub target_version: i64,
    pub schema_ok: bool,
    pub integrity_ok: bool,
    pub chain_ok: bool,
    pub events_checked: i64,
}

fn steps() -> &'static [Step] {
    &[
        Step {
            version: 1,
            name: "0001_initial",
            apply: m0001_initial,
        },
        Step {
            version: 2,
            name: "0002_runs_context",
            apply: m0002_runs_context,
        },
        Step {
            version: 3,
            name: "0003_memory_mcp",
            apply: m0003_memory_mcp,
        },
        Step {
            version: 4,
            name: "0004_modules_mch",
            apply: m0004_modules_mch,
        },
        Step {
            version: 5,
            name: "0005_async_runs",
            apply: m0005_async_runs,
        },
        Step {
            version: 6,
            name: "0006_mch_autostart",
            apply: m0006_mch_autostart,
        },
        Step {
            version: 7,
            name: "0007_module_signing",
            apply: m0007_module_signing,
        },
        Step {
            version: 8,
            name: "0008_run_plans",
            apply: m0008_run_plans,
        },
        Step {
            version: 9,
            name: "0009_tool_permissions",
            apply: m0009_tool_permissions,
        },
    ]
}

pub fn steps_list() -> &'static [Step] {
    steps()
}

fn bootstrap(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version    INTEGER PRIMARY KEY,
           applied_at TEXT NOT NULL,
           checksum   TEXT NOT NULL
         );",
    )?;
    Ok(())
}

pub fn current_version(conn: &Connection) -> Result<i64> {
    bootstrap(conn)?;
    let v: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .map_err(HliError::from)?;
    Ok(v.unwrap_or(0))
}

pub fn pending_steps(conn: &Connection) -> Result<Vec<Pending>> {
    let current = current_version(conn)?;
    Ok(steps()
        .iter()
        .filter(|s| s.version > current)
        .map(|s| Pending {
            version: s.version,
            name: s.name,
        })
        .collect())
}

/// Aplica todas las migraciones pendientes hasta `DATABASE_SCHEMA_VERSION`.
pub fn ensure(conn: &mut Connection) -> Result<ApplyReport> {
    apply(conn, DATABASE_SCHEMA_VERSION)
}

pub fn apply(conn: &mut Connection, target: i64) -> Result<ApplyReport> {
    apply_with_failure(conn, target, None)
}

/// `fail_at` inyecta un fallo tras aplicar ese paso (uso exclusivo de pruebas):
/// la transacción se revierte y la versión registrada no avanza.
pub fn apply_with_failure(
    conn: &mut Connection,
    target: i64,
    fail_at: Option<i64>,
) -> Result<ApplyReport> {
    if target > DATABASE_SCHEMA_VERSION {
        return Err(HliError::invalid(format!(
            "versión objetivo futura: {target} > {DATABASE_SCHEMA_VERSION}"
        )));
    }
    bootstrap(conn)?;
    let from = current_version(conn)?;
    if from > DATABASE_SCHEMA_VERSION {
        return Err(HliError::conflict(format!(
            "schema futuro: {from} > {DATABASE_SCHEMA_VERSION}; actualiza HLI"
        )));
    }
    if from > target {
        return Err(HliError::conflict(format!(
            "no se puede degradar el schema de {from} a {target}"
        )));
    }
    let mut applied = Vec::new();
    for step in steps()
        .iter()
        .filter(|s| s.version > from && s.version <= target)
    {
        let tx = conn.transaction()?;
        (step.apply)(&tx).map_err(|e| {
            HliError::new(
                "migration_failed",
                format!("{} falló: {}", step.name, e.message),
            )
        })?;
        if fail_at == Some(step.version) {
            return Err(HliError::new(
                "migration_failed",
                format!("{} falló (simulado)", step.name),
            ));
        }
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at, checksum) VALUES (?1, ?2, ?3)",
            rusqlite::params![step.version, now_iso(), step.name],
        )?;
        tx.commit()?;
        applied.push(step.version);
    }
    Ok(ApplyReport {
        from_version: from,
        to_version: current_version(conn)?,
        applied,
    })
}

pub fn verify_schema(conn: &Connection) -> Result<()> {
    let current = current_version(conn)?;
    if current != DATABASE_SCHEMA_VERSION {
        return Err(HliError::conflict(format!(
            "schema parcial: versión {current} de {DATABASE_SCHEMA_VERSION}; ejecuta hli migrate apply"
        )));
    }
    for table in [
        "workspaces",
        "threads",
        "branches",
        "events",
        "artifacts",
        "snapshots",
        "runs",
        "context_manifests",
        "memories",
        "modules",
        "mch_profiles",
        "mch_instances",
        "run_plans",
        "permission_grants",
    ] {
        if !table_exists(conn, table)? {
            return Err(HliError::conflict(format!(
                "schema parcial: falta la tabla {table}"
            )));
        }
    }
    for (table, column) in [
        ("mch_instances", "autostart"),
        ("runs", "receipt_json"),
        ("runs", "branch_revision"),
        ("memories", "supersedes_json"),
        ("events", "event_hash"),
    ] {
        if !column_exists(conn, table, column)? {
            return Err(HliError::conflict(format!(
                "schema parcial: falta {table}.{column}"
            )));
        }
    }
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(HliError::conflict(format!(
            "integrity_check devolvió: {integrity}"
        )));
    }
    Ok(())
}

pub fn verify_path(db: &Path) -> Result<VerifyReport> {
    let conn = Connection::open(db)?;
    let current = current_version(&conn)?;
    let schema_ok = verify_schema(&conn).is_ok();
    let integrity_ok = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map(|v| v == "ok")
        .unwrap_or(false);
    let (chain_ok, events_checked) = verify_chain(&conn)?;
    Ok(VerifyReport {
        current_version: current,
        target_version: DATABASE_SCHEMA_VERSION,
        schema_ok,
        integrity_ok,
        chain_ok,
        events_checked,
    })
}

fn verify_chain(conn: &Connection) -> Result<(bool, i64)> {
    let mut stmt = conn.prepare(
        "SELECT workspace_id, event_hash, prev_hash FROM events ORDER BY workspace_id, sequence",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut last_ws = String::new();
    let mut last_hash = String::new();
    let mut checked = 0i64;
    for row in rows {
        let (ws, hash, prev) = row?;
        if ws != last_ws {
            last_ws = ws;
            last_hash = zero_hash();
        }
        if prev != last_hash {
            return Ok((false, checked));
        }
        last_hash = hash;
        checked += 1;
    }
    Ok((true, checked))
}

pub fn status_path(db: &Path) -> Result<Status> {
    let exists = db.exists();
    if !exists {
        return Ok(Status {
            db_exists: false,
            current_version: 0,
            target_version: DATABASE_SCHEMA_VERSION,
            pending: public_pending(&[]),
            compatible: true,
            backup_required: false,
        });
    }
    let conn = Connection::open(db)?;
    let current = current_version(&conn)?;
    let pending = pending_steps(&conn)?;
    Ok(Status {
        db_exists: true,
        current_version: current,
        target_version: DATABASE_SCHEMA_VERSION,
        compatible: current <= DATABASE_SCHEMA_VERSION,
        backup_required: current > 0 && !pending.is_empty(),
        pending,
    })
}

fn public_pending(list: &[Pending]) -> Vec<Pending> {
    list.iter()
        .map(|p| Pending {
            version: p.version,
            name: p.name,
        })
        .collect()
}

/// Backup consistente vía `VACUUM INTO`; no copia secretos (solo la base).
pub fn backup_path(db: &Path, backups_dir: &Path) -> Result<PathBuf> {
    if !db.exists() {
        return Err(HliError::not_found("base de datos"));
    }
    std::fs::create_dir_all(backups_dir)?;
    let stamp = now_iso().replace([':', '-', 'T', 'Z', '.'], "");
    let out = backups_dir.join(format!("hli-{stamp}.db"));
    let conn = Connection::open(db)?;
    conn.execute("VACUUM INTO ?1", [out.to_string_lossy().to_string()])?;
    Ok(out)
}

pub fn apply_path(
    db: &Path,
    backups_dir: &Path,
    backup: bool,
) -> Result<(ApplyReport, Option<PathBuf>)> {
    let mut conn = Connection::open(db)?;
    bootstrap(&conn)?;
    let current = current_version(&conn)?;
    let target = DATABASE_SCHEMA_VERSION;
    if current > target {
        return Err(HliError::conflict(format!(
            "schema futuro: {current} > {target}; actualiza HLI"
        )));
    }
    let mut backup_file = None;
    if backup && current > 0 && current < target {
        backup_file = Some(backup_path(db, backups_dir)?);
    }
    let fail_at = std::env::var("HLI_MIGRATION_FAIL_AT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok());
    let report = apply_with_failure(&mut conn, target, fail_at)?;
    verify_schema(&conn)?;
    Ok((report, backup_file))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn add_column(tx: &Transaction<'_>, table: &str, column: &str, ddl: &str) -> Result<()> {
    let n: i64 = tx.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |r| r.get(0),
    )?;
    if n == 0 {
        tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"))?;
    }
    Ok(())
}

fn zero_hash() -> String {
    format!("sha256:{}", "0".repeat(64))
}

fn m0001_initial(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS workspaces (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  root_path   TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  archived_at TEXT,
  config_json TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS threads (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  title        TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  archived_at  TEXT
);
CREATE TABLE IF NOT EXISTS branches (
  id               TEXT PRIMARY KEY,
  thread_id        TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  name             TEXT NOT NULL,
  parent_branch_id TEXT,
  fork_event_id    TEXT,
  head_event_id    TEXT,
  head_sequence    INTEGER NOT NULL DEFAULT 0,
  snapshot_id      TEXT,
  created_at       TEXT NOT NULL,
  UNIQUE (thread_id, name)
);
CREATE TABLE IF NOT EXISTS events (
  event_id        TEXT PRIMARY KEY,
  workspace_id    TEXT NOT NULL REFERENCES workspaces(id),
  thread_id       TEXT,
  branch_id       TEXT,
  run_id          TEXT,
  sequence        INTEGER NOT NULL,
  type            TEXT NOT NULL,
  schema_version  INTEGER NOT NULL,
  created_at      TEXT NOT NULL,
  actor_json      TEXT NOT NULL,
  payload_json    TEXT NOT NULL,
  artifacts_json  TEXT NOT NULL DEFAULT '[]',
  causation_id    TEXT,
  correlation_id  TEXT,
  idempotency_key TEXT,
  prev_hash       TEXT NOT NULL,
  event_hash      TEXT NOT NULL,
  UNIQUE (workspace_id, sequence),
  UNIQUE (workspace_id, idempotency_key)
);
CREATE TABLE IF NOT EXISTS artifacts (
  id              TEXT PRIMARY KEY,
  workspace_id    TEXT NOT NULL REFERENCES workspaces(id),
  sha256          TEXT NOT NULL,
  mime            TEXT NOT NULL,
  bytes           INTEGER NOT NULL,
  source_event_id TEXT,
  created_at      TEXT NOT NULL,
  UNIQUE (workspace_id, sha256)
);
CREATE TABLE IF NOT EXISTS snapshots (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  branch_id    TEXT NOT NULL,
  at_sequence  INTEGER NOT NULL,
  path         TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  UNIQUE (branch_id, at_sequence)
);
CREATE TABLE IF NOT EXISTS runs (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  thread_id    TEXT NOT NULL REFERENCES threads(id),
  branch_id    TEXT NOT NULL REFERENCES branches(id),
  prompt       TEXT NOT NULL,
  started_at   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS artifact_slices (
  id            TEXT PRIMARY KEY,
  artifact_id   TEXT NOT NULL,
  workspace_id  TEXT NOT NULL,
  selector_json TEXT NOT NULL,
  media_type    TEXT NOT NULL,
  slice_hash    TEXT NOT NULL,
  bytes         INTEGER NOT NULL,
  content       BLOB NOT NULL,
  reason        TEXT NOT NULL,
  created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_slices_artifact ON artifact_slices(artifact_id);
CREATE INDEX IF NOT EXISTS idx_threads_ws      ON threads(workspace_id, created_at);
CREATE INDEX IF NOT EXISTS idx_branches_thread ON branches(thread_id, created_at);
CREATE INDEX IF NOT EXISTS idx_events_branch   ON events(branch_id, sequence);
CREATE INDEX IF NOT EXISTS idx_events_type     ON events(workspace_id, type);
CREATE INDEX IF NOT EXISTS idx_artifacts_hash  ON artifacts(workspace_id, sha256);
CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
"#,
    )?;
    Ok(())
}

fn m0002_runs_context(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS context_manifests (
  id               TEXT PRIMARY KEY,
  workspace_id     TEXT NOT NULL REFERENCES workspaces(id),
  run_id           TEXT NOT NULL REFERENCES runs(id),
  branch_revision  INTEGER NOT NULL,
  budget_tokens    INTEGER NOT NULL,
  estimated_tokens INTEGER NOT NULL,
  content_hash     TEXT NOT NULL,
  body_json        TEXT NOT NULL,
  created_at       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_manifests_run ON context_manifests(run_id);
"#,
    )?;
    Ok(())
}

fn m0003_memory_mcp(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS memories (
  id               TEXT PRIMARY KEY,
  workspace_id     TEXT NOT NULL REFERENCES workspaces(id),
  scope            TEXT NOT NULL,
  scope_id         TEXT,
  type             TEXT NOT NULL,
  status           TEXT NOT NULL,
  content          TEXT NOT NULL,
  observed_at      TEXT NOT NULL DEFAULT '',
  valid_from       TEXT NOT NULL,
  superseded_at    TEXT,
  source_authority TEXT NOT NULL DEFAULT 'user',
  subject          TEXT,
  relation         TEXT,
  object           TEXT,
  valid_until      TEXT,
  supersedes_json  TEXT NOT NULL DEFAULT '[]',
  contradicts_json TEXT NOT NULL DEFAULT '[]',
  sensitivity      TEXT NOT NULL,
  authorship_json  TEXT NOT NULL,
  source_events    TEXT NOT NULL DEFAULT '[]',
  created_at       TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
  content,
  mem_id UNINDEXED,
  workspace_id UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE INDEX IF NOT EXISTS idx_memories_ws ON memories(workspace_id, status);
"#,
    )?;
    Ok(())
}

fn m0004_modules_mch(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS modules (
  name         TEXT NOT NULL,
  version      TEXT NOT NULL,
  path         TEXT NOT NULL,
  manifest_json TEXT NOT NULL,
  checksum     TEXT NOT NULL,
  installed_at TEXT NOT NULL,
  PRIMARY KEY (name, version)
);
CREATE TABLE IF NOT EXISTS mch_profiles (
  name          TEXT PRIMARY KEY,
  module_name   TEXT NOT NULL,
  domain        TEXT NOT NULL,
  manifest_json TEXT NOT NULL,
  created_at    TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS mch_instances (
  name         TEXT PRIMARY KEY,
  profile      TEXT NOT NULL,
  workspace_id TEXT,
  state        TEXT NOT NULL,
  pid          INTEGER,
  created_at   TEXT NOT NULL
);
"#,
    )?;
    Ok(())
}

fn m0005_async_runs(tx: &Transaction<'_>) -> Result<()> {
    add_column(tx, "runs", "branch_revision", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(tx, "runs", "state", "TEXT NOT NULL DEFAULT 'completed'")?;
    add_column(tx, "runs", "ended_at", "TEXT")?;
    add_column(tx, "runs", "result_json", "TEXT")?;
    add_column(tx, "runs", "receipt_json", "TEXT")?;
    tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_runs_ws ON runs(workspace_id, started_at);")?;
    Ok(())
}

fn m0006_mch_autostart(tx: &Transaction<'_>) -> Result<()> {
    add_column(
        tx,
        "mch_instances",
        "autostart",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

fn m0007_module_signing(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS module_signatures (
  module_name  TEXT NOT NULL,
  version      TEXT NOT NULL,
  key_id       TEXT,
  fingerprint  TEXT,
  payload_hash TEXT,
  signed_at    TEXT,
  PRIMARY KEY (module_name, version)
);
"#,
    )?;
    Ok(())
}


fn m0009_tool_permissions(tx: &Transaction<'_>) -> Result<()> {
    add_column(tx, "runs", "approval_kind", "TEXT")?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS tool_permission_requests (
  request_id          TEXT PRIMARY KEY,
  run_id              TEXT NOT NULL,
  plan_id             TEXT NOT NULL,
  plan_revision       INTEGER NOT NULL,
  workspace_id        TEXT NOT NULL REFERENCES workspaces(id),
  thread_id           TEXT NOT NULL,
  branch_id           TEXT NOT NULL,
  tool_name           TEXT NOT NULL,
  operation           TEXT NOT NULL,
  effect              TEXT NOT NULL,
  target              TEXT,
  arguments_hash      TEXT NOT NULL,
  arguments_summary   TEXT NOT NULL,
  reason              TEXT NOT NULL DEFAULT '',
  requested_scope     TEXT,
  status              TEXT NOT NULL,
  created_at          TEXT NOT NULL,
  decided_at          TEXT,
  decided_by          TEXT,
  decision            TEXT,
  decision_scope      TEXT,
  content_hash        TEXT NOT NULL,
  body_json           TEXT NOT NULL,
  invocation_json     TEXT,
  invocation_redacted INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_tool_requests_run ON tool_permission_requests(run_id, status);
CREATE INDEX IF NOT EXISTS idx_tool_requests_ws  ON tool_permission_requests(workspace_id, status);
CREATE TABLE IF NOT EXISTS tool_grants (
  grant_id     TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  run_id       TEXT,
  effect       TEXT NOT NULL,
  tool_name    TEXT NOT NULL,
  target       TEXT,
  scope        TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  created_by   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tool_grants_lookup ON tool_grants(workspace_id, tool_name, scope);
"#,
    )?;
    Ok(())
}

fn m0008_run_plans(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS run_plans (
  plan_id               TEXT PRIMARY KEY,
  run_id                TEXT NOT NULL,
  revision              INTEGER NOT NULL,
  supersedes_plan_id    TEXT,
  invalidation_reasons  TEXT NOT NULL DEFAULT '[]',
  workspace_id          TEXT NOT NULL REFERENCES workspaces(id),
  thread_id             TEXT NOT NULL,
  branch_id             TEXT NOT NULL,
  branch_revision       INTEGER NOT NULL,
  context_manifest_id   TEXT NOT NULL,
  origin                TEXT NOT NULL,
  status                TEXT NOT NULL,
  content_hash          TEXT NOT NULL,
  body_json             TEXT NOT NULL,
  execution_json        TEXT,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  approved_at           TEXT,
  rejected_at           TEXT,
  approved_by           TEXT,
  approval_scope        TEXT,
  rejection_reason      TEXT,
  UNIQUE (run_id, revision)
);
CREATE INDEX IF NOT EXISTS idx_run_plans_run    ON run_plans(run_id, revision);
CREATE INDEX IF NOT EXISTS idx_run_plans_ws     ON run_plans(workspace_id, created_at);
CREATE INDEX IF NOT EXISTS idx_run_plans_status ON run_plans(workspace_id, status);
CREATE TABLE IF NOT EXISTS permission_grants (
  grant_id     TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  effect       TEXT NOT NULL,
  scope        TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  created_by   TEXT NOT NULL,
  run_id       TEXT,
  UNIQUE (workspace_id, effect, scope)
);
"#,
    )?;
    Ok(())
}
