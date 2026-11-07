//! Runs asíncronos: estado, cancelación, timeout y recuperación.

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
static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}
impl Ctx {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("hli-10-{}-{n}", std::process::id()));
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
            if let Ok(v) = serde_json::from_str::<Value>(&self.run(&["daemon", "status"]).1) {
                if v["running"].as_bool() == Some(true) {
                    return;
                }
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
    fn ids(&self) -> (String, String, String) {
        let ws = self.json(&["workspace", "create", "demo"])["id"]
            .as_str()
            .unwrap()
            .to_string();
        let th = self.json(&["thread", "create", "t", "--workspace", &ws])["id"]
            .as_str()
            .unwrap()
            .to_string();
        let br = self.json(&["branch", "create", "main", "--thread", &th])["id"]
            .as_str()
            .unwrap()
            .to_string();
        (ws, th, br)
    }
}
impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn async_start_returns_persisted_run() {
    let mut c = Ctx::new();
    c.start();
    let (ws, th, br) = c.ids();
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
        "--async",
        "--sim-delay-ms",
        "400",
    ]);
    let id = run["id"].as_str().unwrap();
    assert!(matches!(
        run["state"].as_str(),
        Some("queued") | Some("running")
    ));
    let st = c.json(&["run", "status", id]);
    assert_eq!(st["run"]["id"].as_str(), Some(id));
    let w = c.json(&["run", "wait", id, "--timeout-secs", "5"]);
    assert_eq!(w["run"]["state"].as_str(), Some("completed"));
    assert!(!w["run"]["receipt"].is_null());
}

#[test]
fn cancel_running_run_and_restart() {
    let mut c = Ctx::new();
    c.start();
    let (ws, th, br) = c.ids();
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
        "--async",
        "--sim-delay-ms",
        "3000",
    ]);
    let id = run["id"].as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_millis(300));
    let st = c.json(&["run", "status", &id]);
    assert_eq!(st["run"]["state"].as_str(), Some("running"));
    let cr = c.json(&["run", "cancel", &id]);
    assert!(matches!(
        cr["run"]["state"].as_str(),
        Some("cancelling") | Some("cancelled")
    ));
    let w = c.json(&["run", "wait", &id, "--timeout-secs", "6"]);
    assert_eq!(w["run"]["state"].as_str(), Some("cancelled"));
    assert!(!w["run"]["receipt"].is_null());
    let again = c.json(&["run", "cancel", &id]);
    assert_eq!(again["idempotent"].as_bool(), Some(true));
    c.stop();
    c.start();
    let got = c.json(&["run", "get", &id]);
    assert_eq!(got["run"]["state"].as_str(), Some("cancelled"));
    assert!(!got["run"]["receipt"].is_null());
}

#[test]
fn timeout_marks_run_timed_out() {
    let mut c = Ctx::new();
    c.start();
    let (ws, th, br) = c.ids();
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
        "--async",
        "--sim-delay-ms",
        "2500",
        "--timeout-secs",
        "1",
    ]);
    let id = run["id"].as_str().unwrap();
    let w = c.json(&["run", "wait", id, "--timeout-secs", "6"]);
    assert_eq!(w["run"]["state"].as_str(), Some("timed_out"));
}

#[test]
fn daemon_restart_marks_running_interrupted() {
    let mut c = Ctx::new();
    c.start();
    let (ws, th, br) = c.ids();
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
        "--async",
        "--sim-delay-ms",
        "5000",
    ]);
    let id = run["id"].as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_millis(300));
    c.stop();
    c.start();
    let got = c.json(&["run", "get", &id]);
    assert_eq!(got["run"]["state"].as_str(), Some("interrupted"));
}
