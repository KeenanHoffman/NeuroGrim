# Write a Skill — Full Guide

Deep reference for the `write-skill.md` skill. The skill itself carries
the decision surface (what a good skill is, the lead-paragraph
contract, required sections list, length targets, quality checklist).
This guide carries the depth: role taxonomy with examples, Governs
field, the full template, style conventions, the Why-This-Matters
requirement, companion hook rubric, wiring steps, and troubleshooting.

Read the skill first. Come here when you need the role taxonomy,
the template, or the philosophy behind a specific rule.

---

## Role Taxonomy (complete table)

| Role | Meaning | Example skills |
|------|---------|----------------|
| `philosophy` | Platform-agnostic principles, the "why" | `archived/devops-philosophy.md` |
| `teaching` | Bridges knowledge gaps for devs new to ops | `archived/devops-for-developers.md` |
| `operational` | Step-by-step procedures, "run this now" | `apply-infra.md`, `docker-builds.md` |
| `diagnostic` | Detecting/reading/understanding current state | `debug-cloud-run.md`, `archived/gate-status.md` |
| `recovery` | Restoring things when broken | `incident-response.md`, `rollback-deployment.md` |
| `planning` | Deciding what to do before acting | `weigh-time-risk.md`, `preflight.md` |
| `validation` | Verifying correctness of code or infra | `smoke-infra.md`, `playwright-e2e.md` |
| `reference` | Lookup tables, topology snapshots, inventories | `ci-workflows.md`, `network-topology.md` |
| `configuration` | Setting up resources, env, auth, tooling | `setup.md`, `local-dev.md` |
| `ci-cd` | Pipeline automation and management | `ci-testing.md` |
| `meta` | Skills about the skill system itself | `write-skill.md`, `archived/skill-index.md` |

**Role declaration syntax.** A single `Role:` line placed immediately
after the opening description, before trigger phrases:

```markdown
Role: operational
```

Or with two roles (max two, separated by ` · `):
```markdown
Role: diagnostic · planning
```

**Why roles matter.** When an agent needs to "roll back a broken
deploy", it should immediately know it needs a `recovery` skill,
not a `planning` one. Roles let agents filter by intent before
reading the full file, and let humans scan `archived/skill-index.md`
by purpose rather than alphabetically.

---

## The `Governs:` Field — Required for Action Roles

Skills with role `operational`, `validation`, `diagnostic`, or
`recovery` must include a `Governs:` field listing the scripts or
config files they govern (comma-separated, one line). This enables
`skill-context-on-read.sh` to surface live gate state when the
skill is read. Skills with `reference`, `meta`, `planning`,
`teaching`, `philosophy`, `configuration`, or `ci-cd` roles that
don't govern specific files should omit it.

```markdown
Governs: neurogrim/crates/neurogrim-sensory/src/git_health.rs
```

Or for multiple files:
```markdown
Governs: neurogrim-core/src/scoring.rs, neurogrim-core/src/agent_output.rs
```

---

## Trigger Phrases — Deeper Notes

A comma-separated list of natural-language phrases an agent or
human might say. Used by agents to decide which skill to read.

```markdown
Trigger phrases: "debug cloud run", "service is down", "container crashed",
"cold start", "OOM", "revision failed"
```

**Include:**
- Common abbreviations (`OOM`, `CI`, `SLA`).
- Error message fragments (`revision failed`, `container crashed`).
- Task descriptions (`debug cloud run`, `rollback the deploy`).
- Casual phrasings (`service is down`, `something's broken`).

Aim for 4–8 phrases. More is better than fewer — missing phrases
are the failure mode; extra phrases don't cause harm.

---

## H2 Section Structure Requirements

At least three H2 sections (`##`). Skills need navigable structure
— flat walls of text are hard to scan under pressure. Minimum
structure: **Overview**, **Steps** (or numbered steps),
**Troubleshooting** (or Tips).

---

## Optional but Strongly Recommended Sections

| Section | When to include |
|---------|----------------|
| **Quick Reference / Quick Triage** | When there are 3+ common entry points |
| **Decision Table** | When the right action depends on a condition |
| **Step N — [Specific step name]** | When steps are long or have prerequisites |
| **Troubleshooting** | Always — list at least 3 common failure patterns |
| **See Also / Related Skills** | When another skill is a prerequisite or natural next step |

