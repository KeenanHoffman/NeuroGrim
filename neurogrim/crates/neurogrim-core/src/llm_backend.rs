//! Pluggable LLM backend dispatch (V5-feature-1, 2026-05-09).
//!
//! Mirrors the V5-MOD-1 / V5-MOD-2 / V5-MOD-3 trait+registry pattern
//! (see [`scoring_source`], [`sensor`], [`queue_backend`]):
//! - `trait LlmBackend` — the abstraction
//! - `trait LlmBackendFactory` — wire-name → instance constructor
//! - `LlmBackendRegistry` — `HashMap<&'static str, Box<dyn LlmBackendFactory>>`
//!
//! ## Why
//!
//! NeuroGrim's CLI and MCP tools want to spawn LLM subagents through
//! a uniform abstraction so a Claude Code session can invoke (a) a
//! Copilot-proxied OpenAI-shaped model, (b) the local `codex` CLI as
//! a subprocess, or (c) a direct Anthropic API call — all behind one
//! type. This trait is the seam.
//!
//! ## Where the actual backend impls live
//!
//! Built-in implementations are NOT in this file (and not in
//! `neurogrim-core` at all) — they pull HTTP clients and process
//! spawning, both of which break this crate's "pure logic, zero I/O"
//! posture. Instead, the four built-ins ship in their consumer crates:
//!
//! - `neurogrim-mcp` for the HTTP-shaped backends
//!   (`anthropic`, `anthropic-proxied`, `copilot-proxied`)
//! - `neurogrim-cli` for the subprocess-shaped backend (`codex-cli`)
//!
//! Each consumer registers its built-ins via
//! `registry.register_all(crate::llm_backends::built_in_factories())`
//! at startup. Operators register third-party factories the same way.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// One LLM invocation's input options. Most fields are optional; the
/// backend supplies sensible defaults when they're `None`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LlmInvokeOptions {
    /// Hard cap on output tokens. `None` lets the backend use its
    /// own default (typically 4k–8k depending on the upstream model).
    pub max_tokens: Option<u32>,
    /// Optional system prompt; backend renders it the way the upstream
    /// expects (Anthropic system role; OpenAI-shaped first system
    /// message; Codex CLI's stdin envelope).
    pub system_prompt: Option<String>,
    /// Sampling temperature. `None` honors the backend's default.
    pub temperature: Option<f32>,
    /// Optional per-call timeout. `None` keeps the backend's default
    /// (typically 60–120 s).
    pub timeout: Option<Duration>,
}

/// One streaming chunk. Stream backends emit a sequence of these and
/// then a final `LlmResponse` to the unary path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmStreamChunk {
    /// Incremental text. Append to the running output.
    Delta { text: String },
    /// Periodic usage update (some upstreams emit final-only).
    Usage { tokens_in: u32, tokens_out: u32 },
    /// Stream is done. The unary `invoke()` call's `Result` carries
    /// the canonical totals; this is informational only.
    Done,
}

/// Final unary response. Backends always return this from `invoke`;
/// streaming backends additionally emit deltas via the optional sink
/// (see [`LlmBackend::invoke_streaming`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// The model the backend actually invoked. Echoes the request's
    /// `model` for direct calls; may differ for backends that perform
    /// model-aliasing internally.
    pub model: String,
    /// The wire-name of the backend that handled the call (mirrors
    /// `Self::name`). Useful for the outcome ledger.
    pub backend: String,
    pub duration_ms: u64,
}

/// Wire-name to backend dispatch. Object-safe trait for `Arc<dyn>`
/// storage in the registry. `async_trait` for `async fn` until the
/// workspace MSRV bumps high enough for native trait async-fn support.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Stable wire-name. Matches the corresponding factory's `name()`.
    fn name(&self) -> &str;

    /// True iff this backend can stream (the streaming sink in the
    /// `LlmInvokeOptions` will be honored when set). Backends that
    /// always buffer — like `codex-cli` — return false.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Unary invocation. Backends MUST honor `options.timeout` when set,
    /// MUST return `Err` on upstream non-2xx (do NOT swallow errors as
    /// `LlmResponse` text — the conformance suite checks this), and
    /// MUST set `LlmResponse::backend = self.name()`.
    async fn invoke(
        &self,
        prompt: &str,
        model: &str,
        options: &LlmInvokeOptions,
    ) -> anyhow::Result<LlmResponse>;
}

