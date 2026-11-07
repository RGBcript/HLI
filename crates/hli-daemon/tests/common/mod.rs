//! Arnés compartido por las pruebas de integración del daemon MCH.
//!
//! Cada `Ctx` usa su propio `HLI_HOME`, empaqueta el fixture real y limpia
//! daemon y procesos en `Drop`, de modo que las pruebas pueden correr en
//! paralelo sin estado compartido.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const MODULE: &str = "mch-stub";
static SEQ: AtomicUsize = AtomicUsize::new(0);

pub fn daemon_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hli-daemon"))
}

pub fn stub_bin() -> PathBuf {
    // target/<perfil>/deps/<prueba>-<hash>.exe -> target/<perfil>/examples/mch_stub[.exe]
    let exe = std::env::current_exe().expect("current_exe");
    let profile = exe.parent().and_then(|p| p.parent()).expect("target dir");
    profile.join("examples").join(if cfg!(windows) {
        "mch_stub.exe"
    } else {
        "mch_stub"
    })
}

pub fn module_entry() -> &'static str {
    if cfg!(windows) {
        "bin/mch-stub.exe"
    } else {
        "bin/mch-stub"
    }
}

pub fn sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("sha256:{}", hex::encode(h.finalize()))
}

pub fn container(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"HLIAR1\n".to_vec();
    for (path, bytes) in files {
        out.extend_from_slice(format!("FILE {} {}\n", bytes.len(), path).as_bytes());
        out.extend_from_slice(bytes);
        out.push(b'\n');
    }
    out
}

pub fn build_package(home: &Path) -> PathBuf {
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

pub fn call_home(home: &Path, cmd: &str, args: Value) -> Result<Value, String> {
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

pub fn kill_pid(pid: i64) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
}

pub fn pid_alive(pid: i64) -> bool {
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

pub struct Ctx {
    home: PathBuf,
    child: Option<Child>,
}

impl Ctx {
    pub fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let home = std::env::temp_dir().join(format!("hli-mch-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        Self { home, child: None }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    fn log_path(&self) -> PathBuf {
        self.home.join("daemon-stderr.log")
    }

    pub fn log_tail(&self) -> String {
        let text = std::fs::read_to_string(self.log_path()).unwrap_or_default();
        let start = text.len().saturating_sub(4000);
        text[start..].to_string()
    }

    pub fn fixture(&self, cfg: Value) {
        std::fs::write(
            self.home.join("fixture.json"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .unwrap();
    }

    /// Configuración que solo aplica a una instancia (`HLI_MCH_INSTANCE`).
    pub fn fixture_for(&self, instance: &str, cfg: Value) {
        std::fs::write(
            self.home.join(format!("fixture-{instance}.json")),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .unwrap();
    }

    pub fn remove_fixture_for(&self, instance: &str) {
        let _ = std::fs::remove_file(self.home.join(format!("fixture-{instance}.json")));
    }

    pub fn call(&self, cmd: &str, args: Value) -> Result<Value, String> {
        call_home(&self.home, cmd, args)
    }

    pub fn ok(&self, cmd: &str, args: Value) -> Value {
        match self.call(cmd, args) {
            Ok(v) => v,
            Err(e) => panic!("{cmd} falló: {e}\n--- daemon ---\n{}", self.log_tail()),
        }
    }

    pub fn start(&mut self) {
        self.start_env(&[]);
    }

    pub fn start_env(&mut self, extra: &[(&str, &str)]) {
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path())
            .unwrap();
        let mut cmd = Command::new(daemon_bin());
        cmd.env("HLI_HOME", &self.home)
            .env("HLI_MCH_HEALTH_INTERVAL_MS", "50")
            .env("HLI_MCH_HEALTH_TIMEOUT_MS", "1000")
            .env("HLI_MCH_DEGRADED_AFTER", "3")
            .env("HLI_MCH_RESTART_AFTER", "1000")
            .env("HLI_MCH_MAX_RESTARTS", "0")
            .env("HLI_MCH_BACKOFF_MS", "10")
            .env("HLI_MCH_RESTART_WINDOW_MS", "60000")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("no se pudo lanzar el daemon");
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

    pub fn stop(&mut self) {
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

    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        std::thread::sleep(Duration::from_millis(100));
        let _ = std::fs::remove_file(self.home.join("daemon.json"));
    }

    pub fn trace(&self) -> Vec<Value> {
        std::fs::read_to_string(self.home.join("mch_trace.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    pub fn initialize_count(&self) -> usize {
        self.trace()
            .iter()
            .filter(|l| l["method"] == "initialize")
            .count()
    }

    pub fn install_module(&self) -> String {
        let pkg = build_package(&self.home);
        self.ok("module.install", json!({"path": pkg.to_string_lossy()}));
        self.ok("workspace.create", json!({"name": "demo", "root": "."}))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub fn create_instance(&self, instance: &str, autostart: bool) {
        let ws = self.install_workspace();
        self.ok(
            "mch.create",
            json!({
                "name": instance,
                "module": MODULE,
                "domain": "memory",
                "workspace_id": ws,
                "autostart": autostart
            }),
        );
    }

    fn install_workspace(&self) -> String {
        self.ok("workspace.create", json!({"name": "demo", "root": "."}))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub fn wait_state(
        &self,
        instance: &str,
        pred: impl Fn(&Value) -> bool,
        timeout: Duration,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        let mut last = Value::Null;
        loop {
            if let Ok(st) = self.call("mch.status", json!({"name": instance})) {
                if pred(&st) {
                    return st;
                }
                last = st;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timeout esperando estado de {instance}; último: {last}\n--- daemon ---\n{}",
                    self.log_tail()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn invoke(&self, instance: &str) -> Value {
        self.ok(
            "mch.invoke",
            json!({"name": instance, "operation": "memory.search", "arguments": {"query": ""}}),
        )
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

pub fn is_ready(st: &Value) -> bool {
    st["instance"]["state"].as_str() == Some("ready")
        && st["process"]["alive"].as_bool() == Some(true)
}

pub fn state_is(state: &str) -> impl Fn(&Value) -> bool + '_ {
    move |st: &Value| st["instance"]["state"].as_str() == Some(state)
}

/// Daemon inicial para instalar el módulo y crear las instancias; se detiene
/// limpiamente para que cada prueba arranque el daemon que sí importa.
pub fn prepare(tag: &str, instances: &[(&str, bool)], fixture: Value, env: &[(&str, &str)]) -> Ctx {
    let mut c = Ctx::new(tag);
    c.fixture(fixture);
    c.start_env(env);
    let ws = c.install_module();
    for (name, autostart) in instances {
        c.ok(
            "mch.create",
            json!({
                "name": name,
                "module": MODULE,
                "domain": "memory",
                "workspace_id": ws,
                "autostart": autostart
            }),
        );
    }
    c.stop();
    c
}
