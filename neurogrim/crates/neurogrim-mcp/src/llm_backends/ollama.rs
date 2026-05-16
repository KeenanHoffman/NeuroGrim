//! `ollama` LLM backend — OpenAI-compatible chat completions routed
//! through a local Ollama daemon (default `http://127.0.0.1:11434`).
//!
//! v2-Feature 1 — 2026-05-09. Mirrors the `copilot_proxied.rs` pattern
//! since Ollama exposes the same OpenAI shape at `/v1/chat/completions`,
//! letting the request/response code reuse with minimal divergence.
//!
//! ## Auth posture
//!
//! Ollama has no authentication on its localhost listener. The factory
//! REJECTS any non-loopback `base_url` — operators who tunnel Ollama
//! to another host must opt in via a future config flag (Phase 2).
//! This is defense-in-depth: if an operator's environment somehow
//! exposes an Ollama instance on a non-loopback interface, this
//! backend won't accidentally point at it.
//!
//! ## Default model
//!
//! When the caller passes an empty model string, the backend defaults
//! to `qwen3.5:0.8b` (operator-locked choice). The CLI verb's
//! `--model` flag overrides per call.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};

use neurogrim_core::llm_backend::{
    LlmBackend, LlmBackendConfig, LlmBackendFactory, LlmInvokeOptions, LlmResponse,
};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen3.5:0.8b";

#[derive(Default)]
pub struct OllamaFactory;

impl LlmBackendFactory for OllamaFactory {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn build(&self, config: &LlmBackendConfig) -> anyhow::Result<Arc<dyn LlmBackend>> {
        let base_url = config
            .options
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        Ok(Arc::new(OllamaBackend::new(base_url)?))
    }
}

pub struct OllamaBackend {
    http: reqwest::Client,
    base_url: String,
}

impl OllamaBackend {
    pub fn new(base_url: String) -> anyhow::Result<Self> {
        // Defense-in-depth: reject non-loopback URLs. Ollama has no auth
        // on its listener; an operator who configured a non-loopback
        // base_url would expose every prompt to whatever's at the other
        // end. Force them to opt in via a future explicit flag.
        if !is_loopback(&base_url) {
            return Err(anyhow!(
                "ollama backend rejects non-loopback base_url {base_url:?}. \
                 Ollama has no auth on its listener; only 127.0.0.1 / ::1 / localhost \
                 are accepted. Tunneling to another host requires an explicit \
                 opt-in flag (Phase 2 — not yet shipped)."
            ));
        }
        let http = reqwest::Client::builder()
            .user_agent(format!("neurogrim-mcp/{}", env!("CARGO_PKG_VERSION")))
            // Ollama responses on local hardware are typically faster
            // than upstream HTTPS, but a cold model load can take 10-20s.
            .timeout(Duration::from_secs(120))
            .build()
            .context("building reqwest client")?;
        Ok(Self { http, base_url })
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }

    /// Probe `/api/tags` for the locally-installed model list. Used by
    /// the future `--list-models` UX (deferred to Phase 1.4). Returns
    /// the raw JSON so the caller can decide how to render.
    pub async fn probe_models(&self) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("probing ollama tags at {url}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "ollama /api/tags returned {}: is the daemon running?",
                resp.status()
            ));
        }
        Ok(resp.json::<serde_json::Value>().await?)
    }
}

fn is_loopback(base_url: &str) -> bool {
    // Cheap host-only check; we don't need full URL parsing here. The
    // base_url comes from the operator's config; we just match the
    // common shapes:
    let lower = base_url.to_lowercase();
    lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://localhost")
        || lower.starts_with("https://127.0.0.1")
        || lower.starts_with("https://[::1]")
        || lower.starts_with("https://localhost")
}

// ── OpenAI-compatible request/response shapes (shared semantics with
//    copilot_proxied; intentional duplication until Phase 1.5b dedupes
//    backend impls into neurogrim-core under a feature flag).

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: Option<String>,
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    fn supports_streaming(&self) -> bool {
        // Ollama streams NDJSON by default; we opt out via {"stream":
        // false} so the unary invoke contract returns once. Streaming
        // wires in alongside the Phase 1.5b refactor.
        false
    }

    async fn invoke(
        &self,
        prompt: &str,
        model: &str,
        options: &LlmInvokeOptions,
    ) -> anyhow::Result<LlmResponse> {
        let model = if model.trim().is_empty() {
            DEFAULT_MODEL
        } else {
            model
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse()?,
        );

        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(sys) = options.system_prompt.as_deref() {
            messages.push(ChatMessage {
                role: "system",
                content: sys,
            });
        }
        messages.push(ChatMessage {
            role: "user",
            content: prompt,
        });

        let body = ChatRequest {
            model,
            messages,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            // Explicit `stream: false` — Ollama's OpenAI-compatible
            // endpoint defaults to streaming NDJSON otherwise.
            stream: Some(false),
        };

        let started = Instant::now();
        let resp = self
            .http
            .post(self.endpoint())
            .headers(headers)
            .json(&body)
            .send()
            .await
            .context("posting chat completion to ollama")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "ollama returned {status}: {}",
                truncate(&body, 400)
            ));
        }
        let parsed: ChatResponse = resp
            .json()
            .await
            .context("parsing chat-completion response as JSON")?;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        let (tokens_in, tokens_out) = parsed
            .usage
            .as_ref()
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));
        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            model: parsed.model.unwrap_or_else(|| model.to_string()),
            backend: "ollama".to_string(),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() > n {
        format!("{}…", &s[..n])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_check_accepts_documented_urls() {
        assert!(is_loopback("http://127.0.0.1:11434"));
        assert!(is_loopback("http://[::1]:11434"));
        assert!(is_loopback("http://localhost:11434"));
        assert!(is_loopback("https://127.0.0.1"));
    }

    #[test]
    fn loopback_check_rejects_non_loopback() {
        assert!(!is_loopback("http://10.0.0.5:11434"));
        assert!(!is_loopback("http://example.com:11434"));
        assert!(!is_loopback("http://192.168.1.10"));
    }

    #[test]
    fn factory_rejects_non_loopback_base_url() {
        let factory = OllamaFactory::default();
        let mut cfg = LlmBackendConfig {
            name: "ollama".into(),
            options: serde_json::Map::new(),
        };
        cfg.options.insert(
            "base_url".into(),
            serde_json::Value::String("http://10.0.0.5:11434".into()),
        );
        let result = factory.build(&cfg);
        assert!(result.is_err());
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("non-loopback"), "expected loopback rejection, got: {msg}");
    }

    #[test]
    fn factory_accepts_loopback_base_url() {
        let factory = OllamaFactory::default();
        let cfg = LlmBackendConfig {
            name: "ollama".into(),
            options: serde_json::Map::new(),
        };
        let result = factory.build(&cfg);
        assert!(result.is_ok(), "expected loopback default to succeed");
    }

    #[test]
    fn factory_default_url_is_loopback() {
        assert!(is_loopback(DEFAULT_BASE_URL));
    }
}
