# Broker Framework Backlog (38 building blocks)

The implementation backlog for the broker framework specified across
[`../docs/BROKER-CONTRACT.md`](../docs/BROKER-CONTRACT.md) +
[`../docs/BROKER-INTERNALS.md`](../docs/BROKER-INTERNALS.md) +
[`../docs/BROKER-AWARENESS.md`](../docs/BROKER-AWARENESS.md) +
[`../docs/BROKER-WRAPPING.md`](../docs/BROKER-WRAPPING.md) +
[`../docs/BROKER-MANIFEST-SCHEMA.md`](../docs/BROKER-MANIFEST-SCHEMA.md) +
[`../docs/CLUSTER-MANIFEST-SCHEMA.md`](../docs/CLUSTER-MANIFEST-SCHEMA.md) +
[`../docs/BROKER-FRAMES.md`](../docs/BROKER-FRAMES.md) +
[`../docs/PUBLIC-VS-PROPRIETARY.md`](../docs/PUBLIC-VS-PROPRIETARY.md). 35 main
building blocks + 1 sub-numbered (#22a Materializer Composer).

**Status (2026-06-24, Phase 8):** spec-stable across Phases 1–8 (Phase 8 added 3
new BBs: #36 Agent-Behavior Observability, #37 Pipeline Deprecation Manager, #38
Sensor Quarantine Manager; resolved 11 load-bearing + 14 medium findings from the
Phase 7 audit). Stage mapping reflects cereGrim's S\*-T roadmap branch (S0-T
framework foundation → S1-T Sense+InnateAbility brokers → S2-T Embodiment +
Effectors → S3-T Awareness Service hardening → S\*-C cluster work). Phase 7.5
verification corrected ultra-pass-flagged audit-misreads (LB-2 was a misread; LB-7
deferred to a separate cluster-federation-bootstrap planning session per its
chicken-and-egg open-design status).

