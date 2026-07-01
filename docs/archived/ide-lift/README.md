---
doc-version: 1.0
date: 2026-06-30
status: superseded
anchored-to: none
front-door: false
---

# Archived — IDE-LIFT design docs

These documents describe the **IDE broker-migration ("IDE lift") effort** — the
plan to migrate the NeuroGrim IDE's `ide_action` call sites onto the broker
substrate (cluster manifests, per-call-site migration mapping, broker templates,
adversarial review, phase-progress tracking).

## Why they are archived

That work **moved to a separate repository** on 2026-05-09. The NeuroGrim IDE now
lives at its own ecosystem root (`D:\local-pc-operational-management\children\neurogrim-ide`),
**outside** this `D:\Brains\NeuroGrim` tree. These docs are therefore orphaned
here: they reference IDE-side call sites, Tauri commands, and frontend handlers
that do not exist in this repository. They are retained for historical provenance
(what the IDE lift set out to do, how the migration was sequenced, the adversarial
findings) but are no longer part of NeuroGrim's live reading path.

## Contents

| File | What it was |
|------|-------------|
| `PHASE-PROGRESS.md` | IDE full-lift plan — per-phase progress tracker (A–E). |
| `PHASE-A-ADVERSARIAL-REVIEW.md` | Adversarial-hat review of Phase A surfaces (B5 deliverable, 10 findings). |
| `IDE-LIFT-C9-CLASSIFICATION.md` | C9.0 — `IdeAction` variant classification (64 variants → 20 brokers; mechanical/bespoke split). |
| `IDE-LIFT-CLUSTER-MANIFEST.md` | C10 — operator activation procedure: full `cluster.toml` + per-broker manifest templates. |
| `IDE-LIFT-CALL-SITE-MIGRATION.md` | C10 — per-call-site migration mapping (legacy `invoke("ide_action")` → `broker:*` listeners). |
| `IDE-LIFT-TEMPLATES.md` | Phase C broker-shape templates. |
| `IDE-LIFT-FINAL-STATE.md` | Snapshot of the IDE-lift end state as of the final in-repo session. |

## Notes

- These files cross-reference each other by bare filename (e.g. `PHASE-PROGRESS.md`);
  since they were moved together into this directory, those references remain
  internally resolvable. Some prose paths of the form `NeuroGrim/docs/IDE-LIFT-*.md`
  are historically stale (they predate this move) and were intentionally left
  untouched — archived docs are preserved as-written, not rewritten.
- Per the `skill-deprecation` convention, superseded material is moved under an
  `archived/` path rather than deleted, so the doc-broker walk skips it (any
  `archived/` directory at any depth is excluded) while the provenance stays
  in-tree and greppable.
- Archived 2026-06-30 as part of the Documentation v5.0 upgrade (Phase 4).
