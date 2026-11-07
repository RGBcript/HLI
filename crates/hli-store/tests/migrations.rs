//! Migraciones versionadas: orden, backup, rollback, fixture previa y datos.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use hli_store::fixtures::*;
use hli_store::migrations;
use hli_store::{Reader, Store};
use rusqlite::Connection;

static SEQ: AtomicUsize = AtomicUsize::new(0);

struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("hli-migrate-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn db(&self) -> PathBuf {
        self.0.join("hli.db")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn reader(home: &Home) -> Reader {
    Reader::open(&home.db(), home.path().join("artifacts")).unwrap()
}

fn versions(home: &Home) -> Vec<i64> {
    let conn = Connection::open(home.db()).unwrap();
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<Vec<i64>, _>>()
        .unwrap()
}

#[test]
fn migration_applies_in_order() {
    let home = Home::new("order");
    let mut conn = Connection::open(home.db()).unwrap();
    let report = migrations::apply(&mut conn, migrations::DATABASE_SCHEMA_VERSION).unwrap();
    assert_eq!(report.from_version, 0);
    assert_eq!(report.applied, vec![1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(versions(&home), vec![1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn migration_is_not_reapplied() {
    let home = Home::new("idempotent");
    let mut conn = Connection::open(home.db()).unwrap();
    migrations::apply(&mut conn, migrations::DATABASE_SCHEMA_VERSION).unwrap();
    let again = migrations::apply(&mut conn, migrations::DATABASE_SCHEMA_VERSION).unwrap();
    assert!(again.applied.is_empty());
    assert_eq!(versions(&home).len(), 7);
}

#[test]
fn failed_migration_rolls_back() {
    let home = Home::new("rollback");
    let mut conn = Connection::open(home.db()).unwrap();
    let err = migrations::apply_with_failure(&mut conn, 7, Some(3)).unwrap_err();
    assert_eq!(err.code, "migration_failed");
    assert_eq!(migrations::current_version(&conn).unwrap(), 2);
    let memories: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        memories, 0,
        "la tabla de la migración fallida no debe existir"
    );
    // La base original sigue utilizable.
    conn.execute(
        "INSERT INTO workspaces(id, name, root_path, created_at, config_json)
         VALUES ('ws_ok', 'ok', '.', '2026-01-01T00:00:00.000Z', '{}')",
        [],
    )
    .unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn unsupported_future_schema_is_rejected() {
    let home = Home::new("future");
    let mut conn = Connection::open(home.db()).unwrap();
    migrations::apply(&mut conn, 7).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations(version, applied_at, checksum)
         VALUES (99, '2026-01-01T00:00:00.000Z', 'future')",
        [],
    )
    .unwrap();
    let err = migrations::apply(&mut conn, 7).unwrap_err();
    assert_eq!(err.code, "conflict");
    drop(conn);
    let err = Store::open(home.path())
        .err()
        .expect("debe rechazar schema futuro");
    assert!(err.message.contains("futuro"));
}

#[test]
fn backup_is_created_before_apply() {
    let home = Home::new("backup");
    create_previous_fixture(home.path()).unwrap();
    let backups = home.path().join("backups");
    let manual = migrations::backup_path(&home.db(), &backups).unwrap();
    assert!(manual.exists());
    let conn = Connection::open(&manual).unwrap();
    assert_eq!(migrations::current_version(&conn).unwrap(), 5);
    drop(conn);

    let (report, backup) = migrations::apply_path(&home.db(), &backups, true).unwrap();
    assert_eq!(report.from_version, 5);
    assert_eq!(report.to_version, 7);
    let backup = backup.expect("apply debe crear backup");
    assert!(backup.exists());
    let conn = Connection::open(&backup).unwrap();
    assert_eq!(migrations::current_version(&conn).unwrap(), 5);
}

#[test]
fn migrated_events_preserve_chain() {
    let home = Home::new("chain");
    create_previous_fixture(home.path()).unwrap();
    let before = reader(&home).verify(FIXTURE_WORKSPACE).unwrap();
    assert!(before.ok);
    assert_eq!(before.checked, 3);

    let store = Store::open(home.path()).unwrap();
    let (_, r) = store.start().unwrap();
    let after = r.verify(FIXTURE_WORKSPACE).unwrap();
    assert!(after.ok);
    assert_eq!(after.checked, before.checked);
    assert_eq!(after.chain_head, before.chain_head);
    let events = r.events(FIXTURE_WORKSPACE, Some(FIXTURE_BRANCH)).unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.verify()));
}

#[test]
fn migrated_artifacts_preserve_hashes() {
    let home = Home::new("artifact");
    create_previous_fixture(home.path()).unwrap();
    Store::open(home.path()).unwrap();
    let r = reader(&home);
    let artifact = r.artifact(FIXTURE_ARTIFACT).unwrap();
    let bytes = std::fs::read(&artifact.path).unwrap();
    assert_eq!(
        artifact.sha256,
        format!("sha256:{}", hli_store::export::sha256_hex(&bytes))
    );
    assert_eq!(artifact.bytes as usize, bytes.len());
    assert!(artifact.sha256.starts_with("sha256:"));
}

#[test]
fn migrated_memory_preserves_supersession() {
    let home = Home::new("memory");
    create_previous_fixture(home.path()).unwrap();
    Store::open(home.path()).unwrap();
    let r = reader(&home);
    let active = r.memory(FIXTURE_MEMORY_ACTIVE).unwrap();
    let old = r.memory(FIXTURE_MEMORY_SUPERSEDED).unwrap();
    assert_eq!(active.status, "active");
    assert_eq!(old.status, "superseded");
    assert!(active
        .supersedes
        .contains(&FIXTURE_MEMORY_SUPERSEDED.to_string()));
    assert!(old.superseded_at.is_some());
}

#[test]
fn migrated_mch_preserves_autostart() {
    let home = Home::new("mch");
    create_previous_fixture(home.path()).unwrap();
    let store = Store::open(home.path()).unwrap();
    let (w, r) = store.start().unwrap();
    let before = r.mch(FIXTURE_MCH).unwrap();
    assert_eq!(before["autostart"].as_bool(), Some(false));
    w.mch_set_autostart(FIXTURE_MCH, true).unwrap();
    let after = r.mch(FIXTURE_MCH).unwrap();
    assert_eq!(after["autostart"].as_bool(), Some(true));
}

#[test]
fn hliar_future_version_is_rejected() {
    let raw = b"HLIAR2\nFILE 2 a\nhi\n";
    let err = hli_store::export::read_container(raw).unwrap_err();
    assert!(err.message.contains("futura"), "{}", err.message);
}

#[test]
fn hliar_unknown_version_is_rejected() {
    let raw = b"HLIARx\nFILE 2 a\nhi\n";
    let err = hli_store::export::read_container(raw).unwrap_err();
    assert!(err.message.contains("desconocida"), "{}", err.message);
}

#[test]
fn daemon_refuses_partial_schema() {
    let home = Home::new("partial");
    let conn = Connection::open(home.db()).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL, checksum TEXT NOT NULL);
         INSERT INTO schema_migrations VALUES (7, '2026-01-01T00:00:00.000Z', 'parcial');",
    )
    .unwrap();
    drop(conn);
    let err = Store::open(home.path())
        .err()
        .expect("debe rechazar schema parcial");
    assert!(
        err.message.contains("parcial") || err.message.contains("falta"),
        "mensaje inesperado: {}",
        err.message
    );
}
