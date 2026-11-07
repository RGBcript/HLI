//! Regresión: error del proveedor deja el run en failed con receipt.

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

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}
impl Ctx {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("hli-08-{}", std::process::id()));
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
fn provider_error_marks_run_failed() {
    let mut c = Ctx::new();
    c.start();
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
        "--provider",
        "ollama",
        "--base-url",
        "http://127.0.0.1:9",
        "--model",
        "x",
        "--timeout-secs",
        "2",
    ]);
    assert_eq!(run["state"].as_str(), Some("failed"));
    assert!(run["result"]["error"].as_str().is_some());
    assert_eq!(run["receipt"]["provider"].as_str(), Some("ollama"));
    let got = c.json(&["run", "get", run["id"].as_str().unwrap()]);
    assert_eq!(got["run"]["state"].as_str(), Some("failed"));
}
