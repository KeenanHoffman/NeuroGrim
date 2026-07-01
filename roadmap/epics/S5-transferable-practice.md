---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Transferable Practice (North Star)

**Stage:** 5 — Transferable Practice
**Status:** In progress
**Priority:** High
**Depends on:** S2-interface-contract, S3-prescriptive-autonomy, S4-fractal-composition
**Blocks:** nothing — this is the destination
**Stage gate:** The pattern is adoptable by someone outside this project

---

## Summary

LSP Brains becomes a transferable specification. NeuroGrim becomes a product teams
can adopt. Not "install this tool" but "adopt this methodology." Stage 5 is methodology
transfer: LSP Brains proven in DevOps becomes a reusable specification any domain can
absorb. The architecture — sensory tools, central scoring, declared governance, reflexive
hooks — extracts from this project into something any team can use.

**Revised scope (2026-04-09):** Three stories moved to earlier stages per adversary review:
- S5-TP-1 (Fractal Architecture) → S4-FC-1/S4-FC-2 in S4-fractal-composition.md
- S5-TP-4 (Domain-Agnostic Scoring) → S2-IC-2 in S2-interface-contract.md
- S5-TP-5 (Ecosystem Brain) → S4-FC-2/S4-FC-3 in S4-fractal-composition.md

What remains is the human-facing deliverable: starter kit, documentation, external adoption.

---

## Stories

### S5-TP-1: Starter Kit / Template Project

**Status:** Complete
**Effort:** XL
**Depends on:** S2-IC-2 (domain-agnostic scoring), S4-FC-1 (fractal registration)

A template directory (`starter-kit/`) someone can copy to adopt the pattern. Ships with
3 CMDB-type domains (code-quality, test-health, deploy-readiness), an adapted Brain engine
(~2,770 lines from 4,612 LaaS lines), 3 PowerShell sensory tools, 2 gates, 1 hook, 1 hat,
1 correlation, 1 incident pattern, and a 6-step tutorial.

**Key decisions (13 architectural decisions from 2-round adversary review):**
- AD3: All domains use `scoring_source.type = "cmdb"` — zero PS code per domain
- AD5: Sensory tools in PowerShell (not bash) — single-runtime dependency
- AD9: Path resolution uses `$PSScriptRoot`-relative, not `.git`-walking (CRITICAL)
- AD10: `Get-ArtifactFreshness` kept as stub returning 'unknown' (safe with empty `$artifactList`)
- AD11: Example incident pattern uses `conditions: {}` (no domain variable refs)
- AD12: Ledgers are JSON arrays `[]`, not objects

**Acceptance criteria:**
- [x] Template repository exists with working Brain, registry, and at least 1 hook
- [x] Starter kit tutorial explicitly follows the 6-step adoption ramp (skills, gates, hooks, sensory tools, Brain, hats); each step is a distinct section with achievable checkpoints
- [x] A new user can clone, configure 3 domains, and get a health score in <30 minutes
- [x] README explains the pattern in terms of LSP Brains, not LaaS
- [x] Starter kit Brain conforms to the interface contract from S2-IC-1
- [x] Starter kit documents the three truth layers and includes at least one runtime-truth CMDB example

---

### S5-TP-2: LSP Brains Specification + Documentation

**Status:** Complete
**Effort:** XL
**Depends on:** S5-TP-1 (starter kit provides implementation to codify)

Write the LSP Brains specification — a language-agnostic document that defines what any
Brain implementation must do. Also finalize the whitepaper and guides.

**Key decisions:**
- Location: `lsp-brains-spec/` at repo root (distinguishable from LaaS docs)
- Format: 12-section specification using RFC 2119 (MUST/SHOULD/MAY) language
- Narrative arc: Problem → insight → architecture → adoption path → proof
- Essential sections (MUST ship): Sensory Tool Protocol, Scoring Contract, Interface Contract, Trajectory Protocol

