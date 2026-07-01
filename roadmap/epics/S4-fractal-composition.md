---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Fractal Composition

**Stage:** 4 — Fractal Composition
**Status:** Complete (all stories complete)
**Priority:** --
**Depends on:** S2-interface-contract (interface + schema), S3-prescriptive-autonomy (parent needs teeth)
**Blocks:** S5-transferable-practice
**Stage gate:** Satisfies all Stage 4 transition criteria
**Supersedes:** S2-multi-project.md fractal stories, S5-TP-1 (Fractal Architecture), S5-TP-5 (Ecosystem Brain)

---

## Summary

Parent Brain consumes child Brain scores via the interface contract established in Stage 2.
Cross-project incident patterns fire. The fractal architecture from VISION.md becomes real.

**Why this merges old S2 + S5 fractal work:** The adversary review (2026-04-09) found
significant overlap between S2-MP-1 (Cross-Project Dependency Graph) and S5-TP-1 (Fractal
Architecture), and between S2-MP-2/S2-MP-3 and S5-TP-5 (Ecosystem Brain). Building these
separately would mean building the same thing twice. The communication-as-interface principle
(Design Principle #8) reveals why: the parent Brain is just another consumer of the child's
`-Mode agent` output. The interface contract (S2) is the composition protocol.

**Why this comes after autonomy (S3):** A parent Brain that can only observe but not act is
a dashboard. Prescriptive autonomy gives the parent Brain the ability to auto-execute safe
operations on children and escalate risky ones to the human.

---

## Stories

### S4-FC-1: Child Brain Registration

**Status:** Complete
**Effort:** L
**Depends on:** S2-IC-1 (interface contract must exist)
**Completed:** 2026-04-10

Declare child projects in `ecosystem-registry.json`. Each entry is a path to a child Brain
script + its interface version + dependency relationships. Adding a child project is a PR.

**Key decisions:**
- Registry location: `.claude/brain/ecosystem-registry.json`
- Minimal child entry: `{ "project_id": "...", "brain_path": "...", "interface_version": "1.x", "depends_on": [] }`
- Discovery model: parent pulls child scores on demand (not event-driven)
- Prototype: two local directories within LaaS pretending to be separate projects
- Schema: `meta.schema_version` + `children` object keyed by project_id
- Each child: `display_name`, `brain_path` (repo-relative), `interface_version`, `depends_on`, `weight`, `enabled`
- New module: `ecosystem.ps1` with `Get-EcosystemChildren` and `Get-EcosystemExecutionOrder`
- Topological ordering: Kahn's algorithm with cycle fallback (same pattern as Invoke-Propose)
- Validate check 12: validates ecosystem registry schema, child paths, dependency references
- Synthetic children: child-alpha (no deps) and child-beta (depends on child-alpha) as stubs

**Acceptance criteria:**
- [x] `ecosystem-registry.json` schema defined and validated by tests
- [x] At least 2 child entries registered (can be synthetic/test projects)
- [x] Parent can enumerate children and their dependency relationships
- [x] Adding/removing a child requires only a registry edit (no `.ps1` changes)

---

### S4-FC-2: Parent Score Aggregation

**Status:** Complete
**Effort:** XL
**Depends on:** S4-FC-1
**Completed:** 2026-04-10

Parent Brain runs child Brains via their registered paths, consumes `-Mode agent` output
(validated against the interface contract schema), and produces an ecosystem-level unified
score. Child staleness is handled via the existing CMDB freshness multiplier pattern —
a stale child score drags the parent's confidence down.

**Key decisions:**
- Aggregation model: weighted average of child unified scores, with per-child weight in registry
- Confidence propagation: recursive — parent confidence = f(own confidence, child confidences)
- Child staleness: apply `Get-CmdbFreshnessMultiplier` to child score age
- New mode: `-Mode ecosystem` (separate from `-Mode agent`; agent = own score, ecosystem = aggregated)
- New file: `modes-ecosystem.ps1` with `Invoke-Ecosystem` (follows modes-display/modes-agent decomposition)
- Aggregation functions in `ecosystem.ps1`: `Invoke-ChildBrain`, `Get-EcosystemScore`
- Children array added to `agent-output-schema.json` as optional property
- Each child entry: project_id, score, scored_at, confidence, weight, freshness_multiplier, effective_score, domains, status
- Child status enum: ok, error, stale, disabled — error children excluded from weighted average
- Validate check 13: child output conformance (invokes children, validates against schema)
- Synthetic children produce deterministic scores: child-alpha=72, child-beta=58

**Acceptance criteria:**
- [x] Parent Brain produces ecosystem-level score from child scores
- [x] Child staleness reduces parent confidence (freshness multiplier applied)
- [x] Ecosystem score output conforms to the interface contract (recursive)
- [x] A test validates parent aggregation with 2 synthetic children

---

### S4-FC-3: Cross-Project Incident Patterns

**Status:** Complete
**Effort:** L
**Depends on:** S4-FC-1, S4-FC-2, S1-DR (composable conditions)
**Completed:** 2026-04-10

Incident patterns that reference child Brain domain variables. The correlation engine
evaluates conditions like "child_a:gates:dirty AND child_b:artifacts:stale" to detect
dependency cascades.

**Key decisions:**
- Variable namespace: `child.<project_id>.<domain>:<variable>` for cross-project references
- Pattern location: parent's `brain-registry.json` incident_patterns section
- Zero correlation engine changes: `Invoke-Condition` reads from `$script:domainVars` by key;
  cross-project variables are populated with `child.` prefix — engine evaluates transparently
- Children export `domain_variables` in agent output; parent merges them after invocation
- Convenience variables: `child.<id>:status`, `child.<id>:score`, `child.<id>:confidence`
- `Test-IncidentPatterns` moved to after child variable population in ecosystem mode
- `Get-AffectedChildren` extracts child project IDs from condition variable references
- Validate check 14: verifies cross-project pattern variable references map to registered children
- Temporal cross-project patterns deferred — snapshot history assumes single-project GCS snapshots
- Two patterns: `child_dependency_cascade` (positive case — fires) and `ecosystem_health_degradation` (negative case — does not fire with healthy children)

**Acceptance criteria:**
- [x] Cross-project incident patterns can be defined in parent registry
- [x] At least one cross-project pattern fires correctly with synthetic children
- [x] Pattern evaluation uses the composable condition engine from S1-DR
- [x] Cross-project incident alerts include affected child project IDs

---

### S4-FC-4: External State as First-Class Citizen

**Status:** Complete
**Effort:** L
**Depends on:** S2-IC-4 (truth separation), S2-IC-1 (interface contract)
**Completed:** 2026-04-11

Formalize the CMDB pattern (external source → snapshot script → local JSON → LSP tool →
Brain domain) as a first-class architectural element. Jira becomes the second runtime-truth
source (after Terraform state). Any external system follows the same pattern.

**Key decisions:**
- CMDB pattern formalized as a 4-step recipe in DATA-ARCHITECTURE.md
- Each runtime-truth source follows: snapshot script + CMDB JSON + LSP tool + brain-registry entry
- Jira CMDB populated with synthetic data (6 issues, 1 blocked, 2 epics, 1 sprint)
- Pattern is language-agnostic: test-synthetic-probe.py (Python) follows same conventions as update-artifact-cmdb.ps1 (PowerShell)
- 7 CMDB instances inventoried across PowerShell and Python
- Meta envelope schema documented: `{ "meta": { "schema_version", "updated_at", "updated_by", "source" } }`
- Freshness decay via `Get-CmdbFreshnessMultiplier` applied to all runtime CMDBs

**Acceptance criteria:**
- [x] CMDB pattern is documented as a reusable pattern in DATA-ARCHITECTURE.md
- [x] At least 2 runtime truth sources use the pattern (Terraform state + Jira)
- [x] The pattern is language-agnostic (Jira scaffold follows the same conventions)
- [x] Adding a new external source requires only: snapshot script + CMDB JSON + LSP tool + brain-registry entry

---

## Epic Completion Criteria

- [x] Cross-project dependency graph is declared as code in ecosystem-registry.json
- [x] Parent Brain consumes child Brain scores via the interface contract
- [x] Ecosystem-level unified score is produced with recursive confidence
- [x] At least one cross-project incident pattern fires correctly
- [x] The fractal pattern works at 2 levels (parent + children)
- [x] CMDB pattern formalized as a reusable runtime-truth integration pattern

## North Star Check

- Does this make the pattern more general? **Critical** — this is where the pattern proves
  it scales beyond a single project.
- Does this make the ecosystem Brain easier? **This IS the ecosystem Brain.**
