---
doc-version: 5.0.0
date: 2026-06-30
status: current
anchored-to: neurogrim
front-door: true
---

# Agent primer — index to bundled methodology docs

This document is a **thin index** to the methodology primer that ships
inside the NeuroGrim CLI binary. The canonical content lives in 8 short
topic files bundled at compile time and accessible via:

```bash
neurogrim explain <topic>
```

For an unfamiliar agent or operator entering a NeuroGrim project, the
fastest path to comprehension is:

```bash
neurogrim agent --prose          # what is this Brain right now?
neurogrim doctor                 # is the configuration sound?
neurogrim explain methodology    # what is the methodology overall?
```

## Bundled topics

| Topic | What it answers | Typical reader |
|-------|-----------------|----------------|
| [methodology](#methodology) | What is LSP Brains, the overlay framing, why scoring matters | New agent on first entry |
| [domain](#domain) | Anatomy of a domain, weight tiers, when to add | Agent extending a Brain |
| [sensor](#sensor) | Sensor authoring contract, CMDB envelope, score formula patterns | Agent or operator authoring a sensor |
| [hat](#hat) | The 8 declared hats and when to wear each | Agent making a non-trivial decision |
| [scoring](#scoring) | Unified score, confidence, trajectory, floors | Agent reading score output |
| [federation](#federation) | A2A peers, fractal composition, read-only siblings | Operator setting up a multi-Brain ecosystem |
| [cli](#cli) | All ~22 commands grouped by purpose | Anyone learning the CLI surface |
| [culture](#culture) | culture.yaml as floor-only invariant | Anyone reading shared values |

Run `neurogrim explain` (no topic) to list the topics with one-line
summaries from the live binary.

## How this is maintained

The 8 topic files are bundled at compile time via `include_str!` from
`crates/neurogrim-cli/data/explain/<topic>.md`. The bundle is the
canonical source — *not* this index. If you want to update primer
content, edit the bundled files and rebuild. This index document
exists for human discoverability (e.g., GitHub browsing); the binary
ships its own copy independently.

A compile-time test (`tests/methodology_drift.rs`) verifies that
bundled topics are non-empty and carry the expected version header
markers. Bundle drift across NeuroGrim releases is captured by the
version header in each topic file (`<!-- topic: X — bundled in
neurogrim-cli vY.Z -->`).

## Cross-references

- **The full LSP Brains specification** (RFC-2119 normative,
  ~4000 lines): `https://github.com/KeenanHoffman/LSP-Brains`
- **NeuroGrim repository CLAUDE.md**: project-level agent guide
- **`.claude/skills/`**: tactical skills (planning, hat usage,
  rubber-ducking, security review, etc.)
- **`NeuroGrim/PITCH.md`**: the under-one-minute elevator pitch
- **`NeuroGrim/INTRO.md` (in LSP-Brains repo)**: the 5-minute
  problem-first introduction

The bundled primer (this set of 8 topics) covers what *agents* need
to author and operate Brains. The spec covers what *implementors*
need to build engines. The skills cover *how* to do specific tasks
inside an existing Brain. Pick the surface that matches your need.
