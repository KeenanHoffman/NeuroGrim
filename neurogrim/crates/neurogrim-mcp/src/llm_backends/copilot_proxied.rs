//! `copilot-proxied` LLM backend — OpenAI-compatible chat completions
//! routed through `D:/Brains/copilot-proxy` on its loopback port.
//!
//! Auth: `X-Scope-Token` header. Token sourced from environment
//! variable `COPILOT_PROXY_SCOPE_TOKEN`. Issue one via
//! `proxy-cli issue --label <name> --profile default`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};

use neurogrim_core::llm_backend::{
    LlmBackend, LlmBackendConfig, LlmBackendFactory, LlmInvokeOptions, LlmResponse,
};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4546";
const SCOPE_TOKEN_ENV: &str = "COPILOT_PROXY_SCOPE_TOKEN";

/// Wire-name + factory shape mirroring NeuroGrim's other registry
/// patterns (`QueueBackendFactory`, `SensorFactory`, `ScoringSourceFactory`).
#[derive(Default)]
pub struct CopilotProxiedFactory;

impl LlmBackendFactory for CopilotProxiedFactory {
    fn name(&self) -> &'static str {
        "copilot-proxied"
    }

    fn build(&self, config: &LlmBackendConfig) -> anyhow::Result<Arc<dyn LlmBackend>> {
        // Optional override of the proxy base URL via
        // config.options.base_url. Defaults to the proxy's documented
        // loopback bind.
        let base_url = config
            .options
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        Ok(Arc::new(CopilotProxiedBackend::new(base_url)?))
    }
}

pub struct CopilotProxiedBackend {
    http: reqwest::Client,
    base_url: String,
}

impl CopilotProxiedBackend {
    pub fn new(base_url: String) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(format!("neurogrim-mcp/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(120))
            .build()
            .context("building reqwest client")?;
        Ok(Self { http, base_url })
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

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
impl LlmBackend for CopilotProxiedBackend {
    fn name(&self) -> &str {
        "copilot-proxied"
    }

    fn supports_streaming(&self) -> bool {
        // The proxy supports SSE pass-through; we don't surface it to
        // the unary `invoke` callers in Phase 1.4 — defer to a later
        // streaming-aware revision when there's a consumer.
        false
    }

    async fn invoke(
        &self,
        prompt: &str,
        model: &str,
        options: &LlmInvokeOptions,
    ) -> anyhow::Result<LlmResponse> {
        let scope_token = std::env::var(SCOPE_TOKEN_ENV).map_err(|_| {
            anyhow!(
                "{SCOPE_TOKEN_ENV} env var not set. Run \
                 `proxy-cli issue --label <name>` and export the resulting \
                 nb_sct_… token before invoking this backend."
            )
        })?;
        let mut headers = HeaderMap::new();
        headers.insert("X-Scope-Token", scope_token.parse()?);
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
            stream: None,
        };

        let started = Instant::now();
        let resp = self
            .http
            .post(self.endpoint())
            .headers(headers)
            .json(&body)
            .send()
            .await
            .context("posting chat completion to copilot-proxy")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "copilot-proxy returned {status}: {}",
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
            backend: "copilot-proxied".to_string(),
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
