# Data Architecture

**Last updated:** 2026-04-17

---

## Guiding Principle

Everything the Brain persists must be declarable as code (VISION.md principle 1).
Machine-optimized formats (JSON) for CMDBs the Brain reads programmatically.
Human-readable formats (markdown) only for documents humans read directly.

---

## Data Envelopes (Three Shapes, One Validation Discipline)

Three distinct JSON envelope shapes cross the Brain's boundary. Each has its own schema,
its own validator, and its own role in the architecture. They are composable but MUST NOT
be conflated.

| Envelope | Schema | Role | Boundary |
|----------|--------|------|----------|
| **CMDB Envelope** | `cmdb-envelope-v1.schema.json` | What a sensory tool produces | Sensory tool → Brain (via MCP or file) |
| **Agent Output** | `agent-output-v1.schema.json` | What the Brain produces | Brain → LLM agent (via MCP) OR Brain → peer Brain (via A2A, wrapped) |
| **A2A Envelope** | `a2a-envelope-v1.schema.json` | How peer Brains exchange messages | Brain ↔ Brain (via A2A) |

**Important:** A2A envelopes *wrap* agent output when peer Brains exchange scores. The A2A
envelope's `payload` field carries the agent-output-v1 JSON. Validation happens at both
layers — A2A envelope validates as A2A; payload validates as agent output. They are not
alternatives to each other.

