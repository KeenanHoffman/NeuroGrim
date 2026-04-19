//! Pluggable transport abstraction for A2A.
//!
//! Per spec v2.1 §13.5, two transports are defined:
//!
//! | Transport | Status | Notes |
//! |-----------|--------|-------|
//! | HTTP + SSE | RECOMMENDED | First-class; [`HttpSseTransport`] is the default |
//! | JSON-RPC over HTTP | Permitted | [`JsonRpcTransport`] is a v1 stub (spec §13.5) |
//!
//! The [`Transport`] trait exposes the three operations the client side of an A2A
//! interaction needs: post a task, poll for its final envelope, and optionally
//! consume an SSE progress stream. The server half does not sit behind this trait —
//! see `server.rs`, which binds axum directly; a separate server-side trait would
//! be over-abstraction at this stage.
//!
//! # Honesty
//!
//! This file was originally authored without access to `cargo check` and carried
//! a few `FIXME: needs cargo verification` markers. Those have since been resolved
//! — the whole workspace compiles cleanly on Rust 1.95 GNU and the mock-peer
//! integration test exercises the SSE + POST/GET paths end-to-end. If the SSE
//! parser misbehaves against a more exotic peer in the future, swapping in
//! `eventsource-stream` or `reqwest-eventsource` is the natural upgrade.

use crate::envelope::A2aEnvelope;
use crate::error::A2aError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use url::Url;

/// Result of the initial `POST /a2a/v1/tasks` — the peer has accepted the envelope
/// and assigned a `task_id` (spec §13.3 step 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAccepted {
    /// Server-assigned task identifier. Used to poll or stream for the result.
    pub task_id: String,
    /// When the peer acknowledged the task. Reported in the 202 response body
    /// (or synthesized at receive time if the peer omits it).
    pub accepted_at: DateTime<Utc>,
}

/// Boxed stream of envelope events, as emitted by an SSE `.../events` endpoint.
/// Held in a type alias because the type is long and appears twice.
pub type EnvelopeStream = Pin<Box<dyn Stream<Item = Result<A2aEnvelope, A2aError>> + Send>>;

/// Transport abstraction. Implementors provide wire-level transport of A2A
/// envelopes to a peer Brain. Kept narrow on purpose — v1 only needs what
/// [`crate::client::TaskClient`] calls.
///
/// Honesty note: every method returns `Result<_, A2aError>` so callers get
/// structured failure information rather than opaque string errors.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Create a task on the peer. Corresponds to `POST {endpoint}{tasks_path}`
    /// in spec §13.3 step 1.
    async fn post_task(
        &self,
        endpoint: &Url,
        envelope: &A2aEnvelope,
    ) -> Result<TaskAccepted, A2aError>;

    /// Poll the peer for the task's terminal envelope. Returns `None` if the
    /// peer reports the task is still in progress (spec §13.3 step 3/4).
    /// Returns the completed envelope on `Some`.
    async fn poll_task(
        &self,
        endpoint: &Url,
        task_id: &str,
    ) -> Result<Option<A2aEnvelope>, A2aError>;

    /// Open the peer's SSE progress stream for this task (spec §13.3 step 3).
    /// Each yielded envelope is one SSE `data:` event. The stream closes when
    /// the peer sends the terminal envelope.
    async fn stream_task(&self, endpoint: &Url, task_id: &str) -> Result<EnvelopeStream, A2aError>;
}

// ---------------------------------------------------------------------------
// HttpSseTransport — the RECOMMENDED transport per spec §13.5.
// ---------------------------------------------------------------------------

/// Default HTTP + SSE implementation of [`Transport`]. Built on `reqwest`.
///
/// Task URLs are constructed against the endpoint's *authority root* using an
/// absolute-path reference (`/a2a/v1/tasks...`). The leading `/` matters: it
/// replaces any path component the Agent Card's `transport.endpoint` carries
/// (e.g. `http://host/a2a/v1/`), instead of appending underneath it (which
/// would produce double-prefixed URLs like `http://host/a2a/v1/a2a/v1/tasks`
/// and 404 against the server's routes). The Agent Card's `transport.tasks_path`
/// can override `/a2a/v1/tasks`, but v1 of this client follows the spec
/// default (§13.8, G.6) and hardcodes it; future work can plumb the override
/// in from the AgentCard.
#[derive(Debug, Clone)]
pub struct HttpSseTransport {
    client: reqwest::Client,
}

