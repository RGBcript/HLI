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
