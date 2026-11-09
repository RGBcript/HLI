//! Renderer clásico: conversación en el scrollback nativo, sin alternate
//! screen ni captura de ratón.
//!
//! Se activa con `--classic` o `HLI_DISABLE_ALTERNATE_SCREEN=1`. Reutiliza el
//! mismo daemon y las mismas operaciones que Agent Mode: no hay un segundo
//! motor de negocio, sólo otra superficie de presentación.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::{App, event_line, terminal_summary};
use crate::client;

fn out(line: impl AsRef<str>) {
    println!("{}", line.as_ref());
    let _ = io::stdout().flush();
}

fn ask(prompt: &str) -> String {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    line.trim().to_string()
}

pub fn run(root: Option<PathBuf>) -> io::Result<()> {
    let mut app = App::new();
    app.begin_agent(root);
    if app.agent.workspace.is_none() {
        out("Sin workspace vinculado a este directorio. Creando uno…");
        app.accept_offer();
        if app.agent.workspace.is_none() {
            out("No se pudo crear el workspace; ¿está el daemon en marcha?");
            return Ok(());
        }
    }
    out("HLI · modo clásico (scrollback nativo, Ctrl+C para salir)");
    out("Escribe tu tarea. Línea vacía sale.\n");

    loop {
        let line = ask("> ");
        if line.is_empty() {
            break;
        }
        if line == "/exit" || line == "/quit" {
            break;
        }
        execute(&mut app, &line);
    }
    Ok(())
}

fn ids(app: &App) -> Option<(String, String, String)> {
    Some((
        app.agent
            .workspace
            .as_ref()?
            .get("id")?
            .as_str()?
            .to_string(),
        app.agent.thread.as_ref()?.get("id")?.as_str()?.to_string(),
        app.agent.branch.as_ref()?.get("id")?.as_str()?.to_string(),
    ))
}

fn execute(app: &mut App, prompt: &str) {
    let Some((ws, th, br)) = ids(app) else {
        out("Sin workspace, thread o branch activos.");
        return;
    };
    let mut args = json!({
        "workspace_id": ws, "thread_id": th, "branch_id": br,
        "prompt": prompt, "async": true
    });
    if app.agent.provider != "sim" {
        args["provider"] = json!(app.agent.provider);
        args["model"] = json!(app.agent.model);
    }
    let run = match client::call("run.start", args) {
        Ok(v) => v,
        Err(e) => {
            out(format!("error: {e}"));
            return;
        }
    };
    let id = run["id"].as_str().unwrap_or("-").to_string();
    let mut last_seq = 0i64;
    let mut last_state = String::new();
    let started = Instant::now();

    loop {
        // Eventos primero: el orden causal nunca se invierte.
        if let Ok(list) = client::call("event.list", json!({"workspace_id": ws}))
            && let Some(arr) = list.as_array()
        {
            let mut evs: Vec<&Value> = arr
                .iter()
                .filter(|e| e["run_id"].as_str() == Some(id.as_str()))
                .collect();
            evs.sort_by_key(|e| e["sequence"].as_i64().unwrap_or(0));
            for e in evs {
                let seq = e["sequence"].as_i64().unwrap_or(0);
                if seq <= last_seq {
                    continue;
                }
                last_seq = seq;
                if let Some(line) = event_line(e) {
                    out(format!("  · {line}"));
                }
            }
        }

        let state = client::call("run.status", json!({"run_id": id}))
            .ok()
            .map(|v| v["run"].clone())
            .unwrap_or(Value::Null);
        let name = state["state"].as_str().unwrap_or("").to_string();
        if name != last_state {
            last_state = name.clone();
        }

        match name.as_str() {
            "waiting_approval" => match state["approval_kind"].as_str() {
                Some("plan") => handle_plan(&id),
                Some("tool") => handle_permission(&id),
                _ => {}
            },
            "completed" | "failed" | "cancelled" | "timed_out" | "interrupted" => {
                out("");
                out(terminal_summary(&state));
                break;
            }
            _ => {}
        }

        if started.elapsed() > Duration::from_secs(600) {
            out("timeout del cliente; el run sigue en el daemon");
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn handle_plan(run_id: &str) {
    let Ok(plan) = client::call("run.plan", json!({"run_id": run_id})) else {
        return;
    };
    let p = &plan["plan"];
    out("");
    out(format!(
        "Plan {} · revisión {} · {}",
        p["plan_id"].as_str().unwrap_or("-"),
        p["revision"].as_i64().unwrap_or(1),
        p["origin"].as_str().unwrap_or("-")
    ));
    if let Some(steps) = p["steps"].as_array() {
        for s in steps {
            out(format!(
                "  {}. {}{}",
                s["order"].as_i64().unwrap_or(0),
                s["description"].as_str().unwrap_or(""),
                if s["requires_approval"].as_bool() == Some(true) {
                    " · approval required"
                } else {
                    ""
                }
            ));
        }
    }
    let answer =
        ask("[y] once  [a] run  [w] workspace  [n] reject  [r] replan  [Enter] esperar > ");
    match answer.as_str() {
        "y" | "a" | "w" => {
            let scope = match answer.as_str() {
                "a" => "run",
                "w" => "workspace",
                _ => "once",
            };
            let _ = client::call(
                "run.approve",
                json!({"run_id": run_id, "scope": scope, "plan_id": p["plan_id"]}),
            );
        }
        "n" => {
            let _ = client::call(
                "run.reject",
                json!({"run_id": run_id, "plan_id": p["plan_id"], "reason": "rechazado en CLI"}),
            );
        }
        "r" => {
            let _ = client::call("run.replan", json!({"run_id": run_id}));
        }
        _ => {}
    }
}

fn handle_permission(run_id: &str) {
    let Ok(list) = client::call(
        "permission.list",
        json!({"run_id": run_id, "status": "pending"}),
    ) else {
        return;
    };
    let Some(req) = list.as_array().and_then(|a| a.first()) else {
        return;
    };
    out("");
    out(format!(
        "Permiso requerido · {} · target {} · efecto {}",
        req["tool_name"].as_str().unwrap_or("-"),
        req["target"].as_str().unwrap_or("-"),
        req["effect"].as_str().unwrap_or("-")
    ));
    let answer = ask("[y] once  [a] run  [w] workspace  [n] deny  [Enter] esperar > ");
    let request_id = req["request_id"].as_str().unwrap_or("");
    match answer.as_str() {
        "y" | "a" | "w" => {
            let scope = match answer.as_str() {
                "a" => "run",
                "w" => "workspace",
                _ => "once",
            };
            let _ = client::call(
                "permission.approve",
                json!({"request_id": request_id, "scope": scope}),
            );
        }
        "n" => {
            let _ = client::call(
                "permission.deny",
                json!({"request_id": request_id, "reason": "denegado en CLI"}),
            );
        }
        _ => {}
    }
}
