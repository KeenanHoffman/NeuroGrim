---
doc-version: 5.0.0
date: 2026-06-30
status: current
anchored-to: neurogrim
front-door: true
---

# NeuroGrim — Scaffolding (reading-path map)

The front door to NeuroGrim's documentation. Read this first: it groups the
repo's docs into reading paths so a newcomer (human or agent) can follow a
logical route from "what is this" → "how do I use it" → "how does it work
inside." Each entry is a link + a one-line purpose + a Status
(**Stable** / **Draft** / **Reference**).

> Not every internal/reference doc is listed — this map pulls the genuinely
> useful narrative docs into reach. For the exhaustive live doc graph, run
> `neurogrim docs map` and `neurogrim sensory documentation-graph`.

## Start here (front doors)

| Path | Purpose | Status |
|---|---|---|
| [`../CLAUDE.md`](../CLAUDE.md) | Agent guide — the entry point for any agent entering this repo (orientation commands, structure, skills, broker framework, key files). | Stable |
| [`../README.md`](../README.md) | What NeuroGrim is in product language + install + command reference/aliases. | Stable |
| [`../PITCH.md`](../PITCH.md) | Elevator pitch — the one-paragraph "why this exists." | Stable |
| [`AGENT-PRIMER.md`](AGENT-PRIMER.md) | Thin index to the 8 bundled `neurogrim explain <topic>` methodology docs; the fastest comprehension path. | Stable |
| [`getting-started.md`](getting-started.md) | First-run walkthrough: install, initialize a Brain, score, read health. | Stable |

## Broker framework

The broker-pattern substrate (structured store + deterministic dispatcher)
that consuming harnesses like cereGrim build on. Read **CONTRACT** then
**INTERNALS** first; the rest are authoring/operating detail.

| Path | Purpose | Status |
|---|---|---|
| [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) | The named-primitive contract: 6-piece LLM pattern, 3-piece terminal pattern, role-set composition, canonical brokers, Workspace Manager, Topology Broker, Sensory-Queue enforcer. | Draft |
| [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) | Framework framing + 23 building blocks across three layers + Pipeline primitive + Workflow Engine + four-tier tunability. | Draft |
| [`BROKER-AUTHORING.md`](BROKER-AUTHORING.md) | How to author a broker against the contract. | Stable |
| [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) | How brokers surface awareness into the harness. | Stable |
| [`BROKER-FRAMES.md`](BROKER-FRAMES.md) | Broker frame model — the framing units brokers operate over. | Stable |
| [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) | Wrapping existing services/APIs as brokers. | Stable |
| [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md) | Schema for broker manifests. | Reference |
| [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md) | Schema for broker-cluster manifests. | Reference |
| [`BROKER-OPERATOR-GUIDE.md`](BROKER-OPERATOR-GUIDE.md) | Operator onboarding for running the broker framework. | Stable |
| [`BROKER-HARNESS-DEMO.md`](BROKER-HARNESS-DEMO.md) | Operator demo procedure for the S*-T harness MVP. | Stable |
| [`BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md`](BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md) | Pre-execution gate model for broker scaffolding. | Stable |
| [`BROKER-SPEC-GAPS.md`](BROKER-SPEC-GAPS.md) | Discovered-gap ledger for the broker spec. | Reference |
| [`PUBLIC-VS-PROPRIETARY.md`](PUBLIC-VS-PROPRIETARY.md) | IP-boundary policy — which broker surface is public vs. consuming-project proprietary. | Stable |
| [`diagrams/DIAGRAM-V4-SPEC.md`](diagrams/DIAGRAM-V4-SPEC.md) | Spec for the v4 broker-pattern Mermaid diagram (visual reference). | Reference |

## Roadmap, vision & planning

| Path | Purpose | Status |
|---|---|---|
| [`../roadmap/VISION.md`](../roadmap/VISION.md) | North-star principles guiding the project. | Stable |
| [`../roadmap/ROADMAP.md`](../roadmap/ROADMAP.md) | Stage progression and live stage status. | Stable |
| [`../roadmap/BACKLOG.md`](../roadmap/BACKLOG.md) | Backlog of candidate work items. | Reference |
| [`../roadmap/broker-framework-backlog.md`](../roadmap/broker-framework-backlog.md) | The 38-building-block broker-framework backlog. | Reference |
| [`../roadmap/doc-broker-and-doc-v5-upgrade.md`](../roadmap/doc-broker-and-doc-v5-upgrade.md) | Plan for the documentation broker + the Doc v5.0 version upgrade it powers (this map is part of it). | Draft |
| [`../roadmap/v5-roadmap.md`](../roadmap/v5-roadmap.md) | v5 "Everything is Lego" roadmap. | Reference |
| [`../roadmap/LSP-BODY.md`](../roadmap/LSP-BODY.md) | The full-body view of the LSP Brains system. | Reference |
| [`../roadmap/DATA-ARCHITECTURE.md`](../roadmap/DATA-ARCHITECTURE.md) | Data architecture (bus / TSDB / stores). | Reference |
| [`../roadmap/DEPENDENCIES.md`](../roadmap/DEPENDENCIES.md) | Cross-stage dependency graph. | Reference |

