use hli_core::{time::now_iso, HliError, Result};
use rusqlite::Connection;

pub const SCHEMA_VERSION: i64 = 1;

const DDL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL,
  checksum   TEXT NOT NULL
);

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
  id              TEXT PRIMARY KEY,
  workspace_id    TEXT NOT NULL REFERENCES workspaces(id),
  thread_id       TEXT NOT NULL REFERENCES threads(id),
  branch_id       TEXT NOT NULL REFERENCES branches(id),
  branch_revision INTEGER NOT NULL,
  state           TEXT NOT NULL,
  prompt          TEXT NOT NULL,
  started_at      TEXT NOT NULL,
  ended_at        TEXT,
  result_json     TEXT,
  receipt_json    TEXT
);

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

CREATE TABLE IF NOT EXISTS artifact_slices (
  id           TEXT PRIMARY KEY,
  artifact_id  TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  selector_json TEXT NOT NULL,
  media_type   TEXT NOT NULL,
  slice_hash   TEXT NOT NULL,
  bytes        INTEGER NOT NULL,
  content      BLOB NOT NULL,
  reason       TEXT NOT NULL,
  created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_slices_artifact ON artifact_slices(artifact_id);
CREATE INDEX IF NOT EXISTS idx_memories_ws ON memories(workspace_id, status);

CREATE INDEX IF NOT EXISTS idx_runs_ws      ON runs(workspace_id, started_at);
CREATE INDEX IF NOT EXISTS idx_manifests_run ON context_manifests(run_id);

CREATE INDEX IF NOT EXISTS idx_threads_ws      ON threads(workspace_id, created_at);
CREATE INDEX IF NOT EXISTS idx_branches_thread ON branches(thread_id, created_at);
CREATE INDEX IF NOT EXISTS idx_events_branch   ON events(branch_id, sequence);
CREATE INDEX IF NOT EXISTS idx_events_type     ON events(workspace_id, type);
CREATE INDEX IF NOT EXISTS idx_artifacts_hash  ON artifacts(workspace_id, sha256);

CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;

CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
"#;

pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    ensure_column(conn, "memories", "observed_at", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "memories", "superseded_at", "TEXT")?;
    ensure_column(conn, "memories", "source_authority", "TEXT NOT NULL DEFAULT 'user'")?;
    ensure_column(conn, "memories", "subject", "TEXT")?;
    ensure_column(conn, "memories", "relation", "TEXT")?;
    ensure_column(conn, "memories", "object", "TEXT")?;
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
        [SCHEMA_VERSION],
        |r| r.get(0),
    )?;
    if exists == 0 {
        conn.execute(
            "INSERT INTO schema_migrations(version, applied_at, checksum) VALUES (?1, ?2, ?3)",
            rusqlite::params![SCHEMA_VERSION, now_iso(), "v1"],
        )?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, col: &str, ddl: &str) -> Result<()> {
    let exists: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        [col],
        |r| r.get(0),
    )?;
    if exists == 0 {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {ddl}"))?;
    }
    Ok(())
}

pub fn require_workspace(conn: &Connection, id: &str) -> Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM workspaces WHERE id = ?1", [id], |r| r.get(0))?;
    if n == 0 {
        return Err(HliError::not_found(&format!("workspace {id}")));
    }
    Ok(())
}
