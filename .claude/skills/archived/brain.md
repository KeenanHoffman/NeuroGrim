# NeuroGrim

Role: diagnostic · reference · meta
Governs: scripts/dev/Find-Brain.ps1, .claude/brain-registry.json

Trigger phrases: "brain", "neurogrim", "moth(er):br+ai+n", "system health", "unified health",
Domain: brain
Methodology-step: skills
"cross-domain", "tool registry", "domain score", "what's the overall health", "brain score",
"recommend actions", "prioritized action list", "brain v2", "principle score",
"gitops integrity", "defense in depth score", "blast radius score", "eac coverage",
"everything as code coverage", "least privilege score"

---

## What It Is

The Brain is Step 5 of LSP Brains — the central nervous system that integrates
signals from all sensory tools into a unified health picture.

**NeuroGrim** is the intelligence layer that sits above all other LSP tools. Where
`Find-SessionContext.ps1` answers "what should I do before commit?", NeuroGrim answers
"how healthy is the whole system right now, and what should be fixed first?"

NeuroGrim v2 does four things:
1. **Scores** each domain 0-100 against the 8 DevOps whitepaper principles with a weighted
   unified score and **confidence percentage** per domain, so you can see both system health
   and how much to trust each score.
2. **Correlates** across domains via **domain variable interfaces** — a dirty drift gate caps
   gates confidence; many deploy-blocking dirty gates lower artifacts confidence. These
   connections don't surface in any single CMDB.
3. **Diagnoses** EaC coverage (`-Mode eac`): which files and scripts lack skill-level governance.
4. **Registers** all `Find-*.ps1` tools with their file-type mappings and domain variable exports,
   discoverable and invocable through the brain rather than hardcoded paths.

NeuroGrim complements `Find-SessionContext.ps1` — they are not redundant:

| Tool | Driven by | Answers |
|------|-----------|---------|
| `Find-SessionContext.ps1` | Action (commit/deploy/review/debug) | What blocks my next action? |
| `Find-Brain.ps1` | Scoring + correlation | How healthy is the whole system? |

---

## Principle Alignment

V2 domain weights are derived from the 8 DevOps principles in `devops-philosophy.md`.
The score answers not just "can we deploy" but "are we practicing good DevOps."

| Domain | Weight | Principle |
|--------|--------|-----------|
| gates | 25% | Fail Fast / Shift Left |
| artifacts | 15% | Immutable Infrastructure |
| topology | 15% | Observability Before Action |
| gitops-integrity | 15% | GitOps / Single Source of Truth |
| defense-in-depth | 10% | Defense in Depth |
| least-privilege | 10% | Least Privilege |
| everything-is-code | 5% | Everything as Code |
| supply-chain | 5% | Security Everywhere |
| infrastructure | 0% advisory | IaC |
| git-tree | 0% advisory | — |

The score will be lower on first run v2 than v1 because `gitops-integrity` (15%) and
`defense-in-depth` (10%) start with `needs-run` gates — honest signal, not regression.

---

## Quick Reference

