//! Modelo de dominio de HLI 0.1.
//!
//! El sobre canónico de un evento incluye `previous_hash` y excluye `event_hash`.
//! La especificación (§15.8) decía «sin integrity»; eso dejaría el hash anterior
//! fuera de la cadena, así que la implementación sigue la variante segura y la
//! decisión queda registrada en ADR-0002.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
    pub usage: Value,
    pub cost: Value,
    pub result_hash: String,
    pub terminal_event: Option<String>,
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
            "payload_hash": self.payload_hash,
            "estimated_tokens": self.estimated_tokens,
            "created_at": self.created_at
        })
    }

    pub fn seal(&mut self) {
        self.content_hash = hash_canonical(&self.envelope());
    }
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
    pub valid_from: String,
    pub valid_until: Option<String>,
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
}
