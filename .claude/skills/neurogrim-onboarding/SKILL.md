---
name: neurogrim-onboarding
description: >-
  You are an AI agent (or human operator) entering a NeuroGrim project for the
  first time and need to orient quickly: what is this Brain, what's it
  measuring, what state is it in, what can I do with it. This skill points at
  the four commands that answer those questions in under a minute, and at the
  bundled methodology primer when you need to go deeper.
when_to_use: >-
  "what is this Brain", "what's measured here", "where do I start", "I just
  entered this project", "how does this Brain work", "what's the methodology",
  "I need to add a domain", "how do I score this", "explain neurogrim",
  "tour this brain", "introspect", "orient me", "first time on this codebase"
---

# Skill: NeuroGrim Onboarding

**When to use this skill:** You're an agent or operator entering a project
that has NeuroGrim wired in (`.claude/brain-registry.json` exists) and you
need to know — quickly — what's here, what it's measuring, and what you can
do. Don't read the spec. Don't grep skills. Run four commands.

## The four commands

```bash
neurogrim agent --prose          # 1. What is this Brain right now?
neurogrim doctor                 # 2. Is the configuration sound?
neurogrim explain methodology    # 3. What's the methodology overall?
neurogrim explain cli            # 4. What can I invoke?
```

Read in order. Each takes ~5–30 seconds to digest.

## What each command answers

### `neurogrim agent --prose`

The "what is this Brain" question. Outputs ~30 lines covering:
- Brain identity (project name, path, domain count)
- Current state (unified score, confidence, trajectory)
- Strongest signals (top 3 highest-scoring domains)
- Calls to action (top 3 recommendations from the scoring pipeline)
- Available skills (everything in `.claude/skills/`)
- Available hats (the lenses you can wear)
- Federation peers (other Brains this one talks to)

If the Brain is "all-advisory" (no weighted domains), it'll say so —
that's a legitimate observe-only posture, not a misconfiguration.

### `neurogrim doctor`

The "is this sound" question. Six families of checks: schema,
domain definitions, principle map alignment, CMDB path resolution,
culture.yaml presence, federation port uniqueness. Exit 0 = clean,
1 = warnings, 2 = errors. Read the output even when exit is 0 —
the absence of findings is itself a signal.

### `neurogrim explain methodology`

The "what's the model" question. ~150 lines covering the overlay
framing, the 5-piece model (domains, sensors, scoring, governance,
federation), why this exists, and what to read next. Read this once
per project; you don't need to re-read it.

### `neurogrim explain cli`

The "what can I do" question. Enumerates ~22 commands grouped by
purpose (introspection / authoring / execution / bookkeeping). When
you forget which command does what, this is faster than `--help`.

## Going deeper — when you need it

```bash
neurogrim explain               # list all 8 bundled topics
neurogrim explain domain        # adding or modifying a domain
neurogrim explain sensor        # authoring a sensor (CMDB envelope)
neurogrim explain hat           # the 8 hats and when to wear each
neurogrim explain scoring       # how the unified score actually works
neurogrim explain federation    # multi-Brain ecosystems
neurogrim explain culture       # the floor-only invariant
```

Each topic stands alone. Read in any order, depending on the task
in front of you.

## Common follow-on workflows

| You want to... | Run this |
|----------------|----------|
| Add a new domain | `neurogrim domain new <name>` (read `explain domain` first if unsure) |
| Add a project-specific skill | `neurogrim skill new <name>` |
| Connect a peer Brain | `neurogrim federation register --name <peer> --path <path>` |
| Refresh a domain's score | `neurogrim sensory <name> --project-root . > .claude/<name>-cmdb.json` |
| Narrate state through a hat | `neurogrim narrate --hat <hat-name>` |
| Bypass MCP (CLI-only mode) | Read `.claude/skills/cli-mode/SKILL.md` |

## What this skill does NOT do

This skill is a router, not a tutorial. It does not teach the
methodology, walk you through authoring a domain, or replace
the full LSP Brains specification. For those:

- **Tutorial**: `neurogrim explain methodology` then walk topics
- **Authoring**: `neurogrim domain new`, `neurogrim skill new`,
  or read the relevant `explain` topic first
- **Spec**: `https://github.com/KeenanHoffman/LSP-Brains` for the
  full RFC-2119 normative specification

## Cultural substrate

NeuroGrim's cultural substrate (`.claude/culture.yaml`) declares
five floor-only invariants: positivity, integrity, honesty,
critical-but-kind, respect. Apply them to onboarding outputs the
same way you apply them everywhere else — you can be more, never
less, than these five. See `neurogrim explain culture`.

## See also

- `.claude/brain-registry.json` — the project's registry; the
  `--prose` orientation reads from this
- `.claude/skills/cli-mode/SKILL.md` — invoke NeuroGrim via Bash
  instead of MCP (zero-context-cost mode for power users)
- `.claude/skills/hats/SKILL.md` — formal hat-system documentation
- `docs/AGENT-PRIMER.md` (NeuroGrim repo) — index doc pointing at
  the bundled topics this skill references