```powershell
# System health score in one line (used by gate-completion.sh hook)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode score -Plain

# Full multi-domain health report (principle-aligned)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode health -Plain

# Prioritized action list, ranked by urgency
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -Plain
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -All -Plain

# EaC coverage diagnostics: governed vs. ungoverned files
pwsh -File scripts/dev/Find-Brain.ps1 -Mode eac -Plain

# Cross-domain impact for a specific resource
pwsh -File scripts/dev/Find-Brain.ps1 -Mode impact -Domain gates -Name pester:deploy -Plain
pwsh -File scripts/dev/Find-Brain.ps1 -Mode impact -Domain least-privilege -Name <binding-id> -Plain
pwsh -File scripts/dev/Find-Brain.ps1 -Mode impact -Domain gitops-integrity -Name drift:all -Plain
pwsh -File scripts/dev/Find-Brain.ps1 -Mode impact -Domain defense-in-depth -Name before-merge -Plain

# Tool registry: tools, file type registry, domain variable exports
pwsh -File scripts/dev/Find-Brain.ps1 -Mode registry -Plain

# Safe tool invocation via registry
pwsh -File scripts/dev/Find-Brain.ps1 -Mode invoke -Tool Find-GateSymbol.ps1 -Args "-Check -Plain"

# File-based impact analysis
pwsh -File scripts/dev/Find-Brain.ps1 -Mode impact -Change 'terraform/modules/web-app/main.tf' -Plain

# Full agent analysis (JSON: score, domains, recommendations, correlations, incidents)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode agent -Plain
pwsh -File scripts/dev/Find-Brain.ps1 -Mode agent -Persist -Plain  # also persists snapshot to GCS

# Historical score trajectory from GCS snapshots
pwsh -File scripts/dev/Find-Brain.ps1 -Mode trend -Plain

# Structured remediation proposals with commands, blast radius, and dependency ordering
pwsh -File scripts/dev/Find-Brain.ps1 -Mode propose -Plain

# Multi-step execution plan with topologically sorted waves
pwsh -File scripts/dev/Find-Brain.ps1 -Mode plan -Plain

# Session context with brain score prepended
pwsh -File scripts/dev/Find-SessionContext.ps1 -Brain -Plain
```

### Hat-Aware Quick Reference

Add `-Hat` to any mode to apply domain emphasis. The score doesn't change — only
recommendation priority and output ordering shift. See `hats.md` for full details.

```powershell
# Operator-focused (deploy readiness: gates x2.0, artifacts x1.5)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -Hat operator -Plain

# Security-focused (access control: least-privilege x2.0, supply-chain x2.0)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -Hat security -Plain

# Architect-focused (structural health: EaC x2.0, topology x1.5)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -Hat architect -Plain

# Check which hat the Brain suggests based on current signals
pwsh -File scripts/dev/Find-Brain.ps1 -Mode health -Plain
# → look for "Hat suggestion:" line at the bottom

# Agent mode with hat context (includes active_hat + suggested_hat in JSON)
pwsh -File scripts/dev/Find-Brain.ps1 -Mode agent -Hat operator -Plain
```

---

## Mode Details

### `-Mode score`
Fast path. Single output line. Used by `gate-completion.sh` after every gate run.
Uses fast scoring (trusts stored CMDB status, partial skill reads) for speed.
Supply-chain and workflow corpus checks are skipped (confidence=0/50).

```
NeuroGrim: 64/100 [gates:80(60%) artifacts:30(25%) topology:72(85%) gitops-integrity:53(40%) defense-in-depth:45(100%) least-privilege:70(0%) everything-is-code:55(50%) supply-chain:100(0%) infrastructure:100(0%) git-tree:80(75%)]
```

Each domain shows `score(confidence%)`. A score of `53(40%)` means "below threshold, partial data."

### `-Mode health`
Full breakdown with one section per domain showing clean/dirty/unknown counts and
flagging items that need attention. Includes cross-domain IAM warnings in the topology
section when unreviewed existential bindings are detected. Ends with nudge to `-Mode recommend`.

### `-Mode recommend`
Ranked action list (default top 10, `-All` for full). Grouped by domain with time estimates.
Priority = `tierWeight × ageMultiplier × downstreamImpact`. New domains (gitops-integrity,
defense-in-depth, least-privilege) are surfaced alongside gates and artifacts.

| Factor | Values |
|--------|--------|
| tierWeight | immediate=4, before-merge=3, pre-deploy=2, advisory=1 |
| ageMultiplier | hours_dirty / stale_threshold (floor 1.0, cap 4.0); stale artifact=2.0 |
| downstreamImpact | deploy-blocking=2.0, merge-blocking=1.5, else=1.0; existential=2.0 |

