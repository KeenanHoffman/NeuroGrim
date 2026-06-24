# NeuroGrim — Broker Internals

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

A surfaced pipeline's `GovernancePolicy` declares which of these are composed in.
Defaults: every surfaced pipeline gets `check-trust-budget` + `check-kill-switch` +
`record-dispatch` + `record-outcome` automatically. Operators can declare additional
governance pipelines per surfaced pipeline (e.g., `require-operator-confirmation` for
high-stakes actions).

The broker author provides: governance policy declarations per pipeline in YAML.

---

## 3. Building blocks

The full framework surface, split by what NeuroGrim provides vs. what a new-broker
author provides. Twenty-four building blocks across three layers.

### Layer A — Pattern primitives (architectural shapes)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 1 | **Broker capsule** | `Broker` trait | impl for one specific projection domain |
| 2a | **Overlay** (read-only contract surface) | Generic `Overlay<T>` (read-only to consumer, atomic-swap updates, versioned read, no-torn-read enforcement) | The `T` shape + a **curation policy** declaring what materializes into the Overlay (see [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"The Overlay contract") |
| 2b | **Working state** (broker-private) | Generic `WorkingState<W>` (full read/write inside the broker; not exposed) | The `W` shape (loaded catalog, workflow positions, Skill Filter weight cache, rate-limit counters, etc.) |
| 3 | **Internal Service** | `InternalService` trait + tick subscription | impl: project cold → working state; broker then materializes the curated subset into the Overlay |
| 4 | **Queue** | Reuse `neurogrim_core::queue` (shipped) | Topic names + payload schemas |
| 5 | **External Service** | `ExternalService` trait + queue-consumer scaffold | impl: ingest world → cold |
| 6 | **Cold Store** | Trait over SQLite/JSONL backends (reuse) | Schema migration files |

### Layer B — Pipeline primitives (universal unit)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 7 | **Pipeline type** | The struct + Serde derives | — |
| 8 | **Step type** | enum (Leaf / SubPipeline / Guard / Branch) | Leaf-op implementations (Rust fns) |
| 9 | **Pipeline Catalog** | Generic catalog + YAML/TOML loader + hot reload | Pipeline definitions in YAML |
| 10 | **Pipeline Runner** | The executor, suspension support, bootstrap layer | — |
| 11 | **Workflow Engine** | Cold-store-as-truth + hot-store positions + resume | — (just author pipelines that span ticks) |
| 12 | **Trace Sink** | Trace format + write path + replay-against-historical-state harness | — (trace is automatic) |
| 13 | **Replay tooling** | CLI + library API for `replay <pipeline_id> <historical_state>` | — |

### Layer C — Substrate composition (cross-broker glue)

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 14 | **Broker Registry** | Discover/load brokers at startup; the manifest schema (incl. role-set declaration) | A manifest declaration per broker (incl. its role-set) |
| 15 | **Tick Source** | Hook-driven + file-injection ticks; subscription API | Subscribe to tick events the broker needs |
| 16 | **Workspace Manager** (canonical Embodiment broker) — *trait + spinal-cord defaults in S0-T; concrete impl in S2-T* | The Embodiment-role spinal cord — Effector subordination, Workspace Queue, real-time-vs-queued dispatch, cross-effector synchronization, cross-role sense-feedback routing | Effector registration |
| 17 | **Topology Broker** (canonical Sense broker) — *trait + spinal-cord defaults in S0-T; concrete impl in S1-T* | Cross-broker routing, ACL enforcement, per-consumer Topology Overlay, subscription fanout, ACL-mutation self-bypass invariant | Initial ACL definitions per broker |
| 18 | **Awareness Service** | The Sensory-Queue rate-limit/sanitize enforcer; **the role-boundary guard** for the Sense cross-role write path | Sensor schemas + redaction rules + rate budgets |
| 19 | **Governance Composer** | The set of Tier 2 governance pipelines (see §2.4) | Per-pipeline policy declarations in YAML |
| 20 | **Skill Filter** | Generic rank/dedupe/learn-from-rejection; **enforces the reachability channel split** (governance pipelines exposed via `governance_pipelines()` sidecar, distinct from `legal_pipelines()` capability ranking); per-broker/per-pipeline/per-role hygiene classifier with `audit_class` filter | Per-broker weight cells + tuning policy |
| 21 | **Proposal Ledger** | Reuse `.claude/brain/proposal-ledger.json` (shipped); the tuning-pipeline protocol | Tuning-pipeline definitions in the catalog |
| 22 | **Hot-Store Materializer** | Writes per-broker Overlay state to `.claude/brain/broker/segments/overlay.md`; composed by the Composer (bootstrap-layer Tier 3) into `current-projection.md` | — |
| 23 | **Role-set scaffolding** | Per-role defaults the framework wires automatically on broker registration (Sense / InnateAbility / Embodiment spinal cords); composes for multi-role brokers | Role-set declaration in manifest |
| 24 | **Awareness Materializer** | Writes pipeline catalog routing signals (description + when_to_use per Surfaced pipeline + alive/dead/new hygiene status) to `.claude/brain/broker/segments/awareness-routing.md`; composed by the Composer into `current-projection.md`. See [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §2 for the L1 awareness slot details. **Collision-safe with #22 by construction:** each materializer writes to its own segregated file; the Composer (Tier 3 bootstrap) concatenates them in operator-declared order. No last-writer-wins, no merge-on-write coordinator needed. | — |

**Totals:** ~18 framework-side blocks (write once for NeuroGrim, all brokers benefit
across all role-set compositions); ~8 broker-author blocks (mostly declarative — YAML
schemas, manifest with role-set declaration, weight cells, curation policies, a
handful of leaf-op functions). **24 building blocks total across three layers.**

**Deprecation discipline.** Every new building block added to this table MUST carry a
`displaces` / `deprecates` consideration (may be explicitly empty — `displaces: nothing`
— but must be considered, not omitted). The framework refuses to grow indefinitely; new
blocks must justify either net-new surface OR replacement of existing surface. This
column is informal in the v1 table (the discipline is stated here; per-row annotation
lands as the table evolves) but enforced going forward. Examples:
- BB #24 (Awareness Materializer) `displaces: nothing` — net-new surface; mirrors #22
  Hot-Store Materializer at a different abstraction level (routing-signal injection vs
  file-projection injection).
- A hypothetical future "Unified Materializer" replacing both #22 and #24 would carry
  `displaces: #22, #24` — the operator can see the consolidation intent at a glance.

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

**Critical invariant — `MaxBrokerDepth`:** the framework enforces a maximum
broker-wrapping depth at registration time. A broker wrapping a broker wrapping a
broker is *bounded* — the recursion has a fixed-point declaration the framework refuses
to exceed. Default `MaxBrokerDepth = 3` (broker → wrapper-broker → meta-wrapper);
operator-tunable per cluster manifest. Closes the "absorbing more decisions into
deterministic pipelines absorbs the agent itself" failure mode: at some level of
wrapping, the agent becomes a passive recipient of pre-decided actions with no
judgment surface. The depth bound prevents that by construction.

**Critical safety case:** the Awareness Service rate-limit enforcer
([`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"The Sensory Queue contract enforcer") is
`Untunable`. The LLM cannot demote its own oversight. If the operator wants to adjust
the rate limits, that's a code change *and* a deliberate operator decision — not an
LLM-driven tuning pipeline. Same applies to the Topology Broker self-bypass invariant
(ACL-mutation pipelines are Tier 3 plain functions, marked Untunable).

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

Concrete next-step work item: file NeuroGrim-side tickets for the 24 building blocks,
sequenced per the above, with cross-references back to the relevant sections of this
doc. That backlog is what S0-T tracks against. **Building blocks #16 (Workspace Manager)
and #17 (Topology Broker) get trait scaffolds + role-spinal-cord defaults in S0-T;
their concrete impls land in their owning stages (S2-T for Workspace Manager, S1-T for
Topology Broker).**

---

## 8. Open design surfaces (the next pass)

This doc pins the internals, the tunability tiers, the framework split, the half-day
authoring claim. Things deliberately left for a later pass:

- **Cross-broker pipeline composition.** Can the Work broker include the Sensory
  broker's `read-awareness-summary` as a step in its own pipelines? Probably yes, but
  the trust + governance implications across brokers need design.
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
