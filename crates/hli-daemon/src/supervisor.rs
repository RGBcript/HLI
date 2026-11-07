//! Supervisor de procesos MCH persistentes (JSON-RPC 2.0 sobre stdio).
//!
//! Un proceso por instancia, reutilizado entre invocaciones. HLI sigue siendo
//! la fuente de verdad: el modulo no abre SQLite ni conserva estado durable.
//!
//! Los reinicios automáticos están acotados por instancia: como máximo
//! `max_restarts` procesos de reemplazo dentro de `restart_window`. Superado el
//! límite la instancia queda `failed` y solo un `mch restart` manual la recupera.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hli_core::{time::now_iso, HliError, Result};
use hli_store::Writer;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct SupervisorPolicy {
    pub health_interval: Duration,
    pub health_timeout: Duration,
    pub degraded_after: u32,
    pub restart_after: u32,
    pub max_restarts: u32,
    pub restart_window: Duration,
    pub initial_backoff: Duration,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            health_interval: Duration::from_secs(30),
            health_timeout: Duration::from_secs(5),
            degraded_after: 2,
            restart_after: 3,
            max_restarts: 3,
            restart_window: Duration::from_secs(60),
            initial_backoff: Duration::from_secs(1),
        }
    }
}

fn env_ms(key: &str, def: Duration) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(def)
}

fn env_u32(key: &str, def: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(def)
}

fn policy() -> SupervisorPolicy {
    let d = SupervisorPolicy::default();
    SupervisorPolicy {
        health_interval: env_ms("HLI_MCH_HEALTH_INTERVAL_MS", d.health_interval),
        health_timeout: env_ms("HLI_MCH_HEALTH_TIMEOUT_MS", d.health_timeout),
        degraded_after: env_u32("HLI_MCH_DEGRADED_AFTER", d.degraded_after),
        restart_after: env_u32("HLI_MCH_RESTART_AFTER", d.restart_after),
        max_restarts: env_u32("HLI_MCH_MAX_RESTARTS", d.max_restarts),
        restart_window: env_ms("HLI_MCH_RESTART_WINDOW_MS", d.restart_window),
        initial_backoff: env_ms("HLI_MCH_BACKOFF_MS", d.initial_backoff),
    }
}

pub struct MchHandle {
    pub pid: u32,
    pub generation: u64,
    pub child: Mutex<Child>,
    pub stdin: Mutex<ChildStdin>,
    pub pending: Mutex<HashMap<String, SyncSender<Result<Value>>>>,
    pub alive: AtomicBool,
    pub stderr_tail: Mutex<String>,
    pub next_id: AtomicU64,
    pub started: Instant,
    pub capabilities: Mutex<Value>,
    pub state: Mutex<String>,
    pub health_failures: AtomicU32,
    pub stop_flag: AtomicBool,
    pub last_health_at: Mutex<Option<String>>,
    pub last_health_ok: AtomicBool,
    pub last_error: Mutex<Option<Value>>,
}

pub type Shared = Arc<MchHandle>;

/// Estado del limitador de reinicios automáticos, por instancia.
///
/// `count` cuenta procesos de reemplazo iniciados por recuperación automática
/// dentro de la ventana; los fallos de health, el proceso inicial, el autostart
/// inicial y los reinicios manuales no incrementan este contador.
#[derive(Default)]
struct LimitState {
    count: u32,
    total_restarts: u32,
    window_started: Option<Instant>,
    window_started_at: Option<String>,
    last_restart_at: Option<String>,
    next_restart_at: Option<String>,
    phase: String,
    generation: u64,
    last_error: Option<Value>,
    cancel_epoch: u64,
}

fn limits() -> &'static Mutex<HashMap<String, Arc<Mutex<LimitState>>>> {
    static L: OnceLock<Mutex<HashMap<String, Arc<Mutex<LimitState>>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

fn limit_state(name: &str) -> Arc<Mutex<LimitState>> {
    limits()
        .lock()
        .unwrap()
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(LimitState::default())))
        .clone()
}

fn expire_window(s: &mut LimitState, window: Duration) {
    if let Some(started) = s.window_started {
        if started.elapsed() >= window {
            s.count = 0;
            s.window_started = None;
            s.window_started_at = None;
            s.next_restart_at = None;
        }
    }
}

fn reset_limits(name: &str) {
    let lim = limit_state(name);
    let mut s = lim.lock().unwrap();
    s.count = 0;
    s.window_started = None;
    s.window_started_at = None;
    s.next_restart_at = None;
    s.phase.clear();
    s.last_error = None;
    s.cancel_epoch += 1;
}

