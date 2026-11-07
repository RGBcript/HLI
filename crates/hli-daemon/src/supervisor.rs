//! Supervisor de procesos MCH persistentes (JSON-RPC 2.0 sobre stdio).
//!
//! Un proceso por instancia, reutilizado entre invocaciones. HLI sigue siendo
//! la fuente de verdad: el modulo no abre SQLite ni conserva estado durable.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hli_core::{HliError, Result};
use serde_json::{json, Value};

pub struct MchHandle {
    pub pid: u32,
    pub child: Mutex<Child>,
    pub stdin: Mutex<ChildStdin>,
    pub pending: Mutex<HashMap<String, SyncSender<Result<Value>>>>,
    pub alive: AtomicBool,
    pub stderr_tail: Mutex<String>,
    pub next_id: AtomicU64,
    pub started: Instant,
}

pub type Shared = Arc<MchHandle>;

fn registry() -> &'static Mutex<HashMap<String, Shared>> {
    static R: OnceLock<Mutex<HashMap<String, Shared>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get(name: &str) -> Option<Shared> {
    registry().lock().unwrap().get(name).cloned()
}

#[allow(dead_code)]
pub fn is_alive(name: &str) -> bool {
    get(name)
        .map(|h| h.alive.load(Ordering::SeqCst))
        .unwrap_or(false)
}

pub fn start(name: &str, bin: &Path, home: &Path, workspace: &str) -> Result<Shared> {
    if let Some(h) = get(name) {
        if h.alive.load(Ordering::SeqCst) {
            return Ok(h);
        }
        stop(name)?;
    }
    if !bin.exists() {
        return Err(HliError::not_found(&format!(
            "binario del modulo {}",
            bin.display()
        )));
    }
    let mut child = Command::new(bin)
        .env("HLI_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HliError::internal(format!("no se pudo lanzar el modulo: {e}")))?;
    let pid = child.id();
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| HliError::internal("MCH sin stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HliError::internal("MCH sin stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| HliError::internal("MCH sin stderr"))?;

    let h = Arc::new(MchHandle {
        pid,
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        stderr_tail: Mutex::new(String::new()),
        next_id: AtomicU64::new(1),
        started: Instant::now(),
    });

    let reader = h.clone();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
                        if let Some(tx) = reader.pending.lock().unwrap().remove(id) {
                            let _ = tx.send(Ok(v));
                        }
                    }
                }
            }
        }
        reader.alive.store(false, Ordering::SeqCst);
        for (_, tx) in reader.pending.lock().unwrap().drain() {
            let _ = tx.send(Err(HliError::internal("proceso MCH termino")));
        }
    });

    let errs = h.clone();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stderr);
        let mut line = String::new();
        while let Ok(n) = r.read_line(&mut line) {
            if n == 0 {
                break;
            }
            let mut tail = errs.stderr_tail.lock().unwrap();
            if tail.len() < 8192 {
                tail.push_str(&line);
            }
            line.clear();
        }
    });

    registry()
        .lock()
        .unwrap()
        .insert(name.to_string(), h.clone());

    if let Err(e) = request(
        &h,
        "initialize",
        json!({"_meta": {"hli.dev/workspaceId": workspace}}),
        Duration::from_secs(10),
    ) {
        let _ = stop(name);
        return Err(e);
    }
    Ok(h)
}

fn request(h: &Shared, method: &str, params: Value, timeout: Duration) -> Result<Value> {
    if !h.alive.load(Ordering::SeqCst) {
        return Err(HliError::conflict("proceso MCH no esta vivo"));
    }
    let id = format!("req_{}", h.next_id.fetch_add(1, Ordering::SeqCst));
    let (tx, rx) = sync_channel(1);
    h.pending.lock().unwrap().insert(id.clone(), tx);
    let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    {
        let mut stdin = h.stdin.lock().unwrap();
        if let Err(e) = writeln!(stdin, "{msg}").and_then(|_| stdin.flush()) {
            h.pending.lock().unwrap().remove(&id);
            return Err(HliError::internal(format!("MCH stdin: {e}")));
        }
    }
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => {
            if let Some(err) = v.get("error") {
                return Err(HliError::internal(format!("MCH: {err}")));
            }
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            h.pending.lock().unwrap().remove(&id);
            Err(HliError::new("timeout", "MCH no respondio en el plazo"))
        }
    }
}

pub fn invoke(
    name: &str,
    operation: &str,
    arguments: &Value,
    workspace: &str,
    timeout: Duration,
) -> Result<Value> {
    let h = get(name).ok_or_else(|| HliError::conflict("MCH no iniciado; ejecuta mch serve"))?;
    if !h.alive.load(Ordering::SeqCst) {
        return Err(HliError::conflict("proceso MCH no esta vivo"));
    }
    let request_id = format!("inv_{}", h.next_id.fetch_add(1, Ordering::SeqCst));
    request(
        &h,
        "invoke",
        json!({
            "name": operation,
            "arguments": arguments,
            "_meta": {
                "hli.dev/workspaceId": workspace,
                "hli.dev/requestId": request_id,
                "hli.dev/idempotencyKey": format!("idem_{operation}")
            }
        }),
        timeout,
    )
}

pub fn health(name: &str, timeout: Duration) -> Result<Value> {
    let h = get(name).ok_or_else(|| HliError::conflict("MCH no iniciado"))?;
    let request_id = format!("hl_{}", h.next_id.fetch_add(1, Ordering::SeqCst));
    request(
        &h,
        "invoke",
        json!({
            "name": "memory.search",
            "arguments": {"query": ""},
            "_meta": {"hli.dev/workspaceId": "", "hli.dev/requestId": request_id}
        }),
        timeout,
    )
}

pub fn stop(name: &str) -> Result<()> {
    let h = match registry().lock().unwrap().remove(name) {
        Some(h) => h,
        None => return Ok(()),
    };
    let _ = request(&h, "shutdown", json!({}), Duration::from_secs(2));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let done = matches!(h.child.lock().unwrap().try_wait(), Ok(Some(_)));
        if done {
            break;
        }
        if Instant::now() >= deadline {
            let _ = h.child.lock().unwrap().kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = h.child.lock().unwrap().wait();
    h.alive.store(false, Ordering::SeqCst);
    Ok(())
}

pub fn restart(name: &str, bin: &Path, home: &Path, workspace: &str) -> Result<Shared> {
    stop(name)?;
    start(name, bin, home, workspace)
}

pub fn status(name: &str) -> Option<Value> {
    get(name).map(|h| {
        json!({
            "pid": h.pid,
            "alive": h.alive.load(Ordering::SeqCst),
            "uptime_ms": h.started.elapsed().as_millis() as i64,
            "stderr_tail": h.stderr_tail.lock().unwrap().clone(),
        })
    })
}

pub fn logs(name: &str) -> Option<String> {
    get(name).map(|h| h.stderr_tail.lock().unwrap().clone())
}
