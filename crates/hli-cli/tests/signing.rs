//! La clave privada nunca debe aparecer en eventos, logs ni salidas.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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
    log: PathBuf,
}

impl Ctx {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let home =
            std::env::temp_dir().join(format!("hli-signing-cli-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let log = home.join("daemon-stderr.log");
        Self {
            home,
            child: None,
            log,
        }
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
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .unwrap();
        let child = Command::new(daemon_bin())
            .env("HLI_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
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
        std::thread::sleep(Duration::from_millis(150));
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

fn build_package(home: &Path) -> PathBuf {
    let manifest = json!({
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": "signing-cli", "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "runtime": {"kind": "subprocess", "entry": "bin/app.bin"},
            "operations": [{"name": "memory.search", "effects": "read"}],
            "lifecycle": {"health": "health", "shutdown": "shutdown"}
        }
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let bin = b"fake-module-binary".to_vec();
    let checks = json!({
        "manifest.json": sha256(&manifest_bytes),
        "bin/app.bin": sha256(&bin)
    });
    let raw = container(&[
        ("manifest.json", manifest_bytes),
        ("bin/app.bin", bin),
        ("checksums.json", serde_json::to_vec(&checks).unwrap()),
    ]);
    let path = home.join("signed.hlimod");
    std::fs::write(&path, raw).unwrap();
    path
}

fn prepare_key(ctx: &Ctx) -> (String, String) {
    let rec = ctx.json(&["key", "generate", "leak"]);
    let key_id = rec["key_id"].as_str().unwrap().to_string();
    let private =
        std::fs::read_to_string(ctx.home.join("keys").join(format!("{key_id}.key"))).unwrap();
    let secret = private
        .trim()
        .strip_prefix("base64:")
        .unwrap_or(private.trim())
        .to_string();
    let public = ctx.json(&["key", "export-public", &key_id]);
    let pub_path = ctx.home.join("public.json");
    std::fs::write(&pub_path, serde_json::to_vec_pretty(&public).unwrap()).unwrap();
    ctx.json(&["trust", "add", pub_path.to_str().unwrap()]);
    (key_id, secret)
}

fn assert_absent(label: &str, text: &str, secret: &str) {
    assert!(
        !text.contains(secret),
        "la clave privada apareció en {label}"
    );
}

fn scan_home_except_key(ctx: &Ctx, secret: &str) {
    fn walk(dir: &Path, skip: &Path, secret: &str) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path == skip {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                walk(&path, skip, secret);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                assert!(
                    !text.contains(secret),
                    "la clave privada apareció en {}",
                    path.display()
                );
            }
        }
    }
    let key_dir = ctx.home.join("keys");
    for entry in std::fs::read_dir(&key_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("key") {
            walk(&ctx.home, &path, secret);
        }
    }
}

#[test]
fn private_key_never_appears_in_events() {
    let mut ctx = Ctx::new("events");
    let (key_id, secret) = prepare_key(&ctx);
    let pkg = build_package(&ctx.home);
    ctx.json(&["module", "sign", pkg.to_str().unwrap(), "--key", &key_id]);

    ctx.start();
    let installed = ctx.json(&["module", "install", pkg.to_str().unwrap()]);
    assert_eq!(
        installed["signature_state"].as_str(),
        Some("signed-trusted")
    );
    let ws = ctx.json(&["workspace", "create", "demo"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    ctx.json(&[
        "event",
        "append",
        "--type",
        "test",
        "--workspace",
        &ws,
        "--payload",
        "{\"k\":\"v\"}",
    ]);
    let events = ctx.run(&["event", "list", "--workspace", &ws]).1;
    assert!(events.contains("\"test\""));
    assert_absent("event list", &events, &secret);
    assert_absent("key list", &ctx.run(&["key", "list"]).1, &secret);
    assert_absent("module list", &ctx.run(&["module", "list"]).1, &secret);
    assert_absent("trust list", &ctx.run(&["trust", "list"]).1, &secret);
    assert_absent(
        "module verify",
        &ctx.run(&["module", "verify", pkg.to_str().unwrap()]).1,
        &secret,
    );
    assert_absent(
        "daemon stderr",
        &std::fs::read_to_string(&ctx.log).unwrap_or_default(),
        &secret,
    );
}

#[test]
fn private_key_never_appears_in_logs() {
    let mut ctx = Ctx::new("logs");
    let (key_id, secret) = prepare_key(&ctx);
    let pkg = build_package(&ctx.home);
    ctx.json(&["module", "sign", pkg.to_str().unwrap(), "--key", &key_id]);

    ctx.start();
    ctx.json(&["module", "install", pkg.to_str().unwrap()]);
    let verified = ctx.json(&["module", "verify", pkg.to_str().unwrap()]);
    assert_eq!(verified["state"].as_str(), Some("signed-trusted"));
    let keys = ctx.json(&["key", "list"]);
    assert_eq!(keys[0]["key_id"].as_str(), Some(key_id.as_str()));

    let daemon_log = std::fs::read_to_string(&ctx.log).unwrap_or_default();
    let cli_log = std::fs::read_to_string(ctx.home.join("daemon.log")).unwrap_or_default();
    assert_absent("daemon stderr", &daemon_log, &secret);
    assert_absent("daemon.log", &cli_log, &secret);
    scan_home_except_key(&ctx, &secret);
}
