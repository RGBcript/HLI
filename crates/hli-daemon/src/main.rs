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
                std::thread::spawn(move || {
                    let _ = handle(s, w, r);
                });
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

fn handle(stream: TcpStream, writer: Writer, reader: Arc<Reader>) -> std::io::Result<()> {
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

        match dispatch(cmd, &args, &writer, &reader) {
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

fn s(args: &Value, key: &str) -> Result<String, HliError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| HliError::invalid(format!("falta el argumento '{key}'")))
}

fn o(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|v| v.to_string())
}

fn dispatch(cmd: &str, args: &Value, w: &Writer, r: &Reader) -> Result<Value, HliError> {
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
            let b = w.create_branch(&s(args, "thread_id")?, &s(args, "name")?, o(args, "from_event"))?;
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
        "status" => {
            let ws = r.workspaces()?;
            Ok(json!({"workspaces": ws.len(), "version": "0.1.0"}))
        }
        other => Err(HliError::new("unknown_command", format!("comando desconocido: {other}"))),
    }
}