impl HttpSseTransport {
    /// Construct a new transport with a default `reqwest` client. Panics only
    /// if `reqwest` itself cannot build a client in this environment (TLS
    /// backend missing, etc.) — extremely unlikely in a normal Rust host.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Construct from an existing `reqwest::Client`. Useful for plumbing
    /// timeouts, proxies, or shared connection pools from the caller.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Build the tasks-collection URL: `{authority}/a2a/v1/tasks`. The leading
    /// `/` makes this an absolute-path reference per RFC 3986 §5.3 — the
    /// endpoint's scheme + authority are kept, but its path is discarded.
    fn tasks_url(endpoint: &Url) -> Result<Url, A2aError> {
        endpoint
            .join("/a2a/v1/tasks")
            .map_err(|e| A2aError::Transport(format!("bad endpoint URL: {e}")))
    }

    fn task_url(endpoint: &Url, task_id: &str) -> Result<Url, A2aError> {
        endpoint
            .join(&format!("/a2a/v1/tasks/{task_id}"))
            .map_err(|e| A2aError::Transport(format!("bad task URL: {e}")))
    }

    fn events_url(endpoint: &Url, task_id: &str) -> Result<Url, A2aError> {
        endpoint
            .join(&format!("/a2a/v1/tasks/{task_id}/events"))
            .map_err(|e| A2aError::Transport(format!("bad events URL: {e}")))
    }
}

impl Default for HttpSseTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for HttpSseTransport {
    async fn post_task(
        &self,
        endpoint: &Url,
        envelope: &A2aEnvelope,
    ) -> Result<TaskAccepted, A2aError> {
        let url = Self::tasks_url(endpoint)?;
        let resp = self
            .client
            .post(url)
            .json(envelope)
            .send()
            .await
            .map_err(|e| A2aError::Transport(format!("POST failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(A2aError::PeerError {
                status: status.as_u16(),
                body,
            });
        }

        // Spec §13.3 step 2: 202 Accepted with `{"task_id": "..."}`.
        // We also accept an `accepted_at`, but synthesize `Utc::now()` if absent
        // — the timestamp is a convenience, not a wire contract.
        #[derive(Deserialize)]
        struct AcceptedBody {
            task_id: String,
            #[serde(default)]
            accepted_at: Option<DateTime<Utc>>,
        }
        let body: AcceptedBody = resp
            .json()
            .await
            .map_err(|e| A2aError::Transport(format!("bad 202 body: {e}")))?;

        Ok(TaskAccepted {
            task_id: body.task_id,
            accepted_at: body.accepted_at.unwrap_or_else(Utc::now),
        })
    }

    async fn poll_task(
        &self,
        endpoint: &Url,
        task_id: &str,
    ) -> Result<Option<A2aEnvelope>, A2aError> {
        let url = Self::task_url(endpoint, task_id)?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| A2aError::Transport(format!("GET failed: {e}")))?;

        let status = resp.status();
        // Convention: 404 => task not yet complete (still pending) OR unknown.
        // The client cannot distinguish these without a richer server contract;
        // we treat both as `None` and let the client surface a timeout higher up.
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(A2aError::PeerError {
                status: status.as_u16(),
                body,
            });
        }

        let env: A2aEnvelope = resp
            .json()
            .await
            .map_err(|e| A2aError::InvalidEnvelope(format!("bad response body: {e}")))?;
        Ok(Some(env))
    }

    async fn stream_task(&self, endpoint: &Url, task_id: &str) -> Result<EnvelopeStream, A2aError> {
        // SSE parsing is hand-rolled on top of reqwest's byte stream. The v1
        // implementation handles the single-terminal-event case (spec G.6:
        // "Each SSE event: `data: <a2a-envelope-json>\n\n`") and is
        // intentionally minimal — verified end-to-end by the mock-peer
        // integration test. If a more exotic peer requires multi-event or
        // reconnect semantics, swap in `eventsource-stream` or
        // `reqwest-eventsource`.
        use futures::stream::StreamExt;

        let url = Self::events_url(endpoint, task_id)?;
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|e| A2aError::Transport(format!("GET events failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(A2aError::PeerError { status, body });
        }

        let byte_stream = resp.bytes_stream();
        // Fold bytes into complete SSE events (delimited by "\n\n") and yield
        // parsed envelopes. Kept simple — no retry, no reconnect, no id tracking.
        let envelope_stream = byte_stream
            .map(|chunk_result| -> Result<Vec<u8>, A2aError> {
                chunk_result
                    .map(|b| b.to_vec())
                    .map_err(|e| A2aError::Transport(format!("SSE read: {e}")))
            })
            .scan(Vec::<u8>::new(), |buf, chunk_result| {
                let out = match chunk_result {
                    Ok(chunk) => {
                        buf.extend_from_slice(&chunk);
                        let mut events = Vec::new();
                        while let Some(pos) = find_double_newline(buf) {
                            let raw = buf.drain(..pos + 2).collect::<Vec<_>>();
                            // Strip the trailing "\n\n" we consumed.
                            let text = String::from_utf8_lossy(&raw[..raw.len() - 2]).to_string();
                            events.push(parse_sse_event(&text));
                        }
                        Ok(events)
                    }
                    Err(e) => Err(e),
                };
                futures::future::ready(Some(out))
            })
            // Flatten Vec<Result<Envelope, _>> yielded per chunk into a flat stream.
            .flat_map(|res| match res {
                Ok(events) => futures::stream::iter(events).boxed(),
                Err(e) => futures::stream::iter(vec![Err(e)]).boxed(),
            })
            .boxed();

        Ok(envelope_stream)
    }
}