fn iso_in_ms(ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total = now.as_millis() as u64 + ms;
    hli_core::time::iso_from_unix((total / 1000) as i64, (total % 1000) as u32)
}

fn limits_snapshot(name: &str) -> Option<Value> {
    let lim = limits().lock().unwrap().get(name).cloned()?;
    let mut s = lim.lock().unwrap();
    expire_window(&mut s, policy().restart_window);
    Some(json!({
        "restart_count": s.total_restarts,
        "restart_count_in_window": s.count,
        "restart_window_started_at": s.window_started_at,
        "last_restart_at": s.last_restart_at,
        "next_restart_at": s.next_restart_at,
        "phase": s.phase,
        "generation": s.generation,
        "last_error": s.last_error,
    }))
}

fn merge_limits(status: &mut Value, limits: &Value) {
    for key in [
        "restart_count",
        "restart_count_in_window",
        "restart_window_started_at",
        "last_restart_at",
        "next_restart_at",
    ] {
        status[key] = limits[key].clone();
    }
    if status["last_error"].is_null() {
        status["last_error"] = limits["last_error"].clone();
    }
}

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

/// Arranque explícito (autostart, `mch serve`, `mch restart`): reinicia el
/// limitador porque no es una recuperación automática.
pub fn start(
    name: &str,
    bin: &Path,
    home: &Path,
    workspace: &str,
    writer: &Writer,
) -> Result<Shared> {
    let guard = instance_guard(name);
    let _held = guard.lock().unwrap_or_else(|e| e.into_inner());
    start_locked(name, bin, home, workspace, writer, true)
}

