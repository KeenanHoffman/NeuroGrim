//! D2 / BB #11 — Workflow Engine MVP.
//!
//! Multi-tick suspended pipelines. V0 Pipeline Runner is single-tick: a
//! dispatch runs every step in one call. This module adds *suspension*:
//! a leaf-op (or wrapping `Step::Suspend`) can return `Pending(state)`
//! and the runner enqueues the dispatch into a `WorkflowEngine` for
//! resumption on the next tick (or matching wakeup signal).
//!
//! ## V0 MVP scope (per plan §D2 + V0-RETRO §A5 ~300-400 LOC)
//!
//! - **`SuspendedDispatch`** — captures the partial state of a paused
//!   dispatch: broker_id, pipeline_id, params, step cursor, optional
//!   workflow-specific payload, wake_condition.
//! - **`WakeCondition::Tick`** — resume on next tick (simplest condition).
//! - **`WakeCondition::AfterDuration(Duration)`** — resume after a wall-
//!   clock duration elapses.
//! - **`WorkflowEngine`** — in-memory queue of suspended dispatches.
//!   Persistence to cold store is deferred to a follow-on; V0 loses
//!   in-flight workflows on process restart (acceptable for MVP).
//! - **`engine.suspend(SuspendedDispatch)`** — enqueue a dispatch for
//!   later resumption.
//! - **`engine.ready_to_resume()`** — list dispatches whose wake
//!   condition is currently satisfied.
//!
//! The runner integration is INTENTIONALLY thin in V0: the
//! `WorkflowEngine` is exposed as a substrate primitive that broker
//! authors can use to model multi-step workflows manually. A future
//! version (S1-T) wires `Step::Suspend` directly into the runner's
//! execute_step path so brokers can author multi-tick pipelines
//! declaratively in YAML.

use crate::pipeline::{ParamMap, PipelineId};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// V0 wake conditions — minimal set; S1-T extends with event-driven
/// (signal from another broker), state-condition (Overlay-derived), etc.
#[derive(Debug, Clone)]
pub enum WakeCondition {
    /// Resume on the next tick — useful for "let other brokers project,
    /// then continue."
    Tick,
    /// Resume after the given wall-clock duration elapses since suspension.
    AfterDuration(Duration),
}

/// A dispatch that was suspended mid-execution + can be resumed later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspendedDispatch {
    pub broker_id: String,
    pub pipeline_id: PipelineId,
    pub params: ParamMap,
    /// Index of the next step to execute when resumed. V0 single-tick
    /// pipelines step from 0 to N-1; suspension at step K means resume
    /// at K+1 (or K if the leaf-op handles re-entry itself).
    pub resume_at_step: usize,
    /// Optional workflow-specific payload — e.g., the broker's
    /// partial-result state needed to continue from where it left off.
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Suspension trace_id — links the resumed dispatch back to the
    /// original via trace.jsonl.
    pub origin_trace_id: String,
    /// Operator-readable note explaining why this dispatch was suspended
    /// (e.g., "waiting for dashboard precleanup grace period").
    #[serde(default)]
    pub reason: String,
}

/// Internal entry: a SuspendedDispatch + its computed wake-instant.
struct EngineEntry {
    suspension_id: String,
    dispatch: SuspendedDispatch,
    suspended_at: Instant,
    wake_condition: WakeConditionRuntime,
}

/// Runtime form of WakeCondition (Tick has no extra state; AfterDuration
/// captures the suspension instant).
enum WakeConditionRuntime {
    Tick,
    AfterDuration(Duration),
}

/// In-memory queue of suspended dispatches. Persistence to cold store +
/// crash-recovery resumption is deferred to a follow-on.
pub struct WorkflowEngine {
    entries: Mutex<Vec<EngineEntry>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Enqueue a dispatch for later resumption. Returns the engine-assigned
    /// suspension_id (useful for cancellation + diagnostics).
    pub fn suspend(&self, dispatch: SuspendedDispatch, condition: WakeCondition) -> String {
        let suspension_id = format!(
            "susp-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
        );
        let entry = EngineEntry {
            suspension_id: suspension_id.clone(),
            dispatch,
            suspended_at: Instant::now(),
            wake_condition: match condition {
                WakeCondition::Tick => WakeConditionRuntime::Tick,
                WakeCondition::AfterDuration(d) => WakeConditionRuntime::AfterDuration(d),
            },
        };
        self.entries
            .lock()
            .expect("workflow engine poisoned")
            .push(entry);
        suspension_id
    }

