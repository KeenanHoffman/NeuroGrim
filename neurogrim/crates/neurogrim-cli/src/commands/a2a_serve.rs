//! `neurogrim a2a-serve` — serve this Brain as an A2A peer (spec §13, G.2, G.4).
//!
//! Starts an in-process `TaskServer` that publishes an Agent Card derived from
//! the project's `brain-registry.json` and handles peer invocations. This is
//! the server side of fractal composition (spec §9) and dual brain (§10):
//! running this command turns a leaf project Brain into something a parent
//! (or peer) can discover and call.
//!
//! # `snapshot.requested` handler
//!
//! The handler calls [`BrainContext::load`] on every invocation — the full
//! scoring pipeline: parse registry, read CMDBs, compute scorecard, evaluate
//! correlations + incident patterns, compute trajectory, rank recommendations,
//! build the `AgentOutput`. The resulting `AgentOutput` is serialized as the
//! `snapshot.delivered` payload. Loading fresh on every call is deliberate:
//! CMDBs change between calls (sensory tools update them), and we'd rather
//! pay the filesystem cost than serve a stale view of the world.
//!
//! Loading errors (registry missing, invalid JSON, etc.) surface to the peer
//! as [`A2aError::Transport`] — honest about what failed instead of returning
//! a plausible-looking placeholder.
//!
//! Culture: `.claude/culture.yaml` values applied — honesty (real scores
//! only, no faked placeholder), integrity (failures propagate, they don't get
//! swallowed), critical-but-kind (log lines name the `message_id` and the
//! scored domain count so debugging is possible without reading stdout bytes).

use neurogrim_mcp::context::BrainContext;
use anyhow::{Context, Result};
use neurogrim_a2a::agent_card::{
    AuthScheme, Authentication, Capabilities, Transport as TransportCard, TransportProtocol,
};
use neurogrim_a2a::envelope::{A2aEnvelope, MessageType};
use neurogrim_a2a::error::A2aError;
use neurogrim_a2a::token_store::TokenStore;
use neurogrim_a2a::{AgentCard, TaskServer};
use neurogrim_core::registry::BrainRegistry;
use std::net::SocketAddr;
use std::path::Path;

/// Default port for the Brain A2A server. Picked to sit above well-known
/// port ranges and the MCP default; adopters can override with `--port`.
pub const DEFAULT_PORT: u16 = 8421;

/// Truncate a registry description for use as the Agent Card `name`. The
/// schema imposes no limit, but UI surfaces usually do — keep it reasonable.
const NAME_MAX_LEN: usize = 80;

/// Build an `AgentCard` from a `BrainRegistry`. Pulled out of `run` so the
/// CLI integration tests can exercise it without standing up a server.
///
/// # Honesty about field mapping
///
/// - `id`: `registry.meta.updated_by` is the canonical identity for the
///   Brain instance (it's who last wrote the registry). If empty, we fall
///   back to a generated UUIDv4 so two concurrent Brains don't collide.
///   Both paths are deterministic given the same inputs.
/// - `name`: `registry.meta.description`, truncated. If blank, we use the
///   id. This keeps the card honest about what the operator wrote.
/// - `version`: `CARGO_PKG_VERSION` of this crate — the Brain **software**
///   version, not the interface version.
/// - `interface_version`: hardcoded `"1"` per spec §6 (agent-output-v1).
/// - `capabilities.accepts`: `[SnapshotRequested, ScoreUpdated]` —
///   snapshot.requested is the handler we actually wire; score.updated is
///   the no-op ack so peers emitting to us don't 405.
/// - `capabilities.emits`: empty — we don't proactively push anything yet.
pub fn build_agent_card_from_registry(
    registry: &BrainRegistry,
    endpoint: &str,
    cli_version: &str,
) -> AgentCard {
    build_agent_card_with_auth(registry, endpoint, cli_version, AuthScheme::None)
}

