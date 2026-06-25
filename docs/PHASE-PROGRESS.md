# IDE Full-Lift Plan — Phase Progress Tracker

**Plan source:** `C:\Users\koff0\.claude\plans\for-your-new-session-modular-pretzel.md`

**Scope honesty:** The full plan is **12-15 weeks single-developer** (50-64 days
recalibrated per ultra-pass). This document tracks what's actually shipped
vs what remains, so any session can pick up cleanly without re-deriving status.

---

## Phase A — Substrate prerequisites (COMPLETE)

All 16 Phase A items shipped + exit gate verified.

| # | Item | Status | Commit |
|---|---|---|---|
| Hygiene | Backlog cleanup (U7-U9) + B-62/63/64 filing | ✅ Shipped | `ba80741` (eco), Phase A bundle |
| A1 | C7+C8 governance fixes | ✅ Shipped | `fe2e8c3` |
| A1.5 | `bypasses_kill_switch` field (B-64) | ✅ Shipped | `fe2e8c3` |
| A2 | Work Broker ↔ next_ready() | ✅ Shipped | `fe2e8c3` |
| A4 | Governance composition slots (U4) | ✅ Shipped | `974b66d` |
| A6a | `BrokerHost` extract (B-62) | ✅ Shipped | `fe2e8c3` |
| A7 | Rate-limit subgate | ✅ Shipped | `974b66d` |
| A8 | System-pressure subgate + provider trait | ✅ Shipped | `a1776b3` |
| A9 | Capability matcher subgate + registry trait | ✅ Shipped | `a1776b3` |
| A10 | Per-broker materializer budget allocation | ✅ Shipped | `974b66d` |
| A11 | JSON Schema param validation (lightweight subset) | ✅ Shipped | `0cb0dbd` |
| A12 | Cluster-manifest schema evolution | ✅ Shipped | `0cb0dbd` |
| A13 | Frame stack BB #35 | ✅ Shipped | `0cb0dbd` |
| A14 | `Visibility::AuditOnly` variant (U6) | ✅ Shipped | `fe2e8c3` |
| A15 | broker_demo_driver regression test (U15) | ✅ Shipped | `fe2e8c3` |
| Exit | BROKER-HARNESS-DEMO.md re-run end-to-end | ✅ Verified | `0cb0dbd` |

**Test count:** 66 (V0 baseline) → 101 lib tests + 2 harness + 2 host = **105 tests**, all green.

---

## Phase B — Proof-of-concept IDE broker (SUBSTRATE-SIDE COMPLETE; IDE-SIDE DEFERRED)

LocalAwareness chosen as PoC. Substrate-side broker + tests shipped.
IDE-side Tauri integration deferred to IDE-repo session.

| # | Item | Status | Notes |
|---|---|---|---|
| B1 | `LocalAwarenessBroker` (Sense role) | ✅ Shipped | `src/local_awareness_broker.rs`; 5 tests |
| B2 | Two-write coherence + fault-injection test | ✅ Shipped | `b2_failed_disk_write_leaves_overlay_unchanged` |
| B4 | R-O-4 isolation 10-broker bench re-verify | ✅ Verified | Wave 5.5b's `jsonl_concurrent_writes_across_brokers_dont_interfere` |
| **B3** | **IDE round-trip via Tauri IPC** | 🔵 IDE-repo work | Needs `InProcessBrokerHost` wired into Tauri main.rs |
| **B5** | **Adversarial review of Phase A surfaces** | 🔵 IDE-repo work | Pair with B3 since lift exercises both |

