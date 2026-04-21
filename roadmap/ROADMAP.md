# Roadmap: LSP Brains + NeuroGrim

**North star:** `VISION.md`
**Dependencies:** `DEPENDENCIES.md`
**Data architecture:** `DATA-ARCHITECTURE.md`
**Last updated:** 2026-04-21 (S6 Remote-Agent Topology shipped: bearer auth + Caddy TLS + webhook-sync + CEO template + e2e-sim; new Stage 7 Agent Behavior Verification epic planned)

---

## Stages Overview

| Stage | Name | Status | Epics |
|-------|------|--------|-------|
| 1 | Honest Single Brain | **Complete** | 4 epics (15 stories) |
| 2 | Interface Contract | **Complete** | 1 epic (5 stories) |
| 3 | Prescriptive Autonomy | **Complete** | 1 epic (3 stories) |
| 4 | Fractal Composition | **Complete** | 1 epic (4 stories) |
| 5 | Transferable Practice | In progress | 1 epic (10 stories) |
| 6 | Dual Brain via A2A (prior name: Dual Brain Implementation) | **Complete** — DB-1..5 + DB-7 all shipped; DB-6 (Python SDK helper) remains as stretch only; Remote-Agent Topology (bearer + Caddy + webhook-sync + CEO template + e2e-sim) shipped 2026-04-21 | 1 epic (7 stories) + Remote-Agent Topology sub-epic (5 phases, all shipped) |
| 7 | Agent Behavior Verification | Planned (2026-04-21) | 1 epic (7 stories) — `S7-agent-behavior-verification.md` |

Stages are sequential but overlapping. Each stage must produce a working system, not just
scaffolding.

**Revision history:** Stages 2-5 restructured on 2026-04-09 after adversary review of
Stage 1 implementation lessons. See `.claude/plans/north-star-adversary-review.md` for full
rationale. Key changes: old Stage 3 (Multi-Agent Coordination) eliminated — already solved
by hats + filesystem. Old Stage 4 promoted to Stage 3. S2/S5 fractal work merged into Stage 4.
Framework extraction pulled into Stage 2. Net: 14 stories → 12, critical path shortened.

---

## Stage 1: Honest Single Brain

**Goal:** The unified score reflects reality. Confidence is actionable, not decorative.
The Brain learns from its own recommendations.

