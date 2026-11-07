//! Supersesión explícita y procedencia en el receipt.

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
    root: PathBuf,
    child: Option<Child>,
}

impl Ctx {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("hli-07-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let root = base.join("root");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        Self {
            home,
            root,
            child: None,
        }
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(hli())
            .args(args)
            .env("HLI_HOME", &self.home)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into(),
            String::from_utf8_lossy(&out.stderr).into(),
        )
    }
    fn ok(&self, args: &[&str]) -> String {
        let (c, o, e) = self.run(args);
        assert_eq!(c, 0, "falló {args:?}: {e}{o}");
        o
    }
    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_str(&self.ok(args)).unwrap()
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
            if let Ok(v) = serde_json::from_str::<Value>(&self.ok(&["daemon", "status"]))
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
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
    }
    fn seed(&self) -> (String, String, String) {
        let ws = self.json(&[
            "workspace",
            "create",
            "demo",
            "--root",
            self.root.to_str().unwrap(),
        ])["id"]
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
        if let Some(p) = self.home.parent() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

#[test]
fn explicit_supersession_retires_old_memory() {
    let mut c = Ctx::new("retire");
    c.start();
    let (ws, _, _) = c.seed();
    let old = c.json(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "El límite es 100.",
    ])["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "El límite es 200.",
        "--supersedes",
        &old,
    ]);
    let m = c.json(&["memory", "get", &old]);
    assert_eq!(m["status"].as_str(), Some("superseded"));
    assert!(m["superseded_at"].as_str().is_some());
    assert!(m["valid_until"].as_str().is_some());
}

#[test]
fn superseded_memory_is_not_retrieved() {
    let mut c = Ctx::new("notret");
    c.start();
    let (ws, _, _) = c.seed();
    let old = c.json(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "La base es PostgreSQL.",
    ])["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "La base es SQLite.",
        "--supersedes",
        &old,
    ]);
    let found = c.json(&[
        "memory",
        "search",
        "--workspace",
        &ws,
        "--query",
        "base",
        "--limit",
        "10",
    ]);
    assert!(
        !found
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == old.as_str())
    );
}

#[test]
fn historical_query_can_read_superseded_memory() {
    let mut c = Ctx::new("hist");
    c.start();
    let (ws, _, _) = c.seed();
    let old = c.json(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "La base era PostgreSQL.",
    ])["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "La base es SQLite.",
        "--supersedes",
        &old,
    ]);
    let m = c.json(&["memory", "get", &old]);
    assert_eq!(m["content"].as_str(), Some("La base era PostgreSQL."));
    assert_eq!(m["status"].as_str(), Some("superseded"));
}

#[test]
fn supersession_is_atomic() {
    let mut c = Ctx::new("atomic");
    c.start();
    let (ws, _, _) = c.seed();
    let (code, _, err) = c.run(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "Nueva.",
        "--supersedes",
        "mem_inexistente",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("not_found") || err.contains("no existe"));
    let list = c.json(&["memory", "list", "--workspace", &ws]);
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[test]
fn decision_memory_does_not_require_sro() {
    let mut c = Ctx::new("dec");
    c.start();
    let (ws, _, _) = c.seed();
    let m = c.json(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "decision",
        "--content",
        "Se decidió usar SQLite WAL.",
    ]);
    assert_eq!(m["status"].as_str(), Some("active"));
    assert!(m["subject"].is_null() && m["relation"].is_null() && m["object"].is_null());
}

#[test]
fn receipt_records_used_memory_and_slice_and_excludes_candidates() {
    let mut c = Ctx::new("receipt");
    c.start();
    let (ws, th, br) = c.seed();
    for i in 0..6 {
        c.ok(&[
            "memory",
            "commit",
            "--workspace",
            &ws,
            "--type",
            "fact",
            "--content",
            &format!("Dato {i} sobre el tema."),
        ]);
    }
    let log = c.root.join("log.txt");
    std::fs::write(&log, "ERROR db timeout\n").unwrap();
    let aid = c.json(&["artifact", "put", log.to_str().unwrap(), "--workspace", &ws])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sl = c.json(&[
        "artifact",
        "slice",
        &aid,
        "--workspace",
        &ws,
        "--kind",
        "full_text_if_small",
    ])["slice_id"]
        .as_str()
        .unwrap()
        .to_string();
    let rv = c.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "Dato sobre el tema",
        "--artifact",
        &aid,
        "--top-k",
        "6",
    ]);
    let r = &rv["receipt"];
    assert!(!r["used_memory_ids"].as_array().unwrap().is_empty());
    assert!(
        r["used_slice_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == sl.as_str())
    );
    assert!(!r["used_event_ids"].as_array().unwrap().is_empty());
    assert!(r["candidate_memory_ids"].as_array().unwrap().len() >= 2);
    assert!(!r["rejected_memory_ids"].as_array().unwrap().is_empty());
}

#[test]
fn receipt_survives_restart_and_export_import() {
    let mut c = Ctx::new("persist");
    c.start();
    let (ws, th, br) = c.seed();
    c.ok(&[
        "memory",
        "commit",
        "--workspace",
        &ws,
        "--type",
        "fact",
        "--content",
        "La base es SQLite.",
    ]);
    let rv = c.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "¿Qué base es?",
        "--top-k",
        "2",
    ]);
    let id = rv["id"].as_str().unwrap().to_string();
    c.stop();
    c.start();
    let after = c.json(&["run", "get", &id]);
    assert!(
        !after["run"]["receipt"]["used_memory_ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let archive = c.home.join("ws.hliar");
    c.ok(&["export", &ws, "--out", archive.to_str().unwrap()]);
    c.stop();
    let mut c2 = Ctx::new("persist2");
    c2.start();
    c2.ok(&["import", archive.to_str().unwrap()]);
    let imported = c2.json(&["run", "get", &id]);
    assert!(
        !imported["run"]["receipt"]["used_memory_ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
