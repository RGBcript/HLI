//! Daemon de HLI 0.1: API local por TCP + NDJSON.
//!
//! Decisión (ADR-0001): el transporte de 0.1 es un socket TCP en loopback con
//! mensajes JSON delimitados por línea. HTTP/WebSocket llega en 0.2 sin cambiar
//! el contrato de comandos.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hli_core::{time::now_iso, HliError};
mod supervisor;

use hli_agent::{build_manifest, execute_tool, permission_for, Permission, Provider, SimProvider};
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

    eprintln!("hli-daemon escuchando en 127.0.0.1:{port} (home={})", home.display());

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

fn handle(stream: TcpStream, writer: Writer, reader: Arc<Reader>, home: PathBuf) -> std::io::Result<()> {
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
                let resp = json!({"id": id, "ok": false, "error": {"code": e.code, "message": e.message}});
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

fn make_receipt(
    provider: &SimProvider,
    manifest: &hli_core::model::ContextManifest,
    result: &Value,
) -> hli_core::model::Receipt {
    hli_core::model::Receipt {
        provider: provider.name().to_string(),
        model: provider.model().to_string(),
        adapter_version: "0.2.0".into(),
        context_manifest: manifest.manifest_id.clone(),
        tool_schemas: vec!["filesystem.read".into(), "filesystem.write".into()],
        effective_permissions: json!({
            "filesystem.read": "allow",
            "filesystem.write": "ask",
            "shell.execute": "deny"
        }),
        nonsecret_environment: json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH}),
        secret_handles: vec![],
        input_artifacts: vec![],
        module_versions: vec![],
        sampling: json!({"temperature": 0.0, "seed": 0}),
        result_artifacts: vec![],
        sandbox: json!({"mode": "none", "note": "0.2 no implementa sandbox; shell denegado"}),
        result_hash: hli_core::canon::hash_canonical(result),
        terminal_event: None,
    }
}

fn s(args: &Value, key: &str) -> Result<String, HliError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| HliError::invalid(format!("falta el argumento '{key}'")))
}

fn o(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|v| v.to_string())
}

