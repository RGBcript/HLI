//! Servidor MCP de HLI 0.3 sobre stdio (JSON-RPC 2.0 con framing Content-Length).
//!
//! Baseline: 2025-11-25 (`initialize`) y `server/discover` para clientes modernos.
//! No mantiene estado de protocolo: cada tool call lleva sus identificadores.

use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

fn main() {
    let home = hli_home();
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout();

    loop {
        let msg = match read_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => break,
            Err(_) => break,
        };
        let req: Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));

        // Notificaciones: sin respuesta.
        if id.is_none() {
            continue;
        }

        let result = handle(method, &params, &home);
        let resp = match result {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err((code, message)) => json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": code, "message": message}
            }),
        };
        if write_message(&mut stdout, &resp.to_string()).is_err() {
            break;
        }
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

fn read_message(r: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let t = line.trim_end_matches(['\r', '\n']);
        if t.is_empty() {
            break;
        }
        if let Some(v) = t.strip_prefix("Content-Length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    if len == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(String::from_utf8_lossy(&buf).to_string()))
}

fn write_message(w: &mut impl Write, body: &str) -> std::io::Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n{}", body.as_bytes().len(), body)?;
    w.flush()
}

fn handle(method: &str, params: &Value, home: &PathBuf) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "hli", "version": "0.3.0"}
        })),
        "server/discover" => Ok(json!({
            "protocolVersions": ["2025-11-25", "2026-07-28"],
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "hli", "version": "0.3.0"}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let out = call_tool(name, &args, home)?;
            Ok(json!({
                "content": [{"type": "text", "text": out.to_string()}],
                "isError": false
            }))
        }
        "resources/list" => Ok(json!({"resources": [
            {"uri": "hli://workspace/{id}", "name": "workspace", "mimeType": "application/json"},
            {"uri": "hli://thread/{id}", "name": "thread", "mimeType": "application/json"},
            {"uri": "hli://memory/{id}", "name": "memory", "mimeType": "application/json"},
            {"uri": "hli://artifact/{id}", "name": "artifact", "mimeType": "application/json"},
            {"uri": "hli://run/{id}", "name": "run", "mimeType": "application/json"}
        ]})),
        "resources/read" => {
            let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            let out = read_resource(uri, home)?;
            Ok(json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": out.to_string()}]}))
        }
        "prompts/list" => Ok(json!({"prompts": [
            {"name": "hli.plan", "description": "Planificar sobre un workspace HLI"},
            {"name": "hli.review", "description": "Revisar decisiones y memorias de HLI"}
        ]})),
        other => Err((-32601, format!("método no soportado: {other}"))),
    }
}

fn tools() -> Value {
    let t = |name: &str, desc: &str, props: Value, required: Value| {
        json!({
            "name": name,
            "description": desc,
            "inputSchema": {"type": "object", "properties": props, "required": required}
        })
    };
    json!([
        t("workspace_list", "Lista los workspaces", json!({}), json!([])),
        t("thread_read", "Lista los threads de un workspace",
          json!({"workspace_id": {"type": "string"}}), json!(["workspace_id"])),
        t("memory_search", "Busca memorias activas",
          json!({"workspace_id": {"type": "string"}, "query": {"type": "string"}}),
          json!(["workspace_id", "query"])),
        t("memory_commit", "Registra una memoria",
          json!({"workspace_id": {"type": "string"}, "type": {"type": "string"},
                 "content": {"type": "string"}, "scope": {"type": "string"}}),
          json!(["workspace_id", "type", "content"])),
        t("run_start", "Inicia un run en una rama",
          json!({"workspace_id": {"type": "string"}, "thread_id": {"type": "string"},
                 "branch_id": {"type": "string"}, "prompt": {"type": "string"}}),
          json!(["workspace_id", "thread_id", "branch_id", "prompt"])),
        t("run_get", "Obtiene un run y su manifiesto",
          json!({"run_id": {"type": "string"}}), json!(["run_id"])),
        t("artifact_read", "Lee un artefacto de texto",
          json!({"artifact_id": {"type": "string"}}), json!(["artifact_id"]))
    ])
}

fn call_tool(name: &str, args: &Value, home: &PathBuf) -> Result<Value, (i64, String)> {
    match name {
        "workspace_list" => daemon(home, "workspace.list", json!({})),
        "thread_read" => daemon(home, "thread.list", json!({"workspace_id": args["workspace_id"]})),
        "memory_search" => daemon(
            home,
            "memory.search",
            json!({"workspace_id": args["workspace_id"], "query": args["query"]}),
        ),
        "memory_commit" => daemon(
            home,
            "memory.commit",
            json!({
                "workspace_id": args["workspace_id"],
                "type": args["type"],
                "content": args["content"],
                "scope": args.get("scope").cloned().unwrap_or(json!("workspace"))
            }),
        ),
        "run_start" => daemon(
            home,
            "run.start",
            json!({
                "workspace_id": args["workspace_id"],
                "thread_id": args["thread_id"],
                "branch_id": args["branch_id"],
                "prompt": args["prompt"]
            }),
        ),
        "run_get" => daemon(home, "run.get", json!({"run_id": args["run_id"]})),
        "artifact_read" => daemon(home, "artifact.read", json!({"artifact_id": args["artifact_id"]})),
        other => Err((-32602, format!("tool desconocida: {other}"))),
    }
}

fn read_resource(uri: &str, home: &PathBuf) -> Result<Value, (i64, String)> {
    let rest = uri.strip_prefix("hli://").ok_or((-32602, "uri inválida".to_string()))?;
    let (kind, id) = rest.split_once('/').ok_or((-32602, "uri inválida".to_string()))?;
    match kind {
        "workspace" => daemon(home, "workspace.get", json!({"workspace_id": id})),
        "thread" => daemon(home, "thread.get", json!({"thread_id": id})),
        "memory" => daemon(home, "memory.get", json!({"memory_id": id})),
        "artifact" => daemon(home, "artifact.read", json!({"artifact_id": id})),
        "run" => daemon(home, "run.get", json!({"run_id": id})),
        _ => Err((-32602, format!("recurso desconocido: {kind}"))),
    }
}

fn daemon(home: &PathBuf, cmd: &str, args: Value) -> Result<Value, (i64, String)> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    let info = std::fs::read_to_string(home.join("daemon.json"))
        .map_err(|_| (-32000, "daemon no está en marcha".to_string()))?;
    let v: Value = serde_json::from_str(&info).map_err(|e| (-32000, e.to_string()))?;
    let port = v["port"].as_u64().ok_or((-32000, "daemon.json sin puerto".to_string()))? as u16;
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| (-32000, e.to_string()))?;
    let req = json!({"id": "mcp", "cmd": cmd, "args": args});
    writeln!(s, "{}", req).map_err(|e| (-32000, e.to_string()))?;
    s.flush().map_err(|e| (-32000, e.to_string()))?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).map_err(|e| (-32000, e.to_string()))?;
    let resp: Value = serde_json::from_str(&line).map_err(|e| (-32000, e.to_string()))?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        Err((
            -32000,
            resp["error"]["message"].as_str().unwrap_or("error del daemon").to_string(),
        ))
    }
}
