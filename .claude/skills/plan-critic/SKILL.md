---
name: plan-critic
description: Adversarial plan reviewer. Spawn this skill before implementing any plan — it surfaces pitfalls, missing rollback paths, gate gaps, and compatibility risks before a single line of code is written or a single `apply` is run. Reviews a plan file in `.claude/plans/` and returns findings tagged 🔴 Blocking / 🟡 Concern / 🔵 Suggestion / 🟢 Strength plus a PROCEED / PROCEED WITH CAUTION / REVISE verdict. Invokes the `adversary` hat.
when_to_use: Run whenever you see a plan file in `.claude/plans/`, or when the user says "review my plan", "devil's advocate", "critique this plan", "before I implement", "what could go wrong", "sanity check my plan", "adversarial review", "poke holes", or "stress test this plan." Default rule — run the critic unless the plan is exceedingly simple (one file, documented pattern, no ripple effects).
---

# Plan Critic

Use this skill before implementing any plan. The plan critic performs an adversarial review
of a plan file — surfacing pitfalls, missing rollback paths, gate gaps, and compatibility
risks before a single line of code is written or a single `apply` is run.

Use this skill whenever you see a plan file in `.claude/plans/`, whenever the user says
"review my plan", "devil's advocate", "what could go wrong", "critique this", "before I
implement", or any variation of "sanity check my plan."

Role: meta

Trigger phrases: "review my plan", "critique this plan", "devil's advocate", "plan-critic",
Methodology-step: skills
"before I implement", "what could go wrong", "sanity check my plan", "adversarial review",
"find the holes in this plan", "poke holes", "stress test this plan"

---

## Hat

This skill invokes the `adversary` hat. Declare it at the start:

```
> Wear Hat: adversary — reviewing [plan name] before implementation.
```

See `hats/SKILL.md` for the full hat system, subagent briefing format, and
adversary checklist.

---

## When to Run the Critic

**Default rule: run the critic unless the plan is exceedingly simple.**

A plan is exceedingly simple when it is narrow on both complexity axes in Step 0
(one file, executing a documented pattern, no ripple effects). Everything else
gets a critic pass.

**Four signals that always warrant the critic** — if a plan scores any of
these, run it:

| Signal | Why it matters |
|--------|---------------|
| Plan touches >3 files | Multiple-file changes have interaction effects and ordering risks that single-file changes don't |
| Introduces a new hook | Hooks run on every matching tool call — a bug fires repeatedly, silently, until caught |
| Adds a new external tool dependency | New tools have setup requirements, version constraints, and fallback behaviors to plan for |
| Cross-cutting (hooks + skills, or code + infra) | Cross-cutting changes have two failure modes: each layer individually, and their interaction |

**Practical heuristic:** did you have to read more than two existing files to
write the plan? If yes, the plan is complex enough to benefit from adversarial
review.

**For pre-release / epic-close-out contexts**: see METHODOLOGY-EVOLUTION §16 for the multi-round assessment cadence (strict bar → surgical bar → diminishing-returns + Phase 1.5 escape hatch). §16 is RECOMMENDED post-execution; this skill remains the single-pass plan-time critic.

---

## Step 0 — Complexity Threshold (Summary)

Decide: full protocol or light mode? Full guide in
`docs/plan-critic-guide.md` § Step 0.

- **Full protocol** when either (a) impact surface is broad — affects agent
  behavior across sessions, establishes patterns, cross-cutting — OR (b)
  novelty is high — first instance of a technique, new skill, new abstraction.
- **Light mode** only when both narrow: one file, documented pattern, no
  ripple. Light mode runs the DevOps checklist from `hats/SKILL.md` inline without
  spawning subagents.

---

## Protocol (Full Adversary, 5 Steps)

### 1. Read the plan

Read the plan file in `.claude/plans/` (or wherever the user points you).
Note: which files are created/modified, which scripts change, whether
infrastructure is touched, which skills/gates the plan references (or
fails to reference).

### 2. Spawn targeted research subagents

The pilot agent is the orchestrator. It spawns Explore subagents for
specific verifiable concerns, receives their reports, and synthesizes
findings. Subagents do not inherit the `adversary` hat — they run without
hat context but receive an explicit briefing:

```
Hat: adversary — adversarial plan reviewer; skeptical, surface edge cases
Research: {specific concern}
Framing: {what the pilot agent is deciding}
Calibration: lean toward surfacing edge cases — false negatives are worse
             than false positives.
```

Spawn only the subagents relevant to what the plan actually contains.

**Research targets by category:**

