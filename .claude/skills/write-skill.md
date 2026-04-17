# Write a Skill

Use this skill when creating a new skill file or substantially revising an existing one.
Writing a well-structured skill ensures future agents (and humans) can reliably trigger,
understand, and follow the guidance it contains.

Role: meta

Trigger phrases: "write a skill", "create a skill", "add a skill", "new skill for",
Methodology-step: skills
"document this as a skill", "how do I write a skill"

---

## What Makes a Good Skill

A skill is a **self-contained, actionable reference document.** It should answer:
- *When should I use this?* (trigger phrases, situations)
- *What do I do?* (step-by-step with exact commands)
- *How do I know it worked?* (verification, expected output)
- *What can go wrong?* (troubleshooting, edge cases)

A skill is NOT a tutorial, a design doc, or general reference. If it can't be acted on
immediately, it's probably better as a comment in the relevant script.

---

## Required Sections

Every skill must have all of these:

### 1. Title (H1)

Short, imperative verb phrase. Matches what someone would search for.
```
# Debug Cloud Run        ✓
# Cloud Run Debugging    ✗ (noun phrase)
# How to Debug Cloud Run ✗ (too wordy)
```

### 2. Role Tag

A single `Role:` line placed immediately after the opening description, before trigger phrases.
Declares the skill's primary function so agents know what kind of help they're getting.

```markdown
Role: operational
```

Or with two roles (max two, separated by ` · `):
```markdown
Role: diagnostic · planning
```

**Role taxonomy:**

| Role | Meaning | Example skills |
|------|---------|----------------|
| `philosophy` | Platform-agnostic principles, the "why" | `devops-philosophy.md` |
| `teaching` | Bridges knowledge gaps for devs new to ops | `devops-for-developers.md` |
| `operational` | Step-by-step procedures, "run this now" | `apply-infra.md`, `docker-builds.md` |
| `diagnostic` | Detecting/reading/understanding current state | `debug-cloud-run.md`, `gate-status.md` |
| `recovery` | Restoring things when broken | `incident-response.md`, `rollback-deployment.md` |
| `planning` | Deciding what to do before acting | `weigh-time-risk.md`, `preflight.md` |
| `validation` | Verifying correctness of code or infra | `smoke-infra.md`, `playwright-e2e.md` |
| `reference` | Lookup tables, topology snapshots, inventories | `ci-workflows.md`, `network-topology.md` |
| `configuration` | Setting up resources, env, auth, tooling | `setup.md`, `local-dev.md` |
| `ci-cd` | Pipeline automation and management | `ci-testing.md` |
| `meta` | Skills about the skill system itself | `write-skill.md`, `skill-index.md` |

**Why roles matter:** When an agent needs to "roll back a broken deploy", it should immediately
know it needs a `recovery` skill, not a `planning` one. Roles let agents filter by intent before
reading the full file, and let humans scan `skill-index.md` by purpose rather than alphabetically.

**`Governs:` field — required for action roles:** Skills with role `operational`, `validation`,
`diagnostic`, or `recovery` must include a `Governs:` field listing the scripts or config files
they govern (comma-separated, one line). This enables `skill-context-on-read.sh` to surface live
gate state when the skill is read. Skills with `reference`, `meta`, `planning`, `teaching`,
`philosophy`, `configuration`, or `ci-cd` roles that don't govern specific files should omit it.

```markdown
Governs: scripts/verify/smoke-infra.ps1
```

Or for multiple files:
```markdown
Governs: scripts/deploy/apply-foundation.ps1, scripts/deploy/apply-infra.ps1
```

### 4. Trigger Phrases

A comma-separated list of natural-language phrases an agent or human might say.
These are used by agents to decide which skill to read.

```markdown
Trigger phrases: "debug cloud run", "service is down", "container crashed",
"cold start", "OOM", "revision failed"
```

Include: common abbreviations, error message fragments, task descriptions, casual phrasings.
Aim for 4–8 phrases. More is better than fewer.

### 5. At Least One Code Block

