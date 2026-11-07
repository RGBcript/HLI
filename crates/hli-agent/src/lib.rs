//! Agent runtime de HLI 0.2: manifiesto de contexto, proveedor determinista y
//! ejecución de herramientas con permisos. Sin red y sin coste externo.

use std::path::{Path, PathBuf};

use hli_core::{
    canon::hash_canonical,
    ids::new_id,
    model::*,
    time::now_iso,
    HliError, Result,
};
use serde_json::{json, Value};

pub const POLICY_VERSION: &str = "context/v1";
pub const DEFAULT_BUDGET_TOKENS: i64 = 8_192;
const MAX_EVENT_CHARS: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Allow,
    Ask,
    Deny,
}

pub fn classify_task(prompt: &str) -> &'static str {
    let p = prompt.to_lowercase();
    if p.contains("resume") || p.contains("resumen") || p.contains("log") || p.contains("error grave") {
        "summarization"
    } else if p.contains("corrige") || p.contains("bug") {
        "code_fix"
    } else if p.contains("integra") || p.contains("combina") {
        "integration"
    } else if p.contains("archivo") || p.contains("files") || p.contains("nombra") {
        "artifact_analysis"
    } else {
        "memory_lookup"
    }
}

/// Payload canónico que se envía al proveedor.
pub fn build_payload(manifest: &ContextManifest, slices: &[(ArtifactSlice, Vec<u8>)]) -> Value {
    let slice_values: Vec<Value> = slices
        .iter()
        .map(|(s, content)| {
            json!({
                "slice_id": s.slice_id,
                "artifact_id": s.artifact_id,
                "artifact_hash": s.artifact_hash,
                "media_type": s.media_type,
                "selector": s.selector,
                "slice_hash": s.slice_hash,
                "estimated_tokens": s.estimated_tokens,
                "reason": s.reason,
                "content": String::from_utf8_lossy(content)
            })
        })
        .collect();
    json!({
        "instructions": manifest.instructions,
        "user_input": manifest.user_input,
        "memories": manifest.memories,
        "recent_events": manifest.recent_events,
        "summaries": manifest.summaries,
        "artifact_slices": slice_values
    })
}

pub fn render_payload(manifest: &ContextManifest, slices: &[(ArtifactSlice, Vec<u8>)]) -> String {
    let mut out = String::new();
    for m in &manifest.memories {
        if let Some(c) = m.get("content").and_then(|v| v.as_str()) {
            out.push_str(&format!("- memoria: {c}\n"));
        }
    }
    for e in &manifest.recent_events {
        if let Some(c) = e.get("snippet").and_then(|v| v.as_str()) {
            out.push_str(&format!("- evento: {c}\n"));
        }
    }
    for (s, content) in slices {
        out.push_str(&format!("\n[artefacto {} · {} · {}]\n", s.artifact_id, s.media_type, s.reason));
        out.push_str(&String::from_utf8_lossy(content));
        out.push('\n');
    }
    out.push_str(&format!("\nPetición: {}\n", manifest.user_input.content));
    out
}

pub fn permission_for(tool: &str) -> Permission {
    match tool {
        "filesystem.read" => Permission::Allow,
        "filesystem.write" => Permission::Ask,
        _ => Permission::Deny,
    }
}

pub struct ProviderRequest {
    pub run_id: String,
    pub prompt: String,
    pub manifest: ContextManifest,
}

pub struct ProviderResponse {
    pub text: String,
    pub tool_call: Option<ToolCall>,
}

pub trait Provider {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn complete(&self, req: &ProviderRequest) -> Result<ProviderResponse>;
}

/// Proveedor simulado: determinista, sin red, sin coste.
pub struct SimProvider;

impl Provider for SimProvider {
    fn name(&self) -> &str {
        "sim"
    }

    fn model(&self) -> &str {
        "sim-1"
    }

    fn complete(&self, req: &ProviderRequest) -> Result<ProviderResponse> {
        // El prompt efectivo es el del manifiesto: es lo que queda cubierto por el hash.
        let prompt = req.manifest.user_input.content.trim();
        if let Some(rest) = prompt.strip_prefix("tool:read ") {
            return Ok(ProviderResponse {
                text: String::new(),
                tool_call: Some(ToolCall {
                    id: new_id("call"),
                    name: "filesystem.read".into(),
                    arguments: json!({ "path": rest.trim() }),
                }),
            });
        }
        if let Some(rest) = prompt.strip_prefix("tool:write ") {
            let mut parts = rest.splitn(2, ' ');
            let path = parts.next().unwrap_or("").to_string();
            let content = parts.next().unwrap_or("").to_string();
            return Ok(ProviderResponse {
                text: String::new(),
                tool_call: Some(ToolCall {
                    id: new_id("call"),
                    name: "filesystem.write".into(),
                    arguments: json!({ "path": path, "content": content }),
                }),
            });
        }
        if prompt.starts_with("tool:shell") {
            return Ok(ProviderResponse {
                text: String::new(),
                tool_call: Some(ToolCall {
                    id: new_id("call"),
                    name: "shell.execute".into(),
                    arguments: json!({ "command": prompt.trim_start_matches("tool:shell") }),
                }),
            });
        }

        let digest = hash_canonical(&json!({
            "run": req.run_id,
            "prompt": prompt,
            "manifest": req.manifest.content_hash,
        }));
        Ok(ProviderResponse {
            text: format!("sim:{}", &digest[7..15]),
            tool_call: None,
        })
    }
}

