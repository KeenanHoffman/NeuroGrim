# Dual Review Protocol

Two-pass review for skills, hooks, and architectural decisions. Apply when correctness
and principle alignment both matter — a Technical Reviewer and a Philosophy Reviewer
evaluate independently, then the results are synthesized with philosophy taking precedence.

Role: meta

Trigger phrases: "dual review", "review this skill", "technical and philosophy review",
Methodology-step: skills
"T+P review", "philosophy review", "check this against principles", "is this principled",
"both technical and philosophical check", "review this hook", "evaluate this decision"

---

## Why Two Passes

A single review pass has selection bias: a technically-focused reviewer approves commands
that work but embed bad patterns; a philosophically-focused reviewer may approve guidance
that reads well but contains incorrect commands. Running both passes in sequence, with the
second reviewer reading the first's output, surfaces conflicts explicitly so they can be
resolved rather than silently overridden.

---

## The Two Reviewer Lenses

### Technical Reviewer (T)

Focuses on: *Does this work correctly? Will it break in edge cases? Is it discoverable?*

**Questions T1–T5:**

| # | Question | Pass condition |
|---|---------|----------------|
| T1 | Are all code blocks syntactically valid and runnable as-written? | No placeholder commands, no truncated flags |
| T2 | Do the trigger phrases cover the realistic ways this skill would be invoked? | At least 4 phrases; covers abbreviations + error fragments |
| T3 | Does the troubleshooting section cover the top 3 real failure modes? | At minimum 2 concrete failure patterns with detection + fix |
| T4 | Are all cross-referenced skills/hooks/scripts named exactly as they exist on disk? | Every backtick-wrapped `.md` file resolves |
| T5 | Would a developer unfamiliar with this system understand every step without context? | No assumed knowledge of project history or prior sessions |

### Philosophy Reviewer (P)

Focuses on: *Does this reinforce platform principles? Is the "why" genuine?*

**Questions P1–P4:**

| # | Question | Pass condition |
|---|---------|----------------|
| P1 | Does the `## Why This Matters` section give a genuine reason, or just restate what the skill does? | "This prevents X" or "This ensures Y" — not "This is how we do X" |
| P2 | Would the Platform Migration Test pass? If the platform moved from GCP to AWS tomorrow, would the principle still apply? | Principle is infrastructure-agnostic; only the commands are GCP-specific |
| P3 | Is exactly ONE principle cited? Multiple principles may mean the skill is doing too much. | One principle per skill; split if two distinct principles appear |
| P4 | Does the skill's guidance reinforce the principle, or could a developer follow the steps while violating it? | No shortcuts embedded in the steps that contradict the stated principle |

---

## Standard Path: Sequential Awareness (2 passes)

This is the default protocol. Use it for any new skill or hook before registering it.

```
1. T Reviewer runs first
   → Produces T1–T5 assessment (pass / warn / fail per question)
   → Notes any commands needing correction

2. P Reviewer runs second, reading T's output
   → Produces P1–P4 assessment
   → If T found a technical issue, P considers whether the philosophy recommendation
     would make the technical issue worse or better

3. Synthesis
   → If no conflicts: approve
   → If T and P conflict: philosophy takes precedence
   → Technical constraints that cannot be resolved without compromising a principle
     must be documented as a named tension in ## Implementation Notes
```

**When to use sequential awareness vs. escalate:**
- Use sequential awareness when T and P assessments are compatible (most cases)
- Escalate to alternating rounds only when T identifies a concrete correctness failure
  that P's recommendation would cause

---

## Staged Agent Path (optional enhancement)

For high-stakes reviews, T and P can be spawned as actual subagents rather than running
as successive prompts in one conversation. Use this when structural visibility of conflict
is important — separate agents cannot resolve conflicts silently.

**When to use spawned agents vs. sequential prompts:**

