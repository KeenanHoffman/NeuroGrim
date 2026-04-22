# Imagination Mode

**When to use this skill:** You are about to plan something non-trivial and the
right approach is not yet obvious — OR the user signals they want to think
out loud before committing ("imagine", "let's think", "what could we do",
"brainstorm", "before we plan"). Imagination mode is a conversational
pre-planning space: surface-level, idea-based, generalized. No code, no files,
no commands — just approaches, tradeoffs, and constraints explored aloud until
the right shape emerges.

Role: planning · meta

Hat: visionary

Trigger phrases: "imagine", "imagination mode", "let's think", "what could we do", "how might we",
Methodology-step: skills
"brainstorm", "explore approaches", "think through this", "ideate", "before we plan",
"what are the options", "what would it look like if", "talk through", "think out loud"

---

## What Imagination Mode Is

Imagination mode is a deliberate pre-plan thinking space. It exists because good software is
written twice: once in imagination, once in code. Before a plan is written — and before an
adversary reviews it — the most valuable question is: **are we solving the right problem in
the right way?**

In imagination mode, the agent and user think out loud together about approaches, tradeoffs,
and constraints — without committing to specifics. No files are written. No code is shown. No
commands are run. Everything stays at the level of concepts and generalizations.

This saves tokens (no deep research, no subagent spawning), reduces planning rework (a bad
approach discovered in imagination costs nothing to discard), and produces better plans (the
plan starts from a considered choice rather than the first workable idea).

---

## Hat

This skill invokes the `visionary` hat. Declare it at the start:

```
> Wear Hat: visionary — imagining [problem description].
```

The visionary mindset:
- **Explores breadth before depth** — surface multiple approaches before examining any closely
- **Names tradeoffs, doesn't resolve them** — identify decision points, not answers
- **Speaks in generalizations** — "some kind of post-edit hook", "a wrapper script", not filenames
- **Stays curious and conversational** — ask before converging; invite reaction before moving on
- **Defers specifics** — when an implementation detail surfaces, note it and continue

---

## How Imagination Sessions Work

An imagination session is a back-and-forth conversation. The agent proposes, the user reacts,
the agent explores the reaction. A session runs until the user feels ready to plan — typically
3–7 exchanges.

**Opening move:** Restate the goal in one sentence. Offer 2–3 meaningfully different approaches
with their core tradeoff named. Don't pick a winner. Present neutrally.

**Middle exchanges:** Probe each approach with open questions:
- "What breaks if we go this way?"
- "Is there a simpler version of this?"
- "What would have to be true for this to work?"
- "What are we trading away?"

**Closing move:** When the user signals readiness, summarize what was explored, which approaches
were ruled out and why, and what the key remaining decision points are. This summary is the
handoff to plan mode.

---

## Rules of Imagination Mode

These constraints are what make imagination mode lightweight and useful:

| Rule | Why |
|------|-----|
| No code examples | Code anchors thinking to one approach prematurely |
| No specific file paths | Paths are plan-level detail, not imagination-level |
| No commands | Commands are implementation, not imagination |
| No deep research | Imagination uses existing knowledge; research is for planning |
| No subagent spawning | Imagination is lightweight by design |
| Generalized tech references | "a hook" not the hook's filename; "a config file" not its path |
| No unilateral convergence | Don't pick an approach without user confirmation |

---

## Closing Imagination Mode

Before writing the closing summary, write a plan scaffold to `.claude/plans/_scaffold.md`
using the template below. If a previous scaffold exists, overwrite it. This scaffold is the
memory bridge between imagination and plan mode — plan mode reads it as seed context before
exploring the codebase, then deletes it after incorporating it into the real plan file.

**Scaffold template** (fill in from the imagination session):

