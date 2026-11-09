//! Revisión de plan por herramienta no declarada (feature `fixture-provider`).
//!
//! El provider de prueba declara `filesystem.read` en el plan y emite
//! `filesystem.write` al ejecutar. Nada se escribe hasta que se aprueban la
//! revisión del plan y el permiso concreto.
#![cfg(feature = "fixture-provider")]

mod common;

use common::Ctx;
use serde_json::{Value, json};

fn setup(c: &Ctx) -> (String, String, String, String) {
    let root = c.home().join("root");
    let ws = c.ok("workspace.create", json!({"name": "rev", "root": root}));
    let ws_id = ws["id"].as_str().unwrap().to_string();
    let th = c.ok(
        "thread.create",
        json!({"workspace_id": ws_id, "title": "t"}),
    );
    let th_id = th["id"].as_str().unwrap().to_string();
    let br = c.ok("branch.create", json!({"thread_id": th_id, "name": "main"}));
    let br_id = br["id"].as_str().unwrap().to_string();
    (ws_id, th_id, br_id, root.to_string_lossy().to_string())
}

fn wait_for(c: &Ctx, run_id: &str, kind: &str, timeout: u64) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut last = Value::Null;
    while std::time::Instant::now() < deadline {
        last = c.ok("run.status", json!({"run_id": run_id}))["run"].clone();
        if last["state"] == "waiting_approval" && last["approval_kind"] == kind {
            return last;
        }
        if matches!(
            last["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            return last;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    last
}

fn start(c: &Ctx, ws: &str, th: &str, br: &str) -> Value {
    c.ok(
        "run.start",
        json!({
            "workspace_id": ws, "thread_id": th, "branch_id": br,
            "prompt": "fixture task", "provider": "fixture"
        }),
    )
}

#[test]
fn undeclared_tool_revises_plan_and_requires_both_approvals() {
    let mut c = Ctx::new("rev-flow");
    c.start();
    let (ws, th, br, root) = setup(&c);
    let run = start(&c, &ws, &th, &br);
    let run_id = run["id"].as_str().unwrap().to_string();

    let waiting = wait_for(&c, &run_id, "plan", 10);
    assert_eq!(waiting["state"], "waiting_approval");
    assert_eq!(waiting["approval_kind"], "plan");
    assert!(
        !std::path::Path::new(&root)
            .join("revision-test.txt")
            .exists(),
        "la herramienta no declarada no debe ejecutarse"
    );

    let plan = c.ok("run.plan", json!({"run_id": run_id})).clone();
    let current = plan["plan"].clone();
    assert_eq!(current["revision"], 2);
    assert!(
        current["requested_effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "write")
    );
    let v1 = current["supersedes_plan_id"].as_str().unwrap().to_string();
    let revisions = plan["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0]["plan_id"], v1);
    assert_eq!(revisions[0]["status"], "cancelled");

    let events = c.ok("event.list", json!({"workspace_id": ws}));
    let kinds: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["run_id"].as_str() == Some(run_id.as_str()))
        .filter_map(|e| e["type"].as_str())
        .collect();
    assert!(kinds.contains(&"plan.created"));
    assert!(kinds.contains(&"plan.revised"));
    assert!(kinds.contains(&"run.waiting_approval"));
    assert!(!kinds.contains(&"tool.completed"));

    // La aprobación de la revisión 1 no cubre la 2.
    let err = c
        .call(
            "run.approve",
            json!({"run_id": run_id, "scope": "once", "plan_id": v1}),
        )
        .expect_err("la revisión antigua no debe aprobarse");
    assert!(err.contains("plan_revision_changed"), "{err}");

    // Aprobar la revisión 2 abre el permiso concreto, no la ejecución.
    c.ok(
        "run.approve",
        json!({"run_id": run_id, "scope": "once", "plan_id": current["plan_id"]}),
    );
    let tool_wait = wait_for(&c, &run_id, "tool", 10);
    assert_eq!(tool_wait["approval_kind"], "tool");
    assert!(
        !std::path::Path::new(&root)
            .join("revision-test.txt")
            .exists()
    );

    let pending = c.ok(
        "permission.list",
        json!({"run_id": run_id, "status": "pending"}),
    );
    let req = pending[0].clone();
    c.ok(
        "permission.approve",
        json!({"request_id": req["request_id"], "scope": "once"}),
    );
    let done = c.ok("run.wait", json!({"run_id": run_id, "timeout_secs": 10}))["run"].clone();
    assert_eq!(done["state"], "completed");
    assert_eq!(
        std::fs::read_to_string(std::path::Path::new(&root).join("revision-test.txt")).unwrap(),
        "written-after-revision"
    );

    let receipt = &done["receipt"];
    assert_eq!(receipt["plan_revision"], 2);
    assert_eq!(receipt["superseded_plan_ids"][0], v1);
    assert_eq!(receipt["permission_request_ids"][0], req["request_id"]);
    assert_eq!(
        receipt["permission_decisions"][0]["arguments_hash"],
        req["arguments_hash"]
    );

    // Repetir la aprobación no vuelve a ejecutar.
    let again = c.ok(
        "permission.approve",
        json!({"request_id": req["request_id"], "scope": "once"}),
    );
    assert_eq!(again["idempotent"], true);
    let after = c.ok("run.status", json!({"run_id": run_id}))["run"].clone();
    assert_eq!(after["state"], "completed");
}

#[test]
fn revised_plan_survives_daemon_restart() {
    let mut c = Ctx::new("rev-restart");
    c.start();
    let (ws, th, br, _root) = setup(&c);
    let run = start(&c, &ws, &th, &br);
    let run_id = run["id"].as_str().unwrap().to_string();
    wait_for(&c, &run_id, "plan", 10);

    c.stop();
    c.start();
    let plan = c.ok("run.plan", json!({"run_id": run_id}));
    assert_eq!(plan["plan"]["revision"], 2);
    assert_eq!(plan["revisions"].as_array().unwrap().len(), 2);
    let run_state = c.ok("run.status", json!({"run_id": run_id}))["run"].clone();
    assert_eq!(run_state["state"], "waiting_approval");
    assert_eq!(run_state["approval_kind"], "plan");
}
