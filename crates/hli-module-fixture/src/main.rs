//! Módulo MCH de prueba. Configuración por `HLI_HOME/fixture.json`, no por
//! variables globales: cada test usa su propio HLI_HOME y no comparte estado.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

fn config() -> Value {
    let home = std::env::var("HLI_HOME").unwrap_or_default();
    std::fs::read_to_string(format!("{home}/fixture.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

fn send(id: &Value, value: Value) {
    println!("{}", json!({"jsonrpc": "2.0", "id": id, "result": value}));
}

fn send_err(id: &Value, code: i64, message: &str) {
    println!(
        "{}",
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
    );
}

fn main() {
    let stdin = std::io::stdin();
    let mut health_count: u64 = 0;
    let mut invoke_count: u64 = 0;
    let mut configured_mode = "normal".to_string();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let cfg = config();
        let mode = cfg
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("normal")
            .to_string();

        match method {
            "initialize" => {
                if mode == "initialize_failure" {
                    send_err(&id, -32000, "initialize failure");
                } else {
                    send(
                        &id,
                        json!({
                            "module": "hli-module-fixture",
                            "version": "0.1.0",
                            "sdk": ">=0.4 <0.5",
                            "pid": std::process::id()
                        }),
                    );
                }
            }
            "capabilities" => send(
                &id,
                json!({"operations": [
                    {"name": "memory.search", "effects": "read"},
                    {"name": "memory.commit", "effects": "write"}
                ]}),
            ),
            "configure" => {
                configured_mode = mode.clone();
                if mode == "configure_failure" {
                    send_err(&id, -32000, "configure failure");
                } else {
                    send(&id, json!({"ok": true, "mode": configured_mode}));
                }
            }
            "health" => {
                health_count += 1;
                if mode == "health_timeout" {
                    let ms = cfg
                        .get("health_delay_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(5000);
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                let fail_n = cfg
                    .get("health_failures")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let recover = cfg
                    .get("recover_after_n")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(u64::MAX);
                let fail = (mode == "health_fail_n" && health_count <= fail_n)
                    || (mode == "recover_after_n" && health_count <= recover);
                if fail {
                    send_err(&id, -32000, "health failure");
                } else {
                    send(
                        &id,
                        json!({"ok": true, "health_count": health_count, "pid": std::process::id()}),
                    );
                }
            }
            "invoke" => {
                invoke_count += 1;
                if let Some(n) = cfg.get("crash_after_invokes").and_then(|v| v.as_u64()) {
                    if invoke_count >= n {
                        std::process::exit(1);
                    }
                }
                if mode == "invalid_json"
                    || cfg.get("emit_invalid_json_on").and_then(|v| v.as_str()) == Some("invoke")
                {
                    println!("not-json");
                    let _ = std::io::stdout().flush();
                    continue;
                }
                let out = json!({
                    "pid": std::process::id(),
                    "health_count": health_count,
                    "invoke_count": invoke_count,
                    "mode": configured_mode
                });
                if cfg.get("emit_unknown_id").and_then(|v| v.as_bool()) == Some(true) {
                    println!(
                        "{}",
                        json!({"jsonrpc": "2.0", "id": "unknown", "result": out})
                    );
                } else {
                    send(&id, out);
                }
            }
            "shutdown" => {
                if cfg.get("ignore_shutdown").and_then(|v| v.as_bool()) == Some(true) {
                    continue;
                }
                send(&id, json!({"ok": true}));
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            }
            "cancel" | "checkpoint" => send(&id, json!({"ok": true})),
            other => send_err(&id, -32601, &format!("metodo no soportado: {other}")),
        }
        let _ = std::io::stdout().flush();
    }
}
