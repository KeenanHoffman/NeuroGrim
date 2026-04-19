# Dependency Graph

**Last updated:** 2026-04-17
**Revision:** Added S5-TP-9 (Cultural Substrate), S5-TP-10 (LSP-Brains Brain), S6-DB-7
(Ecosystem Brain at Session Root). Earlier same-day: Added S5-TP-8 (spec v2.1 with
hybrid MCP+A2A) + S6 rescoped as "Dual Brain via A2A" with 6 stories. 2026-04-11:
Updated for LSP Brains reframe. S2-S4 marked complete. S5 expanded to 7 stories.

---

## Critical Path

The longest chain determines minimum time-to-completion for each stage.

**Stages 1-4 (ALL COMPLETE):**
```
Brain decomposition (DONE)
  --> S1 (all 4 epics DONE)
      --> S2 (interface contract DONE)
          --> S3 (prescriptive autonomy DONE)
              --> S4 (fractal composition DONE)
                  --> S5-TP-1 (starter kit DONE)
```

**Stage 5 critical path:**
```
S5-TP-1 (starter kit) [COMPLETE]
    |
    ├──> S5-TP-2 (spec + docs) ──────────┐
    |        |                             |
    |        ├──> S5-TP-5 (personas)       |
    |        └──> S5-TP-7 (dual brain)     |
    |                                      |
    ├──> S5-TP-4 (trajectories) ───────────┤
    |                                      |
    ├──> S5-TP-6 (zero-config) ────────────┤
    |                                      v
    └──────────────────────────────> S5-TP-3 (product delivery)
```

**Shortest path to Stage 5 completion:** S5-TP-2 → S5-TP-3 (sequential, with TP-4/5/6/7 parallel).

**Stage 5 → Stage 6:**
```
S5-TP-7 (dual brain design) ────┐
                                 ├──> S6-DB-1 (neurogrim-a2a crate)
S5-TP-8 (spec v2.1 + schemas) ──┘         |
                                           ├──> S6-DB-2 (ecosystem refactor, needs S4-FC-*)
                                           ├──> S6-DB-3 (Brain A2A server)
                                           |         |
                                           |         v
                                           └──> S6-DB-4 (dual brain pair integration)
                                                     |
                                                     v
                                                S6-DB-5 (external brain ref deployment)

S6-DB-1 ──> S6-DB-6 (stretch: Python SDK A2A helper)
```

---

## Full Dependency Map

```
    Stages 1-4 (ALL COMPLETE)
              |
              v
    S5-TP-1 (starter kit) [COMPLETE]
       /          |          \
      v           v           v
  S5-TP-2    S5-TP-4      S5-TP-6
  (spec)    (trajectory)  (zero-config)
   /    \         |           |
  v      v        |           |
TP-5   TP-7      |           |
(pers) (dual)    |           |
  |      |       |           |
  v      v       v           v
  S5-TP-3 (product delivery + external adoption)
              |
              v
         Stage 6 (dual brain implementation)
```

---

## Cross-Stage Dependencies

### Completed (Stages 1-4)

| From | To | Status |
|------|----|--------|
| S1-honest-scoring | S2-IC-2 | **Complete** |
| S1-diagnostic-reasoner | S4-FC-3 | **Complete** |
| S1-learning-brain | S3-PA-1 | **Complete** |
| S1-context-aware-agent | S3-PA-3 | **Complete** |
| S2-IC-1 | S3-PA-1 | **Complete** |
| S2-IC-1 | S4-FC-1 | **Complete** |
| S2-IC-2 | S4-FC-2 | **Complete** |
| S2-IC-2 | S5-TP-1 | **Complete** |
| S3-PA-1 | S4-FC-2 | **Complete** |
| S4-FC-1 | S5-TP-1 | **Complete** |

### Active (Stage 5)

