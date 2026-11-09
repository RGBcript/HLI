//! Ciclo durable de un run: planificación, aprobación y ejecución reanudable.
//!
//! `prepare` fija el contexto i) manifiesto, ii) plan público, iii) instantánea
//! privada de ejecución. Si el plan declara efectos, el run queda en
//! `waiting_approval` y muere el handler: nada se ejecuta. `approve` verifica
//! la instantánea y sólo entonces arranca `execute_prepared_run`, que relee
//! todo del store, comprueba hashes y ejecuta.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hli_core::{
    HliError,
    ids::new_id,
    model::{
        ContextManifest, EXEC_SNAPSHOT_VERSION, ExecMchInstance, ExecSampling, ExecSnapshot,
        ModuleVersion, PermissionGrant, PlanStep, Receipt, Run, RunPlan, SecretHandleRef, ToolCall,
        ToolPermissionRequest, approval_scope_valid, run_is_terminal,
    },
    time::now_iso,
};
use hli_provider::ChatProvider;
use hli_store::{Reader, Writer};
use serde_json::{Value, json};

use crate::{cancels, load_price, make_receipt, providers, read_secret};

pub const SYSTEM_PROMPT: &str = "\
Eres HLI (Hierarchical Layered Intelligence), un agente local que ejecuta tareas
sobre el workspace del usuario. No eres un asistente genérico ni un producto de
terceros: HLI es un runtime agéntico con event log, planes y permisos.

Protocolo:
1. Responde en el idioma del usuario, breve y factual.
2. Si la tarea requiere leer o escribir archivos, declara el plan y los efectos.
   No inventes rutas, contenidos ni resultados.
3. Si no puedes ejecutar algo, dilo y explica qué falta.
4. No reveles secretos, credenciales ni variables de entorno.
5. Si generas código o HTML, entrégalo completo y autocontenido.";

/// Datos ya preparados por `run.start` (manifiesto sellado y payload
/// verificado). Todo lo demás se decide aquí.
pub struct Prepared {
    pub run: Run,
    pub manifest: ContextManifest,
    pub prompt: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub model: String,
    pub base_url: String,
    pub timeout_secs: u64,
    pub temperature: f64,
    pub max_tokens: i64,
    pub budget_tokens: i64,
    pub secret_handles: Vec<Value>,
    pub candidate_memory_ids: Vec<String>,
    pub is_async: bool,
    pub sim_delay_ms: Option<u64>,
}

/// Plan declarado por el provider de prueba: sólo la tool declarada, nunca la
/// que emitirá durante la ejecución.
#[cfg(feature = "fixture-provider")]
fn fixture_declared_plan(
    run: &Run,
    manifest: &ContextManifest,
    kind: &str,
    granted: &[String],
) -> Result<Option<RunPlan>, HliError> {
    if kind != "fixture" {
        return Ok(None);
    }
    let f = crate::fixture::FixtureProvider::from_env();
    let step = PlanStep::new(
        1,
        "Declared operation",
        &f.declared_tool,
        &f.declared_effect,
        Some(&f.declared_tool),
        None,
    );
    Ok(Some(hli_agent::plan_from_provider(
        run,
        manifest,
        "fixture plan",
        vec![step],
        granted,
    )?))
}

#[cfg(not(feature = "fixture-provider"))]
fn fixture_declared_plan(
    _run: &Run,
    _manifest: &ContextManifest,
    _kind: &str,
    _granted: &[String],
) -> Result<Option<RunPlan>, HliError> {
    Ok(None)
}

/// Resultado interno del provider de prueba.
#[allow(dead_code)]
type FixtureCall = (
    String,
    hli_provider::Usage,
    String,
    String,
    String,
    Option<ToolCall>,
);

#[cfg(feature = "fixture-provider")]
fn fixture_complete(run: &Run, manifest: &ContextManifest) -> Result<FixtureCall, HliError> {
    let f = crate::fixture::FixtureProvider::from_env();
    let req = hli_agent::ProviderRequest {
        run_id: run.id.clone(),
        prompt: run.prompt.clone(),
        manifest: manifest.clone(),
    };
    let resp = hli_agent::Provider::complete(&f, &req)?;
    Ok((
        resp.text,
        hli_provider::Usage::default(),
        "fixture".into(),
        "fixture-1".into(),
        "fixture".into(),
        resp.tool_call,
    ))
}

#[cfg(not(feature = "fixture-provider"))]
fn fixture_complete(_run: &Run, _manifest: &ContextManifest) -> Result<FixtureCall, HliError> {
    Err(HliError::invalid("provider fixture no compilado"))
}

fn permission_label(tool: &str) -> &'static str {
    match hli_agent::permission_for(tool) {
        hli_agent::Permission::Allow => "allow",
        hli_agent::Permission::Ask => "ask",
        hli_agent::Permission::Deny => "deny",
    }
}

fn tool_policy() -> BTreeMap<String, String> {
    ["filesystem.read", "filesystem.write", "shell.execute"]
        .into_iter()
        .map(|t| (t.to_string(), permission_label(t).to_string()))
        .collect()
}

