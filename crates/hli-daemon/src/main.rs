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

use hli_core::{time::now_iso, HliError};
mod providers;
mod supervisor;

use hli_agent::{build_manifest, execute_tool, permission_for, Permission, Provider, SimProvider};
use hli_provider::{ChatProvider, ChatRequest, OllamaProvider, OpenAiProvider};
use hli_store::{Reader, Store, Writer};
use serde_json::{json, Value};

fn main() {
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
    // Store::open normaliza los PID y estados de la ejecución anterior antes de
    // publicar daemon.json: ningún proceso persistido se da por vivo.
    let store = Store::open(&home)?;
    let (writer, reader) = store.start()?;
    let reader = Arc::new(reader);

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();

    let info = json!({
        "version": "0.1.0",
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
    r: &Reader,
    home: &std::path::Path,
) -> Result<Value, HliError> {
    match cmd {
        "daemon.ping" => Ok(json!({"pong": true, "version": "0.1.0", "pid": std::process::id()})),
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
            let mut api_key = String::new();
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
                api_key = value;
                secret_handles.push(json!({"id": handle, "version": version}));
            }

            let run = w.create_run(&ws, &th, &br, &prompt)?;
            let ws_obj = r.workspace(&ws)?;
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
            let mut manifest =
                build_manifest(&run, &visible, instructions, &prompt, &memories, budget);

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
                if let Some(o) = m.as_object_mut() {
                    if let Some(c) = o.get("content").and_then(|v| v.as_str()) {
                        if c.chars().count() > 400 {
                            let cut: String = c.chars().take(400).collect();
                            o.insert("content".into(), json!(format!("{cut}…")));
                        }
                    }
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
            let rendered = hli_agent::render_payload(&manifest, &reloaded);

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

            let is_async = args.get("async").and_then(|v| v.as_bool()).unwrap_or(false);
            if is_async {
                let cancel = Arc::new(AtomicBool::new(false));
                cancels()
                    .lock()
                    .unwrap()
                    .insert(run.id.clone(), cancel.clone());
                let w2 = w.clone();
                let run2 = run.clone();
                let manifest2 = manifest.clone();
                let prompt2 = prompt.clone();
                let rendered2 = rendered.clone();
                let provider2 = provider_name.clone();
                let model2 = model.clone();
                let base2 = base_url.clone();
                let api2 = api_key.clone();
                let secret2 = secret_handles.clone();
                let cand2 = candidate_memory_ids.clone();
                let system2 = "Eres HLI. Responde en español, breve y factual.".to_string();
                let sim_delay_ms = args.get("sim_delay_ms").and_then(|v| v.as_u64());
                std::thread::spawn(move || {
                    let _ = w2.set_run_state(&run2.id, "running");
                    let started = std::time::Instant::now();
                    if provider2 == "sim" {
                        if let Some(ms) = sim_delay_ms {
                            std::thread::sleep(Duration::from_millis(ms));
                        }
                    }
                    let call: Result<
                        (String, hli_provider::Usage, String, String, String),
                        HliError,
                    > = match provider2.as_str() {
                        "sim" => {
                            let p = SimProvider;
                            let pr = hli_agent::ProviderRequest {
                                run_id: run2.id.clone(),
                                prompt: prompt2,
                                manifest: manifest2.clone(),
                            };
                            match p.complete(&pr) {
                                Ok(resp) if resp.tool_call.is_none() => {
                                    let u = hli_provider::Usage {
                                        input_tokens: hli_provider::estimate_tokens(&system2)
                                            + manifest2.estimated_tokens,
                                        output_tokens: hli_provider::estimate_tokens(&resp.text),
                                        cached_tokens: 0,
                                        latency_first_token_ms: 0,
                                        latency_total_ms: 0,
                                        finish_reason: "stop".into(),
                                        raw_usage_available: false,
                                        usage_source: "estimated".into(),
                                    };
                                    Ok((resp.text, u, "sim".into(), "sim-1".into(), "0.2.0".into()))
                                }
                                Ok(_) => Err(HliError::internal(
                                    "tool loop no soportado en runs asincronos",
                                )),
                                Err(e) => Err(e),
                            }
                        }
                        "ollama" => {
                            let p = hli_provider::OllamaProvider::new(&base2, &model2, timeout);
                            let req = ChatRequest {
                                system: &system2,
                                user: &rendered2,
                                temperature,
                                max_tokens,
                            };
                            p.chat(&req)
                                .map(|r| (r.text, r.usage, r.provider, r.model, r.adapter_version))
                        }
                        "openai" => {
                            let p =
                                hli_provider::OpenAiProvider::new(&base2, &model2, &api2, timeout);
                            let req = ChatRequest {
                                system: &system2,
                                user: &rendered2,
                                temperature,
                                max_tokens,
                            };
                            p.chat(&req)
                                .map(|r| (r.text, r.usage, r.provider, r.model, r.adapter_version))
                        }
                        other => Err(HliError::invalid(format!("proveedor desconocido: {other}"))),
                    };
                    let cancelled = cancel.load(Ordering::SeqCst);
                    let usage_default = hli_provider::Usage::default();
                    let (text, usage, up, um, adapter) = match call {
                        Ok(v) => v,
                        Err(e) => {
                            let state = if e.message.contains("timeout")
                                || e.message.contains("timed out")
                            {
                                "timed_out"
                            } else {
                                "failed"
                            };
                            let cost = hli_provider::cost_of(None, &usage_default);
                            let receipt = make_receipt(
                                &provider2,
                                &model2,
                                "error",
                                &manifest2,
                                &json!({"error": e.message}),
                                &usage_default,
                                &cost,
                                secret2,
                                vec![],
                                vec![],
                                vec![],
                                cand2,
                                vec![],
                            );
                            let _ = w2.finish_run(
                                &run2.id,
                                state,
                                Some(json!({"error": e.message})),
                                Some(receipt),
                            );
                            cancels().lock().unwrap().remove(&run2.id);
                            return;
                        }
                    };
                    let used_memory_ids: Vec<String> = manifest2
                        .memories
                        .iter()
                        .filter_map(|m| m["id"].as_str().map(str::to_string))
                        .collect();
                    let used_slice_ids: Vec<String> = manifest2
                        .artifact_slices
                        .iter()
                        .filter_map(|sl| sl["slice_id"].as_str().map(str::to_string))
                        .collect();
                    let used_event_ids: Vec<String> = manifest2
                        .recent_events
                        .iter()
                        .filter_map(|ev| ev["event_id"].as_str().map(str::to_string))
                        .collect();
                    let rejected: Vec<String> = cand2
                        .iter()
                        .filter(|id| !used_memory_ids.contains(id))
                        .cloned()
                        .collect();
                    let cost = hli_provider::cost_of(None, &usage);
                    let receipt = make_receipt(
                        &up,
                        &um,
                        &adapter,
                        &manifest2,
                        &json!({"text": text}),
                        &usage,
                        &cost,
                        secret2,
                        used_memory_ids,
                        used_slice_ids,
                        used_event_ids,
                        cand2,
                        rejected,
                    );
                    let timed_out = started.elapsed() > Duration::from_secs(timeout);
                    let state = if cancelled {
                        "cancelled"
                    } else if timed_out {
                        "timed_out"
                    } else {
                        "completed"
                    };
                    let _ =
                        w2.finish_run(&run2.id, state, Some(json!({"text": text})), Some(receipt));
                    cancels().lock().unwrap().remove(&run2.id);
                });
                return Ok(serde_json::to_value(run)?);
            }

            let system = "Eres HLI. Responde en español, breve y factual.";
            let system_owned = system.to_string();
            let req = hli_provider::ChatRequest {
                system: &system_owned,
                user: &rendered,
                temperature,
                max_tokens,
            };

            let mut tool_call = None;
            let call_result: Result<
                (String, hli_provider::Usage, String, String, String),
                HliError,
            > = match provider_name.as_str() {
                "sim" => {
                    let p = SimProvider;
                    let pr = hli_agent::ProviderRequest {
                        run_id: run.id.clone(),
                        prompt: prompt.clone(),
                        manifest: manifest.clone(),
                    };
                    match p.complete(&pr) {
                        Ok(resp) => {
                            tool_call = resp.tool_call;
                            let u = hli_provider::Usage {
                                input_tokens: hli_provider::estimate_tokens(&system_owned)
                                    + manifest.estimated_tokens,
                                output_tokens: hli_provider::estimate_tokens(&resp.text),
                                cached_tokens: 0,
                                latency_first_token_ms: 0,
                                latency_total_ms: 0,
                                finish_reason: "stop".into(),
                                raw_usage_available: false,
                                usage_source: "estimated".into(),
                            };
                            Ok((
                                resp.text,
                                u,
                                "sim".to_string(),
                                "sim-1".to_string(),
                                "0.2.0".to_string(),
                            ))
                        }
                        Err(e) => Err(e),
                    }
                }
                "ollama" => {
                    let p = hli_provider::OllamaProvider::new(&base_url, &model, timeout);
                    p.chat(&req)
                        .map(|r| (r.text, r.usage, r.provider, r.model, r.adapter_version))
                }
                "openai" => {
                    let p = hli_provider::OpenAiProvider::new(&base_url, &model, &api_key, timeout);
                    p.chat(&req)
                        .map(|r| (r.text, r.usage, r.provider, r.model, r.adapter_version))
                }
                other => Err(HliError::invalid(format!("proveedor desconocido: {other}"))),
            };
            let (text, usage, used_provider, used_model, adapter) = match call_result {
                Ok(v) => v,
                Err(e) => {
                    let usage = hli_provider::Usage::default();
                    let cost = hli_provider::cost_of(None, &usage);
                    let receipt = make_receipt(
                        &provider_name,
                        &model,
                        "error",
                        &manifest,
                        &json!({"error": e.message}),
                        &usage,
                        &cost,
                        secret_handles,
                        vec![],
                        vec![],
                        vec![],
                        candidate_memory_ids,
                        vec![],
                    );
                    let done = w.finish_run(
                        &run.id,
                        "failed",
                        Some(json!({"error": e.message})),
                        Some(receipt),
                    )?;
                    return Ok(serde_json::to_value(done)?);
                }
            };

            let price = load_price(home, &used_provider, &used_model);
            let cost = hli_provider::cost_of(price.as_ref(), &usage);
            w.run_event(
                &run.id,
                "model.responded",
                json!({
                    "run_id": run.id, "provider": used_provider, "model": used_model,
                    "usage": usage, "cost": cost
                }),
            )?;

            let mut result = json!({
                "text": text, "manifest_id": manifest.manifest_id,
                "provider": used_provider, "model": used_model
            });
            let mut state = "completed";

            if let Some(call) = tool_call {
                w.run_event(
                    &run.id,
                    "tool.requested",
                    json!({
                        "run_id": run.id, "tool": call.name, "arguments": call.arguments
                    }),
                )?;
                match permission_for(&call.name) {
                    Permission::Deny => {
                        w.run_event(
                            &run.id,
                            "tool.denied",
                            json!({"run_id": run.id, "tool": call.name}),
                        )?;
                        state = "failed";
                        result["error"] = json!("herramienta denegada por política");
                    }
                    Permission::Ask => {
                        state = "waiting_approval";
                        result["pending_tool"] = serde_json::to_value(&call)?;
                    }
                    Permission::Allow => {
                        let outcome = execute_tool(std::path::Path::new(&ws_obj.root_path), &call);
                        w.run_event(
                            &run.id,
                            "tool.completed",
                            json!({
                                "run_id": run.id, "tool": outcome.tool, "status": outcome.status,
                                "stdout": outcome.stdout, "changed_files": outcome.changed_files,
                                "error": outcome.error
                            }),
                        )?;
                        result["observation"] = serde_json::to_value(&outcome)?;
                        if outcome.status != "success" {
                            state = "failed";
                        }
                    }
                }
            }

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
            let rejected_memory_ids: Vec<String> = candidate_memory_ids
                .iter()
                .filter(|id| !used_memory_ids.contains(id))
                .cloned()
                .collect();
            let receipt = make_receipt(
                &used_provider,
                &used_model,
                &adapter,
                &manifest,
                &result,
                &usage,
                &cost,
                secret_handles,
                used_memory_ids,
                used_slice_ids,
                used_event_ids,
                candidate_memory_ids,
                rejected_memory_ids,
            );
            let done = w.finish_run(&run.id, state, Some(result), Some(receipt))?;
            Ok(serde_json::to_value(done)?)
        }
        "run.approve" => {
            let run_id = s(args, "run_id")?;
            let run = r.run(&run_id)?;
            let pending = run
                .result
                .as_ref()
                .and_then(|v| v.get("pending_tool"))
                .cloned()
                .ok_or_else(|| HliError::invalid("el run no tiene herramienta pendiente"))?;
            let call: hli_core::model::ToolCall = serde_json::from_value(pending)?;
            let ws_obj = r.workspace(&run.workspace_id)?;
            let outcome = execute_tool(std::path::Path::new(&ws_obj.root_path), &call);
            w.run_event(
                &run_id,
                "tool.completed",
                json!({"run_id": run_id, "tool": outcome.tool, "status": outcome.status,
                       "stdout": outcome.stdout, "changed_files": outcome.changed_files,
                       "error": outcome.error}),
            )?;
            let manifest = r.manifest_for_run(&run_id)?;
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
            let done = w.finish_run(&run_id, state, Some(result), Some(receipt))?;
            Ok(serde_json::to_value(done)?)
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
                if hli_core::model::run_is_terminal(&run.state) {
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
            if run.state == "queued" {
                let done =
                    w.finish_run(&run_id, "cancelled", Some(json!({"cancelled": true})), None)?;
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
            Ok(json!({"installed": name, "version": version, "path": dir.to_string_lossy()}))
        }
        "module.list" => Ok(Value::Array(r.modules()?)),
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
        "module.inspect" => Ok(r.module(&s(args, "name")?)?),
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
            if let Some(p) = &process {
                if let Some(st) = p["state"].as_str() {
                    let _ = w.mch_set_state(&name, st, p["pid"].as_i64());
                }
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
            Ok(json!({"workspaces": ws.len(), "version": "0.1.0"}))
        }
        other => Err(HliError::new(
            "unknown_command",
            format!("comando desconocido: {other}"),
        )),
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