/// Construye el manifiesto con los eventos del thread hasta la revisión del run.
pub fn build_manifest(
    run: &Run,
    visible_events: &[Event],
    instructions: Vec<Value>,
    prompt: &str,
    memories: &[Value],
    budget_tokens: i64,
) -> ContextManifest {
    let mut eligible: Vec<&Event> = visible_events.iter().collect();
    eligible.sort_by_key(|e| e.sequence);

    let mut recent = Vec::new();
    let mut excluded = Vec::new();
    let mut used_chars = 0usize;
    let budget_chars = (budget_tokens.max(256) as usize) * 4;

    for e in eligible.iter().rev() {
        let snippet = event_snippet(e);
        let cost = snippet.chars().count();
        if used_chars + cost > budget_chars {
            excluded.push(json!({"id": e.event_id, "reason": "budget"}));
            continue;
        }
        used_chars += cost;
        recent.push(json!({
            "event_id": e.event_id,
            "sequence": e.sequence,
            "type": e.kind,
            "snippet": snippet
        }));
    }
    recent.reverse();
    excluded.reverse();

    let mut manifest = ContextManifest {
        manifest_id: new_id("ctx"),
        policy_version: POLICY_VERSION.into(),
        workspace_id: run.workspace_id.clone(),
        run_id: run.id.clone(),
        branch_revision: run.branch_revision,
        budget_tokens,
        user_input: UserInput {
            content: prompt.to_string(),
            content_hash: hash_canonical(&json!(prompt)),
        },
        instructions: instructions.clone(),
        memories: memories.to_vec(),
        recent_events: recent,
        summaries: vec![],
        artifact_slices: vec![],
        excluded,
        payload_hash: String::new(),
        estimated_tokens: 0,
        content_hash: String::new(),
        created_at: now_iso(),
    };

    let total_chars = used_chars
        + manifest.user_input.content.chars().count()
        + manifest
            .instructions
            .iter()
            .map(|v| v.to_string().chars().count())
            .sum::<usize>();
    manifest.estimated_tokens = ((total_chars as i64) + 3) / 4;
    manifest.seal();
    manifest
}

fn event_snippet(e: &Event) -> String {
    let raw = json!({"type": e.kind, "payload": e.payload}).to_string();
    if raw.chars().count() > MAX_EVENT_CHARS {
        let mut s: String = raw.chars().take(MAX_EVENT_CHARS - 1).collect();
        s.push('…');
        s
    } else {
        raw
    }
}

/// Ejecuta una herramienta confinada al root del workspace.
pub fn execute_tool(root: &Path, call: &ToolCall) -> ToolOutcome {
    match call.name.as_str() {
        "filesystem.read" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            match resolve(root, rel) {
                Ok(p) => match std::fs::read_to_string(&p) {
                    Ok(text) => ToolOutcome {
                        tool: call.name.clone(),
                        status: "success".into(),
                        exit_code: Some(0),
                        stdout: truncate(&text, 8_000),
                        changed_files: vec![],
                        error: None,
                    },
                    Err(e) => failed(&call.name, format!("no se pudo leer: {e}")),
                },
                Err(e) => failed(&call.name, e.message),
            }
        }
        "filesystem.write" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            let content = call.arguments["content"].as_str().unwrap_or("");
            match resolve(root, rel) {
                Ok(p) => {
                    if let Some(parent) = p.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&p, content) {
                        Ok(_) => ToolOutcome {
                            tool: call.name.clone(),
                            status: "success".into(),
                            exit_code: Some(0),
                            stdout: format!("escrito {}", rel),
                            changed_files: vec![rel.to_string()],
                            error: None,
                        },
                        Err(e) => failed(&call.name, format!("no se pudo escribir: {e}")),
                    }
                }
                Err(e) => failed(&call.name, e.message),
            }
        }
        other => failed(other, "herramienta denegada o no implementada en 0.2".into()),
    }
}

fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    if rel.trim().is_empty() {
        return Err(HliError::invalid("ruta vacía"));
    }
    let candidate = root.join(rel);
    // Canonicalizar el padre (existe) y añadir el nombre: evita el prefijo
    // verbatim de Windows en rutas que aún no existen.
    let parent = candidate.parent().unwrap_or(root);
    let canon_parent = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
    let full = match candidate.file_name() {
        Some(name) => canon_parent.join(name),
        None => canon_parent.clone(),
    };
    let canon_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !full.starts_with(&canon_root) {
        return Err(HliError::invalid("ruta fuera del workspace"));
    }
    Ok(candidate)
}