| From | To | Reason |
|------|----|--------|
| S5-TP-1 (starter kit) | S5-TP-2 (spec) | Spec codifies the starter kit's implementation |
| S5-TP-1 (starter kit) | S5-TP-4 (trajectory) | Trajectory adds to working Brain |
| S5-TP-1 (starter kit) | S5-TP-6 (zero-config) | Zero-config builds on starter kit sensory tools |
| S5-TP-2 (spec) | S5-TP-5 (personas) | Persona protocol defined in spec |
| S5-TP-2 (spec) | S5-TP-7 (dual brain) | Dual brain architecture defined in spec |
| S5-TP-4 (trajectory) | S5-TP-3 (adoption) | Adopters need trajectory to validate over time |
| S5-TP-6 (zero-config) | S5-TP-3 (adoption) | Zero-config lowers adoption barrier |
| S5-TP-7 (dual brain design) | S6-DB-1 (neurogrim-a2a crate) | Design precedes implementation |
| S5-TP-8 (spec v2.1 + schemas) | S6-DB-1 (neurogrim-a2a crate) | Schemas must exist before crate can validate envelopes |
| S5-TP-8 (spec v2.1 + schemas) | S6-DB-2 (ecosystem refactor) | Schema additions for `a2a_endpoint` must be normative |
| S5-TP-8 (spec v2.1 + schemas) | S5-TP-9 (cultural substrate) | Culture schema lands alongside v2.1 publication |
| S5-TP-9 (cultural substrate) | S5-TP-10 (LSP-Brains Brain) | LSP-Brains Brain needs `culture.yaml` copy |
| S5-TP-9 (cultural substrate) | S6-DB-7 (Ecosystem Brain) | Ecosystem Brain carries the third culture.yaml + `culture-coherence` domain |
| S5-TP-10 (LSP-Brains Brain) | S6-DB-7 (Ecosystem Brain) | Ecosystem needs a child Brain to talk to |

### Active (Stage 6 — Dual Brain via A2A)

| From | To | Reason |
|------|----|--------|
| S6-DB-1 (neurogrim-a2a crate) | S6-DB-2 (ecosystem refactor) | Ecosystem dispatches to the A2A crate for A2A children |
| S6-DB-1 (neurogrim-a2a crate) | S6-DB-3 (Brain A2A server) | CLI a2a-serve uses the crate's TaskServer |
| S6-DB-1 (neurogrim-a2a crate) | S6-DB-6 (Python SDK stretch) | Mirrors the Rust crate's interfaces |
| S4-FC-* (fractal composition done) | S6-DB-2 (ecosystem refactor) | Subprocess branch ports from S4 implementation |
| S6-DB-2 (ecosystem refactor) | S6-DB-4 (dual brain pair test) | Pair test exercises ecosystem through A2A transport |
| S6-DB-3 (Brain A2A server) | S6-DB-4 (dual brain pair test) | Both peers run a2a-serve in the integration test |
| S6-DB-4 (pair test) | S6-DB-5 (external deployment) | Deployment depends on verified pair behavior |

---

## S5 Parallelization

| Track A | Track B | Parallel? |
|---------|---------|-----------|
| S5-TP-2 (spec) | S5-TP-4 (trajectory impl) | **Yes** — spec and impl can co-develop |
| S5-TP-2 (spec) | S5-TP-6 (zero-config impl) | **Yes** — independent implementation |
| S5-TP-4 (trajectory) | S5-TP-6 (zero-config) | **Yes** — fully independent |
| S5-TP-5 (personas) | S5-TP-7 (dual brain) | **Yes** — both depend on spec structure only |
| S5-TP-8 (spec v2.1) | S5-TP-3 (external adoption) | **Yes** — spec publication and adopter-facing work are independent |

## S6 Parallelization

| Track A | Track B | Parallel? |
|---------|---------|-----------|
| S6-DB-2 (ecosystem refactor) | S6-DB-3 (Brain A2A server) | **Yes** — independent code paths in different crates; both depend only on S6-DB-1 |
| S6-DB-5 (external deployment) | S6-DB-6 (Python SDK stretch) | **Yes** — orthogonal concerns |

---

## Eliminated Dependencies

These dependencies existed in the original plan but were removed by the adversary review:

| From | To | Why removed |
|------|----|-------------|
| ~~S3-multi-agent~~ | ~~S4-prescriptive-autonomy~~ | S3 eliminated; autonomy doesn't need multi-agent coordination |
| ~~S2-multi-project~~ | ~~S3-multi-agent~~ | Both eliminated/restructured |
| ~~DATA-ARCHITECTURE.md finalized~~ | ~~S2 can begin~~ | Open questions already answered by S1 implementation |

---

## Dependency Rules

1. Stories within an epic may have internal ordering. Check the epic file.
2. Cross-epic dependencies are listed here AND in both epic files (Depends on / Blocks).
3. Cross-stage dependencies always go forward (Stage N blocks Stage N+M, never reverse).
4. If a new dependency is discovered, update this file AND both affected epic files.
5. "Design can precede implementation dependency" means design work on a blocked epic
   can start before its dependency completes, but implementation must wait.