```
┌─── A2A Envelope (a2a-envelope-v1) ──────────────────────────────┐
│  message_id, task_id, timestamp, brain_id, message_type="score.updated"
│  payload:                                                        │
│  ┌─── Agent Output (agent-output-v1) ──────────────────────────┐ │
│  │  schema_version, scored_at, score, domains, gates, ...      │ │
│  │  (Each domain may reference a CMDB file — cmdb-envelope-v1) │ │
│  └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

Agent Cards (`agent-card-v1.schema.json`) are a fourth, static envelope — they are Brain
self-description, not data in motion. Published once at `/.well-known/agent-card.json`;
fetched by peer Brains during discovery.

### When Each Envelope Appears

- **CMDB envelope:** every time a sensory tool runs (MCP `tools/call` response, or a file
  at `.claude/<domain>-cmdb.json`). Truth layer: runtime.
- **Agent Output:** every time the Brain produces a score for a consumer. Returned as the
  body of MCP `get_health_score`, or as the `payload` of an A2A `score.updated` message.
- **A2A envelope:** every message between peer Brains (parent↔child, local↔external).
  Not used for sensory invocation; not used for LLM-facing output.
- **Agent Card:** published once per Brain, fetched by peers at discovery time.

---

## Truth Layers

Every data file the Brain touches belongs to exactly one truth layer. The layer determines
who writes it, whether it's committed, and how staleness is tracked.

### Definitions

| Layer | Definition | Writer | Committed? | Staleness |
|-------|-----------|--------|------------|-----------|
| **Source** | Hand-maintained artifacts that define system behavior. The ground truth for configuration, rules, and schema. | Human (via agent edits) | Yes | N/A — always current by definition |
| **Runtime** | Snapshots of external system state. Accurate at capture time, decays with age. | Sensory tools (`update-*.ps1`, `Find-*.ps1`) | Yes (snapshots) | `updated_at` field; freshness multiplier decay (1d/3d/7d) |
| **Derived** | Compiled from source or runtime artifacts on demand. Reproducible — can be regenerated from inputs. | Compile scripts (`-Compile`, `-Check`) | No (gitignored) | Regenerate on demand; `compile_command` in registry |

### Classification

Every `data_sources` entry in `brain-registry.json` carries a `truth_layer` field.

| Artifact | Path | Truth Layer | Owner Tool |
|----------|------|-------------|------------|
| test-gates.json | `.claude/test-gates.json` | source | Find-GateSymbol.ps1 |
| brain-registry.json | `.claude/brain-registry.json` | source | Find-Brain.ps1 |
| terminology-catalog.json | `.claude/terminology-catalog.json` | source | Find-TerminologySymbol.ps1 |
| skills-dir | `.claude/skills/` | source | Find-SkillSymbol.ps1 |
| github-workflows-dir | `.github/workflows/` | source | Find-WorkflowSymbol.ps1 |
| agent-output-schema.json | `.claude/brain/agent-output-schema.json` | source | — |
| artifact-cmdb.json | `.claude/artifact-cmdb.json` | runtime | Find-ArtifactSymbol.ps1 |
| network-topology.json | `.claude/network/network-topology.json` | runtime | Find-TopoSymbol.ps1 |
| access-topology.json | `.claude/network/access-topology.json` | runtime | Find-TopoSymbol.ps1 |
| git-tree-cmdb.json | `.claude/git-tree-cmdb.json` | runtime | Find-TreeSymbol.ps1 |
| tf-state-cmdb.json | `.claude/tf-state-cmdb.json` | runtime | Find-TFStateSymbol.ps1 |
| jira-cmdb.json | `.claude/jira-cmdb.json` | runtime | Find-JiraSymbol.ps1 |
| deploy-state.json | `.claude/deploy-state.json` | runtime | update-deploy-state.ps1 |
| incident-ledger.json | `.claude/brain/incident-ledger.json` | runtime | Find-Brain.ps1 |
| proposal-ledger.json | `.claude/brain/proposal-ledger.json` | runtime | Find-Brain.ps1 |
| score-history.json | `.claude/brain/score-history.json` | runtime | Find-Brain.ps1 (-Mode agent) |
| tag-index.json | `.claude/tag-index.json` | derived | Find-SkillSymbol.ps1 |
| adoption-index.json | `.claude/adoption-index.json` | derived | Find-SkillSymbol.ps1 |
| chain-index.json | `.claude/chain-index.json` | derived | Find-SkillSymbol.ps1 |
| terminology-cmdb.json | `.claude/terminology-cmdb.json` | derived | Find-TerminologySymbol.ps1 |
| test-synthetic-cmdb.json | `.claude/test-synthetic-cmdb.json` | derived | test-synthetic-probe.py |

### CMDB Pattern as Truth Bridge

The CMDB pattern bridges runtime truth into agent reasoning:

```
external source → sensory tool → local JSON (runtime) → LSP tool → Brain domain
```

Runtime CMDBs are snapshots — they were true when captured but decay with age. The Brain
applies `Get-CmdbFreshnessMultiplier` to `updated_at` timestamps, reducing confidence in
stale snapshots rather than treating them as current fact.

### Derived Artifact Invariants

1. All derived artifacts are gitignored (never committed)
2. Every derived `data_sources` entry has a `compile_command` to regenerate it
3. Source entries never have a `compile_command` (they are hand-maintained)
4. Derived artifacts can be deleted and regenerated without data loss

---

## State Inventory

> **Note:** The Truth Layer column references the classification above.

| State | Owner | Format | Location | Lifecycle | Truth Layer |
|-------|-------|--------|----------|-----------|-------------|
| Domain scores + confidence | existing | JSON | per-invocation (not persisted) | Ephemeral | — |
| Score snapshots | existing | JSON | GCS `brain/scores/` | Append-only, 90d retention | runtime |
| Incident records | existing | JSON | GCS `brain/incidents/` | Append-only, 365d retention | runtime |
| Gate ledger | existing | JSON | GCS `brain/gate-ledger/` | Append-only, 90d retention | runtime |
| Deploy outcomes | existing | JSON | GCS `brain/deploy-outcomes/` | Append-only, 180d retention | runtime |
| Scoring config | existing | JSON | `.claude/brain-registry.json` (config) | Hand-maintained | source |
| Correlation rules | existing | JSON | `.claude/brain-registry.json` (correlations) | Hand-maintained | source |
| Incident patterns | existing | JSON | `.claude/brain-registry.json` (incident_patterns) | Hand-maintained | source |
| Incident recurrence | S1-DR | JSON | `.claude/brain/incident-ledger.json` | Append, auto-prune 30d | runtime |
| Proposal ledger | S1-LB | JSON | `.claude/brain/proposal-ledger.json` | Append, auto-prune 90d | runtime |
| Outcome correlations | S1-LB | JSON | derived from proposal ledger on read | Computed | derived |
| Hat definitions | S1-CA | Markdown | `.claude/skills/hats.md` | Hand-maintained | source |
| Hat observation tags | S1-CA | JSON | co-located in proposal ledger entries | Append with proposals | runtime |
| Terminology catalog | S1 / Ongoing | JSON | `.claude/terminology-catalog.json` | Hand-maintained | source |
| Terminology compliance | Ongoing | JSON | `.claude/terminology-cmdb.json` | Rewritten on `-Check` | derived |
| Tag index | Ongoing | JSON | `.claude/tag-index.json` | Rewritten on `-Compile` | derived |
| Cross-project registry | S2 | JSON | `.claude/brain/ecosystem-registry.json` | Hand-maintained | source |
| Autonomy boundaries | S4 | JSON | `.claude/brain-registry.json` (config) | Hand-maintained | source |
| Score history | S5-TP-4 | JSON | `.claude/brain/score-history.json` | Append, auto-prune 30d | runtime |

---

## Storage Patterns

### Pattern 1: CMDB (structured state)

Structured snapshots of system state. The Brain reads these on every invocation.

- **Format:** JSON with `meta` envelope (`schema_version`, `updated_at`, `updated_by`)
- **Location:** `.claude/` for project-scoped CMDBs
- **Writer:** dedicated `update-*.ps1` script with atomic write (tmp + Move-Item)
- **Reader:** `Find-*.ps1` LSP tool or `Find-Brain.ps1` directly
- **Examples:** `artifact-cmdb.json`, `test-gates.json`, `network-topology.json`

#### Adding a New External Source (CMDB Pattern Recipe)

Any external system can become a Brain domain by following these 4 steps. The pattern is
language-agnostic: `test-synthetic-probe.py` (Python) follows the same conventions as
`update-artifact-cmdb.ps1` (PowerShell). The convention IS the contract, not the language.

**Step 1: Snapshot script** — fetches from the external source and writes local JSON.
- Naming: `update-<name>-cmdb.ps1` (or any language: `.py`, `.sh`, `.ts`)
- Must write atomically (temp file + move) to avoid partial reads
- Must populate the `meta` envelope (see below)
- Can be invoked by CI, hooks, or manually

**Step 2: CMDB JSON** — the local snapshot file.
- Location: `.claude/<name>-cmdb.json`
- Required `meta` envelope:
  ```json
  {
    "meta": {
      "schema_version": "1",
      "updated_at": "2026-04-11T00:00:00Z",
      "updated_by": "update-<name>-cmdb.ps1",
      "source": "description of external source"
    }
  }
  ```
- The `updated_at` field drives freshness decay via `Get-CmdbFreshnessMultiplier`

**Step 3: LSP tool** — query interface for the CMDB.
- Naming: `Find-<Name>Symbol.ps1` (in `scripts/dev/`)
- Standard modes: `-Mode check` (summary), `-Mode name` (single lookup)
- Must include Brain scoring functions: `Get-<Name>Score` and `Get-<Name>Confidence`
- Confidence should decay with CMDB age (read `meta.updated_at`)

**Step 4: brain-registry.json wiring** — register the domain.
- Add domain to `config.domain_weights` (0.00 for advisory, >0 for scored)
- Add to `config.advisory_domains` if advisory
- Add to `config.principle_map`
- Add to `config.domain_definitions` with scoring source type
- Add to `config.domain_variables.<name>.exports`
- Add tool to `config.sensory_tools`
- Add CMDB to `config.data_sources`

**Checklist for a new external source:**
- [ ] Snapshot script exists and writes CMDB JSON with `meta` envelope
- [ ] CMDB JSON is in `.claude/` and is gitignored (or committed if source-of-truth)
- [ ] LSP tool exists with `-Mode check` and scoring functions
- [ ] brain-registry.json has domain weight, tool, data source entries
- [ ] Domain variables are exported for use in incident patterns
- [ ] At least one Pester test validates the CMDB is readable

#### CMDB Instance Inventory

| Instance | Snapshot Script | CMDB JSON | LSP Tool | Brain Domain | Language |
|----------|----------------|-----------|----------|--------------|----------|
| Artifacts | `update-artifact-cmdb.ps1` | `artifact-cmdb.json` | `Find-ArtifactSymbol.ps1` | artifacts | PowerShell |
| Git Tree | `update-git-tree-cmdb.ps1` | `git-tree-cmdb.json` | `Find-TreeSymbol.ps1` | git-tree | PowerShell |
| TF State | `update-tf-state-cmdb.ps1` | `tf-state-cmdb.json` | `Find-TFStateSymbol.ps1` | infrastructure | PowerShell |
| Network | `update-network-topology.ps1` | `network-topology.json` | `Find-TopoSymbol.ps1` | topology | PowerShell |
| Access | `update-access-topology.ps1` | `access-topology.json` | `Find-TopoSymbol.ps1` | topology | PowerShell |
| Jira | `Find-JiraSymbol.ps1 -Sync` | `jira-cmdb.json` | `Find-JiraSymbol.ps1` | jira | PowerShell |
| Test Synthetic | `test-synthetic-probe.py` | `test-synthetic-cmdb.json` | (generic cmdb scorer) | test-synthetic | Python |

### Pattern 2: Ledger (append-only log)

Operational data that accumulates over time. The Brain reads these for trend analysis,
feedback loops, and recurrence tracking.

- **Format:** JSON array (one file, periodically pruned) or JSONL (one entry per line)
- **Location:** `.claude/brain/` directory (NEW — separates operational data from config)
- **Pruning:** auto-prune entries older than retention period on write
- **Writer:** `Find-Brain.ps1` modes or hooks
- **Reader:** `Find-Brain.ps1` trend/propose/agent modes
- **New for:** `proposal-ledger.json`, `incident-ledger.json`

### Pattern 3: Configuration (declared behavior)

Settings and rules that control Brain behavior. Hand-maintained, version-controlled.

- **Format:** JSON section within `brain-registry.json`, or standalone markdown
- **Location:** `.claude/brain-registry.json` or `.claude/skills/`
- **Writer:** human (via agent edits)
- **Examples:** `domain_weights`, `correlation` rules, `gate_tiers`, hat definitions

---

## New Directory: `.claude/brain/`

Persistent Brain state that is NOT configuration and NOT a domain CMDB.
Separates append-only operational data (ledgers) from:
- Hand-maintained config (`brain-registry.json`)
- Domain CMDBs (`.claude/artifacts/`, `.claude/access/`, etc.)
- GCS snapshots (historical, cross-machine)

Contents (planned):
```
.claude/brain/
  proposal-ledger.json     # S1-LB: what was proposed, executed, outcome
  incident-ledger.json     # S1-DR: recurrence tracking for incident patterns
  ecosystem-registry.json  # S4: child project registry
  score-history.json       # S5-TP-4: trajectory intelligence scoring history
