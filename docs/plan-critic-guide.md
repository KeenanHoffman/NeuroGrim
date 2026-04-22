# Plan Critic — Full Guide

Deep reference for the `plan-critic.md` skill. The skill carries
the decision surface (when to run, complexity threshold, 5-step
protocol summary, research-category table, severity tier framework,
output template, Why This Matters). This guide carries the depth:
Step 0 calibration questions, light-mode operation, Step 2b Symbol
Impact Audit with per-symbol-type rules, Step 2a Scaled Review
variant with domain subagent template, tone rules, worked examples,
and cost-of-skipping lessons.

Read the skill first. Come here when you need the per-symbol audit
tables, the domain-subagent variant for cross-cutting plans, or
examples of specific concerns the critic has caught.

---

## Step 0 — Complexity Threshold (Full Calibration)

Before running the full protocol, decide how much review this plan
warrants. The adversary's value is proportional to **how much the
plan's failure would cost** and **how little precedent exists to
catch it**. Ask two questions:

**Question 1 — Impact surface:** If this plan is wrong, how wide
does the damage spread?
- *Narrow*: one script, one service, one workflow with no ripple
  effects → lighter review.
- *Broad*: affects agent behavior across sessions, establishes
  patterns others will follow, cross-cutting through multiple
  systems → full adversary.

**Question 2 — Novelty:** Is this plan executing a documented
pattern, or establishing a new one?
- *Executing*: following an existing skill recipe (add-new-app,
  rotate a secret, fix a gate) → lighter review.
- *Establishing*: first instance of a technique, new skill that
  defines behavior, new hook wiring, new abstraction layer →
  full adversary.

**Third calibration question:** Would a future agent reading this
plan encounter a concept that doesn't yet exist in the skill
system? If yes, the plan is establishing new ground regardless of
file type — invoke the full adversary.

**The rule:** use the full protocol when either question points
to broad impact or high novelty. When both are narrow, a light
scan (pilot agent inline, no subagents) is sufficient.

**Grounding example:** the hats + plan-critic work was all `.md`
files — no code, no schemas — yet it clearly warranted full
adversary: it established new patterns that every future planning
session would follow (broad impact) and introduced abstractions
that didn't exist in the skill system (high novelty). A single-
line syntax fix with a known safe pattern is narrow on both axes
— light scan.

### Light mode (narrow + low novelty)

Run the DevOps checklist from `hats.md` inline without spawning
subagents. Write a short paragraph noting any concerns. No
structured output template required.

---

## Step 2b — Symbol Impact Audit (Full)

Spawn this subagent when the plan **renames or removes** any of
the following:
- A function, struct, enum variant, trait method, or module
  (Rust / Python / TS).
- A schema field or JSON / YAML registry key.
- A skill file (rename or retirement).
- A CLI command or subcommand name.
- A sensor domain key.

**Subagent briefing template:**

```
Hat: adversary — Symbol Impact Audit
Research: find ALL callers/references of '{symbol}' in the codebase before
          it is renamed/removed.
Symbol type: {rust function | schema field | skill | cli subcommand | sensor domain}
Tool: `grep -rn <symbol>` across source; `cargo check` after edits to
      surface hard refs.
Calibration: lean toward false positives — a missed caller means a silent
             runtime break.
```

**Adversary questions per symbol type:**

| Symbol | What to verify |
|--------|---------------|
| **Skill rename/retire** | Cross-refs in `See Also:` sections of other skills? `CLAUDE.md` Skills Index entries? `Governs:` fields pointing to old path? |
| **Rust function / export rename** | `use` statements in other crates? Caller `fn` bodies (grep)? `impl` blocks pulling the renamed trait method? Doc tests in rustdoc? `cargo check` catches most hard breaks immediately. |
| **Schema field rename** | Rust struct `#[serde(rename = ...)]` and Python pydantic/dataclass field names? All registry / CMDB files on disk that still use the old key? Schema validation errors after the change? |
| **Sensor domain key rename** | `domain_definitions` in every `brain-registry.json`? CMDB files named after the old key on disk? Tests asserting the old domain name? |
| **CLI subcommand rename** | Skills referencing the old subcommand? `CLAUDE.md` Run Tests section? README command examples? CI workflow invocations? |

**When NOT to spawn:** the plan only adds new symbols (functions,
skills, gates) without removing or renaming existing ones — new
symbols have no existing callers to break.

---

## Step 2a — Scaled Review (Optional Variant)

Use this variant when the plan introduces **≥ 3 independent
subsystems**, each with its own schema, test surface, and
conventions, and cross-domain interaction effects are the primary
risk.

**When to use:**
- Plan spans multiple structured corpora (e.g., skills + gates +
  workflows + topology).
