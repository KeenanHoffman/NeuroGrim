---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Interface Contract + Framework Extraction

**Stage:** 2 — Interface Contract
**Status:** In progress (S2-IC-1, S2-IC-2, S2-IC-3, S2-IC-4 complete)
**Priority:** --
**Depends on:** S1-honest-scoring, S1-diagnostic-reasoner, S1-learning-brain, S1-context-aware-agent
**Blocks:** S3-prescriptive-autonomy, S4-fractal-composition, S5-transferable-practice
**Stage gate:** Satisfies all Stage 2 transition criteria
**Supersedes:** S2-multi-project.md (archived), S3-multi-agent.md (eliminated)

---

## Summary

The Brain's interface is formalized as a versioned contract. The scoring framework becomes
separable from LaaS-specific domain definitions. Someone can define a new domain by editing
only `brain-registry.json`. The interface contract is LSP Brains' portability
layer — it defines how any implementation's sensory tools talk to the Brain. These are the
prerequisites for both prescriptive autonomy (a consumer must trust the contract to
auto-execute) and fractal composition (a parent must know the child's output shape).

**Why this replaces old S2 + S3:** The adversary review (2026-04-09) found that:
- Cross-project awareness needs an interface contract before it needs wiring
- Multi-agent coordination is already solved by hats + filesystem shared state
- Framework extraction should happen now, not after Stage 4

---

## Stories

### S2-IC-1: Brain Interface Contract — SHIPPED

**Status:** Complete
**Effort:** L
**Depends on:** none (within Stage 2)
**Completed:** 2026-04-10

Formalize the `-Mode agent` JSON output as a versioned schema. This is the contract that
any consumer — human, subagent, parent Brain, or autonomy engine — depends on. The schema
is declared as code and validated in tests.

**Key decisions:**
- Schema format: JSON Schema draft-07 in `.claude/brain/agent-output-schema.json`
- Versioning: `schema_version` field in output (const `"1"`); consumers declare which version they expect
- Breaking changes: new major version; additive changes are minor
- Hat context: schema includes `current_hat` (string, optional) and `suggested_hat`
  (object with hat name + reason + signal count) so consumers know which attentional bias
  shaped the recommendations

**Acceptance criteria:**
- [x] JSON Schema file exists and documents every field in `-Mode agent` output
- [x] A Pester test validates that `-Mode agent` output conforms to the schema
- [x] Schema version is included in every `-Mode agent` invocation
- [x] Schema includes `current_hat` and `suggested_hat` fields
- [x] At least one consumer (gate-completion hook) validates against the schema

---

### S2-IC-2: Domain-Agnostic Scoring — SHIPPED

**Status:** Complete
**Effort:** XL
**Depends on:** S2-IC-1
**Completed:** 2026-04-10

Extract any remaining LaaS-specific logic from the scoring engine. A user should be able
to add a custom domain by editing only `brain-registry.json` — no `.ps1` modifications.

**Key decisions:**
- Minimum domain config: name in `domain_weights`, `scoring_source` in `domain_definitions` (type `function` or `cmdb`)
- CMDB convention: `.claude/<domain>-cmdb.json` with `score` and `updated_at` fields
- Function dispatch: `Get-{PascalCase}Score`/`Confidence` with `scoring_function_prefix` override for legacy names
- Two domains break PascalCase: `gitops-integrity` → `GitOpsIntegrity`, `infrastructure` → `Infra`
- Generic CMDB scorer reads JSON, navigates dot-path to score/updated_at fields
- 12-vs-15 domain bug fixed: Build-Scorecard previously skipped adoption, chains, jira

**Acceptance criteria:**
- [x] A synthetic test domain can be added to brain-registry.json
- [x] The synthetic domain scores correctly via `-Mode score`
- [x] The synthetic domain can be removed — no `.ps1` changes at any point
- [x] At least one synthetic sensory tool is written in a non-PowerShell language (Bash or Python) to validate language-agnostic portability
- [x] Documentation in brain-registry.json explains the domain definition schema

---

### S2-IC-3: Consumer Output Contracts — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** S2-IC-1
**Completed:** 2026-04-10

Define output contracts for each consumer type. The Brain produces one internal
representation; consumers get views tailored to their needs. This is the
communication-as-interface principle (Design Principle #8) made concrete.

**Key decisions:**
- Human consumer: three modes (score/health/recommend) with testable format patterns
- Agent consumer: JSON + `scored_at` freshness timestamp for staleness decay
- Parent Brain consumer: profile of agent contract — same schema, aggregation semantics documented
- Freshness guarantee: single `scored_at` field (ISO 8601 UTC) serves all consumers
- Parent staleness: feeds `scored_at` to `Get-CmdbFreshnessMultiplier` (1d/3d/7d decay)

**Acceptance criteria:**
- [x] Consumer contract documentation exists for 3 consumer types (human, agent, parent)
- [x] Each contract specifies: required fields, optional fields, format, freshness guarantee
- [x] At least one test validates each consumer contract against actual output

---

### S2-IC-4: Truth Separation Architecture — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** S2-IC-1
**Completed:** 2026-04-10

Formalize the three truth layers (source/runtime/derived) in DATA-ARCHITECTURE.md. Classify
every data artifact by truth layer. Gitignore all derived artifacts. Document the CMDB pattern
as the bridge between runtime truth and agent reasoning.

**Key decisions:**
- Source truth: committed files — Terraform configs, skills, hooks, scripts, brain-registry.json
- Runtime truth: external system snapshots — artifact-cmdb.json, deploy-state.json, topology JSONs
- Derived truth: compiled indexes — tag-index.json, adoption-index.json, chain-index.json
- CMDB pattern: external source → snapshot script → local JSON → LSP tool → Brain domain
- `truth_layer` field added to every `data_sources` entry in brain-registry.json
- `compile_command` field on all derived entries documents regeneration
- 6 new data_sources entries added for previously unregistered files

**Acceptance criteria:**
- [x] DATA-ARCHITECTURE.md has a truth layer section with classification of every data file
- [x] All derived artifacts (tag-index.json, adoption-index.json, chain-index.json) are gitignored
- [x] brain-registry.json data_sources entries distinguish derived from source/runtime
- [x] Compile scripts produce fresh derived artifacts on demand; staleness is tracked

---

### S2-IC-5: File Interpretation Registry — CANDIDATE

**Status:** Not started
**Effort:** L
**Depends on:** S2-IC-4

Evolve the file type registry concept in brain-registry.json into a formal interpretation
spectrum. Every file type declares its interpretation_depth: native (language parser),
annotated (metadata comments/frontmatter), compiled_proxy (companion JSON), or derived
(git-tree position). LSP tools declare which depths they support. Gap analysis identifies
file types with no LSP coverage.

**Key decisions:**
- Spectrum levels: native | annotated | compiled_proxy | derived
- Registry location: brain-registry.json `file_type_registry` (extend existing concept)
- Gap analysis: Find-SkillSymbol.ps1 `-Check` reports interpretation coverage
- Meta-proxy pattern: for file types that can't carry comments (JSON, images, binaries)

**Acceptance criteria:**
- [ ] brain-registry.json file_type_registry entries include interpretation_depth field
- [ ] Find-SkillSymbol.ps1 -Check reports interpretation coverage (file types with/without LSP)
- [ ] At least one compiled meta-proxy pattern is documented
- [ ] Gap analysis identifies file extensions in the repo without an LSP tool

---

## Epic Completion Criteria

- [x] `-Mode agent` output validates against a declared JSON schema
- [x] A synthetic test domain can be added/scored/removed via registry-only changes
- [x] Output contracts are documented and tested for 3 consumer types
- [x] No LaaS-specific domain names are hardcoded in scoring.ps1 or correlation.ps1
- [x] Truth separation formalized: source/runtime/derived artifacts classified
- [ ] File interpretation spectrum documented with gap analysis

## North Star Check

- Does this make the pattern more general? **Critical** — this is where the Brain stops
  being a LaaS tool and starts being a framework.
- Does this make the ecosystem Brain easier? **Yes** — the interface contract IS the
  composition protocol. Once it exists, wiring parents to children is mechanical.
