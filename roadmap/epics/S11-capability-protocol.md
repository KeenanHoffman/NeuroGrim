# Epic: Capability Protocol (CapProto) — Stage 11

**Stage:** 11
**Status:** `stub` — conditional on B-10 Phase 3 go-criteria. NOT yet
promoted to `ROADMAP.md`. This file exists to capture the vision
durably while evidence accrues.
**Priority:** Deferred (pending B-10 Phase 3 evidence)

---

## Scope Note — Read This First

**This is a stub epic.** It describes a stage that MAY ship if the
B-10 research arc produces evidence that justifies it. It does NOT
represent committed work. No row exists in `ROADMAP.md`. No schemas
have been written. No code has been sketched.

Per the 2026-04-22 planning decision (see
`~/.claude/plans/parallel-hugging-eich.md`), the operator chose
"partial anchor" — capture the thinking in an epic file, but do not
advertise a stage number until evidence arrives.

**Activation trigger (all three must hold):** B-10 Phase 3 prototype
benchmark meets every go/no-go criterion documented in the B-10
BACKLOG entry:
- typical-session delta ≥ 5k tokens saved
- worst-case latency ≤ 300ms per `load_capability` call
- no stale-cache bug observed in a 2-week dogfood window on NeuroGrim's
  own skill corpus

If any criterion fails, this stub stays a stub. B-10 parks. No
ceremonial escalation.

---

## Goal

Introduce a third protocol vertex to the LSP Brains ecosystem:
**CapProto** (Capability Protocol). Alongside MCP (sensing the world)
and A2A (peering with other Brains), CapProto indexes the Brain's
**own** capability surface — skills, tools, and subagents — under one
manifest with LSP-inspired lazy fetches.

The outcome: a Brain's pilot agent starts a session with a compact
table of contents (~1-2k tokens) instead of the full capability body
(~10-50k tokens today). Details load on demand, cached per session.

## Framing

The name "LSP Brains" originally borrowed "LSP" as a methodology
metaphor: sensors observing state the way language servers observe
code. CapProto borrows "LSP" in a second, narrower sense: the lazy
request/response pattern of `textDocument/hover`,
`workspace/symbol`, and `textDocument/definition`. These are two
different inspirations from the same acronym — a coincidence that
must be managed carefully, not papered over.

**Mandatory naming firewall:** the first two paragraphs of the spec
§16 CapProto section MUST open by differentiating:
- "LSP Brains" — the sensor-methodology name.
- "LSP-inspired capability indexing" — a tooling-layer optimization
  the Brain uses to describe itself efficiently.

This differentiation is a gating review criterion for S11, not a
cleanup task.

## Absorbs

None at stub time. Absorption decisions land when the stub activates.

## Depends on

- **B-09** (CLI-mode sensory access) — must ship first. Provides the
  first data point for the measurement arc.
- **B-10 Phase 1** (measurement) — must run. Without it, the problem
  size is unknown and S11 cannot be justified.
- **B-10 Phase 2** (approach selection) — must complete with
  plan-critic review.
- **B-10 Phase 3** (prototype go/no-go) — must pass all three
  criteria above.

## Blocks

- Future "capability-hygiene" Brain domain (ecosystem-level scoring
  for dead, shadowed, and deprecated capabilities). Cannot score what
  is not manifested.
- Any future unification of skill/tool/subagent governance. CapProto
  gives the three surfaces a shared schema; without it, each remains
  a separate prose-and-convention surface.

---

## Stage 11 Is Done When

(Only relevant after the stub activates. Listed for vision durability.)

- [ ] `capability-manifest-v1.schema.json` published in `LSP-Brains/schemas/`.
      Models skills, tools, subagents uniformly: `id`, `kind` (skill/tool/
      subagent), `summary` (hover preview ≤ 120 chars), `body_ref` (load
      pointer), `tags`, `shadowed_by`, `deprecated`. Includes `canonical_id`
      for cross-Brain sharing.
- [ ] `capability-envelope-v1.schema.json` published in `LSP-Brains/schemas/`.
      **Parallel to `a2a-envelope-v1` — NOT an expansion.** Carries the new
      message types (`capability.query`, `capability.definition`,
      `capability.diagnostics`). Semver-safe, domain-clean.
- [ ] Selection-quality writing standard landed: new skill
      `capability-hook-authoring.md` + spec §16 subsection. The
      `summary` field becomes the routing contract; hook quality is
      load-bearing.
- [ ] Diagnostics channel shipped: server-push of staleness/shadow/
      deprecation signals. Transport: additions to the capability-envelope
      response shape, streamed when the pilot polls.
- [ ] Subagents brought under the manifest as `kind: subagent` entries.
- [ ] `capability-hygiene` Brain domain added to the ecosystem registry.
      Integrates with the S10 domain-promotion pipeline so bad-hook growth
      becomes scoreable.
- [ ] Naming firewall present in spec §16 (gating review criterion).
- [ ] LSP-Brains spec bump v2.5 → v2.6; `METHODOLOGY-EVOLUTION.md` gains
      a new chapter; changelog cites the evolution section.
