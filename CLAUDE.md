# Moth(er):Br+AI+n — Agent Guide

Moth(er):Br+AI+n is the reference implementation of the LSP Brains specification: a Rust-based
Brain engine that gives AI agents continuous project health awareness through MCP-based sensory
tools, cross-domain correlation, trajectory intelligence, and gated governance.

**LSP Brains Specification:** https://github.com/keenanHoffmanSparq/LSP-Brains

## Repository Structure

| Directory | Contents |
|-----------|----------|
| `motherbrain/` | Rust Brain engine (workspace with core, sensory, mcp, a2a [Stage 6], cli crates) |
| `starter-kit/` | **Archived 2026-04-17** — moved to `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` |
| `domains/laas/` | Archived first-customer domain: LaaS (16 domains, 26 gates, 3 hats) |
| `whitepaper/` | LSP Brains methodology whitepaper (Markdown; HTML build archived) |
| `roadmap/` | Vision, roadmap, data architecture, dependencies, stage epics (S1–S6) |
| `.claude/skills/` | Methodology and brain operation skills |

## Run Tests

```bash
# Rust Brain engine tests (primary test suite)
cd motherbrain
cargo test --workspace --all-targets

# Python SDK tests
cd ../sdk-python
py -3 -m pytest tests/
```

## Skills Index

### Peer Protocols (A2A — Stage 6)

| Task | Skill |
|------|-------|
| When to use A2A, invoking a peer Brain, reading Agent Cards | `a2a.md` |
| Running Moth(er):Br+AI+n as an A2A peer (serve, discover, troubleshoot) | `peer-brain.md` |

### Brain Operations

| Task | Skill |
|------|-------|
| Unified health score, cross-domain correlation, tool registry | `brain.md` |
| Hat system: domain emphasis, hat suggestion, hat-tagged outcomes | `hats.md` |
| Agent persona system (roles for operator agent) | `personas.md` |
| Inspect and clear gate state | `gate-status.md` |
| Gate system architecture and state machine | `gate-system-overview.md` |
| Query historical operational data | `operational-memory.md` |
| Terminology governance: drift scan, term lookup | `terminology-governance.md` |
| Cross-domain correlation health and coherence scoring | `coherence.md` |
| Human communication model: preferences, per-hat overrides | `human-comms.md` |
| Safe secret reference catalog: providers, manifest, CMDB | `secret-refs.md` |
| Security posture: SECURITY.md, SAST, secret scanning | `security-standards.md` |
| Multi-agent task protocol and subagent health | `agent-protocol.md` |

### Planning & Workflow

| Task | Skill |
|------|-------|
| Prioritized action list before commit/deploy | `what-next.md` |
| Regain focus during long or drifting sessions | `refocus.md` |
| Adversarial plan review before implementation | `plan-critic.md` |
| Common multi-skill sequences for complex tasks | `skill-chain.md` |
| North star alignment check | `north-star.md` |
| Pre-plan ideation: explore approaches conversationally | `imagination-mode.md` |

### LSP & Static Analysis

| Task | Skill |
|------|-------|
| Symbol search / LSP static analysis (PS, TF, TS, PY, SH) | `lsp.md` |
| LSP-grounded workflow patterns | `lsp-grounded.md` |
| Parallel multi-domain LSP queries via subagents | `lsp-subagent-queries.md` |
| Add LSP support for a new language | `add-lsp-for-language.md` |

### DevOps Concepts & Philosophy

| Task | Skill |
|------|-------|
| DevOps concepts explained (teaching) | `devops-for-developers.md` |
| DevOps principles / the "why" | `devops-philosophy.md` |
| Principle-to-skill cross-reference | `philosophy-index.md` |
| Everything as Code principles and the five EaC pillars | `everything-as-code.md` |

### Meta (Skills System)

| Task | Skill |
|------|-------|
| Full skill discovery map by category | `skill-index.md` |
| Authoring guide: template and quality checklist | `write-skill.md` |
| Skill+hook pair catalog | `skill-hook-pairs.md` |
| Reference for all registered hooks | `hooks-reference.md` |
| Coordinate subagents / parallel workflows | `subagent-patterns.md` |
| Dual-agent T+P review protocol | `dual-review.md` |
| Iterative T+P+Code Reviewer quality loop | `review-loop.md` |
| Worked examples of agent-skill interactions | `demo.md` |
| Compact summary of all skills | `summarize-skills.md` |
| Living record of missing and stale skills | `skill-gap-tracker.md` |
| Process for retiring outdated skills | `skill-deprecation.md` |

## Key Files

| File | Purpose |
|------|---------|
| `motherbrain/Cargo.toml` | Rust workspace root |
| `motherbrain/crates/motherbrain-core/` | Pure scoring logic (zero I/O) |
| `motherbrain/crates/motherbrain-sensory/` | Built-in sensory tool implementations |
| `motherbrain/crates/motherbrain-mcp/` | MCP client + server integration (sensory + LLM-facing) |
| `motherbrain/crates/motherbrain-a2a/` | **Stage 6:** A2A peer protocol — Agent Card, envelope, task client/server |
| `motherbrain/crates/motherbrain-cli/` | CLI binary entry point |
| `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` | Archived legacy PowerShell starter (read-only) |
| `domains/laas/brain-registry.json` | Full LaaS registry (16 domains, real-world reference) |
| `roadmap/VISION.md` | 16 guiding principles (v2.1+: #16 "right protocol for the role") |
| `roadmap/ROADMAP.md` | Stage progression (Stages 1–4 done; S5 in progress; S6 "Dual Brain via A2A") |

## Agent Philosophy

When wearing a hat, announce it visibly: `Wear Hat: <hat-name>`.

Every task in this repo has a documented skill. Read the relevant skill before acting.

The LSP Brains specification lives in its own repo (https://github.com/keenanHoffmanSparq/LSP-Brains).
This repo implements the spec in Rust.

The `domains/laas/` archive is read-only reference material. Do not modify it to match
spec changes — it preserves the state at the time of separation.
