//! Autostart MCH: recuperación tras reinicio del daemon, aislamiento de fallos,
//! lifecycle completo antes de `ready` y exclusión mutua entre autostart y
//! `mch serve`.
//!
//! Cada prueba usa su propio `HLI_HOME`, políticas de milisegundos y un módulo
//! fixture empaquetado al vuelo, sin estado compartido entre pruebas.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const INSTANCE: &str = "autostart-main";
const MODULE: &str = "mch-stub";
static SEQ: AtomicUsize = AtomicUsize::new(0);

fn daemon_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli-daemon"))
}

fn stub_bin() -> PathBuf {
    // target/<perfil>/deps/<prueba>-<hash>.exe -> target/<perfil>/examples/mch_stub[.exe]
    let exe = std::env::current_exe().expect("current_exe");
    let profile = exe.parent().and_then(|p| p.parent()).expect("target dir");
    profile.join("examples").join(if cfg!(windows) {
        "mch_stub.exe"
    } else {
        "mch_stub"
    })
}

fn module_entry() -> &'static str {
    if cfg!(windows) {
        "bin/mch-stub.exe"
    } else {
        "bin/mch-stub"
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
    let entry = module_entry();
    let exe = std::fs::read(stub_bin()).expect("binario del fixture");
    let manifest = json!({
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": MODULE, "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "protocol": "hli-module/1",
            "transport": "stdio-ndjson",
            "runtime": {"kind": "subprocess", "entry": entry},
            "operations": [
                {"name": "memory.search", "effects": "read"},
                {"name": "memory.commit", "effects": "write"}
            ],
            "permissions": {"filesystem": "none", "network": "daemon-only", "secrets": "none"},
            "lifecycle": {"health": "health", "shutdown": "shutdown"}
        }
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let checks = json!({
        "manifest.json": sha256(&manifest_bytes),
        entry: sha256(&exe),
    });
    let checks_bytes = serde_json::to_vec(&checks).unwrap();
    let raw = container(&[
        ("manifest.json", manifest_bytes),
        (entry, exe),
        ("checksums.json", checks_bytes),
    ]);
    let path = home.join("mch-stub.hlimod");
    std::fs::write(&path, raw).unwrap();
    path
}

fn call_home(home: &Path, cmd: &str, args: Value) -> Result<Value, String> {
    let raw = std::fs::read_to_string(home.join("daemon.json"))
        .map_err(|e| format!("daemon.json: {e}"))?;
    let info: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let port = info["port"].as_u64().ok_or("daemon.json sin puerto")? as u16;
    let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let mut out = stream.try_clone().map_err(|e| e.to_string())?;
    let req = json!({"id": "1", "cmd": cmd, "args": args});
    writeln!(out, "{req}").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let resp: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        Err(format!(
            "{}: {}",
            resp["error"]["code"].as_str().unwrap_or("error"),
            resp["error"]["message"].as_str().unwrap_or("sin detalle")
        ))
    }
}

fn kill_pid(pid: i64) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
}

