//! Política de permisos por invocación.
//!
//! Resuelve una invocación concreta (tool + efecto + target + argumentos) sin
//! tocar el plan: la aprobación del plan autoriza el conjunto de efectos, no
//! cada llamada.

use hli_core::{
    canon::hash_canonical,
    model::{ToolGrant, effect_requires_approval},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolDecision {
    Allow,
    Ask,
    Deny,
}

/// `shell.execute` está denegado incondicionalmente en HLI 1.0.
pub const ALWAYS_DENIED: &str = "shell.execute";

fn target_matches(grant: &ToolGrant, target: Option<&str>) -> bool {
    match grant.target.as_deref() {
        None => true,
        Some(t) => Some(t) == target,
    }
}

/// Resuelve la política efectiva. `deny` explícito gana; después los grants
/// válidos (workspace o run); sin regla aplicable, `ask`. `read` avanza por
/// política base.
pub fn decide(
    tool: &str,
    effect: &str,
    target: Option<&str>,
    run_id: &str,
    grants: &[ToolGrant],
) -> ToolDecision {
    if tool == ALWAYS_DENIED {
        return ToolDecision::Deny;
    }
    let allowed = grants.iter().any(|g| {
        g.tool_name == tool
            && g.effect == effect
            && target_matches(g, target)
            && match g.scope.as_str() {
                "workspace" => true,
                "run" => g.run_id.as_deref() == Some(run_id),
                _ => false,
            }
    });
    if allowed {
        return ToolDecision::Allow;
    }
    if effect_requires_approval(effect) {
        ToolDecision::Ask
    } else {
        ToolDecision::Allow
    }
}

/// Target normalizado por tool (rutas relativas al workspace, sin `.`/`..`).
pub fn normalize_target(tool: &str, args: &Value) -> Option<String> {
    if !tool.starts_with("filesystem.") {
        return None;
    }
    let raw = args["path"].as_str()?;
    let mut parts: Vec<&str> = Vec::new();
    let normalized = raw.replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// Hash de la invocación completa (tool + argumentos) que se congela mientras
/// la solicitud está pendiente.
pub fn invocation_hash(tool: &str, args: &Value) -> String {
    hash_canonical(&json!({ "tool": tool, "arguments": args }))
}

const SENSITIVE_KEYS: [&str; 6] = [
    "key",
    "api_key",
    "token",
    "password",
    "secret",
    "authorization",
];

fn is_sensitive(key: &str) -> bool {
    let k = key.to_lowercase();
    SENSITIVE_KEYS.iter().any(|s| k == *s || k.ends_with(&format!("_{s}")))
}

/// Redacta valores sensibles antes de persistir o mostrar la invocación.
/// Devuelve `(redacted, hubo_redacción)`.
pub fn redact_arguments(value: &Value) -> (Value, bool) {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            let mut changed = false;
            for (k, v) in map {
                if is_sensitive(k) {
                    out.insert(k.clone(), json!("<redacted>"));
                    changed = true;
                } else {
                    let (rv, rc) = redact_arguments(v);
                    out.insert(k.clone(), rv);
                    changed |= rc;
                }
            }
            (Value::Object(out), changed)
        }
        Value::Array(items) => {
            let mut changed = false;
            let out: Vec<Value> = items
                .iter()
                .map(|v| {
                    let (rv, rc) = redact_arguments(v);
                    changed |= rc;
                    rv
                })
                .collect();
            (Value::Array(out), changed)
        }
        other => (other.clone(), false),
    }
}

/// Resumen legible sin secretos.
pub fn summarize(tool: &str, args: &Value) -> String {
    match tool {
        "filesystem.read" | "filesystem.write" => {
            let path = args["path"].as_str().unwrap_or("(sin ruta)");
            if tool == "filesystem.write" {
                let bytes = args["content"].as_str().map(str::len).unwrap_or(0);
                format!("{path} · {bytes} bytes")
            } else {
                path.to_string()
            }
        }
        "shell.execute" => "(comando denegado por política)".to_string(),
        _ => "(invocación sin resumen)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(scope: &str, run: Option<&str>, target: Option<&str>) -> ToolGrant {
        ToolGrant {
            grant_id: "g1".into(),
            workspace_id: "ws".into(),
            run_id: run.map(str::to_string),
            effect: "write".into(),
            tool_name: "filesystem.write".into(),
            target: target.map(str::to_string),
            scope: scope.into(),
            created_at: "now".into(),
            created_by: "user".into(),
        }
    }

    #[test]
    fn deny_has_priority() {
        assert_eq!(
            decide("shell.execute", "destructive", None, "run_1", &[grant("workspace", None, None)]),
            ToolDecision::Deny
        );
    }

    #[test]
    fn no_rule_means_ask_for_effects() {
        assert_eq!(
            decide("filesystem.write", "write", Some("a.txt"), "run_1", &[]),
            ToolDecision::Ask
        );
        assert_eq!(
            decide("filesystem.read", "read", Some("a.txt"), "run_1", &[]),
            ToolDecision::Allow
        );
    }

    #[test]
    fn grants_cover_only_their_scope_and_target() {
        let run_grant = grant("run", Some("run_1"), Some("a.txt"));
        assert_eq!(
            decide("filesystem.write", "write", Some("a.txt"), "run_1", &[run_grant.clone()]),
            ToolDecision::Allow
        );
        assert_eq!(
            decide("filesystem.write", "write", Some("a.txt"), "run_2", &[run_grant.clone()]),
            ToolDecision::Ask
        );
        assert_eq!(
            decide("filesystem.write", "write", Some("b.txt"), "run_1", &[run_grant]),
            ToolDecision::Ask
        );
        let ws_grant = grant("workspace", None, None);
        assert_eq!(
            decide("filesystem.write", "write", Some("b.txt"), "run_9", &[ws_grant]),
            ToolDecision::Allow
        );
    }

    #[test]
    fn grant_does_not_cover_other_effect() {
        let g = grant("workspace", None, None);
        assert_eq!(
            decide("http.request", "network", None, "run_1", &[g]),
            ToolDecision::Ask
        );
    }

    #[test]
    fn targets_are_normalized() {
        assert_eq!(
            normalize_target("filesystem.write", &json!({"path": "./a/../b.txt"})),
            Some("b.txt".into())
        );
        assert_eq!(
            normalize_target("filesystem.write", &json!({"path": "a\\b\\c.txt"})),
            Some("a/b/c.txt".into())
        );
    }

    #[test]
    fn invocation_hash_is_stable_and_sensitive() {
        let a = json!({"path": "a.txt", "content": "x"});
        let b = json!({"path": "a.txt", "content": "y"});
        assert_eq!(
            invocation_hash("filesystem.write", &a),
            invocation_hash("filesystem.write", &a)
        );
        assert_ne!(
            invocation_hash("filesystem.write", &a),
            invocation_hash("filesystem.write", &b)
        );
    }

    #[test]
    fn secrets_are_redacted() {
        let (r, changed) = redact_arguments(&json!({
            "path": "a.txt", "api_key": "sk-123", "nested": {"token": "t"}
        }));
        assert!(changed);
        assert_eq!(r["api_key"], "<redacted>");
        assert_eq!(r["nested"]["token"], "<redacted>");
        assert_eq!(r["path"], "a.txt");
        assert!(!r.to_string().contains("sk-123"));
    }

    #[test]
    fn summary_has_no_content() {
        let s = summarize(
            "filesystem.write",
            &json!({"path": "a.txt", "content": "secreto"}),
        );
        assert!(s.contains("a.txt"));
        assert!(!s.contains("secreto"));
    }
}