**IDE repo:** `D:\local-pc-operational-management\children\neurogrim-ide\src-tauri\`

---

## Phase C — Bulk full-lift (SUBSTRATE-SIDE C1 PARTIAL; IDE-SIDE DEFERRED)

Plan §C is mostly IDE-side authoring (each sub-phase migrates an IDE
subsystem onto the substrate). Substrate-side adapters needed at C1.

| # | Item | Status | Notes |
|---|---|---|---|
| C1 (substrate) | `TraceSink::append_external` for unified audit | ✅ Shipped | Allows non-broker components to write to the same JSONL |
| C1 (IDE) | process_broker/session_broker library wiring | 🔵 IDE-repo work | Call TraceSink::append_external on spawn |
| C2 | `browser-kill-switch-broker` | 🔵 IDE-repo work | |
| C3 | `browser-quotas-broker` (uses A7 RateLimitSubgate) | 🔵 IDE-repo work | |
| C4 | `browser-admission-broker` (uses A8 SystemPressureSubgate) | 🔵 IDE-repo work | |
| C5 | Capability + batch-approval brokers (uses A9 CapabilitySubgate) | 🔵 IDE-repo work | |
| C6 | `agent-permission-tokens-broker` | 🔵 IDE-repo work | |
| C7 | `agent-self-awareness-broker` | 🔵 IDE-repo work | |
| C8 | `browser-overlay-broker` (IDE-only; uses AuditOnly visibility from A14) | 🔵 IDE-repo work | |
| C9 | IdeAction 40+ variant consolidation (THE BIG ONE) | 🔵 IDE-repo work | Build scaffolder first per plan; 8-12d realistic |
| C10 | Dead-code removal sweep | 🔵 IDE-repo work | One release cycle after each sub-phase |

---

## Phase D — Substrate completion (COMPLETE — all 3 primitives shipped)

| # | Item | Status | Notes |
|---|---|---|---|
| D1 | BB #27 cross-broker `sub_pipeline:` composition | ✅ Shipped | `validate_catalog_with_policy(CrossBrokerPolicy::Allow)` + `PipelineRunner::set_registry()`; runner routes cross-broker sub_pipeline through registry lookup |
| D2 | BB #11 Workflow Engine MVP (suspended-dispatch primitive) | ✅ Shipped | `src/workflow.rs`: `WorkflowEngine` + `SuspendedDispatch` + `WakeCondition::{Tick, AfterDuration}`; brokers explicitly suspend + resume on tick. Runner-level integration (declarative `Step::Suspend`) deferred to S1-T |
| D3 | BB #20 Skill Filter primitive | ✅ Shipped | `src/skill_filter.rs`: `SegmentRanker` trait + `NoOpRanker` default + `RankerContext` (hat/posture/task); operators register a ranker to enable top-K segment selection. Composer integration deferred to S2-T |

**D1 design decisions made (U18 closure):**
- Outer dispatch's governance covers whole sub-pipeline graph; callee
  pipeline's governance does NOT re-fire on cross-broker dispatch (no
  double-debit). This is V0 default per plan §D1.
- `parent_trace_id` field on TraceRecord NOT yet added — trace correlation
  for cross-broker dispatches is deferred to BB #28 Diagnostics. V0 trace
  shows each dispatch as standalone; child dispatches have their own
  trace_id but no explicit parent reference. Future enhancement.
- Cycle detection NOT yet implemented — Tarjan SCC over the new dependency
  edges would land in catalog loader. V0 trusts the operator-authored
  catalog; runtime would hit stack overflow on a cycle. Acceptable risk
  for V0 since cluster manifests are small + operator-curated.

---

## Honest realistic next-session plan

**Most-impactful next sessions, in order:**

1. **IDE-side Phase B3 + Phase C** (multi-session, weeks of work) — the actual
   bulk migration. Requires the operator to work in the IDE repo
   (`D:\local-pc-operational-management\children\neurogrim-ide\src-tauri\`)
   with full IDE context. Each Phase C sub-phase (C2-C10) is its own session
   anchor.

2. **D2 BB #11 Workflow Engine** (single session, ~3 days) — substrate
   addition; unlocks multi-tick patterns (dashboard precleanup→spawn→sensor;
   multi-turn batch approval; long-running automations). Pure substrate
   work, lands cleanly without IDE context.

3. **D3 BB #20 Skill Filter** (single session, ~4-5 days) — substrate
   addition; proper answer to materializer scale at very large broker
   counts. Per plan, may slip to S2-T; A10's per-broker budget
   allocation holds the line for IDE-scale (8-12 brokers).

**Phase A pre-execution gates** (still locked-in from plan acceptance):
- Frame stack promoted to Phase A blocker ✅ honored (A13 shipped)
- process_broker library path locked-in ✅ honored (C1 substrate adapter shipped)
- D6 (C7+C8) governance fixes are A1 hard prerequisite ✅ honored (A1 shipped)