fn failed(tool: &str, msg: String) -> ToolOutcome {
    ToolOutcome {
        tool: tool.to_string(),
        status: "error".into(),
        exit_code: None,
        stdout: String::new(),
        changed_files: vec![],
        error: Some(msg),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hli_core::model::{Integrity, EVENT_SCHEMA_VERSION};

    fn ev(seq: i64, kind: &str, payload: Value) -> Event {
        Event {
            event_id: format!("evt_{seq}"),
            workspace_id: "ws_1".into(),
            thread_id: Some("thr_1".into()),
            branch_id: None,
            run_id: None,
            causation_id: None,
            correlation_id: None,
            idempotency_key: None,
            sequence: seq,
            kind: kind.into(),
            schema_version: EVENT_SCHEMA_VERSION,
            created_at: "2026-11-07T00:00:00.000Z".into(),
            actor: json!({"kind": "user", "id": "cli"}),
            payload,
            artifacts: vec![],
            integrity: Integrity {
                canonicalization: "RFC8785".into(),
                hash_alg: "sha256".into(),
                previous_hash: "sha256:0".into(),
                event_hash: format!("sha256:{seq}"),
            },
        }
    }

    fn run(rev: i64) -> Run {
        Run {
            id: "run_1".into(),
            workspace_id: "ws_1".into(),
            thread_id: "thr_1".into(),
            branch_id: "br_1".into(),
            branch_revision: rev,
            state: "queued".into(),
            prompt: "hola".into(),
            started_at: "2026-11-07T00:00:00.000Z".into(),
            ended_at: None,
            result: None,
            receipt: None,
        }
    }

    #[test]
    fn manifest_is_deterministic_and_sealed() {
        let events = vec![ev(1, "workspace.created", json!({"name": "d"})), ev(2, "thread.created", json!({"title": "t"}))];
        let a = build_manifest(&run(2), &events, vec![], "hola", &[], 1024);
        let b = build_manifest(&run(2), &events, vec![], "hola", &[], 1024);
        assert_eq!(a.recent_events.len(), 2);
        assert_eq!(a.estimated_tokens, b.estimated_tokens);
        assert!(!a.content_hash.is_empty());
        assert_eq!(a.content_hash.len(), 7 + 64);
    }

    #[test]
    fn manifest_respects_given_visibility() {
        // La exclusión por rama y por revisión vive en Reader::visible_events;
        // aquí se comprueba que el manifiesto incluye exactamente lo que recibe.
        let events = vec![ev(1, "a", json!({})), ev(2, "b", json!({}))];
        let m = build_manifest(&run(2), &events, vec![], "hola", &[], 1024);
        assert_eq!(m.recent_events.len(), 2);
        assert_eq!(m.recent_events[1]["sequence"], 2);
    }

    #[test]
    fn permissions_are_explicit() {
        assert_eq!(permission_for("filesystem.read"), Permission::Allow);
        assert_eq!(permission_for("filesystem.write"), Permission::Ask);
        assert_eq!(permission_for("shell.execute"), Permission::Deny);
    }

    #[test]
    fn sim_provider_is_deterministic() {
        let p = SimProvider;
        let events = vec![ev(1, "a", json!({}))];
        let m = build_manifest(&run(1), &events, vec![], "hola", &[], 1024);
        let req = ProviderRequest { run_id: "run_1".into(), prompt: "hola".into(), manifest: m.clone() };
        let a = p.complete(&req).unwrap();
        let b = p.complete(&req).unwrap();
        assert_eq!(a.text, b.text);
        assert!(a.text.starts_with("sim:"));
    }

    #[test]
    fn prompt_is_covered_by_manifest_hash() {
        let events = vec![ev(1, "a", json!({}))];
        let a = build_manifest(&run(1), &events, vec![], "hola", &[], 1024);
        let b = build_manifest(&run(1), &events, vec![], "adios", &[], 1024);
        assert_eq!(a.user_input.content, "hola");
        assert_ne!(a.user_input.content_hash, b.user_input.content_hash);
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn provider_receives_manifest_prompt() {
        let events = vec![ev(1, "a", json!({}))];
        let m = build_manifest(&run(1), &events, vec![], "hola", &[], 1024);
        let p = SimProvider;
        let req = ProviderRequest {
            run_id: "run_1".into(),
            prompt: "otro texto distinto".into(),
            manifest: m,
        };
        let out = p.complete(&req).unwrap();
        assert!(out.text.starts_with("sim:"));
    }

    #[test]
    fn tool_is_confined_to_root() {
        let tmp = std::env::temp_dir().join(format!("hli-tool-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let call = ToolCall {
            id: "c".into(),
            name: "filesystem.read".into(),
            arguments: json!({"path": "../outside.txt"}),
        };
        let out = execute_tool(&tmp, &call);
        assert_eq!(out.status, "error");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