## Guides (how to use it)

| Path | Purpose | Status |
|---|---|---|
| [`sdk.md`](sdk.md) | The NeuroGrim SDK — extending the Brain with custom sensors/sources. | Stable |
| [`cli-mode.md`](cli-mode.md) | CLI mode — invoking the Brain via Bash subcommands instead of MCP (0-token startup). | Stable |
| [`cli-sensory-surface.md`](cli-sensory-surface.md) | MCP tool ↔ CLI subcommand mapping. | Reference |
| [`v5-composition-guide.md`](v5-composition-guide.md) | v5 composition guide — assembling modular pieces. | Stable |
| [`pilot-protocol-guide.md`](pilot-protocol-guide.md) | Full guide to the pilot↔subagent interface protocol. | Stable |
| [`plan-critic-guide.md`](plan-critic-guide.md) | Full guide to adversarial plan review. | Stable |
| [`subagent-patterns-guide.md`](subagent-patterns-guide.md) | Full guide to subagent coordination patterns. | Stable |
| [`write-skill-guide.md`](write-skill-guide.md) | Full guide to authoring skills. | Stable |

## Domains, sensors & operations (reference)

| Path | Purpose | Status |
|---|---|---|
| [`DOMAINS.md`](DOMAINS.md) | The domain catalog — what each Brain domain measures. | Reference |
| [`invocation-ledger.md`](invocation-ledger.md) | Setup + privacy stance for the skill-invocation ledger (capability-hygiene). | Reference |
| [`domain-promotion-audit.md`](domain-promotion-audit.md) | Runbook for promoting a domain past advisory weight. | Reference |
| [`judge-calibration-profiles.md`](judge-calibration-profiles.md) | Judge calibration profiles for agent-behavior scoring. | Reference |
| [`agent-behavior-troubleshooting.md`](agent-behavior-troubleshooting.md) | Troubleshooting playbook for agent-behavior verification. | Reference |
| [`agent-behavior-red-taxonomy.md`](agent-behavior-red-taxonomy.md) | Red-sample failure-mode taxonomy. | Reference |
| [`supply-chain-sca.md`](supply-chain-sca.md) | Layer 1 — native-Rust software composition analysis. | Reference |
| [`supply-chain-vigilance.md`](supply-chain-vigilance.md) | Layer 2 — deep-signal supply-chain sensors. | Reference |
| [`supply-chain-review.md`](supply-chain-review.md) | Layer 3 — agent-assisted human review. | Reference |
| [`supply-chain-calibration.md`](supply-chain-calibration.md) | Three-layer supply-chain calibration harness. | Reference |
| [`test-slo.md`](test-slo.md) | Test-suite SLO model. | Reference |
| [`container-brain.md`](container-brain.md) | Running NeuroGrim in containers (optional). | Reference |
| [`EXTERNAL-BRAIN-DEPLOYMENT.md`](EXTERNAL-BRAIN-DEPLOYMENT.md) | External-Brain reference deployment (S6-DB-5). | Reference |
| [`brain-capability-audit-2026-04-23.md`](brain-capability-audit-2026-04-23.md) | Deeper evidence reading of which Brain capabilities have been empirically tested. | Reference |

## Whitepaper & spec

| Path | Purpose | Status |
|---|---|---|
| [`../whitepaper/WHITEPAPER.md`](../whitepaper/WHITEPAPER.md) | "A Nervous System for AI Agents" — the LSP Brains methodology whitepaper. | Reference |
| [`../spec/README.md`](../spec/README.md) | Pointer into the LSP Brains specification (canonical spec lives in the LSP-Brains repo). | Reference |

## Ecosystem context

NeuroGrim is one Brain in a five-Brain ecosystem. For the ecosystem-level
reading path (project front doors + per-project maps), see the ecosystem
scaffolding at [`../../SCAFFOLDING.md`](../../SCAFFOLDING.md).