/// Per-backend type-erased configuration. Backends declare their own
/// schema via the `Value`; the registry treats it as opaque + passes
/// it through to `build()`. Mirrors the `queue_backend::Factory`
/// pattern's `queue_root + topic` shape, but for LLMs the surface is
/// looser since each backend has its own knobs.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LlmBackendConfig {
    /// Backend wire-name (selector). Required.
    pub name: String,
    /// Backend-specific options. Each backend documents its own keys.
    /// Examples:
    /// - `anthropic`: `{ "api_key_env": "ANTHROPIC_API_KEY", "base_url": "..." }`
    /// - `copilot-proxied`: `{ "scope_token_env": "COPILOT_SCOPE_TOKEN", "base_url": "http://127.0.0.1:4546" }`
    /// - `codex-cli`: `{ "binary": "codex", "expected_version": "1.x" }`
    #[serde(default)]
    pub options: serde_json::Map<String, serde_json::Value>,
}

/// Constructor for [`LlmBackend`] instances. Same shape as
/// `QueueBackendFactory` — registries hold `Box<dyn>` factories +
/// hand out `Arc<dyn LlmBackend>` from `build()`.
pub trait LlmBackendFactory: Send + Sync {
    /// Stable wire-name. Lookup key in the registry.
    fn name(&self) -> &'static str;

    /// Build one backend instance from the given config. Configs that
    /// don't match the factory's expected schema return Err so the
    /// dispatch fails-closed at registry-build time, not at first call.
    fn build(&self, config: &LlmBackendConfig) -> anyhow::Result<Arc<dyn LlmBackend>>;
}

/// Hand-rolled registry mapping wire-names to factories. Same posture
/// as [`crate::queue_backend::QueueBackendRegistry`] —
/// `HashMap<&'static str, Box<dyn LlmBackendFactory>>`, no
/// `inventory`/`linkme`/`ctor` substrate, last-write-wins on duplicate
/// name (lets tests + operators override built-ins).
pub struct LlmBackendRegistry {
    factories: HashMap<&'static str, Box<dyn LlmBackendFactory>>,
}

