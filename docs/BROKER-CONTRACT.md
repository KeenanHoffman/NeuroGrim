---
doc-version: 1.0
date: 2026-06-30
status: draft
anchored-to: none
front-door: false
---

# NeuroGrim — Broker Contract

> **Status: DRAFT until 6 months post-launch (R-X-14 closure, Phase 9).** Spec is
> comprehensive at the named-primitive level (38 BBs, Phase 8) but no reference
> implementation has run yet. First implementation (S0-T) will discover spec gaps;
> patches accrue to [`BROKER-SPEC-GAPS.md`](BROKER-SPEC-GAPS.md) and propagate back
> into this doc + companions. Stability is measured against working code, not against
> design documents. Treat consumers (cereGrim, others) as adopting a draft contract;
> expect ≥2 patch cycles before the spec is battle-stable.

This document is the **named specification** for what a "broker" is in NeuroGrim: the shape
of each capsule, the obligations the pieces hold toward one another, and the placement
boundary the pattern carves between LLM-class hardware and host-class hardware. It is the
contract that the `harness-coherence` and `substrate-reuse` Brain domains in consuming
projects (e.g. cereGrim) score against.

The visual reference is [`diagrams/broker-pattern.mmd`](diagrams/broker-pattern.mmd)
(v4 Mermaid source, rendered from `diagrams/DIAGRAM-V4-SPEC.md`).
This document is the prose of that diagram — when the two disagree, the prose wins
(the diagram is generated from understanding; the contract is the understanding).

This is **substrate infrastructure** — the broker pattern as an architectural primitive
NeuroGrim provides for any agent harness to consume. Consuming-project-specific design
articulation and adoption rationale (e.g., cereGrim's reasons for adopting brokered cognition
in its dual-lobe harness) live in those projects' own documentation (e.g.,
`../../cereGrim/thesis/`, proprietary to that subproject). The IP-boundary policy + the
enumerated list of claims that stay in consuming-project documentation rather than this
public spec lives in [`PUBLIC-VS-PROPRIETARY.md`](PUBLIC-VS-PROPRIETARY.md).

---

## Glossary

Terms used precisely throughout this spec. Other documents (especially
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md), `BROKER-AWARENESS.md`, `BROKER-WRAPPING.md`,
and consuming projects' composition docs) reference these definitions rather than
re-spelling them.

| Term | Definition |
|---|---|
| **broker** | A deterministic substrate component that projects currently-legal pipelines from substrate state at agent-tick cadence. Carries a *role-set* (see below). Unqualified "broker" = intra-agent broker; "inter-agent broker (IAB)" used when at the cluster level. |
| **intra-agent broker** | A broker that mediates between substrate and one agent's LLM (the canonical brokers Context, Workspace, Sensory, Topology, Work, Workspace Manager + Effectors). |
| **inter-agent broker (IAB)** | A broker that mediates between agents in a cluster (the cluster-level recursion of the broker pattern). Same primitive, one level up. Documented in cereGrim's INTER-AGENT-BROKER.md stub; framework substrate eventually upstream. |
| **cognitive-agent** | The LLM at the center of one harness from that harness's perspective. The thing the canonical broker set wraps. |
| **peer-agent** | The same harness viewed externally by another agent in a cluster. The IAB sees peer-agents via their A2A agent-cards; their internal cognitive-agent is opaque. |
| **broker-role** | One of `Sense` / `InnateAbility` / `Embodiment`. Brokers declare a *role-set* (subset); framework composes role-specific scaffolding. See §"Broker roles". |
| **cluster-role** | An IAB-layer agent role declared in the cluster manifest (e.g., `Coder`, `Reviewer`, `PM`, `Security-Auditor`). Distinct from broker-role: a *cognitive-agent* may carry multiple cluster-roles via its peer-agent agent-card. |
| **Pipeline** | The broker's output unit — a named, parameterized, currently-legal sequence of actions the consumer can dispatch. Surfaced (LLM-facing) or Internal (broker plumbing). Documented in BROKER-INTERNALS §1. |
| **cluster-pipeline** | The IAB-layer construct: a typed contract between peer-agents, dispatched at the cluster level. Same Pipeline primitive, one level up. Named distinctly to preserve the word "contract" for substrate invariants (Overlay contract, broker contract). |
| **contract** | A substrate-level invariant — the Overlay contract (read-only, atomic-swap, no-torn-read), the broker contract (this document), the workflow checkpoint atomicity contract, etc. NOT used for IAB cluster-pipelines. |
| **Overlay** | (tier-1) The broker's read-only, consumer-facing projection of its working state. Atomic-swap, no-torn-read. Per-broker curation policy. The contract surface. |
| **OverlayView** | (tier-2) A derived/filtered projection layered on top of one or more Overlays. The Topology Broker's per-consumer ACL-filtered topology is an OverlayView (each consumer-broker sees a different filtered view of the broker registry). |
| **OverlayMesh** | (tier-3) A cluster-aggregated projection across multiple peer-agents' Overlays. The IAB's cluster-Sense projection (when wired) is an OverlayMesh. Consistency model is explicit per the IAB stub (snapshot-on-read or arbiter-mediated). |
| **Tier 1 / Surfaced** | (P-5) A pipeline's visibility classification. Synonymous: "Tier 1" (logical layer, used in BROKER-INTERNALS.md §1.2 diagram), "Surfaced" (semantic name, used in prose), and `visibility: surfaced` (YAML manifest field value). LLM sees + picks; operator audits; default governance composed. |
| **Tier 2 / Internal** | (P-5) A pipeline's visibility classification. Synonymous: "Tier 2" (logical layer), "Internal" (semantic name), and `visibility: internal` (YAML manifest field value). Broker plumbing; not in LLM offering; still traced + governed. |
| **Tier 3 / plain function** | (P-5) The bootstrap-only execution layer — Rust functions called from inside a pipeline step's Leaf body. Never enters the catalog, never gets a trace. Reserved for the executor (Pipeline Runner, Catalog Loader, ColdStore opener); not used for general broker logic. |
| **Type erasure at broker boundary** | (V0-RETROSPECTIVE §B1) The `Broker` trait's surface methods return `serde_json::Value` for Overlay state rather than typed associated types, so `Arc<dyn Broker>` is dyn-compatible + the Broker Registry holds heterogeneous brokers across the framework. Concrete brokers retain typed `Overlay<T>` + `WorkingState<W>` INTERNALLY; they JSON-serialize at the trait boundary. Trade-off: dyn-compatibility (load-bearing for the Registry pattern) wins over compile-time type-safety at the consumer boundary. Consumers see Value not typed structs. Materializer projects to Markdown anyway, so the loss is minor where it matters. Pinned in V0 implementation per BB #1 row clarification. |

