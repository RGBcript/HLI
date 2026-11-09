//! Modelo de dominio de HLI 0.1.
//!
//! El sobre canónico de un evento incluye `previous_hash` y excluye `event_hash`.
//! La especificación (§15.8) decía «sin integrity»; eso dejaría el hash anterior
//! fuera de la cadena, así que la implementación sigue la variante segura y la
//! decisión queda registrada en ADR-0002.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canon::hash_canonical;

pub const EVENT_SCHEMA_VERSION: i64 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub created_at: String,
    pub archived_at: Option<String>,
    pub config: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Thread {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Branch {
    pub id: String,
    pub thread_id: String,
    pub name: String,
    pub parent_branch_id: Option<String>,
    pub fork_event_id: Option<String>,
    pub head_event_id: Option<String>,
    pub head_sequence: i64,
    pub snapshot_id: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArtifactRef {
    pub sha256: String,
    pub mime: String,
    pub bytes: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Integrity {
    pub canonicalization: String,
    pub hash_alg: String,
    pub previous_hash: String,
    pub event_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub event_id: String,
    pub workspace_id: String,
    pub thread_id: Option<String>,
    pub branch_id: Option<String>,
    pub run_id: Option<String>,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub sequence: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub schema_version: i64,
    pub created_at: String,
    pub actor: Value,
    pub payload: Value,
    pub artifacts: Vec<ArtifactRef>,
    pub integrity: Integrity,
}

impl Event {
    /// Sobre canónico: todo el evento salvo `event_hash`.
    pub fn envelope(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "workspace_id": self.workspace_id,
            "thread_id": self.thread_id,
            "branch_id": self.branch_id,
            "run_id": self.run_id,
            "causation_id": self.causation_id,
            "correlation_id": self.correlation_id,
            "idempotency_key": self.idempotency_key,
            "sequence": self.sequence,
            "type": self.kind,
            "schema_version": self.schema_version,
            "created_at": self.created_at,
            "actor": self.actor,
            "payload": self.payload,
            "artifacts": self.artifacts,
            "integrity": {
                "canonicalization": self.integrity.canonicalization,
                "hash_alg": self.integrity.hash_alg,
                "previous_hash": self.integrity.previous_hash,
            }
        })
    }

    pub fn compute_hash(&self) -> String {
        hash_canonical(&self.envelope())
    }

    pub fn seal(&mut self) {
        self.integrity.event_hash = self.compute_hash();
    }

    pub fn verify(&self) -> bool {
        self.integrity.event_hash == self.compute_hash()
    }
}

/// Vista mínima para respuestas de CLI.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BranchState {
    pub branch: Branch,
    pub thread: Thread,
    pub workspace: Workspace,
    pub head_event: Option<Event>,
    pub event_count: i64,
    pub chain_head: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerifyReport {
    pub workspace_id: String,
    pub checked: i64,
    pub ok: bool,
    pub chain_head: Option<String>,
    pub first_bad: Option<BadEvent>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BadEvent {
    pub event_id: String,
    pub sequence: i64,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// 0.2 — Runs, manifiestos y receipts
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Run {
    pub id: String,
    pub workspace_id: String,
    pub thread_id: String,
    pub branch_id: String,
    pub branch_revision: i64,
    pub state: String,
    /// Distingue una espera de plan (`plan`) de una espera de permiso de tool
    /// (`tool`); `None` fuera de `waiting_approval`.
    #[serde(default)]
    pub approval_kind: Option<String>,
    pub prompt: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub result: Option<Value>,
    pub receipt: Option<Receipt>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Receipt {
    pub provider: String,
    pub model: String,
    pub adapter_version: String,
    pub context_manifest: String,
    pub tool_schemas: Vec<String>,
    pub effective_permissions: Value,
    pub nonsecret_environment: Value,
    pub secret_handles: Vec<Value>,
    pub input_artifacts: Vec<String>,
    pub module_versions: Vec<String>,
    pub sampling: Value,
    pub result_artifacts: Vec<String>,
    pub sandbox: Value,
    pub used_memory_ids: Vec<String>,
    pub used_slice_ids: Vec<String>,
    pub used_event_ids: Vec<String>,
    #[serde(default)]
    pub used_explicit_reference_ids: Vec<String>,
    #[serde(default)]
    pub rejected_explicit_reference_ids: Vec<String>,
    pub candidate_memory_ids: Vec<String>,
    pub rejected_memory_ids: Vec<String>,
    pub rejection_reasons: Value,
    pub usage: Value,
    pub cost: Value,
    pub result_hash: String,
    pub terminal_event: Option<String>,
    // Procedencia del plan (0.9). Opcionales para que los receipts históricos
    // sigan siendo legibles sin migración.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approved_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_plan_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidation_events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_decisions: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_request_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tool_invocations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_tool_invocations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed_once_grants: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective_workspace_grants: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContextManifest {
    pub manifest_id: String,
    pub policy_version: String,
    pub workspace_id: String,
    pub run_id: String,
    pub branch_revision: i64,
    pub budget_tokens: i64,
    pub user_input: UserInput,
    pub instructions: Vec<Value>,
    pub memories: Vec<Value>,
    pub recent_events: Vec<Value>,
    pub visible_event_ids: Vec<String>,
    pub summaries: Vec<Value>,
    pub artifact_slices: Vec<Value>,
    pub excluded: Vec<Value>,
    #[serde(default)]
    pub explicit_references: Vec<ExplicitReferenceEntry>,
    pub payload_hash: String,
    pub estimated_tokens: i64,
    pub content_hash: String,
    pub created_at: String,
}

impl ContextManifest {
    /// Sobre canónico: todos los campos menos `content_hash`.
    pub fn envelope(&self) -> Value {
        json!({
            "manifest_id": self.manifest_id,
            "policy_version": self.policy_version,
            "workspace_id": self.workspace_id,
            "run_id": self.run_id,
            "branch_revision": self.branch_revision,
            "budget_tokens": self.budget_tokens,
            "user_input": self.user_input,
            "instructions": self.instructions,
            "memories": self.memories,
            "recent_events": self.recent_events,
            "visible_event_ids": self.visible_event_ids,
            "summaries": self.summaries,
            "artifact_slices": self.artifact_slices,
            "excluded": self.excluded,
            "explicit_references": self.explicit_references,
            "payload_hash": self.payload_hash,
            "estimated_tokens": self.estimated_tokens,
            "created_at": self.created_at
        })
    }

    pub fn seal(&mut self) {
        self.content_hash = hash_canonical(&self.envelope());
    }
}

/// Referencia explícita tal como la envía el cliente.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExplicitReference {
    pub reference_type: String,
    pub stable_id: String,
    #[serde(default)]
    pub display_text: String,
    #[serde(default)]
    pub selected_at: String,
    #[serde(default)]
    pub selection_reason: String,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub revision: Option<i64>,
    #[serde(default)]
    pub source_scope: Option<String>,
}

/// Entrada del manifiesto: referencia + resultado de validación y materialización.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExplicitReferenceEntry {
    pub reference_type: String,
    pub stable_id: String,
    pub display_text: String,
    pub selected_at: String,
    pub selection_reason: String,
    pub validation_state: String,
    pub content_hash: Option<String>,
    pub revision: Option<i64>,
    pub materialization_kind: String,
    pub materialized_artifact_id: Option<String>,
    pub slice_ids: Vec<String>,
    pub estimated_tokens: i64,
    pub included: bool,
    pub exclusion_reason: Option<String>,
    /// Contenido materializado (solo texto y dentro de presupuesto).
    #[serde(default)]
    pub materialized_text: Option<String>,
    /// Metadatos por tipo (size, media_type, status, receipt hash, thread_id...).
    #[serde(default)]
    pub detail: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArtifactSlice {
    pub slice_id: String,
    pub artifact_id: String,
    pub artifact_hash: String,
    pub media_type: String,
    pub selector: Value,
    pub slice_hash: String,
    pub estimated_tokens: i64,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserInput {
    pub content: String,
    pub content_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Memory {
    pub id: String,
    pub workspace_id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub content: String,
    pub observed_at: String,
    pub valid_from: String,
    pub valid_until: Option<String>,
    pub superseded_at: Option<String>,
    pub source_authority: String,
    pub subject: Option<String>,
    pub relation: Option<String>,
    pub object: Option<String>,
    pub supersedes: Vec<String>,
    pub contradicts: Vec<String>,
    pub sensitivity: String,
    pub authorship: Value,
    pub source_events: Vec<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolOutcome {
    pub tool: String,
    pub status: String,
    pub exit_code: Option<i64>,
    pub stdout: String,
    pub changed_files: Vec<String>,
    pub error: Option<String>,
}

/// Tabla explícita de transiciones de estado de un run.
pub fn run_transition_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("queued", "planning")
            | ("queued", "running")
            | ("queued", "completed")
            | ("queued", "cancelled")
            | ("queued", "failed")
            | ("queued", "timed_out")
            | ("queued", "interrupted")
            | ("queued", "waiting_approval")
            | ("queued", "paused")
            | ("planning", "waiting_approval")
            | ("planning", "running")
            | ("planning", "failed")
            | ("planning", "cancelled")
            | ("planning", "interrupted")
            | ("running", "waiting_approval")
            | ("running", "paused")
            | ("running", "cancelling")
            | ("running", "completed")
            | ("running", "failed")
            | ("running", "timed_out")
            | ("running", "interrupted")
            | ("cancelling", "cancelled")
            | ("cancelling", "failed")
            | ("waiting_approval", "running")
            | ("waiting_approval", "paused")
            | ("waiting_approval", "cancelled")
            | ("waiting_approval", "interrupted")
            | ("waiting_approval", "completed")
            | ("waiting_approval", "failed")
            | ("waiting_approval", "timed_out")
            | ("paused", "running")
            | ("paused", "cancelled")
            | ("paused", "interrupted")
            | ("paused", "completed")
            | ("paused", "failed")
            | ("paused", "timed_out")
    )
}

pub fn run_is_terminal(state: &str) -> bool {
    matches!(
        state,
        "completed" | "failed" | "cancelled" | "timed_out" | "interrupted"
    )
}

// ---------------------------------------------------------------------------
// 0.9 — Plan durable, aprobación y grants
// ---------------------------------------------------------------------------

/// Versión del documento de plan (independiente del esquema de eventos).
pub const PLAN_SCHEMA_VERSION: i64 = 1;

/// Efectos que un plan puede declarar. `unknown` es el cierre seguro: si no se
/// puede clasificar, se exige aprobación explícita.
pub const PLAN_EFFECTS: [&str; 6] = [
    "read",
    "write",
    "network",
    "external_service",
    "destructive",
    "unknown",
];

/// Estados del plan.
pub const PLAN_STATUSES: [&str; 8] = [
    "draft",
    "waiting_approval",
    "approved",
    "rejected",
    "cancelled",
    "executing",
    "completed",
    "failed",
];

/// Scopes de aprobación admitidos.
pub const APPROVAL_SCOPES: [&str; 3] = ["once", "run", "workspace"];

pub fn plan_effect_is_known(effect: &str) -> bool {
    PLAN_EFFECTS.contains(&effect)
}

/// Sólo `read` avanza sin aprobación; el resto (incluido `unknown`) la exige.
pub fn effect_requires_approval(effect: &str) -> bool {
    effect != "read"
}

pub fn approval_scope_valid(scope: &str) -> bool {
    APPROVAL_SCOPES.contains(&scope)
}

pub fn plan_status_valid(status: &str) -> bool {
    PLAN_STATUSES.contains(&status)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanStep {
    pub step_id: String,
    pub order: i64,
    pub description: String,
    pub operation_class: String,
    pub effect: String,
    pub tool_name: Option<String>,
    pub target: Option<String>,
    pub requires_approval: bool,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    pub plan_id: String,
    /// Revisión del plan para este run: 1, 2, 3… Cada replan crea una nueva
    /// revisión y conserva las anteriores.
    pub revision: i64,
    /// Revisión a la que sustituye, si esta nació de un replan.
    pub supersedes_plan_id: Option<String>,
    /// Motivos por los que la revisión anterior dejó de ser válida.
    pub invalidation_reasons: Vec<String>,
    pub run_id: String,
    pub workspace_id: String,
    pub thread_id: String,
    pub branch_id: String,
    pub branch_revision: i64,
    pub context_manifest_id: String,
    pub origin: String,
    pub status: String,
    pub summary: String,
    pub steps: Vec<PlanStep>,
    pub requested_effects: Vec<String>,
    pub content_hash: String,
    pub created_at: String,
    pub approved_at: Option<String>,
    pub rejected_at: Option<String>,
    pub approved_by: Option<String>,
    pub approval_scope: Option<String>,
    pub rejection_reason: Option<String>,
}

impl RunPlan {
    /// Sobre canónico del **contenido revisado**: pasos, efectos y vínculos.
    /// Quedan fuera el estado y los metadatos de aprobación para que el hash
    /// siga siendo el mismo antes y después de aprobar (es el hash que firma
    /// la decisión y el que viaja en el receipt).
    pub fn envelope(&self) -> Value {
        json!({
            "schema_version": PLAN_SCHEMA_VERSION,
            "run_id": self.run_id,
            "workspace_id": self.workspace_id,
            "thread_id": self.thread_id,
            "branch_id": self.branch_id,
            "branch_revision": self.branch_revision,
            "context_manifest_id": self.context_manifest_id,
            "origin": self.origin,
            "revision": self.revision,
            "supersedes_plan_id": self.supersedes_plan_id,
            "invalidation_reasons": self.invalidation_reasons,
            "summary": self.summary,
            "steps": self.steps.iter().map(PlanStep::canonical).collect::<Vec<_>>(),
            "requested_effects": self.requested_effects
        })
    }

    pub fn seal(&mut self) {
        self.content_hash = hash_canonical(&self.envelope());
    }

    pub fn verify(&self) -> bool {
        self.content_hash == hash_canonical(&self.envelope())
    }

    pub fn requires_approval(&self) -> bool {
        self.steps.iter().any(|s| s.requires_approval)
    }

    pub fn effectful_requests(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.steps {
            if s.requires_approval && !out.contains(&s.effect) {
                out.push(s.effect.clone());
            }
        }
        out
    }
}

impl PlanStep {
    pub fn new(
        order: i64,
        description: &str,
        operation_class: &str,
        effect: &str,
        tool_name: Option<&str>,
        target: Option<&str>,
    ) -> Self {
        Self {
            step_id: crate::ids::new_id("step"),
            order,
            description: description.to_string(),
            operation_class: operation_class.to_string(),
            effect: effect.to_string(),
            tool_name: tool_name.map(str::to_string),
            target: target.map(str::to_string),
            requires_approval: effect_requires_approval(effect),
            status: "pending".into(),
        }
    }

    /// Campos que entran en el hash del plan: los `step_id` son identidad de
    /// presentación y no deben alterar el hash de un plan equivalente.
    pub fn canonical(&self) -> Value {
        json!({
            "order": self.order,
            "description": self.description,
            "operation_class": self.operation_class,
            "effect": self.effect,
            "tool_name": self.tool_name,
            "target": self.target,
            "requires_approval": self.requires_approval,
            "status": self.status
        })
    }
}

/// Grant persistente del Permission Engine para un workspace.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PermissionGrant {
    pub grant_id: String,
    pub workspace_id: String,
    pub effect: String,
    pub scope: String,
    pub created_at: String,
    pub created_by: String,
    pub run_id: Option<String>,
}


// ---------------------------------------------------------------------------
// Permisos por invocación (0.10)
// ---------------------------------------------------------------------------

pub const TOOL_REQUEST_STATUSES: [&str; 6] = [
    "pending",
    "approved",
    "denied",
    "cancelled",
    "expired",
    "superseded",
];

pub fn tool_request_status_valid(status: &str) -> bool {
    TOOL_REQUEST_STATUSES.contains(&status)
}

/// Solicitud durable para autorizar **una invocación concreta**. No contiene
/// argumentos completos: sólo su hash, un resumen sin secretos y el target
/// normalizado.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolPermissionRequest {
    pub request_id: String,
    pub run_id: String,
    pub plan_id: String,
    pub plan_revision: i64,
    pub workspace_id: String,
    pub thread_id: String,
    pub branch_id: String,
    pub tool_name: String,
    pub operation: String,
    pub effect: String,
    pub target: Option<String>,
    pub arguments_hash: String,
    pub arguments_summary: String,
    pub reason: String,
    pub requested_scope: Option<String>,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
    pub decision: Option<String>,
    pub decision_scope: Option<String>,
    pub content_hash: String,
}

impl ToolPermissionRequest {
    pub fn envelope(&self) -> Value {
        json!({
            "request_id": self.request_id,
            "run_id": self.run_id,
            "plan_id": self.plan_id,
            "plan_revision": self.plan_revision,
            "workspace_id": self.workspace_id,
            "thread_id": self.thread_id,
            "branch_id": self.branch_id,
            "tool_name": self.tool_name,
            "operation": self.operation,
            "effect": self.effect,
            "target": self.target,
            "arguments_hash": self.arguments_hash,
            "arguments_summary": self.arguments_summary,
            "reason": self.reason,
            "requested_scope": self.requested_scope,
            "status": self.status,
            "created_at": self.created_at,
            "decided_at": self.decided_at,
            "decided_by": self.decided_by,
            "decision": self.decision,
            "decision_scope": self.decision_scope
        })
    }

    pub fn seal(&mut self) {
        self.content_hash = hash_canonical(&self.envelope());
    }

    pub fn verify(&self) -> bool {
        self.content_hash == hash_canonical(&self.envelope())
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "approved" | "denied" | "cancelled" | "expired" | "superseded")
    }
}

/// Grant por invocación: `once` no se persiste (se consume al decidir);
/// `run` dura hasta el estado terminal del run; `workspace` sobrevive.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolGrant {
    pub grant_id: String,
    pub workspace_id: String,
    pub run_id: Option<String>,
    pub effect: String,
    pub tool_name: String,
    pub target: Option<String>,
    pub scope: String,
    pub created_at: String,
    pub created_by: String,
}

/// Transiciones válidas del documento de plan. Un plan terminal no revive.
pub fn plan_transition_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("draft", "waiting_approval")
            | ("draft", "approved")
            | ("draft", "executing")
            | ("draft", "cancelled")
            | ("draft", "failed")
            | ("waiting_approval", "approved")
            | ("waiting_approval", "executing")
            | ("waiting_approval", "rejected")
            | ("waiting_approval", "cancelled")
            | ("waiting_approval", "failed")
            | ("approved", "executing")
            | ("approved", "cancelled")
            | ("approved", "failed")
            | ("executing", "completed")
            | ("executing", "failed")
            | ("executing", "waiting_approval")
            | ("executing", "cancelled")
    )
}