---

## Required: Why This Matters (Philosophy)

Every operational skill **must** include either a `## Why This
Matters` section OR a reference to `archived/devops-philosophy.md`
that explains *why* the practice exists — not just *how* to do it.

**Why this is required.** Skills that only explain "how" become
obsolete the moment the platform changes (GCP → AWS, Terraform →
Pulumi). Skills that explain the underlying principle survive
migrations because they give agents and humans the reasoning to
adapt the "how" to a new context.

Keep the section to 1–3 sentences. Link back to the relevant
principle by name.

```markdown
## Why This Matters

This skill implements the **[Principle Name]** principle from
`archived/devops-philosophy.md`: [one sentence on what the
principle says]. [one sentence on why the specific steps in this
skill exist — what failure mode they prevent].
```

**Example (for `apply-infra.md`):**

```markdown
## Why This Matters

This skill implements **GitOps / Single Source of Truth** and
**Fail Fast / Shift Left** from `archived/devops-philosophy.md`.
The plan → review → apply sequence ensures the repository state
is what gets deployed, not ad-hoc console changes. The pre-apply
gate checks exist because failures found before apply are cheaper
than failures found after.
```

Use `archived/philosophy-index.md` to find which principle
applies to a given skill area.

**Exempt skills** (do not need this section — they ARE the
philosophy layer):
- `archived/devops-philosophy.md`, `archived/philosophy-index.md`,
  `archived/devops-for-developers.md`
- Meta-skills: `archived/skill-index.md`, `write-skill.md`,
  `archived/skill-chain.md`, `archived/skill-gap-tracker.md`,
  `skill-deprecation.md`, `archived/demo.md`,
  `session-handoff.md`, `session-recap.md`

---

## Style Conventions

**Cross-referencing other skills:**
```markdown
See `rollback-deployment.md` for the full rollback procedure.
Read `archived/gate-status.md` first if you haven't set up gates yet.
```
Use backtick filename format. Do NOT use markdown links — filenames
are enough.

**Commands:**
- Use an explicit language tag on code fences (`bash`, `rust`,
  `python`) — not generic ` ``` `.
- Always include the full command, not just the flags.
- Add comments explaining non-obvious flags.

**Variables in commands:**
```bash
neurogrim sensory code-quality --project-root <path-to-project>
# Use <angle-brackets> for user-supplied values
# Use $VARIABLE for env vars
```

**Avoid:**
- Passive voice ("this can be done by...") → use imperative
  ("run:").
- Vague timing ("eventually", "after a while") → use specific
  signals ("wait for `Ready: True`").
- Repeating content that's already in another skill →
  cross-reference instead.
- Hardcoding sandbox-specific values without noting they vary
  per user.

---

## Template

```markdown
# <Verb Phrase Title>

**When to use this skill:** <2-4 sentences: concrete situations,
symptoms, task descriptions, error messages that route here>.

Role: <role-tag>
Governs: <comma-separated paths>   ← required for operational/validation/diagnostic/recovery
Domain: <brain-domain(s)>          ← optional; comma-separated Brain domain names
Methodology-step: skills           ← always "skills" for skill files

Trigger phrases: "phrase one", "phrase two", "phrase three",
"phrase four", "phrase five"

---

## Overview (optional — skip for simple skills)

Brief context: why this task exists, what it accomplishes, what
it does NOT cover.

---

## Quick Reference (optional)

| Situation | Command |
|-----------|---------|
| Most common case | `command here` |

---

## Step 1 — <First Major Step>

Explanation.

\`\`\`bash
actual command here
\`\`\`

Expected output / how to verify it worked.

---

## Step 2 — <Second Major Step>

...

---

## Why This Matters

This skill implements **[Principle from archived/devops-philosophy.md]**.
[Why this practice exists — what failure mode it prevents, why a
new platform would still need an equivalent approach.]

---

## Troubleshooting

**Problem: <symptom>**
- Likely cause: ...
- Fix: `command`

**Problem: <symptom>**
- ...

---

## See Also

- `<other-skill>.md` — for X
- `<prereq-skill>.md` — prerequisite for this skill
```