| Situation | Spawned agents | Sequential prompts |
|-----------|---------------|-------------------|
| New blocking hook (exits non-zero) | ✓ Conflicts must be structurally visible | — |
| Changes to gate behavior or deploy order | ✓ High blast radius | — |
| Skill that explicitly overrides a DevOps principle | ✓ Override needs documented justification | — |
| Routine skill review (new path, updated step) | — | ✓ Sequential is sufficient |
| Sequential awareness passes cleanly | — | ✓ No benefit to spawning |

**Agent A — T Reviewer prompt template:**
```
"You are a Technical Reviewer for the LaaS skill system. Read `.claude/skills/<skill-name>.md`.
Evaluate ONLY the Technical Lens (T1–T5 from `dual-review.md`).
Return ONLY a JSON result:
{
  \"passed\": bool,
  \"error\": null | \"<summary of failures>\",
  \"t1\": \"pass|warn|fail\", \"t1_note\": \"<finding or OK>\",
  \"t2\": \"pass|warn|fail\", \"t2_note\": \"<finding or OK>\",
  \"t3\": \"pass|warn|fail\", \"t3_note\": \"<finding or OK>\",
  \"t4\": \"pass|warn|fail\", \"t4_note\": \"<finding or OK>\",
  \"t5\": \"pass|warn|fail\", \"t5_note\": \"<finding or OK>\",
  \"conflicts_for_p\": [\"<finding that P should address with philosophy precedence>\"]
}"
```

