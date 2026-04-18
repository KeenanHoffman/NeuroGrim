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

## Persona

This skill invokes the `adversary` persona. Declare it at the start:

```
> Persona: adversary — reviewing [plan name] before implementation.
```

See `personas.md` for the full persona system, subagent briefing format, and adversary checklist.

---

## When to Run the Critic

**Default rule: run the critic unless the plan is exceedingly simple.**

A plan is exceedingly simple when it is narrow on both axes in Step 0 (one file, executing a
documented pattern, no ripple effects). Everything else gets a critic pass.

**Four signals that always warrant the critic** — if a plan scores any of these, run it:

| Signal | Why it matters |
|--------|---------------|
| Plan touches >3 files | Multiple-file changes have interaction effects and ordering risks that single-file changes don't |
| Introduces a new hook | Hooks run on every matching tool call — a bug fires repeatedly, silently, until caught |
| Adds a new external tool dependency | New tools have setup requirements, version constraints, and fallback behaviors to plan for |
| Cross-cutting (hooks + skills, or code + infra) | Cross-cutting changes have two failure modes: each layer individually, and their interaction |

**Practical heuristic:** Did you have to read more than two existing files to write the plan? If
yes, the plan is complex enough to benefit from adversarial review.

**The cost of skipping:** The LSP deep integration plan skipped the critic. In retrospect it would
have caught (1) the grep detection false-positive discrimination problem and (2) the pyright CLI
go-to-definition gap — both required mid-implementation course corrections. The critic's job is to
surface these before code is written, not after.

---

## Step 0: Complexity Threshold

Before running the full protocol, decide how much review this plan warrants. The adversary's
value is proportional to **how much the plan's failure would cost** and **how little
precedent exists to catch it**. Ask two questions:

**Question 1 — Impact surface:** If this plan is wrong, how wide does the damage spread?
- *Narrow*: one script, one service, one workflow with no ripple effects → lighter review
- *Broad*: affects agent behavior across sessions, establishes patterns others will follow,
  cross-cutting through multiple systems → full adversary

**Question 2 — Novelty:** Is this plan executing a documented pattern, or establishing a new one?
- *Executing*: following an existing skill recipe (add-new-app, rotate a secret, fix a gate) → lighter review
- *Establishing*: first instance of a technique, new skill that defines behavior, new hook
  wiring, new abstraction layer → full adversary

**Third calibration question:** Would a future agent reading this plan encounter a concept
that doesn't yet exist in the skill system? If yes, the plan is establishing new ground
regardless of file type — invoke the full adversary.

**The rule:** Use the full protocol when either question points to broad impact or high novelty.
When both are narrow, a light scan (pilot agent inline, no subagents) is sufficient.

**Grounding example:** The personas + plan-critic work was all `.md` files — no code, no
schemas — yet it clearly warranted full adversary: it established new patterns that every
future planning session would follow (broad impact) and introduced abstractions that didn't
exist in the skill system (high novelty). A single-line syntax fix with a known safe pattern
is narrow on both axes — light scan.

### Light mode (narrow + low novelty)

Run the DevOps checklist from `personas.md` inline without spawning subagents. Write a
short paragraph noting any concerns. No structured output template required.

---

## Protocol (full adversary)

### 1. Read the plan

Read the plan file in `.claude/plans/` (or wherever the user points you). Note:
- Which files will be created or modified
- Which scripts will be added or changed
- Whether the plan touches any infrastructure (applies, destroys, state changes)
- Which skills or gates the plan references (or fails to reference)

### 2. Spawn targeted research subagents

The pilot agent is the orchestrator. It spawns Explore subagents for specific verifiable
concerns, receives their reports, and synthesizes findings. Subagents do not inherit the
`adversary` persona — they run without persona context but receive an explicit briefing:

```
[Persona: adversary] The pilot agent is acting as an adversarial plan reviewer.
Research: {specific concern}
Framing: {what the pilot agent is deciding}
Calibration: lean toward surfacing edge cases — false negatives are worse than false positives.
```

Spawn only the subagents relevant to what the plan actually contains. Use the adversary
checklist from `personas.md` to decide which categories apply.

**Research targets by category:**

| Category | What to look for |
|----------|-----------------|
| **Language-version compatibility** | Any new/modified source files using language features that post-date the CI toolchain — e.g., Rust `let else` needs 1.65+, Python match-statements need 3.10+. Verify the project's pinned toolchain supports every syntax choice. |
| **Test coverage** | New code paths or behaviors — are they covered by unit / integration / smoke tests? New sensors — is there a behavioral fixture? |
| **Idempotency** | Steps that modify state — can each step run twice without error or side effects? |
| **Ordering** | Steps that create resources — does any step depend on a resource created in a later step? |
| **Scope safety** | Any deploy / migration / data-mutation — does the plan risk touching production or a peer environment? |
| **Rollback path** | Steps that are hard to reverse — does the plan explain how to recover if step N fails? |
| **Secret safety** | Any new secret or env var handling — could it end up in a URL parameter, a log line, or a committed file? |
| **Destroy guard** | Any destructive operation — is the confirmation / dry-run / explicit-flag guard mentioned? |
| **Symbol impact** | Any rename/removal of a function, variable, skill, crate export, or schema field — find all callers via `grep` / `cargo check`; verify no silent breaks |

### 2b. Symbol Impact Audit (when plan renames or removes symbols)

Spawn this subagent when the plan **renames or removes** any of the following:
- A function, struct, enum variant, trait method, or module (Rust / Python / TS)
- A schema field or JSON / YAML registry key
- A skill file (rename or retirement)
- A CLI command or subcommand name
- A sensor domain key

