---
name: subagent-patterns
description: >-
  You have a workflow with multiple independent concerns and you are
  deciding whether to spawn subagents to run them in parallel vs. serialize
  inline — OR you've already decided to spawn and need the specific pattern
  (fan-out, pipeline, staged verification) and convergence approach. Use
  this skill whenever a task has ≥ 2 concerns that are genuinely
  independent, wall-clock saving > 60s, and each concern fits a precise 3-5
  sentence prompt. This skill carries the decision surface + pattern index;
  `docs/subagent-patterns-guide.md` carries the walk-throughs.
when_to_use: >-
  "spawn a subagent", "run in parallel", "parallelize this workflow",
  "subagent coordination", "fan-out pattern", "multiple agents", "staged
  agents", "parallel verification", "how do I use the Agent tool",
  "coordinate agents", "independent concerns", "run these simultaneously"
---

# Coordinate Subagents

**When to use this skill:** You have a workflow with multiple independent
concerns and you are deciding whether to spawn subagents to run them in
parallel vs. serialize inline — OR you've already decided to spawn and need
the specific pattern (fan-out, pipeline, staged verification) and convergence
approach. Use this skill whenever a task has ≥ 2 concerns that are genuinely
independent, wall-clock saving > 60s, and each concern fits a precise 3-5
sentence prompt. This skill carries the decision surface + pattern index;
`docs/subagent-patterns-guide.md` carries the walk-throughs.

Role: operational · reference
Protocol: lsp-brains/agent/1.0 (see pilot-protocol/SKILL.md for full envelope reference)

Trigger phrases: "spawn a subagent", "run in parallel", "parallelize this workflow",
Domain: deploy
Methodology-step: skills
"subagent coordination", "fan-out pattern", "multiple agents", "staged agents",
"parallel verification", "how do I use the Agent tool", "coordinate agents",
"independent concerns", "run these simultaneously"

---

## Decision Table — Spawn or Inline?

| Condition | Decision |
|-----------|---------|
| ≥2 concerns are genuinely independent (no shared write target, no dependency on each other's output) AND wall-clock saving > 60s AND each concern fits a precise 3–5 sentence prompt | **Spawn** |
| B requires A's output as input | **Run inline sequentially** |
| Overhead of writing precise subagent prompts exceeds the time saved | **Run inline** |
| Total task time < 90 seconds | **Run inline** |
| Concerns write to shared state (git commit, gate update, topology JSON) | **Never spawn — serialize** |

**Shared-state rule:** two agents writing the same file will corrupt it.
Gates, git commits, and topology JSON are always written by the parent after
all subagents return. Never delegate a write.

**Overhead rule:** spawning costs ~5–15 seconds per agent. Parallelism only
pays when each concern's work exceeds that threshold.

---

## Pattern Summary

| # | Pattern | Use when | Full walk-through |
|---|---------|----------|-------------------|
| 1 | **Parallel Fan-Out** | N independent concerns, no shared state, each ≥ 15s work | [`guide § Pattern 1`](../../../docs/subagent-patterns-guide.md#pattern-1--parallel-fan-out) |
| 2 | **Staged Convergence** | Parallel stage 1 feeds a stage 2 decision (inline or parallel) | [`guide § Pattern 2`](../../../docs/subagent-patterns-guide.md#pattern-2--staged-convergence) |
| 3 | **Sequential Hand-Off** | A's structured output flows as explicit input to B | [`guide § Pattern 3`](../../../docs/subagent-patterns-guide.md#pattern-3--sequential-hand-off) |
| 4 | **Hat-Calibrated Briefing** | Parent wearing a hat; subagents need the same lens | [`guide § Pattern 4`](../../../docs/subagent-patterns-guide.md#pattern-4--hat-calibrated-briefing) |
| 5 | **Sensor Fan-Out** | Parallel `neurogrim sensory *` queries bucketed by latency | [`guide § Pattern 5`](../../../docs/subagent-patterns-guide.md#pattern-5--sensor-fan-out) |
| 6 | **Human-Facing Output** | Any message to the human user — treat as a distillation problem | [`guide § Pattern 6`](../../../docs/subagent-patterns-guide.md#pattern-6--human-facing-output-communication-interface) |

The guide carries every pattern's walk-through with worked examples (LaaS
incident response, post-deploy verification, pre-deploy safety gate, debug
Cloud Run probes), complete prompt templates, convergence logic, and hat
calibration blocks.

---

## Envelope Protocol (Required)

Every subagent prompt uses the LSP Brains agent envelope. Full schema:
`pilot-protocol/SKILL.md`. Summary:

1. Read the skill manifest → get `responsibility`, `required_hat`, schemas.
2. Build the request envelope (copy `required_hat` into `wear_hat`).
3. Spawn; parse the delimited `<!-- LSP-ENVELOPE:{id} -->` block from the
   response; validate `worn_hat == wear_hat`.
4. On malformed output: retry once, then abort. Never assume success.

Convergence failure handling + envelope integration details:
`docs/subagent-patterns-guide.md` § Envelope Protocol Integration.

---

## Why This Matters

Parallelism in verification isn't about speed for its own sake — it's about
closing the feedback loop before the next decision point. A serialized 4-probe
check takes 2–4 minutes; the same probes in parallel take 30–60 seconds.
Under incident pressure, those minutes are the gap between a fast rollback
decision and a delayed one. Implements **Fail Fast / Shift Left** from
`archived/devops-philosophy.md`.

---

## Troubleshooting (Top 3)

**Subagent returns partial output / stops mid-task** — prompt scope was too
broad. Narrow to a single bounded concern. Add to the prompt: *"if you
encounter any ambiguity or need to make a judgment call, return `{error:
'ambiguity: <description>'}` rather than asking a question or continuing"*.

**Parallel results arrive in different orders** — normal async behavior when
multiple agents spawn in one message. Always key results by concern name
(`{preflight: ..., plan_review: ...}`), never by list index.

**Subagent reads a stale skill version from disk** — skill was updated between
the parent's read and the subagent's read. Use Method 2 from the guide (inline
the relevant section) when parent-subagent consistency matters.

Further troubleshooting: `docs/subagent-patterns-guide.md` § Troubleshooting.

---

## See Also

- `pilot-protocol/SKILL.md` — envelope schema (required before spawning).
- `dual-review/SKILL.md` — Pattern 3 worked example.
- `docs/subagent-patterns-guide.md` — full reference for all 6 patterns with
  worked examples, hat calibration blocks, envelope integration, failure
  handling, and the hook-system boundary.
