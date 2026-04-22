---
name: sync-ecosystem
description: >-
  Someone needs to know whether the ecosystem Brain's view of its children
  is still accurate. NeuroGrim or LSP-Brains may have added new domains,
  new schemas, new spec sections, or new skills that the ecosystem registry
  doesn't yet know about. Also useful before any ecosystem-level reporting
  or trajectory computation — no point scoring against an outdated child
  inventory.
when_to_use: >-
  You want to check whether the ecosystem Brain's view of its children is
  still accurate — before ecosystem-level reporting, before trajectory
  computation, or after NeuroGrim/LSP-Brains adds new domains, schemas,
  spec sections, or skills. Trigger phrases — "sync ecosystem", "check
  children drift", "ecosystem registry stale", "refresh ecosystem view".
---

# Skill: Sync Ecosystem

**When to use this skill:** Someone needs to know whether the ecosystem Brain's view of
its children is still accurate. NeuroGrim or LSP-Brains may have added new domains,
new schemas, new spec sections, or new skills that the ecosystem registry doesn't yet
know about. Also useful before any ecosystem-level reporting or trajectory computation —
no point scoring against an outdated child inventory.

The skill flags drift. It does NOT auto-update the ecosystem registry. Governance
concern, not automation — humans decide what to adopt.

## What "drift" means here

Three categories of potentially-interesting change:

1. **Capability drift** — a child Brain added or removed a domain, sensory tool, or
   registered MCP/A2A server since the ecosystem registry was last reviewed.
2. **Schema drift** — the LSP-Brains repo bumped a schema version (agent-output,
   brain-registry, cmdb-envelope, a2a-envelope, agent-card, culture-manifest) and the
   ecosystem hasn't noted the new version.
3. **Culture drift** — the four `culture.yaml` copies (ecosystem + 3 children) are no
   longer byte-identical. The `culture-coherence` sensor now catches this
   automatically (score 100 when aligned); this step remains as a fallback checklist
   for operators working without the sensor.

## How to run (manual v1)

Until a sensory tool exists, this skill is a checklist for a human or subagent to walk.

### Step 1 — NeuroGrim capability check

1. Read `D:\Brains\NeuroGrim\.claude\brain-registry.json`
2. Compare its `domain_weights` keys to what the ecosystem registry's NeuroGrim child
   entry expects (check `interface_version`)
3. Read `D:\Brains\NeuroGrim\neurogrim\crates\neurogrim-sensory\src\` — has a new sensory module landed?
4. Read `D:\Brains\NeuroGrim\roadmap\ROADMAP.md` — have
   S6-DB-* stories shipped that add new capabilities?

Flag: any new sensory tool, any new domain, any Stage 6 story that moves from
"Not started" to "Complete."

### Step 2 — LSP-Brains capability check

1. Read `D:\Brains\LSP-Brains\.claude\brain-registry.json` — same check
   as above
2. Read `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` header — has the
   version bumped since the ecosystem last recorded it?
3. Read `D:\Brains\LSP-Brains\schemas\` — any new `*.schema.json` files?
4. Read `D:\Brains\LSP-Brains\spec\METHODOLOGY-EVOLUTION.md` — any new
   sections?

Flag: new schemas, spec version bumps, new METH-EV sections.

### Step 3 — Culture coherence check

Run this as the most concrete step:

Preferred: run the sensor, which SHA-256s all four copies and writes a CMDB at
`.claude/culture-coherence-cmdb.json`:

```bash
py -3 D:/Brains/sensory/check_culture_coherence.py
```

Score 100 = all four identical; anything less names the diverging paths.

Fallback (no sensor available): pairwise diffs.

```bash
diff D:/Brains/.claude/culture.yaml \
     D:/Brains/NeuroGrim/.claude/culture.yaml
diff D:/Brains/.claude/culture.yaml \
     D:/Brains/LSP-Brains/.claude/culture.yaml
diff D:/Brains/.claude/culture.yaml \
     D:/Brains/NeuroGrim/NeuroGrim-python-starter/.claude/culture.yaml
```

All diffs MUST be empty. If any is non-empty, either:

- (a) Someone edited one copy and forgot the others — propagate and recommit
- (b) A child project has a genuine reason for local extension — in which case culture
      `version` should change, and we should discuss whether the ecosystem's copy
      follows or stays

Option (b) is rare; option (a) is the common case. The skill should name the diff and
let the human decide.

### Step 4 — Report

Produce a concise report in this format:

```
Ecosystem sync report — YYYY-MM-DD HH:MM:SS UTC

NeuroGrim drift:     [none | list]
LSP-Brains drift:      [none | list]
Culture coherence:     [identical | diverged at <file>, see diff]

Recommended adoptions:
  - [specific item, with pointer to where to update the ecosystem registry]

Decisions needed from human:
  - [any flagged change that requires judgment, not mechanical adoption]
```

Keep it short. Cultural substrate applies: honest (flag real drift), critical-but-kind
(don't scold the human for forgetting to sync), respectful (decisions are theirs).

## Anti-patterns

- **Don't auto-update the ecosystem registry.** The whole point is human governance. If
  this skill silently rewrites `D:\Brains\.claude\brain-registry.json`, the governance
  is lost.
- **Don't normalize whitespace in the culture diff.** Byte-identity is the measurement.
  Trailing whitespace matters for this check.
- **Don't sync on a schedule.** This skill runs on demand (or as a pre-flight before a
  cross-project operation), not on a cron. Scheduled sync creates false urgency.

## Troubleshooting

### `git submodule status` shows `-` prefix after a rename

If an earlier session renamed a submodule (e.g. `Moth-er-Br-AI-n` → `NeuroGrim`),
the ecosystem's `.git/config` can retain the old `[submodule "…"]` section name
alongside the new one. `git submodule status` then shows the new path with a `-`
prefix (uninitialized) even though the working tree is populated and pushes work.

Symptom:

```
 5f726cec… LSP-Brains (heads/main)
-7fbd08fb… NeuroGrim
```

Repair:

```bash
cd D:/Brains
git config --file .git/config --remove-section submodule.<old-name>
git config --file .git/config submodule.<new-name>.url <remote-url>
git config --file .git/config submodule.<new-name>.active true
git submodule sync
git submodule status   # should show no leading `-` now
```

This is metadata-only; no working-tree or commit-history changes.

## Future work (when a sensory tool lands)

A proper `sync-ecosystem` sensory tool would produce a CMDB at
`.claude/sync-ecosystem-cmdb.json` scoring the drift: 100 if everything is aligned,
dropping as divergence accumulates. Would feed into the `culture-coherence`,
`terminology-coherence`, and `spec-impl-alignment` domains. Tracked as a follow-on to
S6-DB-7.

## Related skills

- `rubber-duck/SKILL.md` — use if you're unsure whether a flagged drift item should be adopted
- `archived/brain.md` (in NeuroGrim repo) — operational guide for the NeuroGrim child
- Future `peer-archived/brain.md` — running an A2A peer (Stage 6 S6-DB-3)

## Related reading

- `D:\Brains\.claude\brain-registry.json` — the ecosystem registry this skill diffs against
- `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` §14 — Cultural Substrate
- `D:\Brains\NeuroGrim\roadmap\ROADMAP.md` — ecosystem stories (S6-DB-7)