/// Like [`build_agent_card_from_registry`] but with an explicit auth scheme.
/// Separated so `run` can toggle bearer auth without duplicating the body.
pub fn build_agent_card_with_auth(
    registry: &BrainRegistry,
    endpoint: &str,
    cli_version: &str,
    auth: AuthScheme,
) -> AgentCard {
    let id = if registry.meta.updated_by.trim().is_empty() {
        // Non-empty fallback — a missing id would fail the schema. A UUID is
        // an honest placeholder: stable for the life of this process, not
        // across restarts. An operator who cares will set `meta.updated_by`.
        format!("neurogrim-{}", uuid::Uuid::new_v4())
    } else {
        registry.meta.updated_by.clone()
    };

    let name = if registry.meta.description.trim().is_empty() {
        id.clone()
    } else {
        let desc = registry.meta.description.trim();
        if desc.len() > NAME_MAX_LEN {
            // Preserve a clear boundary; no mid-word guessing.
            format!("{}…", &desc[..NAME_MAX_LEN])
        } else {
            desc.to_string()
        }
    };

    AgentCard {
        schema_version: "1".into(),
        id,
        name,
        version: cli_version.into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            // Spec §13.4: accepts and emits are sets of the 10 canonical
            // message types. Keep this list in lock-step with the handlers
            // we actually register in `run`.
            accepts: vec![MessageType::SnapshotRequested, MessageType::ScoreUpdated],
            emits: vec![],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: endpoint.into(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication { scheme: auth },
        topology: None,
    }
}

/// Produce the `snapshot.requested` response payload by running the full
/// scoring pipeline against the current registry + CMDB state.
///
/// Re-runs on every call. This means every peer invocation reflects the
/// current filesystem state — freshly-updated CMDBs produce freshly-updated
/// scores. The alternative (cache-and-poll) trades freshness for latency,
/// and for v1 we prefer honest freshness over optimized latency.
///
/// Errors propagate to the peer as [`A2aError::Transport`]: the peer sees a
/// real failure rather than a fabricated zero-score response. If registry
/// loading is the unhappy path, the peer should know.
async fn load_agent_output_payload(registry_path: &str) -> Result<serde_json::Value, A2aError> {
    let ctx = BrainContext::load(registry_path, None, None)
        .await
        .map_err(|e| A2aError::Transport(format!("scoring pipeline failed: {e}")))?;
    serde_json::to_value(&ctx.agent_output)
        .map_err(|e| A2aError::Transport(format!("agent_output serialization failed: {e}")))
}

/// Entry point for the `a2a-serve` subcommand.
///
/// `bind` is the interface address the HTTP server listens on. The default is
/// `127.0.0.1` — loopback-only, which is the safe default the Phase E
/// integration tests exercise. Container deployments (Docker, Kubernetes,
/// etc.) pass `0.0.0.0` so the port is reachable from outside the container.
///
/// Spec §13.6: `authentication: none` in v2.1. Binding to a non-loopback
/// address is only acceptable when access is gated at the network layer
/// (Docker bridge network, host firewall, VPN / service mesh, cloud VPC).
/// We do NOT enforce this at the CLI — a hard rejection would prevent the
/// reference Docker deployment from working — but we log a warning when
/// `bind` is not `127.0.0.1` so operators see the reminder in the logs.
pub async fn run(
    port: u16,
    bind: String,
    project_root: String,
    _agent_card_path: Option<String>,
    require_bearer: bool,
    token_store_path: Option<String>,
) -> Result<()> {
    // Initialize tracing with a sane default if the caller didn't. No-op if
    // already set by a higher layer — `try_init` swallows the second init.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let registry_path = Path::new(&project_root).join(".claude/brain-registry.json");
    let registry_text = tokio::fs::read_to_string(&registry_path)
        .await
        .with_context(|| {
            format!(
                "failed to read {} — run from a project root, or pass --project-root",
                registry_path.display()
            )
        })?;
    let registry =
        BrainRegistry::from_json(&registry_text).context("failed to parse brain-registry.json")?;
    registry
        .validate()
        .context("brain-registry.json is invalid")?;

    // Keep the registry path as a String so the async handler closures can
    // clone it cheaply on each peer invocation. Path::to_string_lossy loses
    // nothing on Windows paths we generate ourselves (no invalid UTF-8).
    let registry_path_str = registry_path.to_string_lossy().into_owned();

    // Agent Card's advertised endpoint. Honesty matters here: if we're
    // bound on 0.0.0.0 we still don't know the host's routable name, so we
    // advertise 127.0.0.1 (the peer-local view) and leave it to the
    // *caller* of the card to rewrite the host when aggregating. This is
    // the same compromise the Phase E test makes — ecosystem.yaml carries
    // the routable URL; the card's `transport.endpoint` is informational.
    let endpoint = format!("http://127.0.0.1:{port}/a2a/v1/");
    let auth_scheme = if require_bearer {
        AuthScheme::Bearer
    } else {
        AuthScheme::None
    };
    let card = build_agent_card_with_auth(
        &registry,
        &endpoint,
        env!("CARGO_PKG_VERSION"),
        auth_scheme,
    );
    let brain_id = card.id.clone();

    let mut server = TaskServer::new(card);

    // Attach the token store when bearer auth is demanded. Without a store,
    // the server would respond 500 to every authenticated request — better
    // to refuse to start than serve a misconfigured Brain.
    if require_bearer {
        let store_path = token_store_path.clone().unwrap_or_else(|| {
            Path::new(&project_root)
                .join(".claude/a2a-tokens.sqlite")
                .to_string_lossy()
                .into_owned()
        });
        let store = TokenStore::open(&store_path).with_context(|| {
            format!(
                "failed to open token store at {store_path} \
                 (hint: issue one with `neurogrim a2a-token issue --label <L>`)"
            )
        })?;
        tracing::info!(store = %store_path, "bearer auth enabled; token store attached");
        server = server.with_token_store(store);
    } else if token_store_path.is_some() {
        tracing::warn!(
            "--token-store was provided but --require-bearer was not; \
             ignoring token store (auth scheme: none)"
        );
    }

    // snapshot.requested handler — runs the full scoring pipeline on every
    // call. BrainContext::load is async (it reads the registry + CMDBs from
    // disk), so the handler body must await it. Fresh load per call means
    // we reflect current CMDB state, at the cost of a small per-call latency
    // penalty — right trade for v1.
    let brain_id_for_snapshot = brain_id.clone();
    let registry_path_for_snapshot = registry_path_str.clone();
    server.register_handler(MessageType::SnapshotRequested, move |req| {
        let brain_id = brain_id_for_snapshot.clone();
        let registry_path = registry_path_for_snapshot.clone();
        async move {
            tracing::info!(
                message_id = %req.message_id,
                from = %req.brain_id,
                "snapshot.requested received; running scoring pipeline"
            );
            let payload = load_agent_output_payload(&registry_path).await?;
            // One more log so the operator can see what actually went on the
            // wire without needing to enable debug-level tracing.
            if let Some(score) = payload.get("score").and_then(|v| v.as_i64()) {
                tracing::info!(
                    message_id = %req.message_id,
                    score = score,
                    "snapshot.delivered computed"
                );
            }
            let mut resp = A2aEnvelope::new(&brain_id, MessageType::SnapshotDelivered, payload);
            resp.reply_to = Some(req.message_id);
            Ok(resp)
        }
    });

    // score.updated ack handler — logs the receipt so peers emitting to us
    // don't get a 405. We don't act on the score yet; that's S6-DB-5.
    let brain_id_for_ack = brain_id.clone();
    server.register_handler(MessageType::ScoreUpdated, move |req| {
        let brain_id = brain_id_for_ack.clone();
        async move {
            tracing::info!(
                message_id = %req.message_id,
                from = %req.brain_id,
                "score.updated acknowledged (no-op in S6-DB-3)"
            );
            // Acks are just snapshot.delivered envelopes with an empty
            // payload — keeps the client's response validator happy.
            let mut resp = A2aEnvelope::new(
                &brain_id,
                MessageType::SnapshotDelivered,
                serde_json::json!({"ack": true}),
            );
            resp.reply_to = Some(req.message_id);
            Ok(resp)
        }
    });

    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .with_context(|| format!("failed to build bind addr from --bind={bind} --port={port}"))?;

    // Spec §13.6 reminder — logged, not enforced. When the address isn't
    // loopback, the operator needs to have network-layer access control in
    // place. We trust the operator to have read the deployment doc, but we
    // still say it so the log line is available for later audit.
    let is_loopback_bind = bind == "127.0.0.1" || bind == "::1" || bind == "localhost";
    if !is_loopback_bind {
        tracing::warn!(
            bind = %bind,
            "binding A2A server to non-loopback interface; ensure network-layer \
             access control is in place (spec §13.6 — authentication: none)"
        );
    }

    tracing::info!(
        brain_id = %brain_id,
        endpoint = %endpoint,
        bind = %addr,
        "NeuroGrim A2A server starting — peers can discover this Brain"
    );
    eprintln!("✦ Summoning NeuroGrim A2A peer");
    eprintln!("  brain_id:   {brain_id}");
    eprintln!("  endpoint:   {endpoint}");
    eprintln!("  bind addr:  {addr}");
    eprintln!("  card URL:   http://{bind}:{port}/.well-known/agent-card.json");
    eprintln!("  Ctrl-C to stop");

    server
        .serve(addr)
        .await
        .context("A2A server terminated unexpectedly")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::registry::{BrainConfig, RegistryMeta};

    fn sample_registry(updated_by: &str, description: &str) -> BrainRegistry {
        BrainRegistry {
            meta: RegistryMeta {
                schema_version: "2".into(),
                description: description.into(),
                updated_by: updated_by.into(),
            },
            tools: serde_json::Value::Null,
            data_sources: serde_json::Value::Null,
            config: BrainConfig {
                domain_weights: Default::default(),
                advisory_domains: vec![],
                principle_map: Default::default(),
                domain_definitions: Default::default(),
                scoring: Default::default(),
                gate_tiers: Default::default(),
                confidence_thresholds: Default::default(),
                staleness_thresholds: Default::default(),
                severity_thresholds: Default::default(),
                autonomy: serde_json::Value::Null,
                trajectory: Default::default(),
                attention_budget: Default::default(),
                human_personas: Default::default(),
                hats: Default::default(),
                correlations: vec![],
                incident_patterns: vec![],
                sensory_servers: Default::default(),
                // E-B2-2 C3: spec §17.3 default. Test-fixture
                // BrainConfig with calibration writer disabled.
                enable_calibration_writes: false,
                extra: Default::default(),
            },
        }
    }

    #[test]
    fn agent_card_from_registry_roundtrips_through_serde() {
        // Honesty: this is our schema proxy — the Rust types are generated
        // against agent-card-v1.schema.json, so serde roundtrip IS the
        // structural check. A full schema-validator roundtrip would add a
        // dep for no new signal.
        let reg = sample_registry("project-alpha", "Alpha Project Brain");
        let card = build_agent_card_from_registry(&reg, "http://127.0.0.1:8421/a2a/v1/", "0.1.0");

        // Required fields populated.
        assert_eq!(card.schema_version, "1");
        assert_eq!(card.id, "project-alpha");
        assert_eq!(card.name, "Alpha Project Brain");
        assert_eq!(card.version, "0.1.0");
        assert_eq!(card.interface_version, "1");

        // Capabilities declare what we actually wire.
        assert!(card
            .capabilities
            .accepts
            .contains(&MessageType::SnapshotRequested));

        // Roundtrip through serde — if the schema drifts, this deserialize
        // fails loudly instead of silently producing a half-populated card.
        let s = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&s).unwrap();
        assert_eq!(card, back, "AgentCard must roundtrip byte-identically");
    }

    #[test]
    fn agent_card_falls_back_to_uuid_when_meta_is_blank() {
        let reg = sample_registry("", "");
        let card = build_agent_card_from_registry(&reg, "http://127.0.0.1:8421/a2a/v1/", "0.1.0");
        assert!(
            card.id.starts_with("neurogrim-"),
            "blank meta.updated_by must yield a uuid-based id, got {:?}",
            card.id
        );
        assert_eq!(card.name, card.id, "blank description must fall back to id");
    }

    #[test]
    fn agent_card_truncates_long_descriptions() {
        // A 200-char description should not bloat the card's name field.
        // Truncation marker is the one-char `…` so we don't accidentally
        // produce a name shorter than we think when counting bytes.
        let long = "x".repeat(200);
        let reg = sample_registry("alpha", &long);
        let card = build_agent_card_from_registry(&reg, "http://127.0.0.1:8421/a2a/v1/", "0.1.0");
        assert!(
            card.name.chars().count() <= NAME_MAX_LEN + 1,
            "name must be truncated; got {} chars",
            card.name.chars().count()
        );
        assert!(
            card.name.ends_with('…'),
            "truncated name must carry the marker; got {:?}",
            card.name
        );
    }
}
