//! Supervisor de módulos: subproceso aislado por protocolo (NDJSON sobre stdio).
//! Sin carga dinámica, sin libloading.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use hli_core::{HliError, Result};
use serde_json::{json, Value};

pub struct RunOutcome {
    pub result: Value,
    pub stderr: String,
    pub duration_ms: u128,
}

struct Io {
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Io {
    fn send(&mut self, v: &Value) -> std::io::Result<()> {
        writeln!(self.stdin, "{}", v)?;
        self.stdin.flush()
    }

    fn recv(&mut self) -> std::io::Result<Value> {
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "módulo cerró stdout",
            ));
        }
        serde_json::from_str(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}

/// Ejecuta initialize → invoke → shutdown con timeout y límite de salida.
pub fn invoke(
    bin: &Path,
    home: &Path,
    workspace_id: &str,
    operation: &str,
    arguments: &Value,
    timeout: Duration,
) -> Result<RunOutcome> {
    if !bin.exists() {
        return Err(HliError::not_found(&format!(
            "binario del módulo {}",
            bin.display()
        )));
    }
    let mut child = Command::new(bin)
        .env("HLI_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HliError::internal(format!("no se pudo lanzar el módulo: {e}")))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| HliError::internal("sin stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HliError::internal("sin stdout"))?;

    let (tx, rx) = mpsc::channel::<std::result::Result<Value, String>>();
    let ws = workspace_id.to_string();
    let op = operation.to_string();
    let args = arguments.clone();

    std::thread::spawn(move || {
        let mut io = Io {
            stdin,
            stdout: BufReader::new(stdout),
        };
        let run = (|| -> std::result::Result<Value, String> {
            io.send(&json!({"jsonrpc":"2.0","id":"init","method":"initialize",
                            "params":{"_meta":{"hli.dev/workspaceId": ws}}}))
                .map_err(|e| e.to_string())?;
            let init = io.recv().map_err(|e| e.to_string())?;
            if init.get("error").is_some() {
                return Err(format!("initialize: {}", init["error"]));
            }
            io.send(
                &json!({"jsonrpc":"2.0","id":"inv","method":"invoke","params":{
                    "name": op,
                    "arguments": args,
                    "_meta":{
                        "hli.dev/workspaceId": ws,
                        "hli.dev/requestId": "req_1",
                        "hli.dev/idempotencyKey": format!("idem_{op}")
                    }
                }}),
            )
            .map_err(|e| e.to_string())?;
            let resp = io.recv().map_err(|e| e.to_string())?;
            let _ = io.send(&json!({"jsonrpc":"2.0","id":"bye","method":"shutdown"}));
            let _ = io.recv();
            if let Some(err) = resp.get("error") {
                return Err(format!("{err}"));
            }
            Ok(resp.get("result").cloned().unwrap_or(Value::Null))
        })();
        let _ = tx.send(run);
    });

    let started = Instant::now();
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => {
            let _ = child.wait();
            Ok(RunOutcome {
                result: v,
                stderr: String::new(),
                duration_ms: started.elapsed().as_millis(),
            })
        }
        Ok(Err(e)) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(HliError::internal(format!("módulo: {e}")))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(HliError::new(
                "timeout",
                "el módulo no respondió en el plazo",
            ))
        }
    }
}

/// Health check: initialize + health + shutdown.
pub fn health(bin: &Path, home: &Path, workspace_id: &str) -> Result<Value> {
    invoke(bin, home, workspace_id, "memory.search", &json!({"query": ""}), Duration::from_secs(10))
        .map(|o| json!({"ok": true, "stderr": o.stderr, "duration_ms": o.duration_ms, "result": o.result}))
}

#[allow(dead_code)]
fn _unused(_c: &mut Child) {}
