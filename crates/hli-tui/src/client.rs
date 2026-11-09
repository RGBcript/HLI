use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use serde_json::{Value, json};

pub fn home() -> PathBuf {
    if let Ok(h) = std::env::var("HLI_HOME") {
        return PathBuf::from(h);
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".hli")
}

/// Llamada normal: operaciones de una tarea pueden tardar.
pub fn call(cmd: &str, args: Value) -> Result<Value, String> {
    call_with_timeout(cmd, args, std::time::Duration::from_secs(10))
}

/// Llamada desde el hilo de la UI: nunca debe congelar la pantalla. Cuatro
/// segundos es el techo; si el daemon no responde, la vista sigue viva.
pub fn call_ui(cmd: &str, args: Value) -> Result<Value, String> {
    call_with_timeout(cmd, args, std::time::Duration::from_secs(4))
}

pub fn call_with_timeout(
    cmd: &str,
    args: Value,
    timeout: std::time::Duration,
) -> Result<Value, String> {
    let info = std::fs::read_to_string(home().join("daemon.json"))
        .map_err(|_| "daemon no está en marcha".to_string())?;
    let v: Value = serde_json::from_str(&info).map_err(|e| e.to_string())?;
    let port = v["port"].as_u64().ok_or("daemon.json sin puerto")? as u16;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500))
        .map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let req = json!({"id": "tui", "cmd": cmd, "args": args});
    writeln!(s, "{}", req).map_err(|e| e.to_string())?;
    s.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(s)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let resp: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        Err(resp["error"]["message"]
            .as_str()
            .unwrap_or("error")
            .to_string())
    }
}

pub fn program() -> PathBuf {
    std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name(if cfg!(windows) { "hli.exe" } else { "hli" }))
        .unwrap_or_else(|| PathBuf::from("hli"))
}
