//! Planificación durable: convierte la intención del run en un plan operativo
//! explícito, clasifica efectos y aplica la política inicial de aprobación.
//!
//! No se guarda razonamiento privado: sólo pasos mostrables, herramientas,
//! objetivos y efectos. El hash del plan cubre exactamente ese contenido.

use hli_core::{
    Result,
    ids::new_id,
    model::{
        ContextManifest, PlanStep, Run, RunPlan, effect_requires_approval, plan_effect_is_known,
    },
    time::now_iso,
};

/// Herramienta que HLI 1.0 nunca ejecuta.
pub const FORBIDDEN_TOOL: &str = "shell.execute";

/// Clasifica una herramienta en un efecto del plan. Lo desconocido no se
/// adivina: se cierra como `unknown` y exige aprobación.
pub fn effect_for_tool(tool: &str) -> &'static str {
    match tool {
        "filesystem.read" => "read",
        "filesystem.write" => "write",
        "http.request" | "network.fetch" => "network",
        "service.call" | "webhook.post" => "external_service",
        "shell.execute" => "destructive",
        _ => "unknown",
    }
}

/// Efecto de una operación declarada por el runtime cuando no hay herramienta.
pub fn effect_for_operation(operation: &str) -> &'static str {
    match operation {
        "reasoning" | "inspect" | "verify" | "filesystem.read" => "read",
        "filesystem.write" => "write",
        "network.request" => "network",
        "service.invoke" => "external_service",
        "shell.execute" => "destructive",
        _ => "unknown",
    }
}

/// Efectos declarados por el plan, en orden estable y sin duplicados.
fn collect_effects(steps: &[PlanStep]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in steps {
        if !out.contains(&s.effect) {
            out.push(s.effect.clone());
        }
    }
    out
}

/// Un paso exige aprobación si su efecto la exige y no hay grant aplicable.
fn step_requires_approval(effect: &str, granted: &[String]) -> bool {
    effect_requires_approval(effect) && !granted.iter().any(|g| g == effect)
}

fn step(
    order: i64,
    description: &str,
    operation_class: &str,
    effect: &str,
    tool: Option<&str>,
    target: Option<&str>,
    granted: &[String],
) -> PlanStep {
    let mut s = PlanStep::new(order, description, operation_class, effect, tool, target);
    s.requires_approval = step_requires_approval(effect, granted);
    s
}

/// Detecta una llamada de herramienta explícita en el prompt (formato del
/// proveedor simulado, usado por el gate y las pruebas).
fn explicit_tool(prompt: &str) -> Option<(String, Option<String>)> {
    let p = prompt.trim();
    if let Some(rest) = p.strip_prefix("tool:write ") {
        let target = rest.split_whitespace().next().map(str::to_string);
        return Some(("filesystem.write".into(), target));
    }
    if let Some(rest) = p.strip_prefix("tool:read ") {
        let target = rest.split_whitespace().next().map(str::to_string);
        return Some(("filesystem.read".into(), target));
    }
    if p.starts_with("tool:shell") {
        return Some(("shell.execute".into(), None));
    }
    if p.starts_with("tool:http") || p.starts_with("tool:network") {
        return Some(("http.request".into(), None));
    }
    None
}

/// Intención de escritura en lenguaje natural. No autoriza nada por sí sola:
/// sólo añade un paso que *solicita* aprobación con efecto `write`.
fn mentions_write(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    [
        "corrige",
        "corregir",
        "modifica",
        "modificar",
        "escribe",
        "escribir",
        "crea el archivo",
        "edit ",
        "write ",
        "fix ",
        "modify ",
    ]
    .iter()
    .any(|k| p.contains(k))
}

