//! Configuración pública de proveedores: roundtrip, default y secretos.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn hli() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli"))
}
static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn daemon() -> PathBuf {
    hli().parent().unwrap().join(if cfg!(windows) {
        "hli-daemon.exe"
    } else {
        "hli-daemon"
    })
}

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}
impl Ctx {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("hli-09-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        Self {
            home: base,
            child: None,
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
        let c = Command::new(daemon())
            .env("HLI_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.child = Some(c);
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(80));
            if let Ok(v) = serde_json::from_str::<Value>(&self.run(&["daemon", "status"]).1)
                && v["running"].as_bool() == Some(true)
            {
                return;
            }
        }
        panic!("daemon no arrancó");
    }
}
impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
        if let Some(mut c) = self.child.take() {
            let _ = c.wait();
        }
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn provider_config_roundtrip_and_default() {
    let mut c = Ctx::new();
    c.start();
    c.json(&[
        "provider",
        "add",
        "sim1",
        "--kind",
        "sim",
        "--base-url",
        "http://localhost",
        "--model",
        "sim-1",
    ]);
    let list = c.json(&["provider", "list"]);
    assert_eq!(list["providers"][0]["name"].as_str(), Some("sim1"));
    assert_eq!(list["providers"][0]["has_secret"].as_bool(), Some(false));
    let t = c.json(&["provider", "test", "sim1"]);
    assert_eq!(t["provider"].as_str(), Some("sim"));
    assert_eq!(t["text"].as_str(), Some("pong"));
    c.json(&["provider", "set-default", "sim1"]);
    let list = c.json(&["provider", "list"]);
    assert_eq!(list["default_provider"].as_str(), Some("sim1"));
    let ws = c.json(&["workspace", "create", "demo"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let th = c.json(&["thread", "create", "t", "--workspace", &ws])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let br = c.json(&["branch", "create", "main", "--thread", &th])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let run = c.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "hola",
    ]);
    assert_eq!(run["receipt"]["provider"].as_str(), Some("sim"));
    assert_eq!(run["state"].as_str(), Some("completed"));
}

#[test]
fn provider_config_never_stores_secret() {
    let mut c = Ctx::new();
    c.start();
    std::fs::create_dir_all(c.home.join("secrets")).unwrap();
    std::fs::write(c.home.join("secrets/tok"), "SUPERSECRET-123").unwrap();
    c.json(&[
        "provider",
        "add",
        "remote",
        "--kind",
        "openai",
        "--base-url",
        "https://api.example.com/v1",
        "--model",
        "m",
        "--secret-handle",
        "tok",
    ]);
    let toml = std::fs::read_to_string(c.home.join("providers.toml")).unwrap();
    assert!(!toml.contains("SUPERSECRET-123"));
    assert!(toml.contains("secret_handle"));
    let i = c.json(&["provider", "inspect", "remote"]);
    assert_eq!(i["has_secret"].as_bool(), Some(true));
}