**Deliverables:**
1. LSP Brains Specification (`lsp-brains-spec/LSP-BRAINS-SPEC.md`) — 1,185 lines, 124 RFC 2119 terms
2. Spec README and directory structure
3. 7 Mermaid diagrams (nervous system, dual brain, fractal, scoring pipeline, trajectory, autonomy resolution, condition tree)
4. Updated whitepaper with LSP Brains framing — deferred to S5-TP-3
5. Updated VISION.md with new sections — deferred to S5-TP-3

**Acceptance criteria:**
- [x] Specification covers: sensory protocol, scoring contract, interface contract, trajectory
      protocol, governance model, fractal composition, dual brain design
- [x] Spec uses RFC 2119 and is implementable by reading only the spec (no PowerShell required)
- [ ] Whitepaper updated with LSP Brains framing and new concepts (deferred to S5-TP-3)
- [x] No references to "Operator Methodology" except in historical context
- [x] At least one diagram per major concept (7 diagrams for 7 concepts)

---

### S5-TP-3: Product Delivery + External Adoption

**Status:** Not started
**Effort:** XL
**Depends on:** S5-TP-2, S5-TP-4, S5-TP-5, S5-TP-6

Deliver NeuroGrim as a product to at least one team. Validate that the LSP Brains
methodology transfers via the specification + starter kit.

This is both active product delivery ("here is a tool for your project") AND passive
methodology validation ("can you adopt this pattern without direct assistance?").