```markdown
# Plan: [session topic]

> Imagination seed — written by visionary mode. Flesh out in plan mode, then delete this notice.

## Context
[leading direction from imagination closing, 2-3 sentences]

## Approaches Considered
- [approach A]
- [approach B]
- [approach C if applicable]

## Ruled Out
- [approach] — [one-line reason]

## Key Decision Points (open questions for the plan)
- [decision 1]
- [decision 2]

## Architecture Decisions
[TBD]

## Files to Modify
[TBD]

## Implementation Order
[TBD]

## Verification
[TBD]
```

Then end with the explicit handoff summary:

```
> Remove Hat: visionary — imagination complete. Scaffold → `.claude/plans/_scaffold.md`.

**Imagination summary:**
Explored: [approach A], [approach B], [approach C if applicable]
Ruled out: [approach] — [one-line reason]
Leading direction: [generalized description]
Key decision points for the plan:
  - [decision 1]
  - [decision 2]

Ready for: plan mode
```

The user can respond "write the plan" to continue into plan mode, or continue imagining a
different aspect of the problem.

**Plan mode pickup:** When entering plan mode after an imagination session, check for
`.claude/plans/_scaffold.md`. If it exists, read it as the first step of Phase 1 exploration
(before launching any Explore agents). After the plan file is fully written, delete
`_scaffold.md`. The scaffold is automatically overwritten by the next imagination session,
so deletion is best-effort cleanup, not critical.

---

## Example

> User: "I want to add rate limiting to the API."

> Wear Hat: visionary — imagining rate limiting approaches for the FastAPI backend.

Three meaningfully different approaches come to mind:

**A) Middleware-level** — intercept every request before it hits a handler. Clean separation,
consistent enforcement, but adds overhead to every call including internal health checks.

**B) Decorator-based** — annotate specific endpoints. Surgical and readable, but enforcement
is per-developer discipline and every new route needs the annotation.

**C) Gateway-level** — enforce at the load balancer before requests reach the service at all.
Zero app code changes, but less granular — harder to limit by user identity or schema type.

What's the primary driver here: abuse prevention, cost control, or fair use between tenants?
That shapes which approach is worth going deeper on.

---

## When to Use Imagination Mode

| Signal | Use imagination? |
|--------|-----------------|
| "I want to add X" — feature request, no obvious path | Yes — explore approaches |
| "How do we handle Y?" — open design question | Yes — explore tradeoffs |
| "What would it look like if..." — speculation | Yes — imagination territory |
| "Fix this bug in Z" — specific known fix | No — go straight to implementation |
| "Do what we did last time for X" — established pattern | No — follow the pattern |
| A plan already exists and needs review | No — use `plan-critic.md` instead |

---

## The Full Pre-Implementation Sequence

```
imagination-mode.md      (explore approaches — no code, no files)
  → plan mode            (write the plan file)
  → plan-critic.md       (adversarial review before implementing)
  → implementation
```

Imagination feeds plan mode with a considered direction. Plan mode feeds the critic with a
documented approach. The critic feeds implementation with a reviewed, de-risked plan.

Skipping imagination is fine for narrow, established patterns. Skipping the critic is only
fine for exceedingly simple plans. See `plan-critic.md` for the when-to-run heuristic.

---

## Why This Matters

Imagination mode implements a principle the rest of this skill system implicitly follows but
rarely names: **the cost of discarding an idea is zero; the cost of discarding half-written
code is high.** Every plan in this system is written before code is written. Imagination mode
extends that one step further — it is the thinking that happens before the plan is written.

This is **Fail Fast / Shift Left** from `archived/devops-philosophy.md` applied to design: catch the
wrong approach at the conversation layer, not the plan layer, not the implementation layer.

---

## See Also

- `plan-critic.md` — adversarial review of the plan that follows imagination; includes when-to-run heuristic
- `hats.md` — full hat system; `visionary` is the pre-planning lens
- `archived/skill-chain.md` — skill sequences; imagination → plan → plan-critic → implement
- `weigh-time-risk.md` — after imagination, calibrate how much planning depth the change needs