Every skill needs at least one concrete, runnable command. No skill is useful if it only
describes concepts without showing the exact command to run.

```bash
# This is the minimum bar — one complete, copy-pasteable command
gcloud run services list --project=laas-489115 --region=us-central1
```

### 6. At Least Three H2 Sections (`##`)

Skills need navigable structure. Flat walls of text are hard to scan under pressure.
Minimum structure: **Overview**, **Steps** (or numbered steps), **Troubleshooting** (or Tips).

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

Every operational skill **must** include either a `## Why This Matters` section OR a reference
to `devops-philosophy.md` that explains *why* the practice exists — not just *how* to do it.

**Why this is required:** Skills that only explain "how" become obsolete the moment the platform
changes (GCP → AWS, Terraform → Pulumi). Skills that explain the underlying principle survive
migrations because they give agents and humans the reasoning to adapt the "how" to a new context.

Keep the section to 1–3 sentences. Link back to the relevant principle by name.

```markdown
## Why This Matters

This skill implements the **[Principle Name]** principle from `devops-philosophy.md`:
[one sentence on what the principle says]. [one sentence on why the specific steps in this
skill exist — what failure mode they prevent].
```

Example (for `apply-infra.md`):

```markdown
## Why This Matters

This skill implements **GitOps / Single Source of Truth** and **Fail Fast / Shift Left**
from `devops-philosophy.md`. The plan → review → apply sequence ensures the repository
state is what gets deployed, not ad-hoc console changes. The pre-apply gate checks exist
because failures found before apply are cheaper than failures found after.
```

Use `philosophy-index.md` to find which principle applies to a given skill area.

**Exempt skills** (do not need this section — they ARE the philosophy layer):
- `devops-philosophy.md`, `philosophy-index.md`, `devops-for-developers.md`
- Meta-skills: `skill-index.md`, `write-skill.md`, `skill-chain.md`, `skill-gap-tracker.md`,
  `skill-deprecation.md`, `demo.md`, `session-handoff.md`, `session-recap.md`

---

## Style Conventions

**Cross-referencing other skills:**
```markdown
See `rollback-deployment.md` for the full rollback procedure.
Read `gate-status.md` first if you haven't set up gates yet.
```
Use backtick filename format. Do NOT use markdown links — filenames are enough.

