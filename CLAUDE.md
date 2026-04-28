# NeuroGrim — Agent Guide

> *a book of spells for AI agents*

NeuroGrim is the reference implementation of the LSP Brains specification: a Rust-based
Brain engine that gives AI agents continuous project health awareness through MCP-based sensory
tools, cross-domain correlation, trajectory intelligence, and gated governance.

LSP Brains itself is **a declared overlay of project-shaped commitments on a
general-purpose statistical engine** — the LLM provides cognition; the Brain
provides what to be cognizant of. NeuroGrim is the engine that runs that overlay.

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

## Brain Access Patterns — Tentative Findings (2026-04-23)

> **CAVEAT (2026-04-23 held-out update):** A dispatch rule was
> proposed ("inject L1 only when the answer requires project-specific
> facts; otherwise L0"). On the original 12-task benchmark it
> captured ~89% of the oracle ceiling. **It did NOT generalize to a
> 22-task held-out set** — direct agreement with oracle dropped to
> 40.9%, within-5-pts to 68.2%, both below the pre-registered kill
> thresholds. The rule is therefore *indicative but not validated*.
> Full post-mortem:
> [`reframe-post-mortem.md`](../.claude/experiments/brain-vs-control/reports/reframe-post-mortem.md).

> **Scope note — one-shot vs longitudinal value.** The experiments
> this section summarizes are all single-turn: one task → one
> response → one judge score. They measure how static context
> injection affects *that* response on *that* rubric. They do NOT —
> and structurally cannot — measure what most of the Brain's
> architecture is built for: **cumulative project awareness across
> sessions**, cultural substrate persistence, capability-hygiene
> drift detection, invocation-ledger self-observability, governance
> decision history. Those are longitudinal properties; single-turn
> tests are bounded instruments. Treat the guidance below as
> applying to "which context to inject for this specific response"
> — not to "does the Brain have value over a project's life." The
> latter is the primary value hypothesis and is tested by
> artifacts-over-time (proposal ledger, promotion ledger,
> capability-hygiene history), not by one-shot comparisons.

What the experiments DO support:

- **L1 helps broadly on Sonnet**, not narrowly. On the held-out 22-task
  set, L1 won 18/22 tasks including generic coding and explanation
  tasks. The "factual-augmentation service" framing was directly
  contradicted.
- **L0 remains best on trivial tasks** (greetings, unit conversion,
  arithmetic) where context overhead + over-assertion hurt response
  quality.
- **Agent self-routing works** at the tool-invocation level (L2 Phase
  3). When given a `brain_query` tool with per-domain cost warnings,
  Sonnet invoked the Brain ~100% on repo-aware tasks and 0% on
  trivial tasks.
- **L1 can fail catastrophically on factual-lookup tasks** when the
  injected content is stale or wrong (held-out: 3/6 factual-lookup
  tasks went L0 by large margins). Content freshness is load-bearing.

What the experiments DO NOT support (as of 2026-04-23):

- A sharp "when to inject" rule. The held-out contradiction means
  any simple operator heuristic needs broader validation.
- The "Brain is a factual-augmentation service" reframe. Directly
  contradicted by held-out L1 wins on generic tasks.
- Class-level extrapolation. The per-task variance dominates class
  averages at N=12 and N=22.

**Tentative operator guidance** (revise if follow-up experiments
contradict):

- **Prefer Sonnet+ for Brain-augmented sessions.** Haiku's Phase 1
  pilot scored worse with context across every class.
- **Skip Brain context for trivial tasks** (greetings, short
  conversions, single-line regex). L1's overhead dominates.
- **Consider Brain context for tasks where the model might
  over-assert without reference material** — the Brain's injected
  context appears to help broadly on Sonnet, not just on
  "project-specific-facts" queries. The operative question isn't
  "does the answer require Brain data" but "would the model hedge
  or hallucinate without grounding content."
- **Check Brain content freshness before injecting.** Stale or wrong
  content produces worse L1 outcomes than no context at all on
  factual-lookup tasks.

**Experiment results** (summary):

| Arm | Behavior summary |
|---|---|
| L0 — no Brain | Best on trivial (~90 pts). Catastrophic on factual-lookup (scored 3-61 when answers require unknowable content). |
| L1 — static context (~6k tokens) | Broadly helpful on Sonnet (won 18/22 held-out tasks). Catastrophic on trivial tasks where context is pure overhead. Catastrophic on factual-lookup when context is stale/wrong. |
| L2 — live `brain_query` tool | Self-routing validated; synthesis under multi-turn tool use lags L1 on repo-aware (−12.75 pts). Matches L0 on trivial (tool refused 100%). |

Evidence base: Phase 2 (240 trials, N=10 Sonnet), Phase 3 (144 L2
trials), held-out Phase 4 (440 L0+L1 trials on 22 held-out tasks).

**Deeper evidence reading** (not all capabilities are tested — most
Brain features were never the independent variable):
[`docs/brain-capability-audit-2026-04-23.md`](docs/brain-capability-audit-2026-04-23.md).

**Experiment reports:** Phase 2 L0/L1
[phase2-report.md](../.claude/experiments/brain-vs-control/reports/phase2-report.md),
Phase 3 L2
[phase3-report.md](../.claude/experiments/brain-vs-control/reports/phase3-report.md),
cross-phase
[synthesis.md](../.claude/experiments/brain-vs-control/reports/synthesis.md),
dispatch-rule
[dispatcher-rule-analysis.md](../.claude/experiments/brain-vs-control/reports/dispatcher-rule-analysis.md),
spec discovery log
[METHODOLOGY-EVOLUTION.md §14](../LSP-Brains/spec/METHODOLOGY-EVOLUTION.md).
Candidate future work: BACKLOG B-14 (dispatch) + B-15 (audit-driven
review process).

## Agent Philosophy

When wearing a hat, announce it visibly: `Wear Hat: <hat-name>`.

Every task in this repo has a documented skill. Read the relevant skill before acting.

The LSP Brains specification lives in its own repo (https://github.com/KeenanHoffman/LSP-Brains).
This repo implements the spec in Rust.

The `domains/laas/` archive is read-only reference material. Do not modify it to match
spec changes — it preserves the state at the time of separation.