pub fn plan_is_terminal(status: &str) -> bool {
    matches!(status, "rejected" | "cancelled" | "completed" | "failed")
}

// ---------------------------------------------------------------------------
// Instantánea durable de ejecución (separada del RunPlan público)
// ---------------------------------------------------------------------------

/// Versión del documento de instantánea.
pub const EXEC_SNAPSHOT_VERSION: i64 = 1;

/// Referencia versionada a un secreto. Nunca contiene el valor.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SecretHandleRef {
    pub id: String,
    pub version: String,
}

/// Módulo y versión que participaron en la planificación.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleVersion {
    pub name: String,
    pub version: String,
}

/// Instancia MCH relevante para la ejecución revisada.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecMchInstance {
    pub name: String,
    pub profile: String,
    pub module_name: String,
    pub version: String,
}

/// Parámetros de muestreo fijados al planificar.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExecSampling {
    pub temperature: f64,
    pub max_tokens: i64,
    pub seed: i64,
}

/// Instantánea privada de las condiciones revisadas. Al aprobar se verifica su
/// hash y su vigencia; si algo cambió, la aprobación se invalida en lugar de
/// sustituir la configuración en silencio.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecSnapshot {
    pub snapshot_id: String,
    pub snapshot_version: i64,
    pub run_id: String,
    pub plan_id: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub endpoint: String,
    pub model: String,
    pub adapter_version: String,
    pub sampling: ExecSampling,
    pub timeout_secs: u64,
    pub budget_tokens: i64,
    pub context_manifest_id: String,
    pub context_manifest_hash: String,
    pub branch_revision: i64,
    /// tool → allow|ask|deny, ordenado para un hash estable.
    pub tool_policy: std::collections::BTreeMap<String, String>,
    pub module_versions: Vec<ModuleVersion>,
    pub mch_instances: Vec<ExecMchInstance>,
    pub secret_handles: Vec<SecretHandleRef>,
    /// Candidatos de memoria que el retrieval propuso al planificar; permite
    /// que el receipt conserve la procedencia tras la aprobación.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_memory_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sim_delay_ms: Option<u64>,
    pub created_at: String,
    pub content_hash: String,
}

