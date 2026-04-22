---
name: skill-deprecation
description: >-
  A skill is stale, superseded, or describes a workflow that no longer
  exists — and you need to retire or update it safely without breaking
  cross-references or misleading future agents. This skill covers the
  decision tree (update vs deprecate vs delete), the redirect-block
  convention for deprecated-in-place skills, the cross-reference sweep, and
  the archival move. Reach for this when you encounter an out-of-date
  skill, when a review identifies outdated guidance, or when a platform
  change invalidates existing procedure docs.
when_to_use: >-
  "skill is outdated", "retire a skill", "deprecate skill", "skill is
  wrong", "remove a skill", "stale skill", "this skill is no longer
  accurate"
---

# Deprecate or Retire a Skill

**When to use this skill:** A skill is stale, superseded, or describes a
workflow that no longer exists — and you need to retire or update it safely
without breaking cross-references or misleading future agents. This skill
covers the decision tree (update vs deprecate vs delete), the redirect-block
convention for deprecated-in-place skills, the cross-reference sweep, and the
archival move. Reach for this when you encounter an out-of-date skill, when a
review identifies outdated guidance, or when a platform change invalidates
existing procedure docs.

Role: meta

Trigger phrases: "skill is outdated", "retire a skill", "deprecate skill", "skill is wrong",
Methodology-step: skills
"remove a skill", "stale skill", "this skill is no longer accurate"

---

## When to Deprecate vs Update vs Delete

| Situation | Action |
|-----------|--------|
| Skill describes a workflow that has changed but the task still exists | **Update** the skill in place |
| Skill describes a task that no longer exists in the platform | **Delete** after removing cross-references |
| Skill is mostly correct but has one stale section | **Update** that section only |
| Skill has been superseded by a better skill covering the same topic | **Deprecate** with redirect notice |
| Skill was never completed (stub) | **Complete** it or **delete** if the task is no longer relevant |

The bias should be toward **updating** rather than deleting. A wrong skill is worse than
a missing skill — it actively misleads agents. An outdated skill that has a prominent
"STALE — see X instead" notice is better than a silent broken reference.

---

## Signs a Skill Is Stale

- Commands reference files that no longer exist
- Workflow steps contradict what `CLAUDE.md` or other skills describe
- The trigger phrases match tasks that no longer exist
- Cross-references point to other skills that have been renamed or deleted
- Code blocks contain hardcoded values that have changed (project IDs, region names, service names)
- The skill was written before a major refactor and hasn't been updated

**Quick staleness check for any skill:**
```bash
# Find all skill cross-references and verify the files exist
grep -h "\.md\`" .claude/skills/*.md | \
  grep -oE '[a-z-]+\.md' | sort -u | \
  while read f; do
    [ -f ".claude/skills/$f" ] || echo "BROKEN REF: $f"
  done
```

---

## Step 1 — Find All Cross-References

Before modifying or deleting a skill, find everything that links to it:

```bash
SKILL="old-skill-name.md"

# Other skills that reference it
grep -rl "$SKILL" .claude/skills/

# CLAUDE.md skills index entry
grep "$SKILL" CLAUDE.md

# archived/skill-index.md entry
grep "$SKILL" .claude/skills/skill-index.md

# archived/skill-chain.md references
grep "$SKILL" .claude/skills/skill-chain.md
```

---

## Step 2 — Update Cross-References First

For every file found in step 1, update the reference BEFORE modifying the skill file.
This prevents a window where links point to a nonexistent or renamed file.

```bash
# Rename: update all references from old to new name
OLD="old-skill-name.md"
NEW="new-skill-name.md"

# Preview changes
grep -rl "$OLD" .claude/skills/ CLAUDE.md | xargs grep -l "$OLD"

# Apply (PowerShell)
Get-ChildItem -Recurse -Include "*.md" | ForEach-Object {
    (Get-Content $_.FullName) -replace $OLD, $NEW | Set-Content $_.FullName
}
```

---

## Step 3a — Update in Place (most common)

If the task still exists but the workflow changed:

1. Read the existing skill carefully
2. Update the stale sections (commands, file paths, workflow steps)
3. Add a comment at the top if there's history context worth preserving:
   ```markdown
   > **Updated 2026-04-06**: Reflects new `detect-changes` job in deploy-dev.yml.
   > Prior to this, all jobs ran unconditionally.
   ```
4. Run the quality checklist from `write-skill/SKILL.md`
5. Commit with a clear message: `docs: update ci-workflows.md for detect-changes DAG`

---

## Step 3b — Deprecate with Redirect

If the skill has been superseded by a better one covering the same topic:

Add this block at the top of the deprecated skill:

```markdown
> **DEPRECATED — see <replacement-skill>.md instead.**
> This skill describes the pre-2026-04 workflow. The guidance below is preserved
> for historical reference but should not be followed for new work.
```

Do NOT delete the file yet — leave it in place so old links still resolve. Schedule deletion
for the next major cleanup pass.

---

## Step 3c — Delete

If the task no longer exists or the skill is purely harmful (wrong commands that could
cause damage):

```bash
# 1. Verify all cross-references have been updated (step 2)
# 2. Remove from CLAUDE.md skills index
# 3. Remove from archived/skill-index.md
# 4. Remove from archived/skill-chain.md if referenced
# 5. Remove from archived/skill-gap-tracker.md if tracked there
# 6. Delete the file
rm .claude/skills/old-skill-name.md

# 7. Commit (stage explicitly — avoid git add -A)
git add CLAUDE.md .claude/skills/skill-index.md .claude/skills/skill-chain.md .claude/skills/skill-gap-tracker.md
git rm .claude/skills/old-skill-name.md
git commit -m "docs: remove deprecated old-skill-name.md — task no longer exists"
```

---

## Step 4 — Post-Deprecation Check

After updating/deleting a skill, run the reference check to confirm no broken links remain:

```bash
grep -h "\.md\`" .claude/skills/*.md CLAUDE.md | \
  grep -oE '`[a-z-]+\.md`' | sort -u | \
  while read ref; do
    fname=$(echo $ref | tr -d '`')
    [ -f ".claude/skills/$fname" ] || echo "BROKEN: $ref"
  done
```

---

## Troubleshooting

**Problem: `assess-skill-on-edit.sh` reports a broken cross-reference in a deprecation notice**
- Cause: The `> **DEPRECATED — see <name>.md instead.**` block uses backtick-wrapped `.md` names
  that the checker treats as real references.
- Fix: Use angle-bracket notation (`<replacement-skill>.md`) in deprecation notices rather than
  backtick-wrapped filenames. This is the pattern used in this skill's Step 3b template.

**Problem: Can't tell if a skill is stale or just a snapshot**
- Cause: Some skills document a state-of-the-world (topology, architecture decisions) rather than
  a repeatable procedure. These are expected to go stale faster.
- Fix: Add a `*Last reviewed:*` timestamp at the bottom. Use the staleness check script to confirm
  cross-references are still valid even if content needs updating.

**Problem: Deleting a skill breaks another skill's cross-reference**
- Cause: Step 2 (find references) wasn't run before deletion, or a new reference was added after.
- Fix: Re-run Step 4 (post-deprecation check) immediately after deletion. If you find broken refs,
  update the referencing skills to remove or redirect the link.

**Problem: The DEPRECATED marker triggers a warning on every edit**
- Expected behavior: `assess-skill-on-edit.sh` emits `⚠ Skill is marked DEPRECATED` on any edit.
- This is intentional — it's a reminder to complete the deprecation process rather than editing
  a skill that should be replaced. If you're doing the final update before deletion, proceed.

---

## Skill Freshness Policy

| Skill type | Review frequency |
|-----------|-----------------|
| Deployment / CI skills | After any significant CI or terraform refactor |
| Operation skills (debug, rollback) | After any GCP API changes or service renames |
| Meta-skills (this file, write-skill/SKILL.md) | After any change to the skills system itself |
| Reference skills (environments.md, setup.md) | Quarterly |

When a skill is updated, note the date at the bottom:
```markdown
---
*Last reviewed: 2026-04-06 — verified against deploy-dev.yml detect-changes refactor.*
```
