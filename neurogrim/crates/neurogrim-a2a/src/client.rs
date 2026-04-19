//! `TaskClient` — invokes peer Brains over A2A.
//!
//! Implements the client-side task lifecycle from spec §13.3 + Appendix G.3:
//!
//! 1. POST envelope to `{endpoint}/a2a/v1/tasks`
//! 2. Receive 202 + `task_id`
//! 3. Poll (or stream) until the peer returns the terminal envelope
//! 4. Validate the response and hand it back to the caller
//!
//! # Idempotency (spec §13.3 step 5)
//!
//! Repeating a call with the same `message_id` MUST yield the same response
//! without re-invoking the peer. v1 caches in memory keyed by `message_id`.
//! A production deployment would persist this cache so a crash doesn't cause
//! duplicate executions — we acknowledge that up front rather than claim
//! correctness we don't deliver.

use crate::agent_card::AgentCard;
use crate::envelope::A2aEnvelope;
use crate::error::A2aError;
use crate::transport::{HttpSseTransport, Transport};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use url::Url;

/// Polling cadence and ceiling when waiting for a task to complete.
/// Values are conservative — tuned for honesty over speed. Adopters with
/// real-time needs should swap in streaming via `Transport::stream_task`.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const POLL_MAX_ATTEMPTS: usize = 300; // 60s total at 200ms — long enough for any v1 task.

/// Client for invoking peer Brains. Generic over [`Transport`] so tests can
/// substitute an in-memory implementation without touching the network.
///
/// Clone semantics: `TaskClient` is cheap to clone — the transport is behind
/// an `Arc` and the idempotency cache is shared across clones. This is
/// intentional so a single logical client can be handed to multiple tasks.
pub struct TaskClient<T: Transport> {
    transport: Arc<T>,
    /// Idempotency cache keyed by `message_id` → the terminal envelope the
    /// peer returned. v1 is process-local; persisting is future work.
    cache: Arc<RwLock<HashMap<String, A2aEnvelope>>>,
    http: reqwest::Client,
}

impl TaskClient<HttpSseTransport> {
    /// Convenience constructor — wires up the default HTTP+SSE transport
    /// per spec §13.5 RECOMMENDED.
    pub fn new_http() -> Self {
        Self::new(HttpSseTransport::new())
    }
}