**Commands:**
- Use `bash` or `powershell` code fences (not generic ` ``` `)
- Always include the full command, not just the flags
- Add comments explaining non-obvious flags
- For PowerShell, use `pwsh -File scripts/...` format (not relative paths)

**Variables in commands:**
```bash
gcloud run services describe <service-name> --project=laas-489115
# Use <angle-brackets> for user-supplied values
# Use $VARIABLE for env vars
```

**Avoid:**
- Passive voice ("this can be done by...") → use imperative ("run:")
- Vague timing ("eventually", "after a while") → use specific signals ("wait for `Ready: True`")
- Repeating content that's already in another skill → cross-reference instead
- Hardcoding sandbox-specific values without noting they vary per user

---

## Length Guidelines

| Skill type | Target length |
|-----------|--------------|
| Simple utility (single task) | 100–300 lines |
| Multi-step workflow | 200–500 lines |
| Reference (many commands) | 300–600 lines |
| Meta-skill (this document) | 200–400 lines |

Longer is only better if the extra content is actionable. Cut anything that is "good to know"
but doesn't change what someone does.

---

## Template

```markdown
# <Verb Phrase Title>

One-sentence description of what this skill covers and when to use it.

Role: <role-tag>
Governs: <comma-separated paths>   ← required for operational/validation/diagnostic/recovery
Domain: <brain-domain(s)>          ← optional; comma-separated Brain domain names
Methodology-step: skills           ← always "skills" for skill files

Trigger phrases: "phrase one", "phrase two", "phrase three",
"phrase four", "phrase five"

---

## Overview (optional — skip for simple skills)

Brief context: why this task exists, what it accomplishes, what it does NOT cover.

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

This skill implements **[Principle from devops-philosophy.md]**.
[Why this practice exists — what failure mode it prevents, why a new platform would still
need an equivalent approach.]

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

After drafting the skill content, ask these four questions to determine whether a companion
hook should be proposed:

1. **Enforcement:** Does this skill describe a step that must happen before another action
   can be safely taken? Could it be skipped by accident?
2. **Detection:** Does this skill describe state that should be automatically surfaced
   rather than requiring manual inspection?
3. **Verification:** Does this skill describe a verification step that could run automatically
   after the triggering action completes?
4. **Automation:** Does this skill trigger downstream work that currently requires a
   separate manual invocation?

**If any answer is "yes":** Add a proposed pair entry to `skill-hook-pairs.md`.

**If all answers are "no":** Add this note to the skill's `## See Also` section:
```
No companion hook needed (evaluated YYYY-MM-DD).
```

`assess-skill-on-edit.sh` check 10 will emit an advisory if your skill has an operational/
diagnostic/recovery/validation role but no entry in `skill-hook-pairs.md`.

---

## Wiring a New Skill

After writing the skill file, complete these three steps:

### 1. Add to `CLAUDE.md` skills index

```markdown
| <task description> | `<new-skill>.md` |
```

### 2. Add to `skill-index.md`

Add an entry to the appropriate category section in `skill-index.md`.

### 3. Add a gate (if the skill covers a testable operation)

If the skill describes a task that can be gated (e.g., an e2e test, a drift check, a Pester
suite), add a corresponding entry to `.claude/test-gates.json`.

---

## Troubleshooting

**Problem: `assess-skill-on-edit.sh` reports broken cross-references for template placeholder names**
- Cause: The checker scans for all backtick-wrapped `.md` filenames, including those inside
  code fences used as template examples.
- Fix: Use angle-bracket notation for placeholder filenames in templates and examples:
  `<other-skill>.md` instead of the backtick-wrapped form. The checker fires on any
  backtick-wrapped word-word.md pattern (any lowercase-hyphenated name), including template placeholders.

**Problem: Skill is too long and hard to navigate**
- Cause: Combined multiple procedures into one skill, or included background that belongs elsewhere.
- Fix: Split into separate skills if two distinct tasks can each stand alone. Cross-reference
  between them rather than repeating content. Aim for one skill = one task.

**Problem: Trigger phrases overlap with another skill**
- Cause: Two skills cover adjacent topics with similar natural language.
- Fix: Make trigger phrases more specific — include concrete nouns (service names, error codes,
  flag names) rather than generic verbs. Add a disambiguation note at the top of both skills:
  > "If you want X, see <other-skill>.md. This skill covers Y only."

**Problem: Skill passes structural checks but agents still pick the wrong skill**
- Cause: Trigger phrases aren't specific enough, or the skill title doesn't match what agents
  search for.
- Fix: Run Scenario 6 from `demo.md` (fuzzy skill search) against your trigger phrase to see
  which skill an agent would actually choose. Add the natural-language phrase that failed to
  the trigger phrases list.

---

## Quality Checklist (run before saving)

- [ ] Title is an imperative verb phrase
- [ ] `Role:` tag present (1-2 roles from the taxonomy in `write-skill.md`)
- [ ] Trigger phrases section present with ≥ 4 phrases
- [ ] At least one runnable code block
- [ ] At least 3 H2 sections
- [ ] All cross-referenced skill files actually exist (run `assess-skill-on-edit.sh` to verify)
- [ ] Commands are complete and copy-pasteable (no `...` placeholders)
- [ ] Troubleshooting section covers at least 2 failure patterns
- [ ] Length is appropriate (not a stub, not a dissertation)
- [ ] **`## Why This Matters` section present** (or explicit philosophy reference) — unless skill is exempt
- [ ] `Domain:` tag present for operational/validation/diagnostic/recovery roles (optional for meta/teaching/reference)
- [ ] `Methodology-step: skills` present
- [ ] Companion hook evaluated (`skill-hook-pairs.md` updated or "no hook needed" note added)
- [ ] Added to `CLAUDE.md` skills index
- [ ] Added to `skill-index.md`