**Subagent briefing template:**

```
[Persona: adversary] Symbol Impact Audit
Research: find ALL callers/references of '{symbol}' in the codebase before it is renamed/removed.
Symbol type: {rust function | schema field | skill | cli subcommand | sensor domain}
Tool: `grep -rn <symbol>` across source; `cargo check` after edits to surface hard refs.
Calibration: lean toward false positives — a missed caller means a silent runtime break.
```

**Adversary questions per symbol type:**

| Symbol | What to verify |
|--------|---------------|
| **Skill rename/retire** | Cross-refs in `See Also:` sections of other skills? `CLAUDE.md` Skills Index entries? `Governs:` fields pointing to old path? |
| **Rust function / export rename** | `use` statements in other crates? Caller `fn` bodies (grep)? `impl` blocks pulling the renamed trait method? Doc tests in rustdoc? `cargo check` catches most hard breaks immediately. |
| **Schema field rename** | Rust struct `#[serde(rename = ...)]` and Python pydantic/dataclass field names? All registry / CMDB files on disk that still use the old key? Schema validation errors after the change? |
| **Sensor domain key rename** | `domain_definitions` in every `brain-registry.json`? CMDB files named after the old key on disk? Tests asserting the old domain name? |
| **CLI subcommand rename** | Skills referencing the old subcommand? `CLAUDE.md` Run Tests section? README command examples? CI workflow invocations? |

**When NOT to spawn:** The plan only adds new symbols (functions, skills, gates) without removing
or renaming existing ones — new symbols have no existing callers to break.

### 2a. Scaled Review: One Subagent Per Domain (optional)

Use this variant when the plan introduces **≥3 independent subsystems**, each with its own
schema, test surface, and conventions, and cross-domain interaction effects are the primary risk.

**When to use:**
- Plan spans multiple structured corpora (e.g., skills + gates + workflows + topology)
- Each domain has a distinct file type and validation convention
- Cross-domain calls (hook in Domain A calling a script in Domain B) are a key risk

**When NOT to use:**
- Domains are structurally independent (no shared files, no cross-references)
- Use standard per-concern subagents (from Step 2 above) instead

**Domain subagent template:**

```
[Persona: adversary] Domain review: {domain name}
Scope: only the {domain-specific changes} in this plan
Research: what could fail specifically within this domain's conventions, schema, or test surface?
Calibration: lean toward edge cases — do NOT review cross-domain interactions (pilot agent's job).
Key schema/conventions: {paste relevant spec excerpt}
```

**Pilot agent synthesis:** After domain subagents report, the pilot agent reviews cross-domain
interactions exclusively: does a new hook in Domain A call a script in Domain B that Domain B's
subagent flagged as broken? Is there a circular dependency? Does a gate in Domain C watch a
path that Domain D's changes invalidate?

This step is the pilot agent's exclusive responsibility — domain subagents must not attempt it.

### 3. Synthesize findings

After subagents report back, synthesize using the severity tier framework:

| Symbol | Label | Meaning |
|--------|-------|---------|
| 🔴 | **Blocking** | Plan should not proceed without a fix — known breakage, data loss risk, security gap |
| 🟡 | **Concern** | Likely problem; worth addressing but not a hard stop |
| 🔵 | **Suggestion** | Optional improvement — style, efficiency, or future-proofing |
| 🟢 | **Strength** | Something done well — name it explicitly and genuinely |

**Tone rules:**
- Start with strengths. Find the genuine good in the plan — don't fabricate praise, but
  if the plan is well-structured, thorough, or catches its own rollback risks, say so.
- Frame concerns as forward-looking: "what could go wrong if..." rather than "you forgot..."
- Be proportionate. A style inconsistency is 🔵, not 🔴. Reserve 🔴 for real breakage.
- Every review must contain at least one 🟢. If the plan is genuinely problematic, you can
  still praise the effort to document before implementing.

### 4. Present the review

Use this output template:

```
## Plan Critic Review — [plan name]
**Persona: adversary**

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

If the verdict is `REVISE BEFORE IMPLEMENTING`, specify exactly what needs to change and
offer to update the plan file directly.

### 5. Return to default mode

After presenting the review:

```
> Persona: default — adversary review complete.
```

---

## Example: Language-version Compatibility Concern

```
🔴 [Blocking] The new fn uses `let Some(x) = get()? else { return Err(...) };` — this
is `let else` syntax stabilized in Rust 1.65. The workspace's rust-toolchain pin is 1.64,
so CI fails at parse time. Either bump the toolchain pin (with justification), or rewrite
as `let x = match get()? { Some(x) => x, None => return Err(...) };`.
```

---

## Why This Matters

Plans that look sound on paper routinely break in production due to environmental
differences, implicit ordering assumptions, and missing rollback paths. Writing a plan is
cheap; undoing a half-applied infrastructure change is expensive. The adversary persona
exists because the author of a plan is the least likely person to spot their own blind spots
— they already believe the plan is correct. A structured adversarial pass, with targeted
subagent research on verifiable concerns, surfaces the class of problems that optimistic
planning consistently misses. This is **Everything is Code** from `archived/devops-philosophy.md`
applied to planning itself: the review protocol is the code; the plan file is the artifact
it validates.

---

## See Also

- `personas.md` — full persona system, subagent briefing format, adversary checklist
- `review-loop.md` — iterative T+P+Code Reviewer loop for when plans involve skill or code authoring
- `dual-review.md` — T+P review protocol for skill/infrastructure quality review
- `weigh-time-risk.md` — risk/time tradeoff before deploy decisions
- `preflight.md` — 8-item readiness checklist before any `apply`
