//! Pruebas de extremo a extremo de HLI 0.2: run, ContextManifest, proveedor
//! determinista, receipts, permisos y recuperación tras reinicio.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn hli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli"))
}

fn daemon_bin() -> PathBuf {
    hli_bin().parent().unwrap().join(if cfg!(windows) {
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
        let base = std::env::temp_dir().join(format!("hli-02-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let root = base.join("root");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        assert!(
            daemon_bin().exists(),
            "falta {:?}: ejecuta `cargo build --workspace`",
            daemon_bin()
        );
        Self {
            home,
            root,
            child: None,
        }
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(hli_bin())
            .args(args)
            .env("HLI_HOME", &self.home)
            .output()
            .expect("ejecutar hli");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    fn ok(&self, args: &[&str]) -> String {
        let (code, stdout, stderr) = self.run(args);
        assert_eq!(
            code, 0,
            "falló {:?}\nstdout: {stdout}\nstderr: {stderr}",
            args
        );
        stdout
    }

    fn json(&self, args: &[&str]) -> Value {
        let stdout = self.ok(args);
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("json inválido en {:?}: {e}\n{stdout}", args))
    }

    fn start_daemon(&mut self) {
        let child = Command::new(daemon_bin())
            .env("HLI_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("arrancar hli-daemon");
        self.child = Some(child);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(80));
            let (code, stdout, _) = self.run(&["daemon", "status"]);
            if code == 0
                && let Ok(v) = serde_json::from_str::<Value>(&stdout)
                && v["running"].as_bool() == Some(true)
            {
                return;
            }
        }
        panic!("el daemon no arrancó en 10 s");
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
        ]);
        let ws_id = ws["id"].as_str().unwrap().to_string();
        let th = self.json(&["thread", "create", "research", "--workspace", &ws_id]);
        let th_id = th["id"].as_str().unwrap().to_string();
        let br = self.json(&["branch", "create", "main", "--thread", &th_id]);
        let br_id = br["id"].as_str().unwrap().to_string();
        (ws_id, th_id, br_id)
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        if let Some(parent) = self.home.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}

#[test]
fn run_manifest_receipt_and_restart() {
    let mut ctx = Ctx::new("gate");
    ctx.start_daemon();
    let (ws, th, br) = ctx.seed();

    let run = ctx.json(&[
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
    let run_id = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["state"].as_str(), Some("completed"));
    assert_eq!(run["branch_revision"].as_i64(), Some(3));

    let got = ctx.json(&["run", "get", &run_id]);
    assert_eq!(got["run"]["state"].as_str(), Some("completed"));
    let manifest = &got["manifest"];
    assert!(
        manifest["manifest_id"]
            .as_str()
            .unwrap()
            .starts_with("ctx_")
    );
    assert!(
        manifest["content_hash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(manifest["run_id"].as_str().unwrap(), run_id);
    assert!(manifest["recent_events"].as_array().unwrap().len() >= 2);

    let receipt = &got["run"]["receipt"];
    assert_eq!(receipt["provider"].as_str(), Some("sim"));
    assert_eq!(
        receipt["context_manifest"].as_str(),
        manifest["manifest_id"].as_str()
    );
    assert!(receipt["effective_permissions"]["filesystem.write"].as_str() == Some("ask"));

    // El manifiesto se emite antes de model.requested.
    let events = ctx.json(&["event", "list", "--workspace", &ws]);
    let arr = events.as_array().unwrap();
    let m = arr
        .iter()
        .find(|e| e["type"] == "context.manifest.created")
        .unwrap();
    let r = arr.iter().find(|e| e["type"] == "model.requested").unwrap();
    assert!(m["sequence"].as_i64() < r["sequence"].as_i64());
    assert_eq!(
        m["payload"]["manifest_id"].as_str(),
        Some(manifest["manifest_id"].as_str().unwrap())
    );

    let hash_before = manifest["content_hash"].as_str().unwrap().to_string();
    ctx.stop();
    ctx.start_daemon();

    let after = ctx.json(&["run", "get", &run_id]);
    assert_eq!(after["run"]["state"].as_str(), Some("completed"));
    assert_eq!(
        after["manifest"]["content_hash"].as_str().unwrap(),
        hash_before
    );

    let verify = ctx.json(&["verify", "--workspace", &ws]);
    assert_eq!(verify["ok"].as_bool(), Some(true));
}

#[test]
fn tool_read_is_allowed() {
    let mut ctx = Ctx::new("read");
    ctx.start_daemon();
    let (ws, th, br) = ctx.seed();
    std::fs::write(ctx.root.join("note.txt"), b"contenido").unwrap();

    let run = ctx.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "tool:read note.txt",
    ]);
    assert_eq!(run["state"].as_str(), Some("completed"));
    assert_eq!(
        run["result"]["observation"]["status"].as_str(),
        Some("success")
    );
    assert_eq!(
        run["result"]["observation"]["stdout"].as_str(),
        Some("contenido")
    );
}

#[test]
fn tool_write_requires_approval() {
    let mut ctx = Ctx::new("write");
    ctx.start_daemon();
    let (ws, th, br) = ctx.seed();

    let run = ctx.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "tool:write out.txt hola mundo",
    ]);
    let run_id = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["state"].as_str(), Some("waiting_approval"));
    assert!(!ctx.root.join("out.txt").exists());

    let approved = ctx.json(&["run", "approve", &run_id, "--scope", "once"]);
    assert_eq!(approved["run"]["state"].as_str(), Some("running"));
    // El plan autoriza el efecto; la invocación concreta pide su propio permiso.
    let waiting = ctx.json(&["run", "wait", &run_id, "--timeout-secs", "5"]);
    assert_eq!(waiting["run"]["state"].as_str(), Some("waiting_approval"));
    assert_eq!(waiting["run"]["approval_kind"].as_str(), Some("tool"));
    let pending = ctx.json(&[
        "permission",
        "list",
        "--run",
        &run_id,
        "--status",
        "pending",
    ]);
    let req = pending[0]["request_id"].as_str().unwrap().to_string();
    ctx.json(&["permission", "approve", &req, "--scope", "once"]);
    let done = ctx.json(&["run", "wait", &run_id, "--timeout-secs", "10"]);
    assert_eq!(done["run"]["state"].as_str(), Some("completed"));
    assert_eq!(
        std::fs::read_to_string(ctx.root.join("out.txt")).unwrap(),
        "hola mundo"
    );
}

#[test]
fn tool_shell_is_denied() {
    let mut ctx = Ctx::new("deny");
    ctx.start_daemon();
    let (ws, th, br) = ctx.seed();

    let run = ctx.json(&[
        "run",
        "start",
        "--workspace",
        &ws,
        "--thread",
        &th,
        "--branch",
        &br,
        "--prompt",
        "tool:shell whoami",
    ]);
    assert_eq!(run["state"].as_str(), Some("failed"));
    assert!(run["result"]["error"].as_str().is_some());
}
