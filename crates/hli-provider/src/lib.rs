//! Proveedores reales de HLI: Ollama nativo y OpenAI-compatible.
//! Sin streaming en esta fase; el usage se reporta o se marca como estimado.

use std::time::{Duration, Instant};

use hli_core::{HliError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub latency_first_token_ms: i64,
    pub latency_total_ms: i64,
    pub finish_reason: String,
    pub raw_usage_available: bool,
    pub usage_source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatResponse {
    pub provider: String,
    pub model: String,
    pub adapter_version: String,
    pub text: String,
    pub usage: Usage,
}

pub struct ChatRequest<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub temperature: f64,
    pub max_tokens: i64,
}

pub trait ChatProvider {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn chat(&self, req: &ChatRequest<'_>) -> Result<ChatResponse>;
}

pub fn estimate_tokens(s: &str) -> i64 {
    ((s.chars().count() as i64) + 3) / 4
}

// ---------------------------------------------------------------------------
// Ollama nativo (/api/chat)
// ---------------------------------------------------------------------------

pub struct OllamaProvider {
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
}

impl OllamaProvider {
    pub fn new(base_url: &str, model: &str, timeout_secs: u64) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

impl ChatProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    fn model(&self) -> &str {
        &self.model
    }

    fn chat(&self, req: &ChatRequest<'_>) -> Result<ChatResponse> {
        let started = Instant::now();
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "stream": false,
            "options": {"temperature": req.temperature, "num_predict": req.max_tokens}
        });
        let url = format!("{}/api/chat", self.base_url);
        let resp = ureq::post(&url)
            .timeout(self.timeout)
            .send_json(body)
            .map_err(|e| HliError::internal(format!("ollama: {e}")))?;
        let v: Value = resp.into_json().map_err(|e| HliError::internal(format!("ollama json: {e}")))?;
        if let Some(err) = v.get("error") {
            return Err(HliError::internal(format!("ollama: {err}")));
        }
        let text = v["message"]["content"].as_str().unwrap_or("").to_string();
        let total_ms = started.elapsed().as_millis() as i64;
        let input = v["prompt_eval_count"].as_i64();
        let output = v["eval_count"].as_i64();
        let cached = v["prompt_eval_cached_count"].as_i64().unwrap_or(0);
        let (usage, source) = match (input, output) {
            (Some(i), Some(o)) => (
                Usage {
                    input_tokens: i,
                    output_tokens: o,
                    cached_tokens: cached,
                    latency_first_token_ms: total_ms,
                    latency_total_ms: total_ms,
                    finish_reason: v["done_reason"].as_str().unwrap_or("stop").to_string(),
                    raw_usage_available: true,
                    usage_source: "reported".into(),
                },
                "reported",
            ),
            _ => (
                Usage {
                    input_tokens: estimate_tokens(req.system) + estimate_tokens(req.user),
                    output_tokens: estimate_tokens(&text),
                    cached_tokens: 0,
                    latency_first_token_ms: total_ms,
                    latency_total_ms: total_ms,
                    finish_reason: "unknown".into(),
                    raw_usage_available: false,
                    usage_source: "estimated".into(),
                },
                "estimated",
            ),
        };
        let _ = source;
        Ok(ChatResponse {
            provider: "ollama".into(),
            model: v["model"].as_str().unwrap_or(&self.model).to_string(),
            adapter_version: "0.5.0".into(),
            text,
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible (/v1/chat/completions)
// ---------------------------------------------------------------------------

pub struct OpenAiProvider {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub timeout: Duration,
}

impl OpenAiProvider {
    pub fn new(base_url: &str, model: &str, api_key: &str, timeout_secs: u64) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

impl ChatProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai-compatible"
    }
    fn model(&self) -> &str {
        &self.model
    }

    fn chat(&self, req: &ChatRequest<'_>) -> Result<ChatResponse> {
        let started = Instant::now();
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "temperature": req.temperature,
            "max_tokens": req.max_tokens
        });
        let url = format!("{}/chat/completions", self.base_url);
        let resp = ureq::post(&url)
            .timeout(self.timeout)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(body)
            .map_err(|e| HliError::internal(format!("openai-compatible: {e}")))?;
        let v: Value = resp.into_json().map_err(|e| HliError::internal(format!("openai json: {e}")))?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let total_ms = started.elapsed().as_millis() as i64;
        let input = v["usage"]["prompt_tokens"].as_i64();
        let output = v["usage"]["completion_tokens"].as_i64();
        let cached = v["usage"]["prompt_tokens_details"]["cached_tokens"]
            .as_i64()
            .unwrap_or(0);
        let usage = match (input, output) {
            (Some(i), Some(o)) => Usage {
                input_tokens: i,
                output_tokens: o,
                cached_tokens: cached,
                latency_first_token_ms: total_ms,
                latency_total_ms: total_ms,
                finish_reason: v["choices"][0]["finish_reason"].as_str().unwrap_or("stop").to_string(),
                raw_usage_available: true,
                usage_source: "reported".into(),
            },
            _ => Usage {
                input_tokens: estimate_tokens(req.system) + estimate_tokens(req.user),
                output_tokens: estimate_tokens(&text),
                cached_tokens: 0,
                latency_first_token_ms: total_ms,
                latency_total_ms: total_ms,
                finish_reason: "unknown".into(),
                raw_usage_available: false,
                usage_source: "estimated".into(),
            },
        };
        Ok(ChatResponse {
            provider: "openai-compatible".into(),
            model: v["model"].as_str().unwrap_or(&self.model).to_string(),
            adapter_version: "0.5.0".into(),
            text,
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// Precios configurables y coste
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Price {
    pub provider: String,
    pub model: String,
    pub currency: String,
    pub input_per_million: f64,
    pub output_per_million: f64,
    #[serde(default)]
    pub cached_input_per_million: f64,
    pub effective_from: String,
    pub source: String,
}

pub fn parse_prices_yaml(text: &str) -> Vec<Price> {
    let mut out = Vec::new();
    let mut cur: Option<Price> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let t = t.strip_prefix("- ").unwrap_or(t);
        let Some((k, v)) = t.split_once(':') else { continue };
        let k = k.trim();
        let v = v.trim().trim_matches('"');
        if k == "provider" {
            if let Some(p) = cur.take() {
                out.push(p);
            }
            cur = Some(Price {
                provider: v.to_string(),
                model: String::new(),
                currency: "USD".into(),
                input_per_million: 0.0,
                output_per_million: 0.0,
                cached_input_per_million: 0.0,
                effective_from: String::new(),
                source: "user-configured".into(),
            });
        } else if let Some(p) = cur.as_mut() {
            match k {
                "model" => p.model = v.to_string(),
                "currency" => p.currency = v.to_string(),
                "input_per_million" => p.input_per_million = v.parse().unwrap_or(0.0),
                "output_per_million" => p.output_per_million = v.parse().unwrap_or(0.0),
                "cached_input_per_million" => p.cached_input_per_million = v.parse().unwrap_or(0.0),
                "effective_from" => p.effective_from = v.to_string(),
                "source" => p.source = v.to_string(),
                _ => {}
            }
        }
    }
    if let Some(p) = cur {
        out.push(p);
    }
    out
}

pub fn cost_of(price: Option<&Price>, usage: &Usage) -> Value {
    let Some(p) = price else {
        return json!({"currency": null, "input": 0.0, "output": 0.0, "cache": 0.0,
                      "total": 0.0, "priced": false});
    };
    let input = (usage.input_tokens as f64) * p.input_per_million / 1_000_000.0;
    let output = (usage.output_tokens as f64) * p.output_per_million / 1_000_000.0;
    let cache = (usage.cached_tokens as f64) * p.cached_input_per_million / 1_000_000.0;
    json!({
        "currency": p.currency,
        "input": input,
        "output": output,
        "cache": cache,
        "total": input + output + cache,
        "priced": true,
        "source": p.source,
        "effective_from": p.effective_from
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prices() {
        let yaml = "provider: ollama\nmodel: gemma4\ncurrency: USD\ninput_per_million: 0\noutput_per_million: 0\neffective_from: 2026-11-07\nsource: user-configured\n";
        let p = parse_prices_yaml(yaml);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].model, "gemma4");
    }

    #[test]
    fn cost_is_zero_without_price() {
        let u = Usage { input_tokens: 1000, output_tokens: 100, ..Default::default() };
        let c = cost_of(None, &u);
        assert_eq!(c["priced"], false);
    }

    #[test]
    fn cost_computes_per_million() {
        let p = Price {
            provider: "x".into(), model: "m".into(), currency: "USD".into(),
            input_per_million: 1.0, output_per_million: 2.0,
            cached_input_per_million: 0.5, effective_from: "2026-01-01".into(),
            source: "user-configured".into(),
        };
        let u = Usage { input_tokens: 1_000_000, output_tokens: 1_000_000, cached_tokens: 0, ..Default::default() };
        let c = cost_of(Some(&p), &u);
        assert!((c["total"].as_f64().unwrap() - 3.0).abs() < 1e-9);
    }
}