/// Construye el plan runtime-generated de un run. `granted` contiene los
/// efectos con grant persistente de workspace.
pub fn build_run_plan(
    run: &Run,
    manifest: &ContextManifest,
    prompt: &str,
    granted: &[String],
) -> Result<RunPlan> {
    let mut steps: Vec<PlanStep> = Vec::new();
    let mut order = 0i64;
    let mut push = |s: PlanStep| {
        order += 1;
        let mut s = s;
        s.order = order;
        steps.push(s);
    };

    push(step(
        0,
        "Inspect the workspace",
        "inspect",
        "read",
        None,
        None,
        granted,
    ));

    match explicit_tool(prompt) {
        Some((tool, target)) => {
            let effect = effect_for_tool(&tool);
            let description = match tool.as_str() {
                "filesystem.write" => "Modify the selected file",
                "filesystem.read" => "Read the selected file",
                "shell.execute" => "Execute a shell command",
                "http.request" => "Call an external endpoint",
                _ => "Run the requested operation",
            };
            push(step(
                0,
                description,
                &tool,
                effect,
                Some(&tool),
                target.as_deref(),
                granted,
            ));
        }
        None => {
            push(step(
                0,
                "Read relevant files",
                "filesystem.read",
                "read",
                Some("filesystem.read"),
                None,
                granted,
            ));
            if mentions_write(prompt) {
                push(step(
                    0,
                    "Request approval before writing",
                    "filesystem.write",
                    "write",
                    Some("filesystem.write"),
                    None,
                    granted,
                ));
            }
        }
    }

    push(step(
        0,
        "Verify the result",
        "verify",
        "read",
        None,
        None,
        granted,
    ));

    // Un efecto no clasificable nunca se convierte en read silencioso.
    for s in steps.iter_mut() {
        if !plan_effect_is_known(&s.effect) {
            s.effect = "unknown".into();
            s.requires_approval = true;
        }
    }

    let requested = collect_effects(&steps);
    let summary = format!(
        "{} pasos; efectos: {}",
        steps.len(),
        if requested.is_empty() {
            "ninguno".to_string()
        } else {
            requested.join(", ")
        }
    );

    let mut plan = RunPlan {
        plan_id: new_id("plan"),
        revision: 1,
        supersedes_plan_id: None,
        invalidation_reasons: vec![],
        run_id: run.id.clone(),
        workspace_id: run.workspace_id.clone(),
        thread_id: run.thread_id.clone(),
        branch_id: run.branch_id.clone(),
        branch_revision: run.branch_revision,
        context_manifest_id: manifest.manifest_id.clone(),
        origin: "runtime_generated".into(),
        status: "draft".into(),
        summary,
        steps,
        requested_effects: requested,
        content_hash: String::new(),
        created_at: now_iso(),
        approved_at: None,
        rejected_at: None,
        approved_by: None,
        approval_scope: None,
        rejection_reason: None,
    };
    plan.seal();
    Ok(plan)
}

/// Plan aceptado tal cual lo entrega el proveedor. Se valida que cada paso
/// declare un efecto conocido; si no, se degrada a `unknown`.
pub fn plan_from_provider(
    run: &Run,
    manifest: &ContextManifest,
    summary: &str,
    steps: Vec<PlanStep>,
    granted: &[String],
) -> Result<RunPlan> {
    let mut steps = steps;
    for (i, s) in steps.iter_mut().enumerate() {
        s.order = i as i64 + 1;
        if !plan_effect_is_known(&s.effect) {
            s.effect = "unknown".into();
        }
        s.requires_approval = step_requires_approval(&s.effect, granted);
    }
    let requested = collect_effects(&steps);
    let mut plan = RunPlan {
        plan_id: new_id("plan"),
        revision: 1,
        supersedes_plan_id: None,
        invalidation_reasons: vec![],
        run_id: run.id.clone(),
        workspace_id: run.workspace_id.clone(),
        thread_id: run.thread_id.clone(),
        branch_id: run.branch_id.clone(),
        branch_revision: run.branch_revision,
        context_manifest_id: manifest.manifest_id.clone(),
        origin: "provider".into(),
        status: "draft".into(),
        summary: summary.to_string(),
        steps,
        requested_effects: requested,
        content_hash: String::new(),
        created_at: now_iso(),
        approved_at: None,
        rejected_at: None,
        approved_by: None,
        approval_scope: None,
        rejection_reason: None,
    };
    plan.seal();
    Ok(plan)
}