**See also (Frame-related terms):** the Frame primitive is documented in
[`BROKER-FRAMES.md`](BROKER-FRAMES.md) (currently a stub). When it matures, this
glossary will absorb the seven Frame-type terms — `Hat`, `Stakes`, `Tempo`, `Mode`,
`Confidence`, `Audience`, `Scope` — plus `Frame stack` and `Frame-rotation` and
`broker-prescribed Frame`. Until then, refer to BROKER-FRAMES §2 for the Frame-type
taxonomy.

---

## What a broker IS

A **broker** is a deterministic substrate component that **projects currently-legal pipelines
from substrate state at agent-tick cadence.** Its output answers: *given what the world is
right now, what is the next move's option set?*

It is **not** a static tool catalog (MCP). It is **not** a sensor that scores (NeuroGrim's
sensor/domain split). It is **not** an LLM with prompts. It is a finite-state-machine over
a typed store, with a `legal_pipelines(state) → Vec<Pipeline>` function that the LLM
consumes instead of being handed a catalog of capabilities and asked to choose.

The LLM never sees a capability whose preconditions aren't met. **This is the broker's central invariant.**

---

## Broker roles — composable role-set, one framework primitive

The broker pattern is uniform — every broker uses the same 6-piece (LLM-level) or
3-piece (terminal-level) shape, the same Pipeline primitive, the same Overlay contract,
the same governance composition, the same tunability tiers. But brokers play
**distinct architectural roles** in the agent's overall architecture, and the framework
provides **role-specific scaffolding** (the "spinal cord" for that role) so a broker
that declares a role inherits the right coordination behavior automatically.

Roles are a **role-set**, not a role-class: a broker declares a *set* of roles it carries
(subset of `{Sense, InnateAbility, Embodiment}`), and the framework composes scaffolding
from every declared role. Single-role brokers are common; multi-role brokers are
first-class — Browser carries `[Sense, Embodiment]`; Custom Sensor carries
`[Embodiment-afferent]` (an Effector whose outputs feed into Sense via the Sensory Queue).

| Role | What it is | Read/write balance | Consumer |
|---|---|---|---|
| **Sense** | A way for the agent to *know about* something — perception of substrate | Read-mostly *from the consumer's perspective* (Overlay is read-only); cold may receive writes via the sanctioned Sensory-Queue + enforcer path | LLM (Primary) |
| **Innate Ability** | A specialized cognitive function the agent has by nature — narrow judgment the broker absorbs | Read + write: broker has internal logic that processes and emits | LLM (Primary) |
| **Embodiment** | The agent's *hands and feet* — how it acts on reality through Effectors | Outbound-dominant: dispatches to Effectors; reads back effect results | LLM (via the Embodiment broker's dispatch surface) |

