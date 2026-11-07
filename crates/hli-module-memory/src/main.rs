//! Módulo `hli-memory` de HLI 0.4.
//!
//! Habla el Module Protocol (JSON-RPC 2.0 delimitado por línea sobre stdio) y
//! delega toda la persistencia en el daemon de HLI por TCP. El módulo no abre
//! `hli.db`, no lee secretos y no expone MCP por sí mismo.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use serde_json::{json, Value};

const MODULE: &str = "hli-memory";
const VERSION: &str = "0.4.0";

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in BufReader::new(stdin.lock()).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(out, "{}", json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}));
                let _ = out.flush();
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));

        if method == "shutdown" {
            let _ = writeln!(out, "{}", json!({"jsonrpc":"2.0","id":id,"result":{"ok":true}}));
            let _ = out.flush();
            break;
        }

        let result = handle(method, &params);
        let resp = match result {
            Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
            Err((code, message)) => json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
        };
        if writeln!(out, "{}", resp).is_err() {
            break;
        }
        let _ = out.flush();
    }
}

fn handle(method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "module": MODULE,
            "version": VERSION,
            "sdk": ">=0.4 <0.5",
            "operations": operations()
        })),
        "capabilities" => Ok(json!({"operations": operations()})),
        "configure" => Ok(json!({"ok": true})),
        "health" => Ok(json!({"ok": true, "module": MODULE, "version": VERSION})),
        "cancel" => Ok(json!({"ok": true})),
        "checkpoint" => Ok(json!({"ok": true, "note": "sin estado propio"})),
        "invoke" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let meta = params.get("_meta").cloned().unwrap_or(json!({}));
            let workspace = meta
                .get("hli.dev/workspaceId")
                .or_else(|| meta.get("workspace_id"))
                .and_then(|v| v.as_str())
                .ok_or((-32602, "falta workspace_id en _meta".to_string()))?;
            invoke(name, workspace, &args)
        }
        other => Err((-32601, format!("método no soportado: {other}"))),
    }
}

fn operations() -> Value {
    json!([
        {"name": "memory.commit", "effects": "write"},
        {"name": "memory.get", "effects": "read"},
        {"name": "memory.search", "effects": "read"},
        {"name": "memory.update", "effects": "write"},
        {"name": "memory.forget", "effects": "write"},
        {"name": "context.build", "effects": "read"}
    ])
}

fn invoke(name: &str, workspace: &str, args: &Value) -> Result<Value, (i64, String)> {
    match name {
        "memory.commit" => daemon(
            "memory.commit",
            json!({
                "workspace_id": workspace,
                "type": args.get("type").cloned().unwrap_or(json!("fact")),
                "content": args.get("content").cloned().unwrap_or(json!("")),
                "scope": args.get("scope").cloned().unwrap_or(json!("workspace"))
            }),
        ),
        "memory.get" => daemon("memory.get", json!({"memory_id": args["memory_id"]})),
        "memory.search" => daemon(
            "memory.search",
            json!({
                "workspace_id": workspace,
                "query": args.get("query").cloned().unwrap_or(json!("")),
                "limit": args.get("limit").cloned().unwrap_or(json!(8))
            }),
        ),
        "memory.update" => daemon(
            "memory.update",
            json!({
                "memory_id": args["memory_id"],
                "content": args.get("content").cloned(),
                "status": args.get("status").cloned()
            }),
        ),
        "memory.forget" => daemon("memory.forget", json!({"memory_id": args["memory_id"]})),
        "context.build" => {
            let memories = daemon(
                "memory.search",
                json!({
                    "workspace_id": workspace,
                    "query": args.get("query").cloned().unwrap_or(json!("")),
                    "limit": args.get("limit").cloned().unwrap_or(json!(8))
                }),
            )?;
            let count = memories.as_array().map(|a| a.len()).unwrap_or(0);
            Ok(json!({"memories": memories, "count": count}))
        }
        other => Err((-32602, format!("operación no declarada: {other}"))),
    }
}

fn daemon(cmd: &str, args: Value) -> Result<Value, (i64, String)> {
    let home = hli_home();
    let info = std::fs::read_to_string(home.join("daemon.json"))
        .map_err(|_| (-32000, "daemon no está en marcha".to_string()))?;
    let v: Value = serde_json::from_str(&info).map_err(|e| (-32000, e.to_string()))?;
    let port = v["port"].as_u64().ok_or((-32000, "sin puerto".to_string()))? as u16;
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| (-32000, e.to_string()))?;
    let req = json!({"id": "module", "cmd": cmd, "args": args});
    writeln!(s, "{}", req).map_err(|e| (-32000, e.to_string()))?;
    s.flush().map_err(|e| (-32000, e.to_string()))?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).map_err(|e| (-32000, e.to_string()))?;
    let resp: Value = serde_json::from_str(&line).map_err(|e| (-32000, e.to_string()))?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        Err((-32000, resp["error"]["message"].as_str().unwrap_or("error").to_string()))
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
