---
name: write-skill
description: Authoring standard for new skills or substantive skill revisions. Ensures the routing contract (plugin `description` + `when_to_use` frontmatter, or legacy lead paragraph) is routing-critical and stays within the 1,536-char Claude Code skill-index budget. Covers role taxonomy, required sections, size targets, the B-13 skill/guide compression pattern, and a pre-save quality checklist — everything `capability-hygiene` domain enforces.
when_to_use: >-
  You are creating a new skill file, substantially revising an
  existing one, or migrating a legacy `.md` skill to the plugin
  `SKILL.md` format. Trigger phrases — "write a skill", "create a
  skill", "add a skill", "new skill for", "document this as a
  skill", "how do I write a skill", "skill authoring", "skill
  format".
---

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

## Skill File Format — Plugin vs Legacy

Claude Code supports two skill file formats; both are visible to the
`capability-hygiene` sensor, but only one is **Skill-tool-invocable**:

| Format | Path | Skill-tool invocable? | Visible in `/` menu? |
|--------|------|-----------------------|----------------------|
| **Plugin** (recommended) | `.claude/skills/<name>/SKILL.md` | ✅ | ✅ |
| Legacy (archival only) | `.claude/skills/<name>.md` | ❌ (Read-only) | ❌ |

**Always author new skills in plugin format.** Legacy skills are
Read-referenced documents — agents can navigate to them, but Claude Code's
skill index does not route to them via the `Skill` tool, and the Axis 4
invocation ledger can't observe their usage.

### Plugin Frontmatter (required)

```yaml
---
name: my-skill               # lowercase, hyphens, matches directory
description: |               # required — what the skill does; routing signal
  <one-to-two-sentence synopsis naming when to use this skill.>
when_to_use: |               # optional but strongly recommended
  <trigger cues, symptoms, user phrasings, or file-path hints.>
---
```

The `description` + `when_to_use` fields are concatenated by Claude Code
and **truncated at 1,536 characters combined** in the skill index. Front-load
the routing signal in `description`; use `when_to_use` to extend with trigger
phrases. Everything past 1,536 chars is invisible to Claude's auto-invocation.