### `-Mode eac`
Everything-as-Code coverage diagnostics. Reports:
- Overall governance percentage (covered/total files)
- List of ungoverned files (requires git-tree CMDB; degrades gracefully without it)
- Skill governance map: which skill governs which file paths

Powered by `Governs:` fields in skill files + `git-tree-cmdb.json` xref entries.
Line-range resolution (`Find-EaCSymbol.ps1`) is Phase 3.

### `-Mode impact -Domain <d> -Name <n>`
Cross-domain impact analysis for a single resource. Supported domains:
`gates`, `artifacts`, `topology`, `least-privilege`, `gitops-integrity`,
`defense-in-depth`, `everything-is-code`, `infrastructure`

### `-Mode registry`
Lists all registered `Find-*.ps1` tools with domain and `[EXISTS]`/`[MISSING]` disk check.
Also shows the **file type registry** (extension → LSP tool + tier) and
**domain variable exports** (which domains export cross-domain signals).

### `-Mode invoke -Tool <file> -Args <flags>`
Validates that `<file>` is registered and on disk, then invokes via `pwsh -NonInteractive`.
Passes exit code through. Prevents running arbitrary scripts not in the registry.

### `-Mode impact -Change <filepath>`
File-based impact analysis. Given a repo-relative filepath, determines which gates, domains,
topology resources, and skills would be affected by changes to that file. Returns structured
JSON with `blast_radius` (low/medium/high) and a recommendation.

### `-Mode agent` / `-Mode agent -Persist`
Full agent analysis: computes unified score (confidence-weighted), domain map with `score`,
`effective_score`, `confidence`, and `weight` per domain, dirty gates, stale artifacts,
domain variables, top recommendations (with `depends_on`), fired correlations, and incident
patterns. With `-Persist`, also saves the score snapshot to GCS.

### `-Mode trend`
Historical score trajectory from GCS snapshots. Returns score deltas, worst domain, recurring
incidents, and a recommendation. Degrades gracefully to `no-data` when GCS is unreachable.

### `-Mode propose`
Structured remediation proposals. For each recommendation, provides exact command, `writes_to`
declarations, estimated duration, dependency ordering, and risk level. Proposals are
suggestions only — the agent must present them to the user and get confirmation before
executing. After each proposal completes, re-run `-Mode score` to verify the expected outcome.