- [ ] Row added to `NeuroGrim/roadmap/ROADMAP.md` (explicit
      promotion — the "partial anchor" dissolves only when this box is
      checked).

**Anti-criteria (explicit non-goals):**
- NOT a replacement for MCP. CapProto indexes the Brain's self;
  MCP remains the wire for external tools.
- NOT a replacement for A2A. CapProto is strictly self-indexing;
  peer-to-peer stays on A2A.
- NOT an attempt to lazy-load MCP tool schemas that Claude Code
  natively loads. Scope is skills + Brain-owned capabilities until
  Claude Code supports lazy tool-schema registration natively.
- NOT a central-skill-registry. Three-Brain skill byte-duplication
  is B-11 (separate backlog item, separate evaluation).
- NOT a semantic search / RAG layer. Selection happens via hook
  quality, not embedding similarity.

---

## Work Items Sketch (only relevant post-activation)

Inferred from the 2026-04-22 planning session; not committed until
S11 activates.

### S11-CP-1 — Capability manifest schema
New `LSP-Brains/schemas/capability-manifest-v1.schema.json`. Required
fields: `id`, `kind`, `summary`, `body_ref`. Optional: `tags`,
`shadowed_by`, `deprecated` (boolean + reason), `canonical_id`
(cross-Brain share). Template: shape mirrors `agent-card-v1.schema.json`
where applicable.

### S11-CP-2 — Capability envelope schema
New `LSP-Brains/schemas/capability-envelope-v1.schema.json`. Parallel
to `a2a-envelope-v1`. Message-type enum:
- `capability.query` (request a TOC slice, with optional filters)
- `capability.definition` (deliver one capability's full body)
- `capability.diagnostics` (server-push hints: stale, shadowed,
  deprecated)

### S11-CP-3 — Selection-quality writing standard
New `LSP-Brains/.claude/skills/capability-hook-authoring.md`.
Captures the standard for `summary` fields (≤ 120 chars, verb-first,
names the decision the capability helps with, not the implementation).
Spec §16 subsection codifies the contract. Lint rule: hooks without
verbs, hooks that duplicate names, hooks longer than the cap.

### S11-CP-4 — Diagnostics channel
Server-push of three signal types: `staleness` (skill unchanged >
6 months, low usage), `shadow` (two capabilities overlap semantically
— flagged for operator review), `deprecated` (explicit marker plus
replacement pointer).

### S11-CP-5 — Subagent-as-capability unification
Today subagents live in `.claude/subagents/` (or equivalent) as plain
prompt files. Bring them under the manifest as `kind: subagent`
entries. Expected to be trivial once CP-1 lands.

### S11-CP-6 — `capability-hygiene` Brain domain
Ecosystem-level scoring domain. Penalizes: dead capabilities (no
usage in N days), shadow pairs, hook-quality failures, hooks without
bodies, orphan `canonical_id` pointers. Promotes via the S10
mechanism — begins as advisory, earns weight through calibration.

---

## Spec Impact (post-activation)

- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` — new §16
  "Capability Discovery Protocol (CapProto)". Version bump v2.5 → v2.6.
  Opens with the naming firewall.
- `LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` — new chapter (§14 or §15
  depending on what lands between now and activation). Motivating
  question: "Sensors observe the world. What observes the Brain's own
  capability surface?"
- 2 new schemas (CP-1, CP-2).
- 1 new skill (CP-3).
- Changelog entries in spec header citing the METHODOLOGY-EVOLUTION
  section.

---

## Risks (plan-critic pre-activation)

1. **Phase 1 kills the parent.** If B-10 Phase 1 measurement shows
   worst-Brain cold-start ≤ 8k tokens, B-10 parks and this stub never
   activates. Honor the evidence; do not promote.
2. **Claude Code skill-harness incompatibility.** The biggest
   architectural unknown: today Claude Code's harness discovers
   `.claude/skills/*.md` at session start independent of MCP.
   Meta-MCP lazy loading works for *new* capabilities but cannot
   retroactively lazy-load files the harness already reads. B-10
   Phase 2 must validate this before picking an approach.
3. **Hook quality is load-bearing.** CapProto succeeds or fails on
   the quality of the `summary` field. Garbage hooks → wasted lazy
   loads → worse than baseline. CP-3 is not optional scope.
4. **Naming firewall drift.** Once live, "LSP" gets used casually in
   two senses. Code reviews must catch this before it calcifies in
   spec prose.
5. **Ecosystem-coupling risk.** CapProto introduces a new
   ecosystem-wide schema pair. Every Brain adopts the manifest at
   roughly the same cadence, or the `culture-coherence`-style byte-
   check falls out of sync. Plan deprecation + migration windows
   into activation.

---

## References

- Planning document: `~/.claude/plans/parallel-hugging-eich.md`
- Parent backlog items: `NeuroGrim/roadmap/BACKLOG.md` B-09, B-10, B-11
- Visionary framing: session transcript 2026-04-22 ("third protocol
  vertex" — MCP, A2A, CapProto)
- Shape templates: `LSP-Brains/schemas/agent-card-v1.schema.json`,
  `a2a-envelope-v1.schema.json`
