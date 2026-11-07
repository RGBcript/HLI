//! CLI de HLI 0.1. Cliente del daemon; nunca escribe en SQLite directamente.

use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("hli: {e}");
            std::process::exit(1);
        }
    }
}

fn home() -> PathBuf {
    if let Ok(h) = std::env::var("HLI_HOME") {
        return PathBuf::from(h);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".hli")
}

fn daemon_file() -> PathBuf {
    home().join("daemon.json")
}

fn daemon_info() -> Result<Value, String> {
    let raw = std::fs::read_to_string(daemon_file())
        .map_err(|_| "daemon no está en marcha (falta daemon.json)".to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("daemon.json inválido: {e}"))
}

fn call(cmd: &str, args: Value) -> Result<Value, String> {
    let info = daemon_info()?;
    let port = info["port"].as_u64().ok_or("daemon.json sin puerto")? as u16;
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("no se pudo conectar al daemon: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let req = json!({"id": "1", "cmd": cmd, "args": args});
    writeln!(stream, "{}", req).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| format!("sin respuesta del daemon: {e}"))?;
    let resp: Value =
        serde_json::from_str(&line).map_err(|e| format!("respuesta inválida: {e}"))?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        let code = resp["error"]["code"].as_str().unwrap_or("error");
        let msg = resp["error"]["message"].as_str().unwrap_or("sin detalle");
        Err(format!("{code}: {msg}"))
    }
}