**Spinal cords per role.** Each role has its own coordination machinery — the "spinal
cord" — that the framework wires up when a broker declares the role. A multi-role broker
gets the union of spinal-cord scaffolding from each declared role.

- **Sense role spinal cord** — LLM-read-only Overlays; cold stores may receive writes from
  Custom Sensor Effectors via the Sensory Queue, mediated by the Awareness Service
  enforcer — the **sanctioned cross-role write path** (the enforcer is the role-boundary
  guard, not just the trust-boundary guard). Broadcast subscription model for
  delta-perception across Sense brokers.
- **Innate Ability role spinal cord** — escalation paths (escalate-to-lobe-only-on-ambiguity),
  dispatch chains, intra-role composition for cognitive workflows.
- **Embodiment role spinal cord** — Workspace Manager IS the canonical Embodiment spinal
  cord; coordinates Effectors, manages real-time-vs-queued delivery via the Workspace
  Queue, handles cross-Effector synchronization.

The framework commits to *recognizing* the role-set on broker registration and providing
role-appropriate defaults. The specifics of per-role scaffolding fill in as brokers within
each role are built.

**Multi-role brokers are first-class.** A broker is not partitioned into a single role.
The framework records the broker's role-set in its manifest and composes scaffolding from
every declared role. Examples:
- Browser broker (`roles: [Sense, Embodiment]`) — reads page content (Sense) and dispatches
  clicks/types (Embodiment). Inherits both Sense-role Overlay-curation discipline and
  Embodiment-role Effector subordination.
- Custom Sensor (`roles: [Embodiment-afferent]`) — registered with Workspace Manager as an
  Effector (Embodiment role), but its outputs publish to the Sensory Queue (Sense
  contribution mediated by the Awareness Service enforcer). The framework distinguishes
  *efferent* (motor: IDE click, Browser type) from *afferent* (sensor: Custom Sensor
  emission) at the Embodiment role's manifest level.
- Topology Broker (`roles: [Sense]`) — single-role, but its surfaced pipelines include
  ACL mutations (see §"Topology Broker self-bypass invariant" below); the role-set is
  unchanged.

The Meta lobe scope (see §"Meta lobe access" below) reads overlays from brokers carrying
the Sense role; multi-role brokers expose per-role sub-overlays so Meta sees only the
Sense portion.

---

## The two broker patterns

