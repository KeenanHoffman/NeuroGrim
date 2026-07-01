---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# NeuroGrim — Broker Awareness

How agents become aware of brokers + the pipelines those brokers currently offer. **An
agent can't use what it doesn't know it has access to.** This document pins how the
broker framework rides NeuroGrim's existing awareness substrate (capability-hygiene + L1/L2
context injection + invocation ledger + skill manifest format) rather than inventing new
awareness mechanisms.

Companion to [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) (the named primitive) and
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) (the framework internals). Terminology
defined in CONTRACT §"Glossary."

---

## The principle

NeuroGrim's existing awareness substrate is the right shape; brokers extend it rather
than replace it. Three substrate layers already in production:

> **⚠ Architectural elevation (V0-RETROSPECTIVE.md §C1; post-Wave-5 finding).** When
> the broker harness is deployed against Claude Code (or any agent harness using a
> **single generic `dispatch_pipeline` MCP tool** + L1-discovery design), the
> **Materializer Composer (BB #22a) IS the primary agent-facing interface** — not
> "one of 38 building blocks." Everything that determines the agent's behavior —
> pipeline visibility, ranking, per-pipeline parameter schemas, governance
> reachability, currently-legal status — lives in `current-projection.md`. MCP is
> reduced to a wire protocol for the dispatch action; the materializer output is
> where the agent learns what's dispatchable + why + with what params. **Without a
> high-quality Materializer Composer + Awareness Materializer (BB #24) output, the
> broker pattern's value collapses** (the agent can't curate from an opaque dispatch
> tool surface). The R-O-3 closure governance-first discipline + the U1 closure
> per-pipeline param schema surfacing make this elevation operationally
> well-supported in V0; treat the Materializer Composer's design importance + its
> output discipline as commensurate with the architectural role it plays, not just
> the BB-row weight.


| Layer | NeuroGrim mechanism | Broker extension |
|---|---|---|
| **Routing signal** | Skill manifests (`description` + `when_to_use` ≤1,536 chars; per `write-skill/SKILL.md`) | Every Surfaced pipeline carries the same fields with the same budget |
| **Context injection** | L1 (~6k tokens at session-start) + L2 (`brain_query` live tool) per the 2026-04-23 dispatch experiments | Brokers project pipeline catalogs through the same L1/L2 path |
| **Invocation tracking** | PostToolUse hook → `.claude/brain/invocation-ledger.jsonl` (name + timestamp only, privacy-by-design); read by `capability-hygiene` domain | Pipeline dispatches recorded as a new `type: "pipeline"` row alongside `type: "skill"` |

Brokers do not invent new awareness paths. They speak the existing language: routing
signals at the same budget, L1/L2 injection through the same Materializer machinery,
invocation tracking through the same ledger.

---

## §1 — Pipeline routing signal

Every **Surfaced** pipeline (Tier 1, LLM-facing — see
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §1.2) carries two mandatory routing-signal
fields in its YAML catalog definition:

| Field | Purpose | Budget |
|---|---|---|
| `description` | One- to three-sentence statement of what the pipeline does + when to use it | ≤1,200 chars |
| `when_to_use` | 4-8 trigger phrases (operator-facing or LLM-facing) that signal "this is the moment to dispatch" | ≤336 chars |
| **Combined budget** | description + when_to_use | **≤1,536 chars total** (matches Claude Code's skill-index per-capability limit) |

The combined ≤1,536-char limit mirrors the routing-critical contract that NeuroGrim's
`capability-hygiene` domain enforces for skill manifests (per `write-skill/SKILL.md` —
the authoring standard). A pipeline's routing signal is what the agent *sees first*; the
description + when_to_use determine whether the LLM ever dispatches the pipeline at all.

**Why this matters:** without routing signals at the skill-grade budget, agents either
(a) see only a name and have to guess what the pipeline does, or (b) get the full
pipeline body in context and burn through their context budget. The same trade-off
NeuroGrim already solved for skills applies verbatim to pipelines.

**Catalog validation at load:** the Pipeline Catalog (building block #9 in
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3) validates `description` + `when_to_use`
presence and budget at load time. Missing fields = broker startup failure (loud, not
silent). Per-broker `capability-hygiene` scoring (see §4 below) catches degradation over
time even when the load-time check passes.

**Internal pipelines** (Tier 2 — broker plumbing) do NOT require routing signals; they
are not LLM-facing. The validator skips Tier 2 entries. Tier 3 (plain functions) is not
in the catalog at all.

---

## §2 — L1 awareness slot (the canonical injection path)

The broker framework's awareness output lands in the agent's L1 context — the
session-start static injection (~6k tokens). Per the Hot-Store Materializer (building
block #22), brokers write structured projections into a file the agent's CLAUDE.md
mechanism auto-loads.

**Canonical injection path:** `.claude/brain/broker/current-projection.md` — a single
file that contains the concatenated outputs of every materializer in declared order. Per
the Composer rule (see [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 building block
#22 / #24 with collision fix): each materializer writes to its own *segregated* file
under a manifest, and the bootstrap-layer Composer concatenates them in
operator-declared order into `current-projection.md`. Collisions impossible by
construction.

**Two materializers in scope:**

| Materializer | File | Content |
|---|---|---|
| Hot-Store Materializer (#22) | `.claude/brain/broker/segments/overlay.md` | Per-broker Overlay state (the LLM-readable hot-tier projections) — steady-state |
| Awareness Materializer (#24) | `.claude/brain/broker/segments/awareness-routing.md` | Pipeline catalog routing signals (description + when_to_use per Surfaced pipeline; alive/dead/new status per per-broker capability-hygiene) — steady-state |
| Onboarding Projection (#30) | `.claude/brain/broker/segments/onboarding-<broker_id>.md` | First-encounter awareness — surfaces broker purpose + role-set + top-N pipelines with routing signals + governance posture + skill-body cross-refs. Auto-injected on agent's first tick with this broker; replaced by steady-state segments on subsequent ticks. **Distinct from #22 / #24** — different cadence (once per broker-per-agent vs every projection cycle) and richer content (full onboarding briefing vs delta-update). |

The Materializer Composer (#22a) concatenates all three into `current-projection.md`
per the operator's declared order (typically `[onboarding-*, overlay, awareness-routing]`
so onboarding flows naturally into steady-state on the first tick).

**Cadence:** session-start by default (writes once, agent reads on auto-load). Optional
hook-triggered re-projection per PreToolUse for finer cadence (replaces file in-place
atomically; agent picks up on next read).

**L2 fallback:** a `broker_query(broker_id, query)` live tool (mirroring NeuroGrim's
`brain_query`) lets agents request fresh broker state mid-session. The 2026-04-23
dispatch experiments showed L2 synthesis lags L1 on Sonnet repo-aware tasks (-12.75pts);
treat L2 as a *fallback for state-change-sensitive moments*, not as the primary
awareness path. The framework provides both; operators pick per deployment.

**Context budget caveat:** L1's ~6k-token budget is shared across CLAUDE.md sections,
MEMORY.md entries, skill-index, current-projection.md, etc. Brokers must be polite
budget-citizens. Per-broker materializer output should be ~100-500 tokens per Overlay
(the curation policy is what makes this possible — Overlays are curated, not raw
substrate dumps).

---

## §3 — Invocation ledger extension

Pipeline dispatches are recorded to `.claude/brain/invocation-ledger.jsonl` per the same
PostToolUse hook mechanism NeuroGrim already uses for skill invocations. Same
privacy-by-design: name + timestamp only; no arguments, no responses, no transcript.

**Schema extension (v3 row format):**

```json
{
  "type": "pipeline",
  "name": "<broker_id>/<pipeline_id>",
  "timestamp": "2026-06-23T14:30:00Z",
  "audit_class": "capability"
}
```

Three `audit_class` values per the [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §2.3
self-exclusion rule:

| `audit_class` | Examples | Read by capability-hygiene? |
|---|---|---|
| `capability` | `dispatch-work-unit`, `pin-search-result`, `read-current-page` | YES |
| `governance` | `arm-kill-switch`, `propose-pipeline-deprecation`, `record-dispatch` | YES (via `governance_pipelines()` sidecar) |
| `meta-observation` | hygiene-scoring dispatches, trace-sink reads, ledger-introspection pipelines | **NO** — excluded from the feed they themselves consume |

The exclusion closes the self-referential loop: a pipeline that observes the ledger
cannot inflate its own apparent aliveness by being dispatched.

**v1 (skill rows) + v3 (pipeline rows) coexist in the same ledger file.** Readers must
filter by `type`. The `capability-hygiene` domain v-next folds both into one alive/dead/new
classification namespace (skills + pipelines compete in the same scoring space).

**Ledger remains opt-in.** Per
[`invocation-ledger.md`](invocation-ledger.md), the hook is operator-installed; brokers
work with or without it (no ledger = pipelines score "new" indefinitely with grace
period; same fallback as skills).

---

## §4 — Hygiene scope

The `capability-hygiene` domain's classifier (alive/dead/new) extends to pipelines per
the [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §2.3 scope: **per-broker per-pipeline
AND per-role**.

| Scope | What it catches |
|---|---|
| Per-pipeline | Pipelines never dispatched in N days (alive/dead/new classification per the 90-day skill window; rare pipelines per the 365-day extended window) |
| Per-broker | A broker whose pipelines are collectively dead — operator candidate for retirement |
| Per-role | A `broker-role` (Sense / InnateAbility / Embodiment) with zero registered brokers for N days — the role's spinal-cord scaffolding is dead and can be retired (closes "framework carries unused role config indefinitely") |

**Routing-signal quality** is scored alongside aliveness. The same routing-critical
standards `capability-hygiene` applies to skills (description + when_to_use quality,
≤1,536 char budget, "when to use" signal presence) apply to pipelines. A pipeline with a
weak description scores poorly on hygiene even when its dispatch count is high.

**Brain UI surfacing** (when wired): `neurogrim agent` and `neurogrim health` surface
pipeline hygiene findings alongside skill findings. The agent's onboarding entry points
(per `AGENT-PRIMER.md`) get the same brokered-capability awareness as they currently get
for skills.

---

## Open follow-ons

- **L1/L2 routing decision tree for pipelines.** The 2026-04-23 experiments validated
  agent self-routing for skills via L2 tools (~100% on repo-aware, 0% on trivial). The
  same experiment shape for `broker_query` is queued — once S0-T's reference broker
  lands and L1 injection is observable, the L1-vs-L2 trade-off can be measured for
  pipelines specifically.
- **Per-broker Skill Filter weight cells exposed to capability-hygiene.** The Skill
  Filter (building block #20) has per-broker weight cells driving its rank ordering;
  hygiene could read these to score "is this weight policy consistent with observed
  usage" — a meta-signal beyond alive/dead/new. Future scoring extension; not v1.
- **Cluster-pipeline awareness via the IAB.** Cluster-pipelines (IAB-layer constructs)
  have the same routing-signal + invocation-ledger + hygiene requirements. The IAB stub
  (cereGrim/docs/INTER-AGENT-BROKER.md) is the design surface; this doc's mechanisms
  apply uniformly when cluster-pipelines land.
