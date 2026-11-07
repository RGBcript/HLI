//! Pruebas de extremo a extremo de HLI 0.1.
//!
//! El daemon se arranca como proceso independiente desde el propio test (no a
//! través del CLI), para que el reinicio sea real y para no heredar los pipes
//! del proceso de prueba. Cada test usa su propio HLI_HOME.

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
    child: Option<Child>,
}

impl Ctx {
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("hli-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert!(
            daemon_bin().exists(),
            "falta {:?}: ejecuta `cargo build --workspace`",
            daemon_bin()
        );
        Self { home, child: None }
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
        panic!("el daemon no arrancó en 10 s (home {:?})", self.home);
    }

    fn stop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
        if let Some(mut c) = self.child.take() {
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn lifecycle_survives_daemon_restart() {
    let mut ctx = Ctx::new("restart");
    ctx.start_daemon();

    let ws = ctx.json(&["workspace", "create", "demo"]);
    let ws_id = ws["id"].as_str().unwrap().to_string();
    let th = ctx.json(&["thread", "create", "research", "--workspace", &ws_id]);
    let th_id = th["id"].as_str().unwrap().to_string();
    let ev = ctx.json(&[
        "event",
        "append",
        "--type",
        "test.recorded",
        "--payload",
        r#"{"n":1}"#,
        "--workspace",
        &ws_id,
        "--thread",
        &th_id,
    ]);
    let ev_id = ev["event_id"].as_str().unwrap().to_string();
    let br = ctx.json(&["branch", "create", "alternative", "--from", &ev_id]);
    let br_id = br["id"].as_str().unwrap().to_string();

    let before = ctx.json(&["branch", "show", &br_id]);
    let before_seq = before["branch"]["head_sequence"].as_i64().unwrap();
    let before_head = before["chain_head"].as_str().unwrap().to_string();

    ctx.stop();
    ctx.start_daemon();

    let after = ctx.json(&["branch", "show", &br_id]);
    assert_eq!(
        after["branch"]["head_sequence"].as_i64().unwrap(),
        before_seq
    );
    assert_eq!(after["chain_head"].as_str().unwrap(), before_head);
    assert_eq!(after["branch"]["name"].as_str().unwrap(), "alternative");
    let snap_id = after["branch"]["snapshot_id"]
        .as_str()
        .expect("snapshot_id");
    assert!(snap_id.starts_with("snap_"));
    let seq = after["branch"]["head_sequence"].as_i64().unwrap();
    assert!(
        ctx.home
            .join("snapshots")
            .join(&br_id)
            .join(format!("{seq}.json"))
            .exists()
    );

    let verify = ctx.json(&["verify", "--workspace", &ws_id]);
    assert_eq!(verify["ok"].as_bool(), Some(true));
    assert_eq!(verify["checked"].as_i64(), Some(4));
}

#[test]
fn export_import_roundtrip_is_verifiable() {
    let mut ctx = Ctx::new("export");
    ctx.start_daemon();
    let ws = ctx.json(&["workspace", "create", "demo"]);
    let ws_id = ws["id"].as_str().unwrap().to_string();
    let th = ctx.json(&["thread", "create", "research", "--workspace", &ws_id]);
    let th_id = th["id"].as_str().unwrap().to_string();
    let ev = ctx.json(&[
        "event",
        "append",
        "--type",
        "test.recorded",
        "--payload",
        r#"{"n":2}"#,
        "--workspace",
        &ws_id,
        "--thread",
        &th_id,
    ]);
    let ev_id = ev["event_id"].as_str().unwrap().to_string();
    let br = ctx.json(&[
        "branch", "create", "alt", "--thread", &th_id, "--from", &ev_id,
    ]);
    let br_id = br["id"].as_str().unwrap().to_string();

    let note = ctx.home.join("note.txt");
    std::fs::write(&note, b"hola hli").unwrap();
    let art = ctx.json(&[
        "artifact",
        "put",
        note.to_str().unwrap(),
        "--workspace",
        &ws_id,
    ]);
    assert!(art["sha256"].as_str().unwrap().starts_with("sha256:"));

    let archive = ctx.home.join("demo.hliar");
    let export = ctx.json(&["export", &ws_id, "--out", archive.to_str().unwrap()]);
    let chain = export["chain_head"].as_str().unwrap().to_string();
    assert_eq!(export["events"].as_i64(), Some(5));
    assert_eq!(export["artifacts"].as_i64(), Some(1));
    assert!(archive.exists());
    ctx.stop();

    let mut ctx2 = Ctx::new("import");
    ctx2.start_daemon();
    let imported = ctx2.json(&["import", archive.to_str().unwrap()]);
    assert_eq!(imported["verified"].as_bool(), Some(true));
    assert_eq!(imported["chain_head"].as_str().unwrap(), chain);
    assert_eq!(imported["workspace_id"].as_str().unwrap(), ws_id);

    let verify = ctx2.json(&["verify", "--workspace", &ws_id]);
    assert_eq!(verify["ok"].as_bool(), Some(true));
    assert_eq!(verify["chain_head"].as_str().unwrap(), chain);

    let shown = ctx2.json(&["branch", "show", &br_id]);
    assert_eq!(shown["branch"]["head_sequence"].as_i64(), Some(4));
    assert_eq!(shown["branch"]["name"].as_str().unwrap(), "alt");
}

#[test]
fn append_is_idempotent() {
    let mut ctx = Ctx::new("idem");
    ctx.start_daemon();
    let ws = ctx.json(&["workspace", "create", "demo"]);
    let ws_id = ws["id"].as_str().unwrap().to_string();

    let first = ctx.json(&[
        "event",
        "append",
        "--type",
        "test.recorded",
        "--payload",
        r#"{"n":3}"#,
        "--workspace",
        &ws_id,
        "--idem",
        "k-1",
    ]);
    let second = ctx.json(&[
        "event",
        "append",
        "--type",
        "test.recorded",
        "--payload",
        r#"{"n":4}"#,
        "--workspace",
        &ws_id,
        "--idem",
        "k-1",
    ]);
    assert_eq!(first["event_id"], second["event_id"]);
    assert_eq!(first["sequence"], second["sequence"]);

    let verify = ctx.json(&["verify", "--workspace", &ws_id]);
    assert_eq!(verify["checked"].as_i64(), Some(2));
}

#[test]
fn import_rejects_tampered_manifest() {
    let mut ctx = Ctx::new("tamper");
    ctx.start_daemon();
    let ws = ctx.json(&["workspace", "create", "demo"]);
    let ws_id = ws["id"].as_str().unwrap().to_string();
    let archive = ctx.home.join("t.hliar");
    ctx.json(&["export", &ws_id, "--out", archive.to_str().unwrap()]);
    ctx.stop();

    let mut raw = std::fs::read(&archive).unwrap();
    let needle = b"\"chain_head\":\"sha256:";
    let pos = raw
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("chain_head en el manifiesto");
    let start = pos + needle.len();
    raw[start] = if raw[start] == b'a' { b'b' } else { b'a' };
    std::fs::write(&archive, &raw).unwrap();

    let mut ctx2 = Ctx::new("tamper-import");
    ctx2.start_daemon();
    let (code, _, stderr) = ctx2.run(&["import", archive.to_str().unwrap()]);
    assert_ne!(code, 0, "el import debía fallar");
    assert!(
        stderr.contains("conflict") || stderr.contains("chain_head"),
        "error inesperado: {stderr}"
    );
}
