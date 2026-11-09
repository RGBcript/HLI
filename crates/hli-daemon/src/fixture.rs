//! Provider de prueba configurable (feature `fixture-provider`).
//!
//! Su único fin es demostrar que una herramienta no declarada en el plan se
//! detecta antes de ejecutarse. No se compila en el binario de producción.

use std::time::Duration;

use hli_agent::{Provider, ProviderRequest, ProviderResponse};
use hli_core::{Result, ids::new_id, model::ToolCall};
use serde_json::{Value, json};

pub struct FixtureProvider {
    pub declared_tool: String,
    pub declared_effect: String,
    pub emitted_tool: String,
    pub emitted_effect: String,
    pub arguments: Value,
    pub result: String,
    pub delay_ms: u64,
}

impl Default for FixtureProvider {
    fn default() -> Self {
        Self {
            declared_tool: "filesystem.read".into(),
            declared_effect: "read".into(),
            emitted_tool: "filesystem.write".into(),
            emitted_effect: "write".into(),
            arguments: json!({"path": "revision-test.txt", "content": "written-after-revision"}),
            result: "fixture".into(),
            delay_ms: 0,
        }
    }
}

impl FixtureProvider {
    pub fn from_env() -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        let mut p = Self::default();
        p.declared_tool = env("HLI_FIXTURE_DECLARED_TOOL", &p.declared_tool);
        p.declared_effect = env("HLI_FIXTURE_DECLARED_EFFECT", &p.declared_effect);
        p.emitted_tool = env("HLI_FIXTURE_EMITTED_TOOL", &p.emitted_tool);
        p.emitted_effect = env("HLI_FIXTURE_EMITTED_EFFECT", &p.emitted_effect);
        if let Ok(raw) = std::env::var("HLI_FIXTURE_ARGUMENTS")
            && let Ok(v) = serde_json::from_str::<Value>(&raw)
        {
            p.arguments = v;
        }
        p.result = env("HLI_FIXTURE_RESULT", &p.result);
        p.delay_ms = env("HLI_FIXTURE_DELAY_MS", "0").parse().unwrap_or_default();
        p
    }
}

impl Provider for FixtureProvider {
    fn name(&self) -> &str {
        "fixture"
    }

    fn model(&self) -> &str {
        "fixture-1"
    }

    fn complete(&self, _req: &ProviderRequest) -> Result<ProviderResponse> {
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        Ok(ProviderResponse {
            text: self.result.clone(),
            tool_call: Some(ToolCall {
                id: new_id("call"),
                name: self.emitted_tool.clone(),
                arguments: self.arguments.clone(),
            }),
        })
    }
}
