//! Planificación durable: espera, aprobación, rechazo, invalidación y replan.
//!
//! Cada prueba usa su propio `HLI_HOME` y su propio daemon; el contexto se
//! relee del store en cada paso, nunca de memoria del proceso.

mod common;

use common::Ctx;
use serde_json::{Value, json};

fn setup(c: &Ctx) -> (String, String, String, String) {
    let root = c.home().join("root");
    let ws = c.ok("workspace.create", json!({"name": "demo", "root": root}));
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

fn start_run(c: &Ctx, ws: &str, th: &str, br: &str, prompt: &str, provider: Option<&str>) -> Value {
    let mut args = json!({
        "workspace_id": ws, "thread_id": th, "branch_id": br,
        "prompt": prompt, "provider": provider.unwrap_or("sim")
    });
    if provider.is_some() {
        args["timeout_secs"] = json!(2);
    }
    c.ok("run.start", args)
}

#[test]
fn read_only_run_skips_approval_and_completes() {
    let mut c = Ctx::new("plan-read");
    c.start();
    let (ws, th, br, _) = setup(&c);
    let run = start_run(&c, &ws, &th, &br, "resume el hilo", None);
    assert_eq!(run["state"], "completed");
    let plan = c.ok("run.plan", json!({"run_id": run["id"]}));
    assert_eq!(plan["plan"]["status"], "completed");
    assert_eq!(plan["plan"]["revision"], 1);
    assert!(plan["plan"]["steps"].as_array().unwrap().len() >= 2);
    let receipt = &run["receipt"];
    assert_eq!(receipt["plan_id"], plan["plan"]["plan_id"]);
    assert_eq!(receipt["approval_scope"], Value::Null);
}

#[test]
fn effectful_run_waits_and_does_not_execute() {
    let mut c = Ctx::new("plan-wait");
    c.start();
    let (ws, th, br, root) = setup(&c);
    let run = start_run(&c, &ws, &th, &br, "tool:write nota.txt hola", None);
    assert_eq!(run["state"], "waiting_approval");
    assert!(
        !std::path::Path::new(&root).join("nota.txt").exists(),
        "no debe ejecutarse nada antes de aprobar"
    );
    let plan = c.ok("run.plan", json!({"run_id": run["id"]}));
    assert_eq!(plan["plan"]["status"], "waiting_approval");
    assert!(
        !plan["plan"]["requested_effects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // La instantánea es privada y no expone valores de secreto.
    let exec = &plan["execution"];
    assert_eq!(exec["run_id"], run["id"]);
    assert!(
        exec["content_hash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(exec.get("secret_values").is_none());
}

#[test]
fn approve_executes_frozen_snapshot_and_records_receipt() {
    let mut c = Ctx::new("plan-approve");
    c.start();
    let (ws, th, br, root) = setup(&c);
    let run = start_run(&c, &ws, &th, &br, "tool:write nota.txt hola", None);
    let run_id = run["id"].as_str().unwrap().to_string();
    let plan = c.ok("run.plan", json!({"run_id": run_id}));
    let plan_id = plan["plan"]["plan_id"].as_str().unwrap().to_string();
    let snapshot_hash = plan["execution"]["content_hash"]
        .as_str()
        .unwrap()
        .to_string();

    let approved = c.ok(
        "run.approve",
        json!({"run_id": run_id, "scope": "once", "plan_id": plan_id}),
    );
    assert_eq!(approved["scope"], "once");
    assert_eq!(approved["approval_events"].as_array().unwrap().len(), 1);

    let waited = c.ok("run.wait", json!({"run_id": run_id, "timeout_secs": 10}));
    assert_eq!(waited["run"]["state"], "completed");
    let text = std::fs::read_to_string(std::path::Path::new(&root).join("nota.txt")).unwrap();
    assert_eq!(text, "hola");

    let final_run = c.ok("run.get", json!({"run_id": run_id}))["run"].clone();
    let receipt = &final_run["receipt"];
    assert_eq!(receipt["plan_id"], plan_id);
    assert_eq!(receipt["approval_status"], "completed");
    assert_eq!(receipt["approval_scope"], "once");
    assert_eq!(receipt["plan_hash"], plan["plan"]["content_hash"]);
    assert!(
        receipt["approved_effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "write")
    );
    assert_eq!(receipt["plan_revision"], 1);
    let exec_hash = plan["execution"]["content_hash"].as_str().unwrap();
    assert_eq!(receipt["execution_snapshot_hash"], exec_hash);
    assert_eq!(snapshot_hash, exec_hash);
    assert_eq!(receipt["provider_id"], "sim");
    // Nunca se persisten valores de secreto.
    let raw = serde_json::to_string(receipt).unwrap();
    assert!(!raw.contains("sk-"), "el receipt no debe contener claves");
    assert!(!raw.contains("secret_values"));
}

#[test]
fn stale_snapshot_invalidates_keeps_revision_and_requires_replan() {
    let mut c = Ctx::new("plan-stale");
    c.start();
    let (ws, th, br, _) = setup(&c);
    c.ok(
        "provider.add",
        json!({"name": "local", "kind": "ollama", "base_url": "http://127.0.0.1:9",
               "model": "m1", "timeout_seconds": 2}),
    );
    let run = start_run(&c, &ws, &th, &br, "tool:write a.txt x", Some("local"));
    let run_id = run["id"].as_str().unwrap().to_string();
    let before = c.ok("run.plan", json!({"run_id": run_id}));
    let v1 = before["plan"]["plan_id"].as_str().unwrap().to_string();
    let v1_hash = before["plan"]["content_hash"].as_str().unwrap().to_string();

    // La configuración cambia entre plan y aprobación.
    c.ok(
        "provider.add",
        json!({"name": "local", "kind": "ollama", "base_url": "http://127.0.0.1:9",
               "model": "m2", "timeout_seconds": 2}),
    );

    let err = c
        .call("run.approve", json!({"run_id": run_id, "scope": "once"}))
        .expect_err("la aprobación debe invalidarse");
    assert!(err.contains("plan_stale"), "error inesperado: {err}");

    let after = c.ok("run.plan", json!({"run_id": run_id}));
    // La revisión anterior se conserva intacta y el run sigue esperando.
    assert_eq!(after["plan"]["plan_id"], v1);
    assert_eq!(after["plan"]["content_hash"], v1_hash);
    assert_eq!(after["plan"]["revision"], 1);
    assert_eq!(after["run"]["state"], "waiting_approval");

    let replan = c.ok("run.replan", json!({"run_id": run_id}));
    assert_eq!(replan["plan"]["revision"], 2);
    assert_eq!(replan["plan"]["supersedes_plan_id"], v1);
    assert_eq!(replan["previous_plan_id"], v1);
    assert!(
        !replan["plan"]["invalidation_reasons"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(replan["run"]["state"], "waiting_approval");
    assert_eq!(replan["requires_approval"], true);

    // Aprobar la revisión antigua ya no es posible.
    let err = c
        .call(
            "run.approve",
            json!({"run_id": run_id, "scope": "once", "plan_id": v1}),
        )
        .expect_err("la revisión antigua no debe aprobarse");
    assert!(err.contains("plan_revision_changed"), "error: {err}");

    let revisions = c.ok("run.plan", json!({"run_id": run_id}))["revisions"].clone();
    assert_eq!(revisions.as_array().unwrap().len(), 2);
    assert_eq!(revisions[0]["plan_id"], v1);
    assert_eq!(revisions[1]["revision"], 2);
    // La revisión anterior sigue siendo auditable y no se reutiliza.
    assert_eq!(revisions[0]["status"], "waiting_approval");
    assert_eq!(revisions[1]["supersedes_plan_id"], v1);

    // Aprobar la nueva revisión exige una decisión nueva y deja procedencia.
    let new_plan = revisions[1]["plan_id"].as_str().unwrap().to_string();
    c.ok(
        "run.approve",
        json!({"run_id": run_id, "scope": "once", "plan_id": new_plan}),
    );
    let finished = c.ok("run.wait", json!({"run_id": run_id, "timeout_secs": 10}));
    let receipt = &finished["run"]["receipt"];
    assert_eq!(receipt["plan_revision"], 2);
    assert_eq!(receipt["superseded_plan_ids"][0], v1);
    assert!(
        receipt["execution_snapshot_hash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        !receipt["invalidation_events"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true)
    );
}

#[test]
fn secret_values_never_persist_anywhere() {
    let mut c = Ctx::new("plan-secret");
    c.start();
    let (ws, th, br, _) = setup(&c);
    let secrets = c.home().join("secrets");
    std::fs::create_dir_all(&secrets).unwrap();
    std::fs::write(secrets.join("api"), "SUPERSECRET-9f3a\n").unwrap();
    c.ok(
        "provider.add",
        json!({"name": "oai", "kind": "openai", "base_url": "http://127.0.0.1:9/v1",
               "model": "m", "timeout_seconds": 2, "secret_handle": "api"}),
    );
    let run = start_run(&c, &ws, &th, &br, "tool:write secret.txt x", Some("oai"));
    let plan = c.ok("run.plan", json!({"run_id": run["id"]}));
    let exec = &plan["execution"];
    assert_eq!(exec["secret_handles"][0]["id"], "api");
    assert!(
        exec["secret_handles"][0]["version"]
            .as_str()
            .unwrap()
            .starts_with('v')
    );
    assert!(!exec.to_string().contains("SUPERSECRET"));

    let _ = c.call(
        "run.approve",
        json!({"run_id": run["id"], "scope": "once",
               "plan_id": plan["plan"]["plan_id"]}),
    );
    c.ok("run.wait", json!({"run_id": run["id"], "timeout_secs": 10}));

    let got = c.ok("run.get", json!({"run_id": run["id"]}));
    assert!(
        !serde_json::to_string(&got).unwrap().contains("SUPERSECRET"),
        "el receipt no debe contener el valor del secreto"
    );
    let events = c.ok("event.list", json!({"workspace_id": ws}));
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("SUPERSECRET")
    );

    let out = c.home().join("export.hliar");
    c.ok(
        "export",
        json!({"workspace_id": ws, "out": out.to_string_lossy()}),
    );
    let bytes = std::fs::read(&out).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("SUPERSECRET"));
    assert!(!String::from_utf8_lossy(&bytes).contains("secret_values"));

    let db = std::fs::read(c.home().join("hli.db")).unwrap();
    assert!(
        !String::from_utf8_lossy(&db).contains("SUPERSECRET"),
        "la base SQLite no debe contener valores de secreto"
    );
}

#[test]
fn reject_prevents_execution_and_is_idempotent() {
    let mut c = Ctx::new("plan-reject");
    c.start();
    let (ws, th, br, root) = setup(&c);
    let run = start_run(&c, &ws, &th, &br, "tool:write b.txt y", None);
    let run_id = run["id"].as_str().unwrap().to_string();
    let rejected = c.ok(
        "run.reject",
        json!({"run_id": run_id, "reason": "no autorizado"}),
    );
    assert_eq!(rejected["run"]["state"], "cancelled");
    assert_eq!(rejected["plan"]["status"], "rejected");
    assert_eq!(rejected["terminal_reason"], "approval_rejected");
    assert!(!std::path::Path::new(&root).join("b.txt").exists());

    let again = c.ok(
        "run.reject",
        json!({"run_id": run_id, "reason": "otra vez"}),
    );
    assert_eq!(again["idempotent"], true);
    let run = c.ok("run.get", json!({"run_id": run_id}))["run"].clone();
    assert_eq!(run["state"], "cancelled");
    assert_eq!(run["result"]["error"], "approval_rejected");
}

#[test]
fn workspace_scope_persists_across_runs() {
    let mut c = Ctx::new("plan-grant");
    c.start();
    let (ws, th, br, _) = setup(&c);
    let r1 = start_run(&c, &ws, &th, &br, "tool:write c.txt 1", None);
    c.ok(
        "run.approve",
        json!({"run_id": r1["id"], "scope": "workspace"}),
    );
    c.ok("run.wait", json!({"run_id": r1["id"], "timeout_secs": 10}));
    let r2 = start_run(&c, &ws, &th, &br, "tool:write d.txt 2", None);
    assert_eq!(
        r2["state"], "completed",
        "el grant de workspace evita una segunda aprobación"
    );
}