impl<T: Transport> TaskClient<T> {
    /// Construct a new client around an arbitrary transport.
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
            cache: Arc::new(RwLock::new(HashMap::new())),
            http: reqwest::Client::new(),
        }
    }

    /// Access the idempotency cache (read-only). Useful for tests and for
    /// observability — the cache is also checked internally before every
    /// `invoke` call.
    pub fn cache(&self) -> Arc<RwLock<HashMap<String, A2aEnvelope>>> {
        Arc::clone(&self.cache)
    }

    /// Full task lifecycle: post the envelope, wait for completion, return
    /// the terminal envelope. Honors the idempotency cache — a repeat call
    /// with the same `message_id` returns the cached response without
    /// touching the peer.
    ///
    /// The v1 implementation uses polling only; streaming is available via
    /// [`Transport::stream_task`] but not exercised here to keep the happy
    /// path simple. Future work: prefer streaming when the peer's Agent Card
    /// declares `capabilities.streaming: true`.
    pub async fn invoke(
        &self,
        peer_endpoint: &Url,
        envelope: A2aEnvelope,
    ) -> Result<A2aEnvelope, A2aError> {
        // Idempotency fast-path — spec §13.3 step 5 applies to clients too:
        // if we already have a cached response for this `message_id`, we owe
        // the peer nothing.
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&envelope.message_id) {
                tracing::debug!(
                    message_id = %envelope.message_id,
                    "A2A client idempotency cache hit"
                );
                return Ok(cached.clone());
            }
        }

        let message_id = envelope.message_id.clone();
        let accepted = self.transport.post_task(peer_endpoint, &envelope).await?;
        tracing::info!(
            task_id = %accepted.task_id,
            message_id = %message_id,
            "A2A task accepted by peer"
        );

        // Poll loop. Honest about the simplicity: no backoff, no streaming
        // fall-through. We bound attempts so a buggy peer can't hang us.
        for attempt in 0..POLL_MAX_ATTEMPTS {
            match self
                .transport
                .poll_task(peer_endpoint, &accepted.task_id)
                .await?
            {
                Some(resp) => {
                    validate_response(&envelope, &resp)?;
                    let mut cache = self.cache.write().await;
                    cache.insert(message_id, resp.clone());
                    return Ok(resp);
                }
                None => {
                    if attempt + 1 < POLL_MAX_ATTEMPTS {
                        tokio::time::sleep(POLL_INTERVAL).await;
                    }
                }
            }
        }

        Err(A2aError::Transport(format!(
            "task {} did not complete within {:?}",
            accepted.task_id,
            POLL_INTERVAL * POLL_MAX_ATTEMPTS as u32
        )))
    }

    /// Fetch and validate a peer's Agent Card from its well-known URL
    /// (spec §13.2, §13.8, G.2). Validation in v1:
    ///
    /// - `schema_version` must be `"1"` (otherwise we don't know the shape).
    /// - `capabilities.accepts` and `capabilities.emits` must each be non-empty —
    ///   a Brain that neither accepts nor emits anything cannot participate
    ///   as a peer.
    ///
    /// `peer_endpoint` may carry a path (e.g. `https://host/a2a/v1/`) — the
    /// well-known URL is always resolved against the *authority root* per
    /// RFC 5785, so the endpoint path is deliberately discarded for discovery.
    pub async fn discover(&self, peer_endpoint: &Url) -> Result<AgentCard, A2aError> {
        self.discover_at(peer_endpoint, None).await
    }

    /// Like [`Self::discover`] but with an optional override for the Agent
    /// Card URL. Spec §9.7 step 1 + Appendix G.3 allow a peer registration to
    /// carry a non-default `agent_card_url` — we honor that here.
    ///
    /// If `override_url` is `None`, the well-known URL derived from
    /// `peer_endpoint` is used. The resolution uses an absolute-path
    /// reference (`/.well-known/...`), so the endpoint's scheme + authority
    /// are kept but its path is replaced — matching RFC 5785 semantics for
    /// well-known URIs.
    pub async fn discover_at(
        &self,
        peer_endpoint: &Url,
        override_url: Option<&Url>,
    ) -> Result<AgentCard, A2aError> {
        let card_url = match override_url {
            Some(u) => u.clone(),
            None => peer_endpoint
                .join("/.well-known/agent-card.json")
                .map_err(|e| A2aError::AgentCardUnreachable(format!("bad endpoint: {e}")))?,
        };

        let resp = self
            .http
            .get(card_url.clone())
            .send()
            .await
            .map_err(|e| A2aError::AgentCardUnreachable(format!("{card_url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(A2aError::AgentCardUnreachable(format!(
                "{card_url} returned {}",
                resp.status()
            )));
        }

        let card: AgentCard = resp
            .json()
            .await
            .map_err(|e| A2aError::AgentCardInvalid(format!("parse error: {e}")))?;

        if card.schema_version != "1" {
            return Err(A2aError::AgentCardInvalid(format!(
                "unsupported schema_version {:?}; v1 client only understands \"1\"",
                card.schema_version
            )));
        }
        if card.capabilities.accepts.is_empty() {
            return Err(A2aError::AgentCardInvalid(
                "capabilities.accepts is empty — peer cannot receive any messages".into(),
            ));
        }
        // Intentionally NOT validating that emits is non-empty. A Brain that
        // only serves responses (request/response only — e.g. a leaf Brain
        // that answers snapshot.requested but never proactively emits
        // score.updated) is a legitimate pattern. The schema requires the
        // `emits` field to exist, not to be non-empty; rejecting empty
        // arrays would force such a Brain to lie about emissions it doesn't
        // produce — a culture.yaml integrity violation.

        Ok(card)
    }
}

