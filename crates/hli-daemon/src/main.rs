//! Daemon de HLI 0.1: API local por TCP + NDJSON.
//!
//! Decisión (ADR-0001): el transporte de 0.1 es un socket TCP en loopback con
//! mensajes JSON delimitados por línea. HTTP/WebSocket llega en 0.2 sin cambiar
//! el contrato de comandos.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hli_core::{HliError, time::now_iso};
mod providers;
mod runs;
mod supervisor;

use hli_agent::build_manifest;
use hli_provider::{ChatProvider, ChatRequest, OllamaProvider, OpenAiProvider};
use hli_store::{Reader, Store, Writer};
use serde_json::{Value, json};

fn main() {
    if matches!(std::env::args().nth(1).as_deref(), Some("--version" | "-V")) {
        println!("hli-daemon {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let home = hli_home();
    if let Err(e) = run(home) {
        eprintln!("hli-daemon: {e}");
        std::process::exit(1);
    }
}

fn hli_home() -> PathBuf {
    if let Ok(h) = std::env::var("HLI_HOME") {
        return PathBuf::from(h);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".hli")
}

fn run(home: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    providers::check_version(&home)?;
    // Store::open normaliza los PID y estados de la ejecución anterior antes de
    // publicar daemon.json: ningún proceso persistido se da por vivo.
    let store = Store::open(&home)?;
    let (writer, reader) = store.start()?;
    let reader = Arc::new(reader);

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    let info = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "port": port,
        "home": home.to_string_lossy(),
        "started_at": now_iso(),
        "transport": "tcp-ndjson"
    });
    std::fs::write(home.join("daemon.json"), serde_json::to_vec_pretty(&info)?)?;

    eprintln!(
        "hli-daemon escuchando en 127.0.0.1:{port} (home={})",
        home.display()
    );

    // El listener ya acepta conexiones; el autostart corre en su propio hilo
    // para que un lifecycle lento o fallido nunca retrase la respuesta.
    autostart_instances(home.clone(), writer.clone(), reader.clone());

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let w = writer.clone();
                let r = reader.clone();
                let h = home.clone();
                std::thread::spawn(move || {
                    let _ = handle(s, w, r, h);
                });
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

fn handle(
    stream: TcpStream,
    writer: Writer,
    reader: Arc<Reader>,
    home: PathBuf,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut out = stream.try_clone()?;
    let mut lines = BufReader::new(stream).lines();

    while let Some(Ok(line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = json!({"id": null, "ok": false, "error": {"code": "bad_json", "message": e.to_string()}});
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
        let args = req.get("args").cloned().unwrap_or(json!({}));

        match dispatch(cmd, &args, &writer, &reader, &home) {
            Ok(data) => {
                let resp = json!({"id": id, "ok": true, "data": data});
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
            }
            Err(e) => {
                let resp =
                    json!({"id": id, "ok": false, "error": {"code": e.code, "message": e.message}});
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
            }
        }

        if cmd == "daemon.stop" {
            // El proceso sale aquí: SQLite ya está comprometido por el actor.
            // No se detiene el writer por separado (dejaría un daemon vivo sin escritor).
            std::thread::sleep(Duration::from_millis(60));
            std::process::exit(0);
        }
    }
    Ok(())
}

/// Reconstruye el payload renderizado desde el manifiesto persistido. Se usa al
/// reanudar una ejecución aprobada, donde el hilo original ya no existe.
pub(crate) fn render_for(manifest: &hli_core::model::ContextManifest, r: &Reader) -> String {
    let mut slices = Vec::new();
    for sl in &manifest.artifact_slices {
        if let Some(id) = sl["slice_id"].as_str()
            && let Ok(pair) = r.slice(id)
        {
            slices.push(pair);
        }
    }
    hli_agent::render_payload(manifest, &slices)
}

fn cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

#[allow(clippy::too_many_arguments)]
fn make_receipt(
    provider: &str,
    model: &str,
    adapter: &str,
    manifest: &hli_core::model::ContextManifest,
    result: &Value,
    usage: &hli_provider::Usage,
    cost: &Value,
    secret_handles: Vec<Value>,
    used_memory_ids: Vec<String>,
    used_slice_ids: Vec<String>,
    used_event_ids: Vec<String>,
    candidate_memory_ids: Vec<String>,
    rejected_memory_ids: Vec<String>,
) -> hli_core::model::Receipt {
    hli_core::model::Receipt {
        provider: provider.to_string(),
        model: model.to_string(),
        adapter_version: adapter.to_string(),
        context_manifest: manifest.manifest_id.clone(),
        tool_schemas: vec!["filesystem.read".into(), "filesystem.write".into()],
        effective_permissions: json!({
            "filesystem.read": "allow",
            "filesystem.write": "ask",
            "shell.execute": "deny"
        }),
        nonsecret_environment: json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH}),
        secret_handles,
        input_artifacts: vec![],
        module_versions: vec![],
        sampling: json!({"temperature": 0.0, "seed": 0}),
        result_artifacts: vec![],
        sandbox: json!({"mode": "none", "note": "sin sandbox de SO"}),
        used_memory_ids: used_memory_ids.clone(),
        used_slice_ids,
        used_event_ids,
        used_explicit_reference_ids: manifest
            .explicit_references
            .iter()
            .filter(|r| r.included)
            .map(|r| r.stable_id.clone())
            .collect(),
        rejected_explicit_reference_ids: manifest
            .explicit_references
            .iter()
            .filter(|r| !r.included)
            .map(|r| r.stable_id.clone())
            .collect(),
        candidate_memory_ids,
        rejected_memory_ids: rejected_memory_ids.clone(),
        rejection_reasons: if rejected_memory_ids.is_empty() {
            json!({})
        } else {
            json!({"policy": rejected_memory_ids})
        },
        usage: serde_json::to_value(usage).unwrap_or(json!({})),
        cost: cost.clone(),
        result_hash: hli_core::canon::hash_canonical(result),
        terminal_event: None,
        plan_id: None,
        plan_hash: None,
        plan_origin: None,
        plan_revision: None,
        approval_status: None,
        approval_scope: None,
        approval_events: vec![],
        approved_effects: vec![],
        rejected_effects: vec![],
        execution_snapshot_hash: None,
        provider_id: None,
        superseded_plan_ids: vec![],
        invalidation_events: vec![],
    }
}

fn sdk_allows(sdk: &str) -> bool {
    // Acepta >=0.4 <0.5, ^0.4, 0.4; rechaza rangos que excluyan 0.4.
    let s = sdk.trim();
    if let Some(rest) = s.strip_prefix(">=") {
        let min = rest.split_whitespace().next().unwrap_or("").trim();
        if let Some(max) = s.split('<').nth(1) {
            let max = max.trim();
            let ok_min = min.parse::<f64>().map(|v| v <= 0.4).unwrap_or(false);
            let ok_max = max.parse::<f64>().map(|v| v > 0.4).unwrap_or(false);
            return ok_min && ok_max;
        }
        return min.parse::<f64>().map(|v| v <= 0.4).unwrap_or(false);
    }
    if let Some(rest) = s.strip_prefix('^') {
        return rest
            .trim()
            .parse::<f64>()
            .map(|v| (v - 0.4).abs() < 1e-9 || (0.3..0.4).contains(&v))
            .unwrap_or(false);
    }
    s.contains("0.4")
}