impl ExecSnapshot {
    /// Sobre canónico: todo el documento salvo `content_hash`. No incluye
    /// valores de secreto, sólo identificadores y versiones.
    pub fn envelope(&self) -> Value {
        json!({
            "snapshot_version": self.snapshot_version,
            "run_id": self.run_id,
            "plan_id": self.plan_id,
            "provider_id": self.provider_id,
            "provider_kind": self.provider_kind,
            "endpoint": self.endpoint,
            "model": self.model,
            "adapter_version": self.adapter_version,
            "sampling": self.sampling,
            "timeout_secs": self.timeout_secs,
            "budget_tokens": self.budget_tokens,
            "context_manifest_id": self.context_manifest_id,
            "context_manifest_hash": self.context_manifest_hash,
            "branch_revision": self.branch_revision,
            "tool_policy": self.tool_policy,
            "module_versions": self.module_versions,
            "mch_instances": self.mch_instances,
            "secret_handles": self.secret_handles,
            "candidate_memory_ids": self.candidate_memory_ids,
            "created_at": self.created_at
        })
    }

    pub fn seal(&mut self) {
        self.content_hash = hash_canonical(&self.envelope());
    }

    pub fn verify(&self) -> bool {
        self.content_hash == hash_canonical(&self.envelope())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Event {
        Event {
            event_id: "evt_1".into(),
            workspace_id: "ws_1".into(),
            thread_id: Some("thr_1".into()),
            branch_id: Some("br_1".into()),
            run_id: None,
            causation_id: None,
            correlation_id: None,
            idempotency_key: Some("idem_1".into()),
            sequence: 1,
            kind: "workspace.created".into(),
            schema_version: EVENT_SCHEMA_VERSION,
            created_at: "2026-11-07T00:00:00.000Z".into(),
            actor: json!({"kind": "user", "id": "cli"}),
            payload: json!({"name": "demo"}),
            artifacts: vec![],
            integrity: Integrity {
                canonicalization: "RFC8785".into(),
                hash_alg: "sha256".into(),
                previous_hash: crate::canon::zero_hash(),
                event_hash: String::new(),
            },
        }
    }

    #[test]
    fn seal_then_verify() {
        let mut e = sample();
        e.seal();
        assert!(e.verify());
        assert_eq!(e.integrity.event_hash.len(), 7 + 64);
    }

    #[test]
    fn tampering_payload_breaks_hash() {
        let mut e = sample();
        e.seal();
        e.payload = json!({"name": "otro"});
        assert!(!e.verify());
    }

    #[test]
    fn previous_hash_is_covered() {
        let mut e = sample();
        e.seal();
        let before = e.integrity.event_hash.clone();
        e.integrity.previous_hash = "sha256:abc".into();
        assert_ne!(e.compute_hash(), before);
    }

    fn plan() -> RunPlan {
        RunPlan {
            plan_id: "plan_1".into(),
            revision: 1,
            supersedes_plan_id: None,
            invalidation_reasons: vec![],
            run_id: "run_1".into(),
            workspace_id: "ws_1".into(),
            thread_id: "thr_1".into(),
            branch_id: "br_1".into(),
            branch_revision: 4,
            context_manifest_id: "ctx_1".into(),
            origin: "runtime_generated".into(),
            status: "waiting_approval".into(),
            summary: "plan".into(),
            steps: vec![PlanStep::new(
                1,
                "read",
                "filesystem.read",
                "read",
                Some("filesystem.read"),
                None,
            )],
            requested_effects: vec!["read".into()],
            content_hash: String::new(),
            created_at: "2026-11-09T00:00:00.000Z".into(),
            approved_at: None,
            rejected_at: None,
            approved_by: None,
            approval_scope: None,
            rejection_reason: None,
        }
    }

    #[test]
    fn plan_hash_is_canonical_and_sensitive_to_steps() {
        let mut a = plan();
        let mut b = plan();
        a.seal();
        b.seal();
        assert_eq!(a.content_hash, b.content_hash);
        assert!(a.verify());
        b.steps[0].description = "otra cosa".into();
        b.seal();
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn plan_hash_ignores_identifier_timestamps() {
        let mut a = plan();
        let mut b = plan();
        a.seal();
        b.seal();
        // El hash cubre el contenido operativo, no la identidad del plan.
        assert_eq!(a.content_hash, b.content_hash);
        assert_ne!(a.plan_id, "otro".to_string());
    }

    #[test]
    fn effect_policy_is_explicit() {
        assert!(!effect_requires_approval("read"));
        for e in [
            "write",
            "network",
            "external_service",
            "destructive",
            "unknown",
        ] {
            assert!(effect_requires_approval(e));
        }
        assert!(approval_scope_valid("once"));
        assert!(!approval_scope_valid("global"));
    }

    #[test]
    fn terminal_plans_and_runs_are_terminal() {
        assert!(plan_is_terminal("rejected"));
        assert!(!plan_transition_allowed("rejected", "approved"));
        assert!(!plan_transition_allowed("completed", "executing"));
        assert!(run_transition_allowed("queued", "planning"));
        assert!(run_transition_allowed("planning", "waiting_approval"));
        assert!(run_transition_allowed("waiting_approval", "running"));
        assert!(!run_transition_allowed("completed", "planning"));
    }
}