**Key decisions:**
- First adopter: internal team (user's coworkers)
- Feedback loop: issues in the starter kit, friction documented
- Success: adopter's Brain produces meaningful health score for their project
- The methodology improves faster with teams using the product

**Acceptance criteria:**
- [ ] At least one team outside LaaS is using NeuroGrim
- [ ] The adopter's Brain produces health scores using their own domain definitions
- [ ] Adoption friction documented and fed back into starter kit
- [ ] Adopter did NOT require direct assistance to get started (spec + tutorial sufficient)
- [ ] At least 2 weeks of score history accumulated (trajectory validation)

---

### S5-TP-4: Trajectory Intelligence

**Status:** Complete
**Effort:** L
**Depends on:** S5-TP-1 (needs working Brain to add trajectory)

Add score history tracking, velocity/acceleration computation, and trend classification
to the Brain. The `-Mode trend` stub becomes a real feature.

**Key implementation notes:**
- `Get-ScoreTrajectory` in scoring.ps1: windowed velocity/acceleration with 4-class classification
- `Save-ScoreSnapshot` in scoring.ps1: append-on-agent-run with retention pruning
- OrderedDictionary iteration uses `.Keys` not `.PSObject.Properties`
- Array serialization uses `ConvertTo-Json -InputObject @($array)` to prevent pipeline unwrapping

**Acceptance criteria:**
- [x] Score history file created and appended on each `-Mode agent` run
- [x] `Get-ScoreTrajectory` computes velocity, acceleration, classification
- [x] `-Mode trend` displays human-readable trajectory summary
- [x] Agent output includes `trajectory` object when history exists
- [x] Auto-prune removes entries older than `trajectory.retention_days`

---

### S5-TP-5: Human User Personas

**Status:** Complete
**Effort:** M
**Depends on:** S5-TP-2 (personas are part of the specification)

Add persona-aware output to the Brain. Five human user personas (executive, manager,
developer, specialist, product-manager) control output verbosity and field filtering.

**Key implementation notes:**
- `personas` section in brain-registry.json with output_level, fields, focus per persona
- `-Mode brief` dispatches to persona-specific renderers
- `-Persona` parameter on Find-Brain.ps1 works with brief and agent modes
- Hat controls WHAT is emphasized; persona controls HOW MUCH detail is shown
- Validate check 15 validates persona definitions (output_level, description)
- Agent output includes `current_persona` field when `-Persona` specified

**Acceptance criteria:**
- [x] 5 human user personas defined in brain-registry.json
- [x] `-Mode brief -Persona executive` produces ≤5 lines
- [x] `-Mode brief -Persona developer` produces full output
- [x] Personas are in the spec, not just the implementation (Section 11.2)
- [x] Persona output respects hat emphasis when both are specified

---

### S5-TP-6: Zero-Config Base Brain

**Status:** Complete
**Effort:** M
**Depends on:** S5-TP-1 (builds on starter kit)

Add auto-detect sensory tool that runs all 3 base sensory tools in a single pass.
`-AutoDetect` flag on Find-Brain.ps1 provides the "point and score" experience.

**Key implementation notes:**
- `auto-detect.ps1` orchestrates all 3 sensory tools via subprocess, writes per-domain CMDBs + summary
- `-AutoDetect` suppresses stdout for JSON-emitting modes to prevent pollution
- Confidence note in `-Mode score` shows indicator counts from auto-detect summary

**Acceptance criteria:**
- [x] `auto-detect.ps1` produces all 3 CMDB files plus summary
- [x] `-AutoDetect` flag triggers auto-detect before scoring
- [x] Auto-detected score includes confidence context note
- [x] Individual sensory tools still work independently

---

### S5-TP-7: Dual Brain Architecture Design

**Status:** Complete
**Effort:** L
**Depends on:** S5-TP-2 (the spec must define the dual model)

Design the dual brain architecture — local brain (developer terminal) + external brain
(cloud compute). This is a design-only deliverable; implementation is Stage 6.

**Key deliverables:**
- `lsp-brains-spec/DUAL-BRAIN-DESIGN.md` — 751-line detailed design (12 sections, 3 appendices)
- 3 new Mermaid diagrams (detailed dual brain, sync protocol, migration path)
- `dual_brain` configuration stub in brain-registry.json (optional, ignored by current code)
- Updated spec Section 10 to reference the design document

**Design highlights:**
- 8 trigger types (file.changed, git.committed, ci.completed, webhook.received, schedule.fired, etc.)
- Event protocol with at-least-once delivery, idempotent consumers, event_id deduplication
- Shared state protocol: append-merge for history/ledgers, last-writer-wins for CMDBs/gates
- 5-phase migration path from local-only to full dual brain (zero code changes through Phase 3)
- Security model, failure modes, and 3 implementation patterns (GitHub Actions, Cloud Function, daemon)

**Acceptance criteria:**
- [x] Spec defines local brain responsibilities and trigger model
- [x] Spec defines external brain responsibilities and trigger model
- [x] Metadata proximity rules are explicit
- [x] Both brains produce `agent-output-schema.json` compliant output
- [x] Event protocol covers 6+ trigger types (8 defined)
- [x] Architecture diagram shows information flow (3 new diagrams)
- [x] Sync protocol is specified (how shared state is reconciled)
- [x] Design is implementable without changing the local brain's current code

---

### S5-TP-8: Spec v2.1 Publication (Hybrid MCP + A2A)

**Status:** In progress
**Effort:** L
**Depends on:** — (independent of other S5 stories)

Publish LSP Brains spec v2.1 with the hybrid MCP + A2A protocol split. MCP scope is
narrowed to sensory tool invocation (§3.7, Appendix F) and Brain-as-tool-to-LLM. A2A
(Agent2Agent protocol) is adopted as the normative transport for Brain-to-Brain peer
communication — fractal composition (§9) and dual brain (§10). This is the adopter-facing
spec deliverable; the implementation work lives in Stage 6.

**Key decisions:**
- Spec bump is **v2.0 → v2.1** (additive), not v3.0. No field removed, no existing
  conformance claim invalidated. Subprocess child invocation remains conformant in §9.
- A2A `authentication: none` is the only supported auth scheme in v2.1 (development /
  trusted-network only). Adopters requiring auth gate access at the network layer. Bearer
  tokens and mutual TLS are deferred to a future spec version.
- The event-transport section in `DUAL-BRAIN-DESIGN.md` §5 is recast as the A2A message
  vocabulary; the 10 event types (score.updated, gate.changed, ecosystem.scored,
  incident.detected, incident.resolved, snapshot.requested, snapshot.delivered,
  proposal.created, proposal.resolved, config.changed) become A2A message types.

**Deliverables:**
- `spec/LSP-BRAINS-SPEC.md` v2.1 header + §1.1 scope bullet + §3.7 boundary note +
  §9 transport table + §9.7 A2A mapping + §10.4 rewrite + new §13 "A2A Peer Protocol"
  + new Appendix G "A2A Integration" + glossary additions (A2A, Agent Card, Peer Brain,
  Task, A2A Message) + reference-map rows
- `spec/DUAL-BRAIN-DESIGN.md` v1.1 — §5 recast as A2A Message Vocabulary; shared-file
  transport demoted to degraded-mode fallback
- `spec/METHODOLOGY-EVOLUTION.md` — new §6 entry explaining the protocol split
- `schemas/a2a-envelope-v1.schema.json` — NEW, validates A2A envelope
- `schemas/agent-card-v1.schema.json` — NEW, validates Brain Agent Cards
- `schemas/brain-registry-v2.schema.json` — additive `children[].a2a_endpoint` optional
  field; `dual_brain.event_transport.mode` with "a2a" default

**Acceptance criteria:**
- [x] Spec v2.1 header + changelog entry
- [x] §1.1, §3.7, §9, §10 updated with protocol boundary language
- [x] §13 A2A Peer Protocol added
- [x] Appendix G A2A Integration added
- [x] Glossary includes A2A, Agent Card, Peer Brain, Task (A2A), A2A Message
- [x] `a2a-envelope-v1.schema.json` validates as JSON Schema draft-07
- [x] `agent-card-v1.schema.json` validates as JSON Schema draft-07
- [x] `brain-registry-v2.schema.json` still validates as draft-07 after additive changes
- [x] `METHODOLOGY-EVOLUTION.md` entry documents the decision + rationale
- [x] `DUAL-BRAIN-DESIGN.md` §5 recast complete
- [ ] README cross-links updated in both repos (LSP-Brains + NeuroGrim)
- [ ] Python SDK README references the A2A boundary

---

### S5-TP-9: Cultural Substrate

**Status:** In progress
**Effort:** M
**Depends on:** — (independent)

Introduce a **cultural substrate** — a lightweight, first-class declarative layer that
governs *how* agents communicate (agent↔agent and agent↔human). Five canonical values
carried as invariants analogous to `safety_invariants` in autonomy resolution (§5.5):
positivity, integrity, honesty, critical-but-kind, respect. Research (Anthropic
interpretability) shows emotional activations are load-bearing in LLM outputs regardless
of surface prompting; declaring culture beats ignoring it.

**Key decisions:**
- Five values, ~15-line manifest — exceedingly simple scope; bloat is the primary risk
- **Three identical peer-local copies** (not inheritance by reference) — each agent
  self-sufficient; drift becomes a visible health signal rather than a mechanical concern
- Culture invariants apply LAST in the output pipeline, after hats, personas, and
  human-comms. They can only tighten, never loosen.
- No drift sensor in v1 — declaration is enforcement for now; drift detection deferred
  as future work
- Ecosystem Brain gets a `culture-coherence` domain that verifies byte-identity of the
  three copies (structural drift detection)

**Deliverables:**
- `schemas/culture-manifest-v1.schema.json` (LSP-Brains repo) — validates the manifest
- `culture.yaml` × 3 — identical copies at the ecosystem root, in NeuroGrim, in LSP-Brains
- `spec/LSP-BRAINS-SPEC.md` §14 "Cultural Substrate" + TOC + Appendix D row + Appendix E glossary entries
- `roadmap/VISION.md` principle #17
- `spec/METHODOLOGY-EVOLUTION.md` §7 "Cultural Substrate" with rationale
- `rubber-duck.md` skill × 3 (identical copies) — the first concrete user of the culture layer, demonstrates critical-but-kind in action

**Acceptance criteria:**
- [x] `culture-manifest-v1.schema.json` validates as JSON Schema draft-07
- [x] Three `culture.yaml` copies present and byte-identical at v1.0.0
- [x] Spec §14 added with TOC entry and normative content
- [x] Appendix D updated (culture module row)
- [x] Appendix E glossary updated (Cultural Substrate, Culture Invariant, Culture Manifest)
- [x] VISION.md principle #17 present
- [x] METHODOLOGY-EVOLUTION.md §7 added
- [x] `rubber-duck.md` skill present in all three `.claude/skills/` dirs

---

### S5-TP-10: LSP-Brains Brain

**Status:** In progress
**Effort:** L
**Depends on:** S5-TP-8 (needs schemas), S5-TP-9 (needs culture.yaml)

The LSP-Brains spec repo gets its own Brain — the **first Brain that scores a
specification rather than a codebase**. Proof point for "ideas as code" (or more broadly,
"methodology as code"). Seven advisory domains measure different dimensions of spec
quality; sensory tools are follow-on work (natural Python-SDK examples).

**Key decisions:**
- All domains advisory (weight 0.0) for v1 — this is methodology scoring; promotion to
  weighted happens later if a real blocking use case emerges
- CMDBs start as stubs at score 0 — per spec principle #2 (scoring must be honest),
  unknown is not good; honest zero is better than fake 100
- Sensory tools are Python-SDK follow-ons, not part of this story — the bootstrap is
  the registry + stubs + CLAUDE.md

**Seven domains:**

| Domain | What it measures |
|--------|------------------|
| `spec-completeness`    | All TOC entries have bodies; no "TBD" in normative sections |
| `schema-validity`      | All `*.schema.json` parse as draft-07; `$ref`s resolve |
| `link-integrity`       | Internal `§`-references resolve; METH-EV back-references valid |
| `glossary-freshness`   | Terms in recent sections appear in Appendix E; no orphans |
| `diagram-sync`         | Diagrams referenced in prose exist under `spec/diagrams/` |
| `rfc-2119-compliance`  | MUST/SHOULD/MAY used consistently in normative contexts |
| `changelog-hygiene`    | Spec version bumps carry changelog entries citing METH-EV |

**Deliverables:**
- `D:\Brains\LSP-Brains\.claude\brain-registry.json`
- `D:\Brains\LSP-Brains\.claude\*-cmdb.json` (7 stubs)
- `D:\Brains\LSP-Brains\.claude\culture.yaml` (copy from S5-TP-9)
- `D:\Brains\LSP-Brains\.claude\skills\rubber-duck.md` (copy from S5-TP-9)
- `D:\Brains\LSP-Brains\CLAUDE.md` — agent guide for working in a spec repo

**Acceptance criteria:**
- [x] `.claude/brain-registry.json` validates against `brain-registry-v2.schema.json`
- [x] All 7 CMDBs validate against `cmdb-envelope-v1.schema.json`
- [x] All 7 domains declared advisory (weight 0.0)
- [x] `CLAUDE.md` present with repo purpose, domain table, spec-editing workflow
- [x] `culture.yaml` byte-identical to the ecosystem + NeuroGrim copies
- [ ] (Future) At least one sensory tool lands — likely `spec-completeness` first as the
      simplest text-pattern check

---

## Epic Completion Criteria

- [ ] Someone outside the LaaS project successfully adopts the pattern
- [x] Starter kit enables "declare → score → hook" in one afternoon
- [x] LSP Brains specification is complete and language-agnostic
- [x] Trajectory intelligence, personas, zero-config, and dual brain design are all complete
- [x] Documentation explains the pattern without LaaS-specific assumptions

## North Star Check

- Does this make the pattern more general? **This IS the generalization.**
- Does this make the ecosystem Brain easier? **Proven by external adoption.**
- Does this separate methodology from product? **LSP Brains spec is the methodology. NeuroGrim is the product.**
