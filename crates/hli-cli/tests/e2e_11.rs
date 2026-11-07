//! Supervisor MCH persistente: reutilización de proceso, reinicio y caída.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn hli() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli"))
}
fn daemon() -> PathBuf {
    hli().parent().unwrap().join(if cfg!(windows) {
        "hli-daemon.exe"
    } else {
        "hli-daemon"
    })
}
fn module_pkg() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/hli-memory-0.9.0-beta.1.hlimod")
}
static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}
impl Ctx {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("hli-11-{}-{n}", std::process::id()));
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
    fn stop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    fn setup(&self) -> String {
        let ws = self.json(&["workspace", "create", "demo"])["id"]
            .as_str()
            .unwrap()
            .to_string();
        let pkg = module_pkg();
        if !pkg.exists() {
            return ws;
        }
        self.json(&["module", "install", pkg.to_str().unwrap()]);
        self.json(&[
            "mch",
            "create",
            "memory-main",
            "--module",
            "hli-memory",
            "--domain",
            "memory",
            "--workspace",
            &ws,
        ]);
        ws
    }
}
impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn mch_process_is_reused_across_invocations() {
    if !module_pkg().exists() {
        return;
    }
    let mut c = Ctx::new();
    c.start();
    c.setup();
    let served = c.json(&["mch", "serve", "memory-main"]);
    let pid = served["pid"].as_i64().unwrap();
    c.json(&[
        "mch",
        "invoke",
        "memory-main",
        "memory.search",
        "--arguments",
        "{\"query\":\"\"}",
    ]);
    let st = c.json(&["mch", "status", "memory-main"]);
    assert_eq!(st["process"]["pid"].as_i64(), Some(pid));
    assert_eq!(st["process"]["alive"].as_bool(), Some(true));
    let restarted = c.json(&["mch", "restart", "memory-main"]);
    assert_ne!(restarted["pid"].as_i64(), Some(pid));
    let stopped = c.json(&["mch", "stop", "memory-main"]);
    assert_eq!(stopped["state"].as_str(), Some("stopped"));
}

#[test]
fn mch_crash_does_not_crash_daemon() {
    if !module_pkg().exists() {
        return;
    }
    let mut c = Ctx::new();
    c.start();
    c.setup();
    let pid = c.json(&["mch", "serve", "memory-main"])["pid"]
        .as_i64()
        .unwrap();
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
    std::thread::sleep(Duration::from_millis(500));
    let st = c.json(&["mch", "status", "memory-main"]);
    assert_eq!(st["process"]["alive"].as_bool(), Some(false));
    let (code, _, err) = c.run(&[
        "mch",
        "invoke",
        "memory-main",
        "memory.search",
        "--arguments",
        "{\"query\":\"\"}",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("no esta vivo") || err.contains("no está vivo"));
    assert_eq!(
        c.json(&["daemon", "status"])["running"].as_bool(),
        Some(true)
    );
}