fn pid_alive(pid: i64) -> bool {
    if cfg!(windows) {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    } else {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}

impl Ctx {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let home = std::env::temp_dir().join(format!(
            "hli-mch-autostart-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        Self { home, child: None }
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn log_path(&self) -> PathBuf {
        self.home.join("daemon-stderr.log")
    }

    fn log_tail(&self) -> String {
        let text = std::fs::read_to_string(self.log_path()).unwrap_or_default();
        let start = text.len().saturating_sub(4000);
        text[start..].to_string()
    }

    fn fixture(&self, cfg: Value) {
        std::fs::write(
            self.home.join("fixture.json"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .unwrap();
    }

    fn call(&self, cmd: &str, args: Value) -> Result<Value, String> {
        call_home(&self.home, cmd, args)
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        match self.call(cmd, args) {
            Ok(v) => v,
            Err(e) => panic!("{cmd} falló: {e}\n--- daemon ---\n{}", self.log_tail()),
        }
    }

    fn start(&mut self) {
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())
            .unwrap();
        let child = Command::new(daemon_bin())
            .env("HLI_HOME", &self.home)
            .env("HLI_MCH_HEALTH_INTERVAL_MS", "50")
            .env("HLI_MCH_HEALTH_TIMEOUT_MS", "1000")
            .env("HLI_MCH_DEGRADED_AFTER", "3")
            .env("HLI_MCH_RESTART_AFTER", "1000")
            .env("HLI_MCH_MAX_RESTARTS", "0")
            .env("HLI_MCH_BACKOFF_MS", "10")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("no se pudo lanzar el daemon");
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if self.call("daemon.ping", json!({})).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("el daemon no arrancó\n{}", self.log_tail());
    }

    fn stop(&mut self) {
        let _ = self.call("daemon.stop", json!({}));
        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        std::thread::sleep(Duration::from_millis(100));
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
    }

    fn trace(&self) -> Vec<Value> {
        std::fs::read_to_string(self.home.join("mch_trace.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn wait_state(&self, pred: impl Fn(&Value) -> bool, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        let mut last = Value::Null;
        loop {
            if let Ok(st) = self.call("mch.status", json!({"name": INSTANCE})) {
                if pred(&st) {
                    return st;
                }
                last = st;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timeout esperando estado; último: {last}\n--- daemon ---\n{}",
                    self.log_tail()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn is_ready(st: &Value) -> bool {
    st["instance"]["state"].as_str() == Some("ready")
        && st["process"]["alive"].as_bool() == Some(true)
}

/// Daemon inicial para instalar el módulo y crear la instancia; se detiene
/// limpiamente para que cada prueba arranque el daemon que sí importa.
fn prepare(tag: &str, autostart: bool, fixture: Value) -> Ctx {
    let mut c = Ctx::new(tag);
    c.fixture(fixture);
    c.start();
    let pkg = build_package(c.home());
    c.ok("module.install", json!({"path": pkg.to_string_lossy()}));
    let ws = c.ok("workspace.create", json!({"name": "demo", "root": "."}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.ok(
        "mch.create",
        json!({
            "name": INSTANCE,
            "module": MODULE,
            "domain": "memory",
            "workspace_id": ws,
            "autostart": autostart
        }),
    );
    c.stop();
    c
}

fn invoke_search(c: &Ctx) -> Value {
    c.ok(
        "mch.invoke",
        json!({"name": INSTANCE, "operation": "memory.search", "arguments": {"query": ""}}),
    )
}

#[test]
fn mch_autostart_recovers_after_daemon_restart() {
    let mut c = prepare("recovers", true, json!({"trace": true}));

    c.start();
    let first = c.wait_state(is_ready, Duration::from_secs(15));
    let pid1 = first["process"]["pid"]
        .as_i64()
        .expect("pid tras autostart");
    assert_eq!(invoke_search(&c)["result"]["pid"].as_i64(), Some(pid1));

    // Reinicio del daemon: la instancia debe volver sola a ready con un proceso
    // nuevo, sin `mch serve` manual.
    c.stop();
    c.start();
    let second = c.wait_state(is_ready, Duration::from_secs(15));
    let pid2 = second["process"]["pid"]
        .as_i64()
        .expect("pid tras reinicio");
    assert_ne!(pid2, pid1, "el autostart debe lanzar un proceso nuevo");
    assert_eq!(
        invoke_search(&c)["result"]["pid"].as_i64(),
        Some(pid2),
        "el proceso recuperado debe atender invocaciones"
    );
    assert_eq!(
        c.ok("mch.inspect", json!({"name": INSTANCE}))["autostart"].as_bool(),
        Some(true),
        "autostart debe persistir en la instancia"
    );
}

#[test]
fn mch_non_autostart_remains_stopped() {
    let mut c = prepare("nonauto", false, json!({"trace": true}));

    c.start();
    let served = c.ok("mch.serve", json!({"name": INSTANCE}));
    assert!(served["pid"].as_i64().unwrap_or(0) > 0);
    c.stop();

    let before = c
        .trace()
        .iter()
        .filter(|l| l["method"] == "initialize")
        .count();

    c.start();
    std::thread::sleep(Duration::from_millis(600));

    let st = c.ok("mch.status", json!({"name": INSTANCE}));
    assert_eq!(st["instance"]["state"].as_str(), Some("stopped"));
    assert!(
        st["instance"]["pid"].is_null(),
        "el PID previo debe limpiarse"
    );
    assert!(st["process"].is_null(), "no debe haber proceso supervisado");
    let err = c
        .call(
            "mch.invoke",
            json!({"name": INSTANCE, "operation": "memory.search", "arguments": {}}),
        )
        .unwrap_err();
    assert!(
        err.contains("no está ready") || err.contains("no iniciado"),
        "error inesperado: {err}"
    );

    let after = c
        .trace()
        .iter()
        .filter(|l| l["method"] == "initialize")
        .count();
    assert_eq!(
        after, before,
        "una instancia sin autostart no debe relanzarse al arrancar el daemon"
    );
}

#[test]
fn failing_autostart_does_not_block_daemon() {
    let mut c = prepare(
        "failing",
        true,
        json!({"mode": "initialize_failure", "initialize_delay_ms": 3000}),
    );

    let started = Instant::now();
    c.start();
    let ping = started.elapsed();
    assert!(
        ping < Duration::from_millis(2200),
        "el daemon tardó {ping:?} en responder: el autostart bloqueó el arranque"
    );

    // Con el lifecycle todavía en curso, el daemon ya atiende comandos.
    let list = c.ok("mch.list", json!({}));
    assert_eq!(list.as_array().map(Vec::len), Some(1));

    let failed = c.wait_state(
        |st| st["instance"]["state"] == "failed",
        Duration::from_secs(20),
    );
    assert!(
        failed["process"].is_null(),
        "un autostart fallido no debe dejar proceso supervisado"
    );

    // Y sigue operativo después del fallo.
    c.ok("workspace.list", json!({}));
    c.ok("mch.list", json!({}));
}

#[test]
fn autostart_reaches_ready_only_after_lifecycle() {
    let mut c = prepare(
        "lifecycle",
        true,
        json!({"trace": true, "configure_delay_ms": 800}),
    );

    c.start();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut saw_starting = false;
    let (at_ready, trace_at_ready) = loop {
        let st = c.ok("mch.status", json!({"name": INSTANCE}));
        match st["instance"]["state"].as_str().unwrap_or("") {
            "starting" => saw_starting = true,
            "ready" => break (st, c.trace()),
            _ => {}
        }
        if Instant::now() >= deadline {
            panic!("sin ready; último: {st}\n{}", c.log_tail());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(saw_starting, "debe observarse `starting` antes de `ready`");

    let pid = at_ready["process"]["pid"].as_i64().expect("pid");
    let methods: Vec<String> = trace_at_ready
        .iter()
        .filter(|l| l["pid"].as_i64() == Some(pid))
        .filter_map(|l| l["method"].as_str().map(str::to_string))
        .collect();
    let pos = |m: &str| {
        methods
            .iter()
            .position(|x| x == m)
            .unwrap_or_else(|| panic!("falta {m} antes de ready: {methods:?}"))
    };
    assert!(
        pos("initialize") < pos("capabilities")
            && pos("capabilities") < pos("configure")
            && pos("configure") < pos("health"),
        "orden de lifecycle incorrecto: {methods:?}"
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "initialize").count(),
        1,
        "un solo initialize por proceso: {methods:?}"
    );

    let inv = invoke_search(&c);
    assert_eq!(inv["result"]["pid"].as_i64(), Some(pid));
    assert!(inv["result"]["health_count"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn autostart_does_not_reuse_stale_pid() {
    let mut c = prepare("stale", true, json!({}));

    c.start();
    let first = c.wait_state(is_ready, Duration::from_secs(15));
    let stale = first["process"]["pid"].as_i64().expect("pid inicial");

    // Caída abrupta: la DB conserva state=ready y el PID viejo.
    c.kill();
    kill_pid(stale);
    assert!(!pid_alive(stale), "el proceso viejo debe estar muerto");

    c.start();
    let st = c.wait_state(is_ready, Duration::from_secs(15));
    let pid = st["process"]["pid"].as_i64().expect("pid nuevo");
    assert_ne!(pid, stale, "no debe reutilizarse el PID persistido");
    assert!(pid_alive(pid), "el proceso autostart debe estar vivo");
    assert_eq!(
        invoke_search(&c)["result"]["pid"].as_i64(),
        Some(pid),
        "las invocaciones deben ir al proceso nuevo"
    );
}

#[test]
fn concurrent_serve_and_autostart_spawn_once() {
    let mut c = prepare(
        "concurrent",
        true,
        json!({"trace": true, "configure_delay_ms": 600}),
    );

    c.start();
    let mut threads = Vec::new();
    for _ in 0..4 {
        let home = c.home().to_path_buf();
        threads.push(std::thread::spawn(move || {
            call_home(&home, "mch.serve", json!({"name": INSTANCE}))
        }));
    }
    let mut served = Vec::new();
    for t in threads {
        match t.join().expect("hilo de mch serve") {
            Ok(v) => served.push(v["pid"].as_i64().expect("pid de mch serve")),
            Err(e) => panic!("mch serve concurrente falló: {e}"),
        }
    }

    let st = c.wait_state(is_ready, Duration::from_secs(20));
    let pid = st["process"]["pid"].as_i64().expect("pid supervisado");
    assert!(
        served.iter().all(|p| *p == pid),
        "mch serve y autostart devolvieron procesos distintos: {served:?} vs {pid}"
    );

    let inits: Vec<i64> = c
        .trace()
        .iter()
        .filter(|l| l["method"] == "initialize")
        .filter_map(|l| l["pid"].as_i64())
        .collect();
    assert_eq!(
        inits.len(),
        1,
        "el guard por instancia debe impedir procesos duplicados: {inits:?}"
    );
    assert_eq!(inits[0], pid);
    assert_eq!(invoke_search(&c)["result"]["pid"].as_i64(), Some(pid));
}
