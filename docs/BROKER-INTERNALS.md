# NeuroGrim — Broker Internals

> **Status: DRAFT until 6 months post-launch (R-X-14 closure, Phase 9).** This spec
> is feature-complete at the named-primitive level (38 BBs across 3 layers, Phase 8)
> but no reference implementation has run yet. First implementation will surface
> spec gaps; the discovered-gap ledger lives at
> [`BROKER-SPEC-GAPS.md`](BROKER-SPEC-GAPS.md). Each ratified patch lands in this doc
> + companions; backward-incompatible patches bump the spec's contract version.
> Until 6 months of production use across ≥1 consumer deployment, this document is a
> draft contract; consumers adopt at their own risk and contribute gap discoveries.

The companion to [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md). The contract pins the **shape**
of a broker — the capsule, the hot/cold tiers, the queue as placement boundary, the
canonical brokers (see CONTRACT [`#canonical-broker-list`](BROKER-CONTRACT.md#canonical-broker-list)),
the terminal-level reduced form, the Workspace Manager role, the Sensory-Queue enforcer.
This document pins the **interior** — what's *inside* a broker that makes it a broker
and not just a function returning a list.

Consuming-project-specific design articulation (e.g., cereGrim's rationale for adopting
the broker pattern in a dual-lobe harness) lives in those projects' own documentation
(e.g., `../../cereGrim/thesis/`, proprietary to that subproject). The IP-boundary policy
+ enumerated thesis-grade claims live in
[`PUBLIC-VS-PROPRIETARY.md`](PUBLIC-VS-PROPRIETARY.md). The framework presented here is
**substrate infrastructure** that NeuroGrim provides publicly.

---

## The framework framing

**NeuroGrim *is* the broker framework. The canonical brokers — Context / Workspace
/ Sensory / Topology (`[Sense]` role), Work (`[InnateAbility]` role), Workspace Manager
(`[Embodiment]` role) with IDE / Browser / Custom Sensor as Effectors subordinate to
it — are its first consumers across the role-set composition.**

The framework primitive (6-piece LLM-level pattern, 3-piece terminal pattern, Pipeline
as universal unit, Overlay contract, governance composition, tunability tiers) is
**uniform across roles**. What varies per role is the **coordination scaffolding** the
framework offers around the primitive — see
[`BROKER-CONTRACT.md`](BROKER-CONTRACT.md#broker-roles--composable-role-set-one-framework-primitive)
for the role-set taxonomy. A new Sense-role broker inherits Sense-role coordination
(LLM-read-only Overlays, sanctioned cross-role write path via Sensory Queue + Awareness
Service enforcer, broadcast subscriptions); a new Embodiment Effector inherits Effector
subordination machinery (registers with the Workspace Manager, queues via
the Workspace Queue); a new Innate Ability broker inherits escalation paths and
dispatch chains. The framework knowing about roles is what makes new-broker
authorship a half-day-class exercise across all role-sets — single-role and multi-role
alike.

This is a deliberate inversion of the obvious path. The obvious path is: write the
canonical brokers as bespoke modules, then notice the duplication, then refactor into
a framework. We're choosing the framework-first path because:

1. **The shape is already known.** [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) pinned it.
   Building canonical brokers as bespoke shapes means committing to multiple bespoke
   shapes and reconciling them later. Framework-first means committing to one shape, once.
2. **NeuroGrim already provides ~70% of the framework's substrate.** The bus, the
   queue backends, `BrainContext`, the shipped backlog broker, the sensor framework,
   the proposal ledger, the LLM backend trait — all of these are framework-shaped
   primitives that just need to be composed under the broker pattern.
3. **Authoring a new broker should be a declarative exercise, not a sprint.** Quick + easy
   broker authorship is what makes the broker pattern a *primitive* rather than a
   *pattern you reach for occasionally*. The framework is what makes the difference.

A NeuroGrim broker author should write:
- a **cold-store schema** (what the broker manages),
- a **YAML pipeline catalog** (surfaced + internal pipeline definitions),
- **leaf-op step bodies** in Rust (the actual work the pipelines do),
- a **manifest** registering the broker with NeuroGrim.

Everything else — tick handling, hot-store projection, audit trail, governance composition,
tunability scaffolding, replay infrastructure, the Pipeline Runner itself — is inherited
from the framework. The full mapping is in [§3 Building blocks](#3-building-blocks).

---

## 1. The Pipeline primitive

### 1.1 The semantic-weight definition

A clean operational definition:

> **A pipeline is an operation whose *execution itself* carries semantic weight in the
> broker's behavior story — not just its return value.**

If you'd want to replay it, kill it, audit it, tune around it, see it in a trace, or
have it survive a crash, it's a pipeline. If the only meaningful output is its return
value and re-running it produces no new information, it's a plain function.

This test is sharp enough to use as a triage rule: for any candidate operation in a new
broker, ask whether its *occurrence* belongs in the broker's behavior story. The answer
determines whether it's a Pipeline (in the catalog, runs through the runner) or a plain
Rust function (called from inside a pipeline step's body).

### 1.2 The three-tier layering

```
  ┌─────────────────────────────────────────────────────────────────┐
  │  TIER 1 — SURFACED PIPELINE                                     │
  │  ─ LLM sees + picks; operator audits                            │
  │  ─ default: full governance (kill-switch, trust budget, replay) │
  │  ─ default: operator-tunable; LLM-tunable via proposal ledger   │
  │                                                                 │
  │  Examples: dispatch-work-unit · open-pane · read-current-page   │
  │            propose-pipeline-deprecation · tune-skill-weight     │
  └─────────────────────────────────────────────────────────────────┘
                              │  composes via sub-pipeline steps
                              ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │  TIER 2 — INTERNAL PIPELINE                                     │
  │  ─ broker plumbing; LLM doesn't see by default                  │
  │  ─ default: traced but not audited; governance opt-in           │
  │  ─ default: operator-tunable; LLM cannot tune directly          │
  │                                                                 │
  │  Examples: parse-backlog · rank-by-tier · project-cold-to-hot   │
  │            evaluate-precondition · enforce-governance           │
  │            resume-suspended-workflow                            │
  └─────────────────────────────────────────────────────────────────┘
                              │  composes via leaf-op steps
                              ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │  TIER 3 — PLAIN FUNCTION  (NOT in the Pipeline Catalog)         │
  │  ─ pure transformations; atomic reads; bootstrap-layer code     │
  │  ─ no trace, no governance, no tunability                       │
  │  ─ just Rust functions called from pipeline step bodies         │
  │                                                                 │
  │  Examples: serialize-pipeline-to-json · compare-priority-ints   │
  │            read-counter-cell · the pipeline runner itself       │
  │            hash-pipeline-id · now()                              │
  └─────────────────────────────────────────────────────────────────┘
```

Tier 1 and Tier 2 share the `Pipeline` type, live in the Pipeline Catalog, and run
through the Pipeline Runner. Tier 3 is plain Rust — never enters the catalog, never
gets a trace, never has a `visibility` flag because the concept doesn't apply.

The `visibility` field is **promotable both ways at runtime.** An internal pipeline can
be promoted to surfaced for a debugging session (the LLM and operator see every dispatch);
a surfaced pipeline can be demoted to internal when it's been operating cleanly for long
enough to drop from the operator's attention budget. The broker can auto-promote on
anomaly (an internal pipeline that starts failing becomes surfaced for visibility).

### 1.3 Pipeline is a value, not a handle (resolved)

`Pipeline` is a serializable struct passed to consumers, not a handle the broker resolves
on dispatch. Three reasons this is the right call:

1. **Replay** — pipelines stored in the audit trail must be runnable against historical
   state. A handle that depends on broker-internal state at dispatch time can't be
   replayed cleanly.
2. **Introspection** — surfaced tuning pipelines (the LLM tuning broker internals)
   target other pipelines as *data*: "demote this pipeline I haven't picked," "promote
   this internal pipeline so I can see what it's doing." Pipelines-as-handles make this
   hard; pipelines-as-values make it natural.
3. **Composition** — a pipeline step that includes another pipeline gets the full
   definition serialized inline, not a handle that may have been edited since.

Conceptual shape (illustrative; the framework will refine):

```rust
struct Pipeline {
    id: PipelineId,
    visibility: Visibility,            // Surfaced | Internal
    tunability: Tunability,            // Untunable | OperatorOnly | OperatorConfirmed | Autonomous
    params: ParamSchema,               // typed parameter shape (may be empty)
    preconditions: Vec<Predicate>,     // checked against hot store
    steps: Vec<Step>,                  // ordered sequence
    governance: GovernancePolicy,      // composed-in pipelines (kill-switch, trust budget, ...)
    expected_effect: EffectClass,      // for idempotency reasoning + audit grouping
}

enum Step {
    Leaf(LeafOpId),                    // calls Rust code (Tier 3)
    SubPipeline(PipelineId, ParamMap), // composes (Tier 1 or 2)
    Guard(Predicate, Box<Step>),       // run step if predicate holds
    Branch(Predicate, Box<Step>, Box<Step>),  // if/else over hot store
}
```

A `Pipeline` is `Serialize + Deserialize + Clone`. It can live in YAML on disk, in the
Pipeline Catalog in RAM, in the audit ledger after dispatch, in a proposal-ledger entry
the LLM has proposed for tuning. One type, many homes.

**Parameter validation runs at dispatch time, not load time.** The Pipeline Catalog
loader (BB #9) validates *schema structure* on load (correct field types, required
fields present in the catalog YAML). Dispatch validates *parameter conformance to the
schema* — when an LLM (or a sub-pipeline composition) dispatches the pipeline with
specific param values, the framework validates each value against the declared
schema. Invalid dispatch:
- The pipeline rejects (does not run).
- The rejection records an entry in the invocation ledger with `audit_class: capability`
  + `failure_reason: param_validation` + the specific field(s) that failed.
- The caller can re-dispatch with corrected params; previous dispatch is a clean abort
  (no side effects, no workflow state change).

This contract closes the gap where parameter sourcing (`state-fill` or `model`-supplied
per [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md)) might produce values the
pipeline's preconditions or step bodies can't handle — validation catches it before
governance composition runs (no wasted trust-budget on doomed dispatches).

### 1.4 The bootstrap layer

Some code has to exist *before* the Pipeline Runner. You can't run the Pipeline Catalog
loader as a pipeline because the runner doesn't have pipelines yet. The bootstrap layer
is Tier 3 plain functions:

- The Pipeline Runner itself (executes pipelines, tracks state).
- The Pipeline Catalog loader (reads YAML from cold store at startup).
- The Hot Store initializer (allocates the in-RAM cells).
- The Cold Store opener (SQLite/JSONL handle).

These are infallible-or-panic at startup. Their *occurrence* doesn't belong in the
broker's behavior story because if they fail, the broker doesn't exist. Bootstrap
failure is a hard exit, not an audit entry.

Once bootstrap completes, the first internal pipeline runs (typically `project-cold-to-hot`
to populate the initial hot store), and from that moment on, everything goes through the
pipeline tier.

**Tier 3 minimization discipline (R-S-15 closure, Phase 9).** Tier 3 plain functions
run before any governance pipeline can stop them — kill-switch is not yet armed, trust
budget is not yet enforced, trace sink may not be initialized. This is the framework's
highest-trust layer and therefore its largest supply-chain attack surface: a compromised
Tier 3 function executes with effective root privilege across every deployment.

**Rule:** every candidate Tier 3 function must be justified as **unable to run as a
pipeline**, not merely "simpler to write as a plain function." The four functions listed
above pass the test (the Pipeline Runner literally cannot dispatch itself; the catalog
loader has no catalog to consult). New Tier 3 additions require a written justification
in the proposing PR — operator review confirms there is no Tier 2 path. Tier 3 size
is the framework's audited attack surface; every line added is reviewed for that reason.

**Future hardening** (deferred to B-58 — Bootstrap Supply-Chain Signing): signed-release
process for NeuroGrim binaries + deterministic bootstrap verification against a
pre-computed snapshot at startup. Spec-level discipline (this rule) closes the
minimization gap now; cryptographic hardening lands when ecosystem signing
infrastructure is ready.

---

## 2. The internal sub-systems, as pipelines

The four sub-systems sketched in the earlier visionary pass — Pipeline Catalog, Workflow
Engine, Skill Filter, Governance Layer — aren't four engines. They're four roles played
by pipelines (with bootstrap support from Tier 3).

### 2.1 Pipeline Catalog

The typed registry of every pipeline this broker can emit, before legality is checked.
The Catalog is *data* (loaded from YAML in the Skills & Config cold store at startup),
not code. It contains both Tier 1 and Tier 2 entries uniformly.

The framework provides:
- The catalog loader (Tier 3 bootstrap).
- A catalog-as-pipeline view: `read-catalog` is itself a Tier 2 pipeline, so the LLM
  can introspect the catalog through a surfaced wrapper if the operator opts in.
- Schema validation at load time (malformed pipeline → broker startup fails loudly).
- Hot reload (catalog file changes → `reload-catalog` internal pipeline runs).

The broker author provides: the YAML/TOML catalog file itself.

### 2.2 Workflow Engine — cold-store-as-truth (resolved)

A workflow is a pipeline whose steps may suspend. Examples: a multi-tick browser
automation, a long-running file edit sequence, a backlog grooming session that the
operator pauses mid-stream.

**Workflow state lives in the cold store.** The Internal Service projects active
workflow positions into the hot store at each tick. If the broker crashes mid-workflow,
the cold store remembers; on restart, the Internal Service resumes from there. This
makes context compactions, broker restarts, and host crashes all survivable without
loss of place.

**Checkpoint atomicity is mandatory.** Each workflow checkpoint (workflow ID + current
step + step inputs + accumulated outputs + suspension reason + parent trace ID) must
land as a **single transaction** — one SQLite tx for SQLite-backed cold stores, one
fsync'd append for JSONL-backed cold stores. Torn checkpoints (partial writes from a
mid-tick crash) are not survivable and are treated as **workflow loss** with an
auditable failure entry, not silent recovery. The framework provides the transaction
discipline; the broker author writes the checkpoint payload but never assembles it
across multiple writes.

The framework provides:
- The `WorkflowEngine` (Tier 3 bootstrap + a set of Tier 2 pipelines for
  suspend / resume / fail / rollback).
- The cold-store schema for workflow state (workflow ID, current step,
  step inputs, step outputs accumulated so far, suspension reason, parent
  pipeline trace ID) with the single-transaction checkpoint guarantee.
- Suspension primitives in `Step` (steps can return `Suspended(resume_token)`).
- Resume-on-tick: when the Internal Service projects, it checks for resumable
  workflows whose suspension condition has cleared.

The broker author provides: just author pipelines that span ticks; the engine handles
the rest.

### 2.3 Skill Filter

The ranker + dedupe + prioritizer that runs every tick to produce `legal_pipelines(state)`.
Hard filter (legality from preconditions) followed by soft ranking (fit + recency +
learn-from-rejection).

The framework provides:
- A generic `rank-legal-pipelines` Tier 2 pipeline (operator-tunable; autonomous-tunable
  within bounded weights).
- A "rejection signal" tracker (the LLM's choices over time — which legal pipelines did
  it pick, which did it skip when offered, which did it pick and the result was rolled
  back). Stored in the broker's cold store; weights derived from it.
- A `capability-hygiene`-style classifier (alive/dead/new) reusing NeuroGrim's existing
  invocation-ledger pattern, scoped **per-broker per-pipeline AND per-role**. The
  per-role scope catches dead roles (a role with zero registered brokers for N days is
  flagged dead so its spinal-cord scaffolding can be retired) — without it the framework
  would carry unused role-scaffolding code + config indefinitely.
- An **`audit_class`** field on every Pipeline definition: `capability | governance |
  meta-observation`. The classifier reads ledger entries with `audit_class: capability |
  governance` only; `meta-observation` entries (hygiene-scoring dispatches, trace-sink
  reads, ledger-introspection pipelines) are excluded from the feed they themselves
  consume. Closes the self-referential loop where a pipeline that observes the ledger
  inflates its own apparent aliveness by being dispatched.
- Operator-defined weight overrides in YAML.

The broker author provides: weight defaults + tuning policy (which weights are
operator-only, which are autonomous-tunable, what the autonomous bounds are).

### 2.4 Governance Layer

Not a separate engine — a set of Tier 2 pipelines that the framework provides, composed
*into* surfaced pipelines via the `GovernancePolicy` field.

Framework-provided governance pipelines:
- `check-trust-budget(pipeline_id)` — refuses if over-budget; Tier 2; usually composed
  as the first step in any surfaced pipeline.
- `check-kill-switch(scope)` — refuses if armed; scopes are per-pipeline, per-broker,
  per-band, global.
- `arm-kill-switch(scope)` — surfaced pipeline; operator-only tunability.
- `record-dispatch(pipeline_id, params, projection_snapshot)` — Tier 2; writes the
  audit anchor. Composed at the start of every surfaced pipeline.
- `record-outcome(pipeline_id, outcome)` — Tier 2; written at the end.
- `enforce-rate-limit(source, limit)` — Tier 2; used by the Awareness Service to
  enforce the Sensory-Queue contract from `BROKER-CONTRACT.md` §3.
- `reset-trust-budget(scope)` (P-8) — Tier 2 internal; surfaced as
  `propose-trust-budget-reset` for operator review per the proposal-ledger ceremony.
  On operator approval: records a ledger entry with `audit_class: governance` +
  previous budget + reset reason, atomically resets the budget, emits to BB #28
  Diagnostics with source-class `budget-reset`, logs to BB #32 Operator Telemetry
  with drift detection (≥3 resets per scope in 24h emits a `frequent-budget-reset`
  warning — signal of miscalibration or runaway loop). Operator-only tunability; no
  LLM involvement in the reset itself.

A surfaced pipeline's `GovernancePolicy` declares which of these are composed in.
Defaults: every surfaced pipeline gets `check-trust-budget` + `check-kill-switch` +
`record-dispatch` + `record-outcome` automatically. Operators can declare additional
governance pipelines per surfaced pipeline (e.g., `require-operator-confirmation` for
high-stakes actions).

The broker author provides: governance policy declarations per pipeline in YAML.

---

## 3. Building blocks

The full framework surface, split by what NeuroGrim provides vs. what a new-broker
author provides. Thirty building blocks across three layers.

**Cross-reference convention:** other documents reference building blocks as
`[BB #N](BROKER-INTERNALS.md#layer-X-anchor)` where the anchor lands on the relevant
Layer A/B/C section (Markdown auto-generates anchors from these headers:
`#layer-a--pattern-primitives-architectural-shapes`,
`#layer-b--pipeline-primitives-universal-unit`,
`#layer-c--substrate-composition-cross-broker-glue`). The numeric BB identifier (e.g.,
`#27`) is the canonical reference; the anchor lands the reader on the correct table to
look it up. This keeps cross-refs stable even if individual BB rows are reorganized
within a Layer's table.

### Layer A — Pattern primitives (architectural shapes)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 1 | **Broker capsule** | `Broker` trait | impl for one specific projection domain |
| 2a | **Overlay** (read-only contract surface) | Generic `Overlay<T>` (read-only to consumer, atomic-swap updates, versioned read, no-torn-read enforcement) | The `T` shape + a **curation policy** declaring what materializes into the Overlay (see [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"The Overlay contract") |
| 2b | **Working state** (broker-private) | Generic `WorkingState<W>` (full read/write inside the broker; not exposed) | The `W` shape (loaded catalog, workflow positions, Skill Filter weight cache, rate-limit counters, etc.) |
| 3 | **Internal Service** | `InternalService` trait + tick subscription | impl: project cold → working state; broker then materializes the curated subset into the Overlay |
| 4 | **Queue** | Reuse `neurogrim_core::queue` (shipped) | Topic names + payload schemas |
| 5 | **External Service** | `ExternalService` trait + queue-consumer scaffold | impl: ingest world → cold |
| 6 | **Cold Store** | Trait over SQLite/JSONL backends (reuse). **Per-broker file isolation (R-O-4 closure, Phase 9):** the framework default is **per-broker cold-store file** (each broker has its own SQLite file or JSONL append-stream); SHARED cold-store files across brokers are explicitly forbidden — the SQLite-lock contention cascade under 10+ concurrent broker writes is the failure mode that motivated the isolation. **Backend guidance:** SQLite for single-broker (or low-tick-rate) deployments where transaction integrity matters more than write throughput; JSONL append-only for high-tick-rate deployments where lock contention dominates (trade-off: JSONL queries are slower; checkpoint reads require scanning). Cluster manifest declares per-broker backend; mixing within a cluster is permitted (Sensory Broker on JSONL for high-throughput, Work Broker on SQLite for transactional queries). | Schema migration files (consumed by #26); cold-store backend choice per broker manifest |
| 26 | **Schema Migration Runner** | `SchemaVersion` field on every workflow checkpoint + a `SchemaVersionManifest` in the broker's cold store declaring current + historical versions. **Migration depth ceiling (R-O-2 closure, Phase 9):** cluster manifest declares `max_workflow_age_for_resumption` (default: 10 schema versions behind current). Workflows whose checkpoint SchemaVersion is more than N versions behind current are refused for automatic resumption with `failure_reason: workflow_age_exceeds_migration_ceiling`. Operator-confirmed re-snapshot ceremony (a Surfaced governance pipeline `propose-workflow-resnapshot`): operator inspects the workflow's last-known-good state, either approves a manual re-snapshot under current schema OR discards the workflow with audit-trailed rationale. Prevents the unbounded-migration-chain failure mode (50-version chains accumulate over 2 years and break probabilistically); forces operator-driven cleanup ceremony for stale workflows. **Post-migration governance smoke test (R-S-10 closure, Phase 9):** every `manual-verification-required` migration triggers an automatic post-apply smoke test: framework invokes each governance pipeline (`check-kill-switch`, `arm-kill-switch`, `check-trust-budget`, `record-dispatch`, `record-outcome`, `enforce-rate-limit`, `reset-trust-budget`) with deterministic test inputs; compares output to pre-migration baselines stored alongside the migration manifest. If any governance pipeline's behavior changed, the migration is auto-rolled-back with `failure_reason: governance_pipeline_behavior_change_detected` + the migration is flagged for operator deep-review. Closes the steganographic-trojan attack where shallow operator approval lets a migration secretly modify governance behavior. On broker startup: detects current schema version → applies outstanding migrations in order (idempotent, journaled) → on failure stops before touching the store (safe-to-retry) → logs migration success + timestamp. **Failed migrations abort broker startup loudly** (operator must resolve; no silent degradation). Replay tooling (#13) uses the SchemaVersion to apply version-appropriate deserializers for historical states. **Workflow resumption contract (ORDERING PINNED, LB-6 closure):** when a workflow resumes from a checkpoint whose `SchemaVersion` is older than the broker's current schema, framework runs forward-migrations atomically against the checkpoint's serialized state **as the first step of resumption, BEFORE contract-version validation per BB #34**. Post-migration payload must be contract-compatible with the dispatch-target pipeline's `contract_version`; if not, framework refuses resumption with `failure_reason: contract_version_incompatible_post_migration` + logs pre-migration and post-migration payloads for operator inspection. If forward-migration itself is not feasible (e.g., destructive schema change), framework rejects the resumption with `failure_reason: schema_forward_migration_unavailable` and **surfaces the safe-to-retry status to the agent (LB-2 closure)** via a `schema-migration-blocked` event (audit_class: governance) emitted to BB #28 (Diagnostics Collector) + projected to BB #32 (Operator Telemetry Summarizer) — the existing safe-to-retry path becomes visible, no zombie state. **Idempotency class:** each migration declares `idempotency_class: pure \| deterministic \| manual-verification-required`. Framework refuses to auto-apply `manual-verification-required` migrations without operator confirm; logs idempotency-violation-risk if a retry of a non-pure migration is detected. **Partial schema coexistence:** broker can run two schema versions concurrently for a bounded window — `workflow.schema_version_pinned_at: <ts>` lets old workflows complete under their version while new workflows adopt the new schema; operator declares the coexistence window per broker. | Migration files + idempotency_class declaration per migration; coexistence window declaration in broker manifest |

### Layer B — Pipeline primitives (universal unit)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 7 | **Pipeline type** | The struct + Serde derives | — |
| 8 | **Step type** | enum (Leaf / SubPipeline / Guard / Branch) | Leaf-op implementations (Rust fns) |
| 9 | **Pipeline Catalog** | Generic catalog + YAML/TOML loader + hot reload. **Load-time frame_rotation budget validation (LB-5 closure):** for every `frame_rotation:` step modifier in the catalog, loader pre-computes `rotation_budget = N × single_pipeline_budget × tempo_multiplier + synthesis_budget` (per [`BROKER-FRAMES.md`](BROKER-FRAMES.md) §7.3) against the active Tempo Frame's multiplier and the per-broker budget ceiling. Pipelines whose rotation_budget exceeds the ceiling are rejected at catalog load with `failure_reason: rotation_budget_exceeds_ceiling` + structured detail (which Frame list, computed budget, ceiling). Closes the gap where `MaxFrameRotationDepth` bounds nesting depth but a wide rotation (e.g., 100 Frames at depth 1) blows budget at runtime. **Hot-reload runs the full validation suite atomically (R-O-8 closure, Phase 9):** every hot-reload re-runs (a) schema validation, (b) frame_rotation budget validation, AND (c) the broker-reachability-analyzer cycle detection from BB #27 against the merged broker catalog set. If any reload would introduce a cross-broker cycle that the prior catalog didn't have, the reload is refused atomically with `failure_reason: catalog_reload_introduces_cycle` + the offending edge(s) listed; the prior catalog is preserved. Closes the runtime-cycle window where operator-edited catalogs could introduce cycles silently between startup and the next restart. | Pipeline definitions in YAML; rotation budget ceiling per broker manifest |
| 10 | **Pipeline Runner** | The executor, suspension support, bootstrap layer. **Catalog-version pinning at queue time (R-O-7 closure, Phase 9):** every dispatch records the `catalog_version: u64` at queue time (when the dispatch is enqueued, not when it executes). At execution, framework validates the broker's current catalog_version matches the queued value; refuses with `failure_reason: catalog_version_changed_mid_dispatch` if not — the dispatch is requeued against the current catalog (with operator-tunable retry policy: requeue-N-times \| escalate-immediately \| abort). Closes the hot-reload race where in-flight dispatches could checkpoint old-contract output against the new catalog. Combines with BB #9 hot-reload cycle-detection + rotation-budget validation (R-O-8 closure) so reloads are atomic AND in-flight dispatches detect the boundary. | — |
| 11 | **Workflow Engine** | Cold-store-as-truth + hot-store positions + resume. **Pipeline-ID hash_version field (R-O-10 closure, Phase 9):** every workflow checkpoint carries a `hash_version: u32` field identifying the hash function used to derive pipeline IDs in this checkpoint. On resume, framework looks up the hash version, applies the version-appropriate ID resolver to map checkpoint pipeline IDs to current catalog entries. Framework upgrades that change the hash function (e.g., adding new fields to the YAML serialization that hash takes as input) bump the hash_version constant; old checkpoints carry the old version and resolve correctly via the version-table lookup. Refuses resume with `failure_reason: hash_version_unknown` if checkpoint carries a hash_version no longer in the resolver table (operator-driven retention policy: framework keeps last N hash_version resolvers; older are archived). Closes the silent-breakage path where a hash function change makes all old checkpoints unresumable with no signal. **Resumption context (M-7 closure; sequenced after BB #26 schema-migration order pin per LB-6):** workflow checkpoints carry an optional `resumption_context: { pause_reason: enum, cycle_or_iteration_id: optional, awaiting_external_input: bool, continuation_hint: string }` field populated by the framework at suspend time. On resume, Materializer Composer (BB #22a) projects a `workflow-resumption-context.md` segment into `current-projection.md` surfacing "Workflow ID X resuming at <pause_reason>; was awaiting <input>; continuation hint: <hint>" so the agent picks up mid-stream without re-computing state from cold. **Schema-additive note:** because this is a new checkpoint schema field, it lands as a SchemaVersion bump per BB #26 (forward-migration: old checkpoints without the field deserialize to `resumption_context: null`, which the framework treats as "no context available; agent re-reasons from cold state" — backward-compatible default). | — (just author pipelines that span ticks) |
| 12 | **Trace Sink** | Trace format + write path + replay-against-historical-state harness. **Active Frame-stack observability (P-9):** every dispatch trace record carries `active_frame_stack` — a snapshot of the Frame map at dispatch time (per BB #35). A framework-provided Tier 2 query pipeline `trace-with-frame-context(pipeline_id, tick)` returns the trace + the active Frame stack so operators (and replay-equipped agents) can answer "what Frame stack was active when pipeline X ran at tick T?" Integrates with BB #13 Replay (frame context is included in replay output) and with the agent-behavior observability action-ledger (BB #36) for Frame-stack-distribution counters. | — (trace is automatic) |
| 13 | **Replay tooling** | Three replay scopes (single-pipeline / broker-tick / workflow); read-only by default with opt-in write-mode for golden regeneration; **subsumes test-fixture machinery** (frozen-state-snapshot + expected-output are replay test cases — same harness). See §4 invariant "Replay scope contract". **Write-mode audit + diff ceremony (R-S-7 closure, Phase 9):** every `--write-golden` invocation requires operator-supplied free-text `rationale` field (framework refuses write-mode without it). Loader runs in dry-run mode first: shows the diff (which goldens would change, hashes pre/post, count of test expectations shifting); operator must explicitly confirm before write commits. Every committed write is logged to `<cold-store>/replay-write-ledger.jsonl` with `{timestamp, operator_id, broker_id, tick_range, hash_of_previous_goldens, hash_of_new_goldens, rationale, confirm_token}` (audit_class: governance). Operator + later auditors can review the rationale chain; silent golden rewrites become impossible without rationale. | Test fixtures authored as replay inputs in cold store |
| 25 | **Pipeline Cancellation Handler** | Framework guarantees `on_cancel` step runs even if kill-switch interrupts mid-step; Workflow Engine atomicity contract (partial-step output rolled back; workflow resumes from last checkpoint on next tick OR transitions to `cancelled` state per pipeline policy); per-pipeline cancellation behavior declarable in YAML. **Cancellation depth bound:** `cancellation_depth_max` (default 1, operator-tunable per pipeline) prevents handler-chain recursion — framework rejects `on_cancel` steps that themselves carry `on_cancel`. Audit trail records handler nesting depth per dispatch. **Cancellation context (M-2 closure):** `on_cancel` handler receives a `cancellation_context: { cause: enum(kill_switch \| timeout \| quota_exhausted \| operator_abort \| schema_migration_blocked), cycle_id: optional, iteration: optional, reason_text: optional, redacted: bool }` populated by the framework from the workflow checkpoint state. **Sensitive-cause redaction:** when `cause = kill_switch` AND the kill-switch was armed with a sensitive reason (governance class `security-incident`), the `reason_text` field is replaced by `[REDACTED-SECURITY]` and `redacted: true` is set; handler can still react to the cause class without exposing the underlying reason to traces or downstream brokers. Operator-tunable redaction classes per cluster manifest. | `on_cancel: { steps: [cleanup, log], terminal_state, cancellation_depth_max }` per pipeline (optional) |

### Layer C — Substrate composition (cross-broker glue)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 14 | **Broker Registry** | Discover/load brokers at startup; the manifest schema (incl. role-set declaration) | A manifest declaration per broker (incl. its role-set) |
| 15 | **Tick Source** | Hook-driven + file-injection ticks; subscription API | Subscribe to tick events the broker needs |
| 16 | **Workspace Manager** (canonical Embodiment broker) — *trait + spinal-cord defaults in S0-T; concrete impl in S2-T* | The Embodiment-role spinal cord — Effector subordination, Workspace Queue, real-time-vs-queued dispatch, cross-effector synchronization, cross-role sense-feedback routing | Effector registration |
| 17 | **Topology Broker** (canonical Sense broker) — *trait + spinal-cord defaults in S0-T; concrete impl in S1-T* | Cross-broker routing, ACL enforcement, per-consumer Topology Overlay, subscription fanout, ACL-mutation self-bypass invariant | Initial ACL definitions per broker |
| 18 | **Awareness Service** | The Sensory-Queue rate-limit/sanitize enforcer; **the role-boundary guard** for the Sense cross-role write path | Sensor schemas + redaction rules + rate budgets |
| 19 | **Governance Composer** | The set of Tier 2 governance pipelines (see §2.4) | Per-pipeline policy declarations in YAML |
| 20 | **Skill Filter** | Generic rank/dedupe/learn-from-rejection; **enforces the reachability channel split** (governance pipelines exposed via `governance_pipelines()` sidecar, distinct from `legal_pipelines()` capability ranking); per-broker/per-pipeline/per-role hygiene classifier with `audit_class` filter. **Governance channel is reserved read-only (LB-3 closure):** Skill Filter's tuning surface (operator weight cells, Autonomous tuner bounds, catalog hot-reload) explicitly excludes the governance channel from every tunable operation; loader validation rejects tuner configs that attempt to remove a `governance` audit_class pipeline from `governance_pipelines()` output. **Dead-pipeline tombstones (M-1 closure):** the alive/dead/new classifier projects dead-pipeline tombstones to BB #24 (Awareness Materializer) as a dedicated `dead-pipelines` subsection of the routing-signal segment; dead Internal pipelines are hard-filtered from `legal_pipelines()` output (not just down-ranked) so silent dispatch-of-dead-code is impossible. **Frame-aware re-rank at sub-pipeline dispatch (M-4 closure):** when a pipeline step includes a `with_frame:` modifier (per [`BROKER-FRAMES.md`](BROKER-FRAMES.md) BB #35), Skill Filter recomputes `legal_pipelines()` against the new Frame stack before emitting the sub-pipeline call. The recomputation is **scoped to that sub-pipeline only** (does NOT mutate the parent tick's ranking, which stays at per-tick cadence). Trace records the active Frame context at sub-dispatch time so replay + hygiene scoring can see which Frame stack was active during ranking. Performance bound: at most one re-rank per `with_frame:` step; cluster manifest can cap re-rank-per-tick at a ceiling to prevent runaway. | Per-broker weight cells + tuning policy |
| 21 | **Proposal Ledger** | Reuse `.claude/brain/proposal-ledger.json` (shipped); the tuning-pipeline protocol. **Curation-gap recourse (R-X-8 closure, Phase 9):** when a broker's `legal_pipelines()` returns empty for a workflow that's been actively dispatching (signal: the dispatch queue has waited >`empty_legal_pipelines_grace_seconds` — default 30s — for ANY pipeline to become legal), framework offers a low-specificity `request-operator-assistance` fallback pipeline (audit_class: governance; Surfaced; OperatorConfirmed tunability) that writes a `type: urgent-curation-blocker` entry to the Proposal Ledger with the agent's workflow context. BB #24 Awareness Materializer surfaces urgent entries with priority sorting in the L1 segment; BB #32 Telemetry surfaces urgent entries in the operator summary with separate "blocking" subsection. Closes the dead-end where curation omissions leave the agent indefinitely stuck. Operator reviews the proposal, either adjusts the broker's preconditions, adds a new pipeline, or rejects with rationale. **Auto-promote pattern:** if the same proposal is filed N times across M cycles by the same agent without operator response (default: N=3, M=5 cycles), framework escalates urgency to `critical` + emits a `proposal_ignored_critical` event to BB #28 Diagnostics. | Tuning-pipeline definitions in the catalog |
| 22 | **Hot-Store Materializer** | Writes per-broker Overlay state to `.claude/brain/broker/segments/overlay.md`; composed by the **Materializer Composer** (#22a) into `current-projection.md` | — |
| 22a | **Materializer Composer** (named substrate, promoted from Tier 3) | Concatenates materializer segment files in **operator-declared order** into the auto-loaded `current-projection.md`. Order is operator-tunable per cluster manifest (`materializer_composition_order: [overlay, awareness-routing, ...]`). Each materializer owns one segregated file; the Composer enforces collision-safety by construction (no last-writer-wins). Promoted from implicit Tier 3 plain function because composition order matters for L1 context flow (per [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §2 token budget). **Governance-first override (R-O-3 closure, Phase 9):** the Composer overrides operator's declared order to place `governance-pipelines` segment FIRST in the composed output, regardless of where the operator declared it. The reachability invariant (governance always reachable; per §4) is load-bearing for safety and outranks operator's composition preferences. **Truncation alarm:** if the composed total exceeds the configured context-window budget (per cluster manifest `materializer_context_budget`), the Composer emits a `materializer_truncation_imminent` event to BB #28 Diagnostics (audit_class: governance) AND falls back to **governance-only projection** — only the `governance-pipelines` segment is written to `current-projection.md`; capability segments are omitted with a one-line marker `[capability-segments-omitted-due-to-budget; see telemetry]`. Operator sees the alarm in BB #32 Telemetry; agent reasons under explicit safe-minimal awareness instead of partially-truncated awareness. Closes the silent governance-suppression path. | Composition order declaration + context-budget ceiling in cluster manifest |
| 23 | **Role-set scaffolding** | Per-role defaults the framework wires automatically on broker registration (Sense / InnateAbility / Embodiment spinal cords); composes for multi-role brokers | Role-set declaration in manifest |
| 24 | **Awareness Materializer** | Writes pipeline catalog routing signals (description + when_to_use per Surfaced pipeline + alive/dead/new hygiene status) to `.claude/brain/broker/segments/awareness-routing.md`; composed by the Materializer Composer (#22a) into `current-projection.md`. See [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §2 for the L1 awareness slot details. | — |
| 27 | **Cross-Broker Composition Policy** | A pipeline may declare a sub-step calling another broker's surfaced pipeline (`sub_pipeline: <broker_id>/<pipeline_id>`). Framework enforces: **(a) Atomicity with two-phase commit (R-O-1 closure, Phase 9)** — cross-broker sub-pipelines must complete within a single workflow checkpoint of the parent broker; failure of the cross-broker sub-step rolls back the parent workflow checkpoint per §2.2. **Distributed-transaction ordering is pinned: callee commits FIRST, caller commits SECOND.** On a partial-failure scenario (callee committed, caller crash before its commit): on caller restart, framework detects the unfinalized cross-broker entry in the workflow log, re-reads callee's commit ID, and either (i) completes the caller-side commit if callee's state matches the expected output, OR (ii) issues a compensating call to callee to roll back its commit (callee's pipelines must declare an `on_compensate:` handler for any pipeline that participates in cross-broker composition). On callee crash before its commit: caller times out, rolls back locally, retries or fails the parent workflow per its `on_cancel` policy. The double-debit (trust budget per BB #4) lands on callee's commit, not on caller's invocation; if compensation runs, the debit is reversed atomically with the rollback. **(b) ACL governance** — Topology Broker (#17) mediates the call per its per-consumer OverlayView; calling broker requires ACL grant for target broker's surfaced pipeline. **(c) Trust budget double-debit** — calling broker debits its own trust budget for the cross-broker call AND the called broker debits its budget for the dispatch (prevents free-riding); both parties must use the same trust-budget unit per §4 unit-composition rule. **(d) Cycle detection** — a `broker-reachability-analyzer` (Tier 3 bootstrap function) runs at startup: BFS through each broker's catalog `sub_pipeline:` edges, flags cycles, refuses broker registration on a detected cycle. Output graph at `.claude/brain/broker-reachability-graph.json` for operator inspection. **(e) Cluster-pipeline extension** — same (a)/(b)/(c)/(d) contracts apply to cluster-pipeline composition through the IAB; cluster-pipelines calling other cluster-pipelines compose with the same atomicity + ACL + trust-budget + cycle-detection invariants (see [`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md)). **(f) Governance-pipeline composition discipline (LB-4 closure)** — pipelines carrying `audit_class: governance` MAY be composed via `sub_pipeline:` but ONLY as Tier 2 (Internal) pipelines so BB #27's atomicity + double-debit + cycle-detection contracts govern them. Tier 3 plain functions are bootstrap-only and never enter cross-broker composition. ACL-mutation pipelines (the Topology Broker self-bypass invariant case) remain Tier 3 for direct dispatch from the Topology Broker's mutation handler (preserves self-bypass) but cannot be composed across brokers; a sub_pipeline targeting an ACL-mutation pipeline is rejected at catalog load time with `failure_reason: governance_pipeline_composition_requires_tier_2`. Composability is permitted; accountability fuzziness is prevented by the Tier-2 requirement (every cross-broker composed governance call lands in the trace + checkpoint, never in an unaudited plain-function call). YAML: `composition_mode: [allowed \| requires-acl \| requires-trust-boost]`. | Per-pipeline `sub_pipeline:` declarations with `composition_mode` |
| 28 | **Diagnostics Collector** | Unified live observability: per-pipeline latency histograms (P50 / P95 / P99) + dispatch counts + success/failure rates + governance-block frequency + broker-health indicators (cold-store accessibility, hot-store write atomicity, projection-latency ceiling) + **current trust-budget state per active unit** (e.g., if unit=token-spend, surfaces current-tokens-consumed / ceiling-tokens) + **per-source drop counts (M-5 closure)** for the Awareness Service rate-limit enforcer (which sensors dropped which payloads in the last window; rolled into the Sensory Broker's Overlay as a freshness/confidence signal so agents reason under explicit staleness). Emits to reserved bus topics `_neurogrim/diagnostics/{latency,dispatch-stats,broker-health,trust-budget,sensory-queue-drops}`. **Source-class quota:** the Diagnostics Collector emits under source-class `system-diagnostics`; Sensory Queue enforcer applies per-source rate quota (default: 1 emission per second per broker per topic) to prevent storm-loop where another broker's processing of diagnostics generates more diagnostics. **Reserved unmetered transparency channel (M-5 amplification):** the `_neurogrim/diagnostics/sensory-queue-drops` topic specifically is exempt from rate-limit quota (carries `audit_class: meta-observation` + framework-reserved source-class `transparency-floor`) so transparency events cannot themselves be silenced by the rate limiter — closes the infinite-regress risk where a sensor flood drops events AND drops the drop-reports about them. The exemption is narrow: only this single topic, only for drop-count emissions; abuse-prevented by framework-only write access (no broker-author override). **Feedback predicate:** Diagnostics entries about the Diagnostics Collector itself are excluded from re-sampling decisions (`audit_class: meta-observation` per §2.3 exclusion rule); closes the self-referential loop. Sensory Broker (or consuming-project's Diagnostics Broker per BROKER-COMPOSITION composition decision) surfaces these as Overlay cylinders or Awareness Materializer segments. Framework provides the collector; operator declares emission cadence + retention window per broker manifest. | Emission cadence + retention window per manifest |
| 29 | **Broker Lifecycle** | `BrokerShutdown` pipeline (Tier 2 internal, untunable): stops accepting new dispatches → waits for in-flight pipelines to complete (with `shutdown_timeout_per_pipeline`, default 5000ms — per-pipeline, NOT global, to prevent inter-broker deadlock) → after timeout, in-flight pipeline is force-killed + recorded with `failure_reason: shutdown_timeout` + cancellation handler (#25) runs → flushes queue → writes final audit snapshot → emits shutdown-complete signal to operator + cluster peers (via IAB if cluster mode). **Cluster-pipeline shutdown discipline:** cluster-pipelines declare `allowed_during_shutdown: true \| false` (default: false). When a broker enters shutdown, peers' cluster-pipelines to it that require `allowed_during_shutdown: true` proceed; the rest receive `failure_reason: peer_shutting_down` immediately (closes inter-broker deadlock: A and B both shutting down with cross-calls won't wait on each other indefinitely). `BrokerVersionTransition` (Tier 3 bootstrap, OperatorOnly): validates schema compatibility via #26 → snapshots broker state → atomically swaps cold-store backing → resumes from checkpoint. Idempotent + retryable. **Hot-swap protocol:** operator can transition a broker's version mid-cluster; in-flight workflows pin their broker version at workflow start (mirrors cluster-pipeline version-pinning from cereGrim's IAB stub Q5). | Optional `broker_lifecycle_policy` overrides (shutdown timeout, hot-swap allowed, per-cluster-pipeline allowed_during_shutdown) in manifest |
| 30 | **Agent-Broker Onboarding Projection** | `OnboardingProjection` (distinct from steady-state Overlay + Awareness materializers): runs **once per broker-per-agent on first registration** (unless skipped per P-1 opt-out below). **Opt-out (P-1):** cluster manifest declares `skip_onboarding_for_agents: ["<agent_id>"]` for agents that have read the broker spec directly; agents can also emit a one-time `onboarding-already-read` signal (audit_class: meta-observation) the framework checks before firing projection. Default: opt-in (no skip; safer for unfamiliar agents). Surfaces (1) broker purpose + role-set declaration, (2) top-N Surfaced pipelines with full routing signals + when-to-use phrases, (3) governance posture (which Frame defaults apply per [`BROKER-FRAMES.md`](BROKER-FRAMES.md), what's Untunable), (4) cross-references to relevant skill bodies via Context Broker. Auto-injected into L1 context on the agent's first tick interacting with this broker via the Materializer Composer (#22a) as a separate segment. Subsequent ticks fall back to steady-state projections (#22 + #24). **State persistence:** per-agent registration timestamps live in the broker's cold store at `<cold-store>/onboarding-registrations.jsonl` (per-broker, append-only). Framework marks registration on first dispatch from agent. **Cold-store reset detection:** if framework detects the broker's cold store has been wiped (SchemaVersion reset to baseline, or registration ledger missing), it emits a `broker-cold-store-reset` event (`audit_class: governance`, reason: `cold_store_reset_detected`); Materializer Composer re-injects OnboardingProjection on next tick for all known agents. Closes "agent encounters new broker with no awareness of its capabilities" + "silent awareness loss after cold-store reset" failure modes. Particularly load-bearing for IAB cluster work where peer-agents register dynamically. | Onboarding content template (one-time projection contents) in broker config |
| 31 | **Cluster Federation Topology** | Cross-CLUSTER federation (distinct from BB #27 which extends to cluster-pipeline composition within one cluster). **Canonical Agent Card schema (R-O-6 closure, Phase 9):** BB #31 declares the canonical Agent Card schema for the entire ecosystem; LSP-Brains spec §13 and cereGrim IAB reference this schema rather than declaring their own. Schema carries `schema_version: u32`; IAB negotiates compatible versions with peers at handshake time (peer with higher version offers downgrade-compat or refuses with `failure_reason: agent_card_schema_version_unsupported`). Schema fields: `{agent_id, agent_name, cluster_id, cluster_roles, broker_role_set, advertised_cluster_pipelines, public_a2a_endpoint, schema_version, generated_at: ISO8601, signature_placeholder_for_b58}`. **Anti-impersonation defenses (R-S-13 closure, Phase 9):** (a) **Role attestation via operator pre-authorization** — cluster manifest declares `trusted_peer_agents: { <agent_id>: { allowed_cluster_roles: [...] } }`; IAB rejects cards claiming roles not in the operator-declared mapping with `failure_reason: peer_claims_unauthorized_role`. Operator is the authority on which agents can claim which roles. (b) **Staleness check** — every card carries `generated_at` timestamp; framework rejects cards older than `agent_card_ttl_seconds` (default 3600) with `failure_reason: agent_card_stale`. Prevents replay of stolen cards from weeks ago. (c) **Signature placeholder** — the `signature_placeholder_for_b58` field is reserved for B-58 PKI-based cryptographic authentication; until B-58 lands, (a)+(b) carry the authentication load. Closes the federation-pluralism gap where 3 separate primitives (BB #31, LSP §13, cereGrim IAB) could declare incompatible card formats; cross-project federation works against the single canonical schema. Cluster-A discovers Cluster-B's advertised cluster-pipelines via inter-cluster Topology Broker handshake (over A2A); ACL composes **transitively** (Cluster-A's broker-X can reach Cluster-B's broker-Y if Cluster-A's IAB has ACL grant AND Cluster-B's inter-cluster ACL grants Cluster-A access; chains rejected otherwise). **Version cascade:** cluster-version pinning is per-cluster-pipeline at workflow start; cross-cluster dispatches pin BOTH calling cluster's version AND called cluster's version (version mismatch refuses dispatch with `failure_reason: inter_cluster_version_mismatch`). **Bootstrap policy parallel to IAB's per-cluster modes:** `inter_cluster_bootstrap: federated-mesh \| arbiter-service \| static`. Closes the "cereGrim-cluster-1 wants to dispatch a cluster-pipeline to cereGrim-cluster-2" scenario the IAB stub flagged as future work. | Per-cluster inter-cluster ACL declarations + version-pinning policy in cluster manifest |
| 32 | **Operator Telemetry Summarizer** | Human-readable broker status summary (distinct from BB #28 Diagnostics Collector which emits machine metrics). Reads Diagnostics output + projection state + audit trail + governance-block frequencies; emits Markdown summary to `.claude/brain/broker-telemetry-summary.md` (auto-loaded into operator CLAUDE.md via the standard CLAUDE.md mechanism). Sections: per-broker health snapshot (active / idle / errored / overloaded), recent governance decisions (kill-switches armed, proposals pending), trust-budget consumption across active units, peer-dialogue cycle state (if any active), workflow checkpoint depths. Operator-tunable refresh cadence per cluster manifest (default: every 60s). Closes "operator wants to see what brokers are doing without parsing JSON" — gives ops-grade insight on top of #28's data layer. | Summary template (Markdown skeleton) per cluster |
| 33 | **Pipeline Proposal Mechanism** (extends BB #21) | New entry type for [`Proposal Ledger`](#bb-21) — `type: pipeline-proposal` distinct from `type: tuning-proposal`. Schema: `{pipeline_id, proposed_yaml, preconditions, reasoning, operator_decision, decision_timestamp}`. LLM (or operator) authors a proposal entry when it observes a recurring pattern worth baking as a pipeline (or wants to add a new capability). Awareness Materializer (#24) surfaces pending proposals in L1 context as a `pipeline-proposals` segment so operators see what's queued. On operator approval, framework hot-reloads the new pipeline into the catalog (per BB #9) atomically. On rejection, proposal records the rejection reason for future calibration. Tunability: `OperatorConfirmed` (LLM proposes; operator decides — never autonomous catalog growth). Closes "LLM observes pattern → wants to bake as pipeline → operator reviews" path which had no protocol. | Proposal-ledger entries (operator-authored or LLM-authored); approval-decision overrides per cluster |
| 34 | **Workflow-Pipeline Versioning Contract** (Layer B addition — core gap from Phase 4 audit) | A pipeline declares `contract_version: N` (distinct from schema_version which handles data shape and hash_version which handles pipeline-ID derivation per BB #11; the three version dimensions evolve independently — see BB #11 + BB #26 for the other two). This is the *semantic contract* — what fields it emits, what preconditions it accepts, what governance it composes). Workflow checkpoints declare `compatible_contracts: [N, N-1]` at workflow start (operator-tunable per pipeline's contract-evolution policy). At dispatch, framework validates that the running pipeline's contract_version is in the workflow's compatible_contracts set; refuses dispatch with `failure_reason: contract_version_mismatch` if not. **Contract-evolution policy per broker manifest:** `allow_backward_compatible_only` (new contracts must be supersets of old) \| `allow_forward_compatible_upgrades` (workflows can adopt newer pipeline contracts) \| `manual-operator-approval-per-contract` (each contract version requires operator sign-off). Closes the gap where pipeline contract evolution mid-deployment (operator updates a pipeline to require additional output fields) leaves in-flight workflows silently broken. Distinct from BB #26 (Schema Migration handles data-shape changes; #34 handles contract-shape changes). | Per-pipeline `contract_version` declaration + per-broker contract-evolution policy |
| 35 | **Frame stack** | Typed Frame map in broker state ([`BROKER-FRAMES.md`](BROKER-FRAMES.md) §1); seven canonical Frame types (Hat / Stakes / Tempo / Mode / Confidence / Audience / Scope; §2); merge order across inheritance levels (dispatch → pipeline → role → broker → cluster; §7.2); consumption surfaces (Governance Composer / Skill Filter / Overlay curation / Workflow Engine; §3); `with_frame:` step modifier; `frame_rotation:` step sugar with MaxFrameRotationDepth bound (§4); IAB negotiation protocol with refusal schema (§7.6); conflict precedence matrix (§7.1); rotation budget arithmetic per Tempo (§7.3); coverage-audit pipeline (§7.4); extension protocol (§7.5); L1 awareness injection format with conflict-resolution surface (§7.7). Phase 5 closure: all 7 open design questions pinned. | Per-broker Frame defaults; per-pipeline Frame requirements; per-cluster Frame manifest + conflict-precedence overrides; per-Frame-type weight cells |
| 36 | **Agent-Behavior Observability** | Closes the **VISION principles #21/#22 alignment gap** the BB #20 hygiene classifier covered for pipelines but never covered for agent actions themselves. A per-agent action-ledger keyed by `{agent_id, dispatch_id, broker_id, pipeline_id, outcome, governance_blocks_fired, frame_stack_snapshot}` (separate from the invocation ledger which is keyed by skill-name); written by the Pipeline Runner on every dispatch (success / fail / block) regardless of audit_class. Projects into a `agent-behavior-summary.md` segment via Materializer Composer (#22a): per-agent counters {actions, failures, governance-blocked rate, mean-time-to-completion per pipeline class, Frame-stack-distribution}. Surfaces in L1 context so the agent can perceive its own behavior trajectory (principle #21 "agents must perceive their own blind spots") and steward its own work (#22). Operator-tunable retention window per cluster manifest; default 7 days hot, archived to cold thereafter. **Sensitivity-redaction:** action-ledger entries carry the same redaction rules as BB #18 (Awareness Service) — secret-references stay references, never values — applied at write time. **Hygiene rollup:** the action-ledger feeds back into BB #20 Skill Filter's classifier (agent-action-density per pipeline is a signal of pipeline aliveness from the user side; complements broker-side invocation-ledger). | Action-ledger schema declaration + retention policy in cluster manifest |
| 37 | **Pipeline Deprecation Manager** | The inverse of BB #33 (Pipeline Proposal Mechanism) — operator retires a pipeline atomically with in-flight workflow safety. Cluster manifest declares `deprecated_pipelines: [{id, effective_date, archive_path, reason}]`; Pipeline Runner (BB #10) checks this registry **before** `check-trust-budget` on every dispatch. Behavior: workflows started before `effective_date` continue under their pinned pipeline version (per BB #29 hot-swap protocol + BB #34 contract pinning); new dispatches after `effective_date` refuse with `failure_reason: pipeline_deprecated` + the archive path so callers can read the rationale. Old catalog entries are **archived, not deleted** (audit trail stays intact at `<archive_path>/<pipeline_id>-<version>.yaml`). Surfaces deprecated-pipelines in BB #24 (Awareness Materializer) as a `retired-pipelines` subsection alongside dead-pipeline tombstones (BB #20) so agents see both "won't run" (deprecated) and "broken" (dead) as separate failure modes. **Composition with BB #29 (Broker Lifecycle):** broker shutdown can carry a deprecation step (deprecate-all-of-this-broker's-pipelines as part of the shutdown ceremony). **Composition with BB #33 (Pipeline Proposal):** operator approval of a new pipeline can carry an implicit deprecation of the one it replaces (`displaces: pipeline_id`); the Proposal Mechanism writes both entries atomically. | Per-cluster `deprecated_pipelines:` declarations |
| 38 | **Sensor Quarantine Manager** | A misbehaving custom sensor (malformed payloads, embedded secrets despite redaction, write-flood after rate-limit enforcement) gets isolated, inspected, fixed, and restored without the operator's only options being "stop the sensor completely" or "tolerate the noise." Cluster manifest declares `quarantined_sources: [{source_id, reason, quarantine_date, test_mode_enabled}]`; the Awareness Service enforcer (BB #18) checks the list on every write. Quarantined sources route to a **shadow Awareness Map** instead of the live one (sensor outputs captured for inspection, never reach agent's perception). Operator inspects shadow outputs via `inspect-quarantined-source(source_id)` (Surfaced; operator-only tunability), validates fix in test mode, dispatches `restore-quarantined-source(source_id)` to re-enable live writes. **Composition with BB #28 (Diagnostics):** quarantine duration + restoration attempts tracked; chronic-quarantine sensors flagged for retirement. **Composition with BB #18 (Awareness Service):** the rate-limit drop-counts (M-5) feed a `quarantine-candidate` advisory — sensors that consistently exceed quota are surfaced as candidates for operator-initiated quarantine. Defaults: a sensor with >3 quarantine cycles in 30 days emits a `chronic-sensor-warning` event (audit_class: governance). | Per-cluster `quarantined_sources:` declarations; test-mode shadow-map schema per sensor type |

**Totals:** ~29 framework-side blocks (write once for NeuroGrim, all brokers benefit
across all role-set compositions); ~13 broker-author blocks (mostly declarative — YAML
schemas, manifest with role-set declaration, weight cells, curation policies, migration
files, composition-order declarations, contract-version declarations, ACL grants, Frame
defaults, action-ledger schemas, deprecation declarations, quarantine declarations, a
handful of leaf-op functions). **38 main building blocks (numbered #1–#38) across three
layers, plus 1 sub-numbered entry (#22a Materializer Composer, promoted from implicit
Tier 3 to named substrate).** The main count is 38; #22a is a sub-component within the
materializer cluster, not a separate primitive, so the framework's primary surface is
"38 building blocks." Phase 8 (2026-06-24) added #36 (Agent-Behavior Observability, closes
the VISION #21/#22 alignment gap), #37 (Pipeline Deprecation Manager, the inverse of
#33), and #38 (Sensor Quarantine Manager, closes the Awareness Service misbehaving-sensor
recovery gap).

**Framework-provided pipelines' default `audit_class` values** (operators need not
re-declare; Broker Registry reads from framework defaults):
- `check-trust-budget`, `check-kill-switch`, `arm-kill-switch`, `record-dispatch`,
  `record-outcome`, `enforce-rate-limit` — `audit_class: governance`
- `parse-backlog`, `rank-by-tier`, `materialize-overlay`, `project-cold-to-hot`,
  `rank-legal-pipelines` and other internal projection/ranking pipelines —
  `audit_class: capability` (they're broker plumbing for capability work, not
  governance)
- Diagnostics Collector pipelines (#28 emissions), trace-sink reads, hygiene scoring,
  ledger-introspection pipelines — `audit_class: meta-observation` (excluded from the
  feed they themselves consume, per §2.3)

Operator-authored pipelines MUST declare `audit_class` explicitly per
[`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md) validation rules.

**Deprecation discipline.** Every new building block added to this table MUST carry a
`displaces` / `deprecates` consideration (may be explicitly empty — `displaces: nothing`
— but must be considered, not omitted). The framework refuses to grow indefinitely; new
blocks must justify either net-new surface OR replacement of existing surface. This
column is informal in the v1 table (the discipline is stated here; per-row annotation
lands as the table evolves) but enforced going forward. Examples:
- BB #24 (Awareness Materializer) `displaces: nothing` — net-new surface; mirrors #22
  Hot-Store Materializer at a different abstraction level (routing-signal injection vs
  file-projection injection).
- BB #25 (Pipeline Cancellation Handler) `displaces: nothing` — net-new safety primitive
  closing a previously-undefined contract gap.
- BB #26 (Schema Migration Runner) `displaces: nothing` — formalizes what was previously
  glossed in §6 walkthrough; consumed by #6 (Cold Store) + #13 (Replay).
- BB #27 (Cross-Broker Composition Policy) `displaces: nothing` — pins the
  cross-broker contract that was twice-deferred; closes an open design surface.
- BB #28 (Diagnostics Collector) `displaces: nothing` — net-new live-observability tier
  complementing #12/#13 (Trace Sink + Replay; post-hoc).
- BB #29 (Broker Lifecycle) `displaces: nothing` — operational primitive complementing
  #14 (Broker Registry; startup only).
- BB #30 (Onboarding Projection) `displaces: nothing` — first-encounter awareness tier
  complementing #22/#24 (steady-state materializers).
- BB #31 (Cluster Federation Topology) `displaces: nothing` — net-new substrate for
  cross-cluster federation; parallel to BB #27's intra-cluster cross-broker composition
  but at the cluster-of-clusters level.
- BB #32 (Operator Telemetry Summarizer) `displaces: nothing` — human-readable layer
  on top of BB #28 (machine metrics); operator-facing surface distinct from agent-facing
  awareness projections.
- BB #33 (Pipeline Proposal Mechanism) `extends #21` — adds a new entry type to the
  existing Proposal Ledger; not a separate ledger.
- BB #34 (Workflow-Pipeline Versioning Contract) `displaces: nothing` — Layer B
  addition closing the contract-evolution gap that BB #26 (Schema Migration) doesn't
  cover (schema = data shape; contract = semantic surface; both can evolve
  independently).
- BB #35 (Frame stack) `displaces: nothing` — Layer C addition closing the
  Phase-3-deferred BROKER-FRAMES stub; substrate primitive for the seven canonical
  Frame types (Hat / Stakes / Tempo / Mode / Confidence / Audience / Scope).
- BB #36 (Agent-Behavior Observability) `displaces: nothing` — Layer C addition
  closing the VISION-principle #21/#22 alignment gap; complements BB #20 (pipeline-side
  hygiene) with agent-side action-ledger.
- BB #37 (Pipeline Deprecation Manager) `extends #33` — the operator-facing inverse of
  the Pipeline Proposal Mechanism; not a separate ledger.
- BB #38 (Sensor Quarantine Manager) `extends #18` — operator surface for isolating
  and restoring misbehaving custom sensors without "stop the sensor completely" as the
  only option.
- BB #22a (Materializer Composer) `promoted from Tier 3 to named substrate` — composition
  order is operator-tunable; not implementation detail.
- A hypothetical future "Unified Materializer" replacing #22 + #24 + #30 would carry
  `displaces: #22, #24, #30` — the operator can see the consolidation intent at a glance.

Closes the failure mode "framework keeps adding building blocks without retiring any."

---

## 4. Tunability — four tiers, defaulting to operator-only

A pipeline (or a config cell within a pipeline) carries a `tunability` field with
four legal values:

| Tier | Who can change | Mechanism | Examples |
|---|---|---|---|
| **Untunable** | Nobody at runtime; code change required | — | Awareness Service rate-limit enforcer · Pipeline Runner itself · kill-switch arming logic · the broker's spine pipelines |
| **OperatorOnly** | Operator via config files / Brain UI | Edit YAML, reload-catalog | Trust-budget ceilings · per-sensor rate limits · which pipelines are surfaced vs internal · governance policy attachments |
| **OperatorConfirmed** | LLM proposes via tuning pipeline → proposal ledger → operator confirms | `propose-*` surfaced pipelines write to `proposal-ledger.json`; operator confirms via Brain UI / CLI | "Demote pipeline X — never picked in 30 days" · "Deprecate this duplicate" · "Raise trust budget on Y" |
| **Autonomous** | LLM tunes directly within declared bounds; reversible | LLM dispatches `tune-*` surfaced pipeline; framework enforces bounds | Short-term skill weights (recency decay) · workflow resume-order preferences · per-session formatter style |

**The default for any new tunable cell is OperatorOnly.** Anything else must be
explicitly opted-in by the operator. This is the floor: the LLM can SEE the entire
catalog (transparency), REASON about what it would change (introspection), and PROPOSE
changes (via the proposal ledger), but cannot UNILATERALLY change governance-bearing
pipelines.

**Autonomous tuner bound ceiling + audit trail (R-S-2 closure, Phase 9).** Autonomous
tuners adjust weights within operator-declared bounds. The bounds themselves are
**schema-hard-ceiling** values — operator-declared bounds in the cluster manifest are
validated against schema-defined absolute ceilings (e.g., `autonomous_weight_value ∈
[0.0, 2.0]` is the schema ceiling; operator-declared `[0.1, 1.5]` is permitted;
operator-declared `[0.01, 10.0]` is rejected at manifest-load with
`failure_reason: tuner_bounds_exceed_schema_ceiling`). Schema ceilings live in the
framework binary; a manifest cannot widen them without a framework code change.
Closes the attack where compromised manifests indirectly suppress governance pipelines
via extreme-weight autonomy. Every Autonomous tuning dispatch records a per-cell
audit entry in `<cold-store>/tuner-audit-ledger.jsonl` with
`{timestamp, broker_id, cell_id, old_value, new_value, dispatch_id, audit_class:
meta-observation}` — operator reviews drift; sudden 10× jumps surface as anomalies
in BB #28 Diagnostics.

**Critical invariant — transparency:** untunable pipelines are listed in the catalog
with their tier visible. The LLM knows they exist, what they do, and that they can't be
touched. No hidden infrastructure. The LLM can model the constraint without being able
to escape it.

**Critical invariant — reachability via channel split:** Untunable and OperatorConfirmed
pipelines that carry the **governance** purpose-class are exposed through a sidecar
channel — `governance_pipelines() → Vec<Pipeline>` — distinct from the agent-facing
capability-ranking channel `legal_pipelines(state) → Vec<Pipeline>`.

- **`legal_pipelines(state)`** ranks capability pipelines (the agent's actual choice
  surface). No governance floor consumes ranking slots; the top-K is fully available for
  capability competition.
- **`governance_pipelines()`** is always-reachable, untunable, exposed to the LLM via the
  awareness mechanism (see [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md)). Kill-switches,
  audit anchors, rate-limit enforcers, ACL-mutation bypasses live here.

The split closes the **safety/expressivity collision**: a unified floor would let
~30 governance pipelines (5 per broker × 6 brokers) occupy the LLM's entire top-K,
silently degrading capability. The split preserves both: untunability of governance
(reachable on the sidecar channel, immune to Autonomous tuner suppression) AND
capability oxygen (the choice ranking is for capabilities, not governance).

**Reachability is preserved structurally** — an Autonomous tuner can no longer suppress
`arm-kill-switch` from the LLM's reachable surface, because Autonomous tuning only
affects `legal_pipelines()` weights; `governance_pipelines()` is outside the tunable
scope by construction.

**Reachability enforcement (LB-3 closure).** The "outside the tunable scope by
construction" guarantee is implemented in **BB #20 (Skill Filter)**, where
`governance_pipelines()` is declared as a **reserved read-only channel** — Skill
Filter's tuning surface (operator-defined weight cells, Autonomous tuner bounds,
catalog hot-reload) explicitly excludes the governance channel from every tunable
operation. Framework tests verify suppression-impossibility: a deliberately-malicious
tuner config that attempts to remove `arm-kill-switch` from `governance_pipelines()`
output fails Skill Filter loader validation. Cluster manifest CAN disable L1
injection entirely (operator choice, audit-trailed) but CANNOT partially edit the
governance channel — it is "full + visible" or "absent by operator choice," never
"partially suppressed."

**Critical invariant — cancellation atomicity:** when a kill-switch fires mid-step
within a workflow, the step's partial output is **rolled back** to the last workflow
checkpoint. The framework guarantees the pipeline's `on_cancel` handler (if declared —
see §3 building block #25) runs after rollback but before the workflow transitions to
its terminal state. The terminal state is either `cancelled` (no resume) or
`paused-for-resume` (workflow resumes from the last checkpoint on next tick), declared
per-pipeline via `on_cancel.terminal_state`. Default: `cancelled` (operator must
explicitly opt into auto-resume to prevent infinite-retry on persistent failures).
**No torn workflow state:** the workflow checkpoint contract from §2.2 (single SQLite
tx / fsync'd JSONL append) extends to cancellation rollback — partial state IS the
torn-checkpoint case, treated as workflow loss + auditable failure.

**Critical invariant — trust-budget definition (formalized).** The `check-trust-budget`
governance pipeline (composed by default into every Surfaced pipeline per §2.4) reads
the per-scope budget state and refuses if the dispatch would exceed it. Concrete:

| Aspect | Allowed values | Notes |
|---|---|---|
| **Units** | `dispatch-count` \| `token-spend` \| `compute-time-ms` | Operator picks per deployment in the cluster manifest |
| **Scopes** | `global` \| `per-broker` \| `per-role` \| `per-pipeline` \| `per-agent` | Composable hierarchy; the **nearest scope wins** for the budget check |
| **Allocation** | `fixed-ceiling` \| `proportional` \| `time-decaying` | How operator-declared budget is distributed across the scope |
| **Replenishment** | `manual-operator-reset` \| `time-decay-per-hour` \| `metric-driven` | When budget refills |
| **Failure mode** | `refuse-dispatch` (default) \| `escalate-to-operator` | What `check-trust-budget` does on overage |

The `governance_pipelines()` sidecar exposes the current trust-budget state per scope to
the LLM (read-only — agent sees its budget but cannot adjust without OperatorConfirmed
tuning per [`BROKER-FRAMES.md`](BROKER-FRAMES.md) tunability tiers).

**Cross-broker trust-budget composition (per BB #27):** when a pipeline composes a
sub-step from another broker, both parties debit. The **unit-composition rule**: both
brokers must use the same trust-budget unit (`dispatch-count` matches `dispatch-count`,
`token-spend` matches `token-spend`). If a call crosses unit boundaries (calling broker
on `token-spend`, target broker on `dispatch-count`), the framework refuses the call
with `failure_reason: trust_budget_unit_mismatch` — operator must configure compatible
units across the composition graph OR declare an auto-conversion table per broker
manifest (`unit_conversion: { from: token-spend, to: dispatch-count, rate: 1000 }`).
Default: refuse on mismatch (no silent unit coercion).

**Unit-conversion tables are Untunable.** Autonomous tuners (per §"Tunability") cannot
modify `unit_conversion:` entries — these are operator-only edits. Unit-composition
validation runs as a **gate before** Skill Filter autonomous tuning each tick; the
tuner cannot inadvertently break cross-broker composition by adjusting weights that
affect unit conversion. Mismatch detection at registration time (broker manifests
loaded) and at composition time (`sub_pipeline:` resolution).

**Mismatch recovery surface (LB-1 closure).** Bare refusal leaves the agent stuck. The
framework emits a **`UnitMismatchProposal`** entry to the Proposal Ledger (BB #21) with
`type: unit-conversion-proposal`, fields `{calling_broker, target_broker, calling_unit,
target_unit, suggested_rate (if operator declared a partial conversion table), trace_id}`.
The Awareness Materializer (BB #24) surfaces pending proposals in L1 context. The
calling agent has three paths: (a) reformulate the dispatch to avoid the cross-broker
call, (b) dispatch the Surfaced governance pipeline `propose-unit-conversion`
(`tunability: operator-confirmed`) to request an operator-declared rate, (c) accept a
calling-broker-only fallback if the target broker exposes a `degraded-mode` variant.
The mismatch is therefore visible + recoverable, not silently terminal. Routed through
Proposal Ledger + Governance Composer — NOT the Sensory Queue (which is the Awareness
Service trust boundary for custom sensors, not a governance decision channel).

**Critical invariant — authority hierarchy:** when authority claims collide, the
hierarchy is:

> **Kill-switch (Untunable governance) > broker-escalation-finality (recursion-guard
> outcomes) > broker authority (cold/hot store decisions) > peer-dialogue Meta-lobe
> consideration**

This pins precedence across the framework: kill-switch sits *above* the dialogue, not
within it; broker-escalation-finality (a broker's recursion-guard outcome, including
operator-escalation results per "escalation finality" below) takes precedence over
broker authority's normal cold/hot-store decisions because escalation outcomes are
themselves a load-bearing governance event; broker authority precedes Meta-lobe input
because the broker owns its store; peer-dialogue consensus emerges *within* broker
authority's scope (Meta can't override the broker; the broker can't override its own
escalation outcome; broker-escalation-finality can't override the kill-switch).

**Specifically:** if a kill-switch fires during Meta's deliberation, the kill-switch's
governance authority wins immediately — Meta does NOT get to reopen the dialogue
post-kill-switch. If a broker's escalation finalizes mid-Meta-cycle, the broker
respects the finalized outcome and does not accept further Meta considerations on
that decision. See
[`../../cereGrim/docs/COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md)
§"Peer-dialogue integration contract" for the full integration details.

**Critical invariant — escalation finality:** when a recursion guard fires in a
peer-dialogue cycle and the framework escalates to operator, **operator input is
final, not a continuation of the dialogue.** Framework does NOT loop operator input
back into the cycle as a third party's "consideration." `escalation_is_final_when_recursion_guard_fires:
true` is the framework invariant; closes the "recursive recursion-guard" risk that
would have no bounded fixed point. Operator's decision becomes the workflow outcome
directly (accept Primary's last action, reject + rollback, or replace with
operator-specified output); auditable + recorded with `audit_class: governance` +
`decision_source: operator_escalation`.

**Critical invariant — `MaxBrokerDepth`:** the framework enforces a maximum
broker-wrapping depth at registration time. A broker wrapping a broker wrapping a
broker is *bounded* — the recursion has a fixed-point declaration the framework refuses
to exceed. Default `MaxBrokerDepth = 3` (broker → wrapper-broker → meta-wrapper);
operator-tunable per cluster manifest. **Distinct from cross-broker composition (BB
#27):** MaxBrokerDepth bounds *static* broker wrapping (the pattern diagram).
Cross-broker composition is *dynamic dispatch* and does NOT increase wrapping depth;
instead, a separate `MaxCrossBrokerCompositionDepth` (default 2, operator-tunable)
bounds the depth of sub-pipeline calls across broker boundaries. A → B → C
(cross-broker calls) is depth 2, allowed by default; A → B → C → D requires explicit
operator opt-in. Closes the "absorbing more decisions into
deterministic pipelines absorbs the agent itself" failure mode: at some level of
wrapping, the agent becomes a passive recipient of pre-decided actions with no
judgment surface. The depth bound prevents that by construction.

**Critical safety case:** the Awareness Service rate-limit enforcer
([`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"The Sensory Queue contract enforcer") is
`Untunable`. The LLM cannot demote its own oversight. If the operator wants to adjust
the rate limits, that's a code change *and* a deliberate operator decision — not an
LLM-driven tuning pipeline. Same applies to the Topology Broker self-bypass invariant
(ACL-mutation pipelines are Tier 3 plain functions, marked Untunable).

**Global escalation budget per turn (R-X-15 closure, Phase 9).** Recursion-guard
forbids peer-dialogue self-recursion (per COGNITION-LOOP §"Recursion-guard escalation
semantics") but does not address cascading escalations across brokers — operator's
decision on one escalation can trigger a fresh escalation from a different broker;
chain has no global bound. Defense: cluster manifest declares
`max_escalations_per_turn` (default: 3). Framework counts escalation events
(audit_class: governance, source-class escalation) per turn; if budget is exhausted,
the next escalation is refused with `failure_reason: escalation_budget_exhausted` +
the full escalation chain is dumped to BB #32 Telemetry as a `cascade-escalation`
incident report for operator review. Subsequent escalations within the same turn fail
the turn outright (Workflow Engine transitions to `failed-escalation-cascade`)
rather than queue. Operator sees the cascade chain + can either fix the pathological
config that triggered it or raise the budget if legitimate.

**Kill-switch arming rate limit + lockout (R-S-16 closure, Phase 9).** The
`arm-kill-switch` pipeline (Surfaced, OperatorOnly) is the framework's ultimate
governance lever; uncontrolled arming/disarming is itself a governance attack
(credential compromise → repeated arm/disarm cycles disrupt the cluster). Defenses:

- **Rate limit:** framework tracks kill-switch arming attempts per scope; if same
  scope is armed >3 times in 1 hour, emit a `frequent-kill-switch-arming` warning to
  BB #28 Diagnostics + flag in BB #32 Telemetry for operator review (audit_class:
  governance). Cluster manifest may tune the threshold; default is conservative.
- **Minimum armed duration:** kill-switch remains armed for a minimum
  `kill_switch_lockout_duration_seconds` (default: 300; 5 minutes) before disarm is
  permitted. Attempts to disarm before the lockout expires are rejected with
  `failure_reason: kill_switch_lockout_active`. Prevents rapid arm-disarm cycles that
  could be used to selectively interrupt specific workflows.

These do not prevent a determined attacker with operator creds from eventually
disrupting the cluster, but they raise the cost + create detection signal. Multi-
operator confirmation for global-scope kill-switch is deferred to B-59 (operator-UX
work) which needs the multi-operator-roles infrastructure to be useful.

---

## 5. Worked example — the Work broker decomposed

The Work broker is the obvious starter because [`SUBSTRATE-REUSE.md`](SUBSTRATE-REUSE.md)
already marks it as ~80% shipped (`neurogrim_sensory::backlog::next_ready()` + the
`neurogrim backlog next-ready` CLI). Lifting it into the framework:

### Pipeline Catalog (excerpt, illustrative YAML)

```yaml
# Surfaced pipelines (LLM sees, operator audits)

- id: dispatch-work-unit
  visibility: surfaced
  tunability: operator-confirmed
  params:
    work_unit_id: { type: string, source: overlay.active_work.ids }
  preconditions:
    - work_unit_exists: overlay.active_work.contains(work_unit_id)
    - work_unit_ready: overlay.active_work[work_unit_id].ready
  steps:
    - sub_pipeline: validate-work-unit-id
    - sub_pipeline: claim-work-unit
    - sub_pipeline: emit-dispatch-event
    - sub_pipeline: record-dispatch
    - sub_pipeline: refresh-overlay     # rerun work-broker-tick to publish new Overlay
  governance:
    compose: [check-trust-budget, check-kill-switch, record-dispatch, record-outcome]
  expected_effect: claims_work_unit

- id: groom-backlog
  visibility: surfaced
  tunability: operator-only
  ...

- id: propose-pipeline-deprecation
  visibility: surfaced
  tunability: operator-confirmed
  params:
    target_pipeline_id: { type: pipeline_id }
    reason: { type: string }
  ...

# Internal pipelines (broker plumbing; not in LLM offering)

- id: work-broker-tick
  visibility: internal
  tunability: operator-only
  preconditions: []  # runs every tick
  steps:
    - sub_pipeline: parse-backlog          # writes into broker working state
    - sub_pipeline: rank-by-tier           # writes into broker working state
    - sub_pipeline: filter-by-moscow       # writes into broker working state
    - sub_pipeline: check-readiness        # writes into broker working state
    - sub_pipeline: materialize-overlay    # curated subset of working state → Overlay (atomic swap)
  governance:
    compose: []  # internal; no kill-switch by default

- id: materialize-overlay
  visibility: internal
  tunability: operator-only
  # The curation policy: top-N ready work units only — not the whole ranked list.
  # Policy values live in cold; operator-tunable; framework enforces atomic swap.
  steps:
    - leaf: select_top_n_ready          # working state → staging copy
    - leaf: overlay_atomic_swap         # staging copy → Overlay (no torn reads)
  governance:
    compose: []

- id: parse-backlog
  visibility: internal
  tunability: operator-only
  steps:
    - leaf: read_backlog_file       # Tier 3 plain fn
    - leaf: parse_markdown_blocks   # Tier 3 plain fn
    - leaf: extract_work_units      # Tier 3 plain fn
  ...

- id: rank-by-tier
  visibility: internal
  tunability: autonomous  # weights are autonomous-tunable within bounds
  ...
```

### How a tick unfolds

1. **Tick fires** (file-injection or hook).
2. **Framework** runs `work-broker-tick` (Tier 2 internal pipeline).
3. **Step 1** (`parse-backlog`): leaf fns read BACKLOG.md / ROADMAP.md / execution.md.
   Tier 3 — no governance, no audit. Output goes into broker working state. If parse
   fails, the *pipeline* records the failure (the leaf fn's failure has semantic
   weight at the pipeline level).
4. **Step 2** (`rank-by-tier`): applies implement→groom→capture→idle. Weights pulled
   from the broker's autonomous-tunable cells. If the LLM has been deprioritizing
   "capture" recently, the weight reflects that. Result lands in working state.
5. **Steps 3–4** (`filter-by-moscow`, `check-readiness`): refine the ranking inside
   working state. The LLM does not see any of this — these are broker-private
   intermediate computations.
6. **Step 5** (`materialize-overlay`): the broker's **curation policy** runs —
   select_top_n_ready picks the curated subset (the top-N ready work units, not the
   full ranked list) and overlay_atomic_swap publishes it. The Overlay is updated
   atomically; consumers reading mid-swap see either the prior version or the new
   version, never a torn read.
7. **Hot-Store Materializer** surfaces the Overlay to the LLM at next-turn-tick
   (file-injection path: writes the Active Work cylinder's contents into
   `.claude/brain/broker/current-projection.md` for CLAUDE.md auto-load).
8. **LLM sees** the Active Work Overlay with N entries (not the whole backlog —
   curation is the broker's job). The catalog presents `dispatch-work-unit` as a
   surfaced pipeline whose `work_unit_id` parameter draws from `overlay.active_work.ids`.
   The LLM picks one and dispatches.
9. **Framework** runs `dispatch-work-unit` (Tier 1 surfaced pipeline). Each step
   carries audit, governance, the works.
10. **Step 5** of dispatch (`refresh-overlay`) re-invokes `work-broker-tick` — no
    duplicated code, composition handles it; new Overlay is published; LLM's next-tick
    read reflects the dispatch.

### What this gives us vs. today's `next_ready()`

- Today: the dispatch is opaque Rust; you read the source to know what it does.
  Tomorrow: the dispatch is data; you read the YAML to know what it does.
- Today: failures land in stderr.
  Tomorrow: failures land in the trace sink with full pipeline context.
- Today: no kill-switch — once `next_ready()` is called, it runs to completion.
  Tomorrow: kill-switch composes in by default; operator can interrupt mid-tick.
- Today: ranking weights are constants in code.
  Tomorrow: weights are tunable cells; the LLM can shape its own dispatch
  preferences within operator-defined bounds.

The behavior is the same; the audit / governance / tunability surface is what's new.

---

## 6. Authoring a new broker — the half-day walkthrough

The framework's **design target**: median half-day across an authoring distribution.
"Half-day" is not a single point — it's the *median* of a per-broker-class distribution
with documented variance. A sensor wrap is minutes; a thick MCP wrap with
operator-invented preconditions can be a full day. The half-day claim must be
calibrated against the distribution, not a single fixture.

**Calibration protocol** — S1-T runs a 5-broker authoring batch covering the variance:

| Authoring task | Expected band | Variance source |
|---|---|---|
| Sensor wrap (e.g., `coherence` sensor → Sense-role broker) | Minutes to <1h | Sensors already broker-shaped; nearly identity wrap |
| MCP tool wrap (operator-declared preconditions, governance composition) | 4-8h | Preconditions invented from tool semantics; thin-vs-thick parameter sourcing decision |
| Sense-role greenfield broker (cold-store schema + curation policy + projection logic) | 4-6h | Schema design + curation policy debate |
| InnateAbility-role greenfield broker (escalation paths + cognitive workflow) | 6-10h | Escalation contract design + judgment-vs-broker boundary |
| Multi-role broker (e.g., `[Sense, Embodiment]` Browser-like) | 6-10h | Role composition + cross-role data path design |

**S1-T calibration measurement** publishes `{min, median, max, per-task-class}` from
this batch. Median half-day is validated only when the batch median lands at ~4 hours.
The variance is the framework's honest story; reporting "half-day" as an average across
the distribution would hide the order-of-magnitude spread between sensor wraps and
multi-role greenfield brokers.

Validating that target rigorously matters — naive measurement (framework author writes
the reference broker against a frozen test fixture) is biased: the author knows exactly
what to do, and the frozen fixture skips real cold-store schema design + curation policy
debate + tunability triage. So:

- **S0-T exit measurement (target):** framework author authors the smallest-possible
  reference broker against a frozen fixture inside a half-day; full trace + governance +
  tunability + replay surface automatically. This is the **first-cut measurement** — it
  proves the framework primitives wire up, not that the half-day claim generalizes.
- **S1-T calibration measurement (bias-free):** an external NeuroGrim contributor —
  someone who did *not* write the framework — authors the Search Broker (the worked
  example below) end-to-end against a real (not frozen) substrate, inside a half-day.
  This is the **real validation** of the half-day claim. Until it lands, half-day is a
  design target, not a proven property.

Concretely, the steps to add a hypothetical **"Search Broker"** (perception of recent
searches across files + history + bookmarks, with pipelines for `dispatch-search`,
`pin-search-result`, etc.):

| Hour | What you do | What the framework does for you |
|---|---|---|
| **0:00–0:30** | Sketch the cold-store schema (what does a Search Broker remember? — recent queries, pinned results, source weights) + the working-state shape (loaded weights, recent-search ring buffer) + the **Overlay shape and curation policy** (what does the LLM see? — top-5 most recent unique searches + pinned results, capped at 50). Write the SQLite migration. | Cold-store backend, schema versioning, migration runner, Overlay primitive with atomic-swap |
| **0:30–1:30** | Write the YAML catalog: ~5 surfaced pipelines (`dispatch-search`, `pin-result`, `tune-source-weight`, `propose-source-deprecation`, `replay-search`), ~8 internal pipelines (tick projection, parse, rank, filter, dedup, store, learn-from-rejection, materialize-overlay). Declare governance policy + tunability per pipeline. | Catalog loader, schema validation, governance composer |
| **1:30–3:00** | Implement the leaf ops in Rust: ~15 plain functions (read search-history file, hit the search index, format result, hash query, etc.). Each is small + pure or atomic. | Pipeline Runner calls them via `Step::Leaf`; framework handles wrapping in trace/governance per the calling pipeline's policy |
| **3:00–3:30** | Implement `Broker` trait (~30 lines glue) + `InternalService` trait (~30 lines: project cold → hot). | The `read_hot` / `legal_pipelines` / `tick` machinery; the hot store; the materializer |
| **3:30–4:00** | Write the manifest (`brokers/search/manifest.toml`) declaring broker id, **role-set** (Search carries `[Sense, InnateAbility]` — Sense for "recent search results" perception, InnateAbility for "rank these candidates" cognition), cold-store path, catalog file, tunability defaults, topic names. Register in NeuroGrim's broker discovery. | Broker Registry, startup wiring, role-set scaffolding composition (Sense + InnateAbility spinal cords wire automatically), A2A endpoint if needed |
| **4:00–4:30** | Write the test fixture: a frozen substrate state + a frozen tick + expected `legal_pipelines()` output + a frozen dispatch + expected audit trail. | Replay harness, fixture loader, golden-file diff |

That's a working broker by lunch. The rest of the day (afternoon) is calibration — run
it against real usage, watch the trace sink, tune weights, promote/demote pipelines
between Tier 1 / Tier 2 as patterns emerge.

What you *didn't* write: the runner, the workflow engine, the kill-switch, the trust
budget, the rate-limiter, the replay tooling, the audit ledger, the proposal ledger, the
hot/cold projection mechanism, the tick source, the catalog hot-reload, the schema
validator, the materializer. All inherited.

This is what "broker as primitive" looks like in practice.

---

## 6.5 Operator UX requirements (R-X-6 closure, Phase 9)

The framework's complexity (38 BBs + manifest fields + tunability cells + composition
order + ACL grants + Frame defaults + ...) compounds over time into operator burden.
At 6-12 months in production with multiple operators or evolving deployment, the
following failure modes are near-certain without explicit operator-UX investment:

- **Drift accumulation:** different operators tune different brokers in incompatible
  directions; framework picks last-write; behavior becomes unpredictable.
- **Stale tuning:** original operator's tuning decisions become unintelligible to
  successor operators; configuration is "what's there" not "why it's there."
- **Capability bloat:** dead pipelines + deprecated brokers + quarantined sensors
  accumulate; the operator review queue grows; eventually operator stops reviewing.
- **Coherence regression:** the `harness-coherence` Brain domain score drops without
  any one operator's change being obviously responsible.

The framework MUST surface the following signals to operators (B-59 backlog scopes
the implementation; this section pins the requirements):

**Required operator-UX signals:**

1. **Coherence-drift dashboard.** A single Markdown surface (operator-side projection
   of BB #32 Operator Telemetry Summarizer) showing: (a) current `harness-coherence`
   domain score with delta from S2-launch baseline; (b) tuning-cells flagged as
   contradictory across brokers (e.g., "Context Broker tuned aggressive; Work Broker
   tuned conservative — these are incompatible"); (c) per-broker config-entropy
   measure (how much has this broker's config drifted from its initial state).

2. **Tuning-decision audit trail with rationale.** Every operator tuning change
   records `{timestamp, operator_id, broker_id, cell_id, old_value, new_value,
   rationale}`. Rationale field is required; framework refuses tuning changes without
   it. Future operators reading the trail see WHY each decision was made.

3. **Drift-warning alerting.** When config entropy or coherence drift crosses
   operator-tuned thresholds (default: ≥10% entropy increase per quarter; ≥15%
   coherence drop), framework emits a `governance` event to BB #28 Diagnostics +
   surfaces in BB #32 Telemetry. Operator gets an "investigate before drift becomes
   unmanageable" signal.

4. **Quarterly re-evaluation forcing.** Cluster manifest declares a
   `mandatory_tuning_review_cadence` (default: 90 days). Framework emits a
   `tuning_review_required` event at the cadence; operator must explicitly
   re-affirm or modify every Autonomous-tuned and OperatorConfirmed-tuned cell.
   Stale decisions surface; affirmed decisions get fresh provenance.

5. **Capability hygiene digest.** Weekly auto-generated summary of: dead pipelines
   accumulating; deprecated brokers awaiting retirement; quarantined sensors with
   chronic-warning flags. Operator gets a digestable review queue instead of
   constant per-event noise.

6. **Multi-operator coordination lock.** When ≥2 operators are configured for a
   cluster, the framework supports a per-broker "tuning lock" — operator A holds
   the lock; operator B's tuning attempts are rejected with `failure_reason:
   tuning_locked_by_operator` until A explicitly releases. Prevents incompatible
   tuning conflicts at the cell level.

**These are requirements, not implementations.** B-59 (filed in BACKLOG.md) scopes
the actual dashboard + tooling work, which is multi-day effort and depends on
having S2+ deployments to design against. Until B-59 lands, operators must
implement these signals manually (or accept the drift risk).

---

## 7. What this commits NeuroGrim to

The framework framing changes the NeuroGrim roadmap. Consuming-project staging
(e.g., cereGrim's S\*-T branch in
[`../../cereGrim/roadmap/ROADMAP.md`](../../cereGrim/roadmap/ROADMAP.md)) wants an
**S0-T** stage *before* S1-T: build the broker framework, then write the canonical
brokers (per [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md#canonical-broker-list)) as the
framework's first consumers. Doing the bespoke brokers first and retrofitting a
framework later is exactly the path this doc was written to avoid.

The S0-T deliverables, in dependency order:

1. **Layer B first** (Pipeline primitive + Runner + Catalog + Workflow Engine + Trace
   Sink + Replay tooling). Without these, brokers can't be built at all.
2. **Layer A next** (`Broker` / `InternalService` / `ExternalService` traits; `HotStore<T>`).
   Most existing NeuroGrim primitives slot in as backing impls.
3. **Layer C last** (Broker Registry, Tick Source, Workspace Manager formalization,
   Awareness Service enforcer, Governance Composer set, Skill Filter generic, Proposal
   Ledger protocol, Hot-Store Materializer). Most reuse shipped substrate; the net-new
   code is the Awareness Service enforcer + the Materializer.

Concrete next-step work item: file NeuroGrim-side tickets for the 38 building blocks,
sequenced per the above, with cross-references back to the relevant sections of this
doc. That backlog is what S0-T tracks against. **Building blocks #16 (Workspace Manager)
and #17 (Topology Broker) get trait scaffolds + role-spinal-cord defaults in S0-T;
their concrete impls land in their owning stages (S2-T for Workspace Manager, S1-T for
Topology Broker).**

---

## 8. Open design surfaces (the next pass)

This doc pins the internals, the tunability tiers, the framework split, the half-day
authoring claim. Things deliberately left for a later pass:

- ~~**Cross-broker pipeline composition.**~~ **CLOSED** — pinned in building block #27
  (Cross-Broker Composition Policy) above. Atomicity + ACL + trust-budget double-debit
  contract documented; YAML `composition_mode` field declared. The Work broker CAN
  include `sensory-broker/read-awareness-summary` as a sub-step.
- **Workflow priority + preemption.** When two workflows want to resume on the same
  tick and one is higher-priority, what's the preemption protocol? Cold-store-as-truth
  makes this safe; the policy is still open.
- **Skill Filter learning rate.** Autonomous tuning has *bounds* per the tunability
  tier, but the *rate* at which weights update from rejection signals isn't pinned.
  Too fast = overfitting to recent context; too slow = stale weights ignore new
  patterns. Worth a calibration discovery item before S1-T.
- **The proposal-ledger protocol.** The shipped `proposal-ledger.json` is general;
  the tuning-pipeline-specific schema (target pipeline ID, requested change, reason,
  operator decision, decision rationale) needs to be agreed and added to the registry.
- **Brokers across A2A.** If broker A on machine X wants to consume broker B's
  surfaced pipelines on machine Y, the framework needs an A2A pipeline-dispatch
  protocol. Out of scope for terminal-only S\*-T; in scope when the meta-lobe lands.

These all defer to live experience with the first broker built on the framework.
Premature design here is exactly the failure mode the half-day-authoring goal
guards against.