/// La política de HLI 1.0 jamás admite ejecución de shell.
pub fn plan_is_denied(plan: &RunPlan) -> bool {
    plan.steps
        .iter()
        .any(|s| s.tool_name.as_deref() == Some(FORBIDDEN_TOOL))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hli_core::canon::hash_canonical;
    use hli_core::model::UserInput;
    use serde_json::json;

    fn run() -> Run {
        Run {
            id: "run_1".into(),
            workspace_id: "ws_1".into(),
            thread_id: "thr_1".into(),
            branch_id: "br_1".into(),
            branch_revision: 3,
            state: "queued".into(),
            approval_kind: None,
            prompt: "hola".into(),
            started_at: "2026-11-09T00:00:00.000Z".into(),
            ended_at: None,
            result: None,
            receipt: None,
        }
    }

    fn manifest() -> ContextManifest {
        ContextManifest {
            manifest_id: "ctx_1".into(),
            policy_version: "test".into(),
            workspace_id: "ws_1".into(),
            run_id: "run_1".into(),
            branch_revision: 3,
            budget_tokens: 1024,
            user_input: UserInput {
                content: "hola".into(),
                content_hash: "sha256:u".into(),
            },
            instructions: vec![],
            memories: vec![],
            recent_events: vec![],
            visible_event_ids: vec![],
            summaries: vec![],
            artifact_slices: vec![],
            excluded: vec![],
            explicit_references: vec![],
            payload_hash: String::new(),
            estimated_tokens: 1,
            content_hash: String::new(),
            created_at: "2026-11-09T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn read_only_plan_does_not_require_approval() {
        let p = build_run_plan(&run(), &manifest(), "resume el hilo", &[]).unwrap();
        assert_eq!(p.origin, "runtime_generated");
        assert!(!p.requires_approval());
        assert_eq!(p.requested_effects, vec!["read"]);
        assert!(p.verify());
    }

    #[test]
    fn write_plan_requires_approval() {
        let p = build_run_plan(&run(), &manifest(), "tool:write nota.txt hola", &[]).unwrap();
        assert!(p.requires_approval());
        assert!(p.requested_effects.contains(&"write".to_string()));
        let step = p
            .steps
            .iter()
            .find(|s| s.effect == "write")
            .expect("paso de escritura");
        assert_eq!(step.tool_name.as_deref(), Some("filesystem.write"));
        assert_eq!(step.target.as_deref(), Some("nota.txt"));
    }

    #[test]
    fn workspace_grant_removes_approval_for_declared_effect() {
        let granted = vec!["write".to_string()];
        let p = build_run_plan(&run(), &manifest(), "tool:write nota.txt hola", &granted).unwrap();
        assert!(!p.requires_approval());
        assert!(p.requested_effects.contains(&"write".to_string()));
    }

    #[test]
    fn shell_plan_is_denied() {
        let p = build_run_plan(&run(), &manifest(), "tool:shell rm -rf", &[]).unwrap();
        assert!(plan_is_denied(&p));
        assert!(p.requires_approval());
    }

    #[test]
    fn unknown_effect_requires_approval() {
        let steps = vec![PlanStep::new(
            1,
            "operación rara",
            "mystery.op",
            "no-clasificado",
            Some("mystery.tool"),
            None,
        )];
        let p = plan_from_provider(&run(), &manifest(), "plan", steps, &[]).unwrap();
        assert_eq!(p.steps[0].effect, "unknown");
        assert!(p.steps[0].requires_approval);
    }

    #[test]
    fn plan_contains_no_hidden_reasoning_fields() {
        let p = build_run_plan(&run(), &manifest(), "hola", &[]).unwrap();
        let v = serde_json::to_value(&p).unwrap();
        let text = v.to_string();
        for key in ["reasoning", "chain_of_thought", "thinking", "scratchpad"] {
            assert!(!text.contains(key), "el plan no debe contener {key}");
        }
        let _ = json!({});
    }

    #[test]
    fn same_plan_produces_same_hash() {
        let a = build_run_plan(&run(), &manifest(), "tool:read a.txt", &[]).unwrap();
        let b = build_run_plan(&run(), &manifest(), "tool:read a.txt", &[]).unwrap();
        assert_eq!(a.content_hash, b.content_hash);
        let c = build_run_plan(&run(), &manifest(), "tool:write a.txt x", &[]).unwrap();
        assert_ne!(a.content_hash, c.content_hash);
    }

    #[test]
    fn plan_links_to_manifest_and_keeps_hash_canonical() {
        let p = build_run_plan(&run(), &manifest(), "hola", &[]).unwrap();
        assert_eq!(p.context_manifest_id, "ctx_1");
        assert_eq!(
            p.content_hash,
            hash_canonical(&serde_json::to_value(p.envelope()).unwrap())
        );
    }
}