    /// Drain dispatches whose wake condition is currently satisfied. The
    /// returned dispatches are REMOVED from the engine; the caller is
    /// responsible for resuming them via PipelineRunner.dispatch (or
    /// equivalent).
    ///
    /// `tick_now` semantics: if true, dispatches with `WakeCondition::Tick`
    /// also drain on this call. Call this from a tick handler with
    /// `tick_now=true`; call with `tick_now=false` for a purely time-driven
    /// drain (e.g., a duration-based timer fires).
    pub fn drain_ready(&self, tick_now: bool) -> Vec<(String, SuspendedDispatch)> {
        let now = Instant::now();
        let mut entries = self.entries.lock().expect("workflow engine poisoned");
        let mut keep = Vec::new();
        let mut ready = Vec::new();
        for entry in entries.drain(..) {
            let is_ready = match &entry.wake_condition {
                WakeConditionRuntime::Tick => tick_now,
                WakeConditionRuntime::AfterDuration(d) => {
                    now.duration_since(entry.suspended_at) >= *d
                }
            };
            if is_ready {
                ready.push((entry.suspension_id, entry.dispatch));
            } else {
                keep.push(entry);
            }
        }
        *entries = keep;
        ready
    }

    /// Cancel a suspended dispatch by id. Returns the cancelled dispatch if
    /// found, None otherwise.
    pub fn cancel(&self, suspension_id: &str) -> Option<SuspendedDispatch> {
        let mut entries = self.entries.lock().expect("workflow engine poisoned");
        let pos = entries.iter().position(|e| e.suspension_id == suspension_id)?;
        Some(entries.remove(pos).dispatch)
    }

    /// Number of suspended dispatches currently queued.
    pub fn suspended_count(&self) -> usize {
        self.entries
            .lock()
            .expect("workflow engine poisoned")
            .len()
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_suspended(reason: &str) -> SuspendedDispatch {
        SuspendedDispatch {
            broker_id: "test".to_string(),
            pipeline_id: "test/multi-step".to_string(),
            params: ParamMap::new(),
            resume_at_step: 1,
            payload: serde_json::json!({}),
            origin_trace_id: "trace-1".to_string(),
            reason: reason.to_string(),
        }
    }

    #[test]
    fn engine_starts_empty() {
        let e = WorkflowEngine::new();
        assert_eq!(e.suspended_count(), 0);
    }

    #[test]
    fn engine_suspend_increments_count_and_returns_id() {
        let e = WorkflowEngine::new();
        let id1 = e.suspend(make_suspended("first"), WakeCondition::Tick);
        let id2 = e.suspend(make_suspended("second"), WakeCondition::Tick);
        assert_eq!(e.suspended_count(), 2);
        assert!(id1.starts_with("susp-"));
        assert!(id2.starts_with("susp-"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn drain_ready_with_tick_now_drains_tick_waiters() {
        let e = WorkflowEngine::new();
        e.suspend(make_suspended("first"), WakeCondition::Tick);
        e.suspend(make_suspended("second"), WakeCondition::Tick);
        // tick_now=false: no Tick-waiters drain
        let drained = e.drain_ready(false);
        assert_eq!(drained.len(), 0);
        assert_eq!(e.suspended_count(), 2);
        // tick_now=true: both drain
        let drained = e.drain_ready(true);
        assert_eq!(drained.len(), 2);
        assert_eq!(e.suspended_count(), 0);
    }

    #[test]
    fn drain_ready_with_after_duration_respects_elapsed() {
        let e = WorkflowEngine::new();
        e.suspend(
            make_suspended("waited"),
            WakeCondition::AfterDuration(Duration::from_millis(50)),
        );
        // Immediately: not ready
        assert_eq!(e.drain_ready(false).len(), 0);
        assert_eq!(e.suspended_count(), 1);
        // After 80ms: ready
        std::thread::sleep(Duration::from_millis(80));
        let drained = e.drain_ready(false);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].1.reason, "waited");
    }

    #[test]
    fn cancel_removes_specific_suspension() {
        let e = WorkflowEngine::new();
        let id1 = e.suspend(make_suspended("first"), WakeCondition::Tick);
        let id2 = e.suspend(make_suspended("second"), WakeCondition::Tick);
        let cancelled = e.cancel(&id1).unwrap();
        assert_eq!(cancelled.reason, "first");
        assert_eq!(e.suspended_count(), 1);
        // Cancelling unknown id returns None
        assert!(e.cancel("susp-99999").is_none());
        // Remaining one drains on tick
        let drained = e.drain_ready(true);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].0, id2);
    }

    #[test]
    fn drain_ready_mixed_conditions_only_drains_ready_ones() {
        let e = WorkflowEngine::new();
        e.suspend(make_suspended("tick"), WakeCondition::Tick);
        e.suspend(
            make_suspended("delayed"),
            WakeCondition::AfterDuration(Duration::from_secs(10)),
        );
        // tick_now=true: only Tick waiter drains; delayed remains
        let drained = e.drain_ready(true);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].1.reason, "tick");
        assert_eq!(e.suspended_count(), 1);
    }
}