/// Serializa `start`/`stop` por instancia: autostart y `mch serve` concurrentes
/// no pueden lanzar dos procesos para el mismo `instance_id`.
fn instance_guard(name: &str) -> Arc<Mutex<()>> {
    static G: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn start_locked(
    name: &str,
    bin: &Path,
    home: &Path,
    workspace: &str,
    writer: &Writer,
    reset: bool,
) -> Result<Shared> {
    if let Some(h) = get(name) {
        if h.alive.load(Ordering::SeqCst) {
            return Ok(h);
        }
        stop_locked(name)?;
    }
    if reset {
        reset_limits(name);
    }
    if !bin.exists() {
        return Err(HliError::not_found(&format!(
            "binario del modulo {}",
            bin.display()
        )));
    }
    let mut child = Command::new(bin)
        .env("HLI_HOME", home)
        .env("HLI_MCH_INSTANCE", name)
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

    let generation = next_generation(name);
    limit_state(name).lock().unwrap().generation = generation;
    let h = Arc::new(MchHandle {
        pid,
        generation,
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        stderr_tail: Mutex::new(String::new()),
        next_id: AtomicU64::new(1),
        started: Instant::now(),
        capabilities: Mutex::new(Value::Null),
        state: Mutex::new("starting".into()),
        health_failures: AtomicU32::new(0),
        stop_flag: AtomicBool::new(false),
        last_health_at: Mutex::new(None),
        last_health_ok: AtomicBool::new(false),
        last_error: Mutex::new(None),
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

    let lifecycle = (|| -> Result<()> {
        request(
            &h,
            "initialize",
            json!({"_meta": {"hli.dev/workspaceId": workspace}}),
            Duration::from_secs(10),
        )?;
        let caps = request(&h, "capabilities", json!({}), Duration::from_secs(10))?;
        *h.capabilities.lock().unwrap() = caps;
        request(
            &h,
            "configure",
            json!({"_meta": {"hli.dev/workspaceId": workspace}}),
            Duration::from_secs(10),
        )?;
        request(&h, "health", json!({}), Duration::from_secs(10))?;
        Ok(())
    })();
    if let Err(e) = lifecycle {
        let _ = stop_locked(name);
        return Err(e);
    }
    *h.state.lock().unwrap() = "ready".into();
    let hn = name.to_string();
    let hb = bin.to_path_buf();
    let hh = home.to_path_buf();
    let hw = workspace.to_string();
    let hs = h.clone();
    let w = writer.clone();
    std::thread::spawn(move || health_loop(hn, hb, hh, hw, hs, w));
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
    request(&h, "health", json!({}), timeout)
}

pub fn stop(name: &str) -> Result<()> {
    let guard = instance_guard(name);
    let _held = guard.lock().unwrap_or_else(|e| e.into_inner());
    cancel_pending_backoff(name);
    stop_locked(name)
}

/// Cancela un backoff en curso; el hilo de health lo detecta por época.
fn cancel_pending_backoff(name: &str) {
    let lim = limit_state(name);
    let mut s = lim.lock().unwrap();
    s.cancel_epoch += 1;
    s.next_restart_at = None;
    if s.phase == "restarting" {
        s.phase.clear();
    }
}

fn stop_locked(name: &str) -> Result<()> {
    let h = match registry().lock().unwrap().remove(name) {
        Some(h) => h,
        None => return Ok(()),
    };
    h.stop_flag.store(true, Ordering::SeqCst);
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

pub fn restart(
    name: &str,
    bin: &Path,
    home: &Path,
    workspace: &str,
    writer: &Writer,
) -> Result<Shared> {
    stop(name)?;
    start(name, bin, home, workspace, writer)
}

pub fn status(name: &str) -> Option<Value> {
    let limits = limits_snapshot(name);
    if let Some(h) = get(name) {
        let mut v = json!({
            "pid": h.pid,
            "state": h.state.lock().unwrap().clone(),
            "alive": h.alive.load(Ordering::SeqCst),
            "generation": h.generation,
            "uptime_ms": h.started.elapsed().as_millis() as i64,
            "health_failures": h.health_failures.load(Ordering::SeqCst),
            "restart_count": 0,
            "last_health_at": h.last_health_at.lock().unwrap().clone(),
            "last_health_ok": h.last_health_ok.load(Ordering::SeqCst),
            "last_error": h.last_error.lock().unwrap().clone(),
            "stderr_tail": h.stderr_tail.lock().unwrap().clone(),
        });
        if let Some(l) = limits {
            merge_limits(&mut v, &l);
        }
        return Some(v);
    }
    let l = limits?;
    let phase = l["phase"].as_str().unwrap_or("");
    if phase.is_empty() {
        return None;
    }
    Some(json!({
        "pid": Value::Null,
        "state": phase,
        "alive": false,
        "generation": l["generation"],
        "uptime_ms": 0,
        "health_failures": 0,
        "restart_count": l["restart_count"],
        "last_health_at": Value::Null,
        "last_health_ok": false,
        "last_error": l["last_error"],
        "stderr_tail": "",
        "restart_count_in_window": l["restart_count_in_window"],
        "restart_window_started_at": l["restart_window_started_at"],
        "last_restart_at": l["last_restart_at"],
        "next_restart_at": l["next_restart_at"],
    }))
}

fn next_generation(name: &str) -> u64 {
    static G: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let g = G.get_or_init(|| Mutex::new(HashMap::new()));
    let mut m = g.lock().unwrap();
    let e = m.entry(name.to_string()).or_insert(0);
    *e += 1;
    *e
}

fn health_loop(
    name: String,
    bin: std::path::PathBuf,
    home: std::path::PathBuf,
    ws: String,
    h: Shared,
    writer: Writer,
) {
    let p = policy();
    loop {
        std::thread::sleep(p.health_interval);
        if h.stop_flag.load(Ordering::SeqCst) || !h.alive.load(Ordering::SeqCst) {
            return;
        }
        let res = request(&h, "health", json!({}), p.health_timeout);
        *h.last_health_at.lock().unwrap() = Some(now_iso());
        match res {
            Ok(_) => {
                h.health_failures.store(0, Ordering::SeqCst);
                h.last_health_ok.store(true, Ordering::SeqCst);
                *h.last_error.lock().unwrap() = None;
                let mut st = h.state.lock().unwrap();
                if *st == "degraded" {
                    *st = "ready".into();
                }
            }
            Err(e) => {
                h.last_health_ok.store(false, Ordering::SeqCst);
                *h.last_error.lock().unwrap() = Some(json!({
                    "code": e.code,
                    "message": e.message
                }));
                let n = h.health_failures.fetch_add(1, Ordering::SeqCst) + 1;
                if n >= p.degraded_after {
                    *h.state.lock().unwrap() = "degraded".into();
                }
                if n >= p.restart_after {
                    begin_auto_restart(&name, &bin, &home, &ws, &h, &writer);
                    return;
                }
            }
        }
    }
}

fn restart_limit_error(p: &SupervisorPolicy) -> Value {
    json!({
        "code": "restart_limit_exceeded",
        "message": format!(
            "límite de {} reinicios automáticos en {} ms",
            p.max_restarts,
            p.restart_window.as_millis()
        ),
    })
}

/// Decide y ejecuta un reinicio automático respetando el límite por instancia.
///
/// El backoff no mantiene locks ni transacciones: solo duerme en el hilo de
/// health de esta instancia, en tramos cortos que comprueban la época de
/// cancelación (`mch stop` la incrementa).
fn begin_auto_restart(name: &str, bin: &Path, home: &Path, ws: &str, h: &Shared, writer: &Writer) {
    let p = policy();
    let lim = limit_state(name);
    let epoch0 = lim.lock().unwrap().cancel_epoch;

    {
        let guard = instance_guard(name);
        let _held = guard.lock().unwrap_or_else(|e| e.into_inner());
        let limited = {
            let mut s = lim.lock().unwrap();
            expire_window(&mut s, p.restart_window);
            if s.count >= p.max_restarts {
                s.phase = "failed".into();
                s.next_restart_at = None;
                s.last_error = Some(restart_limit_error(&p));
                s.cancel_epoch += 1;
                s.generation = h.generation;
                true
            } else {
                false
            }
        };
        if limited {
            let err = lim
                .lock()
                .unwrap()
                .last_error
                .clone()
                .unwrap_or_else(|| restart_limit_error(&p));
            mark_failed(name, h, writer, err);
            return;
        }
        let _ = stop_locked(name);
        if lim.lock().unwrap().cancel_epoch != epoch0 {
            return;
        }
    }

    let (backoff, epoch) = {
        let mut s = lim.lock().unwrap();
        if s.cancel_epoch != epoch0 {
            return;
        }
        let backoff = p.initial_backoff * 2u32.pow(s.count.min(10));
        s.phase = "restarting".into();
        s.next_restart_at = Some(iso_in_ms(backoff.as_millis() as u64));
        (backoff, s.cancel_epoch)
    };
    *h.state.lock().unwrap() = "restarting".into();
    let _ = writer.mch_set_state(name, "restarting", None);

    if wait_backoff(name, epoch, backoff) {
        let mut s = lim.lock().unwrap();
        if s.cancel_epoch == epoch {
            s.phase.clear();
            s.next_restart_at = None;
        }
        return;
    }

    let guard = instance_guard(name);
    let _held = guard.lock().unwrap_or_else(|e| e.into_inner());
    let limited = {
        let mut s = lim.lock().unwrap();
        if s.cancel_epoch != epoch {
            return;
        }
        expire_window(&mut s, p.restart_window);
        if s.count >= p.max_restarts {
            s.phase = "failed".into();
            s.next_restart_at = None;
            s.last_error = Some(restart_limit_error(&p));
            s.cancel_epoch += 1;
            s.generation = h.generation;
            true
        } else {
            false
        }
    };
    if limited {
        let err = lim
            .lock()
            .unwrap()
            .last_error
            .clone()
            .unwrap_or_else(|| restart_limit_error(&p));
        mark_failed(name, h, writer, err);
        return;
    }

    match start_locked(name, bin, home, ws, writer, false) {
        Ok(newh) => {
            let mut s = lim.lock().unwrap();
            expire_window(&mut s, p.restart_window);
            if s.window_started.is_none() {
                s.window_started = Some(Instant::now());
                s.window_started_at = Some(now_iso());
            }
            s.count += 1;
            s.total_restarts += 1;
            s.last_restart_at = Some(now_iso());
            s.next_restart_at = None;
            s.phase.clear();
            s.generation = newh.generation;
        }
        Err(e) => {
            let err = json!({"code": e.code, "message": e.message});
            {
                let mut s = lim.lock().unwrap();
                s.phase = "failed".into();
                s.next_restart_at = None;
                s.last_error = Some(err);
                s.generation = h.generation;
            }
            let _ = writer.mch_set_state(name, "failed", None);
        }
    }
}

fn wait_backoff(name: &str, epoch: u64, backoff: Duration) -> bool {
    let lim = limit_state(name);
    let deadline = Instant::now() + backoff;
    loop {
        if lim.lock().unwrap().cancel_epoch != epoch {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
    }
}

fn mark_failed(name: &str, h: &Shared, writer: &Writer, err: Value) {
    *h.last_error.lock().unwrap() = Some(err);
    *h.state.lock().unwrap() = "failed".into();
    let _ = stop_locked(name);
    let _ = writer.mch_set_state(name, "failed", None);
}

pub fn logs(name: &str) -> Option<String> {
    get(name).map(|h| h.stderr_tail.lock().unwrap().clone())
}