**Agent B — P Reviewer prompt template (receives Agent A's full output as context):**
```
"You are a Philosophy Reviewer for the LaaS skill system. Read `.claude/skills/<skill-name>.md`.
The Technical Reviewer returned: <paste Agent A JSON output>.
Evaluate ONLY the Philosophy Lens (P1–P4 from `dual-review.md`). Address any items in
`conflicts_for_p`. Remember: philosophy takes precedence in synthesis.
Return ONLY a JSON result:
{
  \"passed\": bool,
  \"error\": null | \"<summary of failures or tensions>\",
  \"p1\": \"pass|warn|fail\", \"p1_note\": \"<finding or OK>\",
  \"p2\": \"pass|warn|fail\", \"p2_note\": \"<finding or OK>\",
  \"p3\": \"pass|warn|fail\", \"p3_note\": \"<finding or OK>\",
  \"p4\": \"pass|warn|fail\", \"p4_note\": \"<finding or OK>\",
  \"synthesis\": \"approve | revise: <what to change> | escalate: <reason>\",
  \"named_tensions\": [\"<tension description if any>\"]
}"
```

**Why agents over sequential prompts:** When T and P are separate agents, any conflict
between their outputs is structurally visible in the parent context and must be reconciled
explicitly. A single agent playing both roles has a cognitive bias toward resolving
conflicts internally before surfacing them, producing a synthesis that may silently embed
unresolved tensions.

**Convergence:**
- Both `passed: true` + synthesis `approve` → approve the skill
- Either `passed: false` → surface all findings; do not register the skill
- Parent writes the final synthesis; neither subagent writes to disk

See `subagent-patterns.md` Pattern 3 (Sequential Hand-Off) for the full spawn pattern
and JSON result format conventions.

---

## Escalation Path: Alternating Rounds (genuine conflicts only)

Triggered when: the P reviewer overrides a T recommendation that T considers a safety
or correctness issue (not just a style preference).

**Maximum 3 rounds — after round 3, philosophy wins with risk documented.**

```
Round 1 — T states the conflict explicitly:
  "This philosophy recommendation will cause [specific failure scenario]
   because [concrete reason]. The affected steps are [list]."

Round 2 — P acknowledges and proposes a minimal compromise:
  "Acknowledged. The principle still requires [core behavior] but
   [specific technical adjustment] can be made without violating it."

Round 3 — T accepts the compromise or documents residual risk:
  Option A: "Compromise accepted. Updated recommendation: [text]."
  Option B: "Residual risk: [specific scenario] remains possible.
             Documenting as named tension in ## Implementation Notes."
```

---

## Output Format

Use this template when producing a dual review result. It may appear in a skill's
`## Implementation Notes` section, in a PR description, or in a session note.

```markdown
## Dual Review: [skill or hook name]

### Technical Lens
- T1: [pass/warn/fail] — [finding, or "OK"]
- T2: [pass/warn/fail] — [finding, or "OK"]
- T3: [pass/warn/fail] — [finding, or "OK"]
- T4: [pass/warn/fail] — [finding, or "OK"]
- T5: [pass/warn/fail] — [finding, or "OK"]

### Philosophy Lens
- P1: [pass/warn/fail] — [finding, or "OK"]
- P2: [pass/warn/fail] — [finding, or "OK"]
- P3: [pass/warn/fail] — [finding, or "OK"]
- P4: [pass/warn/fail] — [finding, or "OK"]

### Synthesis
- Conflicts: [none | named tension: description]
- Recommendation: [approve | revise: what to change | escalate: reason]
- Implementation Notes: [if tension exists — what was decided and why]
```

---

## When to Invoke Dual Review

`assess-skill-on-edit.sh` runs the T/P questions automatically (as advisory output) on
every skill edit. Invoke the full dual-review protocol manually for higher-stakes cases:

| Situation | Reason |
|-----------|--------|
| New skill introducing a new operational pattern | Pattern will be repeated — get it right before it propagates |
| Any **blocking** hook (exits non-zero) | Blocking hooks cause friction; both lenses needed to justify the cost |
| Architectural decision changing gate behavior or deploy order | High blast radius; conflicts should be explicit |
| A skill that explicitly overrides a DevOps principle | "Skip this gate because..." needs both correctness AND principle justification |
| New skill+hook pair being registered | Before wiring into `settings.json`, confirm both lenses agree |

---

## Synthesis Rule (single sentence)

**Philosophy takes precedence; technical constraints that cannot be resolved without
compromising a principle must be documented as named tensions, never silently embedded
in the implementation.**

---

## Why This Matters

This protocol implements **Observability Before Action** from `devops-philosophy.md`.
A decision made with only one lens is a blind spot: technically correct but philosophically
hollow, or principled but broken in practice. Surfacing both dimensions explicitly — and
forcing a documented synthesis when they conflict — ensures the system's guidance is both
runnable and principled. The Platform Migration Test applies here: on any platform, the
need to validate both technical correctness and principle alignment survives; only the
specific questions change.

---

## Troubleshooting

**Problem: T and P agree on everything — the review feels mechanical**
- That's the expected outcome for a well-written skill. A review that finds nothing is still
  valuable: it confirms the skill passes both lenses and can be registered without caveats.
- Do not manufacture conflicts to make the review feel more rigorous.

**Problem: Not sure whether a conflict is "genuine" (worth escalating) vs. a style preference**
- Genuine conflict: T says "this command will fail in scenario X" and P says "the principle
  requires keeping this command." Escalate.
- Style preference: T says "this phrasing is awkward" or P says "this could be more
  philosophical." Resolve in synthesis without escalation.

**Problem: assess-skill-on-edit.sh T/P output is advisory but I need a formal review**
- The hook output is a quick scan, not a formal dual review. For a formal record (PR
  description, architectural decision log), produce the full output template above and
  include it in the relevant document.

---

## See Also

- `write-skill.md` — authoring guide + companion hook evaluation
- `devops-philosophy.md` — the 8 principles P1–P4 test against
- `skill-hook-pairs.md` — catalog of skill↔hook pairs; new pairs should pass dual review
- `assess-skill-on-edit.sh` — automated T/P pass that fires on every skill edit
- `review-loop.md` — iterative T+P+Code Reviewer loop for when the work will be revised in response to findings; wraps this protocol with a structured iteration harness and exit conditions