| Category | What to look for |
|----------|-----------------|
| **Language-version compatibility** | Any new/modified source files using language features that post-date the CI toolchain |
| **Test coverage** | New code paths — unit/integration/smoke test coverage; new sensors — behavioral fixture |
| **Idempotency** | State-modifying steps — can each run twice without error or side effects? |
| **Ordering** | Resource-creating steps — any step depending on a resource created in a later step? |
| **Scope safety** | Any deploy/migration/data-mutation — risk of touching production or peer environments? |
| **Rollback path** | Hard-to-reverse steps — recovery story if step N fails? |
| **Secret safety** | New secret/env var handling — could it end up in a URL, log line, or committed file? |
| **Destroy guard** | Destructive operations — confirmation/dry-run/explicit-flag guard? |
| **Symbol impact** | Any rename/removal — find all callers; verify no silent breaks |

**Step 2b — Symbol Impact Audit** (when plan renames/removes any symbol):
spawn a dedicated subagent. Per-symbol-type audit tables + briefing
template live in `docs/plan-critic-guide.md` § Step 2b.

**Step 2a — Scaled Review** (≥ 3 independent subsystems, cross-domain
interaction is the primary risk): optional variant with per-domain
subagents. Full pattern + template in
`docs/plan-critic-guide.md` § Step 2a.

### 3. Synthesize findings

Use the severity tier framework:

| Symbol | Label | Meaning |
|--------|-------|---------|
| 🔴 | **Blocking** | Plan should not proceed without a fix — known breakage, data loss risk, security gap |
| 🟡 | **Concern** | Likely problem; worth addressing but not a hard stop |
| 🔵 | **Suggestion** | Optional improvement — style, efficiency, or future-proofing |
| 🟢 | **Strength** | Something done well — name it explicitly and genuinely |

**Tone rules:** start with strengths; frame concerns as forward-looking
("what could go wrong if..."); be proportionate; every review must contain
at least one 🟢. Full tone rules:
`docs/plan-critic-guide.md` § Step 3 Tone Rules.

### 4. Present the review

Use this output template:

```
## Plan Critic Review — [plan name]
**Hat: adversary**

### Strengths
🟢 [Specific thing done well]

### Issues
🔴 [Blocking] [Description of what would break and why]
🟡 [Concern] [What could go wrong if this isn't addressed]
🔵 [Suggestion] [Optional improvement]

### Verdict
[PROCEED | PROCEED WITH CAUTION | REVISE BEFORE IMPLEMENTING]

[One sentence explaining the verdict and what (if anything) needs to change.]
```

**Verdict guidance:**

| Verdict | When to use |
|---------|------------|
| `PROCEED` | No blocking issues; concerns are optional |
| `PROCEED WITH CAUTION` | No blocking issues but one or more 🟡 concerns worth tracking |
| `REVISE BEFORE IMPLEMENTING` | One or more 🔴 blocking issues found |

If the verdict is `REVISE BEFORE IMPLEMENTING`, specify exactly what needs
to change and offer to update the plan file directly.

### 5. Return to default mode

```
> Remove Hat: adversary — review complete.
```

---

## Why This Matters

Plans that look sound on paper routinely break in production due to
environmental differences, implicit ordering assumptions, and missing
rollback paths. Writing a plan is cheap; undoing a half-applied
infrastructure change is expensive. The adversary hat exists because the
author of a plan is the least likely person to spot their own blind spots
— they already believe the plan is correct. A structured adversarial pass,
with targeted subagent research on verifiable concerns, surfaces the class
of problems that optimistic planning consistently misses. This is
**Everything is Code** from `archived/devops-philosophy.md` applied to
planning itself: the review protocol is the code; the plan file is the
artifact it validates.

---

## See Also

- `docs/plan-critic-guide.md` — full reference: Step 0 complexity
  calibration (3 questions + light-mode operation), Step 2b Symbol Impact
  Audit with per-symbol-type rules, Step 2a Scaled Review variant with
  domain subagent template, full tone rules, worked example of a 🔴
  Language-Version finding, cost-of-skipping lessons, research category
  briefing expansions.
- `hats/SKILL.md` — full hat system, subagent briefing format, adversary
  checklist.
- `review-loop/SKILL.md` — iterative T+P+Code Reviewer loop for plans involving
  skill or code authoring.
- `dual-review/SKILL.md` — T+P review protocol for skill/infrastructure quality
  review.
- `weigh-time-risk.md` — risk/time tradeoff before deploy decisions.
- `preflight.md` — 8-item readiness checklist before any `apply`.
- `LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` §16 — multi-round pre-release assessment cadence (RECOMMENDED for pre-release / epic-close-out contexts; complements this skill's single-pass protocol).