**Naming convention:** ticket IDs `BRK-<BB>-<NAME>` (e.g., `BRK-01-BROKER-TRAIT`).
Crate target: `D:\Brains\NeuroGrim\neurogrim\crates\neurogrim-brokers\` (new crate,
sibling of `neurogrim-core`, per the substrate inventory in
[`../../cereGrim/docs/SCAFFOLDING.md`](../../cereGrim/docs/SCAFFOLDING.md)).

---

## Master table — all 35 BBs at a glance

| ID | BB | Layer | Stage | Effort | Depends on |
|---|---|---|---|---|---|
| BRK-01-BROKER-TRAIT | #1 Broker capsule | A | S0-T | M | #4, #6 |
| BRK-02A-OVERLAY | #2a Overlay primitive | A | S0-T | M | #6 |
| BRK-02B-WORKING-STATE | #2b Working state | A | S0-T | S | (none) |
| BRK-03-INTERNAL-SERVICE | #3 Internal Service | A | S0-T | M | #1, #2a, #2b, #6 |
| BRK-04-QUEUE | #4 Queue | A | S0-T | S (reuse) | (reuse `neurogrim_core::queue`) |
| BRK-05-EXTERNAL-SERVICE | #5 External Service | A | S0-T | M | #4, #6 |
| BRK-06-COLD-STORE | #6 Cold Store | A | S0-T | S (reuse) | (reuse SQLite/JSONL trait) |
| BRK-26-SCHEMA-MIGRATION | #26 Schema Migration Runner | A | S0-T | M | #6 |
| BRK-07-PIPELINE | #7 Pipeline type | B | S0-T | S | (Serde derives) |
| BRK-08-STEP | #8 Step type | B | S0-T | S | #7 |
| BRK-09-CATALOG | #9 Pipeline Catalog | B | S0-T | M | #7, #8, #6 |
| BRK-10-RUNNER | #10 Pipeline Runner | B | S0-T | L | #7, #8, #9 |
| BRK-11-WORKFLOW | #11 Workflow Engine | B | S0-T | L | #6, #10, #26 |
| BRK-12-TRACE-SINK | #12 Trace Sink | B | S0-T | M | #4, #9 |
| BRK-13-REPLAY | #13 Replay tooling | B | S0-T | M | #11, #12, #6 |
| BRK-25-CANCEL-HANDLER | #25 Pipeline Cancellation Handler | B | S0-T | M | #10, #11 |
| BRK-34-WPVC | #34 Workflow-Pipeline Versioning Contract | B | S0-T | M | #9, #11 |
| BRK-14-REGISTRY | #14 Broker Registry | C | S0-T | M | #1 |
| BRK-15-TICK | #15 Tick Source | C | S0-T | M | #14 |
| BRK-16-WORKSPACE-MGR | #16 Workspace Manager (trait scaffold) | C | S0-T (trait); S2-T (impl) | L | #1, #14, #4 |
| BRK-17-TOPOLOGY-BROKER | #17 Topology Broker (trait scaffold) | C | S0-T (trait); S1-T (impl) | L | #1, #14, #2a |
| BRK-18-AWARENESS-SVC | #18 Awareness Service | C | S0-T (scaffold); S3-T (hardened) | L | #4, #5 |
| BRK-19-GOVERNANCE | #19 Governance Composer | C | S0-T | M | #10 |
| BRK-20-SKILL-FILTER | #20 Skill Filter | C | S0-T | L | #9, #19 |
| BRK-21-PROPOSAL-LEDGER | #21 Proposal Ledger | C | S0-T | S (reuse) | (reuse `proposal-ledger.json`) |
| BRK-22-HOT-MAT | #22 Hot-Store Materializer | C | S0-T | M | #2a |
| BRK-22A-COMPOSER | #22a Materializer Composer | C | S0-T | S | #22, #24 |
| BRK-23-ROLE-SET | #23 Role-set scaffolding | C | S0-T | M | #14 |
| BRK-24-AWARE-MAT | #24 Awareness Materializer | C | S0-T | M | #9, #20 |
| BRK-27-CROSS-BROKER | #27 Cross-Broker Composition Policy | C | S0-T (contract); S1-T (impl) | L | #17, #25, #11 |
| BRK-28-DIAGNOSTICS | #28 Diagnostics Collector | C | S0-T | M | #4, #12, #10 |
| BRK-29-LIFECYCLE | #29 Broker Lifecycle | C | S0-T | L | #14, #11, #25 |
| BRK-30-ONBOARDING | #30 Onboarding Projection | C | S0-T | M | #22a, #14 |
| BRK-31-CLUSTER-FED | #31 Cluster Federation Topology | C | S0-C | L | #17, IAB substrate |
| BRK-32-TELEMETRY | #32 Operator Telemetry Summarizer | C | S0-T | M | #28 |
| BRK-33-PIPELINE-PROPOSAL | #33 Pipeline Proposal Mechanism | C | S0-T | M | #21, #9 |
| BRK-35-FRAME-STACK | #35 Frame stack | C | S0-T | L | #19, #20, #22, #11 |
| BRK-36-AGENT-BEHAVIOR-OBS | #36 Agent-Behavior Observability | C | S0-T | M | #10, #20, #22a |
| BRK-37-PIPELINE-DEPRECATION | #37 Pipeline Deprecation Manager | C | S0-T | M | #10, #29, #33 |
| BRK-38-SENSOR-QUARANTINE | #38 Sensor Quarantine Manager | C | S0-T (scaffold); S3-T (hardened) | M | #18, #28 |

**Effort tiers (rough):** S = small (<1 day); M = medium (1-3 days); L = large (3-7 days).
These are guesses pre-implementation; refine after first 2-3 BBs land.

---

## Stage mapping summary

### S0-T — Framework foundation (most BBs)

Build the substrate-level framework so any broker can be authored against it.
Per [`../../cereGrim/roadmap/ROADMAP.md`](../../cereGrim/roadmap/ROADMAP.md) S0-T exit:
*"a reference broker can be authored end-to-end in under half a day by the framework
author against a frozen test fixture, with full trace / governance / tunability /
replay surfacing automatically."*

Layer A (8 BBs): #1, #2a, #2b, #3, #4, #5, #6, #26
Layer B (9 BBs): #7, #8, #9, #10, #11, #12, #13, #25, #34
Layer C (18 BBs full or scaffold): #14, #15, #16 (trait), #17 (trait), #18 (scaffold),
#19, #20, #21, #22, #22a, #23, #24, #27 (contract), #28, #29, #30, #32, #33, #35

### S1-T — Sense + InnateAbility brokers (concrete)

- **Concrete Topology Broker** (BB #17 full impl) + Cross-Broker Composition Policy
  enforcement (BB #27 impl)
- 4 Sense broker concretes wrapped over NeuroGrim substrate:
  - Context Broker (wraps `BrainContext::load()`)
  - Workspace Broker (wraps IDE state mirror)
  - Sensory Broker (wraps `.claude/brain/queues/`)
  - Topology Broker (concrete impl)
- Work Broker concrete (wraps `neurogrim_sensory::backlog::next_ready()`)
- Bias-free Search Broker calibration (per
  [`../../NeuroGrim/docs/BROKER-INTERNALS.md`](../../NeuroGrim/docs/BROKER-INTERNALS.md)
  §6 distribution measurement)

### S2-T — Embodiment + Effectors

- **Workspace Manager concrete impl** (BB #16 full)
- IDE Broker (Effector under Workspace Manager)
- Browser Broker (multi-role Sense + Embodiment; v10/v11/v12 browser-band substrate)
- Custom Sensor (afferent Effector)

### S3-T — Awareness Service hardening

- BB #18 hardened to spec (rate-limit + payload validation + redaction + source
  attribution all enforced)
- Custom-sensor extension model validated (deliberately misbehaving sensor doesn't
  poison Awareness Map)

### S0-C onwards — Cluster work

- BB #31 Cluster Federation Topology (cross-cluster federation)
- IAB substrate concretes (S0-C through S3-C per cereGrim's IAB stub)

---

## Per-BB acceptance criteria

### Layer A — Pattern primitives

#### BRK-01-BROKER-TRAIT (BB #1)
**Description:** The `Broker` trait every broker implements. Defines `read_overlay()`,
`legal_pipelines(state)`, `governance_pipelines()`, `tick(WorldEvent)`, role-set
declaration.
**Acceptance:** trait compiles; minimal stub broker (returns empty Overlay + empty
pipeline list) registers + dispatches without panic; trait surface validated against
[`BROKER-INTERNALS.md`](../docs/BROKER-INTERNALS.md) §3 BB #1 column.
**Reuse:** new code; no existing NeuroGrim trait fits this shape.
**Notes:** the trait is the framework's most-referenced surface — keep it minimal and
extend via composition; don't bloat it with default methods.

#### BRK-02A-OVERLAY (BB #2a)
**Description:** Generic `Overlay<T>` — read-only consumer-facing projection with
atomic-swap updates, versioned read, no-torn-read enforcement.
**Acceptance:** atomic-swap demonstrated under concurrent read+write (property test);
no-torn-read enforced; budget overflow triggers `curation-budget-exceeded` alarm per
BROKER-CONTRACT §"Overlay contract" §4 + falls back to last-known-fitting Overlay.
**Reuse:** new code; consider `arc-swap` crate or similar.

#### BRK-02B-WORKING-STATE (BB #2b)
**Description:** Generic `WorkingState<W>` — broker-private full read/write surface
not exposed to consumers.
**Acceptance:** type compiles; access mediated via broker's internal pipelines only
(no public read accessor for consumers).
**Reuse:** new code; trivial wrapper.

#### BRK-03-INTERNAL-SERVICE (BB #3)
**Description:** `InternalService` trait + tick subscription. Projects cold → working
state; broker materializes curated subset into Overlay.
**Acceptance:** trait + tick-subscription wiring + a reference InternalService impl
demonstrating cold→working→overlay projection cycle.
**Reuse:** tick wiring may reuse `neurogrim_core::queue` subscription patterns.

#### BRK-04-QUEUE (BB #4)
**Description:** Reuse `neurogrim_core::queue` for inter-component messaging within
brokers + cross-broker (via Topology).
**Acceptance:** broker framework consumes `neurogrim_core::queue` via published API;
no new queue substrate authored; topic-naming convention documented (`_neurogrim/broker/*`).
**Reuse:** 100% existing.

#### BRK-05-EXTERNAL-SERVICE (BB #5)
**Description:** `ExternalService` trait + queue-consumer scaffold. Ingests world
events into cold store.
**Acceptance:** trait + reference ExternalService that consumes a topic and writes to
cold store.
**Reuse:** `neurogrim_sensory` sensor trait is the closest existing pattern; either
reuse via adapter or borrow shape.

#### BRK-06-COLD-STORE (BB #6)
**Description:** Trait over SQLite/JSONL backends. Schema migration files consumed by
BB #26.
**Acceptance:** trait abstracts SQLite + JSONL backends transparently; per-broker cold
store path declared in manifest; schema version field present on every record.
**Reuse:** `neurogrim_core::queue_backend` is the precedent — same trait shape over
two backends. Likely 80% reuse.

#### BRK-26-SCHEMA-MIGRATION (BB #26)
**Description:** Per-broker `SchemaVersion` field + `SchemaVersionManifest` in cold
store + `MigrationRunner` (Tier 3 bootstrap). Workflow resumption contract under
stale SchemaVersion.
**Acceptance:** failed migration aborts broker startup loudly; idempotency_class
declarations honored (`pure` / `deterministic` / `manual-verification-required`);
workflow checkpoint with old SchemaVersion either auto-migrates forward or refuses
with `failure_reason: schema_forward_migration_unavailable`; partial schema
coexistence window operator-configurable.
**Reuse:** none existing for schema migrations; new code.

---

### Layer B — Pipeline primitives

#### BRK-07-PIPELINE (BB #7)
**Description:** `Pipeline` struct + Serde derives. Carries visibility (Surfaced |
Internal), audit_class (capability | governance | meta-observation), tunability,
params, preconditions, steps, governance, expected_effect, contract_version (BB #34).
**Acceptance:** struct compiles; Serde round-trip preserves all fields; YAML
deserialization from `BROKER-MANIFEST-SCHEMA.md` reference catalog works.
**Reuse:** new struct.

#### BRK-08-STEP (BB #8)
**Description:** `Step` enum — `Leaf(LeafOpId)` | `SubPipeline(PipelineId, ParamMap)`
| `Guard(Predicate, Step)` | `Branch(Predicate, Step, Step)`.
**Acceptance:** enum compiles; YAML deserialization matches spec; sub-pipeline calls
validate (broker_id/pipeline_id format for cross-broker per BB #27).
**Reuse:** new enum.

#### BRK-09-CATALOG (BB #9)
**Description:** Per-broker Pipeline Catalog — YAML loader + hot reload + schema
structure validation at load time + dispatch-time parameter validation.
**Acceptance:** catalog loads from broker's cold-store-relative path; malformed YAML
fails broker startup loudly; missing `description`/`when_to_use` on Surfaced
pipelines fails (per BROKER-AWARENESS §1 routing-signal contract); hot-reload picks
up catalog changes without restart.
**Reuse:** YAML loading via `serde_yaml`; rest is new.

#### BRK-10-RUNNER (BB #10)
**Description:** Pipeline Runner — executes pipelines, tracks state, handles
suspension. The bootstrap layer (Tier 3 plain functions execute the Tier 1/2
pipelines).
**Acceptance:** reference pipeline (one Leaf step, one SubPipeline step) executes
end-to-end; suspension returns `Suspended(resume_token)`; resumption from token works
across simulated process restart.
**Reuse:** new code; bootstrap discipline per BROKER-INTERNALS §1.4.
**Effort: LARGE** — core executor; gates everything Layer C.

#### BRK-11-WORKFLOW (BB #11)
**Description:** Workflow Engine — cold-store-as-truth + hot-store positions + resume.
Single-transaction checkpoints (SQLite tx OR fsync'd JSONL append). Workflow.schema_version
pinned at start.
**Acceptance:** multi-tick workflow checkpoints + resumes correctly; torn checkpoint
detected on simulated mid-write crash + treated as workflow loss; workflow resumption
under stale schema applies forward-migration or refuses cleanly.
**Reuse:** SQLite transaction discipline reuses cold-store backend.
**Effort: LARGE** — workflow atomicity is load-bearing.

#### BRK-12-TRACE-SINK (BB #12)
**Description:** Trace format + write path + replay-against-historical-state harness.
**Acceptance:** every pipeline dispatch produces a trace record; trace records carry
projection_snapshot + dispatch parameters + outcome + audit_class; trace format is
versioned (`schema_version` field).
**Reuse:** invocation-ledger JSONL pattern is the precedent.

#### BRK-13-REPLAY (BB #13)
**Description:** Three replay scopes (single-pipeline / broker-tick / workflow); CLI +
library API; read-only by default with opt-in write-mode for golden regen. Subsumes
test fixtures.
**Acceptance:** `cargo run -- replay --scope single-pipeline --id <pipeline_id>
--state <historical-state-path>` works; replay produces deterministic output for a
known fixture; write-mode requires explicit `--write-golden` flag.
**Reuse:** new CLI surface; library trace-reader.

#### BRK-25-CANCEL-HANDLER (BB #25)
**Description:** Cancellation handler on every pipeline (optional `on_cancel: {steps,
terminal_state, cancellation_depth_max}`). Framework guarantees handler runs even on
kill-switch interrupt.
**Acceptance:** kill-switch fires mid-step → partial output rolled back → on_cancel
runs → workflow transitions per `terminal_state` declaration; `cancellation_depth_max`
prevents handler chain recursion (framework rejects handlers carrying on_cancel
beyond depth-1 by default).
**Reuse:** new; integration point with BB #11 Workflow Engine + BB #19 Governance.

#### BRK-34-WPVC (BB #34)
**Description:** Workflow-Pipeline Versioning Contract — `contract_version: N` per
pipeline + `compatible_contracts: [N, N-1]` per workflow checkpoint + refuse on
mismatch with `failure_reason: contract_version_mismatch`.
**Acceptance:** workflow started with `compatible_contracts: [1]` refuses dispatch
against pipeline `contract_version: 2`; per-broker contract-evolution policy
respected (`allow_backward_compatible_only` | `allow_forward_compatible_upgrades` |
`manual-operator-approval-per-contract`).
**Reuse:** new; integration with BB #11 + BB #9.

---

### Layer C — Substrate composition

#### BRK-14-REGISTRY (BB #14)
**Description:** Broker Registry — discover/load brokers at startup; manifest schema
with role-set declaration.
**Acceptance:** registry loads brokers from operator-declared `brokers_dir` (per
CLUSTER-MANIFEST-SCHEMA); each broker manifest validates against BROKER-MANIFEST-SCHEMA;
duplicate broker IDs fail loudly; role-set declaration drives BB #23 scaffolding
composition.
**Reuse:** new code.

#### BRK-15-TICK (BB #15)
**Description:** Tick Source — hook-driven + file-injection ticks; subscription API.
**Acceptance:** brokers subscribe to tick events they need; PreToolUse hook fires a
tick on demand; file-injection cadence (operator-configurable; default session-start)
fires on schedule.
**Reuse:** Claude Code hook integration via `.claude/settings.local.json` PostToolUse
pattern (existing precedent).

#### BRK-16-WORKSPACE-MGR (BB #16) — trait + spinal-cord defaults in S0-T
**Description:** Workspace Manager — canonical Embodiment broker; Effector
subordination, Workspace Queue, real-time-vs-queued dispatch, cross-effector
synchronization, cross-role sense-feedback routing, `allowed_during_shutdown`
enforcement.
**Acceptance (S0-T):** trait + default Embodiment-role spinal-cord scaffolding;
manifest fields for `shutdown_timeout_per_pipeline_ms` + `hot_swap_allowed` +
`graceful_drain_enabled` validated.
**Acceptance (S2-T concrete):** Effectors register; real-time-vs-queued behavior
validated against simulated LLM busy state.
**Effort: LARGE** — Embodiment is its own coordination story.

#### BRK-17-TOPOLOGY-BROKER (BB #17) — trait + spinal-cord defaults in S0-T
**Description:** Topology Broker — canonical Sense broker; cross-broker routing + ACL
enforcement + per-consumer Topology OverlayView + ACL-mutation self-bypass invariant
(Tier 3 plain functions; Untunable).
**Acceptance (S0-T):** trait + default Sense-role spinal-cord; manifest fields for
initial ACL definitions.
**Acceptance (S1-T concrete):** ACL grants enforced; mutation pipelines (`update-acl`,
`propose-acl-grant`) bypass routing (no infinite regress); cycle-detection
(broker-reachability-analyzer) refuses registration on cycle.
**Effort: LARGE** — ACL discipline is precise.

#### BRK-18-AWARENESS-SVC (BB #18) — scaffold in S0-T; hardened in S3-T
**Description:** Awareness Service — Sensory-Queue rate-limit/sanitize enforcer + the
role-boundary guard for the Sense cross-role write path.
**Acceptance (S0-T scaffold):** scaffold compiles; per-source rate-limit + schema
validation + redaction rules + source-attribution all declared in manifest
(implementation may be partial).
**Acceptance (S3-T hardened):** misbehaving custom-sensor (write-flood, malformed
payload, embedded-secret) contained; Awareness Map shows only safe form; metrics
show drop counts.
**Effort: LARGE**

#### BRK-19-GOVERNANCE (BB #19)
**Description:** Governance Composer — the set of Tier 2 governance pipelines
(`check-trust-budget`, `check-kill-switch`, `arm-kill-switch`, `record-dispatch`,
`record-outcome`, `enforce-rate-limit`); composed into surfaced pipelines via
`GovernancePolicy` field.
**Acceptance:** all 6 framework-provided governance pipelines compile; default
governance composition for Surfaced pipelines applies all 4 base (trust + kill-switch
+ record-dispatch + record-outcome); operator opt-in for extras
(`require-operator-confirmation`).
**Reuse:** new code; integration with BB #10.

#### BRK-20-SKILL-FILTER (BB #20)
**Description:** Skill Filter — rank/dedupe/learn-from-rejection; reachability
channel split (`governance_pipelines()` sidecar separate from `legal_pipelines()`
ranking); per-broker/per-pipeline/per-role hygiene classifier with `audit_class`
filter.
**Acceptance:** governance pipelines exposed via `governance_pipelines()` sidecar (not
in `legal_pipelines()` top-K ranking); per-broker rejection signal tracked; per-role
hygiene detects dead roles after N days (default 30) zero registered brokers.
**Effort: LARGE** — the channel split is load-bearing for agent expressiveness.

#### BRK-21-PROPOSAL-LEDGER (BB #21)
**Description:** Reuse `.claude/brain/proposal-ledger.json` (shipped); add
tuning-pipeline protocol + (per BB #33) `type: pipeline-proposal` entries.
**Acceptance:** broker reads + writes proposal ledger via shipped infrastructure;
tuning pipeline entries surface in Awareness Materializer; operator approval lands
proposal in catalog atomically.
**Reuse:** 100% shipped substrate; extension only.

#### BRK-22-HOT-MAT (BB #22)
**Description:** Hot-Store Materializer — writes per-broker Overlay state to
`.claude/brain/broker/segments/overlay.md`; composed by Materializer Composer (#22a)
into `current-projection.md`.
**Acceptance:** Overlay state serializes to Markdown segment per declared format;
file-injection cadence operator-tunable; segment file written atomically (no torn
reads observable from CLAUDE.md auto-load).
**Reuse:** file injection pattern is well-established; new code for segment format.

#### BRK-22A-COMPOSER (BB #22a)
**Description:** Materializer Composer — concatenates materializer segment files in
operator-declared order into `current-projection.md`. Composition order declared in
cluster manifest.
**Acceptance:** Composer reads ordered segment list from cluster manifest;
concatenates all segments in declared order; atomic write to `current-projection.md`;
missing segment file produces empty section + diagnostic event (not a hard failure).
**Reuse:** new code; trivial concatenation logic.

#### BRK-23-ROLE-SET (BB #23)
**Description:** Role-set scaffolding — per-role defaults the framework wires
automatically on broker registration (Sense / InnateAbility / Embodiment spinal
cords); composes for multi-role brokers.
**Acceptance:** broker registration triggers role-specific scaffolding composition;
multi-role brokers (e.g., `roles: [Sense, Embodiment]`) inherit scaffolding from each
role; conflict between role-scaffolding defaults documented + resolved per role
precedence.
**Effort: MEDIUM** — the registration-time composition is fiddly.

#### BRK-24-AWARE-MAT (BB #24)
**Description:** Awareness Materializer — writes pipeline catalog routing signals
(description + when_to_use per Surfaced pipeline + alive/dead/new hygiene status) to
`.claude/brain/broker/segments/awareness-routing.md`; composed by Composer into
`current-projection.md`.
**Acceptance:** Surfaced pipelines surface description + when_to_use in segment;
alive/dead/new classification per hygiene (BB #20) included; ≤1,536 char per
pipeline routing signal validated.
**Reuse:** new code; segment format mirrors skill manifest format.

#### BRK-11-WORKFLOW-STRESS (P5 test plan, R-O-1 validation)

**Description:** Stress test the Workflow Engine's two-phase-commit ordering
(R-O-1 closure in BB #27). Validates that cross-broker dispatches under
concurrent load preserve atomicity invariant + double-debit ledger consistency.

**Test scenarios:**
1. **Baseline concurrent dispatch** — 10 brokers, 100 concurrent workflows,
   each with 3-step cross-broker compositions. Run for 1000 ticks. **Expected:**
   zero torn checkpoints; debit ledger balanced across all 100 workflows.
2. **Callee-crash-pre-commit injection** — at random intervals during baseline
   load, deliberately crash a callee broker BEFORE its commit lands. **Expected:**
   caller times out per `cross_broker_call_timeout_ms` (cluster manifest); caller
   rolls back locally; parent workflow checkpoint preserves consistency.
3. **Caller-crash-post-callee-commit injection** — callee commits successfully;
   caller process crashes before caller-side commit. **Expected:** on caller
   restart, framework detects unfinalized cross-broker entry; either completes
   the caller-side commit (if callee's state matches expected output) OR issues
   compensating call (callee's `on_compensate` handler runs); debit ledger ends
   balanced.
4. **Network-partition-mid-dispatch injection** (cluster federation scope) — a
   cross-cluster dispatch loses connectivity during the callee's commit window.
   **Expected:** caller times out + retries via configured backoff; or
   `failure_reason: cross_cluster_dispatch_timeout` propagates to parent
   workflow's `on_cancel` handler.

**Pass criteria:** zero workflow data corruption across all 4 scenarios; debit
ledger balanced at end of test; trace records show clean compensation paths
where applicable.

**Tools:** Rust test harness using `tokio` + `proptest` for randomized injection;
mock broker implementations that can be deliberately crashed; replay harness (BB
#13) for post-mortem trace inspection.

**When this runs:** post-S0-T (BB #10/#11/#27 must be implemented); part of S1-T
exit criteria.

#### BRK-04-COLD-STORE-CONTENTION (P6 test plan, R-O-4 validation)

**Description:** Benchmark the per-broker SQLite file isolation default (R-O-4
closure) against the originally-feared shared-file contention scenario.

**Benchmark scenarios:**
1. **Shared SQLite (anti-pattern, for comparison)** — 10 brokers writing to the
   SAME SQLite file at 100 writes/sec each. Measure: per-write latency P50/P95/P99;
   lock-wait-time distribution; throughput ceiling.
2. **Per-broker SQLite (R-O-4 default)** — 10 brokers each with own SQLite file
   at 100 writes/sec. Same metrics. **Expected:** zero lock-wait-time; throughput
   linear in broker count.
3. **JSONL append-only** — 10 brokers writing to per-broker JSONL files at 100
   writes/sec. Same metrics. **Expected:** lowest write-latency (no transaction
   overhead); query-latency higher (per-record scan).
4. **Mixed** — Sensory Broker on JSONL (high throughput), Work Broker on SQLite
   (transactional queries). Measure end-to-end performance under realistic
   broker mix.

**Pass criteria:** per-broker SQLite default eliminates lock contention; mixed
deployment performs predictably; backend-choice guidance documented at the per-BB
level for operators.

**Tools:** Rust criterion benchmark suite; synthetic write/read workloads.

**When this runs:** post-S0-T (BB #6 + #11 must be implemented); part of S1-T
performance validation.

#### BRK-35-FRAME-EVAL-COST (P7 test plan, R-O-5 validation)

**Description:** Microbenchmark Frame stack evaluation cost under nested
sub-pipeline dispatch (R-O-5 latency risk).

**Benchmark scenarios:**
1. **Flat dispatch baseline** — single Surfaced pipeline with no Frame
   inheritance; just `legal_pipelines()` rank evaluation. **Expected:** <500µs
   per dispatch on target hardware.
2. **Single-level Frame inheritance** — pipeline with broker-default Frames;
   cluster manifest declares cluster-default Frames. Measure dispatch latency.
   **Expected:** <800µs per dispatch.
3. **Five-level deep Frame stack** — cluster → broker → role → pipeline →
   dispatch overrides; all 7 Frame types populated at every level. Measure
   dispatch latency. **Expected:** <2ms per dispatch (per R-O-5 mitigation; with
   memoization).
4. **`with_frame:` sub-pipeline modifier** — pipeline step includes
   `with_frame:`; Skill Filter recomputes `legal_pipelines()` under new Frame
   stack. **Expected:** sub-recompute adds <1ms per `with_frame:` step.
5. **Frame-rotation pipeline** — `frame_rotation:` step rotates 5 Frame values;
   load-time budget validation per BB #9. Measure load-time validation cost.
   **Expected:** <5ms catalog-load overhead per rotation pipeline.

**Pass criteria:** under nested + rotated dispatch, per-tick latency stays
within the consuming-project's latency budget (cereGrim's target: <1500ms
end-to-end per turn including LLM inference; substrate should consume <400ms).

**Tools:** Rust criterion benchmark suite; representative Frame stack fixtures.

**When this runs:** post-S0-T (BB #35 implemented; BB #20 Skill Filter
implemented); part of S1-T performance validation.

---

#### BRK-27-CROSS-BROKER (BB #27) — contract in S0-T; impl in S1-T
**Description:** Cross-Broker Composition Policy — atomicity rule (cross-broker
sub-pipelines within single workflow checkpoint); ACL governance via Topology Broker;
trust-budget double-debit (both parties debit; same unit required);
broker-reachability-analyzer cycle detection at startup; extends to cluster-pipeline
composition.
**Acceptance (S0-T contract):** spec documented + YAML schema (`sub_pipeline:
<broker_id>/<pipeline_id>` + `composition_mode`).
**Acceptance (S1-T impl):** cross-broker dispatches enforce ACL + double-debit;
cycle-detection refuses broker registration on cycle; trust-budget unit mismatch
refuses dispatch.
**Effort: LARGE**

#### BRK-28-DIAGNOSTICS (BB #28)
**Description:** Diagnostics Collector — unified live observability (latency
histograms P50/P95/P99 + dispatch counts + success/failure rates + governance-block
frequency + broker-health indicators + trust-budget state per active unit). Emits to
`_neurogrim/diagnostics/{latency,dispatch-stats,broker-health,trust-budget}` with
source-class quota (default 1 emission/sec/broker/topic) + feedback predicate
(meta-observation entries excluded from re-sampling).
**Acceptance:** per-pipeline latency histograms emit at operator-configured cadence;
source-class quota prevents storm-loop (validated with simulated high-frequency
sensor); broker-health indicators detectable when cold-store unavailable.
**Effort: MEDIUM-LARGE**

#### BRK-29-LIFECYCLE (BB #29)
**Description:** Broker Lifecycle — `BrokerShutdown` pipeline (per-pipeline timeout
not global; force-kill + on_cancel after timeout; cluster-pipeline
`allowed_during_shutdown` discipline); `BrokerVersionTransition` (schema-compatible
hot-swap); workflow version pinning at workflow start.
**Acceptance:** graceful shutdown waits per-pipeline timeout then force-kills;
inter-broker shutdown deadlock prevented (per the per-pipeline timeout discipline);
hot-swap preserves in-flight workflows pinned to pre-swap version.
**Effort: LARGE**

#### BRK-30-ONBOARDING (BB #30)
**Description:** Agent-Broker Onboarding Projection — runs once per broker-per-agent
on first registration; surfaces broker purpose + role-set + top-N pipelines +
governance posture + skill-body cross-refs. State persistence in
`<cold-store>/onboarding-registrations.jsonl`; cold-store-reset detection
re-triggers projection.
**Acceptance:** first agent dispatch to a broker injects OnboardingProjection
segment into `current-projection.md`; subsequent dispatches fall back to steady-state
projections; cold-store wipe detected + re-projection on next tick.

#### BRK-31-CLUSTER-FED (BB #31)
**Description:** Cluster Federation Topology — cross-CLUSTER federation (parallel to
BB #27 at within-cluster level); transitive ACL composition; version cascade rules;
bootstrap policy per cluster manifest.
**Acceptance (S0-C):** cluster-A discovers cluster-B's advertised cluster-pipelines
via inter-cluster Topology Broker handshake; transitive ACL refuses non-granted
chains; version cascade rejects mismatched dispatches with `failure_reason:
inter_cluster_version_mismatch`.
**Effort: LARGE** — cross-machine federation is its own coordination story.

#### BRK-32-TELEMETRY (BB #32)
**Description:** Operator Telemetry Summarizer — human-readable broker status summary
(Markdown to `.claude/brain/broker-telemetry-summary.md`). Reads BB #28 + projection
state + audit trail + governance-block frequencies. Sections: per-broker health
snapshot, recent governance decisions, trust-budget consumption, peer-dialogue cycle
state, workflow checkpoint depths. Operator-tunable refresh cadence (default 60s).
**Acceptance:** operator opens summary file; sees current state of all brokers in
human-readable form; cadence respected; markdown auto-loads via CLAUDE.md mechanism.

#### BRK-33-PIPELINE-PROPOSAL (BB #33)
**Description:** Pipeline Proposal Mechanism — extends BB #21 with `type:
pipeline-proposal` entries; LLM (or operator) authors proposal; Awareness Materializer
surfaces pending proposals; operator approval hot-reloads pipeline into catalog.
**Acceptance:** proposal entry schema validates; pending proposals surface in
`.claude/brain/broker/segments/awareness-routing.md`; operator-approved proposal
lands in catalog within one tick + governance compose applies to the new pipeline
immediately.

#### BRK-35-FRAME-STACK (BB #35)
**Description:** Frame stack — typed Frame map; seven canonical Frame types
(Hat/Stakes/Tempo/Mode/Confidence/Audience/Scope); inheritance (cluster → broker →
role → pipeline → dispatch); consumption surfaces (Governance Composer / Skill
Filter / Overlay curation / Workflow Engine); `with_frame:` step modifier;
`frame_rotation:` step sugar with `MaxFrameRotationDepth`; IAB negotiation protocol;
conflict precedence matrix; rotation budget arithmetic per Tempo; coverage-audit
pipeline; extension protocol; L1 awareness injection format.
**Acceptance:** Frame stack threads through all 4 consumption surfaces; inheritance
order validated against per-level overrides; Frame-rotation pipeline expands at load
time + executes correctly; conflict precedence resolves conflicting Frame values per
declared matrix; `active-frame-stack.md` segment surfaces in L1 context with
"Frame Conflicts Resolved" subsection when conflicts active.
**Effort: LARGE** — Frame stack is cross-cutting; touches many other BBs.

#### BRK-36-AGENT-BEHAVIOR-OBS (BB #36)
**Description:** Per-agent action-ledger keyed by `{agent_id, dispatch_id, broker_id,
pipeline_id, outcome, governance_blocks_fired, frame_stack_snapshot}`. Closes the
VISION-principle #21 (agents must perceive their own blind spots) + #22 (agents must
perceive and steward their own work) alignment gap by giving the agent self-observability
over its own behavior (which actions succeeded, failed, were governance-blocked) —
complements BB #20 hygiene (which sees pipelines from the broker side).
**Acceptance:** action-ledger entry written on every Pipeline Runner dispatch (success
/ fail / block); `agent-behavior-summary.md` segment projects per-agent counters; L1
visibility respects retention window (default 7 days hot); redaction rules apply
identically to BB #18; hygiene rollup feeds BB #20 with per-pipeline action density.
**Reuse:** invocation-ledger v3 schema extension (separate keyed-by-agent variant);
Materializer Composer segment slot.
**Effort: MEDIUM**

#### BRK-37-PIPELINE-DEPRECATION (BB #37)
**Description:** Operator-side inverse of BB #33 Pipeline Proposal Mechanism. Cluster
manifest declares `deprecated_pipelines: [{id, effective_date, archive_path, reason}]`;
Pipeline Runner checks before `check-trust-budget`; workflows pinned to pre-effective-date
versions continue, new dispatches refuse with `failure_reason: pipeline_deprecated`.
**Acceptance:** deprecated-pipelines surface in BB #24 Awareness Materializer as a
`retired-pipelines` subsection distinct from dead-pipeline tombstones; archive path
preserves audit trail; composition with BB #29 shutdown ceremony works; BB #33 approval
of replacement pipeline can carry implicit deprecation via `displaces:`.
**Effort: MEDIUM**

#### BRK-38-SENSOR-QUARANTINE (BB #38)
**Description:** Operator surface for isolating + inspecting + restoring misbehaving
custom sensors. Cluster manifest declares `quarantined_sources: [{source_id, reason,
quarantine_date, test_mode_enabled}]`; Awareness Service enforcer routes quarantined
sources to a shadow Awareness Map; operator inspects via `inspect-quarantined-source`
+ restores via `restore-quarantined-source`.
**Acceptance (S0-T scaffold):** quarantine list checked on every Sensory Queue write;
shadow Awareness Map separate from live map; quarantined-source emissions never reach
agent perception; `inspect` + `restore` pipelines Surfaced (operator-only tunability).
**Acceptance (S3-T hardened):** test mode allows operator to validate fix before
restore; chronic-quarantine sensors emit `chronic-sensor-warning` after >3 cycles in
30 days; BB #5 drop-count feed surfaces quarantine candidates.
**Effort: MEDIUM**

---

## Dependency graph (sequencing recommendation)

**First implementation wave (S0-T Layer A + foundational Layer B):**
1. BRK-04-QUEUE + BRK-06-COLD-STORE (reuse-only; quick wins)
2. BRK-07-PIPELINE + BRK-08-STEP (just types)
3. BRK-02B-WORKING-STATE (trivial wrapper)
4. BRK-01-BROKER-TRAIT (depends on #4, #6)
5. BRK-02A-OVERLAY (depends on #6)
6. BRK-26-SCHEMA-MIGRATION (depends on #6)
7. BRK-03-INTERNAL-SERVICE + BRK-05-EXTERNAL-SERVICE (depend on traits above)
8. BRK-09-CATALOG (depends on #7, #8, #6)

**Second wave (Layer B core executor):**
9. BRK-10-RUNNER (large; gates everything downstream)
10. BRK-11-WORKFLOW (large; depends on #10 + #6 + #26)
11. BRK-12-TRACE-SINK (depends on #4 + #9)
12. BRK-13-REPLAY (depends on #11 + #12)
13. BRK-25-CANCEL-HANDLER (depends on #10 + #11)
14. BRK-34-WPVC (depends on #9 + #11)

**Third wave (Layer C foundational):**
15. BRK-14-REGISTRY (depends on #1)
16. BRK-15-TICK (depends on #14)
17. BRK-23-ROLE-SET (depends on #14)
18. BRK-19-GOVERNANCE (depends on #10)
19. BRK-20-SKILL-FILTER (depends on #9 + #19)
20. BRK-21-PROPOSAL-LEDGER (reuse-only)
21. BRK-22-HOT-MAT (depends on #2a)
22. BRK-24-AWARE-MAT (depends on #9 + #20)
23. BRK-22A-COMPOSER (depends on #22 + #24)

**Fourth wave (Layer C composed substrate):**
24. BRK-16-WORKSPACE-MGR (trait only S0-T)
25. BRK-17-TOPOLOGY-BROKER (trait only S0-T)
26. BRK-18-AWARENESS-SVC (scaffold only S0-T)
27. BRK-27-CROSS-BROKER (contract only S0-T)
28. BRK-28-DIAGNOSTICS (depends on #4 + #12 + #10)
29. BRK-29-LIFECYCLE (depends on #14 + #11 + #25)
30. BRK-30-ONBOARDING (depends on #22a + #14)
31. BRK-32-TELEMETRY (depends on #28)
32. BRK-33-PIPELINE-PROPOSAL (depends on #21 + #9)
33. BRK-35-FRAME-STACK (depends on #19 + #20 + #22 + #11)
34. BRK-36-AGENT-BEHAVIOR-OBS (depends on #10 + #20 + #22a)
35. BRK-37-PIPELINE-DEPRECATION (depends on #10 + #29 + #33)
36. BRK-38-SENSOR-QUARANTINE (scaffold only S0-T; depends on #18 + #28)

**S0-T exit gate:** reference broker authored against the above + frozen test fixture
in under half a day by the framework author. Measures the "primitive is real" claim.

**Stage 1+ waves:** S1-T (Topology + Sense + InnateAbility concretes); S2-T
(Workspace Manager + Effectors); S3-T (Awareness Service hardened); S0-C (Cluster
Federation Topology + IAB substrate concretes).

---

## Reuse summary (NeuroGrim substrate inventory; per Phase 1)

| What we reuse | From | BB consumer |
|---|---|---|
| `neurogrim_core::queue` (JSONL + SQLite-backed append-only) | shipped | BB #4 |
| `neurogrim_core::queue_backend::QueueBackend` trait | shipped | BB #6 |
| `neurogrim_core::sensor` factory pattern | shipped | BB #5 (External Service shape) |
| `neurogrim_core::llm_backend::LlmBackend` trait | shipped | adjacent to BB #3 (Internal Service for cognition) |
| `neurogrim_mcp::context::BrainContext::load()` | shipped | S1-T Context Broker wrapper |
| `neurogrim_sensory::backlog::next_ready()` | shipped | S1-T Work Broker wrapper |
| `neurogrim-a2a::TaskServer` trait | shipped | broker A2A endpoint integration |
| `.claude/brain/proposal-ledger.json` | shipped | BB #21 (extend, don't replace) |
| `.claude/brain/invocation-ledger.jsonl` | shipped | BB #20 (capability-hygiene scope; extended in BROKER-AWARENESS §3 with `type: pipeline`) |
| PostToolUse hook pattern | shipped | BB #15 (Tick Source variant) |
| `pulldown-cmark` + similar markdown utilities | shipped | various spec parsers |

---

## Open follow-ons (out of this backlog; future planning)

- **Capability-hygiene scorer code extension** for `type: "pipeline"` rows +
  `audit_class` filtering — flagged in Phase 2 plan; net-new code in
  `neurogrim-sensory`; file separately when S0-T closes.
- **Operator-facing CLI for broker framework** — `neurogrim broker <list | inspect |
  reload | shutdown | telemetry>` — deferred to S1-T calibration once broker
  inventory has 2+ brokers worth listing.
- **A2A pipeline-dispatch protocol** for cross-machine cluster work — load-bearing for
  S\*-C; file when S0-C begins.
- **`wrap_sensor_as_broker!` macro** (BB #5-adjacent) — defer until reference sensor
  wrapper lands and boilerplate is empirically visible.
- **Externally-authored Search Broker calibration** — per BROKER-INTERNALS §6 + the
  S1-T calibration item; not in this backlog (it's a calibration measurement, not a
  BB).
- **v4 diagram drawio.svg update** — operator authoring task; spec at
  `docs/diagrams/DIAGRAM-V4-SPEC.md`.

---

## Cross-references

- [`BACKLOG.md`](BACKLOG.md) — broader project backlog; B-55 entry points at this doc.
- [`../docs/BROKER-CONTRACT.md`](../docs/BROKER-CONTRACT.md) — the named-primitive
  contract.
- [`../docs/BROKER-INTERNALS.md`](../docs/BROKER-INTERNALS.md) — framework framing +
  35 BBs detailed.
- [`../docs/CLUSTER-MANIFEST-SCHEMA.md`](../docs/CLUSTER-MANIFEST-SCHEMA.md) — cluster
  manifest schema (referenced by BB #29 / #31 / #34 / #35).
- [`../docs/diagrams/DIAGRAM-V4-SPEC.md`](../docs/diagrams/DIAGRAM-V4-SPEC.md) —
  visual reference spec.
- [`../../cereGrim/roadmap/ROADMAP.md`](../../cereGrim/roadmap/ROADMAP.md) — S\*-T
  staging branch + S\*-C cluster branch.
- [`../../cereGrim/docs/BROKER-COMPOSITION.md`](../../cereGrim/docs/BROKER-COMPOSITION.md)
  — consuming-project composition over the 35 BBs.