- Each domain has a distinct file type and validation convention.
- Cross-domain calls (hook in Domain A calling a script in
  Domain B) are a key risk.

**When NOT to use:**
- Domains are structurally independent (no shared files, no
  cross-references).
- Use standard per-concern subagents (from the skill's Step 2)
  instead.

**Domain subagent template:**

```
Hat: adversary — Domain review: {domain name}
Scope: only the {domain-specific changes} in this plan
Research: what could fail specifically within this domain's
          conventions, schema, or test surface?
Calibration: lean toward edge cases — do NOT review cross-domain
             interactions (pilot agent's job).
Key schema/conventions: {paste relevant spec excerpt}
```

**Pilot agent synthesis:** after domain subagents report, the
pilot agent reviews cross-domain interactions exclusively — does
a new hook in Domain A call a script in Domain B that Domain B's
subagent flagged as broken? Is there a circular dependency? Does
a gate in Domain C watch a path that Domain D's changes
invalidate?

This step is the pilot agent's exclusive responsibility — domain
subagents must not attempt it.

---

## Step 3 — Tone Rules (Full)

Beyond the severity tier framework in the skill body:

- **Start with strengths.** Find the genuine good in the plan —
  don't fabricate praise, but if the plan is well-structured,
  thorough, or catches its own rollback risks, say so.
- **Frame concerns as forward-looking:** "what could go wrong
  if..." rather than "you forgot...".
- **Be proportionate.** A style inconsistency is 🔵, not 🔴.
  Reserve 🔴 for real breakage.
- **Every review must contain at least one 🟢.** If the plan is
  genuinely problematic, you can still praise the effort to
  document before implementing.

---

## Worked Example: Language-Version Compatibility Concern

```
🔴 [Blocking] The new fn uses `let Some(x) = get()? else { return Err(...) };`
— this is `let else` syntax stabilized in Rust 1.65. The workspace's
rust-toolchain pin is 1.64, so CI fails at parse time. Either bump the
toolchain pin (with justification), or rewrite as
`let x = match get()? { Some(x) => x, None => return Err(...) };`.
```

The finding cites (1) the specific syntax feature, (2) the stabilization
version, (3) the pinned toolchain version, (4) the concrete failure mode,
and (5) two remediations. That is the shape of a good 🔴 finding — specific
enough that fixing it is mechanical.

---

## Cost of Skipping

The LSP deep integration plan skipped the critic. In retrospect
it would have caught:
1. The grep detection false-positive discrimination problem.
2. The pyright CLI go-to-definition gap.

Both required mid-implementation course corrections. The critic's
job is to surface these before code is written, not after.

---

## Research Category Briefing (Expansions)

For each category in the skill's Research Targets table, here are
fuller framings the adversary subagent might receive:

- **Language-version compatibility:** "The workspace pins Rust
  1.64 / Python 3.9 / TypeScript 4.7. Scan the plan for any
  syntax, stdlib feature, or crate API that requires a newer
  version. Every match is a hard breakage."
- **Test coverage:** "For each behavioral change, name the
  specific test (unit, integration, or smoke) that would have
  caught it failing. Gaps are findings."
- **Idempotency:** "Every state-modifying step must be safe to
  retry. Identify any step whose second run would error or have
  a different side effect than the first."
- **Ordering:** "Walk the plan's steps as a DAG. Every edge
  (step N depends on step M) must have M before N. Any reverse
  dependency is a finding."
- **Scope safety:** "Identify every environment touched.
  Production, peer environments, shared infrastructure — is the
  plan scoped to a single target, or could a misapplication
  spread?"
- **Rollback path:** "For every step, how do you undo it if step
  N+1 fails? If there's no rollback, the plan must either own
  the risk explicitly or add a rollback step."
- **Secret safety:** "Any new secret handling — does the secret
  appear in a URL parameter, log line, error message, or
  committed file? Grep the plan for patterns that would cause
  that."
- **Destroy guard:** "Any destructive operation — is there a
  `--dry-run`, `--confirm`, or explicit operator gate before
  execution? Missing guards are 🔴."

---

## See Also

- `.claude/skills/plan-critic.md` — the decision surface (the
  skill body that points here).
- `.claude/skills/hats.md` — full hat system, subagent briefing
  format, adversary checklist.
- `.claude/skills/review-loop.md` — iterative T+P+Code Reviewer
  loop for plans involving skill or code authoring.
- `.claude/skills/dual-review.md` — T+P review protocol for
  skill/infrastructure quality review.
- `.claude/skills/weigh-time-risk.md` — risk/time tradeoff before
  deploy decisions.
- `.claude/skills/preflight.md` — 8-item readiness checklist
  before any `apply`.
