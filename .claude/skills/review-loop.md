# Review Loop

Use this skill when a plan involves authoring or substantially revising a skill, hook, or
architectural decision — and the output needs iterative quality gates rather than a
single-pass review. The review loop runs T (Technical) and P (Philosophy) reviewers in
sequence, hands their findings to a Code Reviewer for synthesis and revision requests, then
loops back until all concerns are resolved or iteration is exhausted.

Use it after `dual-review.md` issues a `revise:` verdict, or whenever the work being
reviewed will be edited in response to findings (as opposed to just being read and approved).

Role: meta

Trigger phrases: "review loop", "iterative review", "keep reviewing until clean",
Methodology-step: skills
"run the loop", "T+P loop", "code reviewer", "review and revise", "loop until approved",
"run until no concerns"

---

## Why a Loop, Not a Single Pass

`dual-review.md`'s Standard Path is a single-cycle evaluation — T reviews, P reviews,
synthesis recommends approve or revise. It deliberately stops there: it says what needs to
change but doesn't define who re-runs T+P after changes are made, or when to stop.

The review loop fills that gap. It's the right tool when:
- The output will be edited based on findings (not just accepted or rejected)
- The revision quality needs to be verified, not assumed
- Multiple rounds are expected before the work is clean

The loop is not a replacement for dual-review — it's the iteration harness that wraps it.

---

## Orchestration

**The pilot agent is the loop orchestrator.** It:
1. Spawns T and P as subagents (or runs them as sequential prompts for lower-stakes work)
2. Receives their reports
3. Spawns the Code Reviewer to synthesize
4. Decides whether to iterate based on the Code Reviewer's verdict
5. Loops back to step 1 if NEEDS-WORK, or exits if APPROVED or ESCALATE

**Hat isolation:** The pilot agent's current hat does not transfer to T, P, or the
Code Reviewer. Each reviewer runs without an inherited hat unless explicitly briefed. The
loop orchestrator remains in whatever mode it was in before the loop started — the loop
itself is not hat-bearing.

---

## The Three Agents

### T — Technical Reviewer

Evaluates technical correctness using T1–T5 from `dual-review.md`:
- T1: Code blocks syntactically valid and runnable?
- T2: Trigger phrases cover realistic invocation patterns?
- T3: Troubleshooting covers top 3 real failure modes?
- T4: All cross-references resolve on disk?
- T5: Understandable without external context?

Returns a structured finding: pass/warn/fail per question, plus any items flagged for P.

### P — Philosophy Reviewer

Reads T's output, then evaluates principle alignment using P1–P4 from `dual-review.md`:
- P1: `## Why This Matters` gives a genuine reason, not a restatement?
- P2: Platform Migration Test passes?
- P3: Exactly one principle cited?
- P4: Steps reinforce the principle (can't be followed while violating it)?

Returns a structured finding: pass/warn/fail per question, plus synthesis recommendation.

**Philosophy precedence is non-negotiable.** The Code Reviewer cannot close a P finding.
Only P can close a P concern — by re-evaluating on the next iteration and finding it
resolved. A Code Reviewer that issues APPROVED while a P concern is open is a protocol
violation.

### Code Reviewer — Synthesis Agent

Receives T and P findings. Its role is synthesis, not arbitration:
- Identifies which T and P findings are unresolved
- Generates a prioritized revision list (most blocking first)
- Issues one of three verdicts: APPROVED, NEEDS-WORK, or ESCALATE
- Does not perform new technical or philosophy checks
- Does not re-litigate findings that were resolved in a previous iteration

**APPROVED** means: T has no open technical blockers AND P has no open philosophy concerns.
Both conditions must hold — APPROVED on T alone is not APPROVED.

---

## The Loop

```
Iteration N:
  Step 1 — T Reviewer evaluates the current version
  Step 2 — P Reviewer evaluates, reading T's output
  Step 3 — Code Reviewer synthesizes T+P findings
            → APPROVED: exit loop, work is clean
            → NEEDS-WORK: produce revision list, loop to iteration N+1
            → ESCALATE: loop has run 3 times with open items; flag for human review
  Step 4 — Worker applies revisions from the revision list
  Step 5 — Loop back to Step 1
```

**Exit conditions:**

| Condition | What it means | Action |
|-----------|--------------|--------|
| APPROVED | No open T or P items | Exit loop; register or merge the work |
| NEEDS-WORK (iterations 1–2) | Open items remain | Apply revisions, loop |
| ESCALATE (iteration 3) | 3 iterations with unresolved items | Halt; surface to human; do not auto-approve |

The 3-iteration cap prevents infinite loops on genuinely ambiguous issues. When ESCALATE
fires, the Code Reviewer documents which items remain open and why they haven't resolved,
then the human decides whether to override, redesign, or accept the tension.

---

## Revision List Format

The Code Reviewer produces revisions in this format:

```
## Revision List — Iteration [N]

**Open T items:**
- [T1/T2/T3/T4/T5]: [what needs to change, specifically]

**Open P items:**
- [P1/P2/P3/P4]: [what needs to change, specifically]
  Note: Only P reviewer can close this on next pass.

**Verdict: NEEDS-WORK**
Apply the above before iteration [N+1].
```

---

## When to Use Staged Agents vs. Sequential Prompts

For lower-stakes work, T and P can run as sequential prompts within the pilot agent rather
than spawned subagents. Use spawned agents when:
- The output is a new blocking hook (exits non-zero)
- The change affects gate behavior or deploy order
- Conflict between T and P needs to be structurally visible

See `dual-review.md` — Staged Agent Path for the full spawn templates and JSON result format.

---

## Why This Matters

A single-pass review is insufficient for work that will be revised. When the author applies
changes in response to a review and no second pass runs, you have unverified fixes — changes
that address the letter of the concern but not the spirit, or introduce new issues while
closing old ones. The loop closes this gap by making re-verification mandatory, not assumed.
The 3-iteration cap and ESCALATE verdict prevent the loop from becoming a bureaucratic
obstacle — it's a quality gate, not a roadblock. This is **Automation Over Documentation**
from `archived/devops-philosophy.md`: the review loop is a repeatable, structured process that
produces consistent quality outcomes rather than relying on reviewer memory and goodwill.

---

## See Also

- `dual-review.md` — the T+P protocol this loop wraps; T1–T5 and P1–P4 question definitions
- `hats/SKILL.md` — hat system; hat isolation during loop orchestration
- `plan-critic/SKILL.md` — adversarial plan review (complement to the review loop)
- `subagent-patterns.md` — Pattern 3 (Sequential Hand-Off) for spawning T and P as agents