---

## Companion Hook Consideration

After drafting the skill content, ask these four questions to
determine whether a companion hook should be proposed:

1. **Enforcement:** Does this skill describe a step that must
   happen before another action can be safely taken? Could it be
   skipped by accident?
2. **Detection:** Does this skill describe state that should be
   automatically surfaced rather than requiring manual inspection?
3. **Verification:** Does this skill describe a verification step
   that could run automatically after the triggering action
   completes?
4. **Automation:** Does this skill trigger downstream work that
   currently requires a separate manual invocation?

**If any answer is "yes":** Add a proposed pair entry to
`archived/skill-hook-pairs.md`.

**If all answers are "no":** Add this note to the skill's
`## See Also` section:
```
No companion hook needed (evaluated YYYY-MM-DD).
```

`assess-skill-on-edit.sh` check 10 will emit an advisory if your
skill has an operational/diagnostic/recovery/validation role but
no entry in `archived/skill-hook-pairs.md`.

---

## Wiring a New Skill

After writing the skill file, complete these three steps:

### 1. Add to `CLAUDE.md` skills index

```markdown
| <task description> | `<new-skill>.md` |
```

### 2. Add to `archived/skill-index.md`

Add an entry to the appropriate category section in
`archived/skill-index.md`.

### 3. Add a test (if the skill covers a behavior worth regression-guarding)

If the skill describes a task that can be covered by an automated
test (e.g., a sensor behavioral test, a CLI smoke test, a schema
conformance check), add a corresponding entry in the appropriate
`tests/` tree. See spec §3.8 "Testing Discipline" for the
SHOULD-level expectation on sensory tools specifically.

---

## Troubleshooting

**Problem: `assess-skill-on-edit.sh` reports broken cross-references for template placeholder names**
- Cause: The checker scans for all backtick-wrapped `.md`
  filenames, including those inside code fences used as template
  examples.
- Fix: Use angle-bracket notation for placeholder filenames in
  templates and examples: `<other-skill>.md` instead of the
  backtick-wrapped form. The checker fires on any
  backtick-wrapped word-word.md pattern (any lowercase-hyphenated
  name), including template placeholders.

**Problem: Skill is too long and hard to navigate**
- Cause: Combined multiple procedures into one skill, or included
  background that belongs elsewhere.
- Fix: Split into separate skills if two distinct tasks can each
  stand alone. Cross-reference between them rather than repeating
  content. Aim for one skill = one task.

**Problem: Trigger phrases overlap with another skill**
- Cause: Two skills cover adjacent topics with similar natural
  language.
- Fix: Make trigger phrases more specific — include concrete nouns
  (service names, error codes, flag names) rather than generic
  verbs. Add a disambiguation note at the top of both skills:
  > "If you want X, see <other-skill>.md. This skill covers Y only."

**Problem: Skill passes structural checks but agents still pick the wrong skill**
- Cause: Trigger phrases aren't specific enough, or the skill title
  doesn't match what agents search for.
- Fix: Run Scenario 6 from `archived/demo.md` (fuzzy skill search)
  against your trigger phrase to see which skill an agent would
  actually choose. Add the natural-language phrase that failed to
  the trigger phrases list.

**Problem: Skill body got fat (> 2000 tokens of narrative)**
- Cause: Deep reference accumulated in the skill that should live
  in a companion `docs/` file.
- Fix: Apply the B-13 pattern — extract depth to
  `docs/<skill-name>-guide.md` with a cross-reference; keep the
  skill body as decision surface (when to use + pattern summary +
  minimal reference). `subagent-patterns.md` and this guide are
  the canonical examples of the pattern.

---

## See Also

- `.claude/skills/write-skill.md` — the decision surface (the
  skill body that points here).
- `archived/skill-index.md` — inventory of existing skills.
- `archived/philosophy-index.md` — which philosophy principle
  applies to which skill area.
- `archived/skill-hook-pairs.md` — hook proposals for skills.
- `archived/skill-chain.md` — notation for multi-skill workflows.