fn run(args: &[String]) -> Result<(), String> {
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("hli {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.is_empty() {
        if std::io::stdout().is_terminal() {
            let tui = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name(if cfg!(windows) {
                    "hli-tui.exe"
                } else {
                    "hli-tui"
                });
            std::process::Command::new(tui)
                .env("HLI_HOME", home())
                .status()
                .map_err(|e| format!("no se pudo abrir la TUI: {e}"))?;
            return Ok(());
        }
        return usage();
    }
    let output = output_mode(args);
    match (args[0].as_str(), args.get(1).map(String::as_str)) {
        ("daemon", Some("start")) => daemon_start(),
        ("daemon", Some("stop")) => daemon_stop(),
        ("daemon", Some("status")) => daemon_status(),
        ("workspace", Some("create")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            let root = flag(args, "--root").unwrap_or_else(|| ".".into());
            let v = call("workspace.create", json!({"name": name, "root": root}))?;
            print_value(&v, output)
        }
        ("workspace", Some("list")) => print_value(&call("workspace.list", json!({}))?, output),
        ("workspace", Some("get")) => {
            let id = args.get(2).ok_or("falta workspace_id")?;
            print_value(&call("workspace.get", json!({"workspace_id": id}))?, output)
        }
        ("thread", Some("create")) => {
            let title = args.get(2).ok_or("falta el título")?;
            let ws = need_flag(args, "--workspace")?;
            print_value(
                &call("thread.create", json!({"workspace_id": ws, "title": title}))?,
                output,
            )
        }
        ("thread", Some("list")) => {
            let ws = need_flag(args, "--workspace")?;
            print_value(&call("thread.list", json!({"workspace_id": ws}))?, output)
        }
        ("branch", Some("create")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            let thread = flag(args, "--thread");
            let from = flag(args, "--from");
            if thread.is_none() && from.is_none() {
                return Err("falta --thread o --from".into());
            }
            print_value(
                &call(
                    "branch.create",
                    json!({"thread_id": thread, "name": name, "from_event": from}),
                )?,
                output,
            )
        }
        ("branch", Some("list")) => {
            let thread = need_flag(args, "--thread")?;
            print_value(&call("branch.list", json!({"thread_id": thread}))?, output)
        }
        ("branch", Some("show")) => {
            let id = args.get(2).ok_or("falta branch_id")?;
            print_value(&call("branch.show", json!({"branch_id": id}))?, output)
        }
        ("branch", Some("snapshot")) => {
            let id = args.get(2).ok_or("falta branch_id")?;
            print_value(&call("branch.snapshot", json!({"branch_id": id}))?, output)
        }
        ("event", Some("append")) => {
            let kind = need_flag(args, "--type")?;
            let ws = need_flag(args, "--workspace")?;
            let payload_raw = flag(args, "--payload").unwrap_or_else(|| "{}".into());
            let payload: Value =
                serde_json::from_str(&payload_raw).map_err(|e| format!("payload inválido: {e}"))?;
            let v = call(
                "event.append",
                json!({
                    "workspace_id": ws,
                    "thread_id": flag(args, "--thread"),
                    "branch_id": flag(args, "--branch"),
                    "type": kind,
                    "payload": payload,
                    "idempotency_key": flag(args, "--idem")
                }),
            )?;
            print_value(&v, output)
        }
        ("event", Some("list")) => {
            let ws = need_flag(args, "--workspace")?;
            let branch = flag(args, "--branch");
            print_value(
                &call(
                    "event.list",
                    json!({"workspace_id": ws, "branch_id": branch}),
                )?,
                output,
            )
        }
        ("artifact", Some("put")) => {
            let path = args.get(2).ok_or("falta la ruta del archivo")?;
            let ws = need_flag(args, "--workspace")?;
            print_value(
                &call("artifact.put", json!({"workspace_id": ws, "path": path}))?,
                output,
            )
        }
        ("artifact", Some("slice")) => {
            let id = args.get(2).ok_or("falta artifact_id")?;
            let ws = need_flag(args, "--workspace")?;
            let v = call(
                "artifact.slice",
                json!({
                    "artifact_id": id, "workspace_id": ws,
                    "kind": flag(args, "--kind").unwrap_or_else(|| "full_text_if_small".into()),
                    "start": flag(args, "--start").and_then(|v| v.parse::<u64>().ok()),
                    "end": flag(args, "--end").and_then(|v| v.parse::<u64>().ok()),
                    "reason": flag(args, "--reason").unwrap_or_else(|| "selected".into())
                }),
            )?;
            print_value(&v, output)
        }
        ("artifact", Some("list")) => {
            let ws = need_flag(args, "--workspace")?;
            print_value(&call("artifact.list", json!({"workspace_id": ws}))?, output)
        }
        ("verify", _) => {
            let ws = need_flag(args, "--workspace")?;
            print_value(&call("verify", json!({"workspace_id": ws}))?, output)
        }
        ("export", _) => {
            let ws = args.get(1).ok_or("falta workspace_id")?;
            let out = need_flag(args, "--out")?;
            print_value(
                &call("export", json!({"workspace_id": ws, "out": out}))?,
                output,
            )
        }
        ("import", _) => {
            let archive = args.get(1).ok_or("falta el archivo")?;
            print_value(&call("import", json!({"archive": archive}))?, output)
        }
        ("module", Some("install")) => {
            let path = args.get(2).ok_or("falta la ruta del .hlimod")?;
            print_value(&call("module.install", json!({"path": path}))?, output)
        }
        ("module", Some("list")) => print_value(&call("module.list", json!({}))?, output),
        ("module", Some("inspect")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("module.inspect", json!({"name": name}))?, output)
        }
        ("module", Some("test")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("module.test", json!({"name": name}))?, output)
        }
        ("module", Some("remove")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("module.remove", json!({"name": name}))?, output)
        }
        ("module", Some("sign")) => {
            let pkg = args.get(2).ok_or("falta la ruta del .hlimod")?;
            let key = need_flag(args, "--key")?;
            let sig = hli_signing::package::sign_package(std::path::Path::new(pkg), &home(), &key)
                .map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&sig).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("module", Some("verify")) => {
            let pkg = args.get(2).ok_or("falta la ruta del .hlimod")?;
            let verification = hli_signing::verify_package(std::path::Path::new(pkg), &home())
                .map_err(signing_err)?;
            let policy = hli_signing::Policy::load(&home());
            let outcome = hli_signing::policy::apply(policy, verification.state);
            let mut v = verification.to_json();
            v["policy"] = json!(policy.as_str());
            v["accepted"] = json!(outcome.accepted);
            v["warning"] = json!(outcome.warning);
            print_value(&v, output)
        }
        ("key", Some("generate")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            let record = hli_signing::keys::generate(&home(), name).map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&record).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("key", Some("list")) => {
            let keys = hli_signing::keys::list(&home()).map_err(signing_err)?;
            let list: Vec<Value> = keys
                .iter()
                .map(|k| serde_json::to_value(k).unwrap_or(Value::Null))
                .collect();
            print_value(&Value::Array(list), output)
        }
        ("key", Some("inspect")) => {
            let id = args.get(2).ok_or("falta key_id")?;
            let record = hli_signing::keys::find(&home(), id).map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&record).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("key", Some("export-public")) => {
            let id = args.get(2).ok_or("falta key_id")?;
            let record = hli_signing::keys::find(&home(), id).map_err(signing_err)?;
            print_value(
                &json!({
                    "key_id": record.key_id,
                    "name": record.name,
                    "public_key": record.public_key,
                    "fingerprint": record.fingerprint,
                    "label": record.name
                }),
                output,
            )
        }
        ("key", Some("revoke")) => {
            let id = args.get(2).ok_or("falta key_id")?;
            let record = hli_signing::keys::revoke(&home(), id).map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&record).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("trust", Some("add")) => {
            let file = args.get(2).ok_or("falta el archivo de clave pública")?;
            let record = hli_signing::trust::add_file(&home(), std::path::Path::new(file))
                .map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&record).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("trust", Some("list")) => {
            let trust = hli_signing::trust::list(&home()).map_err(signing_err)?;
            let list: Vec<Value> = trust
                .iter()
                .map(|k| serde_json::to_value(k).unwrap_or(Value::Null))
                .collect();
            print_value(&Value::Array(list), output)
        }
        ("trust", Some("remove")) => {
            let id = args.get(2).ok_or("falta key_id")?;
            hli_signing::trust::remove(&home(), id).map_err(signing_err)?;
            print_value(&json!({"removed": id}), output)
        }
        ("trust", Some("revoke")) => {
            let id = args.get(2).ok_or("falta key_id")?;
            let record = hli_signing::trust::revoke(&home(), id).map_err(signing_err)?;
            print_value(
                &serde_json::to_value(&record).map_err(|e| e.to_string())?,
                output,
            )
        }
        ("migrate", Some("status")) => {
            let db = home().join("hli.db");
            let st = hli_store::migrations::status_path(&db).map_err(signing_err)?;
            let pending: Vec<Value> = st
                .pending
                .iter()
                .map(|p| json!({"version": p.version, "name": p.name}))
                .collect();
            print_value(
                &json!({
                    "db_exists": st.db_exists,
                    "current_version": st.current_version,
                    "target_version": st.target_version,
                    "pending": pending,
                    "compatible": st.compatible,
                    "backup_required": st.backup_required
                }),
                output,
            )
        }
        ("migrate", Some("backup")) => {
            let db = home().join("hli.db");
            let out = hli_store::migrations::backup_path(&db, &home().join("backups"))
                .map_err(signing_err)?;
            print_value(&json!({"backup": out.to_string_lossy()}), output)
        }
        ("migrate", Some("apply")) => {
            if daemon_is_running() {
                return Err(
                    "el daemon está en marcha; ejecuta hli daemon stop antes de migrar".into(),
                );
            }
            let db = home().join("hli.db");
            let (report, backup) =
                hli_store::migrations::apply_path(&db, &home().join("backups"), true)
                    .map_err(signing_err)?;
            print_value(
                &json!({
                    "from_version": report.from_version,
                    "to_version": report.to_version,
                    "applied": report.applied,
                    "backup": backup.map(|p| p.to_string_lossy().to_string())
                }),
                output,
            )
        }
        ("migrate", Some("verify")) => {
            let db = home().join("hli.db");
            let report = hli_store::migrations::verify_path(&db).map_err(signing_err)?;
            print_value(
                &json!({
                    "current_version": report.current_version,
                    "target_version": report.target_version,
                    "schema_ok": report.schema_ok,
                    "integrity_ok": report.integrity_ok,
                    "chain_ok": report.chain_ok,
                    "events_checked": report.events_checked
                }),
                output,
            )?;
            if report.schema_ok && report.integrity_ok && report.chain_ok {
                Ok(())
            } else {
                Err("verificación de migración fallida".into())
            }
        }
        ("mch", Some("create")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            let v = call(
                "mch.create",
                json!({
                    "name": name,
                    "module": flag(args, "--module").unwrap_or_else(|| "hli-memory".into()),
                    "domain": flag(args, "--domain").unwrap_or_else(|| "memory".into()),
                    "workspace_id": flag(args, "--workspace"),
                    "autostart": args.iter().any(|a| a == "--autostart")
                }),
            )?;
            print_value(&v, output)
        }
        ("mch", Some("set-autostart")) => {
            let name = args.get(2).ok_or("falta instance_id")?;
            let raw = args.get(3).ok_or("falta true|false")?;
            let autostart = match raw.as_str() {
                "true" => true,
                "false" => false,
                other => return Err(format!("valor inválido para autostart: {other}")),
            };
            print_value(
                &call(
                    "mch.set_autostart",
                    json!({"name": name, "autostart": autostart}),
                )?,
                output,
            )
        }
        ("mch", Some("list")) => print_value(&call("mch.list", json!({}))?, output),
        ("mch", Some("inspect")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.inspect", json!({"name": name}))?, output)
        }
        ("mch", Some("serve")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.serve", json!({"name": name}))?, output)
        }
        ("mch", Some("status")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.status", json!({"name": name}))?, output)
        }
        ("mch", Some("restart")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.restart", json!({"name": name}))?, output)
        }
        ("mch", Some("logs")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.logs", json!({"name": name}))?, output)
        }
        ("mch", Some("stop")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.stop", json!({"name": name}))?, output)
        }
        ("mch", Some("remove")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("mch.remove", json!({"name": name}))?, output)
        }
        ("mch", Some("invoke")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            let op = args.get(3).ok_or("falta la operación")?;
            let arguments: Value =
                serde_json::from_str(&flag(args, "--args").unwrap_or_else(|| "{}".into()))
                    .map_err(|e| format!("args inválidos: {e}"))?;
            let v = call(
                "mch.invoke",
                json!({
                    "name": name, "operation": op, "arguments": arguments,
                    "workspace_id": flag(args, "--workspace")
                }),
            )?;
            print_value(&v, output)
        }
        ("memory", Some("commit")) => {
            let ws = need_flag(args, "--workspace")?;
            let kind = need_flag(args, "--type")?;
            let content = need_flag(args, "--content")?;
            let scope = flag(args, "--scope").unwrap_or_else(|| "workspace".into());
            let supersedes: Vec<String> = args
                .iter()
                .enumerate()
                .filter(|(_, a)| *a == "--supersedes")
                .filter_map(|(i, _)| args.get(i + 1).cloned())
                .collect();
            let v = call(
                "memory.commit",
                json!({"workspace_id": ws, "type": kind, "content": content, "scope": scope,
                       "supersedes": supersedes,
                       "authority": flag(args, "--authority"),
                       "subject": flag(args, "--subject"),
                       "relation": flag(args, "--relation"),
                       "object": flag(args, "--object")}),
            )?;
            print_value(&v, output)
        }
        ("memory", Some("search")) => {
            let ws = need_flag(args, "--workspace")?;
            let query = need_flag(args, "--query")?;
            let limit = flag(args, "--limit").and_then(|v| v.parse::<u64>().ok());
            print_value(
                &call(
                    "memory.search",
                    json!({"workspace_id": ws, "query": query, "limit": limit}),
                )?,
                output,
            )
        }
        ("memory", Some("get")) => {
            let id = args.get(2).ok_or("falta memory_id")?;
            print_value(&call("memory.get", json!({"memory_id": id}))?, output)
        }
        ("memory", Some("list")) => {
            let ws = need_flag(args, "--workspace")?;
            print_value(&call("memory.list", json!({"workspace_id": ws}))?, output)
        }
        ("memory", Some("forget")) => {
            let id = args.get(2).ok_or("falta memory_id")?;
            print_value(&call("memory.forget", json!({"memory_id": id}))?, output)
        }
        ("memory", Some("update")) => {
            let id = args.get(2).ok_or("falta memory_id")?;
            let v = call(
                "memory.update",
                json!({"memory_id": id, "content": flag(args, "--content"),
                     "status": flag(args, "--status"), "valid_until": flag(args, "--valid-until")}),
            )?;
            print_value(&v, output)
        }
        ("provider", Some("add")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(
                &call(
                    "provider.add",
                    json!({
                        "name": name,
                        "kind": flag(args, "--kind").ok_or("falta --kind")?,
                        "base_url": flag(args, "--base-url").ok_or("falta --base-url")?,
                        "model": flag(args, "--model").ok_or("falta --model")?,
                        "timeout_seconds": flag(args, "--timeout").and_then(|v| v.parse::<u64>().ok()),
                        "secret_handle": flag(args, "--secret-handle"),
                    }),
                )?,
                output,
            )
        }
        ("provider", Some("list")) => print_value(&call("provider.list", json!({}))?, output),
        ("provider", Some("inspect")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("provider.inspect", json!({"name": name}))?, output)
        }
        ("provider", Some("test")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("provider.test", json!({"name": name}))?, output)
        }
        ("provider", Some("set-default")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(
                &call("provider.set_default", json!({"name": name}))?,
                output,
            )
        }
        ("provider", Some("remove")) => {
            let name = args.get(2).ok_or("falta el nombre")?;
            print_value(&call("provider.remove", json!({"name": name}))?, output)
        }
        ("run", Some("start")) => {
            let ws = need_flag(args, "--workspace")?;
            let th = need_flag(args, "--thread")?;
            let br = need_flag(args, "--branch")?;
            let prompt = need_flag(args, "--prompt")?;
            let v = call(
                "run.start",
                json!({
                    "workspace_id": ws, "thread_id": th, "branch_id": br, "prompt": prompt,
                    "provider": flag(args, "--provider"),
                    "model": flag(args, "--model"),
                    "base_url": flag(args, "--base-url"),
                    "secret": flag(args, "--secret"),
                    "artifact_id": flag(args, "--artifact"),
                    "no_memories": args.contains(&"--no-memories".to_string()),
                    "no_events": args.contains(&"--no-events".to_string()),
                    "no_slices": args.contains(&"--no-slices".to_string()),
                    "no_policy": args.contains(&"--no-policy".to_string()),
                    "top_k": flag(args, "--top-k").and_then(|v| v.parse::<u64>().ok()),
                    "temperature": flag(args, "--temperature").and_then(|v| v.parse::<f64>().ok()),
                    "max_tokens": flag(args, "--max-tokens").and_then(|v| v.parse::<i64>().ok()),
                    "async": args.contains(&"--async".to_string()),
                    "sim_delay_ms": flag(args, "--sim-delay-ms").and_then(|v| v.parse::<u64>().ok()),
                    "timeout_secs": flag(args, "--timeout-secs").and_then(|v| v.parse::<u64>().ok())
                }),
            )?;
            print_value(&v, output)
        }
        ("run", Some("status")) => {
            let id = args.get(2).ok_or("falta el run_id")?;
            print_value(&call("run.status", json!({"run_id": id}))?, output)
        }
        ("run", Some("wait")) => {
            let id = args.get(2).ok_or("falta el run_id")?;
            let t = flag(args, "--timeout-secs").and_then(|v| v.parse::<u64>().ok());
            print_value(
                &call("run.wait", json!({"run_id": id, "timeout_secs": t}))?,
                output,
            )
        }
        ("run", Some("cancel")) => {
            let id = args.get(2).ok_or("falta el run_id")?;
            print_value(&call("run.cancel", json!({"run_id": id}))?, output)
        }
        ("run", Some("get")) => {
            let id = args.get(2).ok_or("falta run_id")?;
            print_value(&call("run.get", json!({"run_id": id}))?, output)
        }
        ("run", Some("list")) => {
            let state = flag(args, "--state");
            let ws = need_flag(args, "--workspace")?;
            print_value(
                &call("run.list", json!({"workspace_id": ws, "state": state}))?,
                output,
            )
        }
        ("run", Some("approve")) => {
            let id = args.get(2).ok_or("falta run_id")?;
            print_value(&call("run.approve", json!({"run_id": id}))?, output)
        }
        ("prices", Some("list")) => print_value(&call("prices.list", json!({}))?, output),
        ("manifest", Some("get")) => {
            let id = args.get(2).ok_or("falta manifest_id")?;
            print_value(&call("manifest.get", json!({"manifest_id": id}))?, output)
        }
        ("status", _) => {
            let v = call("status", json!({}))?;
            print_value(&v, output)
        }
        _ => usage(),
    }
}