### `-Mode plan`
Multi-step execution plan from propose output. Groups proposals into parallel execution waves
using topological sorting (Kahn's algorithm). Returns wave structure with critical path
estimate. Falls back to priority ordering if dependency cycles are detected.

---

## Scoring Formula

| Domain | Weight | Principle | Earns 100 when | Fast mode |
|--------|--------|-----------|----------------|-----------|
| gates | 25% | Fail Fast / Shift Left | All code/deploy gates clean and not expired | full |
| artifacts | 15% | Immutable Infrastructure | All images built from committed source | full |
| topology | 15% | Observability Before Action | All resources annotated and tracked | full |
| gitops-integrity | 15% | GitOps / SSOT | All 5 drift+topology gates clean and fresh | full |
| defense-in-depth | 10% | Defense in Depth | All 3 tier types have ≥1 fresh clean gate | full |
| least-privilege | 10% | Least Privilege | No unreviewed bindings; zero blast-radius penalty | full |
| everything-is-code | 5% | Everything as Code | All skills have `Governs:` + workflow corpus clean | skills only |
| supply-chain | 5% | Security Everywhere | No critical/high vulns | returns 100 (not run) |
| infrastructure | 0% | IaC (advisory) | All TF modules initialized | advisory |
| git-tree | 0% | (advisory) | ≥2 source types per entry | advisory |

**Gate point values** (gates domain): clean (not expired)=100, expired-clean=60,
needs-run=70, dirty=0 + age penalty (cap 30 pts total).

**GitOps gate scoring**: clean (fresh)=100, clean (expired)=60, needs-run=40, dirty=10.
Average of 5 SSOT gate scores: `drift:all`, `network:drift`, `access:drift`,
`network:topology`, `access:topology`.

**Defense-in-Depth tier weights**: immediate=40%, before-merge=35%, pre-deploy=25%.
Score = weighted sum of `(clean_in_tier / total_in_tier) × 100` per tier.

**Least-Privilege penalties** (per unreviewed binding, absent annotation = needs review):
existential=30, critical=20, high=10, medium/low/absent=5. Score = max(0, 100 - total_penalty).

**Confidence** reflects data completeness and CMDB freshness. Domain variable interfaces
enrich confidence cross-domain: dirty drift gate caps gates confidence at 70%;
3+ deploy-blocking dirty gates lowers artifacts confidence by 15%.

### Honest Scoring (S1-HS)

The unified score is **confidence-weighted**. Raw scores alone are dishonest — a domain
returning 100 at 0% confidence contributes nothing, not 100.

**Two models** compute `effective_score` from `(raw_score, confidence)`:

- **Multiplier** (default): `effective = floor(raw × confidence / 100)`. Used by diagnostic
  modes (score, health, agent). A 90 at 20% confidence contributes 18.
- **Floor**: if `confidence < threshold` (default 30%), cap effective at `ceiling` (default 30).
  Used by action-oriented modes (recommend, propose, plan). Binary signal: collect more data.

**Unified formula**: `unified = sum(effective_score_i × weight_i)` where weights are from
`brain-registry.json → config.domain_weights`. Weights sum to 1.0.

**Key property**: a fully-observed system scoring 70 outranks a partially-observed system at 85.
Running a drift check visibly improves the score because confidence rises.

**Configuration** (`brain-registry.json → config.scoring`):
- `model`: `"multiplier"` or `"floor"` (default: multiplier)
- `floor_confidence_threshold`: confidence below this triggers the floor cap (default: 30)
- `floor_score_ceiling`: maximum effective score when below threshold (default: 30)

### Advisory Domain Promotion

Domains with `weight=0` are **advisory** — they appear in output for visibility but don't
affect the unified score. The `advisory_domains` array in `brain-registry.json` explicitly
lists them.

**Promotion criteria** (all must be met to move from advisory to scored):

1. **Stable confidence**: The domain consistently achieves ≥50% confidence in normal
   operation (not just after a fresh run). Checked over 10+ score snapshots.
2. **Actionable score**: The score function produces meaningfully different values (not
   always 100 or always 0). A domain that never varies provides no signal.
3. **Gate coverage**: At least one gate watches the domain's data source, so staleness
   is detectable and the score can degrade honestly.
4. **Human decision**: Promotion is a manual choice recorded in a PR. The PR updates
   `domain_weights` (assign weight > 0, reduce others proportionally to keep sum = 1.0)
   and removes the domain from `advisory_domains`.

**Demotion** back to advisory is allowed via the same PR process. Set weight to 0 and add
the domain back to `advisory_domains`.

**Current advisory domain evaluation:**

| Domain | Confidence? | Actionable? | Gate? | Verdict |
|--------|-------------|-------------|-------|---------|
| infrastructure | Low (25% — TF state rarely refreshed) | Partial (30 for not-initialized, 100 for initialized) | No dedicated gate | **Not ready** — needs a `drift:terraform` or `smoke:terraform` gate |
| git-tree | High (100% when fresh, decays by age) | Yes (0-100 based on xref coverage) | Yes (`git-tree:xref-coverage`) | **Candidate** — meets criteria 1-3; pending human decision on weight |

---

## Domain Variable Interfaces

V2 introduces read-only domain variable exports consumed by other domains' **confidence**
calculations (never raw scores — prevents circular dependencies):

| Variable | Exported by | Consumed by |
|----------|-------------|-------------|
| `gates:deploy_blocking_count` | gates | artifacts confidence |
| `gates:any_gate_dirty` | gates | (advisory) |
| `gitops-integrity:drift_gate_dirty` | gitops-integrity | gates confidence |
| `gitops-integrity:drift_needs_run` | gitops-integrity | (advisory) |
| `least-privilege:unreviewed_existential` | least-privilege | topology IAM warning |
| `least-privilege:total_penalty` | least-privilege | health display |
| `artifacts:stale_count` | artifacts | (advisory) |
| `artifacts:any_stale` | artifacts | (advisory) |
| `artifacts:oldest_stale_hours` | artifacts | temporal conditions |

---

## Temporal Conditions (S1-DR-2)

Incident patterns can use temporal operators that reason about historical state:

| Operator | Format | Data Source | Fast Mode |
|----------|--------|-------------|-----------|
| `duration_above` | `{ "duration_above": { "var": "domain:var", "threshold": N, "hours": H } }` | GCS snapshots | Skipped |
| `delta_in_window` | `{ "delta_in_window": { "var": "domain:var", "delta": N, "snapshots": S } }` | GCS snapshots | Skipped |
| `recurrence_count` | `{ "recurrence_count": { "pattern": "pattern_id", "count": N, "days": D } }` | Local ledger | Works |

**Graceful degradation:** Patterns requiring GCS snapshots (`duration_above`, `delta_in_window`)
are listed in `skipped_temporal` when no snapshot history is available. `recurrence_count` uses
the local incident ledger and always evaluates. Temporal context is included in the narrative
via `[temporal: ...]` when temporal operators fire.

---

## brain-registry.json (v2 schema)

Located at `.claude/brain-registry.json`. Schema version: `"2"`.

Top-level sections:
- `config.domain_weights` — 10-domain principle-aligned weights (sum=1.0)
- `config.principle_map` — domain → DevOps principle name
- `config.scoring` — `age_penalty_cap`, `stale_threshold_hours`
- `config.domain_variables` — exported variable schema per domain
- `config.file_type_registry` — extension → `{ lsp_tool, analyzer, tier, status }`
- `tools` — all `Find-*.ps1` with path, domain, modes
- `data_sources` — all CMDB files with path
- `correlations` — 7 cross-domain correlation rules (documentation)

The `file_type_registry` maps file extensions to LSP tools:
`.ps1→Find-Symbol.ps1(A)`, `.tf→Find-TFSymbol.ps1(A)`, `.ts/.tsx→Find-TSSymbol.ps1(C)`,
`.py→Find-PySymbol.ps1(C)`, `.sh→Find-ShellSymbol.ps1(A)`, `.yml→Find-WorkflowSymbol.ps1(B)`.
Entries marked `status:"planned-phase2"` (nginx.conf, Dockerfile, .mmd) are forward
declarations — tools exist in the registry before their scripts are written.

---

## Hook Integration

`gate-completion.sh` runs `-Mode score` after every gate command:
```
[moth(er):br+ai+n] NeuroGrim: 64/100 [gates:80(60%) artifacts:30(25%) topology:72(85%) gitops-integrity:53(40%) defense-in-depth:45(100%) least-privilege:70(0%) everything-is-code:55(50%) supply-chain:100(0%) infrastructure:100(0%) git-tree:80(75%)]
```

The hook uses `|| true` so a missing `Find-Brain.ps1` degrades gracefully.

---

## See Also

- `lsp.md` — full LSP tool reference with all Find-*.ps1 quick reference commands
- `lsp-grounded.md` — workflow patterns for using LSP tools together
- `devops-philosophy.md` — the 8 principles that define v2 domain weights
- `artifact-cmdb.md` — artifact freshness scoring detail
- `gate-status.md` — gate advisor output and BLOCKED verdict handling
- `session-recap.md` — morning startup sequence (Brain is a natural fit here)
- No companion hook needed for `-Mode eac` (evaluated 2026-04-08).
