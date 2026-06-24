# Broker Framework — Operator Onboarding Guide

> **Status: tiered onboarding (R-X-10 closure, Phase 9).** The framework has 38
> building blocks + manifests + Frame system + tunability tiers + Overlay tiers +
> federation primitives. Full competence is multi-week. This guide tiers the
> onboarding so an operator can deploy + run + tune progressively, without needing
> to internalize all 38 BBs upfront.

**Audience:** new operators deploying or maintaining a broker-framework-equipped
agent harness (e.g., cereGrim or future consumers). Assumes basic familiarity with
LLM agent harnesses + YAML/TOML config.

---

## Tier 1 — First hour: deploy + dry-run

**Goal:** the framework is running locally; brokers are registered; the agent can
read its L1 context (current-projection.md).

**Minimum-viable knowledge:**
- What a **broker** is (deterministic substrate component projecting legal pipelines;
  see [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"What a broker IS").
- What a **pipeline** is (named, parameterized, currently-legal action; YAML in the
  catalog).
- What the **cluster manifest** is (top-level TOML declaring deployment-level
  config; see [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md)).

**Tasks:**
1. Copy the reference cluster manifest from `examples/cluster-manifest.toml`.
2. Edit the `cluster.id` + `cluster.name` for your deployment.
3. Run `neurogrim cluster start` (or equivalent for the consuming project).
4. Confirm `.claude/brain/broker/current-projection.md` is being written.
5. Confirm `neurogrim cluster status` reports all brokers as healthy.

**Skip for now:** Frame system, tunability tiers, governance composition,
federation, trust budgets, deprecation, quarantine, peer-dialogue. These are
post-Tier-1 concerns.

**Done signal:** you can read `current-projection.md` and see broker Overlays
projected. The framework is working at the surface level even if you don't yet
understand the interior.

---

## Tier 2 — First day: read-only troubleshooting

**Goal:** when something goes wrong, you can read the audit trail + telemetry to
understand what happened without changing anything.

**Knowledge added:**
- **Trace Sink** (BB #12): every dispatch leaves a trace; replay tooling (BB #13)
  surfaces historical state.
- **Operator Telemetry Summarizer** (BB #32): human-readable broker status summary
  at `.claude/brain/broker-telemetry-summary.md`.
- **Diagnostics Collector** (BB #28): per-pipeline latency + dispatch counts +
  governance-block frequency.
- **Action-ledger** (BB #36): per-agent action history with outcomes.

**Tasks:**
1. Read `broker-telemetry-summary.md` to see broker health snapshot.
2. Inspect the trace ledger for a specific pipeline that misbehaved:
   `neurogrim trace inspect --pipeline <pipeline_id> --tick <N>`.
3. Read the agent-behavior segment in `current-projection.md` to see per-agent
   action patterns.
4. Identify governance-block patterns (which pipelines were refused by `check-trust-budget`
   or `check-kill-switch`).

**Skip for now:** changing any configuration. This tier is observational.

**Done signal:** when an incident happens, you know which docs/tools to read to
diagnose it. You may not yet know how to *fix* — that's Tier 3.

---

## Tier 3 — First week: basic tuning

**Goal:** you can tune Autonomous-tunable cells (Skill Filter weights, Frame
defaults, trust-budget ceilings) within operator-declared bounds without breaking
governance invariants.

**Knowledge added:**
- **Tunability tiers** (Untunable / OperatorOnly / OperatorConfirmed / Autonomous).
  Default for any new cell is OperatorOnly. See [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md)
  §4.
- **Proposal Ledger** (BB #21): how LLM proposes changes; how operator approves.
- **Frame system basics** (just Stakes + Hat + Tempo to start; defer the full 7
  types). See [`BROKER-FRAMES.md`](BROKER-FRAMES.md) §2.
- **Cluster manifest tunable fields**: the M-13 per-parameter table in
  [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md) shows what's tunable
  at what scope.

**Tasks:**
1. Find a pending proposal in `proposal-ledger.json`; review + decide accept/reject;
   record rationale.
2. Adjust a trust-budget ceiling in the cluster manifest (e.g., raise Sensory
   Broker's budget if Diagnostics shows budget exhaustion).
3. Set per-broker Frame defaults (e.g., `cluster.brokers.work-broker.frame_defaults.stakes
   = "production"`).
4. Reload the catalog: `neurogrim cluster reload-catalog`; verify hot-reload
   succeeded (no cycle introduction; no budget overrun).

**Skip for now:** governance pipeline composition, Workflow Engine internals, IAB
federation, Cluster Federation Topology, peer-dialogue. These are Tier 4+ topics.

**Done signal:** you can make safe tuning changes + verify they took effect. Bad
tunings get rejected at manifest-load (field-level tunability validation per R-S-18)
so you can't accidentally break governance.

---

## Tier 4 — First month: advanced operations

**Goal:** you can manage broker lifecycle (deploy new brokers, deprecate old ones,
quarantine sensors, handle schema migrations) + understand the full Frame system +
diagnose cross-broker composition issues.

**Knowledge added:**
- **Broker Lifecycle** (BB #29): graceful shutdown, hot-swap, version transitions.
- **Pipeline Deprecation Manager** (BB #37): operator retirement ceremony.
- **Sensor Quarantine Manager** (BB #38): isolate + inspect + restore misbehaving
  sensors.
- **Schema Migration Runner** (BB #26): forward-migrations, idempotency classes,
  workflow resumption ordering.
- **Cross-Broker Composition Policy** (BB #27): atomicity, ACL, double-debit,
  cycle detection. Two-phase commit ordering (R-O-1 closure).
- **Full Frame system**: all 7 types (Hat/Stakes/Tempo/Mode/Confidence/Audience/Scope),
  inheritance, conflict precedence, broker-prescribed Frames.
- **Cognition channel speaker pinning** (R-S-8): for consumers using peer-dialogue.

**Tasks:**
1. Deploy a new broker against the framework (use the half-day walkthrough in
   [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §6).
2. Run a schema migration on an existing broker; verify workflow resumption works.
3. Quarantine a misbehaving sensor + restore it via `restore-quarantined-source`.
4. Adjust the Frame conflict-precedence matrix in cluster manifest (respecting the
   Stakes-with-governance hard floor per R-S-17).

**Skip for now:** Cluster Federation Topology (BB #31) and Inter-Agent Broker
unless deploying multi-machine. These are Tier 5 topics for federation operators.

**Done signal:** you can run a production broker deployment + handle incidents
without escalating to framework authors. You understand both the WHAT (the 38 BBs)
and the HOW (which combinations apply when).

---

## Tier 5 — Federation operations (multi-machine deployments)

**Goal:** you can operate cluster-federation deployments (multiple machines, multiple
clusters, inter-agent dispatch).

**Knowledge added:**
- **Cluster Federation Topology** (BB #31): cross-cluster discovery, transitive ACL,
  version cascade.
- **Inter-Agent Broker** (cereGrim/INTER-AGENT-BROKER.md): peer-agent dispatch,
  bootstrap modes (federated-mesh / role-led / arbiter-service / static).
- **Agent Card schema** (canonical in BB #31; R-O-6 closure): role attestation,
  staleness check.
- **A2A protocol bumps for cross-machine pipeline dispatch**.

**Tasks:**
1. Configure cluster manifest's `cluster.federation` section for multi-cluster.
2. Verify inter-cluster ACL grants work transitively (Cluster-A → Cluster-B →
   Cluster-C).
3. Handle a cross-cluster contract-version mismatch (M-3 negotiation surface).

**Done signal:** multiple clusters federate cleanly; cross-cluster dispatch works;
operator can debug federation issues.

---

## Where to go from here

| Need to ... | Read ... |
|---|---|
| Understand the broker pattern's intent | [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) |
| Deep-dive into a specific BB | [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 + the relevant BB row |
| Author a new broker | [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §6 + the [implementation backlog](../roadmap/broker-framework-backlog.md) |
| Configure cluster manifest | [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md) |
| Configure per-broker manifest | [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md) |
| Understand the Frame system | [`BROKER-FRAMES.md`](BROKER-FRAMES.md) |
| Wrap an MCP tool / sensor / skill as a broker | [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) |
| Set up A2A federation | BB #31 in BROKER-INTERNALS.md + cereGrim's INTER-AGENT-BROKER.md |

When you hit a spec gap (something the docs don't answer), log it in
[`BROKER-SPEC-GAPS.md`](BROKER-SPEC-GAPS.md) — the gap ledger is how the spec
becomes battle-stable over time.

---

## Anti-patterns to avoid

- **Don't tune Untunable parameters.** The cluster-manifest loader rejects them per
  the field-level tunability annotations (R-S-18). If your manifest is being
  rejected, look up the field in the [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md)
  tunability table.
- **Don't reorder Frame conflict precedence to suppress governance Stakes.**
  Stakes-with-governance values are floor-protected per R-S-17; manifest will be
  rejected.
- **Don't share cold-store files across brokers.** Per-broker file isolation is
  the framework default for a reason (R-O-4 lock contention).
- **Don't run `--write-golden` without a rationale.** Replay write-mode requires a
  free-text rationale per R-S-7; framework refuses without it.
- **Don't deprecate dead-flagged pipelines without checking if they're
  safety-critical.** Some safety pipelines (e.g., `arm-kill-switch`) are rare-but-needed;
  whitelist them as never-dead per R-X-7 / M-1.

---

## Help signals

- **`harness-coherence` Brain domain score dropping**: configuration is drifting.
  Run the operator-UX dashboard (B-59 once landed) to see contradictory tunings.
- **Workflow timeouts on cold-store writes**: SQLite contention; consider switching
  to JSONL backend per R-O-4 / BB #6 guidance.
- **Recursive escalation chains > 3 per turn**: cascade likely; framework refuses
  the turn per R-X-15 + dumps the chain to telemetry. Inspect the chain to find
  the pathological config.
- **`schema_migration_blocked` events**: workflow checkpoint is too old to migrate;
  run the `propose-workflow-resnapshot` ceremony per R-O-2.