fn output_mode(args: &[String]) -> &'static str {
    match flag(args, "--output").as_deref() {
        Some("human") => "human",
        Some("jsonl") => "jsonl",
        _ => "json",
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn need_flag(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("falta {name}"))
}

fn signing_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn daemon_is_running() -> bool {
    let Ok(raw) = std::fs::read_to_string(daemon_file()) else {
        return false;
    };
    let Ok(info) = serde_json::from_str::<Value>(&raw) else {
        return false;
    };
    let Some(port) = info["port"].as_u64() else {
        return false;
    };
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port as u16));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
}

fn print_value(v: &Value, mode: &str) -> Result<(), String> {
    if mode == "human" {
        if let Some(arr) = v.as_array() {
            for item in arr {
                println!("{}", human_line(item));
            }
            return Ok(());
        }
        println!("{}", human_line(v));
        return Ok(());
    }
    if mode == "jsonl" {
        println!("{}", serde_json::to_string(v).map_err(|e| e.to_string())?);
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn human_line(v: &Value) -> String {
    if let Some(inst) = v.get("instance")
        && inst.is_object()
        && inst.get("name").is_some()
    {
        let name = inst["name"].as_str().unwrap_or("-");
        let state = inst["state"].as_str().unwrap_or("-");
        let proc = &v["process"];
        let pid = match proc["pid"].as_i64() {
            Some(p) => p.to_string(),
            None => "-".into(),
        };
        let generation = match proc["generation"].as_u64() {
            Some(g) => g.to_string(),
            None => "-".into(),
        };
        let restarts = proc["restart_count_in_window"].as_u64().unwrap_or(0);
        let next = proc["next_restart_at"].as_str().unwrap_or("-");
        let code = proc["last_error"]["code"].as_str().unwrap_or("-");
        return format!(
            "{name}  state={state} pid={pid} generation={generation} restarts_window={restarts} next_restart_at={next} last_error={code}"
        );
    }
    if let Some(b) = v.get("branch") {
        let id = b.get("id").and_then(|x| x.as_str()).unwrap_or("-");
        let seq = b.get("head_sequence").and_then(|x| x.as_i64()).unwrap_or(0);
        let evs = v.get("event_count").and_then(|x| x.as_i64()).unwrap_or(0);
        let head = v.get("chain_head").and_then(|x| x.as_str()).unwrap_or("-");
        return format!("{id}  head_sequence={seq} events={evs} chain_head={head}");
    }
    if v.get("ok").is_some() {
        let ok = v["ok"].as_bool().unwrap_or(false);
        let checked = v["checked"].as_i64().unwrap_or(0);
        let head = v["chain_head"].as_str().unwrap_or("-");
        return format!("ok={ok} checked={checked} chain_head={head}");
    }
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("-");
    let name = v
        .get("name")
        .or_else(|| v.get("title"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if name.is_empty() {
        id.to_string()
    } else {
        format!("{id}  {name}")
    }
}

#[cfg(windows)]
fn suppress_std_handle_inheritance() -> Vec<isize> {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut c_void;
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    let mut saved = Vec::new();
    for kind in [(-10i32) as u32, (-11i32) as u32, (-12i32) as u32] {
        let handle = unsafe { GetStdHandle(kind) };
        if handle.is_null() || handle as isize == -1 {
            continue;
        }
        unsafe {
            SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
        }
        saved.push(handle as isize);
    }
    saved
}

#[cfg(windows)]
fn restore_std_handle_inheritance(handles: &[isize]) {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    for handle in handles {
        unsafe {
            SetHandleInformation(
                *handle as *mut c_void,
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            );
        }
    }
}

fn daemon_start() -> Result<(), String> {
    if call("daemon.ping", json!({})).is_ok() {
        println!("{}", json!({"daemon": "already_running"}));
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let daemon = exe.with_file_name(if cfg!(windows) {
        "hli-daemon.exe"
    } else {
        "hli-daemon"
    });
    if !daemon.exists() {
        return Err(format!(
            "no se encontró el binario del daemon en {}",
            daemon.display()
        ));
    }
    std::fs::create_dir_all(home()).map_err(|e| format!("no se pudo crear HLI_HOME: {e}"))?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home().join("daemon.log"))
        .map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let saved_handles = suppress_std_handle_inheritance();
    let spawned = Command::new(&daemon)
        .env("HLI_HOME", home())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_err)
        .spawn();
    #[cfg(windows)]
    restore_std_handle_inheritance(&saved_handles);
    spawned.map_err(|e| format!("no se pudo arrancar el daemon: {e}"))?;

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(80));
        if call("daemon.ping", json!({})).is_ok() {
            let info = daemon_info()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"daemon": "started", "info": info})).unwrap()
            );
            return Ok(());
        }
    }
    Err("el daemon no respondió tras 8 s; revisa daemon.log".into())
}