/// Locate the first `\n\n` (SSE event terminator) in `buf`. Returns the index
/// of the first `\n` of the pair.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

/// Parse a single SSE event text block. Only `data:` lines are honored;
/// concatenated per SSE spec and parsed as a JSON A2A envelope.
fn parse_sse_event(text: &str) -> Result<A2aEnvelope, A2aError> {
    let mut data = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim_start());
        }
        // We intentionally ignore `event:`, `id:`, `retry:` — v1 doesn't use them.
    }
    if data.is_empty() {
        return Err(A2aError::InvalidEnvelope("empty SSE event".into()));
    }
    serde_json::from_str::<A2aEnvelope>(&data)
        .map_err(|e| A2aError::InvalidEnvelope(format!("SSE JSON parse: {e}")))
}

// ---------------------------------------------------------------------------
// JsonRpcTransport — Permitted but not RECOMMENDED (spec §13.5); v1 stub.
// ---------------------------------------------------------------------------

/// Stub implementation for the JSON-RPC transport. Spec §13.5 permits it for
/// "simple request/response peers, compatibility" but does not recommend it.
/// v1 of the reference implementation does not wire it up; the trait methods
/// return `todo!()` so callers fail loudly rather than silently misbehave.
///
/// If you need a JSON-RPC peer, implement `a2a.tasks.create` / `a2a.tasks.get`
/// per spec G.6 and replace these stubs.
#[derive(Debug, Clone, Default)]
pub struct JsonRpcTransport;

impl JsonRpcTransport {
    pub fn new() -> Self {
        Self
    }
}

// Shared error message for all three Transport methods on JsonRpcTransport.
// Returning a typed error (rather than `todo!()` panicking) means a caller
// that constructs this variant by mistake gets a recoverable failure instead
// of a process abort. The spec permits JSON-RPC (§13.5) but doesn't require
// an implementation; v1 ships HTTP+SSE only.
const JSONRPC_UNIMPL: &str =
    "JSON-RPC transport (spec §13.5) is Permitted but not implemented in v1; use HTTP+SSE";

#[async_trait]
impl Transport for JsonRpcTransport {
    async fn post_task(
        &self,
        _endpoint: &Url,
        _envelope: &A2aEnvelope,
    ) -> Result<TaskAccepted, A2aError> {
        Err(A2aError::Transport(JSONRPC_UNIMPL.to_string()))
    }

    async fn poll_task(
        &self,
        _endpoint: &Url,
        _task_id: &str,
    ) -> Result<Option<A2aEnvelope>, A2aError> {
        Err(A2aError::Transport(JSONRPC_UNIMPL.to_string()))
    }