fn dispatch(cmd: &str, args: &Value, w: &Writer, r: &Reader, home: &PathBuf) -> Result<Value, HliError> {
    match cmd {
        "daemon.ping" => Ok(json!({"pong": true, "version": "0.1.0", "pid": std::process::id()})),
        "daemon.stop" => Ok(json!({"stopping": true})),
        "workspace.create" => {
            let ws = w.create_workspace(&s(args, "name")?, args.get("root").and_then(|v| v.as_str()).unwrap_or("."))?;
            Ok(serde_json::to_value(ws)?)
        }
        "workspace.list" => Ok(serde_json::to_value(r.workspaces()?)?),
        "workspace.get" => Ok(serde_json::to_value(r.workspace(&s(args, "workspace_id")?)?)?),
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
                    r.event(&from)?
                        .thread_id
                        .ok_or_else(|| HliError::invalid("el evento no pertenece a ningún thread"))?
                }
            };
            let b = w.create_branch(&thread_id, &s(args, "name")?, o(args, "from_event"))?;
            Ok(serde_json::to_value(b)?)
        }
        "branch.list" => Ok(serde_json::to_value(r.branches(&s(args, "thread_id")?)?)?),
        "branch.show" => Ok(serde_json::to_value(r.branch_state(&s(args, "branch_id")?)?)?),
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
        "event.list" => Ok(serde_json::to_value(
            r.events(&s(args, "workspace_id")?, args.get("branch_id").and_then(|v| v.as_str()))?,
        )?),
        "artifact.put" => {
            let info = w.put_artifact(&s(args, "workspace_id")?, PathBuf::from(s(args, "path")?))?;
            Ok(serde_json::to_value(info)?)
        }
        "artifact.list" => Ok(Value::Array(r.artifacts(&s(args, "workspace_id")?)?)),
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
            let supersedes = args.get("supersedes").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let contradicts = args.get("contradicts").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let m = w.commit_memory(
                &s(args, "workspace_id")?,
                args.get("scope").and_then(|v| v.as_str()).unwrap_or("workspace"),
                o(args, "scope_id"),
                &s(args, "type")?,
                &s(args, "content")?,
                supersedes.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
                contradicts.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            )?;
            Ok(serde_json::to_value(m)?)
        }
        "memory.get" => Ok(serde_json::to_value(r.memory(&s(args, "memory_id")?)?)?),
        "memory.list" => Ok(serde_json::to_value(r.memories(&s(args, "workspace_id")?)?)?),
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
        "memory.forget" => Ok(serde_json::to_value(w.forget_memory(&s(args, "memory_id")?)?)?),
        "run.start" => {
            let ws = s(args, "workspace_id")?;
            let th = s(args, "thread_id")?;
            let br = s(args, "branch_id")?;
            let prompt = s(args, "prompt")?;
            let budget = args.get("budget_tokens").and_then(|v| v.as_i64()).unwrap_or(8_192);

            let run = w.create_run(&ws, &th, &br, &prompt)?;
            let ws_obj = r.workspace(&ws)?;
            let instructions = ws_obj
                .config
                .get("instructions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let visible = r.visible_events(&br, run.branch_revision)?;
            let memories = r.search_memories(&ws, &prompt, 5)?;
            let manifest = build_manifest(&run, &visible, instructions, &prompt, &memories, budget);
            let manifest = w.save_manifest(manifest)?;

            w.run_event(
                &run.id,
                "model.requested",
                json!({"run_id": run.id, "manifest_id": manifest.manifest_id}),
            )?;

            let provider = SimProvider;
            let req = hli_agent::ProviderRequest {
                run_id: run.id.clone(),
                prompt: prompt.clone(),
                manifest: manifest.clone(),
            };
            let resp = provider.complete(&req)?;
            w.run_event(
                &run.id,
                "model.responded",
                json!({"run_id": run.id, "text": resp.text}),
            )?;

            let mut result = json!({"text": resp.text, "manifest_id": manifest.manifest_id});
            let mut state = "completed";

            if let Some(call) = resp.tool_call {
                w.run_event(
                    &run.id,
                    "tool.requested",
                    json!({"run_id": run.id, "tool": call.name, "arguments": call.arguments}),
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
                            json!({"run_id": run.id, "tool": outcome.tool,
                                   "status": outcome.status, "stdout": outcome.stdout,
                                   "changed_files": outcome.changed_files,
                                   "error": outcome.error}),
                        )?;
                        result["observation"] = serde_json::to_value(&outcome)?;
                        if outcome.status != "success" {
                            state = "failed";
                        }
                    }
                }
            }

            let receipt = make_receipt(&provider, &manifest, &result);
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
            let receipt = make_receipt(&SimProvider, &manifest, &result);
            let state = if outcome.status == "success" { "completed" } else { "failed" };
            let done = w.finish_run(&run_id, state, Some(result), Some(receipt))?;
            Ok(serde_json::to_value(done)?)
        }
        "run.get" => {
            let run_id = s(args, "run_id")?;
            let run = r.run(&run_id)?;
            let manifest = r.manifest_for_run(&run_id).ok();
            Ok(json!({"run": run, "manifest": manifest}))
        }
        "run.list" => Ok(serde_json::to_value(r.runs(&s(args, "workspace_id")?)?)?),
        "manifest.get" => Ok(serde_json::to_value(r.manifest(&s(args, "manifest_id")?)?)?),
        "module.install" => {
            let path = PathBuf::from(s(args, "path")?);
            let raw = std::fs::read(&path)?;
            let files = hli_store::export::read_container(&raw)?;
            let get = |n: &str| files.iter().find(|f| f.path == n).map(|f| f.bytes.clone());
            let manifest_bytes = get("manifest.json")
                .ok_or_else(|| HliError::invalid("paquete sin manifest.json"))?;
            let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
            let name = manifest["metadata"]["name"].as_str()
                .ok_or_else(|| HliError::invalid("manifiesto sin nombre"))?.to_string();
            let version = manifest["metadata"]["version"].as_str()
                .ok_or_else(|| HliError::invalid("manifiesto sin versión"))?.to_string();
            let sdk = manifest["spec"]["sdk"].as_str().unwrap_or("");
            if !sdk.contains("0.4") {
                return Err(HliError::conflict(format!("SDK incompatible: {sdk}")));
            }
            let checks: Value = serde_json::from_slice(
                &get("checksums.json").ok_or_else(|| HliError::invalid("paquete sin checksums.json"))?,
            )?;
            for (p, h) in checks.as_object().ok_or_else(|| HliError::invalid("checksums inválido"))? {
                let f = files.iter().find(|f| &f.path == p)
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
                lock.push_str(&json!({"name": m["name"], "version": m["version"], "checksum": m["checksum"]}).to_string());
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
        "module.inspect" => Ok(r.module(&s(args, "name")?)?),
        "module.test" => {
            let m = r.module(&s(args, "name")?)?;
            let entry = m["manifest"]["spec"]["runtime"]["entry"].as_str().unwrap_or("bin/hli-memory.exe");
            let bin = PathBuf::from(m["path"].as_str().unwrap_or("")).join(entry);
            let out = supervisor::health(&bin, home, "")?;
            Ok(json!({"module": m["name"], "ok": true, "probe": out}))
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
            let module_name = args.get("module").and_then(|v| v.as_str()).unwrap_or("hli-memory").to_string();
            let m = r.module(&module_name)?;
            let domain = args.get("domain").and_then(|v| v.as_str()).unwrap_or("memory").to_string();
            let ws = o(args, "workspace_id");
            let out = w.mch_create(&name, &module_name, &domain, ws, m["manifest"].clone())?;
            Ok(out)
        }
        "mch.list" => Ok(Value::Array(r.mchs()?)),
        "mch.inspect" => Ok(r.mch(&s(args, "name")?)?),
        "mch.serve" => {
            let name = s(args, "name")?;
            let m = r.mch(&name)?;
            let module = m["module"].as_str().unwrap_or("");
            let mm = r.module(module)?;
            let entry = mm["manifest"]["spec"]["runtime"]["entry"].as_str().unwrap_or("bin/hli-memory.exe");
            let bin = PathBuf::from(mm["path"].as_str().unwrap_or("")).join(entry);
            let ws = m["workspace_id"].as_str().unwrap_or("");
            let probe = supervisor::health(&bin, home, ws)?;
            let out = w.mch_set_state(&name, "ready", None)?;
            Ok(json!({"instance": out, "probe": probe}))
        }
        "mch.stop" => Ok(w.mch_set_state(&s(args, "name")?, "stopped", None)?),
        "mch.remove" => {
            let name = s(args, "name")?;
            w.mch_remove(&name)?;
            Ok(json!({"removed": name}))
        }
        "mch.invoke" => {
            let name = s(args, "name")?;
            let op = s(args, "operation")?;
            let m = r.mch(&name)?;
            let allowed = m["manifest"]["spec"]["operations"].as_array().cloned().unwrap_or_default();
            if !allowed.iter().any(|o| o["name"].as_str() == Some(op.as_str())) {
                return Err(HliError::new("undeclared_operation",
                    format!("operación no declarada por el módulo: {op}")));
            }
            let module = m["module"].as_str().unwrap_or("");
            let mm = r.module(module)?;
            let entry = mm["manifest"]["spec"]["runtime"]["entry"].as_str().unwrap_or("bin/hli-memory.exe");
            let bin = PathBuf::from(mm["path"].as_str().unwrap_or("")).join(entry);
            let ws = args.get("workspace_id").and_then(|v| v.as_str()).unwrap_or(
                m["workspace_id"].as_str().unwrap_or(""));
            let arguments = args.get("arguments").cloned().unwrap_or(json!({}));
            let out = supervisor::invoke(&bin, home, ws, &op, &arguments, std::time::Duration::from_secs(30))?;
            Ok(json!({"instance": name, "operation": op, "result": out.result,
                       "duration_ms": out.duration_ms}))
        }
        "status" => {
            let ws = r.workspaces()?;
            Ok(json!({"workspaces": ws.len(), "version": "0.1.0"}))
        }
        other => Err(HliError::new("unknown_command", format!("comando desconocido: {other}"))),
    }
}
