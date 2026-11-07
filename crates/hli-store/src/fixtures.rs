//! Fixture reproducible del esquema anterior (versión 5: pre-autostart y
//! pre-firma). Se construye aplicando solo las migraciones históricas 0001–0005
//! y sembrando datos reales; no es una base actual con el número cambiado.

use std::path::Path;

use hli_core::{
    Result,
    canon::hash_canonical,
    model::{EVENT_SCHEMA_VERSION, Event, Integrity},
    time::now_iso,
};
use rusqlite::Connection;
use serde_json::json;

use crate::migrations;

pub const FIXTURE_WORKSPACE: &str = "ws_migration_fixture";
pub const FIXTURE_THREAD: &str = "th_migration_fixture";
pub const FIXTURE_BRANCH: &str = "br_migration_fixture";
pub const FIXTURE_RUN: &str = "run_migration_fixture";
pub const FIXTURE_MEMORY_ACTIVE: &str = "mem_migration_active";
pub const FIXTURE_MEMORY_SUPERSEDED: &str = "mem_migration_old";
pub const FIXTURE_ARTIFACT: &str = "art_migration_fixture";
pub const FIXTURE_MODULE: &str = "migration-module";
pub const FIXTURE_MCH: &str = "migration-main";

pub fn create_previous_fixture(home: &Path) -> Result<()> {
    std::fs::create_dir_all(home.join("artifacts"))?;
    std::fs::create_dir_all(home.join("snapshots"))?;
    let module_dir = home.join("modules").join(FIXTURE_MODULE).join("0.1.0");
    std::fs::create_dir_all(module_dir.join("bin"))?;

    let db = home.join("hli.db");
    let mut conn = Connection::open(&db)?;
    migrations::apply(&mut conn, 5)?;

    let now = now_iso();
    let manifest = json!({
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": FIXTURE_MODULE, "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "runtime": {"kind": "subprocess", "entry": "bin/hli-module-fixture.exe"},
            "operations": [{"name": "memory.search", "effects": "read"}],
            "lifecycle": {"health": "health", "shutdown": "shutdown"}
        }
    });
    std::fs::write(
        module_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    conn.execute(
        "INSERT INTO workspaces(id, name, root_path, created_at, config_json)
         VALUES (?1, 'fixture', ?2, ?3, '{}')",
        rusqlite::params![FIXTURE_WORKSPACE, home.to_string_lossy(), now],
    )?;
    conn.execute(
        "INSERT INTO threads(id, workspace_id, title, created_at)
         VALUES (?1, ?2, 'hilo previo', ?3)",
        rusqlite::params![FIXTURE_THREAD, FIXTURE_WORKSPACE, now],
    )?;
    conn.execute(
        "INSERT INTO branches(id, thread_id, name, head_sequence, created_at)
         VALUES (?1, ?2, 'main', 0, ?3)",
        rusqlite::params![FIXTURE_BRANCH, FIXTURE_THREAD, now],
    )?;

    let mut prev = format!("sha256:{}", "0".repeat(64));
    let mut head = String::new();
    for (seq, kind, payload) in [
        (1i64, "run.started", json!({"prompt": "migrar"})),
        (2i64, "run.completed", json!({"state": "completed"})),
        (
            3i64,
            "memory.committed",
            json!({"memory_id": FIXTURE_MEMORY_ACTIVE}),
        ),
    ] {
        let mut ev = Event {
            event_id: format!("evt_fixture_{seq}"),
            workspace_id: FIXTURE_WORKSPACE.into(),
            thread_id: Some(FIXTURE_THREAD.into()),
            branch_id: Some(FIXTURE_BRANCH.into()),
            run_id: Some(FIXTURE_RUN.into()),
            causation_id: None,
            correlation_id: None,
            idempotency_key: None,
            sequence: seq,
            kind: kind.into(),
            schema_version: EVENT_SCHEMA_VERSION,
            created_at: now.clone(),
            actor: json!({"kind": "user", "id": "fixture"}),
            payload,
            artifacts: vec![],
            integrity: Integrity {
                canonicalization: "RFC8785".into(),
                hash_alg: "sha256".into(),
                previous_hash: prev.clone(),
                event_hash: String::new(),
            },
        };
        ev.seal();
        prev = ev.integrity.event_hash.clone();
        head = ev.event_id.clone();
        conn.execute(
            "INSERT INTO events(event_id, workspace_id, thread_id, branch_id, run_id, sequence,
                                type, schema_version, created_at, actor_json, payload_json,
                                artifacts_json, causation_id, correlation_id, idempotency_key,
                                prev_hash, event_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '[]', NULL, NULL, NULL, ?12, ?13)",
            rusqlite::params![
                ev.event_id,
                ev.workspace_id,
                ev.thread_id,
                ev.branch_id,
                ev.run_id,
                ev.sequence,
                ev.kind,
                ev.schema_version,
                ev.created_at,
                hli_core::canon::canonical_json(&ev.actor),
                hli_core::canon::canonical_json(&ev.payload),
                ev.integrity.previous_hash,
                ev.integrity.event_hash
            ],
        )?;
    }
    conn.execute(
        "UPDATE branches SET head_event_id = ?1, head_sequence = 3 WHERE id = ?2",
        rusqlite::params![head, FIXTURE_BRANCH],
    )?;

    let snapshot_path = home.join("snapshots").join("snapshot-3.json");
    let snapshot_body = json!({"branch": FIXTURE_BRANCH, "at_sequence": 3});
    std::fs::write(&snapshot_path, serde_json::to_vec_pretty(&snapshot_body)?)?;
    conn.execute(
        "INSERT INTO snapshots(id, workspace_id, branch_id, at_sequence, path, content_hash, created_at)
         VALUES ('snap_fixture', ?1, ?2, 3, ?3, ?4, ?5)",
        rusqlite::params![
            FIXTURE_WORKSPACE,
            FIXTURE_BRANCH,
            snapshot_path.to_string_lossy(),
            hash_canonical(&snapshot_body),
            now
        ],
    )?;

    let receipt = json!({"provider": "sim", "model": "sim-1", "mus": 0});
    conn.execute(
        "INSERT INTO runs(id, workspace_id, thread_id, branch_id, branch_revision, state,
                          prompt, started_at, ended_at, result_json, receipt_json)
         VALUES (?1, ?2, ?3, ?4, 0, 'completed', 'migrar', ?5, ?5, '{\"text\":\"ok\"}', ?6)",
        rusqlite::params![
            FIXTURE_RUN,
            FIXTURE_WORKSPACE,
            FIXTURE_THREAD,
            FIXTURE_BRANCH,
            now,
            serde_json::to_string(&receipt)?
        ],
    )?;
    conn.execute(
        "INSERT INTO context_manifests(id, workspace_id, run_id, branch_revision, budget_tokens,
                                       estimated_tokens, content_hash, body_json, created_at)
         VALUES ('ctx_fixture', ?1, ?2, 0, 1000, 10, ?3, '{\"items\":[]}', ?4)",
        rusqlite::params![
            FIXTURE_WORKSPACE,
            FIXTURE_RUN,
            hash_canonical(&json!({"items": []})),
            now
        ],
    )?;

    let artifact_body = b"contenido de artifact previo".to_vec();
    let artifact_sha = crate::export::sha256_hex(&artifact_body);
    std::fs::write(home.join("artifacts").join(&artifact_sha), &artifact_body)?;
    conn.execute(
        "INSERT INTO artifacts(id, workspace_id, sha256, mime, bytes, source_event_id, created_at)
         VALUES (?1, ?2, ?3, 'text/plain', ?4, NULL, ?5)",
        rusqlite::params![
            FIXTURE_ARTIFACT,
            FIXTURE_WORKSPACE,
            artifact_sha,
            artifact_body.len() as i64,
            now
        ],
    )?;

    let authorship = json!({"kind": "user", "id": "fixture"});
    conn.execute(
        "INSERT INTO memories(id, workspace_id, scope, scope_id, type, status, content, observed_at,
                              valid_from, superseded_at, source_authority, supersedes_json,
                              contradicts_json, sensitivity, authorship_json, source_events, created_at)
         VALUES (?1, ?2, 'workspace', NULL, 'fact', 'superseded', 'dato antiguo', ?3, ?3, ?3,
                 'user', '[]', '[]', 'normal', ?4, '[]', ?3)",
        rusqlite::params![
            FIXTURE_MEMORY_SUPERSEDED,
            FIXTURE_WORKSPACE,
            now,
            serde_json::to_string(&authorship)?
        ],
    )?;
    conn.execute(
        "INSERT INTO memories(id, workspace_id, scope, scope_id, type, status, content, observed_at,
                              valid_from, superseded_at, source_authority, supersedes_json,
                              contradicts_json, sensitivity, authorship_json, source_events, created_at)
         VALUES (?1, ?2, 'workspace', NULL, 'fact', 'active', 'dato vigente', ?3, ?3, NULL,
                 'user', ?4, '[]', 'normal', ?5, '[]', ?3)",
        rusqlite::params![
            FIXTURE_MEMORY_ACTIVE,
            FIXTURE_WORKSPACE,
            now,
            serde_json::to_string(&json!([FIXTURE_MEMORY_SUPERSEDED]))?,
            serde_json::to_string(&authorship)?
        ],
    )?;
    conn.execute(
        "INSERT INTO memories_fts(content, mem_id, workspace_id) VALUES
         ('dato antiguo', ?1, ?2), ('dato vigente', ?3, ?2)",
        rusqlite::params![
            FIXTURE_MEMORY_SUPERSEDED,
            FIXTURE_WORKSPACE,
            FIXTURE_MEMORY_ACTIVE
        ],
    )?;

    conn.execute(
        "INSERT INTO modules(name, version, path, manifest_json, checksum, installed_at)
         VALUES (?1, '0.1.0', ?2, ?3, ?4, ?5)",
        rusqlite::params![
            FIXTURE_MODULE,
            module_dir.to_string_lossy(),
            serde_json::to_string(&manifest)?,
            hash_canonical(&manifest),
            now
        ],
    )?;
    conn.execute(
        "INSERT INTO mch_profiles(name, module_name, domain, manifest_json, created_at)
         VALUES (?1, ?2, 'memory', ?3, ?4)",
        rusqlite::params![
            FIXTURE_MCH,
            FIXTURE_MODULE,
            serde_json::to_string(&manifest)?,
            now
        ],
    )?;
    conn.execute(
        "INSERT INTO mch_instances(name, profile, workspace_id, state, pid, created_at)
         VALUES (?1, ?1, ?2, 'stopped', NULL, ?3)",
        rusqlite::params![FIXTURE_MCH, FIXTURE_WORKSPACE, now],
    )?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(conn);
    Ok(())
}