impl LlmBackendRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register(&mut self, factory: Box<dyn LlmBackendFactory>) {
        let name = factory.name();
        self.factories.insert(name, factory);
    }

    pub fn register_all(
        &mut self,
        factories: impl IntoIterator<Item = Box<dyn LlmBackendFactory>>,
    ) {
        for factory in factories {
            self.register(factory);
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn LlmBackendFactory> {
        self.factories.get(name).map(|f| f.as_ref())
    }

    /// Convenience: look up + build. Returns `None` when no factory is
    /// registered; otherwise propagates `build()`'s `Result`.
    pub fn build(
        &self,
        config: &LlmBackendConfig,
    ) -> Option<anyhow::Result<Arc<dyn LlmBackend>>> {
        self.get(&config.name).map(|f| f.build(config))
    }

    pub fn has(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    pub fn registered_names(&self) -> impl Iterator<Item = &&'static str> {
        self.factories.keys()
    }

    pub fn len(&self) -> usize {
        self.factories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

impl Default for LlmBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------
// In-crate test backend.
// ---------------------------------------------------------------------
//
// Lets neurogrim-core's tests verify the registry shape end-to-end
// without pulling reqwest/tokio-process into core. Consumer crates'
// real backends use the conformance suite (sibling module) to verify
// they honor the same contract.
//
// Echoes the prompt back as the response text. NOT a no-op: it sets
// `tokens_in/out` from string lengths so calibration exercises see
// realistic numbers in unit tests.

/// In-memory backend for tests. Echoes the prompt back; counts tokens
/// as `text.split_whitespace().count()`. Configurable failure mode
/// (`options.fail = true`) for error-path tests.
pub struct EchoBackend {
    pub name: &'static str,
}

#[async_trait]
impl LlmBackend for EchoBackend {
    fn name(&self) -> &str {
        self.name
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    async fn invoke(
        &self,
        prompt: &str,
        model: &str,
        _options: &LlmInvokeOptions,
    ) -> anyhow::Result<LlmResponse> {
        let started = std::time::Instant::now();
        let tokens_in = prompt.split_whitespace().count() as u32;
        let text = format!("[echo:{}] {}", self.name, prompt);
        let tokens_out = text.split_whitespace().count() as u32;
        Ok(LlmResponse {
            text,
            tokens_in,
            tokens_out,
            model: model.to_string(),
            backend: self.name.to_string(),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// Factory for [`EchoBackend`].
pub struct EchoBackendFactory {
    pub name: &'static str,
}

impl LlmBackendFactory for EchoBackendFactory {
    fn name(&self) -> &'static str {
        self.name
    }

    fn build(&self, _config: &LlmBackendConfig) -> anyhow::Result<Arc<dyn LlmBackend>> {
        Ok(Arc::new(EchoBackend { name: self.name }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_factory(name: &'static str) -> Box<dyn LlmBackendFactory> {
        Box::new(EchoBackendFactory { name })
    }

    #[test]
    fn registry_starts_empty() {
        let r = LlmBackendRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(!r.has("anthropic"));
    }

    #[test]
    fn register_and_get() {
        let mut r = LlmBackendRegistry::new();
        r.register(echo_factory("test-1"));
        assert!(r.has("test-1"));
        assert_eq!(r.len(), 1);
        let f = r.get("test-1").expect("factory exists");
        assert_eq!(f.name(), "test-1");
    }

    #[test]
    fn register_all_adds_each() {
        let mut r = LlmBackendRegistry::new();
        r.register_all([
            echo_factory("a"),
            echo_factory("b"),
            echo_factory("c"),
        ]);
        assert_eq!(r.len(), 3);
        for name in ["a", "b", "c"] {
            assert!(r.has(name), "registered {name}");
        }
    }

    #[test]
    fn last_write_wins_on_duplicate() {
        let mut r = LlmBackendRegistry::new();
        r.register(echo_factory("dup"));
        r.register(echo_factory("dup")); // overwrite
        assert_eq!(r.len(), 1, "duplicate registration replaces, not stacks");
    }

    #[tokio::test]
    async fn build_then_invoke() {
        let mut r = LlmBackendRegistry::new();
        r.register(echo_factory("echo"));
        let cfg = LlmBackendConfig {
            name: "echo".into(),
            options: Default::default(),
        };
        let backend = r
            .build(&cfg)
            .expect("registered")
            .expect("build succeeds");
        let resp = backend
            .invoke("hello world", "test-model", &LlmInvokeOptions::default())
            .await
            .expect("invoke");
        assert!(resp.text.contains("hello world"));
        assert_eq!(resp.backend, "echo");
        assert_eq!(resp.model, "test-model");
        assert_eq!(resp.tokens_in, 2);
        assert!(resp.tokens_out >= 2);
    }

    #[test]
    fn unknown_backend_returns_none_from_build() {
        let r = LlmBackendRegistry::new();
        let cfg = LlmBackendConfig {
            name: "nobody".into(),
            options: Default::default(),
        };
        let result = r.build(&cfg);
        assert!(result.is_none(), "unknown backend yields None, not panic");
    }

    #[tokio::test]
    async fn duration_ms_is_set() {
        let backend = EchoBackend { name: "echo" };
        let resp = backend
            .invoke("ping", "m", &LlmInvokeOptions::default())
            .await
            .unwrap();
        // Duration is non-negative; on a fast machine it can be zero.
        assert!(resp.duration_ms < 1_000, "echo should be effectively instant");
    }
}