fn daemon_stop() -> Result<(), String> {
    match call("daemon.stop", json!({})) {
        Ok(_) => {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(60));
                if call("daemon.ping", json!({})).is_err() {
                    break;
                }
            }
            let _ = std::fs::remove_file(daemon_file());
            println!("{}", json!({"daemon": "stopped"}));
            Ok(())
        }
        Err(_) => {
            println!("{}", json!({"daemon": "not_running"}));
            Ok(())
        }
    }
}

fn daemon_status() -> Result<(), String> {
    match call("daemon.ping", json!({})) {
        Ok(v) => {
            let info = daemon_info().unwrap_or(json!({}));
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"running": true, "ping": v, "info": info}))
                    .unwrap()
            );
            Ok(())
        }
        Err(_) => {
            println!("{}", json!({"running": false}));
            Ok(())
        }
    }
}

fn usage() -> Result<(), String> {
    eprintln!(
        "hli {}
  hli daemon start|stop|status
  hli workspace create <name> [--root PATH] | list | get <id>
  hli thread create <title> --workspace <id> | list --workspace <id>
  hli branch create <name> --thread <id> [--from EVENT] | list --thread <id> | show <id> | snapshot <id>
  hli event append --type T --payload JSON --workspace <id> [--thread ID] [--branch ID] [--idem K]
  hli event list --workspace <id> [--branch ID]
  hli artifact put <file> --workspace <id> | list --workspace <id>
  hli artifact slice <artifact_id> --workspace <id> [--kind K] [--start N] [--end M] [--reason R]
  hli run start --workspace <id> --thread <id> --branch <id> --prompt PROMPT [--provider sim|ollama|openai] [--model M] [--base-url U] [--secret HANDLE] 
  hli run get <run_id> | list --workspace <id> | approve <run_id>
  hli manifest get <manifest_id>
  hli prices list
  hli module install <file.hlimod> | list | inspect <name> | test <name> | remove <name>
  hli module sign <file.hlimod> --key <key_id> | verify <file.hlimod>
  hli key generate <name> | list | inspect <key_id> | export-public <key_id> | revoke <key_id>
  hli trust add <public-key-file> | list | remove <key_id> | revoke <key_id>
  hli migrate status | backup | apply | verify
  hli mch create <name> [--module M] [--domain D] [--workspace W] [--autostart] | list | inspect <name>
  hli mch set-autostart <name> true|false
  hli mch serve <name> | stop <name> | remove <name>
  hli mch invoke <name> <operation> --args JSON [--workspace W]
  hli memory commit --workspace <id> --type TYPE --content TEXT [--scope S]
  hli memory search --workspace <id> --query TEXT | get <id> | list --workspace <id> | forget <id>
  hli verify --workspace <id>
  hli export <workspace_id> --out <file>
  hli import <file>
  hli status
Todas las salidas aceptan --output json|human",
        env!("CARGO_PKG_VERSION")
    );
    Err("uso".into())
}
