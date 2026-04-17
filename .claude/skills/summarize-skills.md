# Summarize Skills

Produces a category-grouped overview of the full skill library with relationship
annotations. Use when onboarding a new contributor, auditing the library, or answering
"what does this agent know how to do?"

Role: meta · reference

Trigger phrases: "summarize skills", "show me all skills", "skill overview",
Methodology-step: skills
"comprehensive skill breakdown", "what skills exist", "skill library overview",
"map the skills", "skill summary", "categorize the skills", "skill breakdown"

---

## How to Produce a Summary

1. Read `skill-index.md` — the canonical source of all skills, roles, and descriptions
2. Group by the categories defined in `skill-index.md`
3. For each category, render the **Category Table** (see format below)
4. Annotate relationships using the legend
5. Render the **Stats Block** at the top

---

## Output Structure (in order)

```
1. Stats Block
2. Relationship Annotation Legend (reference table)
3. One Category Table per category
4. Cross-Category Skills table
5. Hook Pairs at a Glance table
```

---

## Stats Block Format

Render as a fenced code block at the top of the output:

```
Skills: N total | N active | N deprecated
Roles:  philosophy(N) · teaching(N) · operational(N) · diagnostic(N) · recovery(N)
        planning(N) · validation(N) · reference(N) · configuration(N) · ci-cd(N) · meta(N)
Chains: N chains covering N+ skills  |  Hook pairs: N documented
```

---

## Relationship Annotation Legend

Render as a table immediately after the stats block:

| Symbol | Meaning |
|--------|---------|
| `→` | Feeds into — output of A is direct input to B |
| `⊃` | Prerequisite — must read/run A before B makes sense |
| `↔` | Often paired — commonly used in the same session |
| `⇒` | Supersedes — A replaces B (B is deprecated) |
| `⚡` | Hook pair — a hook fires automatically alongside this skill |
| `∥` | Parallelizable — A and B can run simultaneously |

---

## Category Table Format

Render one table per category. The heading format is:

```
### Category Name *(N skills)*
> One-sentence description of what this category covers.
```

Then the table:

| Skill | Role | Description | Relationships |
|-------|------|-------------|---------------|
| `skill-name.md` | `role · role` | One-line description | `→ other.md`, `⚡ hook.sh` |

**Rules for the Relationships column:**
- List each relationship as `<symbol> target` separated by commas
- Hook pairs use `⚡ hookname.sh`
- Parallelizable partners use `∥ other.md`
- Prerequisite targets use `⊃ other.md`
- Keep the column concise — list the 2–3 most important relationships only
- If a skill has no notable relationships, write `—`

**Rules for the Role column:**
- Use the exact role tag(s) from `skill-index.md`
- Compound roles separated by ` · ` (e.g. `diagnostic · reference`)
- Deprecated skills: append *(deprecated)* after the role

**Reading order note:** After each category table, render a one-line reading order if
one exists in `skill-index.md`:

```
**Reading order:** `a.md` → `b.md` → `c.md`
```

---

## Cross-Category Skills Table

After all category tables, render a single table of skills that appear in more than
one category:

| Skill | Primary Category | Also Relevant In |
|-------|-----------------|-----------------|
| `skill.md` | Category A | Category B |

---

## Hook Pairs at a Glance Table

Render as the final table in the output:

| Skill | Hook | Type |
|-------|------|------|
| `skill.md` or "Any apply/destroy" | `hook-name.sh` | Enforcement / Detection / Verification / Automation |

Source: `skill-hook-pairs.md`. Type definitions: **Enforcement** = blocks on failure (exit 1),
**Detection** = surfaces condition (exit 0), **Verification** = validates after action,
**Automation** = triggers downstream work.

---

## Why This Matters

This skill implements **Observability Before Action** from `devops-philosophy.md`. Before
working in the skill library — adding, retiring, or auditing — you need a clear picture
of what exists and how skills relate to each other. A category-grouped summary with
relationship annotations makes gaps, overlaps, and missing hook pairs visible at a glance.
The Platform Migration Test applies: on any platform, the need to understand a knowledge
library before modifying it survives; only the specific skills change.

---

## Troubleshooting

**Problem: Skill count doesn't match number of `.md` files on disk**
- Run the index health check in `skill-index.md` to find files missing from the index
- A skill on disk but not in the index is an undocumented gap — add to `skill-gap-tracker.md`

**Problem: Relationship annotations seem stale**
- Annotations derive from `skill-chain.md` (sequences) and `skill-index.md`
  (prerequisites + often-paired tables)
- Re-run this skill after any chain update to get current annotations

**Problem: Relationships column overflows — too many entries**
- Cap at 3 relationships per skill; prefer the highest-blast-radius ones
- For skills with many relationships (e.g. `apply-infra.md`), list the `⚡` hook pairs
  last as they are the most discoverable from hooks-reference.md

**Problem: A skill appears in multiple categories**
- List it under its primary category only; record it in the Cross-Category table
- Primary category = first role tag (e.g. `diagnostic · reference` → primary is diagnostic)

---

## See Also

- `skill-index.md` — the canonical source this skill reads from
- `skill-chain.md` — multi-skill sequences (source of `→` and `∥` annotations)
- `skill-gap-tracker.md` — track gaps surfaced by this summary
- `skill-hook-pairs.md` — companion hook catalog (source of `⚡` annotations)
- `dual-review.md` — use after writing a new skill to validate it

---

## No companion hook needed

Evaluated 2026-04-06. This skill produces read-only narrative output — no file writes,
no state changes. A hook would have no actionable trigger.