/// Validate a response envelope against the request it claims to answer.
/// Kept as a free function so tests can call it directly.
fn validate_response(request: &A2aEnvelope, response: &A2aEnvelope) -> Result<(), A2aError> {
    if response.schema_version != "1" {
        return Err(A2aError::InvalidEnvelope(format!(
            "response schema_version {:?} != \"1\"",
            response.schema_version
        )));
    }
    // `reply_to` SHOULD reference the original message_id (spec §13.3 step 4
    // uses "often with reply_to"). We warn but do not reject — some peer
    // message types (e.g. `ecosystem.scored`) are emitted unsolicited.
    if let Some(reply_to) = &response.reply_to {
        if reply_to != &request.message_id {
            tracing::warn!(
                expected = %request.message_id,
                actual = %reply_to,
                "A2A response reply_to does not match request message_id"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::MessageType;
    use crate::transport::{EnvelopeStream, TaskAccepted};
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// In-memory transport that always returns a fixed response. Counts
    /// `post_task` invocations so we can assert idempotency reality.
    struct MockTransport {
        response: A2aEnvelope,
        post_calls: AtomicUsize,
    }

    impl MockTransport {
        fn new(response: A2aEnvelope) -> Self {
            Self {
                response,
                post_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn post_task(
            &self,
            _endpoint: &Url,
            _envelope: &A2aEnvelope,
        ) -> Result<TaskAccepted, A2aError> {
            self.post_calls.fetch_add(1, Ordering::SeqCst);
            Ok(TaskAccepted {
                task_id: "mock-task".into(),
                accepted_at: Utc::now(),
            })
        }

        async fn poll_task(
            &self,
            _endpoint: &Url,
            _task_id: &str,
        ) -> Result<Option<A2aEnvelope>, A2aError> {
            Ok(Some(self.response.clone()))
        }

        async fn stream_task(
            &self,
            _endpoint: &Url,
            _task_id: &str,
        ) -> Result<EnvelopeStream, A2aError> {
            // Unused in these tests; surface an honest error rather than todo!()
            Err(A2aError::Transport("stream_task not used in test".into()))
        }
    }

    fn mk_request() -> A2aEnvelope {
        A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}))
    }

    fn mk_response(reply_to: &str) -> A2aEnvelope {
        let mut env =
            A2aEnvelope::new("peer", MessageType::SnapshotDelivered, json!({"score": 80}));
        env.reply_to = Some(reply_to.into());
        env
    }

    #[tokio::test]
    async fn idempotent_invoke_returns_cached_response() {
        // Positivity frame: repeating a message_id is a legitimate retry —
        // we serve it from cache so the peer stays calm.
        let request = mk_request();
        let response = mk_response(&request.message_id);
        let mock = Arc::new(MockTransport::new(response.clone()));
        let client = TaskClient {
            transport: mock.clone(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            http: reqwest::Client::new(),
        };
        let endpoint = Url::parse("https://peer.example/").unwrap();

        let first = client.invoke(&endpoint, request.clone()).await.unwrap();
        let second = client.invoke(&endpoint, request.clone()).await.unwrap();

        assert_eq!(first, second, "idempotent invoke must return same envelope");
        assert_eq!(
            mock.post_calls.load(Ordering::SeqCst),
            1,
            "second invoke must not hit the peer"
        );
    }

    #[test]
    fn validate_response_rejects_bad_schema_version() {
        // Integrity: a peer claiming a future schema version is flagged, not
        // silently accepted.
        let req = mk_request();
        let mut resp = mk_response(&req.message_id);
        resp.schema_version = "2".into();
        let err = validate_response(&req, &resp).unwrap_err();
        matches!(err, A2aError::InvalidEnvelope(_));
    }

    #[test]
    fn validate_response_accepts_missing_reply_to() {
        // Some spontaneous emissions (ecosystem.scored) have no reply_to —
        // reject-on-missing would be too strict.
        let req = mk_request();
        let mut resp = mk_response(&req.message_id);
        resp.reply_to = None;
        assert!(validate_response(&req, &resp).is_ok());
    }

    #[test]
    fn wellknown_url_ignores_endpoint_path() {
        // Regression guard: RFC 5785 well-known URIs live at the authority
        // root. An earlier version used `Url::join(".well-known/...")` which
        // treats the input as a relative reference and appends to the
        // endpoint path when it ends in `/`. That produced bogus URLs like
        // `http://host/a2a/v1/.well-known/agent-card.json` and 404'd against
        // a correctly-configured server. The fix is the leading `/` — this
        // test pins that behavior so it doesn't silently regress.
        let path_bearing = Url::parse("http://127.0.0.1:18421/a2a/v1/").unwrap();
        let card_url = path_bearing.join("/.well-known/agent-card.json").unwrap();
        assert_eq!(
            card_url.as_str(),
            "http://127.0.0.1:18421/.well-known/agent-card.json",
            "well-known URL must sit at authority root, not under endpoint path"
        );

        // And the no-path case (what `mock_peer.rs` uses) keeps working —
        // this is the shape the rest of the test suite already relies on.
        let root_only = Url::parse("http://127.0.0.1:8421/").unwrap();
        let card_url = root_only.join("/.well-known/agent-card.json").unwrap();
        assert_eq!(
            card_url.as_str(),
            "http://127.0.0.1:8421/.well-known/agent-card.json"
        );
    }
}