```

---

## GCS Structure (existing)

```
gs://laas-489115-brain-snapshots/brain/
  scores/YYYY-MM-DD/score-HH-mm-ss-fff.json       # 90d retention
  incidents/YYYY-MM-DD/incident-HH-mm-ss-fff.json  # 365d retention
  gate-ledger/YYYY-MM-DD/gate-HH-mm-ss-fff.json    # 90d retention
  deploy-outcomes/YYYY-MM-DD/deploy-HH-mm-ss-fff.json  # 180d retention
  trends/YYYY-MM-DD/trend-HH-mm-ss-fff.json        # 30d retention (future)
```

All GCS operations use `Sync-BrainMemory.ps1` with SA impersonation.
Failures are non-blocking (warn + continue).

---

## Resolved Questions (Stage 1)

- [x] **JSONL vs JSON array for ledgers?** JSON array. Both `proposal-ledger.json` and
  `incident-ledger.json` use JSON arrays with auto-prune on write. JSON array is easier to
  query (load + filter) and prune (rewrite filtered array). JSONL would save on partial reads,
  but ledger sizes stay small (<1000 entries at 90d retention) so the tradeoff favors simplicity.
- [x] **Should proposal ledger persist to GCS?** No. Ledgers are session-scoped operational
  state. GCS persistence (`Sync-BrainMemory.ps1`) is for cross-machine historical snapshots
  (scores, incidents, gate-ledger, deploy-outcomes). The proposal ledger is local to the
  working directory and pruned aggressively (90d). If cross-machine continuity becomes needed,
  it can be added later without changing the ledger format.
- [x] **Maximum ledger size before indexing?** Not a concern at current scale. 90d auto-prune
  caps entries at ~hundreds. Linear scan over a JSON array of this size is <10ms. Revisit if
  retention exceeds 1 year or entry count exceeds 10,000.

---

## See Also

- `Sync-BrainMemory.ps1` — GCS persistence layer
- `.claude/brain-registry.json` — configuration (Pattern 3)
- `.claude/skills/operational-memory.md` — query patterns for operational data
- `Find-TerminologySymbol.ps1` — terminology governance tool; generates `terminology-cmdb.json`