fn read_secret(home: &std::path::Path, handle: &str) -> Result<(String, String), HliError> {
    let env_key = format!("HLI_SECRET_{}", handle.to_uppercase().replace('-', "_"));
    let value = std::env::var(&env_key).ok().or_else(|| {
        std::fs::read_to_string(home.join("secrets").join(handle))
            .ok()
            .map(|s| s.trim().to_string())
    });
    let value = value.ok_or_else(|| HliError::not_found(&format!("secreto {handle}")))?;
    let mut h = sha2::Sha256::new();
    use sha2::Digest;
    h.update(value.as_bytes());
    let d = h.finalize();
    Ok((value, format!("v{}", hex::encode(&d[..4]))))
}

fn load_price(home: &std::path::Path, provider: &str, model: &str) -> Option<hli_provider::Price> {
    let text = std::fs::read_to_string(home.join("prices.yaml")).ok()?;
    let prices = hli_provider::parse_prices_yaml(&text);
    prices.into_iter().find(|p| {
        (p.provider == provider || p.provider == "*") && (p.model == model || p.model == "*")
    })
}

fn s(args: &Value, key: &str) -> Result<String, HliError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| HliError::invalid(format!("falta el argumento '{key}'")))
}

fn o(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

fn module_signature_state(home: &std::path::Path, module: &Value) -> String {
    let dir = PathBuf::from(module["path"].as_str().unwrap_or(""));
    hli_signing::package::installed_state(&dir, home)
        .unwrap_or_else(|_| "invalid-signature".to_string())
}

fn mch_bin(r: &Reader, name: &str) -> Result<(PathBuf, String), HliError> {
    let m = r.mch(name)?;
    let module = m["module"].as_str().unwrap_or("");
    let mm = r.module(module)?;
    let entry = mm["manifest"]["spec"]["runtime"]["entry"]
        .as_str()
        .unwrap_or("bin/hli-memory.exe");
    let bin = PathBuf::from(mm["path"].as_str().unwrap_or("")).join(entry);
    let ws = m["workspace_id"].as_str().unwrap_or("").to_string();
    Ok((bin, ws))
}

/// Lanza el proceso de una instancia y ejecuta su lifecycle completo
/// (`initialize → capabilities → configure → health`). Compartido por
/// `mch.serve`, `mch.restart` y el autostart del arranque.
fn launch_instance(
    r: &Reader,
    w: &Writer,
    name: &str,
    home: &std::path::Path,
) -> Result<(supervisor::Shared, Value), HliError> {
    let (bin, ws) = mch_bin(r, name)?;
    w.mch_set_state(name, "starting", None)?;
    let h = match supervisor::start(name, &bin, home, &ws, w) {
        Ok(h) => h,
        Err(e) => {
            let _ = w.mch_set_state(name, "failed", None);
            return Err(e);
        }
    };
    let m = r.mch(name)?;
    let declared = m["manifest"]["spec"]["operations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let caps = h.capabilities.lock().unwrap().clone();
    let cap_ops = caps["operations"].as_array().cloned().unwrap_or_default();
    for op in &declared {
        if !cap_ops.iter().any(|c| c["name"] == op["name"]) {
            let _ = supervisor::stop(name);
            return Err(HliError::conflict(format!(
                "el modulo no declara la operacion {}",
                op["name"].as_str().unwrap_or("-")
            )));
        }
    }
    Ok((h, caps))
}

/// Inicia en segundo plano las instancias con `autostart = true`. Un fallo se
/// registra como `failed` y no interrumpe ni bloquea al daemon.
fn autostart_instances(home: PathBuf, writer: Writer, reader: Arc<Reader>) {
    std::thread::spawn(move || {
        let instances = match reader.mchs() {
            Ok(list) => list,
            Err(e) => {
                eprintln!("autostart: no se pudieron leer las instancias: {e}");
                return;
            }
        };
        for instance in instances {
            if instance["autostart"].as_bool() != Some(true) {
                continue;
            }
            let Some(name) = instance["name"].as_str().map(str::to_string) else {
                continue;
            };
            match launch_instance(&reader, &writer, &name, &home) {
                Ok((h, _)) => {
                    let _ = writer.mch_set_state(&name, "ready", Some(h.pid as i64));
                    eprintln!("autostart: {name} listo (pid {})", h.pid);
                }
                Err(e) => {
                    let _ = writer.mch_set_state(&name, "failed", None);
                    eprintln!("autostart: {name} fallo: {}", e.message);
                }
            }
        }
    });
}

fn dispatch(
    cmd: &str,
    args: &Value,
    w: &Writer,
    r: &Arc<Reader>,
    home: &std::path::Path,
) -> Result<Value, HliError> {
    match cmd {
        "daemon.ping" => Ok(
            json!({"pong": true, "version": env!("CARGO_PKG_VERSION"), "pid": std::process::id()}),
        ),
        "daemon.stop" => Ok(json!({"stopping": true})),
        "workspace.create" => {
            let ws = w.create_workspace(
                &s(args, "name")?,
                args.get("root").and_then(|v| v.as_str()).unwrap_or("."),
            )?;
            Ok(serde_json::to_value(ws)?)
        }
        "workspace.list" => Ok(serde_json::to_value(r.workspaces()?)?),
        "workspace.get" => Ok(serde_json::to_value(
            r.workspace(&s(args, "workspace_id")?)?,
        )?),
        "thread.create" => {
            let t = w.create_thread(&s(args, "workspace_id")?, &s(args, "title")?)?;
            Ok(serde_json::to_value(t)?)
        }
        "thread.list" => Ok(serde_json::to_value(r.threads(&s(args, "workspace_id")?)?)?),
        "branch.create" => {
            let thread_id = match o(args, "thread_id") {
                Some(t) => t,
                None => {
                    let from = s(args, "from_event")?;
                    r.event(&from)?.thread_id.ok_or_else(|| {
                        HliError::invalid("el evento no pertenece a ningún thread")
                    })?
                }
            };
            let b = w.create_branch(&thread_id, &s(args, "name")?, o(args, "from_event"))?;
            Ok(serde_json::to_value(b)?)
        }
        "branch.list" => Ok(serde_json::to_value(r.branches(&s(args, "thread_id")?)?)?),
        "branch.show" => Ok(serde_json::to_value(
            r.branch_state(&s(args, "branch_id")?)?,
        )?),
        "branch.snapshot" => Ok(serde_json::to_value(w.snapshot(&s(args, "branch_id")?)?)?),
        "event.append" => {
            let payload = args.get("payload").cloned().unwrap_or(json!({}));
            let ev = w.append_event(
                &s(args, "workspace_id")?,
                o(args, "thread_id"),
                o(args, "branch_id"),
                &s(args, "type")?,
                payload,
                o(args, "idempotency_key"),
            )?;
            Ok(serde_json::to_value(ev)?)
        }
        "event.list" => Ok(serde_json::to_value(r.events(
            &s(args, "workspace_id")?,
            args.get("branch_id").and_then(|v| v.as_str()),
        )?)?),
        "artifact.put" => {
            let info =
                w.put_artifact(&s(args, "workspace_id")?, PathBuf::from(s(args, "path")?))?;
            Ok(serde_json::to_value(info)?)
        }
        "artifact.list" => Ok(Value::Array(r.artifacts(&s(args, "workspace_id")?)?)),
        "artifact.slice" => {
            let aid = s(args, "artifact_id")?;
            let meta = r.artifact(&aid)?;
            let bytes = std::fs::read(&meta.path)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(HliError::invalid("artefacto demasiado grande para slice"));
            }
            let kind = args
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("full_text_if_small");
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("selected")
                .to_string();
            let (content, selector) = match kind {
                "line_range" => {
                    let text = String::from_utf8_lossy(&bytes);
                    let lines: Vec<&str> = text.lines().collect();
                    let start = args.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let end = args
                        .get("end")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(lines.len() as u64) as usize;
                    let end = end.min(lines.len());
                    let sel = json!({"kind": "line_range", "start": start, "end": end});
                    (lines[start.min(end)..end].join("\n").into_bytes(), sel)
                }
                "byte_range" => {
                    let start = args.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let end = args
                        .get("end")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(bytes.len() as u64) as usize;
                    let end = end.min(bytes.len());
                    let sel = json!({"kind": "byte_range", "start": start, "end": end});
                    (bytes[start.min(end)..end].to_vec(), sel)
                }
                _ => {
                    let sel = json!({"kind": "full_text_if_small", "bytes": bytes.len()});
                    (bytes, sel)
                }
            };
            let workspace_id = s(args, "workspace_id")?;
            let slice = w.create_slice(
                &aid,
                &workspace_id,
                &meta.sha256,
                &meta.mime,
                selector,
                content,
                &reason,
            )?;
            Ok(serde_json::to_value(slice)?)
        }
        "verify" => Ok(serde_json::to_value(r.verify(&s(args, "workspace_id")?)?)?),
        "export" => {
            let rep = w.export(&s(args, "workspace_id")?, PathBuf::from(s(args, "out")?))?;
            Ok(serde_json::to_value(rep)?)
        }
        "import" => {
            let rep = w.import(PathBuf::from(s(args, "archive")?))?;
            Ok(serde_json::to_value(rep)?)
        }
        "memory.commit" => {
            let supersedes = args
                .get("supersedes")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let contradicts = args
                .get("contradicts")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let m = w.commit_memory(
                &s(args, "workspace_id")?,
                args.get("scope")
                    .and_then(|v| v.as_str())
                    .unwrap_or("workspace"),
                o(args, "scope_id"),
                &s(args, "type")?,
                &s(args, "content")?,
                supersedes
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                contradicts
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                o(args, "source_authority"),
                o(args, "subject"),
                o(args, "relation"),
                o(args, "object"),
            )?;
            Ok(serde_json::to_value(m)?)
        }
        "memory.get" => Ok(serde_json::to_value(r.memory(&s(args, "memory_id")?)?)?),
        "memory.list" => Ok(serde_json::to_value(
            r.memories(&s(args, "workspace_id")?)?,
        )?),
        "memory.search" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
            Ok(serde_json::to_value(r.search_memories(
                &s(args, "workspace_id")?,
                &s(args, "query")?,
                limit,
            )?)?)
        }
        "memory.update" => {
            let m = w.update_memory(
                &s(args, "memory_id")?,
                o(args, "content"),
                o(args, "status"),
                o(args, "valid_until"),
            )?;
            Ok(serde_json::to_value(m)?)
        }
        "memory.forget" => Ok(serde_json::to_value(
            w.forget_memory(&s(args, "memory_id")?)?,
        )?),
        "provider.add" => {
            let name = s(args, "name")?;
            let mut cfg = providers::load(home);
            let entry = providers::ProviderEntry {
                kind: s(args, "kind")?,
                base_url: s(args, "base_url")?,
                model: s(args, "model")?,
                timeout_seconds: args
                    .get("timeout_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(120),
                secret_handle: args
                    .get("secret_handle")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            };
            entry.validate()?;
            cfg.providers.insert(name.clone(), entry);
            providers::save(home, &cfg)?;
            Ok(json!({"added": name}))
        }
        "provider.list" => {
            let cfg = providers::load(home);
            let list: Vec<Value> = cfg
                .providers
                .iter()
                .map(|(name, e)| {
                    json!({
                        "name": name,
                        "kind": e.kind,
                        "base_url": e.base_url,
                        "model": e.model,
                        "timeout_seconds": e.timeout_seconds,
                        "has_secret": e.secret_handle.is_some(),
                        "default": cfg.default_provider.as_deref() == Some(name.as_str()),
                    })
                })
                .collect();
            Ok(json!({"default_provider": cfg.default_provider, "providers": list}))
        }
        "provider.inspect" => {
            let name = s(args, "name")?;
            let cfg = providers::load(home);
            let e = cfg
                .providers
                .get(&name)
                .ok_or_else(|| HliError::not_found(&format!("proveedor: {name}")))?;
            Ok(json!({
                "name": name,
                "kind": e.kind,
                "base_url": e.base_url,
                "model": e.model,
                "timeout_seconds": e.timeout_seconds,
                "secret_handle": e.secret_handle,
                "has_secret": e.secret_handle.is_some(),
                "default": cfg.default_provider.as_deref() == Some(name.as_str()),
            }))
        }
        "provider.set_default" => {
            let name = s(args, "name")?;
            let mut cfg = providers::load(home);
            if !cfg.providers.contains_key(&name) {
                return Err(HliError::not_found(&format!("proveedor: {name}")));
            }
            cfg.default_provider = Some(name.clone());
            providers::save(home, &cfg)?;
            Ok(json!({"default_provider": name}))
        }
        "provider.remove" => {
            let name = s(args, "name")?;
            let mut cfg = providers::load(home);
            if cfg.providers.remove(&name).is_none() {
                return Err(HliError::not_found(&format!("proveedor: {name}")));
            }
            if cfg.default_provider.as_deref() == Some(name.as_str()) {
                cfg.default_provider = None;
            }
            providers::save(home, &cfg)?;
            Ok(json!({"removed": name}))
        }
        "provider.test" => {
            let name = s(args, "name")?;
            let cfg = providers::load(home);
            let e = cfg
                .providers
                .get(&name)
                .ok_or_else(|| HliError::not_found(&format!("proveedor: {name}")))?
                .clone();
            e.validate()?;
            let started = std::time::Instant::now();
            let req = ChatRequest {
                system: "Responde solo con pong.",
                user: "ping",
                temperature: 0.0,
                max_tokens: 8,
            };
            let (provider, model, text, usage) = match e.kind.as_str() {
                "sim" => (
                    "sim".to_string(),
                    e.model.clone(),
                    "pong".to_string(),
                    json!({"usage_source": "estimated"}),
                ),
                "ollama" => {
                    let p = OllamaProvider::new(&e.base_url, &e.model, e.timeout_seconds);
                    let r = p.chat(&req)?;
                    (r.provider, r.model, r.text, json!(r.usage))
                }
                "openai" => {
                    let handle = e
                        .secret_handle
                        .as_deref()
                        .ok_or_else(|| HliError::invalid("proveedor openai sin secret_handle"))?;
                    let (key, _) = read_secret(home, handle)?;
                    let p = OpenAiProvider::new(&e.base_url, &e.model, &key, e.timeout_seconds);
                    let r = p.chat(&req)?;
                    (r.provider, r.model, r.text, json!(r.usage))
                }
                _ => unreachable!(),
            };
            Ok(json!({
                "provider": provider,
                "model": model,
                "latency_ms": started.elapsed().as_millis() as i64,
                "usage": usage,
                "text": text,
            }))
        }
        "run.start" => {
            let ws = s(args, "workspace_id")?;
            let th = s(args, "thread_id")?;
            let br = s(args, "branch_id")?;
            let prompt = s(args, "prompt")?;
            let budget = args
                .get("budget_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(2_048);
            let cfg = providers::load(home);
            let requested = args.get("provider").and_then(|v| v.as_str());
            let selected = requested
                .map(str::to_string)
                .or_else(|| cfg.default_provider.clone());
            let entry = selected
                .as_ref()
                .and_then(|n| cfg.providers.get(n))
                .cloned();
            if let Some(e) = &entry {
                e.validate()?;
            }
            let provider_name = entry
                .as_ref()
                .map(|e| e.kind.clone())
                .unwrap_or_else(|| selected.clone().unwrap_or_else(|| "sim".into()));
            let default_model = match provider_name.as_str() {
                "ollama" => "gemma4:31b-cloud",
                "openai" => "gpt-4o-mini",
                _ => "sim-1",
            };
            let model = args
                .get("model")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| entry.as_ref().map(|e| e.model.clone()))
                .unwrap_or_else(|| default_model.to_string());
            let base_url = args
                .get("base_url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| entry.as_ref().map(|e| e.base_url.clone()))
                .unwrap_or_else(|| {
                    if provider_name == "ollama" {
                        "http://127.0.0.1:11434".to_string()
                    } else {
                        "https://api.openai.com/v1".to_string()
                    }
                });
            let timeout = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .or_else(|| entry.as_ref().map(|e| e.timeout_seconds))
                .unwrap_or(120);
            let temperature = args
                .get("temperature")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let max_tokens = args
                .get("max_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(256);

            let mut secret_handles: Vec<Value> = Vec::new();
            let mut _api_key = String::new();
            if provider_name == "openai" {
                let handle = entry
                    .as_ref()
                    .and_then(|e| e.secret_handle.clone())
                    .or_else(|| {
                        args.get("secret")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    })
                    .ok_or_else(|| HliError::invalid("openai requiere --secret o secret_handle"))?;
                let (value, version) = read_secret(home, &handle)?;
                _api_key = value;
                secret_handles.push(json!({"id": handle, "version": version}));
            }

            let ws_obj = r.workspace(&ws)?;
            let explicit_in: Vec<hli_core::model::ExplicitReference> = args
                .get("explicit_references")
                .filter(|v| !v.is_null())
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| HliError::invalid(format!("explicit_references inválidas: {e}")))?
                .unwrap_or_default();
            let budget_chars = (budget.max(256) as usize) * 4;
            let explicit = materialize_references(
                r,
                ws.as_str(),
                ws_obj.root_path.as_str(),
                th.as_str(),
                &explicit_in,
                budget_chars,
            )?;
            let run = w.create_run(&ws, &th, &br, &prompt)?;
            let instructions = ws_obj
                .config
                .get("instructions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let visible = r.visible_events(&br, run.branch_revision)?;
            let top_k = args
                .get("top_k")
                .and_then(|v| v.as_u64())
                .unwrap_or(5)
                .max(1) as usize;
            let memories = r.search_memories(&ws, &prompt, top_k)?;
            let candidate_memory_ids: Vec<String> = memories
                .iter()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect();
            let mut manifest = build_manifest(
                &run,
                &visible,
                instructions,
                &prompt,
                &memories,
                budget,
                explicit,
            );

            // La trayectoria visible completa queda como evidencia; el contexto se presupuesta.
            manifest.visible_event_ids = visible.iter().map(|e| e.event_id.clone()).collect();
            let no_policy = args
                .get("no_policy")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let task = hli_agent::classify_task(&prompt);
            if !no_policy {
                match task {
                    "memory_lookup" => {
                        // La trayectoria visible se conserva; el presupuesto recorta por tamaño.
                        manifest.recent_events.truncate(2);
                        manifest.memories.truncate(3);
                    }
                    "artifact_analysis" | "code_fix" | "summarization" => {
                        manifest.recent_events.clear();
                        manifest.memories.truncate(1);
                    }
                    _ => {
                        manifest.recent_events.truncate(2);
                        manifest.memories.truncate(2);
                    }
                }
            }
            for m in manifest.memories.iter_mut() {
                if let Some(o) = m.as_object_mut()
                    && let Some(c) = o.get("content").and_then(|v| v.as_str())
                    && c.chars().count() > 400
                {
                    let cut: String = c.chars().take(400).collect();
                    o.insert("content".into(), json!(format!("{cut}…")));
                }
            }

            // Slices de artefacto: evidencia seleccionada, persistida y verificable.
            let no_slices = args
                .get("no_slices")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let slice_pairs = if no_slices {
                vec![]
            } else {
                match args.get("artifact_id").and_then(|v| v.as_str()) {
                    Some(aid) => r.slices_for_artifact(aid)?,
                    None => vec![],
                }
            };
            if args
                .get("no_memories")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                manifest.memories.clear();
            }
            if args
                .get("no_events")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                manifest.recent_events.clear();
            }
            manifest.artifact_slices = slice_pairs
                .iter()
                .map(|(sl, _)| serde_json::to_value(sl).unwrap_or(json!({})))
                .collect();
            let payload = hli_agent::build_payload(&manifest, &slice_pairs);
            manifest.payload_hash = hli_core::canon::hash_canonical(&payload);
            manifest.estimated_tokens += slice_pairs
                .iter()
                .map(|(_, c)| ((c.len() as i64) + 3) / 4)
                .sum::<i64>();
            let manifest = w.save_manifest(manifest)?;

            // Verificación completa del payload: se reconstruye desde el manifiesto persistido.
            let mut reloaded = Vec::new();
            for sl in &manifest.artifact_slices {
                let id = sl["slice_id"].as_str().unwrap_or("");
                reloaded.push(r.slice(id)?);
            }
            let payload_check = hli_agent::build_payload(&manifest, &reloaded);
            if hli_core::canon::hash_canonical(&payload_check) != manifest.payload_hash {
                return Err(HliError::conflict(
                    "el payload no coincide con el manifiesto",
                ));
            }

            let prev_provider = r.last_provider(&th).ok().flatten();
            if prev_provider.as_deref() != Some(provider_name.as_str()) {
                w.run_event(
                    &run.id,
                    "provider.handoff",
                    json!({
                        "run_id": run.id, "from": prev_provider,
                        "to": provider_name, "model": model
                    }),
                )?;
            }
            w.run_event(
                &run.id,
                "model.requested",
                json!({
                    "run_id": run.id, "manifest_id": manifest.manifest_id,
                    "payload_hash": manifest.payload_hash,
                    "provider": provider_name, "model": model
                }),
            )?;

            let prepared = runs::Prepared {
                run,
                manifest,
                prompt,
                provider_id: selected.unwrap_or_else(|| provider_name.clone()),
                provider_kind: provider_name,
                model,
                base_url,
                timeout_secs: timeout,
                temperature,
                max_tokens,
                budget_tokens: budget,
                secret_handles,
                candidate_memory_ids,
                is_async: args.get("async").and_then(|v| v.as_bool()).unwrap_or(false),
                sim_delay_ms: args.get("sim_delay_ms").and_then(|v| v.as_u64()),
            };
            runs::plan_and_maybe_execute(home, w, r, prepared)
        }

        "run.plan" => {
            let run_id = s(args, "run_id")?;
            runs::plan_response(r, &run_id)
        }
        "run.approve" => {
            let run_id = s(args, "run_id")?;
            let scope = o(args, "scope").unwrap_or_else(|| "once".into());
            let actor = o(args, "actor").unwrap_or_else(|| "cli".into());
            let expected = o(args, "plan_id");
            match r.plan_for_run(&run_id) {
                Ok(_) => runs::approve(home, w, r, &run_id, &scope, &actor, expected.as_deref()),
                // Compatibilidad con runs anteriores al plan durable.
                Err(_) => approve_legacy(w, r, &run_id),
            }
        }
        "run.replan" => {
            let run_id = s(args, "run_id")?;
            let reasons: Vec<String> = args
                .get("reasons")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let plan_id = o(args, "plan_id");
            runs::replan(home, w, r, &run_id, plan_id.as_deref(), reasons)
        }
        "run.reject" => {
            let run_id = s(args, "run_id")?;
            let reason = o(args, "reason").unwrap_or_else(|| "sin razón".into());
            let actor = o(args, "actor").unwrap_or_else(|| "cli".into());
            let plan_id = o(args, "plan_id");
            runs::reject(w, r, &run_id, &reason, &actor, plan_id.as_deref())
        }
        "run.get" => {
            let run_id = s(args, "run_id")?;
            let run = r.run(&run_id)?;
            let manifest = r.manifest_for_run(&run_id).ok();
            Ok(json!({"run": run, "manifest": manifest}))
        }
        "run.status" => {
            let run_id = s(args, "run_id")?;
            let run = r.run(&run_id)?;
            Ok(json!({"run": run}))
        }
        "run.wait" => {
            let run_id = s(args, "run_id")?;
            let timeout = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(60);
            let started = std::time::Instant::now();
            loop {
                let run = r.run(&run_id)?;
                // waiting_approval es un punto de parada explícito: el cliente
                // necesita verlo para aprobar o rechazar.
                if hli_core::model::run_is_terminal(&run.state) || run.state == "waiting_approval" {
                    return Ok(json!({"run": run}));
                }
                if started.elapsed() >= Duration::from_secs(timeout) {
                    return Ok(json!({"run": run, "timed_out": true}));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        "run.cancel" => {
            let run_id = s(args, "run_id")?;
            let run = r.run(&run_id)?;
            if hli_core::model::run_is_terminal(&run.state) {
                return Ok(json!({"run": run, "idempotent": true}));
            }
            if run.state == "queued" || run.state == "waiting_approval" {
                // Cancelar una espera de aprobación cierra también el plan.
                if let Ok(plan) = r.plan_for_run(&run_id) {
                    let _ = w.set_plan_status(
                        &plan.plan_id,
                        "cancelled",
                        None,
                        None,
                        Some("cancelled_by_user".into()),
                    );
                }
                let done = w.finish_run(
                    &run_id,
                    "cancelled",
                    Some(json!({"cancelled": true, "terminal_reason": "cancelled"})),
                    None,
                )?;
                return Ok(json!({"run": done}));
            }
            if let Some(flag) = cancels().lock().unwrap().get(&run_id).cloned() {
                flag.store(true, Ordering::SeqCst);
            }
            let run = w.set_run_state(&run_id, "cancelling")?;
            Ok(json!({"run": run, "cancelling": true}))
        }
        "run.list" => {
            let mut runs = r.runs(&s(args, "workspace_id")?)?;
            if let Some(state) = args.get("state").and_then(|v| v.as_str()) {
                runs.retain(|run| run.state == state);
            }
            Ok(serde_json::to_value(runs)?)
        }
        "manifest.get" => Ok(serde_json::to_value(r.manifest(&s(args, "manifest_id")?)?)?),
        "prices.list" => {
            let text = std::fs::read_to_string(home.join("prices.yaml")).unwrap_or_default();
            Ok(serde_json::to_value(hli_provider::parse_prices_yaml(
                &text,
            ))?)
        }
        "module.install" => {
            let path = PathBuf::from(s(args, "path")?);
            let meta = std::fs::metadata(&path)?;
            if meta.len() > hli_store::export::HLIMOD_LIMITS.total_bytes {
                return Err(HliError::invalid("modulo excede el limite de 256 MiB"));
            }
            let raw = std::fs::read(&path)?;
            let files =
                hli_store::export::read_container_limited(&raw, hli_store::export::HLIMOD_LIMITS)?;
            let get = |n: &str| files.iter().find(|f| f.path == n).map(|f| f.bytes.clone());
            let manifest_bytes = get("manifest.json")
                .ok_or_else(|| HliError::invalid("paquete sin manifest.json"))?;
            let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
            match manifest["spec"].get("format_version") {
                None | Some(Value::Null) => {}
                Some(v) => match v.as_u64() {
                    Some(n) if n > hli_core::versions::HLIMOD_FORMAT_VERSION as u64 => {
                        return Err(HliError::conflict(format!(
                            "paquete .hlimod con versión futura {n} > {}; actualiza HLI",
                            hli_core::versions::HLIMOD_FORMAT_VERSION
                        )));
                    }
                    Some(n) if n == hli_core::versions::HLIMOD_FORMAT_VERSION as u64 => {}
                    Some(n) => {
                        return Err(HliError::invalid(format!(
                            "paquete .hlimod con versión desconocida {n}"
                        )));
                    }
                    None => return Err(HliError::invalid("format_version no numérico")),
                },
            }
            let name = manifest["metadata"]["name"]
                .as_str()
                .ok_or_else(|| HliError::invalid("manifiesto sin nombre"))?
                .to_string();
            let version = manifest["metadata"]["version"]
                .as_str()
                .ok_or_else(|| HliError::invalid("manifiesto sin versión"))?
                .to_string();
            let sdk = manifest["spec"]["sdk"].as_str().unwrap_or("");
            if !sdk_allows(sdk) {
                return Err(HliError::conflict(format!("SDK incompatible: {sdk}")));
            }
            let checks: Value = serde_json::from_slice(
                &get("checksums.json")
                    .ok_or_else(|| HliError::invalid("paquete sin checksums.json"))?,
            )?;
            let checks_obj = checks
                .as_object()
                .ok_or_else(|| HliError::invalid("checksums inválido"))?;
            if !checks_obj.contains_key("manifest.json") {
                return Err(HliError::invalid("checksums no cubre manifest.json"));
            }
            let entry = manifest["spec"]["runtime"]["entry"].as_str().unwrap_or("");
            if !checks_obj.contains_key(entry) {
                return Err(HliError::invalid(format!(
                    "checksums no cubre el binario {entry}"
                )));
            }
            for (p, h) in checks_obj {
                let f = files
                    .iter()
                    .find(|f| &f.path == p)
                    .ok_or_else(|| HliError::invalid(format!("falta {p} en el paquete")))?;
                let got = format!("sha256:{}", hli_store::export::sha256_hex(&f.bytes));
                if got != h.as_str().unwrap_or("") {
                    return Err(HliError::conflict(format!("checksum no coincide: {p}")));
                }
            }
            // Política de firma: se aplica antes de copiar o ejecutar nada.
            let verification = hli_signing::verify_files(&files, home)?;
            let policy = hli_signing::Policy::load(home);
            let outcome = hli_signing::policy::apply(policy, verification.state);
            if !outcome.accepted {
                return Err(HliError::new(
                    "signature_rejected",
                    format!(
                        "paquete rechazado por política {}: {}",
                        policy.as_str(),
                        verification.message
                    ),
                ));
            }
            let signature_state = verification.state.as_str().to_string();
            let signature_warning = outcome.warning.clone();
            let dir = home.join("modules").join(&name).join(&version);
            std::fs::create_dir_all(&dir)?;
            for f in &files {
                let target = dir.join(&f.path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(target, &f.bytes)?;
            }
            let checksum = format!("sha256:{}", hli_store::export::sha256_hex(&manifest_bytes));
            w.install_module(&name, &version, &dir.to_string_lossy(), manifest, &checksum)?;
            let lock_line = json!({"name": name, "version": version, "checksum": checksum});
            let mut lock = String::new();
            for m in r.modules()? {
                lock.push_str(
                    &json!({"name": m["name"], "version": m["version"], "checksum": m["checksum"]})
                        .to_string(),
                );
                lock.push('\n');
            }
            if !lock.contains(&lock_line["name"].to_string()) {
                lock.push_str(&lock_line.to_string());
                lock.push('\n');
            }
            std::fs::write(home.join("hli.lock"), lock)?;
            Ok(json!({
                "installed": name,
                "version": version,
                "path": dir.to_string_lossy(),
                "signature_state": signature_state,
                "signature_warning": signature_warning
            }))
        }
        "module.list" => {
            let mut modules = r.modules()?;
            for module in &mut modules {
                module["signature_state"] = json!(module_signature_state(home, module));
            }
            Ok(Value::Array(modules))
        }
        "module.verify" => {
            let lock = std::fs::read_to_string(home.join("hli.lock")).unwrap_or_default();
            let mods = r.modules()?;
            for module in &mods {
                let checksum = module["checksum"].as_str().unwrap_or("");
                if !lock.contains(checksum) {
                    return Err(HliError::conflict(format!(
                        "lockfile inconsistente: falta {} {}",
                        module["name"].as_str().unwrap_or("-"),
                        module["version"].as_str().unwrap_or("-")
                    )));
                }
            }
            Ok(json!({"ok": true, "modules": mods.len()}))
        }
        "module.inspect" => {
            let mut module = r.module(&s(args, "name")?)?;
            module["signature_state"] = json!(module_signature_state(home, &module));
            Ok(module)
        }
        "module.test" => {
            let m = r.module(&s(args, "name")?)?;
            let entry = m["manifest"]["spec"]["runtime"]["entry"]
                .as_str()
                .unwrap_or("bin/hli-memory.exe");
            let bin = PathBuf::from(m["path"].as_str().unwrap_or("")).join(entry);
            let tmp = format!("__test_{}", m["name"].as_str().unwrap_or("module"));
            let h = supervisor::start(&tmp, &bin, home, "", w)?;
            let probe = supervisor::health(&tmp, std::time::Duration::from_secs(5))?;
            let _ = supervisor::stop(&tmp);
            Ok(json!({"module": m["name"], "ok": true, "pid": h.pid, "probe": probe}))
        }
        "module.remove" => {
            let name = s(args, "name")?;
            let m = r.module(&name)?;
            let dir = PathBuf::from(m["path"].as_str().unwrap_or(""));
            let _ = std::fs::remove_dir_all(dir.parent().unwrap_or(&dir));
            w.remove_module(&name)?;
            Ok(json!({"removed": name}))
        }
        "mch.create" => {
            let name = s(args, "name")?;
            let module_name = args
                .get("module")
                .and_then(|v| v.as_str())
                .unwrap_or("hli-memory")
                .to_string();
            let m = r.module(&module_name)?;
            let domain = args
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("memory")
                .to_string();
            let ws = o(args, "workspace_id");
            let autostart = args
                .get("autostart")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let out = w.mch_create(
                &name,
                &module_name,
                &domain,
                ws,
                autostart,
                m["manifest"].clone(),
            )?;
            Ok(out)
        }
        "mch.list" => Ok(Value::Array(r.mchs()?)),
        "mch.inspect" => Ok(r.mch(&s(args, "name")?)?),
        "mch.set_autostart" => {
            let name = s(args, "name")?;
            let autostart = args
                .get("autostart")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| HliError::invalid("falta el argumento booleano 'autostart'"))?;
            Ok(w.mch_set_autostart(&name, autostart)?)
        }
        "mch.serve" => {
            let name = s(args, "name")?;
            let (h, caps) = launch_instance(r, w, &name, home)?;
            let out = w.mch_set_state(&name, "ready", Some(h.pid as i64))?;
            Ok(json!({"instance": out, "pid": h.pid, "capabilities": caps}))
        }
        "mch.stop" => {
            let name = s(args, "name")?;
            supervisor::stop(&name)?;
            Ok(w.mch_set_state(&name, "stopped", None)?)
        }
        "mch.status" => {
            let name = s(args, "name")?;
            let instance = r.mch(&name)?;
            let process = supervisor::status(&name);
            if let Some(p) = &process
                && let Some(st) = p["state"].as_str()
            {
                let _ = w.mch_set_state(&name, st, p["pid"].as_i64());
            }
            Ok(json!({"instance": instance, "process": process}))
        }
        "mch.restart" => {
            let name = s(args, "name")?;
            let (bin, ws) = mch_bin(r, &name)?;
            w.mch_set_state(&name, "starting", None)?;
            let h = match supervisor::restart(&name, &bin, home, &ws, w) {
                Ok(h) => h,
                Err(e) => {
                    let _ = w.mch_set_state(&name, "failed", None);
                    return Err(e);
                }
            };
            let out = w.mch_set_state(&name, "ready", Some(h.pid as i64))?;
            Ok(json!({"instance": out, "pid": h.pid}))
        }
        "mch.logs" => {
            let name = s(args, "name")?;
            Ok(json!({"logs": supervisor::logs(&name).unwrap_or_default()}))
        }
        "mch.remove" => {
            let name = s(args, "name")?;
            w.mch_remove(&name)?;
            Ok(json!({"removed": name}))
        }
        "mch.invoke" => {
            let name = s(args, "name")?;
            let op = s(args, "operation")?;
            let m = r.mch(&name)?;
            if m["state"].as_str() != Some("ready") {
                return Err(HliError::conflict("MCH no está ready; ejecuta mch serve"));
            }
            let allowed = m["manifest"]["spec"]["operations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !allowed
                .iter()
                .any(|o| o["name"].as_str() == Some(op.as_str()))
            {
                return Err(HliError::new(
                    "undeclared_operation",
                    format!("operación no declarada por el módulo: {op}"),
                ));
            }
            let ws = args
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .unwrap_or(m["workspace_id"].as_str().unwrap_or(""))
                .to_string();
            let arguments = args.get("arguments").cloned().unwrap_or(json!({}));
            let timeout = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(30);
            let out = supervisor::invoke(
                &name,
                &op,
                &arguments,
                &ws,
                std::time::Duration::from_secs(timeout),
            )?;
            Ok(json!({"instance": name, "operation": op, "result": out}))
        }
        "status" => {
            let ws = r.workspaces()?;
            Ok(json!({"workspaces": ws.len(), "version": env!("CARGO_PKG_VERSION")}))
        }
        other => Err(HliError::new(
            "unknown_command",
            format!("comando desconocido: {other}"),
        )),
    }
}

/// Aprobación heredada (0.3–0.8): ejecuta una herramienta pendiente de un run
/// sin plan durable. Se conserva para exportaciones históricas.
fn approve_legacy(w: &Writer, r: &Reader, run_id: &str) -> Result<Value, HliError> {
    let run = r.run(run_id)?;
    let pending = run
        .result
        .as_ref()
        .and_then(|v| v.get("pending_tool"))
        .cloned()
        .ok_or_else(|| HliError::invalid("el run no tiene herramienta pendiente"))?;
    let call: hli_core::model::ToolCall = serde_json::from_value(pending)?;
    let ws_obj = r.workspace(&run.workspace_id)?;
    let outcome = hli_agent::execute_tool(std::path::Path::new(&ws_obj.root_path), &call);
    w.run_event(
        run_id,
        "tool.completed",
        json!({"run_id": run_id, "tool": outcome.tool, "status": outcome.status,
               "stdout": outcome.stdout, "changed_files": outcome.changed_files,
               "error": outcome.error}),
    )?;
    let manifest = r.manifest_for_run(run_id)?;
    let mut result = run.result.clone().unwrap_or_else(|| json!({}));
    if let Some(o) = result.as_object_mut() {
        o.remove("pending_tool");
    }
    result["observation"] = serde_json::to_value(&outcome)?;
    let usage = hli_provider::Usage {
        input_tokens: manifest.estimated_tokens,
        output_tokens: 0,
        cached_tokens: 0,
        latency_first_token_ms: 0,
        latency_total_ms: 0,
        finish_reason: "stop".into(),
        raw_usage_available: false,
        usage_source: "estimated".into(),
    };
    let cost = hli_provider::cost_of(None, &usage);
    let used_memory_ids: Vec<String> = manifest
        .memories
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_string))
        .collect();
    let used_slice_ids: Vec<String> = manifest
        .artifact_slices
        .iter()
        .filter_map(|s| s["slice_id"].as_str().map(str::to_string))
        .collect();
    let used_event_ids: Vec<String> = manifest
        .recent_events
        .iter()
        .filter_map(|e| e["event_id"].as_str().map(str::to_string))
        .collect();
    let receipt = make_receipt(
        "sim",
        "sim-1",
        "0.2.0",
        &manifest,
        &result,
        &usage,
        &cost,
        vec![],
        used_memory_ids,
        used_slice_ids,
        used_event_ids,
        vec![],
        vec![],
    );
    let state = if outcome.status == "success" {
        "completed"
    } else {
        "failed"
    };
    let done = w.finish_run(run_id, state, Some(result), Some(receipt))?;
    Ok(serde_json::to_value(done)?)
}

fn materialize_references(
    r: &hli_store::reader::Reader,
    workspace_id: &str,
    root: &str,
    thread_id: &str,
    refs: &[hli_core::model::ExplicitReference],
    budget_chars: usize,
) -> hli_core::Result<Vec<hli_core::model::ExplicitReferenceEntry>> {
    use std::path::{Component, Path};

    let mut out = Vec::new();
    let mut used = 0usize;
    for rf in refs {
        let mut e = hli_core::model::ExplicitReferenceEntry {
            reference_type: rf.reference_type.clone(),
            stable_id: rf.stable_id.clone(),
            display_text: rf.display_text.clone(),
            selected_at: rf.selected_at.clone(),
            selection_reason: if rf.selection_reason.is_empty() {
                "user_selected".to_string()
            } else {
                rf.selection_reason.clone()
            },
            validation_state: "valid".to_string(),
            content_hash: rf.content_hash.clone(),
            revision: rf.revision,
            materialization_kind: "metadata".to_string(),
            materialized_artifact_id: None,
            slice_ids: vec![],
            estimated_tokens: 0,
            included: true,
            exclusion_reason: None,
            materialized_text: None,
            detail: json!({}),
        };
        match rf.reference_type.as_str() {
            "file" => {
                let rel = Path::new(&rf.stable_id);
                if rel.is_absolute()
                    || rel
                        .components()
                        .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
                {
                    return Err(HliError::invalid(format!(
                        "reference_invalid_path: {}",
                        rf.stable_id
                    )));
                }
                let path = Path::new(root).join(rel);
                let root_canon =
                    std::fs::canonicalize(root).unwrap_or_else(|_| Path::new(root).to_path_buf());
                let path_canon = std::fs::canonicalize(&path).map_err(|_| {
                    HliError::not_found(&format!("reference_not_found: {}", rf.stable_id))
                })?;
                if !path_canon.starts_with(&root_canon) {
                    return Err(HliError::conflict(format!(
                        "reference_out_of_scope: {}",
                        rf.stable_id
                    )));
                }
                let bytes = std::fs::read(&path_canon).map_err(|_| {
                    HliError::not_found(&format!("reference_not_found: {}", rf.stable_id))
                })?;
                let hash = format!("sha256:{}", hli_store::export::sha256_hex(&bytes));
                if let Some(want) = &rf.content_hash
                    && !want.is_empty()
                    && want != &hash
                {
                    return Err(HliError::conflict(format!(
                        "reference_changed: {}",
                        rf.stable_id
                    )));
                }
                e.content_hash = Some(hash);
                e.detail = json!({"size": bytes.len(), "path": rf.stable_id});
                match String::from_utf8(bytes) {
                    Ok(text) => {
                        let room = budget_chars.saturating_sub(used).min(4000);
                        let cut: String = text.chars().take(room).collect();
                        if !cut.is_empty() {
                            used += cut.chars().count();
                            e.estimated_tokens = ((cut.chars().count() as i64) + 3) / 4;
                            e.materialization_kind = "inline_text".to_string();
                            e.materialized_text = Some(cut);
                        }
                    }
                    Err(_) => e.materialization_kind = "metadata_only".to_string(),
                }
            }
            "memory" => {
                let m = serde_json::to_value(r.memory(&rf.stable_id)?).unwrap_or(json!({}));
                if m["workspace_id"].as_str() != Some(workspace_id) {
                    return Err(HliError::conflict(format!(
                        "reference_out_of_scope: {}",
                        rf.stable_id
                    )));
                }
                let status = m["status"].as_str().unwrap_or("");
                if status != "active" {
                    return Err(HliError::conflict(format!(
                        "reference_{}: {}",
                        status, rf.stable_id
                    )));
                }
                let now = iso_secs(&hli_core::time::now_iso()).unwrap_or(0);
                if let Err(reason) =
                    reference_time_state(m["valid_from"].as_str(), m["valid_until"].as_str(), now)
                {
                    return Err(HliError::conflict(format!(
                        "reference_{}: {}",
                        reason, rf.stable_id
                    )));
                }
                let content: String = m["content"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(1200)
                    .collect();
                used += content.chars().count();
                e.estimated_tokens = ((content.chars().count() as i64) + 3) / 4;
                e.materialization_kind = "inline_text".to_string();
                e.materialized_text = Some(content);
                e.detail = json!({
                    "status": status,
                    "kind": m["kind"],
                    "scope": m["scope"],
                    "source_authority": m["source_authority"]
                });
            }
            "artifact" => {
                let owner = r.artifact_workspace(&rf.stable_id)?;
                if owner != workspace_id {
                    return Err(HliError::conflict(format!(
                        "reference_out_of_scope: {}",
                        rf.stable_id
                    )));
                }
                let a = serde_json::to_value(r.artifact(&rf.stable_id)?).unwrap_or(json!({}));
                let sha = a["sha256"].as_str().unwrap_or("").to_string();
                if let Some(want) = &rf.content_hash
                    && !want.is_empty()
                    && want.trim_start_matches("sha256:") != sha.trim_start_matches("sha256:")
                {
                    return Err(HliError::conflict(format!(
                        "reference_integrity_error: {}",
                        rf.stable_id
                    )));
                }
                e.content_hash = Some(sha.clone());
                e.detail = json!({
                    "sha256": sha,
                    "media_type": a["mime"],
                    "size": a["bytes"]
                });
                let text_like = a["mime"].as_str().is_some_and(|m| m.starts_with("text/"));
                if text_like
                    && a["bytes"].as_i64().unwrap_or(i64::MAX) <= 4000
                    && let Some(path) = a["path"].as_str()
                    && let Ok(bytes) = std::fs::read(path)
                    && let Ok(text) = String::from_utf8(bytes)
                {
                    used += text.chars().count();
                    e.estimated_tokens = ((text.chars().count() as i64) + 3) / 4;
                    e.materialization_kind = "inline_text".to_string();
                    e.materialized_text = Some(text.chars().take(4000).collect());
                } else {
                    e.materialization_kind = "metadata_only".to_string();
                }
            }
            "run" => {
                let run = serde_json::to_value(r.run(&rf.stable_id)?).unwrap_or(json!({}));
                if run["workspace_id"].as_str() != Some(workspace_id) {
                    return Err(HliError::conflict(format!(
                        "reference_out_of_scope: {}",
                        rf.stable_id
                    )));
                }
                let text: String = run["result"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(1200)
                    .collect();
                if !text.is_empty() {
                    used += text.chars().count();
                    e.estimated_tokens = ((text.chars().count() as i64) + 3) / 4;
                    e.materialization_kind = "inline_text".to_string();
                    e.materialized_text = Some(text);
                }
                e.detail = json!({
                    "state": run["state"],
                    "receipt_hash": run["receipt"]["result_hash"],
                    "result_artifacts": run["receipt"]["result_artifacts"]
                });
            }
            "branch" => {
                let branches = r.branches(thread_id)?;
                let found = branches
                    .iter()
                    .find(|b| b.name == rf.stable_id || b.id == rf.stable_id)
                    .ok_or_else(|| {
                        HliError::not_found(&format!("reference_not_found: {}", rf.stable_id))
                    })?;
                if let Some(rev) = rf.revision
                    && rev != found.head_sequence
                {
                    return Err(HliError::conflict(format!(
                        "reference_revision_mismatch: {}",
                        rf.stable_id
                    )));
                }
                e.revision = Some(found.head_sequence);
                e.detail = json!({
                    "branch_id": found.id,
                    "thread_id": found.thread_id,
                    "revision": found.head_sequence
                });
            }
            other => {
                return Err(HliError::invalid(format!("reference_unsupported: {other}")));
            }
        }
        out.push(e);
    }
    Ok(out)
}

fn iso_secs(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let h: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let se: i64 = s.get(17..19)?.parse().ok()?;
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

/// Comprueba los límites temporales de una memoria con timestamps parseados.
fn reference_time_state(
    valid_from: Option<&str>,
    valid_until: Option<&str>,
    now: i64,
) -> std::result::Result<(), &'static str> {
    if let Some(v) = valid_from {
        let t = iso_secs(v).ok_or("invalid_timestamp")?;
        if t > now {
            return Err("not_yet_valid");
        }
    }
    if let Some(v) = valid_until {
        let t = iso_secs(v).ok_or("invalid_timestamp")?;
        if t < now {
            return Err("expired");
        }
    }
    Ok(())
}

#[cfg(test)]
mod reference_time_tests {
    use super::*;

    #[test]
    fn parses_iso_timestamp() {
        assert!(iso_secs("2026-11-08T10:00:00Z").is_some());
    }

    #[test]
    fn rejects_malformed_timestamp() {
        assert!(iso_secs("not-a-date").is_none());
        assert_eq!(
            reference_time_state(Some("xx"), None, 0),
            Err("invalid_timestamp")
        );
    }

    #[test]
    fn future_memory_is_not_yet_valid() {
        assert_eq!(
            reference_time_state(Some("2030-01-01T00:00:00Z"), None, 1_700_000_000),
            Err("not_yet_valid")
        );
    }

    #[test]
    fn expired_memory_is_rejected() {
        assert_eq!(
            reference_time_state(None, Some("2020-01-01T00:00:00Z"), 1_700_000_000),
            Err("expired")
        );
    }

    #[test]
    fn active_window_is_valid() {
        assert_eq!(
            reference_time_state(
                Some("2020-01-01T00:00:00Z"),
                Some("2030-01-01T00:00:00Z"),
                1_700_000_000
            ),
            Ok(())
        );
    }
}

#[cfg(test)]
mod sdk_tests {
    use super::sdk_allows;

    #[test]
    fn accepts_compatible_sdk() {
        assert!(sdk_allows(">=0.4 <0.5"));
        assert!(sdk_allows("^0.4"));
    }

    #[test]
    fn module_rejects_incompatible_sdk() {
        assert!(!sdk_allows(">=9.0 <9.1"));
        assert!(!sdk_allows(">=0.5 <0.6"));
    }
}