fn module_versions(r: &Reader) -> Vec<ModuleVersion> {
    r.modules()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            Some(ModuleVersion {
                name: m.get("name")?.as_str()?.to_string(),
                version: m.get("version")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn mch_instances(r: &Reader, workspace_id: &str) -> Vec<ExecMchInstance> {
    r.mchs()
        .unwrap_or_default()
        .into_iter()
        .filter(|m| {
            m.get("workspace_id")
                .and_then(|v| v.as_str())
                .is_none_or(|w| w == workspace_id)
        })
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let module_name = m.get("module")?.as_str()?.to_string();
            let version = r
                .module(&module_name)
                .ok()
                .and_then(|mm| {
                    mm.get("version")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            Some(ExecMchInstance {
                name,
                profile: m
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                module_name,
                version,
            })
        })
        .collect()
}

fn secret_refs(home: &Path, handles: &[Value]) -> (Vec<SecretHandleRef>, Vec<String>) {
    let mut refs = Vec::new();
    let mut problems = Vec::new();
    for h in handles {
        let Some(id) = h.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        match read_secret(home, id) {
            Ok((_, version)) => refs.push(SecretHandleRef {
                id: id.to_string(),
                version,
            }),
            Err(_) => problems.push(format!("secreto {id} no disponible")),
        }
    }
    (refs, problems)
}

#[allow(clippy::too_many_arguments)]
fn build_snapshot(
    home: &Path,
    r: &Reader,
    run: &Run,
    plan: &RunPlan,
    provider_id: &str,
    provider_kind: &str,
    model: &str,
    base_url: &str,
    timeout_secs: u64,
    temperature: f64,
    max_tokens: i64,
    budget_tokens: i64,
    secret_handles: &[Value],
    candidate_memory_ids: &[String],
    sim_delay_ms: Option<u64>,
) -> (ExecSnapshot, Vec<String>) {
    let (handles, problems) = secret_refs(home, secret_handles);
    let mut snap = ExecSnapshot {
        snapshot_id: new_id("exec"),
        snapshot_version: EXEC_SNAPSHOT_VERSION,
        run_id: run.id.clone(),
        plan_id: plan.plan_id.clone(),
        provider_id: provider_id.to_string(),
        provider_kind: provider_kind.to_string(),
        endpoint: base_url.to_string(),
        model: model.to_string(),
        adapter_version: env!("CARGO_PKG_VERSION").to_string(),
        sampling: ExecSampling {
            temperature,
            max_tokens,
            seed: 0,
        },
        timeout_secs,
        budget_tokens,
        context_manifest_id: plan.context_manifest_id.clone(),
        context_manifest_hash: plan_hash_of_manifest(r, run),
        branch_revision: run.branch_revision,
        tool_policy: tool_policy(),
        module_versions: module_versions(r),
        mch_instances: mch_instances(r, &run.workspace_id),
        secret_handles: handles,
        candidate_memory_ids: candidate_memory_ids.to_vec(),
        sim_delay_ms,
        created_at: now_iso(),
        content_hash: String::new(),
    };
    snap.seal();
    (snap, problems)
}

fn plan_hash_of_manifest(r: &Reader, run: &Run) -> String {
    r.manifest_for_run(&run.id)
        .map(|m| m.content_hash)
        .unwrap_or_default()
}

fn effect_grants(r: &Reader, workspace_id: &str) -> Vec<String> {
    r.grants(workspace_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|g| g.scope == "workspace")
        .map(|g| g.effect)
        .collect()
}

/// Un paso del plan cubre la llamada si declara la misma herramienta y, cuando
/// el paso fija un objetivo, coincide con la ruta solicitada.
fn step_covers(step: &PlanStep, call: &hli_core::model::ToolCall) -> bool {
    if step.tool_name.as_deref() != Some(call.name.as_str()) {
        return false;
    }
    match step.target.as_deref() {
        None => true,
        Some(t) => hli_agent::normalize_target(&call.name, &call.arguments).as_deref() == Some(t),
    }
}

/// Planifica y decide: waiting o ejecución. Devuelve el `Run` persistido.
pub fn plan_and_maybe_execute(
    home: &Path,
    w: &Writer,
    r: &Arc<Reader>,
    p: Prepared,
) -> Result<Value, HliError> {
    let granted = effect_grants(r, &p.run.workspace_id);
    let mut plan = match fixture_declared_plan(&p.run, &p.manifest, &p.provider_kind, &granted)? {
        Some(plan) => plan,
        None => hli_agent::build_run_plan(&p.run, &p.manifest, &p.prompt, &granted)?,
    };

    let (snapshot, problems) = build_snapshot(
        home,
        r,
        &p.run,
        &plan,
        &p.provider_id,
        &p.provider_kind,
        &p.model,
        &p.base_url,
        p.timeout_secs,
        p.temperature,
        p.max_tokens,
        p.budget_tokens,
        &p.secret_handles,
        &p.candidate_memory_ids,
        p.sim_delay_ms,
    );

    if !problems.is_empty() {
        plan.status = "failed".into();
        let plan = w.save_plan(plan, Some(serde_json::to_value(&snapshot)?))?;
        let _ = w.set_run_state(&p.run.id, "failed");
        let usage = hli_provider::Usage::default();
        let mut receipt = make_receipt(
            &p.provider_id,
            &p.model,
            &snapshot.adapter_version,
            &p.manifest,
            &json!({"error": problems.join("; ")}),
            &usage,
            &hli_provider::cost_of(None, &usage),
            p.secret_handles.clone(),
            vec![],
            vec![],
            vec![],
            p.candidate_memory_ids.clone(),
            vec![],
        );
        attach_plan(&mut receipt, &plan, &snapshot, vec![]);
        let done = w.finish_run(
            &p.run.id,
            "failed",
            Some(json!({"error": "secret_unavailable", "details": problems})),
            Some(receipt),
        )?;
        return Ok(serde_json::to_value(done)?);
    }

    // Política: shell.execute nunca se ejecuta en HLI 1.0.
    if hli_agent::plan_is_denied(&plan) {
        plan.status = "failed".into();
        let plan = w.save_plan(plan, Some(serde_json::to_value(&snapshot)?))?;
        w.set_run_state(&p.run.id, "planning")?;
        let usage = hli_provider::Usage::default();
        let mut receipt = make_receipt(
            &p.provider_id,
            &p.model,
            &snapshot.adapter_version,
            &p.manifest,
            &json!({"error": "tool_denied"}),
            &usage,
            &hli_provider::cost_of(None, &usage),
            p.secret_handles.clone(),
            vec![],
            vec![],
            vec![],
            p.candidate_memory_ids.clone(),
            vec![],
        );
        attach_plan(&mut receipt, &plan, &snapshot, vec![]);
        let done = w.finish_run(
            &p.run.id,
            "failed",
            Some(json!({
                "error": "herramienta denegada por política",
                "tool": hli_agent::FORBIDDEN_TOOL
            })),
            Some(receipt),
        )?;
        return Ok(serde_json::to_value(done)?);
    }

    let requires = plan.requires_approval();
    plan.status = if requires {
        "waiting_approval".into()
    } else {
        "executing".into()
    };
    let plan = w.save_plan(plan, Some(serde_json::to_value(&snapshot)?))?;
    let _ = plan;

    w.set_run_state(&p.run.id, "planning")?;
    if requires {
        let run = w.set_run_state(&p.run.id, "waiting_approval")?;
        let _ = w.set_approval_kind(&p.run.id, Some("plan"));
        return Ok(serde_json::to_value(run)?);
    }

    let run = w.set_run_state(&p.run.id, "running")?;
    if p.is_async {
        let w2 = w.clone();
        let r2 = Arc::clone(r);
        let home2 = home.to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        cancels()
            .lock()
            .unwrap()
            .insert(run.id.clone(), cancel.clone());
        let run_id = run.id.clone();
        std::thread::spawn(move || {
            let home3 = home2.clone();
            let _ = execute_prepared_run(&home3, &w2, &r2, &run_id, cancel);
        });
        return Ok(serde_json::to_value(run)?);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    cancels()
        .lock()
        .unwrap()
        .insert(run.id.clone(), cancel.clone());
    let done = execute_prepared_run(home, w, r, &run.id, cancel)?;
    Ok(serde_json::to_value(done)?)
}

fn attach_plan(
    receipt: &mut Receipt,
    plan: &RunPlan,
    snapshot: &ExecSnapshot,
    approval_events: Vec<String>,
) {
    receipt.plan_id = Some(plan.plan_id.clone());
    receipt.plan_hash = Some(plan.content_hash.clone());
    receipt.plan_origin = Some(plan.origin.clone());
    receipt.plan_revision = Some(plan.revision);
    receipt.approval_status = Some(plan.status.clone());
    receipt.approval_scope = plan.approval_scope.clone();
    receipt.approval_events = approval_events;
    receipt.approved_effects = plan.effectful_requests();
    receipt.effective_permissions = json!(snapshot.tool_policy);
    receipt.execution_snapshot_hash = Some(snapshot.content_hash.clone());
    receipt.provider_id = Some(snapshot.provider_id.clone());
    receipt.superseded_plan_ids = plan.supersedes_plan_id.iter().cloned().collect();
}

/// Ejecuta la instantánea aprobada. Relee run, manifiesto, plan y snapshot y
/// verifica sus hashes antes de llamar al proveedor.
pub fn execute_prepared_run(
    home: &Path,
    w: &Writer,
    r: &Reader,
    run_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Run, HliError> {
    let run = r.run(run_id)?;
    if run_is_terminal(&run.state) {
        cancels().lock().unwrap().remove(run_id);
        return Ok(run);
    }
    let manifest = match r.manifest_for_run(run_id) {
        Ok(m) => m,
        Err(e) => {
            finish_error(w, &run, None, "manifest_missing", &e.message, vec![])?;
            cancels().lock().unwrap().remove(run_id);
            return r.run(run_id);
        }
    };
    let plan = match r.plan_for_run(run_id) {
        Ok(p) => p,
        Err(e) => {
            finish_error(w, &run, None, "plan_missing", &e.message, vec![])?;
            cancels().lock().unwrap().remove(run_id);
            return r.run(run_id);
        }
    };
    let snapshot: ExecSnapshot = match r.plan_execution(run_id)? {
        Some(v) => serde_json::from_value(v)
            .map_err(|e| HliError::internal(format!("instantánea inválida: {e}")))?,
        None => {
            finish_error(
                w,
                &run,
                Some(&plan),
                "snapshot_missing",
                "sin instantánea",
                vec![],
            )?;
            cancels().lock().unwrap().remove(run_id);
            return r.run(run_id);
        }
    };
    if !snapshot.verify() || snapshot.context_manifest_hash != manifest.content_hash {
        finish_error(
            w,
            &run,
            Some(&plan),
            "snapshot_stale",
            "la instantánea no coincide con el contexto",
            vec![],
        )?;
        cancels().lock().unwrap().remove(run_id);
        return r.run(run_id);
    }

    let started = std::time::Instant::now();
    if snapshot.provider_kind == "sim"
        && let Some(ms) = snapshot.sim_delay_ms
    {
        std::thread::sleep(Duration::from_millis(ms));
    }

    let system = SYSTEM_PROMPT.to_string();
    let mut tool_call = None;
    let call: Result<(String, hli_provider::Usage, String, String, String), HliError> =
        match snapshot.provider_kind.as_str() {
            "fixture" => {
                let (text, usage, provider, model, adapter, call) =
                    fixture_complete(&run, &manifest)?;
                tool_call = call;
                Ok((text, usage, provider, model, adapter))
            }
            "sim" => {
                let p = hli_agent::SimProvider;
                let req = hli_agent::ProviderRequest {
                    run_id: run.id.clone(),
                    prompt: run.prompt.clone(),
                    manifest: manifest.clone(),
                };
                match hli_agent::Provider::complete(&p, &req) {
                    Ok(resp) => {
                        tool_call = resp.tool_call;
                        let usage = hli_provider::Usage {
                            input_tokens: hli_provider::estimate_tokens(&system)
                                + manifest.estimated_tokens,
                            output_tokens: hli_provider::estimate_tokens(&resp.text),
                            cached_tokens: 0,
                            latency_first_token_ms: 0,
                            latency_total_ms: 0,
                            finish_reason: "stop".into(),
                            raw_usage_available: false,
                            usage_source: "estimated".into(),
                        };
                        Ok((
                            resp.text,
                            usage,
                            "sim".into(),
                            "sim-1".into(),
                            "0.2.0".into(),
                        ))
                    }
                    Err(e) => Err(e),
                }
            }
            "ollama" => {
                let p = hli_provider::OllamaProvider::new(
                    &snapshot.endpoint,
                    &snapshot.model,
                    snapshot.timeout_secs,
                );
                let rendered = crate::render_for(&manifest, r);
                let req = hli_provider::ChatRequest {
                    system: &system,
                    user: &rendered,
                    temperature: snapshot.sampling.temperature,
                    max_tokens: snapshot.sampling.max_tokens,
                };
                p.chat(&req)
                    .map(|x| (x.text, x.usage, x.provider, x.model, x.adapter_version))
            }
            "openai" => {
                let handle = snapshot
                    .secret_handles
                    .first()
                    .map(|h| h.id.clone())
                    .or_else(|| {
                        providers::load(home)
                            .providers
                            .get(&snapshot.provider_id)
                            .and_then(|e| e.secret_handle.clone())
                    });
                let Some(handle) = handle else {
                    finish_error(
                        w,
                        &run,
                        Some(&plan),
                        "secret_missing",
                        "openai requiere secret handle",
                        vec![],
                    )?;
                    cancels().lock().unwrap().remove(run_id);
                    return r.run(run_id);
                };
                let (key, _) = match read_secret(home, &handle) {
                    Ok(v) => v,
                    Err(e) => {
                        finish_error(w, &run, Some(&plan), "secret_missing", &e.message, vec![])?;
                        cancels().lock().unwrap().remove(run_id);
                        return r.run(run_id);
                    }
                };
                let p = hli_provider::OpenAiProvider::new(
                    &snapshot.endpoint,
                    &snapshot.model,
                    &key,
                    snapshot.timeout_secs,
                );
                let rendered = crate::render_for(&manifest, r);
                let req = hli_provider::ChatRequest {
                    system: &system,
                    user: &rendered,
                    temperature: snapshot.sampling.temperature,
                    max_tokens: snapshot.sampling.max_tokens,
                };
                p.chat(&req)
                    .map(|x| (x.text, x.usage, x.provider, x.model, x.adapter_version))
            }
            other => Err(HliError::invalid(format!("proveedor desconocido: {other}"))),
        };

    let (text, usage, used_provider, used_model, adapter) = match call {
        Ok(v) => v,
        Err(e) => {
            let state = if e.message.contains("timeout") || e.message.contains("timed out") {
                "timed_out"
            } else {
                "failed"
            };
            let usage = hli_provider::Usage::default();
            let mut receipt = make_receipt(
                &snapshot.provider_id,
                &snapshot.model,
                &snapshot.adapter_version,
                &manifest,
                &json!({"error": e.message}),
                &usage,
                &hli_provider::cost_of(None, &usage),
                snapshot
                    .secret_handles
                    .iter()
                    .map(|h| json!({"id": h.id, "version": h.version}))
                    .collect(),
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            );
            let plan = w
                .set_plan_status(
                    &plan.plan_id,
                    "failed",
                    plan.approval_scope.clone(),
                    plan.approved_by.clone(),
                    Some("provider_error".into()),
                )
                .unwrap_or(plan);
            attach_plan(&mut receipt, &plan, &snapshot, vec![]);
            receipt.invalidation_events = invalidated_events(r, &run, run_id);
            let _ = w.finish_run(
                run_id,
                state,
                Some(json!({"error": e.message})),
                Some(receipt),
            );
            cancels().lock().unwrap().remove(run_id);
            return r.run(run_id);
        }
    };

    let price = load_price(home, &used_provider, &used_model);
    let cost = hli_provider::cost_of(price.as_ref(), &usage);

    let mut result = json!({
        "text": text,
        "manifest_id": manifest.manifest_id,
        "provider": used_provider,
        "model": used_model
    });
    let mut state = "completed";

    if let Some(call) = tool_call {
        let _ = w.run_event(
            run_id,
            "tool.requested",
            json!({"run_id": run_id, "tool": call.name, "arguments": call.arguments}),
        );
        let declared = plan
            .steps
            .iter()
            .position(|s| step_covers(s, &call))
            .filter(|_| plan.status != "waiting_approval");
        match hli_agent::permission_for(&call.name) {
            hli_agent::Permission::Deny => {
                let _ = w.run_event(
                    run_id,
                    "tool.denied",
                    json!({"run_id": run_id, "tool": call.name}),
                );
                state = "failed";
                result["error"] = json!("herramienta denegada por política");
            }
            hli_agent::Permission::Ask if declared.is_some() => {
                let target = hli_agent::normalize_target(&call.name, &call.arguments);
                let effect = hli_agent::effect_for_tool(&call.name);
                let grants = r
                    .tool_grants(&run.workspace_id, Some(run_id))
                    .unwrap_or_default();
                match hli_agent::decide(&call.name, effect, target.as_deref(), run_id, &grants) {
                    hli_agent::ToolDecision::Deny => {
                        let _ = w.run_event(
                            run_id,
                            "tool.denied",
                            json!({"run_id": run_id, "tool": call.name}),
                        );
                        state = "failed";
                        result["error"] = json!("herramienta denegada por política");
                    }
                    hli_agent::ToolDecision::Ask => {
                        let (redacted, has_secrets) = hli_agent::redact_arguments(&call.arguments);
                        let request = ToolPermissionRequest {
                            request_id: hli_core::ids::new_id("perm"),
                            run_id: run.id.clone(),
                            plan_id: plan.plan_id.clone(),
                            plan_revision: plan.revision,
                            workspace_id: run.workspace_id.clone(),
                            thread_id: run.thread_id.clone(),
                            branch_id: run.branch_id.clone(),
                            tool_name: call.name.clone(),
                            operation: call.name.clone(),
                            effect: effect.to_string(),
                            target: target.clone(),
                            arguments_hash: hli_agent::invocation_hash(&call.name, &call.arguments),
                            arguments_summary: hli_agent::summarize(&call.name, &call.arguments),
                            reason: "invocación declarada en el plan aprobado".into(),
                            requested_scope: None,
                            status: "pending".into(),
                            created_at: now_iso(),
                            decided_at: None,
                            decided_by: None,
                            decision: None,
                            decision_scope: None,
                            content_hash: String::new(),
                        };
                        // Si hubo redacción no se persiste la invocación: no
                        // puede reanudarse sin los valores originales.
                        let invocation = if has_secrets {
                            None
                        } else {
                            Some(call.arguments.clone())
                        };
                        let _ = w.save_tool_request(request, invocation, has_secrets)?;
                        let _ = redacted;
                        w.set_run_state(run_id, "waiting_approval")?;
                        let _ = w.set_approval_kind(run_id, Some("tool"));
                        cancels().lock().unwrap().remove(run_id);
                        return r.run(run_id);
                    }
                    hli_agent::ToolDecision::Allow => {
                        let root = r.workspace(&run.workspace_id)?.root_path;
                        let outcome = hli_agent::execute_tool(Path::new(&root), &call);
                        let _ = w.run_event(
                            run_id,
                            "tool.completed",
                            json!({
                                "run_id": run_id, "tool": outcome.tool,
                                "status": outcome.status, "stdout": outcome.stdout,
                                "changed_files": outcome.changed_files, "error": outcome.error
                            }),
                        );
                        result["observation"] = serde_json::to_value(&outcome)?;
                        if outcome.status != "success" {
                            state = "failed";
                        }
                    }
                }
            }
            hli_agent::Permission::Ask if declared.is_none() => {
                // La aprobación no cubre un efecto no declarado: no se ejecuta,
                // se añade el paso al plan y el run vuelve a esperar revisión.
                let mut revised = plan.clone();
                revised.plan_id = hli_core::ids::new_id("plan");
                revised.revision = plan.revision + 1;
                revised.supersedes_plan_id = Some(plan.plan_id.clone());
                revised.invalidation_reasons = vec![format!("new_effect:{}", call.name)];
                let mut extra = PlanStep::new(
                    revised.steps.len() as i64 + 1,
                    "Ejecutar herramienta no declarada",
                    &call.name,
                    hli_agent::effect_for_tool(&call.name),
                    Some(&call.name),
                    call.arguments["path"].as_str(),
                );
                extra.requires_approval = true;
                revised.steps.push(extra);
                let effect = hli_agent::effect_for_tool(&call.name).to_string();
                if !revised.requested_effects.contains(&effect) {
                    revised.requested_effects.push(effect);
                }
                revised.status = "waiting_approval".into();
                // La nueva revisión necesita su propia instantánea: mismas
                // condiciones revisadas, nuevo plan_id y sello.
                let mut snap2 = snapshot.clone();
                snap2.snapshot_id = hli_core::ids::new_id("exec");
                snap2.plan_id = revised.plan_id.clone();
                snap2.created_at = hli_core::time::now_iso();
                snap2.seal();
                let new_id = revised.plan_id.clone();
                let _ = w.revise_plan(revised, Some(serde_json::to_value(&snap2)?))?;
                let _ = w.set_plan_status(
                    &plan.plan_id,
                    "cancelled",
                    None,
                    None,
                    Some(format!("superseded_by:{new_id}")),
                );
                state = "waiting_approval";
                result["pending_tool"] = serde_json::to_value(&call)?;
            }
            hli_agent::Permission::Ask | hli_agent::Permission::Allow => {
                let root = r.workspace(&run.workspace_id)?.root_path;
                let outcome = hli_agent::execute_tool(Path::new(&root), &call);
                let _ = w.run_event(
                    run_id,
                    "tool.completed",
                    json!({
                        "run_id": run_id, "tool": outcome.tool, "status": outcome.status,
                        "stdout": outcome.stdout, "changed_files": outcome.changed_files,
                        "error": outcome.error
                    }),
                );
                result["observation"] = serde_json::to_value(&outcome)?;
                if outcome.status != "success" {
                    state = "failed";
                }
            }
        }
    }

    let cancelled = cancel.load(Ordering::SeqCst);
    let timed_out = started.elapsed() > Duration::from_secs(snapshot.timeout_secs);
    if cancelled && state == "completed" {
        state = "cancelled";
    } else if timed_out && state == "completed" {
        state = "timed_out";
    }

    let allowed_read: Vec<String> = manifest
        .memories
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_string))
        .collect();
    let used_slices = manifest
        .artifact_slices
        .iter()
        .filter_map(|s| s["slice_id"].as_str().map(str::to_string))
        .collect();
    let used_events = manifest
        .recent_events
        .iter()
        .filter_map(|e| e["event_id"].as_str().map(str::to_string))
        .collect();

    // Una revisión pendiente no es un estado terminal: el run espera la
    // aprobación del plan nuevo y no genera receipt.
    if state == "waiting_approval" {
        let run = w.set_run_state(run_id, "waiting_approval")?;
        let _ = w.set_approval_kind(run_id, Some("plan"));
        cancels().lock().unwrap().remove(run_id);
        return Ok(run);
    }

    // El estado del plan se fija antes del receipt para que la procedencia
    // registrada sea el estado terminal real, no `executing`.
    let current = r.plan_for_run(run_id).unwrap_or_else(|_| plan.clone());
    let plan = if state == "waiting_approval" {
        if current.plan_id != plan.plan_id {
            // Ya existe una revisión vigente que espera aprobación; la anterior
            // se cierra como sustituida y no se toca la nueva.
            let _ = w.set_plan_status(
                &plan.plan_id,
                "cancelled",
                None,
                None,
                Some(format!("superseded_by:{}", current.plan_id)),
            );
            current
        } else {
            w.set_plan_status(
                &plan.plan_id,
                "waiting_approval",
                plan.approval_scope.clone(),
                plan.approved_by.clone(),
                Some("new_effect".into()),
            )
            .unwrap_or(plan)
        }
    } else {
        let plan_state = if state == "completed" {
            "completed"
        } else {
            "failed"
        };
        w.set_plan_status(
            &plan.plan_id,
            plan_state,
            plan.approval_scope.clone(),
            plan.approved_by.clone(),
            None,
        )
        .unwrap_or(plan)
    };

    let rejected_memories: Vec<String> = snapshot
        .candidate_memory_ids
        .iter()
        .filter(|id| !allowed_read.contains(id))
        .cloned()
        .collect();
    let mut receipt = make_receipt(
        &used_provider,
        &used_model,
        &adapter,
        &manifest,
        &result,
        &usage,
        &cost,
        snapshot
            .secret_handles
            .iter()
            .map(|h| json!({"id": h.id, "version": h.version}))
            .collect(),
        allowed_read,
        used_slices,
        used_events,
        snapshot.candidate_memory_ids.clone(),
        rejected_memories,
    );
    attach_plan(&mut receipt, &plan, &snapshot, vec![]);
    receipt.invalidation_events = invalidated_events(r, &run, run_id);

    let done = w.finish_run(run_id, state, Some(result), Some(receipt))?;
    cancels().lock().unwrap().remove(run_id);
    Ok(done)
}

fn finish_error(
    w: &Writer,
    run: &Run,
    plan: Option<&RunPlan>,
    code: &str,
    message: &str,
    events: Vec<String>,
) -> Result<(), HliError> {
    let usage = hli_provider::Usage::default();
    let manifest = hli_core::model::ContextManifest {
        manifest_id: plan
            .map(|p| p.context_manifest_id.clone())
            .unwrap_or_default(),
        policy_version: String::new(),
        workspace_id: run.workspace_id.clone(),
        run_id: run.id.clone(),
        branch_revision: run.branch_revision,
        budget_tokens: 0,
        user_input: hli_core::model::UserInput {
            content: run.prompt.clone(),
            content_hash: hli_core::canon::hash_canonical(&json!(run.prompt)),
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
        estimated_tokens: 0,
        content_hash: String::new(),
        created_at: now_iso(),
    };
    let mut receipt = make_receipt(
        "unavailable",
        "unavailable",
        "0",
        &manifest,
        &json!({"error": code, "message": message}),
        &usage,
        &hli_provider::cost_of(None, &usage),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    if let Some(p) = plan {
        receipt.plan_id = Some(p.plan_id.clone());
        receipt.plan_hash = Some(p.content_hash.clone());
        receipt.plan_origin = Some(p.origin.clone());
        receipt.plan_revision = Some(p.revision);
        receipt.superseded_plan_ids = p.supersedes_plan_id.iter().cloned().collect();
        receipt.approval_status = Some(p.status.clone());
        receipt.approval_events = events;
    }
    w.finish_run(
        &run.id,
        "failed",
        Some(json!({"error": code, "message": message})),
        Some(receipt),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// API de aprobación
// ---------------------------------------------------------------------------

/// IDs de eventos `plan.invalidated` del run, en orden de Event Store.
fn invalidated_events(r: &Reader, run: &Run, run_id: &str) -> Vec<String> {
    r.events(&run.workspace_id, Some(&run.branch_id))
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind == "plan.invalidated" && e.run_id.as_deref() == Some(run_id))
        .map(|e| e.event_id)
        .collect()
}

/// Motivos por los que una instantánea ya no representa la configuración
/// revisada. Vacío = vigente.
fn stale_reasons(
    home: &Path,
    r: &Reader,
    run: &Run,
    plan: &RunPlan,
    s: &ExecSnapshot,
) -> Vec<String> {
    let mut out = Vec::new();
    if !s.verify() {
        out.push("hash de la instantánea alterado".into());
    }
    if s.run_id != run.id || s.plan_id != plan.plan_id {
        out.push("la instantánea pertenece a otro run o plan".into());
    }
    if s.branch_revision != run.branch_revision {
        out.push(format!(
            "branch_revision cambió: {} → {}",
            s.branch_revision, run.branch_revision
        ));
    }
    match r.manifest_for_run(&run.id) {
        Ok(m) => {
            if m.manifest_id != s.context_manifest_id || m.content_hash != s.context_manifest_hash {
                out.push("el ContextManifest ya no coincide".into());
            }
        }
        Err(_) => out.push("el ContextManifest ya no existe".into()),
    }
    if s.tool_policy != tool_policy() {
        out.push("la política de herramientas cambió".into());
    }
    if s.adapter_version != env!("CARGO_PKG_VERSION") {
        out.push(format!(
            "la versión de HLI cambió: {} → {}",
            s.adapter_version,
            env!("CARGO_PKG_VERSION")
        ));
    }
    if s.provider_kind != "sim" && s.provider_kind != "fixture" {
        let cfg = providers::load(home);
        match cfg.providers.get(&s.provider_id) {
            Some(e) => {
                if e.kind != s.provider_kind
                    || e.model != s.model
                    || e.base_url != s.endpoint
                    || e.timeout_seconds != s.timeout_secs
                {
                    out.push(format!("el proveedor {} cambió", s.provider_id));
                }
            }
            None => out.push(format!("el proveedor {} ya no existe", s.provider_id)),
        }
    }
    let current_modules = module_versions(r);
    for m in &s.module_versions {
        if !current_modules.iter().any(|c| c == m) {
            out.push(format!(
                "módulo {} {} ya no está instalado",
                m.name, m.version
            ));
        }
    }
    let current_mch = mch_instances(r, &run.workspace_id);
    for m in &s.mch_instances {
        if !current_mch.iter().any(|c| c == m) {
            out.push(format!("instancia MCH {} cambió o desapareció", m.name));
        }
    }
    for h in &s.secret_handles {
        match read_secret(home, &h.id) {
            Ok((_, v)) => {
                if v != h.version {
                    out.push(format!("el secreto {} cambió de versión", h.id));
                }
            }
            Err(_) => out.push(format!("el secreto {} ya no está disponible", h.id)),
        }
    }
    out
}

fn grants_for_workspace(
    w: &Writer,
    plan: &RunPlan,
    actor: &str,
) -> Result<Vec<PermissionGrant>, HliError> {
    let mut out = Vec::new();
    for effect in plan.effectful_requests() {
        out.push(w.record_grant(
            &plan.workspace_id,
            &effect,
            "workspace",
            actor,
            Some(plan.run_id.clone()),
        )?);
    }
    Ok(out)
}

pub fn approve(
    home: &Path,
    w: &Writer,
    r: &Arc<Reader>,
    run_id: &str,
    scope: &str,
    actor: &str,
    expected_plan_id: Option<&str>,
) -> Result<Value, HliError> {
    if !approval_scope_valid(scope) {
        return Err(HliError::invalid(format!("scope inválido: {scope}")));
    }
    let run = r.run(run_id)?;
    let plan = r.plan_for_run(run_id)?;

    // Aprobar exactamente la revisión que el usuario vio.
    if let Some(expected) = expected_plan_id
        && expected != plan.plan_id
    {
        return Err(HliError::new(
            "plan_revision_changed",
            format!(
                "la revisión aprobada ({expected}) no es la vigente ({} revisión {}); revisa run.plan",
                plan.plan_id, plan.revision
            ),
        ));
    }

    // Idempotencia: repetir la aprobación devuelve el estado actual.
    if plan.status == "approved" && run.state != "waiting_approval" {
        return Ok(json!({"run": run, "plan": plan, "idempotent": true}));
    }
    if plan.status == "rejected" || plan.status == "cancelled" {
        return Err(HliError::conflict("el plan fue rechazado o cancelado"));
    }
    if plan.status == "executing" {
        return Ok(json!({"run": run, "plan": plan, "idempotent": true}));
    }
    if run.state != "waiting_approval" || plan.status != "waiting_approval" {
        return Err(HliError::conflict(format!(
            "el run no espera aprobación (run={}, plan={})",
            run.state, plan.status
        )));
    }

    let snapshot: ExecSnapshot = match r.plan_execution_of(&plan.plan_id)? {
        Some(v) => serde_json::from_value(v)
            .map_err(|e| HliError::internal(format!("instantánea inválida: {e}")))?,
        None => {
            return Err(HliError::conflict(
                "la revisión no tiene instantánea de ejecución; usa run.replan",
            ));
        }
    };

    let reasons = stale_reasons(home, r, &run, &plan, &snapshot);
    if !reasons.is_empty() {
        // No se sustituye ni se reescribe nada: la revisión se conserva tal
        // como se mostró y se exige un replan explícito.
        let _ = w.run_event(
            run_id,
            "plan.invalidated",
            json!({
                "run_id": run_id,
                "plan_id": plan.plan_id,
                "revision": plan.revision,
                "content_hash": plan.content_hash,
                "reasons": reasons,
                "required_action": "run.replan"
            }),
        );
        return Err(HliError::new(
            "plan_stale",
            format!(
                "la configuración revisada ya no es válida: {}. La revisión {} se conserva; ejecuta run.replan para crear una nueva.",
                reasons.join("; "),
                plan.revision
            ),
        ));
    }

    if scope == "workspace" {
        grants_for_workspace(w, &plan, actor)?;
    }
    let events_before = r.events(&plan.workspace_id, Some(&plan.branch_id))?.len();
    let plan = w.set_plan_status(
        &plan.plan_id,
        "approved",
        Some(scope.to_string()),
        Some(actor.to_string()),
        None,
    )?;
    // El evento de aprobación es el último emitido por la transición atómica.
    let approval_events: Vec<String> = r
        .events(&plan.workspace_id, Some(&plan.branch_id))?
        .into_iter()
        .skip(events_before)
        .filter(|e| e.kind == "plan.approved")
        .map(|e| e.event_id)
        .collect();

    let run = w.set_run_state(run_id, "running")?;
    let _ = w.set_approval_kind(run_id, None);
    let _ = w.set_plan_status(&plan.plan_id, "executing", None, None, None);

    let w2 = w.clone();
    let r2 = Arc::clone(r);
    let home2 = home.to_path_buf();
    let run_id2 = run_id.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    cancels()
        .lock()
        .unwrap()
        .insert(run_id.to_string(), cancel.clone());
    std::thread::spawn(move || {
        let home3 = home2.clone();
        let _ = execute_prepared_run(&home3, &w2, &r2, &run_id2, cancel);
    });

    Ok(json!({
        "run": run,
        "plan": plan,
        "approval_events": approval_events,
        "scope": scope,
        "approved_by": actor
    }))
}

/// Crea una nueva revisión del plan y una nueva instantánea con las
/// condiciones actuales. El run permanece en `waiting_approval`: nunca se
/// autoaprueba.
pub fn replan(
    home: &Path,
    w: &Writer,
    r: &Reader,
    run_id: &str,
    plan_id: Option<&str>,
    reasons: Vec<String>,
) -> Result<Value, HliError> {
    let run = r.run(run_id)?;
    if run.state != "waiting_approval" {
        return Err(HliError::conflict(format!(
            "sólo se replanifica un run en waiting_approval (estado {})",
            run.state
        )));
    }
    let prev = r.plan_for_run(run_id)?;
    if let Some(expected) = plan_id
        && expected != prev.plan_id
    {
        return Err(HliError::new(
            "plan_revision_changed",
            format!(
                "la revisión indicada ({expected}) no es la vigente ({})",
                prev.plan_id
            ),
        ));
    }
    if prev.status != "waiting_approval" {
        return Err(HliError::conflict(format!(
            "la revisión vigente no espera aprobación ({})",
            prev.status
        )));
    }
    let manifest = r.manifest_for_run(run_id)?;
    let granted = effect_grants(r, &run.workspace_id);

    // Condiciones actuales del proveedor.
    let cfg = providers::load(home);
    let prev_snap: Option<ExecSnapshot> = r
        .plan_execution_of(&prev.plan_id)?
        .and_then(|v| serde_json::from_value(v).ok());
    let (provider_id, provider_kind, model, base_url, timeout_secs, secret_handles) =
        match &prev_snap {
            Some(s) if s.provider_kind == "sim" => (
                s.provider_id.clone(),
                "sim".to_string(),
                "sim-1".to_string(),
                s.endpoint.clone(),
                s.timeout_secs,
                vec![],
            ),
            Some(s) => match cfg.providers.get(&s.provider_id) {
                Some(e) => (
                    s.provider_id.clone(),
                    e.kind.clone(),
                    e.model.clone(),
                    e.base_url.clone(),
                    e.timeout_seconds,
                    e.secret_handle
                        .clone()
                        .map(|id| vec![json!({"id": id})])
                        .unwrap_or_default(),
                ),
                None => {
                    return Err(HliError::conflict(format!(
                        "el proveedor {} ya no existe; configura uno equivalente antes de replanificar",
                        s.provider_id
                    )));
                }
            },
            None => (
                "sim".to_string(),
                "sim".to_string(),
                "sim-1".to_string(),
                String::new(),
                120,
                vec![],
            ),
        };

    let mut plan = hli_agent::build_run_plan(&run, &manifest, &run.prompt, &granted)?;
    plan.revision = prev.revision + 1;
    plan.supersedes_plan_id = Some(prev.plan_id.clone());
    plan.invalidation_reasons = if reasons.is_empty() {
        prev_snap
            .as_ref()
            .map(|s| stale_reasons(home, r, &run, &prev, s))
            .unwrap_or_default()
    } else {
        reasons
    };
    if plan.invalidation_reasons.is_empty() {
        plan.invalidation_reasons = vec!["manual_replan".to_string()];
    }
    plan.status = "waiting_approval".into();

    let (snapshot, problems) = build_snapshot(
        home,
        r,
        &run,
        &plan,
        &provider_id,
        &provider_kind,
        &model,
        &base_url,
        timeout_secs,
        prev_snap
            .as_ref()
            .map(|s| s.sampling.temperature)
            .unwrap_or(0.0),
        prev_snap
            .as_ref()
            .map(|s| s.sampling.max_tokens)
            // 256 tokens no alcanzan ni para un archivo de código corto.
            .unwrap_or(8192),
        prev_snap.as_ref().map(|s| s.budget_tokens).unwrap_or(2048),
        &secret_handles,
        prev_snap
            .as_ref()
            .map(|s| s.candidate_memory_ids.as_slice())
            .unwrap_or(&[]),
        prev_snap.as_ref().and_then(|s| s.sim_delay_ms),
    );
    if !problems.is_empty() {
        return Err(HliError::conflict(format!(
            "no se puede replanificar: {}",
            problems.join("; ")
        )));
    }

    let plan = w.revise_plan(plan, Some(serde_json::to_value(&snapshot)?))?;
    // El run sigue exactamente donde estaba: waiting_approval.
    let run = r.run(run_id)?;
    Ok(json!({
        "run": run,
        "plan": plan,
        "execution": serde_json::to_value(&snapshot)?,
        "previous_plan_id": prev.plan_id,
        "requires_approval": true
    }))
}

fn empty_manifest(run: &Run, manifest_id: &str) -> ContextManifest {
    ContextManifest {
        manifest_id: manifest_id.to_string(),
        policy_version: String::new(),
        workspace_id: run.workspace_id.clone(),
        run_id: run.id.clone(),
        branch_revision: run.branch_revision,
        budget_tokens: 0,
        user_input: hli_core::model::UserInput {
            content: run.prompt.clone(),
            content_hash: hli_core::canon::hash_canonical(&json!(run.prompt)),
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
        estimated_tokens: 0,
        content_hash: String::new(),
        created_at: now_iso(),
    }
}

pub fn reject(
    w: &Writer,
    r: &Reader,
    run_id: &str,
    reason: &str,
    actor: &str,
    expected_plan_id: Option<&str>,
) -> Result<Value, HliError> {
    let run = r.run(run_id)?;
    let plan = r.plan_for_run(run_id)?;
    if let Some(expected) = expected_plan_id
        && expected != plan.plan_id
    {
        return Err(HliError::new(
            "plan_revision_changed",
            format!(
                "la revisión indicada ({expected}) no es la vigente ({})",
                plan.plan_id
            ),
        ));
    }
    if plan.status == "rejected" {
        return Ok(json!({"run": run, "plan": plan, "idempotent": true}));
    }
    if plan.status != "waiting_approval" || run.state != "waiting_approval" {
        return Err(HliError::conflict(format!(
            "el run no espera aprobación (run={}, plan={})",
            run.state, plan.status
        )));
    }
    let plan = w.set_plan_status(
        &plan.plan_id,
        "rejected",
        None,
        Some(actor.to_string()),
        Some(reason.to_string()),
    )?;
    let result = json!({
        "error": "approval_rejected",
        "reason": reason,
        "plan_id": plan.plan_id,
        "plan_hash": plan.content_hash,
        "rejected_effects": plan.effectful_requests()
    });
    // Un run rechazado no tiene resumen de éxito, pero conserva procedencia.
    let manifest = r
        .manifest_for_run(run_id)
        .unwrap_or_else(|_| empty_manifest(&run, &plan.context_manifest_id));
    let snapshot: Option<ExecSnapshot> = r
        .plan_execution_of(&plan.plan_id)?
        .and_then(|v| serde_json::from_value(v).ok());
    let usage = hli_provider::Usage::default();
    let mut receipt = make_receipt(
        "not_executed",
        "not_executed",
        "0",
        &manifest,
        &result,
        &usage,
        &hli_provider::cost_of(None, &usage),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    receipt.plan_id = Some(plan.plan_id.clone());
    receipt.plan_hash = Some(plan.content_hash.clone());
    receipt.plan_origin = Some(plan.origin.clone());
    receipt.plan_revision = Some(plan.revision);
    receipt.approval_status = Some(plan.status.clone());
    receipt.rejected_effects = plan.effectful_requests();
    receipt.superseded_plan_ids = plan.supersedes_plan_id.iter().cloned().collect();
    if let Some(s) = &snapshot {
        receipt.effective_permissions = json!(s.tool_policy);
        receipt.execution_snapshot_hash = Some(s.content_hash.clone());
        receipt.provider_id = Some(s.provider_id.clone());
    }
    let run = w.finish_run(run_id, "cancelled", Some(result), Some(receipt))?;
    Ok(json!({
        "run": run,
        "plan": plan,
        "terminal_reason": "approval_rejected"
    }))
}

pub fn plan_response(r: &Reader, run_id: &str) -> Result<Value, HliError> {
    let run = r.run(run_id)?;
    let plan = r.plan_for_run(run_id)?;
    let execution = r.plan_execution_of(&plan.plan_id)?;
    let revisions = r.plan_revisions(run_id)?;
    Ok(json!({
        "run": run,
        "plan": plan,
        "execution": execution,
        "revisions": revisions
    }))
}

// ---------------------------------------------------------------------------
// Permisos por invocación
// ---------------------------------------------------------------------------

fn permission_decision(req: &ToolPermissionRequest, actor: &str) -> Value {
    json!({
        "request_id": req.request_id,
        "tool": req.tool_name,
        "operation": req.operation,
        "effect": req.effect,
        "target": req.target,
        "arguments_hash": req.arguments_hash,
        "decision": req.decision,
        "scope": req.decision_scope,
        "actor": actor,
        "timestamp": req.decided_at
    })
}

fn permission_receipt(
    manifest: &ContextManifest,
    plan: &RunPlan,
    snapshot: &ExecSnapshot,
    req: &ToolPermissionRequest,
    result: &Value,
    allowed: bool,
    actor: &str,
) -> Receipt {
    let usage = hli_provider::Usage::default();
    let mut receipt = make_receipt(
        &snapshot.provider_id,
        &snapshot.model,
        &snapshot.adapter_version,
        manifest,
        result,
        &usage,
        &hli_provider::cost_of(None, &usage),
        snapshot
            .secret_handles
            .iter()
            .map(|h| json!({"id": h.id, "version": h.version}))
            .collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    attach_plan(&mut receipt, plan, snapshot, vec![]);
    receipt.permission_decisions = vec![permission_decision(req, actor)];
    receipt.permission_request_ids = vec![req.request_id.clone()];
    let tool = req.tool_name.clone();
    if allowed {
        receipt.allowed_tool_invocations = vec![tool];
        if req.decision_scope.as_deref() == Some("once") {
            receipt.consumed_once_grants = vec![req.request_id.clone()];
        }
    } else {
        receipt.denied_tool_invocations = vec![tool];
    }
    receipt.effective_workspace_grants = vec![];
    receipt
}

fn request_context(
    r: &Reader,
    req: &ToolPermissionRequest,
) -> Result<(RunPlan, ContextManifest, ExecSnapshot, Run), HliError> {
    let run = r.run(&req.run_id)?;
    let plan = r.plan(&req.plan_id)?;
    let manifest = r.manifest_for_run(&req.run_id)?;
    let snapshot: ExecSnapshot = match r.plan_execution_of(&plan.plan_id)? {
        Some(v) => serde_json::from_value(v)
            .map_err(|e| HliError::internal(format!("instantánea inválida: {e}")))?,
        None => {
            return Err(HliError::new(
                "execution_snapshot_invalid",
                "la revisión no tiene instantánea",
            ));
        }
    };
    Ok((plan, manifest, snapshot, run))
}

pub fn approve_permission(
    _home: &Path,
    w: &Writer,
    r: &Reader,
    request_id: &str,
    scope: &str,
    actor: &str,
) -> Result<Value, HliError> {
    if !approval_scope_valid(scope) {
        return Err(HliError::new(
            "permission_scope_invalid",
            format!("scope inválido: {scope}"),
        ));
    }
    let req = r.tool_request(request_id)?;
    if req.status == "approved" && req.decision_scope.as_deref() == Some(scope) {
        return Ok(json!({"request": req, "idempotent": true}));
    }
    if req.is_terminal() {
        return Err(HliError::new(
            "permission_request_terminal",
            format!("la solicitud está en {}", req.status),
        ));
    }
    if req.status != "pending" {
        return Err(HliError::new(
            "permission_conflict",
            "la solicitud no está pendiente",
        ));
    }
    let run = r.run(&req.run_id)?;
    if run.state != "waiting_approval" || run.approval_kind.as_deref() != Some("tool") {
        return Err(HliError::new(
            "permission_conflict",
            format!(
                "el run no espera permiso (estado {}, kind {:?})",
                run.state, run.approval_kind
            ),
        ));
    }
    let invocation = r.tool_invocation(request_id)?.ok_or_else(|| {
        HliError::new(
            "tool_invocation_changed",
            "la invocación no puede reconstruirse sin secretos; replanifica la tarea",
        )
    })?;
    let call = ToolCall {
        id: new_id("call"),
        name: req.tool_name.clone(),
        arguments: invocation.clone(),
    };
    if hli_agent::invocation_hash(&call.name, &invocation) != req.arguments_hash {
        return Err(HliError::new(
            "tool_invocation_changed",
            "los argumentos no coinciden con la solicitud aprobada",
        ));
    }
    let (plan, manifest, snapshot, _) = request_context(r, &req)?;
    let req = w.decide_tool_request(request_id, "allow", scope, actor)?;
    let _ = w.set_run_state(&req.run_id, "running")?;
    let _ = w.set_approval_kind(&req.run_id, None);
    let _ = w.run_event(
        &req.run_id,
        "tool.execution_resumed",
        json!({
            "run_id": req.run_id, "request_id": req.request_id,
            "plan_id": req.plan_id, "plan_revision": req.plan_revision,
            "tool": req.tool_name, "operation": req.operation,
            "effect": req.effect, "target": req.target,
            "arguments_hash": req.arguments_hash, "decision_scope": req.decision_scope
        }),
    );
    let root = r.workspace(&req.workspace_id)?.root_path;
    let outcome = hli_agent::execute_tool(Path::new(&root), &call);
    let _ = w.run_event(
        &req.run_id,
        "tool.completed",
        json!({
            "run_id": req.run_id, "tool": outcome.tool, "status": outcome.status,
            "stdout": outcome.stdout, "changed_files": outcome.changed_files,
            "error": outcome.error
        }),
    );
    let state = if outcome.status == "success" {
        "completed"
    } else {
        "failed"
    };
    let result = json!({
        "tool": outcome.tool, "status": outcome.status, "stdout": outcome.stdout,
        "changed_files": outcome.changed_files, "error": outcome.error,
        "request_id": req.request_id
    });
    let plan = w
        .set_plan_status(
            &plan.plan_id,
            if state == "completed" {
                "completed"
            } else {
                "failed"
            },
            plan.approval_scope.clone(),
            plan.approved_by.clone(),
            None,
        )
        .unwrap_or(plan);
    let mut receipt = permission_receipt(&manifest, &plan, &snapshot, &req, &result, true, actor);
    receipt.result_hash = hli_core::canon::hash_canonical(&result);
    let run = w.finish_run(&req.run_id, state, Some(result), Some(receipt))?;
    Ok(json!({"run": run, "request": req}))
}

pub fn deny_permission(
    w: &Writer,
    r: &Reader,
    request_id: &str,
    reason: &str,
    actor: &str,
) -> Result<Value, HliError> {
    let req = r.tool_request(request_id)?;
    if req.status == "denied" {
        return Ok(json!({"request": req, "idempotent": true}));
    }
    if req.is_terminal() {
        return Err(HliError::new(
            "permission_request_terminal",
            format!("la solicitud está en {}", req.status),
        ));
    }
    let (plan, manifest, snapshot, run) = request_context(r, &req)?;
    let req = w.decide_tool_request(request_id, "deny", "once", actor)?;
    let result = json!({
        "error": "tool_denied", "request_id": req.request_id,
        "reason": reason, "tool": req.tool_name
    });
    let plan = w
        .set_plan_status(
            &plan.plan_id,
            "failed",
            None,
            None,
            Some("tool_denied".into()),
        )
        .unwrap_or(plan);
    let mut receipt = permission_receipt(&manifest, &plan, &snapshot, &req, &result, false, actor);
    receipt.result_hash = hli_core::canon::hash_canonical(&result);
    let run = w.finish_run(&run.id, "failed", Some(result), Some(receipt))?;
    Ok(json!({"run": run, "request": req, "denied": true}))
}

pub fn cancel_permission(
    w: &Writer,
    r: &Reader,
    request_id: &str,
    actor: &str,
) -> Result<Value, HliError> {
    let req = w.cancel_tool_request(request_id, actor)?;
    let run = r.run(&req.run_id)?;
    let run = if !run_is_terminal(&run.state) && run.state == "waiting_approval" {
        w.finish_run(
            &run.id,
            "cancelled",
            Some(json!({"cancelled": true, "request_id": req.request_id})),
            None,
        )?
    } else {
        run
    };
    Ok(json!({"run": run, "request": req, "cancelled": true}))
}

pub fn permission_list(
    r: &Reader,
    run_id: Option<&str>,
    status: Option<&str>,
) -> Result<Value, HliError> {
    Ok(serde_json::to_value(
        r.tool_requests(None, run_id, status)?,
    )?)
}

pub fn permission_get(r: &Reader, request_id: &str) -> Result<Value, HliError> {
    let req = r.tool_request(request_id)?;
    Ok(json!({
        "request": req,
        "invocation_available": r.tool_invocation(request_id)?.is_some()
    }))
}

#[allow(dead_code)]
pub fn provider_plan_steps(steps: &[Value]) -> Vec<PlanStep> {
    steps
        .iter()
        .enumerate()
        .map(|(i, v)| {
            PlanStep::new(
                i as i64 + 1,
                v.get("description").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("operation_class")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown"),
                v.get("effect")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown"),
                v.get("tool_name").and_then(|x| x.as_str()),
                v.get("target").and_then(|x| x.as_str()),
            )
        })
        .collect()
}