**Prerequisite:** Brain decomposition — COMPLETE (PR #61)

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Honest Scoring | `epics/S1-honest-scoring.md` | High | **Complete** | S1-HS-1, S1-HS-2, S1-HS-3 |
| Diagnostic Reasoner | `epics/S1-diagnostic-reasoner.md` | High | **Complete** | S1-DR-1, S1-DR-2, S1-DR-3, S1-DR-4 |
| Learning Brain | `epics/S1-learning-brain.md` | High | **Complete** | S1-LB-1, S1-LB-2, S1-LB-3, S1-LB-4 |
| Context-Aware Agent | `epics/S1-context-aware-agent.md` | Medium | **Complete** | S1-CA-1, S1-CA-2, S1-CA-3, S1-CA-4 |

**Parallel tracks:** Honest Scoring and Diagnostic Reasoner are independent.

**Stage 1 is DONE when:**
- [x] Unified score incorporates confidence (floor + multiplier models)
- [x] Fully-observed system at 70 outscores partially-observed system at 85
- [x] Correlation engine supports AND/OR/NOT with at least one temporal pattern
- [x] Proposal ledger records outcomes; can rank by effectiveness after 10+ entries
- [x] 3+ hats defined with domain emphasis weights
- [x] All epic completion criteria pass

**Methodology note:** Stage 1 proved the pattern works for DevOps. PRs #67-68 named it
"LSP Brains" and established the 6-step adoption ramp, terminology governance,
and the pilot agent / nervous system framing. These are Stage 1's proof that the pattern
transfers — the architecture is domain-agnostic even though the first implementation is DevOps.

---

## Stage 2: Interface Contract + Framework Extraction

**Goal:** The Brain's interface is formalized as a versioned schema. The scoring framework
is separable from LaaS domain definitions. A new domain can be added without modifying
PowerShell scripts.

This is the stage where the methodology becomes portable — the interface contract is what
allows a new implementation to plug in without rewriting the Brain.

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Interface Contract | `epics/S2-interface-contract.md` | -- | **Complete** | S2-IC-1, S2-IC-2, S2-IC-3, S2-IC-4, S2-IC-5 |

**Stage 2 can BEGIN when:**
- [x] Honest Scoring epic complete
- [x] Diagnostic Reasoner epic complete
- [x] DATA-ARCHITECTURE.md open questions resolved

**Stage 2 is DONE when:**
- [x] `-Mode agent` output validates against a declared JSON schema
- [x] A synthetic test domain can be added/scored/removed via registry-only changes
- [x] Output contracts documented and tested for 3 consumer types (human, agent, parent)
- [x] No LaaS-specific domain names hardcoded in scoring.ps1 or correlation.ps1
- [x] Truth separation formalized: source/runtime/derived artifacts classified in DATA-ARCHITECTURE.md
- [x] File interpretation spectrum documented; every file type has at least one LSP pathway

**Stage 3 can BEGIN when:**
- [x] S2-IC-1 (interface contract) is complete

---

## Stage 3: Prescriptive Autonomy

**Goal:** The Brain auto-executes safe proposals, presents risky ones for approval.
The human reviews decisions, not checklists.

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Prescriptive Autonomy | `epics/S3-prescriptive-autonomy.md` | -- | **Complete** | S3-PA-1, S3-PA-2, S3-PA-3 |

**Stage 3 is DONE when:**
- [x] Autonomy gradient defined and configurable in brain-registry.json
- [x] Safe proposals auto-execute with audit trail in proposal ledger
- [x] Human-in-the-loop boundary enforced and adjustable
- [x] No auto-execution of destructive actions regardless of confidence

**Stage 4 can BEGIN when:**
- [x] S3-PA-1 (autonomy gradient) is complete
- [x] S2-IC-1 (interface contract) is complete

---

## Stage 4: Fractal Composition

**Goal:** Parent Brain consumes child Brain scores via the interface contract. Cross-project
incident patterns fire. The fractal architecture from VISION.md becomes real.

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Fractal Composition | `epics/S4-fractal-composition.md` | -- | Complete | S4-FC-1, S4-FC-2, S4-FC-3, S4-FC-4 |

**Stage 4 is DONE when:**
- [x] Cross-project dependency graph declared as code in ecosystem-registry.json
- [x] Parent Brain consumes child Brain scores via interface contract
- [x] Ecosystem-level unified score produced with recursive confidence
- [x] At least one cross-project incident pattern fires correctly

**Stage 5 can BEGIN when:**
- [x] S4-FC-1 (child registration) is complete
- [x] S2-IC-2 (domain-agnostic scoring) is complete

---

## Stage 5: Transferable Practice (North Star)

**Goal:** Someone adopts LSP Brains for their own project in an afternoon.

This is the methodology transfer stage. LSP Brains becomes a transferable specification.
NeuroGrim becomes a product teams can adopt. The adopter follows the 6-step
absorption ramp: declare domains, write sensory tools, score health, gate actions, wire
hooks, add hats.

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Transferable Practice | `epics/S5-transferable-practice.md` | High | In progress | S5-TP-1 through S5-TP-7 |

| Story | Name | Status | Effort |
|-------|------|--------|--------|
| S5-TP-1 | Starter Kit / Template Project | **Complete** (archived 2026-04-17) | XL |
| S5-TP-2 | LSP Brains Specification + Documentation | **Complete** | XL |
| S5-TP-3 | Product Delivery + External Adoption | Not started | XL |
| S5-TP-4 | Trajectory Intelligence | **Complete** | L |
| S5-TP-5 | Human User Personas | **Complete** | M |
| S5-TP-6 | Zero-Config Base Brain | **Complete** | M |
| S5-TP-7 | Dual Brain Architecture Design | **Complete** | L |
| S5-TP-8 | Spec v2.1 Publication (Hybrid MCP + A2A) | In progress | L |
| S5-TP-9 | Cultural Substrate | In progress | M |
| S5-TP-10 | LSP-Brains Brain (ideas as code) | In progress | L |

**Additional domains shipped (beyond S5-TP-2 scope):**

| Domain | Status | Notes |
|--------|--------|-------|
| `coherence` | **Complete** | Cross-domain association cortex; evaluates correlations; advisory weight 0.0 |
| `human-comms` | **Complete** | Two-layer human model (user + project scope); advisory weight 0.0 |
| `secret-refs` | **Complete** | Safe credential reference catalog; extensible via Python SDK; advisory weight 0.0 |

**Stage 5 is DONE when:**
- [x] Starter kit enables "declare → score → hook" in one afternoon
- [x] LSP Brains specification covers: sensory protocol, scoring contract, interface contract,
      trajectory protocol, governance model, fractal composition, dual brain design
- [x] Spec uses RFC 2119 and is implementable by reading only the spec (no PowerShell required)
- [x] Trajectory intelligence produces velocity/acceleration/classification from score history
- [x] Human user personas adapt Brain output for 5 stakeholder types
- [x] Zero-config auto-detection scores any repo with no configuration
- [x] Dual brain architecture designed (local + external, shared state sync protocol)
- [x] Whitepaper updated with LSP Brains framing and new concepts (rewritten as `whitepaper/WHITEPAPER.md`)
- [x] No references to "Operator Methodology" except in historical context
- [ ] Someone outside LaaS successfully adopts the pattern

---

## Stage 6: Dual Brain via A2A

**Prior name:** "Dual Brain Implementation" — renamed 2026-04-17 when A2A (Agent2Agent
Protocol) was adopted as the normative transport for peer Brain communication.

**Goal:** The External Brain responds to CI/CD events, Jira tickets, and other external
changes. The architecture designed in S5-TP-7 becomes operational via the A2A Peer
Protocol (spec v2.1 §13 + Appendix G). Local and external Brains coordinate as A2A peers;
parent/child fractal composition gains A2A as a RECOMMENDED transport alongside subprocess.

**Depends on:** Stage 5 (especially S5-TP-7 dual brain design and S5-TP-8 spec v2.1 + schemas)

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Dual Brain via A2A | `epics/S6-dual-brain-a2a.md` | High | **Complete** (DB-1..5 + DB-7 shipped; DB-6 stretch only) | S6-DB-1, S6-DB-2, S6-DB-3, S6-DB-4, S6-DB-5, S6-DB-6, S6-DB-7 |

| Story | Name | Status | Effort |
|-------|------|--------|--------|
| S6-DB-1 | neurogrim-a2a Crate Scaffold | **Complete** (19/19 tests pass, Rust+MinGW on D:\) | L |
| S6-DB-2 | Ecosystem Refactor to A2A + Subprocess Dispatch | **Complete** (contract test proves transport equivalence; new neurogrim-ecosystem crate) | XL |
| S6-DB-3 | Brain A2A Server (Serve Self as Peer) | **Complete** (CLI subcommands + Agent Card + real scoring pipeline wired; 2 URL bugs fixed live; 173 workspace tests green) | L |
| S6-DB-4 | Dual Brain Pair Integration Test | **Complete** (2 real subprocesses on loopback; 3 tests; 176 workspace tests green) | M |
| S6-DB-5 | External Brain Reference Deployment | **Complete** (local Docker; 145 MB image; dual-brain pair via compose; auth mandate documented) | L |
| S6-DB-6 | (stretch) Python SDK A2A Helper | Not started | M |
| S6-DB-7 | Ecosystem Brain at Session Root | **Complete** (bootstrap shipped in earlier session; ecosystem .claude/ registry + 6 domains + sync-ecosystem skill + CLAUDE.md) | L |

**Stage 6 is DONE when:**
- [ ] `neurogrim-a2a` crate exists and passes `cargo test`
- [ ] `neurogrim-core/src/ecosystem.rs` dispatches on `ChildTransport` (subprocess / A2A)
- [ ] Parent Brain produces identical ecosystem score across both transports
- [ ] `neurogrim a2a-serve` CLI subcommand serves this Brain as an A2A peer
- [ ] Dual brain pair integration test passes in CI
- [ ] External Brain reference deployment documented (one working example)
- [ ] No MCP imports on the dual-brain code path (boundary enforcement via grep)

**Scope:**
- New Rust crate: `neurogrim/crates/neurogrim-a2a` (Agent Card, envelope, task client/server, transport)
- Refactor `neurogrim-core/src/ecosystem.rs` (currently 3-line stub) with `ChildTransport` enum
- CLI additions: `neurogrim a2a-serve`, `neurogrim a2a-invoke`, `neurogrim a2a-discover`
- External Brain reference deployment (Cloud Run or GitHub Action)
- Shared state synchronization semantics unchanged — A2A carries messages, not state

---

## Stage 7: Agent Behavior Verification

**Goal:** Close the loop §14.8 opened. Skills, hats, and culture are
declarations; this stage delivers their verification surface as a
regular CMDB-backed domain. Non-deterministic AI grades non-
deterministic AI against authored rubrics; scores are distributional;
refinement is human-gated via a feedback ledger.

**Depends on:** Stage 5 (Cultural Substrate, proposal ledger), S6-DB-5
(claude-proxy operational).

| Epic | File | Priority | Status | Stories |
|------|------|----------|--------|---------|
| Agent Behavior Verification | `epics/S7-agent-behavior-verification.md` | Medium | Planning complete (2026-04-21) | S7-ABV-1, S7-ABV-2, S7-ABV-3, S7-ABV-4, S7-ABV-5, S7-ABV-6, S7-ABV-7 |

| Story | Name | Status | Effort |
|-------|------|--------|--------|
| S7-ABV-1 | Methodology + schemas (spec §15, VISION #19, METH-EV §11, 2 schemas) | In progress (schemas + spec draft landed) | S |
| S7-ABV-2 | Harness MVP (`agent-behavior-runner/` Python + `abv-run` CLI + tests) | Not started | M |
| S7-ABV-3 | Five v1 scenarios + gold samples (lsp-code / lsp-brain / hat / culture / honest) | Not started | M |
| S7-ABV-4 | Brain integration (registry + `neurogrim cast agent-behavior` dispatch) | Not started | S |
| S7-ABV-5 | Feedback ledger + `refine-agent-behavior.md` skill | Not started | S |
| S7-ABV-6 | Operator docs + worked example + `write-agent-behavior-scenario.md` | Not started | S |
| S7-ABV-7 | e2e-sim scenario 11 + ecosystem wiring + CEO-template stub | Not started | S |

**Stage 7 is DONE when:**
- [ ] LSP-Brains spec v2.3 ships with §15, VISION #19, METH-EV §11.
- [ ] `agent-behavior-runner/` ships with green pytest suite.
- [ ] Five scenarios' gold samples: judge within ±10 of human labels.
- [ ] Ecosystem + NeuroGrim Brains both score `agent-behavior` (advisory).
- [ ] Feedback ledger operational; worked example shows a score delta ≥ 5 points after skill refinement.
- [ ] e2e-sim scenario 11 green.

**Explicit non-goals (deferred to a future epic):**
- Promoting `agent-behavior` past advisory weight.
- Multi-judge consensus / cross-model judges.
- Automatic skill editing.
- Execution-based rubrics (v1 grades stated intent).
- Continuous (per-PR) runs — on-demand + documented weekly cadence.

**Scope:**
- New Python package `D:/Brains/agent-behavior-runner/` (sibling to claude-proxy).
- Two new JSON schemas in LSP-Brains.
- Spec chapter §15 + VISION principle #19 + METHODOLOGY-EVOLUTION §11.
- Five scenario YAMLs + gold samples.
- Two new skills: `refine-agent-behavior.md`, `write-agent-behavior-scenario.md`.
- Thin Rust CLI wrapper in `neurogrim-cli` (subprocess dispatch to `abv-run`).
- e2e-sim scenario 11 for harness-for-the-harness coverage.

---

## Archived Epics

These epics were part of the original plan but were superseded by the 2026-04-09
adversary review. They are kept for historical context. The physical files were moved to
`D:\Brains\archive\Moth-er-Br-AI-n\roadmap-epics-archived\` on 2026-04-17; redirect
stubs remain at each original location.

| Original Epic | Archived Location | Status | Replacement |
|---------------|-------------------|--------|-------------|
| `epics/S2-multi-project.md` | `archive/.../S2-multi-project.md` | Archived | Split → `S2-interface-contract.md` + `S4-fractal-composition.md` |
| `epics/S3-multi-agent.md` | `archive/.../S3-multi-agent.md` | Eliminated | Problem already solved by Stage 1 hat system + filesystem |
| `epics/S4-prescriptive-autonomy.md` | `archive/.../S4-prescriptive-autonomy.md` | Archived | Promoted → `S3-prescriptive-autonomy.md` |

---

## Continuous Activities

These activities are not gated to any specific stage — they happen throughout:

- **Whitepaper updates:** Each stage's completion adds/updates a section in `whitepaper/WHITEPAPER.md`
- **Brain decomposition maintenance:** Keep extracted files clean as new crates and sensory tools are added
- **Test coverage:** Every new Brain feature gets Rust tests (`cargo test`) and Python SDK features get pytest tests
- **Hat integration:** Every new skill, hook, or Brain consumer includes hat context.
  New skills that invoke Brain commands specify the appropriate `-Hat` parameter. New hooks
  that suggest personas also suggest paired hats. See `hats.md` for the hat-aware skill
  usage pattern and hat-persona pairing table.
- **Terminology governance:** Every new term introduced in epics, skills, hooks, or Brain
  features must be added to `.claude/terminology-catalog.json`. Compliance scan
  must stay above 90% before stage transitions.
  Hat-persona pairings must be reviewed when new hats or personas are introduced.

---

## How to Use This Roadmap

- **Before planning work:** Check which stage and epic the work advances. If none, ask
  whether it should. Use `north-star.md` for alignment checks.
- **After completing a story:** Update status in the epic file.
- **After completing an epic:** Update status here and check stage transition criteria.
- **When adding new work:** Add a story to an existing epic or create a new epic file.
- **When prioritizing:** Stage 2 is next. Later stages use `--` until prerequisites are met.
  See `DEPENDENCIES.md` for the critical path.

This roadmap is the source of truth until replaced by Jira.
