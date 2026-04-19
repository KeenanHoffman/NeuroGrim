# NeuroGrim — Agent Guide

NeuroGrim is the reference implementation of the LSP Brains specification: a Rust-based
Brain engine that gives AI agents continuous project health awareness through MCP-based sensory
tools, cross-domain correlation, trajectory intelligence, and gated governance.

**LSP Brains Specification:** https://github.com/KeenanHoffman/LSP-Brains

## Repository Structure

| Directory | Contents |
|-----------|----------|
| `neurogrim/` | Rust Brain engine (workspace with core, sensory, mcp, a2a [Stage 6], cli crates) |
| `starter-kit/` | **Archived 2026-04-17** — moved to `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` |
| `domains/laas/` | Archived first-customer domain: LaaS (16 domains, 26 gates, 3 hats) |
| `whitepaper/` | LSP Brains methodology whitepaper (Markdown; HTML build archived) |
| `roadmap/` | Vision, roadmap, data architecture, dependencies, stage epics (S1–S6) |
| `.claude/skills/` | Methodology and brain operation skills |

## Run Tests

```bash
# Rust Brain engine tests (primary test suite)
cd neurogrim
cargo test --workspace --all-targets

# Python SDK tests
cd ../sdk-python
py -3 -m pytest tests/
```

## Skills Index

The skills below are the **live** inventory for the Rust Brain. Legacy
starter-kit skills (LaaS / PowerShell era) are preserved at
`.claude/skills/archived/` for historical reference — see that directory's
`README.md` for provenance and closest live equivalents. Do not follow the
archived skills; their commands and tools no longer exist.

### Peer Protocols (A2A)

| Task | Skill |
|------|-------|
| When to use A2A, invoking a peer Brain, reading Agent Cards | `a2a.md` |
| Running NeuroGrim as an A2A peer (serve, discover, troubleshoot) | `peer-brain.md` |

### Brain Domains

Skills corresponding to Rust Brain sensor domains:

| Task | Skill |
|------|-------|
| Cross-domain correlation health and coherence scoring | `coherence.md` |
| Human communication model: preferences, per-hat overrides | `human-comms.md` |
| Safe secret reference catalog: providers, manifest, CMDB | `secret-refs.md` |
| Security posture: SOC2 / ISO27001 / NIST CSF evidence scanning | `security-standards.md` |

### Planning & Workflow

| Task | Skill |
|------|-------|
| Adversarial plan review before implementation | `plan-critic.md` |
| Pre-plan ideation: explore approaches conversationally | `imagination-mode.md` |
| North star alignment check | `north-star.md` |
| Rubber-duck a stuck problem with a Socratic listener | `rubber-duck.md` |

### Meta (Skills System)

| Task | Skill |
|------|-------|
| Authoring guide for new skills | `write-skill.md` |
| Agent persona system (adversary, architect, etc.) | `personas.md` |
| Pilot↔subagent interface protocol | `pilot-protocol.md` |
| Coordinate subagents / parallel workflows | `subagent-patterns.md` |
| Dual-agent T+P review protocol | `dual-review.md` |
| Iterative T+P+Code Reviewer quality loop | `review-loop.md` |
| Process for retiring outdated skills | `skill-deprecation.md` |

## Key Files

| File | Purpose |
|------|---------|
| `neurogrim/Cargo.toml` | Rust workspace root |
| `neurogrim/crates/neurogrim-core/` | Pure scoring logic (zero I/O) |
| `neurogrim/crates/neurogrim-sensory/` | Built-in sensory tool implementations |
| `neurogrim/crates/neurogrim-mcp/` | MCP client + server integration (sensory + LLM-facing) |
| `neurogrim/crates/neurogrim-a2a/` | **Stage 6:** A2A peer protocol — Agent Card, envelope, task client/server |
| `neurogrim/crates/neurogrim-cli/` | CLI binary entry point |
| `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` | Archived legacy PowerShell starter (read-only) |
| `domains/laas/brain-registry.json` | Full LaaS registry (16 domains, real-world reference) |
| `roadmap/VISION.md` | 16 guiding principles (v2.1+: #16 "right protocol for the role") |
| `roadmap/ROADMAP.md` | Stage progression (Stages 1–4 done; S5 in progress; S6 "Dual Brain via A2A") |

## Agent Philosophy

When wearing a hat, announce it visibly: `Wear Hat: <hat-name>`.

Every task in this repo has a documented skill. Read the relevant skill before acting.

The LSP Brains specification lives in its own repo (https://github.com/KeenanHoffman/LSP-Brains).
This repo implements the spec in Rust.

The `domains/laas/` archive is read-only reference material. Do not modify it to match
spec changes — it preserves the state at the time of separation.