The diagram shows two reduced forms of the same primitive — one for brokers that live
on LLM-class hardware (where the queue is a placement boundary) and one for brokers that
live at terminal level (where there's no further substrate to push compute into).

### LLM-level broker (6 pieces)

The full pattern, used by the LLM-level brokers (see [Canonical brokers](#canonical-broker-list) below for the current set).

```
              ┌──────────────────────────────────────────────────────────┐
              │  LLM-class hardware (broker + internal service + hot)   │
              │                                                          │
   LLM ◄──────┤  Broker  ◄──read──  Hot Store  ◄──write──  Internal     │
              │     │                                       Service     │
              │     │  legal_pipelines(hot)                    │        │
              └─────┼──────────────────────────────────────────┼────────┘
                    │                                          │
                    │ (broker writes commands back the way)    │  read
                    ▼                                          ▼
              ┌──────────────────────────────────────────────────────────┐
              │  Host-class hardware (queue + external service + cold)  │
              │                                                          │
              │  Queue  ──►  External Service  ◄──read/write──  Cold    │
              │     ▲                  │                        Store   │
              │     │                  │                          ▲     │
              └─────┼──────────────────┼──────────────────────────┼────┘
                    │                  ▼                          │
              ┌─────┴────────────────────────────────────────────┴────┐
              │     World events  (terminals, sensors, network)        │
              └────────────────────────────────────────────────────────┘
```

| Piece | Role | Hardware tier |
|---|---|---|
| **Broker** | Maintains the Overlay, answers `legal_pipelines(state)` to the LLM, writes commands toward the queue, runs its internal pipelines over private working state | LLM-class |
| **Overlay** | The read-only, LLM-facing projection the broker maintains; agent-tick cadence; per-broker curation policy (see §"The Overlay contract") | LLM-class |
| **Internal Service** | Projects from cold → broker's working state at agent-tick cadence; broker then materializes the relevant subset into the Overlay | LLM-class |
| **Queue** | The placement boundary. Bounds clock-skew between LLM-tick and world-tick; absorbs back-pressure | Seam |
| **External Service** | Owns the cold store; ingests world events into cold at world-tick cadence; serves the internal service's projection reads | Host-class |
| **Cold Store** | The persistent, durable "details" store; the long-form truth the broker only summarizes | Host-class |

The broker also keeps **broker-private working state** in RAM (loaded Pipeline Catalog, workflow position pointers, Skill Filter weight cache, rate-limit counters, pending governance decisions). This is implementation, not contract, and is not exposed to the LLM. The Overlay is the only LLM-readable hot-tier surface; the LLM never reads the broker's working state directly.

**The queue is the load-bearing piece.** Without it, agent-tick and world-tick clocks lock
together and you lose the placement boundary. With it, LLM-class hardware projects at LLM
rate and host-class hardware ingests at world rate, and neither has to wait on the other.

### Terminal-level broker (3 pieces, reduced)

Used for brokers that already live on host-class hardware — IDE, Browser, Custom Sensor.
There is no further substrate to push compute into, so the internal service and queue
collapse out.

```
   ┌────────────────────────────────────────────────────────────────┐
   │  Host (terminal already external — no internal-service tier)  │
   │                                                                │
   │  Broker  ◄──read──  Hot Store  ◄──write──  External Service   │
   │     │                                            │             │
   │     │  legal_pipelines(hot)                      │  read/write │
   │     ▼                                            ▼             │
   │   (consumer)                                  Cold Store        │
   │                                              "Skills & Config"  │
   └────────────────────────────────────────────────────────────────┘

   Queuing on this broker's behalf — when the consumer is busy — is the
   Workspace Manager's responsibility, not the terminal broker's.
```

| Piece | Role |
|---|---|
| **Broker** | Same maintenance role; the Overlay's consumer is the Workspace Manager, not the LLM directly |
| **Overlay** | The read-only Manager-facing projection of what the terminal currently looks like (e.g., the IDE Overlay cylinder in the diagram); per-broker curation policy |
| **External Service** | The terminal program itself (the IDE, the Browser engine, the sensor process) |
| **Cold Store ("Skills & Config")** | The persistent pipeline catalog + governance rules for this terminal — *which pipelines exist at all, before legality is checked* |

Terminal-level brokers connect upward to the **Workspace Manager**, which handles real-time
delivery vs. queuing on the broker's behalf.

---

## The Workspace Manager — the canonical Embodiment broker

The Workspace Manager is the **agent's motor cortex** — the only canonical instance of
the Embodiment role (per the role-set vocabulary in §"Broker roles" above), and the framework's reference for what an Embodiment
spinal cord does. It has three responsibilities:

1. **Coordinate Effector brokers.** IDE Broker, Browser Broker, Custom Sensor (and
   any operator-added effector) register as the Manager's subordinates. The LLM
   dispatches actions through the Manager, not directly to the Effectors. New effectors
   inherit the Manager's coordination machinery (queuing, real-time-vs-queued
   delivery, cross-effector synchronization) automatically on registration.
2. **Provide a real-time-or-queued dispatch path.** When the LLM is idle, the Manager
   forwards actions in real time. When the LLM is busy, the Manager queues actions into
   the **Workspace Queue** and replays them when ready. Effectors do not own queues —
   the Manager owns the one shared queue on their behalf.
3. **Bridge cross-class flows.** Many actions produce both an Embodiment effect *and*
   sensory feedback (e.g., a Browser click loads a new page; that's an Embodiment
   dispatch AND a Sense observation). The Manager dispatches the action and routes the
   sensory output through the Sensory Queue to the Sensory Broker. The framework
   distinguishes the two paths internally so neither the Embodiment audit trail nor the
   Sense Overlay confuses the other class's data.

The Manager owns its own persistent layer (**"Workspace Details"** in the diagram) for
cold-storage and metadata that crosses Effector boundaries — which IDE pane is
currently active, what URL the Browser is on, what custom sensors are registered.
Effector cold stores hold *that effector's* config; Workspace Details holds
*cross-effector* state owned by the Manager.

**Note on naming:** the Workspace Manager and the [Workspace Broker (Sense)](#the-workspace-broker--workspace-manager-distinction--the-subtle-boundary)
share subject matter (the workspace) but carry different role-sets. Keeping them
separate is the load-bearing discipline — perception of the workspace and action on it
are different concerns, served by different brokers carrying different role-sets.

---

## The Sensory Queue contract enforcer

The Sensory Queue is the **operator's trust boundary**: anything the operator attaches
(custom sensors, third-party signal sources) writes into this queue, and the Awareness
Service is the only consumer. Without an enforcer, a noisy or malformed custom sensor
would poison the Awareness Map and through it, the Meta lobe's view of ground truth.

The Awareness Service therefore **must** apply, before any write to the Awareness Map
hot store:

1. **Per-source rate limits.** A misbehaving sensor's write rate is bounded — it cannot
   exceed its configured budget, intentional or not. Excess is dropped (not buffered;
   buffering shifts DoS into memory pressure).
2. **Payload-shape validation.** The sensor must emit messages that match its declared
   schema. Schema mismatch is dropped + counted.
3. **Redaction rules.** Configurable patterns (secrets, PII) are stripped before the
   payload enters the hot store. The unredacted payload may still land in the cold
   store under tighter access control; the hot store sees only the safe form.
4. **Source attribution.** Every Awareness Map entry carries which sensor produced it.
   The Meta lobe (when wired) can weight entries by source trust without re-reading
   raw queue traffic.

This is **mandatory**, not advisory. A custom-sensor extension model where any
operator-defined source can write directly to the hot store is not a valid cereGrim
broker configuration. If you skip the enforcer, you've broken the contract.

---

<a id="canonical-broker-list"></a>
## Canonical brokers — single source of truth

The canonical baseline is a starting set, not a closed contract. The framework permits
new brokers (a new broker is a new manifest + role-set declaration + cold-store schema +
catalog + leaf-ops); the framework composes its scaffolding from each broker's declared
roles.

**This section is the *only* enumeration of canonical brokers in any NeuroGrim or
consuming-project doc.** Other documents reference this list by anchor link
(`#canonical-broker-list`); they do not re-spell it. If you find a doc re-spelling the
broker list, update it to reference this section instead — the single-source-of-truth
discipline prevents the count drift this contract previously suffered.

### Current canonical brokers (LLM-level)

| Broker | Roles | Overlay (LLM reads) | Cold store | Notes |
|---|---|---|---|---|
| **Context Broker** | `[Sense]` | Context KV Projection | Context Details | Wraps `BrainContext::load()`; projects currently-relevant CLAUDE.md / MEMORY.md / skills slice |
| **Workspace Broker** | `[Sense]` | Workspace Map | Workspace Details | Read-only perception of open files, IDE chrome, gitStatus, recent edits. **Distinct from Workspace Manager** (see boundary callout below) |
| **Sensory Broker** | `[Sense]` | Awareness Map | Awareness (cold) | Continuous delta-perception; cold receives writes from Custom Sensor Effectors via Sensory Queue + Awareness Service enforcer |
| **Topology Broker** | `[Sense]` | Topology **OverlayView** (per-consumer; ACL-filtered tier-2 projection over Broker Registry) | Broker Registry + ACL Definitions | Cross-broker routing + ACL. Read-mostly OverlayView; ACL-mutation pipelines bypass Topology routing (see §"Topology Broker self-bypass invariant") |
| **Work Broker** | `[InnateAbility]` | Active Work | Backlog | Next-ready dispatch. Wraps NeuroGrim's shipped `next_ready` |

### Current canonical brokers (Embodiment role — motor coordinator)

| Broker | Roles | Overlay (consumer reads) | Cold store |
|---|---|---|---|
| **Workspace Manager** | `[Embodiment]` | Workspace Details (cross-Effector state) | Workspace Manager Persistent State |

The Workspace Manager has subordinate **Effector brokers** registered to it — NOT peers,
but limbs. Each Effector follows the terminal-level 3-piece pattern and is dispatched
*through* the Workspace Manager, not directly by the LLM. New Effectors register with
the Workspace Manager at startup and inherit its coordination machinery automatically.

### Current Effectors (subordinate to Workspace Manager)

| Effector | Roles | Notes |
|---|---|---|
| **IDE Broker** | `[Embodiment]` (efferent) | Wraps IDE state surface (chrome, mission strip, pane registry, paneHostPolicy) |
| **Browser Broker** | `[Sense, Embodiment]` | Multi-role: senses page content + dispatches clicks/types. Sense outputs route to Sensory Broker via Sensory Queue; Embodiment inputs dispatched via Workspace Manager |
| **Custom Sensor** | `[Embodiment-afferent]` | Effector that publishes to Sensory Queue. Operator-extensible; mediated by Awareness Service enforcer |

### Future canonical candidates (queued, not yet built)

Memory Broker, Planning Broker, Tool-Selection Broker — each would absorb a specialized
cognitive function per the broker pattern. Roles: `[InnateAbility]`. Filed in consuming
projects' roadmaps (e.g., cereGrim's D-MEMORY-BROKER discovery item).

### Topology Broker self-bypass invariant

The Topology Broker routes cross-broker traffic through ACL — but **ACL definitions themselves are
mutable** via dispatched pipelines (`update-acl`, `propose-acl-grant`, etc.). If those
mutations routed through the Topology Broker, the routing would check against the very ACL it's
about to mutate, creating an infinite regress (or worse: a stale-ACL check followed by an
ACL change that should have failed it).

**Invariant (M-12 clarification):** ACL-mutation pipelines are **Tier 2 Internal pipelines
marked Untunable** (NOT Tier 3 plain functions, which are bootstrap-only). They live in
the catalog (so they are traced + auditable + appear in `governance_pipelines()` sidecar)
but the framework dispatches them via a **direct self-bypass routing path** — the
Topology Broker's mutation handler executes them without consulting its own ACL
enforcement (no infinite regress). Documented as part of building block #17 in
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3. Cross-broker composition of these
pipelines via `sub_pipeline:` is rejected at catalog load per BB #27 §(f) (governance
pipelines compose only as Tier 2 across brokers, never as Tier 3 plain-function calls
— preserves audit accountability across composition).

### The Workspace Broker / Workspace Manager distinction — the subtle boundary

These two brokers share subject matter (the workspace) but belong to different
role-sets, and the distinction is **load-bearing**:

| | Workspace Broker (Sense) | Workspace Manager (Embodiment) |
|---|---|---|
| Role-class | Sense | Embodiment |
| Direction | Inbound: senses workspace state | Outbound: acts on workspace state |
| Consumer | LLM reads Workspace Map Overlay | LLM dispatches actions through it |
| Mutability | Read-only Overlay | Dispatch surface (writes to world via Effectors) |
| Failure mode if conflated | LLM perception and action collide in one Overlay; read-only contract breaks; audit trail merges senses and dispatches |

The temptation to conflate them is real — "workspace" suggests one concern. The
discipline of keeping them separate is what makes the role-set taxonomy coherent.
A future contributor reading this contract should leave knowing: **perception of the
workspace and action on the workspace are different brokers, carrying different role-sets,
sharing only the substrate they observe and act upon.**

## The Overlay contract

The Overlay is the broker's **read-only contract surface to its consumer.** It is the
only hot-tier surface the LLM (or, for terminal brokers, the Workspace Manager) is
allowed to read. The broker maintains it; the consumer reads it; nobody else writes to
it. The contract has three obligations:

1. **Continuous availability.** The Overlay is always readable, always consistent,
   always current relative to the broker's most recent projection cycle. The broker
   may be running internal pipelines, executing a workflow, mid-governance check —
   none of that affects Overlay readability. Implementation: atomic-swap updates from
   a staging copy, or versioned reads — the framework enforces the no-torn-read
   property.
2. **Per-broker curation policy.** What goes into the Overlay is a broker-author
   decision, declared in the catalog. The framework provides the mechanism; the
   broker provides the policy. Curation policies legitimately differ:

   | Broker | Policy | Rationale |
   |---|---|---|
   | Sensory | "Everything" — full Awareness Map | Broad situational awareness |
   | Work | "Most relevant" — top-N ready work units | Filter the backlog so the LLM doesn't have to |
   | Context | "Currently relevant for the active sub-task" | Context-window pressure makes narrow projection the win |
   | Workspace | "Open + recent" — active files, current cell topology | Historical workspace state isn't useful per-tick |
   | IDE / Browser (terminal) | What the Workspace Manager needs to route | Manager-facing, not LLM-facing, but same curation discipline |

3. **Read-only enforcement.** The framework physically enforces read-only access:
   the consumer cannot mutate the Overlay even if it tries. Mutation paths are the
   surfaced pipelines the broker offers (which the broker then translates into Overlay
   updates via its internal pipelines). This is the load-bearing safety property that
   makes the broker the sole authority over what the LLM sees.

4. **Budget + eviction discipline.** Curation policy is re-evaluated on **every
   projection cycle** (not memoized across ticks — broker state changes between ticks
   and curation must reflect current state). The broker declares a budget per Overlay
   (operator-tunable per cluster manifest, default 4KB hot-state per broker). The
   curation function MUST produce an Overlay that fits the budget; if it cannot:
   - The broker raises a `curation-budget-exceeded` alarm to the Diagnostics Collector
     (see [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) building block #28).
   - The broker falls back to the **last-known-fitting Overlay** (no torn read; no
     incomplete state surfaced).
   - The fallback is logged in the audit trail with `audit_class: governance` +
     `failure_reason: curation_budget_exceeded`.
   This closes the failure mode where long-running deployments grow the broker's
   working state beyond the curation policy's ability to summarize it — the broker
   degrades visibly rather than silently truncating mid-Overlay.

The Overlay is **distinct from the broker's private working state** — the loaded
Pipeline Catalog, workflow position pointers, Skill Filter weight cache, rate-limit
counters. Working state is implementation; the Overlay is contract. The framework
provides separate primitives for both ([`BROKER-INTERNALS.md`](BROKER-INTERNALS.md)
§3 building blocks #2a and #2b).


---

## Meta lobe access

(Meta-lobe semantics are a *consuming-project concern*; NeuroGrim's broker framework
provides the read-scope mechanism but does not itself run a Meta lobe. The constraint
documented here is the read-scope contract that consuming projects like cereGrim
implement.)

The Meta lobe's read-only access is **narrower than Primary's** by design, defined by
broker role:

> **Meta reads overlays from brokers carrying the Sense role. Meta does not read overlays from brokers carrying only Innate Ability or Embodiment roles.**

Meta exists to audit *perception coherence and reasoning soundness* — "did Primary
perceive ground truth correctly and reason coherently from it?" Sense-role brokers are
exactly the perception layer Meta audits against. The action-side substrate (Innate
Ability outputs, Embodiment dispatches) Meta sees only through Primary's reasoning
trace, not by direct read. This prevents Meta from becoming a backseat actor — it
cannot independently form an opinion about what Primary "should have done" because it
lacks the action-side ground truth to do so.

The role-set rule is structurally cleaner than enumerating brokers: when a new
Sense-role broker is added (e.g., a future Memory-recall broker), Meta automatically
gains read access to its overlay; when a new broker carries only Innate Ability or
Embodiment roles, Meta automatically does not. The contract scales with the role-set
declaration in each broker manifest.

For multi-role brokers (e.g., Browser carrying `[Sense, Embodiment]`), Meta reads only
the *Sense-role sub-Overlay* — the framework's per-role sub-Overlay materialization
ensures Meta sees only the Sense portion, never the Embodiment portion.

(This is a consuming-project Meta-lobe constraint; terminal-side broker work can proceed
without the Meta lobe — when consuming projects like cereGrim wire up their Meta lobe,
this is the read scope they get from the framework.)

---

## Pipelines, not tools

A **pipeline** is the broker's output unit — a named, parameterized, currently-legal
sequence of actions the consumer can dispatch. Examples:

- Work Broker pipeline: `dispatch-work-unit { id: "S12-WU1" }`
- IDE Broker pipeline: `open-pane { pane_id: "browser-pane", policy: "allowed" }`
- Browser Broker pipeline: `read-current-page { url: <current>, mode: "dom-summary" }`

A pipeline differs from an MCP tool in four ways:

1. **Precondition-checked.** A broker only emits a pipeline whose preconditions are met
   right now. MCP advertises capability and trusts the model to check; the broker checks.
2. **State-parameterized.** Pipeline parameters are filled from the hot store, not from
   model-generated arguments. The model picks *which* pipeline, not *what arguments* —
   reducing the surface where the model can invent.
3. **Audit-anchored.** Each pipeline dispatch carries the broker's projection it was
   emitted from. Replay tooling can show "at tick N, broker offered {P1, P2, P3}; model
   picked P2; pipeline ran; result was R." Pure tool catalogs don't have this anchor.
4. **Routing-signaled.** Every Surfaced pipeline carries `description` + `when_to_use`
   fields (combined ≤1,536 chars — same routing-critical budget as NeuroGrim's skill
   manifests; per [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §1). The LLM sees the
   routing signal first; the description determines whether dispatch happens at all. MCP
   tools have descriptions but no `when_to_use` trigger phrases and no budget discipline
   — agents either over- or under-route. The broker enforces the same discipline skills
   already follow.

The **Skills & Config** cold store for each terminal broker is the **pipeline catalog**:
all pipelines this broker *could* emit before legality is checked. The broker's
projection function filters that catalog against current hot-store state to produce
`legal_pipelines(state)`.

### Cross-broker pipeline composition

A pipeline can declare a sub-step that calls another broker's surfaced pipeline (e.g.,
the Work broker's `dispatch-work-unit` includes `sensory-broker/read-awareness-summary`
as a step). Per [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) building block #27
(Cross-Broker Composition Policy), the framework enforces three contracts:

1. **Atomicity.** Cross-broker sub-pipelines must complete within a single workflow
   checkpoint of the parent broker. If the cross-broker sub-step fails, the parent
   workflow checkpoint rolls back per §"Workflow Engine" atomicity (no torn workflow
   state across the broker boundary).
2. **ACL governance.** The Topology Broker (see Canonical broker list above) mediates
   the cross-broker call per its per-consumer OverlayView; the calling broker requires
   an ACL grant for the target broker's surfaced pipeline. Calls without the grant
   refuse at dispatch + are recorded with `failure_reason: cross_broker_acl_denied`.
3. **Trust-budget double-debit.** The calling broker debits its own trust budget for
   the cross-broker call AND the called broker debits its budget for the dispatch
   it performs. Prevents free-riding on another broker's budget; ensures cluster-level
   trust accounting stays honest.

YAML shape (per [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md)):
`sub_pipeline: <broker_id>/<pipeline_id>` with optional
`composition_mode: [allowed | requires-acl | requires-trust-boost]`.

---

## Hardware seam (the placement boundary the queue makes possible)

The diagram's "queue as hardware seam" claim, made explicit:

| Tier | Cadence | Memory class | Examples |
|---|---|---|---|
| **LLM-class** | Agent-tick (whenever the LLM gets a turn) | Hot, fast, expensive | Broker, Hot Store, Internal Service |
| **Host-class** | World-tick (whenever the world produces an event) | Cold, durable, cheap | External Service, Queue, Cold Store |

The queue absorbs the rate mismatch. Without it, agent-tick and world-tick clocks couple,
and either the LLM-class side stalls on world events (wasted GPU) or the host-class side
drops events under LLM-class memory pressure (lost ground truth). The queue makes both
sides ignorant of the other's clock — they synchronize only when a message moves.

For terminal-level brokers, both tiers are already host-class, so the queue + internal
service collapse out. The terminal broker reads its own hot store directly and answers
the Workspace Manager. Queuing — when needed — is provided by the Manager, not the
terminal broker.

---

## What NeuroGrim provides vs. what consuming projects provide

- **NeuroGrim owns** (this contract + the implementation): the `Broker` trait, the
  `Pipeline` primitive, the LLM-level broker scaffolding for each role, the canonical
  broker implementations (Context, Workspace, Sensory, Topology, Work, Workspace
  Manager, IDE/Browser/Custom Sensor effectors), the Awareness Service enforcer, the
  Pipeline Runner, the Workflow Engine, the Trace Sink + Replay tooling, the Broker
  Registry, the Hot-Store + Awareness Materializers + Composer, the role-set scaffolding
  system, **the wrapping paths for existing MCP tools + sensors** (see
  [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md)) + the canonical manifest schema (see
  [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md)). Most canonical brokers
  reuse already-shipped NeuroGrim substrate verbatim (Context = `BrainContext`, Work =
  `backlog::next_ready`, Workspace = IDE state mirror).
- **Consuming projects own** (e.g., cereGrim): role-set declarations for the canonical
  brokers as deployed in their harness, cold-store schema specializations, curation
  policy declarations per-broker, leaf-op step implementations specific to their
  use-case, **wrap-existing-capabilities decisions (per BROKER-WRAPPING.md: which MCP
  tools to broker, which sensors to wrap, which skills to surface via Context Broker
  Overlay curation rather than wrapping)**, manifest registration with the framework,
  the validation harness scoring (the `harness-coherence` + `substrate-reuse` Brain
  domains the consuming project runs against its own deployment), consuming-project-
  specific design articulation (e.g. cereGrim's `thesis/` for *why* it adopted this
  primitive).

A NeuroGrim contributor reading this contract should be able to file NeuroGrim backlog
items against any of the 35 building blocks in [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md)
§3. A consuming-project contributor reading this contract should be able to declare a
broker (manifest + role-set + cold schema + catalog + leaf-ops) and have the framework
compose the role-scaffolding automatically.

---

## Open design surfaces — resolved in [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md)

This contract pins the **shape** of a broker. The companion document
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) pins the **interior** — what's *inside*
a broker that makes it a broker and not just a function returning a list. Key resolved
questions:

- **Role-set framing** — brokers declare a *set* of architectural roles (subset of
  `{Sense, InnateAbility, Embodiment}`); the framework composes role-specific
  scaffolding ("spinal cords") from each declared role. Multi-role brokers are
  first-class.
- **Pipeline is the universal primitive** — Surfaced (LLM-facing) and Internal (broker
  plumbing) pipelines share one type, distinguished by a `visibility` field. The
  *semantic-weight test* decides what's a pipeline vs. what stays as a plain Rust
  function.
- **Workflow state lives in cold store, atomically** — the Internal Service projects
  active workflow positions into the hot store at each tick. Checkpoints are
  single-transaction (SQLite tx or fsync'd JSONL append); torn checkpoints are not
  survivable and are treated as workflow loss + auditable failure.
- **Tunability is four-tiered, defaulting to operator-only** — Untunable / OperatorOnly
  / OperatorConfirmed / Autonomous. The LLM can see and propose, but cannot
  unilaterally change governance-bearing pipelines. **Reachability invariant:**
  Untunable + OperatorConfirmed pipelines have a guaranteed rank floor; Autonomous
  tuners cannot suppress them out of `legal_pipelines()` output (closes the
  kill-switch-suppression risk).
- **The Awareness Service rate-limit enforcer is `Untunable`** — the LLM cannot demote
  its own oversight. This makes the §"The Sensory Queue contract enforcer" above
  durable. The enforcer is also the role-boundary guard for the Sense cross-role write
  path (Custom Sensor → Sensory Queue → Sensory Broker cold).
- **Topology Broker self-bypass** — ACL-mutation pipelines are Tier 3 plain functions,
  not routed through the Topology Broker. Closes the ACL infinite-regress risk.
- **NeuroGrim *is* the broker framework** — the canonical brokers (see
  [Canonical brokers](#canonical-broker-list)) are its first consumers. Authoring a new
  broker is a declarative exercise (cold-store schema + role-set declaration + YAML
  catalog + leaf-op functions + manifest); everything else is inherited from the
  framework.

The internals doc lists the **35 building blocks** across three layers (Pattern
primitives / Pipeline primitives / Substrate composition), with the framework-vs-author
split. NeuroGrim-side backlog items are filed against that map.
