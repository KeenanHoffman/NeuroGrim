# NeuroGrim — Agent Guide

> *a book of spells for AI agents*

NeuroGrim is the reference implementation of the LSP Brains specification: a Rust-based
Brain engine that gives AI agents continuous project health awareness through MCP-based sensory
tools, cross-domain correlation, trajectory intelligence, and gated governance.

**LSP Brains Specification:** https://github.com/KeenanHoffman/LSP-Brains

## Repository Structure

| Directory | Contents |
|-----------|----------|
| `neurogrim/` | Rust Brain engine (workspace with core, sensory, mcp, a2a [Stage 6], cli crates) |
| `NeuroGrim-python-starter/` | **Child submodule** — adoption template for Python projects. Declared as NeuroGrim's A2A child (port 8423). |
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
| When to use A2A, invoking a peer Brain, reading Agent Cards | `a2a/SKILL.md` |
| Running NeuroGrim as an A2A peer (serve, discover, troubleshoot) | `peer-brain/SKILL.md` |

### Brain Domains

Skills corresponding to Rust Brain sensor domains:

| Task | Skill |
|------|-------|
| Cross-domain correlation health and coherence scoring | `coherence/SKILL.md` |
| Human communication model: preferences, per-hat overrides | `human-comms/SKILL.md` |
| Safe secret reference catalog: providers, manifest, CMDB | `secret-refs/SKILL.md` |
| Security posture: SOC2 / ISO27001 / NIST CSF evidence scanning | `security-standards/SKILL.md` |

### Planning & Workflow

| Task | Skill |
|------|-------|
| Adversarial plan review before implementation | `plan-critic/SKILL.md` |
| Pre-plan ideation: explore approaches conversationally | `imagination-mode/SKILL.md` |
| North star alignment check | `north-star/SKILL.md` |
| Rubber-duck a stuck problem with a Socratic listener | `rubber-duck/SKILL.md` |

### Meta (Skills System)

| Task | Skill |
|------|-------|
| Authoring guide for new skills | `write-skill/SKILL.md` |
| Agent hat system (adversary, architect, etc.) | `hats/SKILL.md` |
| Pilot↔subagent interface protocol | `pilot-protocol/SKILL.md` |
| Coordinate subagents / parallel workflows | `subagent-patterns/SKILL.md` |
| Dual-agent T+P review protocol | `dual-review/SKILL.md` |
| Iterative T+P+Code Reviewer quality loop | `review-loop/SKILL.md` |
| Process for retiring outdated skills | `skill-deprecation/SKILL.md` |
| Bypass MCP: invoke the Brain via Bash subcommands | `cli-mode/SKILL.md` |

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

## Command Reference

Ten grimoire-themed aliases are available for most CLI commands
(`scry`, `divine`, `drift`, `seal`, `summon`, `cast`, `conjure`,
`commune`, `beacon`, `behold`). Primary names remain canonical. See
`README.md` § Command aliases for the full table, or run
`neurogrim --help` to verify the live list.

## Invocation Ledger (Axis 4 v1 — 2026-04-22)

Every `Skill` tool invocation can be recorded to a per-Brain ledger
at `.claude/brain/invocation-ledger.jsonl` via a PostToolUse hook.
The `capability-hygiene` domain reads this ledger to classify each
skill as **alive**, **dead**, or **new** — closing the empirical
side of the three-axis self-observability loop (hygiene · invocation
· future miss-rate).

**Setup:** add the PostToolUse hook to `.claude/settings.local.json`
pointing at `scripts/record-skill-invocation.sh`. Operator setup
guide + privacy stance + troubleshooting in
[`docs/invocation-ledger.md`](docs/invocation-ledger.md).

**Privacy-by-design:** the ledger captures name + timestamp only.
No arguments, no tool responses, no transcript content.

**Opt-in posture:** ledger is gitignored; the `capability-hygiene`
domain works with or without it (no ledger = every skill scored as
"new", grace-period applies to everyone).

## Tool Invocation Mode (MCP vs CLI)

NeuroGrim exposes its seven BrainServer scoring tools two ways —
as an MCP server (`neurogrim serve`, the default) or as direct
CLI subcommands (`neurogrim score`, `neurogrim trend`, etc.).

| Mode | Context cost at session start | When to use |
|------|-------------------------------|-------------|
| **MCP (default)** | ~983 tokens (7 tool schemas injected) | Newcomers; sessions where discovery/typed errors matter. |
| **CLI (opt-in)** | 0 tokens | Power users who know the CLI surface; long sessions under context pressure. |

Opt into CLI mode by **omitting** the NeuroGrim MCP server from
your `.claude/.mcp.json`. Then load the `cli-mode` skill so the
agent reaches for Bash instead of MCP tools. Full doc + config
examples: `docs/cli-mode.md`. MCP↔CLI surface mapping:
`docs/cli-sensory-surface.md`. Benchmark + methodology:
`roadmap/data/b09-bench-<date>.json`.

## Child Brain

NeuroGrim is itself a parent Brain in the A2A fractal-composition
sense: the `NeuroGrim-python-starter/` submodule is declared as its
child in `.claude/brain-registry.json` (port 8423). This gives
NeuroGrim a peer relationship with an adoption-template Brain and
exercises the multi-hop A2A pattern (ecosystem → NeuroGrim →
python-starter).

## Agent Philosophy

When wearing a hat, announce it visibly: `Wear Hat: <hat-name>`.

Every task in this repo has a documented skill. Read the relevant skill before acting.

The LSP Brains specification lives in its own repo (https://github.com/KeenanHoffman/LSP-Brains).
This repo implements the spec in Rust.

The `domains/laas/` archive is read-only reference material. Do not modify it to match
spec changes — it preserves the state at the time of separation.