    async fn stream_task(
        &self,
        _endpoint: &Url,
        _task_id: &str,
    ) -> Result<EnvelopeStream, A2aError> {
        // JSON-RPC has no native streaming semantics; spec G.6 only defines
        // `a2a.tasks.create` and `a2a.tasks.get`. Return a typed error rather
        // than panic so a misconfigured caller gets a recoverable failure.
        Err(A2aError::Transport(JSONRPC_UNIMPL.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integrity check: constructing a transport must not panic in a normal
    /// test environment. If `reqwest::Client::new()` ever starts requiring
    /// features not compiled in, this catches it early.
    #[test]
    fn http_sse_transport_constructs() {
        let _t = HttpSseTransport::new();
        let _t2 = HttpSseTransport::default();
    }

    #[test]
    fn jsonrpc_transport_constructs() {
        let _t = JsonRpcTransport::new();
        let _t2 = JsonRpcTransport::default();
    }

    #[tokio::test]
    async fn jsonrpc_transport_returns_typed_error_not_panic() {
        // Regression guard: earlier revisions used `todo!()` here, which
        // panicked and aborted the process. The v1 stub must return a typed
        // error so a misconfigured caller gets a recoverable failure.
        use crate::envelope::MessageType;
        let t = JsonRpcTransport::new();
        let endpoint = Url::parse("http://peer.example/").unwrap();
        let env = A2aEnvelope::new(
            "test-brain",
            MessageType::SnapshotRequested,
            serde_json::json!({}),
        );
        let res = t.post_task(&endpoint, &env).await;
        assert!(matches!(res, Err(A2aError::Transport(_))));
        let res = t.poll_task(&endpoint, "id").await;
        assert!(matches!(res, Err(A2aError::Transport(_))));
        let res = t.stream_task(&endpoint, "id").await;
        assert!(matches!(res, Err(A2aError::Transport(_))));
    }

    #[test]
    fn url_joining_matches_spec() {
        // Spec §13.8 / G.6: tasks live under `/a2a/v1/tasks` rooted at the
        // peer's authority. We use absolute-path references so the join works
        // whether the endpoint has a path or not.
        let endpoint = Url::parse("https://peer.example/").unwrap();
        let tasks = HttpSseTransport::tasks_url(&endpoint).unwrap();
        assert_eq!(tasks.as_str(), "https://peer.example/a2a/v1/tasks");

        let task = HttpSseTransport::task_url(&endpoint, "abc-123").unwrap();
        assert_eq!(task.as_str(), "https://peer.example/a2a/v1/tasks/abc-123");

        let events = HttpSseTransport::events_url(&endpoint, "abc-123").unwrap();
        assert_eq!(
            events.as_str(),
            "https://peer.example/a2a/v1/tasks/abc-123/events"
        );
    }

    #[test]
    fn url_joining_strips_endpoint_path() {
        // Regression guard: an earlier version used relative joins without a
        // leading slash. For a path-bearing endpoint like
        // `http://host/a2a/v1/`, that produced doubled-prefix URLs like
        // `http://host/a2a/v1/a2a/v1/tasks`, which 404'd against every live
        // server. The leading `/` pins the join to the authority root.
        let endpoint = Url::parse("http://127.0.0.1:18421/a2a/v1/").unwrap();
        let tasks = HttpSseTransport::tasks_url(&endpoint).unwrap();
        assert_eq!(
            tasks.as_str(),
            "http://127.0.0.1:18421/a2a/v1/tasks",
            "tasks URL must sit at authority root, not under endpoint path"
        );

        let task = HttpSseTransport::task_url(&endpoint, "xyz").unwrap();
        assert_eq!(task.as_str(), "http://127.0.0.1:18421/a2a/v1/tasks/xyz");

        let events = HttpSseTransport::events_url(&endpoint, "xyz").unwrap();
        assert_eq!(
            events.as_str(),
            "http://127.0.0.1:18421/a2a/v1/tasks/xyz/events"
        );
    }

    #[test]
    fn sse_event_parses_single_data_line() {
        // Positive framing: a well-formed event parses cleanly.
        let env = A2aEnvelope::new(
            "test",
            crate::envelope::MessageType::ScoreUpdated,
            serde_json::json!({"score": 50}),
        );
        let json = serde_json::to_string(&env).unwrap();
        let sse_text = format!("data: {json}");
        let parsed = parse_sse_event(&sse_text).unwrap();
        assert_eq!(parsed.brain_id, "test");
    }

    #[test]
    fn sse_event_rejects_empty_data() {
        // Honest failure: empty event is flagged, not silently tolerated.
        let err = parse_sse_event("event: ping").unwrap_err();
        matches!(err, A2aError::InvalidEnvelope(_));
    }

    #[test]
    fn task_accepted_serde_roundtrip() {
        let ta = TaskAccepted {
            task_id: "t-1".into(),
            accepted_at: Utc::now(),
        };
        let s = serde_json::to_string(&ta).unwrap();
        let back: TaskAccepted = serde_json::from_str(&s).unwrap();
        assert_eq!(ta.task_id, back.task_id);
    }
}