Optional fields worth knowing (full list:
[Claude Code docs § Skills frontmatter](https://code.claude.com/docs/en/skills.md#frontmatter-reference)):

- `allowed-tools` — pre-approve tools without per-use prompting.
- `disable-model-invocation: true` — manual-only skills (like rollback runbooks).
- `paths` — glob patterns; only auto-load when working with matching files.
- `usage-rarity: rare` — skill is deliberately niche (quarterly+); extends the
  `capability-hygiene` dead-skill window from 90 days to 365 days. Use ONLY
  for genuinely rare skills — overclassifying hides real dead capabilities.

### Supporting Files in the Skill Directory

The skill directory is yours — you can ship scripts, templates, or deeper
reference docs alongside `SKILL.md`. Claude Code only discovers `SKILL.md`;
anything else is read on demand when `SKILL.md` points to it.

```
my-skill/
├── SKILL.md           # required entrypoint
├── reference.md       # optional deeper reference
├── templates/         # optional — copy-paste starting points
└── scripts/           # optional — runnable helpers
```

---

## The Description Block — Routing-Critical

**The description block IS the routing contract** — it is what the agent reads
to decide whether to invoke this skill at all. Full bodies load on demand via
the `Skill` tool, but only if the description routes correctly.

**For plugin-format skills**, the description block is `description` +
`when_to_use` from the YAML frontmatter (≤ 1,536 chars combined).

**For legacy-format skills**, the `capability-hygiene` domain extracts the
text between the `# Title` and the first `## ` section header as the
description block. Put your "when to use this skill" signal **there**, not
under a `## When to Use` heading.

**Size targets (enforced by `capability-hygiene` domain):**

| Description length | Meaning |
|---|---|
| < 40 tokens | Under-described. Agent can't route reliably. Flag for rewrite. |
| 40-200 tokens | Sweet spot. Plenty of routing signal, fits within the 1,536-char budget. |
| > 300 tokens | Over-described. Move narrative into the body; keep lead terse. |

**Anti-patterns:**
- Using `## When to Use This Skill` as a section header instead of the lead
  paragraph / frontmatter — the routing extractor never sees content under
  that header.
- Leading with "Purpose:" or "Overview:" or "What this does" — these frame the
  skill statically. Lead with *when to reach for it*.
- Leading with a huge TL;DR code block before any prose. Readers learn *what* to
  run but not *when* to reach for the skill.

**Why this matters:** when the lead paragraph is weak, the agent skips the skill
even if the body is excellent — because the agent never sees the body unless the
description routes them to it. Description quality dominates skill ROI.

---

## Required Sections (compact)

Every skill must have all of these. Deep specs + role taxonomy + Governs
field + Why-This-Matters rules live in the guide.

1. **Frontmatter (plugin) or Title (legacy)** — imperative verb phrase for Title.
2. **Role tag** — one or two roles from the taxonomy (see guide § Role Taxonomy).
3. **`Governs:` field** — REQUIRED for `operational`/`validation`/`diagnostic`/`recovery`
   roles; comma-separated list of governed files.
4. **Trigger phrases** — 4-8 comma-separated natural-language phrases (can live
   in `when_to_use` for plugin skills).
5. **At least one code block** — concrete, runnable, with an explicit language tag.
6. **At least 3 H2 sections** — typically Overview, Steps, Troubleshooting.
7. **`## Why This Matters`** — 1-3 sentences naming the principle. Required for
   operational skills; exempt for meta/philosophy skills (list in guide).

Optional but strongly recommended: Quick Reference, Decision Table,
Step-numbered sections, See Also. See guide § Optional Sections for when each
applies.

---

## Length Guidelines

| Skill type | Target length |
|-----------|--------------|
| Simple utility (single task) | 100–300 lines |
| Multi-step workflow | 200–500 lines |
| Reference (many commands) | 300–600 lines |
| Meta-skill | 200–400 lines |

Longer is only better if the extra content is actionable. Cut anything that is
"good to know" but doesn't change what someone does. If the body exceeds ~2000
tokens of narrative, apply the **B-13 compression pattern**: extract depth to
`docs/<skill-name>-guide.md`, keep the skill body as decision surface. See
`subagent-patterns/SKILL.md` + `docs/subagent-patterns-guide.md` as the canonical
example.

---

## Template

The full skill template lives in the guide: see
`docs/write-skill-guide.md` § Template. Copy-paste from there when
authoring a new skill.

---

## Quality Checklist (run before saving)

- [ ] Plugin directory structure: `.claude/skills/<name>/SKILL.md`
- [ ] Frontmatter: `name` (lowercase, hyphens) + `description` (required)
- [ ] `when_to_use` populated for routing-critical skills
- [ ] Combined `description + when_to_use` ≤ 1,536 chars
- [ ] Title is an imperative verb phrase (if body retains one)
- [ ] `Role:` tag present (1-2 roles from the taxonomy in the guide)
- [ ] Trigger phrases section or `when_to_use` field with ≥ 4 phrases
- [ ] At least one runnable code block
- [ ] At least 3 H2 sections
- [ ] All cross-referenced skill files actually exist
- [ ] Commands are complete and copy-pasteable (no `...` placeholders)
- [ ] Troubleshooting section covers at least 2 failure patterns
- [ ] Length is appropriate (not a stub, not a dissertation — ≤ 2000 tokens of
      narrative; otherwise apply B-13 compression)
- [ ] **`## Why This Matters` section present** (or explicit philosophy
      reference) — unless skill is exempt
- [ ] `Domain:` tag present for operational/validation/diagnostic/recovery roles
- [ ] `Methodology-step: skills` present
- [ ] Companion hook evaluated — see guide § Companion Hook Consideration
- [ ] Added to `CLAUDE.md` skills index
- [ ] Added to `archived/skill-index.md`
- [ ] Lead paragraph / frontmatter passes `capability-hygiene` domain check

---

## See Also

- `docs/write-skill-guide.md` — full reference: role taxonomy, Governs field,
  trigger phrases philosophy, optional sections, Why-This-Matters requirement
  + exempt list, style conventions, full template, companion hook rubric,
  wiring steps, deep troubleshooting.
- `subagent-patterns/SKILL.md` + `docs/subagent-patterns-guide.md` — canonical example
  of the B-13 skill/guide split pattern.
- `skill-deprecation/SKILL.md` — when to retire a skill instead of rewriting it.
