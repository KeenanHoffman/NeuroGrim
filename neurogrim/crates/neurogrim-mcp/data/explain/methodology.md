<!-- topic: methodology — bundled in neurogrim-cli v3.5 -->
# LSP Brains methodology — the 5-minute primer

LSP Brains is **a declared overlay of project-shaped commitments on a
general-purpose statistical engine**. The LLM provides cognition; the
Brain provides what to be cognizant of. NeuroGrim is the engine that
runs that overlay.

If you're an agent reading this, you've entered a NeuroGrim project.
This document teaches you the conceptual model. To see what *this*
specific Brain currently looks like, run `neurogrim agent --prose`.

<!-- anchor: drift-problem -->
## The drift problem

Codebases drift. Tests rot, dependencies stale, security postures
slip, documentation diverges from reality. Humans notice late, when
the signals have already compounded into a crisis. AI agents notice
even later, because they enter each session with no continuity.

A Brain is a continuous nervous system for a project. It declares
what to watch (domains), measures the watched things (sensors),
aggregates measurements into a unified score (scoring), and
recommends action when something drifts (governance). The agent's
job is to act on the Brain's signals; the Brain's job is to keep
the signals current and honest.

<!-- anchor: five-piece-overlay -->
## The five-piece overlay

A Brain consists of five layered concerns:

1. **Domains** — declared units of concern. "Test health,"
   "code quality," "deploy readiness," "security standards."
   Each domain has a name, a weight (how much it influences the
   unified score), and a definition (where its data comes from).
   See `neurogrim explain domain`.

2. **Sensors** — small programs that measure a domain. Sensors
   read on-disk artifacts (tests, lint output, lockfiles, docs)
   and emit a CMDB JSON envelope: `{score: 0..100, findings:
   [...], updated_at: ...}`. NeuroGrim ships 20 built-in sensors;
   adopters add their own. See `neurogrim explain sensor`.

3. **Scoring** — aggregates per-domain scores into a unified
   0–100 score. Confidence-weighted (stale data scores low
   confidence; low-confidence domains contribute less). Floor
   gates allow a single critical domain to cap the unified
   score regardless of others. See `neurogrim explain scoring`.

4. **Governance** — when scoring identifies a problem, the Brain
   emits gated recommendations: tier (immediate, before-merge,
   pre-deploy), action, blocking conditions. Agents act on
   recommendations; humans approve high-blast-radius actions.
   See `neurogrim explain hat` for hat-mediated decision making.

5. **Federation** — Brains are A2A peers. An ecosystem Brain
   queries child Brains for their scores via the A2A protocol;
   the children remain authoritative. Fractal composition lets
   a tree of Brains report a single ecosystem score. See
   `neurogrim explain federation`.

<!-- anchor: for-agents -->
## Why this matters for agents

When you act on a project, you have two options:

- **Stateless option:** read the source, infer what's wrong,
  propose a fix. You are guessing at trends, missing what
  multiple sessions revealed but you didn't.
- **Brain-augmented option:** read the Brain's current scorecard,
  see what's drifting, see *why* it's drifting (which domains,
  which findings, which trajectory), and act with continuity.
  The Brain remembers what previous sessions established.

The grammar of the Brain is small (~5 concepts, ~20 commands).
Once you grok it, the cost of operating with it is low and the
return is large.

## Where to go next

- `neurogrim agent --prose` — see this Brain's current state
- `neurogrim doctor` — verify configuration is sound
- `neurogrim explain domain` — learn how to add or modify a domain
- `neurogrim explain sensor` — learn how to author a sensor
- `neurogrim explain cli` — survey the CLI surface
- `neurogrim explain hat` — learn the hat system for decision-making

The full specification lives at
`https://github.com/KeenanHoffman/LSP-Brains` (LSP-BRAINS-SPEC.md,
~4000 lines, RFC-2119 normative). The bundled topics here cover what
agents need to author and operate Brains; the spec covers what
implementors need to build engines.

## Cultural substrate

Every Brain ships with `culture.yaml` declaring five operating
invariants: **positivity, integrity, honesty, critical-but-kind,
respect**. These apply to every Brain output and every peer
exchange. They can only tighten, never loosen — see
`neurogrim explain culture`.
