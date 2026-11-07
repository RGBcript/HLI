//! CLI de migraciones y estado de firma tras migrar.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use hli_store::fixtures;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn hli() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli"))
}

fn daemon_bin() -> PathBuf {
    hli().parent().unwrap().join(if cfg!(windows) {
        "hli-daemon.exe"
    } else {
        "hli-daemon"
    })
}

static SEQ: AtomicUsize = AtomicUsize::new(0);

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}

impl Ctx {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let home =
            std::env::temp_dir().join(format!("hli-migrate-cli-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        Self { home, child: None }
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = Command::new(hli())
            .args(args)
            .env("HLI_HOME", &self.home)
            .output()
            .unwrap();
        (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into(),
            String::from_utf8_lossy(&o.stderr).into(),
        )
    }

    fn json(&self, args: &[&str]) -> Value {
        let (c, o, e) = self.run(args);
        assert_eq!(c, 0, "{e}{o}");
        serde_json::from_str(&o).unwrap()
    }

    fn start(&mut self) {
        let child = Command::new(daemon_bin())
            .env("HLI_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(v) = serde_json::from_str::<Value>(&self.run(&["daemon", "status"]).1) {
                if v["running"].as_bool() == Some(true) {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("daemon no arrancó");
    }

    fn stop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("sha256:{}", hex::encode(h.finalize()))
}

fn container(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"HLIAR1\n".to_vec();
    for (path, bytes) in files {
        out.extend_from_slice(format!("FILE {} {}\n", bytes.len(), path).as_bytes());
        out.extend_from_slice(bytes);
        out.push(b'\n');
    }
    out
}

fn build_package(home: &Path, format_version: u64) -> PathBuf {
    let manifest = json!({
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": "migration-cli", "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "format_version": format_version,
            "runtime": {"kind": "subprocess", "entry": "bin/app.bin"},
            "operations": [{"name": "memory.search", "effects": "read"}],
            "lifecycle": {"health": "health", "shutdown": "shutdown"}
        }
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let bin = b"fake-module".to_vec();
    let checks = json!({
        "manifest.json": sha256(&manifest_bytes),
        "bin/app.bin": sha256(&bin)
    });
    let path = home.join("pkg.hlimod");
    std::fs::write(
        &path,
        container(&[
            ("manifest.json", manifest_bytes),
            ("bin/app.bin", bin),
            ("checksums.json", serde_json::to_vec(&checks).unwrap()),
        ]),
    )
    .unwrap();
    path
}

#[test]
fn migration_cli_status_backup_apply_verify() {
    let ctx = Ctx::new("lifecycle");
    fixtures::create_previous_fixture(&ctx.home).unwrap();

    let status = ctx.json(&["migrate", "status"]);
    assert_eq!(status["current_version"].as_i64(), Some(5));
    assert_eq!(status["target_version"].as_i64(), Some(7));
    assert_eq!(status["pending"].as_array().map(Vec::len), Some(2));
    assert_eq!(status["backup_required"].as_bool(), Some(true));
    assert_eq!(status["compatible"].as_bool(), Some(true));

    let backup = ctx.json(&["migrate", "backup"]);
    assert!(PathBuf::from(backup["backup"].as_str().unwrap()).exists());

    let applied = ctx.json(&["migrate", "apply"]);
    assert_eq!(applied["from_version"].as_i64(), Some(5));
    assert_eq!(applied["to_version"].as_i64(), Some(7));
    assert_eq!(applied["applied"].as_array().map(Vec::len), Some(2));
    assert!(applied["backup"].as_str().is_some());

    let verify = ctx.json(&["migrate", "verify"]);
    assert_eq!(verify["schema_ok"].as_bool(), Some(true));
    assert_eq!(verify["integrity_ok"].as_bool(), Some(true));
    assert_eq!(verify["chain_ok"].as_bool(), Some(true));
    assert_eq!(verify["events_checked"].as_i64(), Some(3));

    let status = ctx.json(&["migrate", "status"]);
    assert_eq!(status["current_version"].as_i64(), Some(7));
    assert_eq!(status["pending"].as_array().map(Vec::len), Some(0));
    assert_eq!(status["backup_required"].as_bool(), Some(false));
}

#[test]
fn migrated_signing_state_is_valid() {
    let ctx = Ctx::new("signing");
    fixtures::create_previous_fixture(&ctx.home).unwrap();
    ctx.json(&["migrate", "apply"]);

    let key = ctx.json(&["key", "generate", "post-migration"]);
    let key_id = key["key_id"].as_str().unwrap().to_string();
    let public = ctx.json(&["key", "export-public", &key_id]);
    let public_path = ctx.home.join("public.json");
    std::fs::write(&public_path, serde_json::to_vec(&public).unwrap()).unwrap();
    ctx.json(&["trust", "add", public_path.to_str().unwrap()]);

    let pkg = build_package(&ctx.home, 1);
    ctx.json(&["module", "sign", pkg.to_str().unwrap(), "--key", &key_id]);
    let verified = ctx.json(&["module", "verify", pkg.to_str().unwrap()]);
    assert_eq!(verified["state"].as_str(), Some("signed-trusted"));
    assert_eq!(verified["accepted"].as_bool(), Some(true));

    // La migración 0007 dejó soporte de firma y la configuración versionada.
    let keys = ctx.json(&["key", "list"]);
    assert_eq!(keys[0]["key_id"].as_str(), Some(key_id.as_str()));
}

#[test]
fn future_hlimod_format_is_rejected() {
    let mut ctx = Ctx::new("hlimod-future");
    ctx.json(&["migrate", "apply"]);
    ctx.start();
    let pkg = build_package(&ctx.home, 2);
    let (code, out, err) = ctx.run(&["module", "install", pkg.to_str().unwrap()]);
    assert_ne!(code, 0, "una versión futura no debe instalarse: {out}");
    assert!(
        err.contains("futura") || out.contains("futura"),
        "error inesperado: {err}{out}"
    );
}
