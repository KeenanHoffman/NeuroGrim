# NeuroGrim — Broker Manifest Schema

The canonical schema for broker manifest TOML files. Every broker — canonical or
operator-authored, greenfield or wrapped — registers via a manifest matching this
schema.

Referenced from:
- [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"What NeuroGrim provides vs. what
  consuming projects provide" (the broker author writes this)
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 building block #14 (Broker Registry —
  the framework reads this)
- [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) (per-wrapping-path manifest excerpts)

---

## Required fields

Every manifest declares at minimum:

```toml
[broker]
id = "<unique-broker-id>"        # kebab-case; unique across the cluster
name = "<human-readable-name>"   # shown in agent UI / Brain dashboards
roles = ["<role>", ...]          # subset of [Sense, InnateAbility, Embodiment]
cold_store = "<path>"            # path to cold-store backing (file or directory)
catalog = "<path>"               # path to YAML pipeline catalog
```

| Field | Type | Validation |
|---|---|---|
| `id` | string | kebab-case; matches `^[a-z][a-z0-9-]*$`; unique across the registry |
| `name` | string | UTF-8; ≤120 chars; no newlines |
| `roles` | string[] | Non-empty subset of `["Sense", "InnateAbility", "Embodiment"]`. Multi-role brokers are first-class. |
| `cold_store` | string | Path relative to project root; framework verifies parent directory exists at startup |
| `catalog` | string | Path to YAML pipeline catalog file; required even for wrapped brokers (the wrapping path generates entries that land here) |

**Load-time validation:** the Broker Registry validates every manifest at startup.
Missing required fields = broker startup failure (loud, not silent). Per
[`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) header — the framework refuses to register a
malformed broker.

---

## Optional fields — wrapping paths

A broker may declare it wraps an existing substrate. The framework reads `broker.wraps`
to know which wrapping path to apply (per [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md)).

### `broker.wraps.type = "mcp"` (MCP tool wrapper)

```toml
[broker.wraps]
type = "mcp"
mcp_server_name = "<wire-name-in-.mcp.json>"
mcp_server_discovery = "via-.mcp.json"  # OR "host:port" for explicit endpoint
tool_bindings = ["tool_a", "tool_b", ...]  # which tools to broker (not necessarily all)
```

The framework discovers the MCP server at startup, fetches tool schemas, and creates
Pipeline entries per the per-tool `[[pipelines]]` blocks (below). Operator-declared
preconditions + parameter sourcing + governance composition are required per tool — see
[`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) Path 1.

### `broker.wraps.type = "sensor"` (sensor wrapper)

```toml
[broker.wraps]
type = "sensor"
sensor_wire_name = "<sensor-name-in-neurogrim-sensory>"
```

The framework looks up the sensor in `neurogrim-sensory`'s registry by `wire_name` and
wires its `analyze()` method as the Internal Service's projection step. No per-pipeline
schema needed — sensor wrappers have one Tier 2 internal pipeline (`run-<sensor>-tick`)
generated automatically. See [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) Path 2.

### Greenfield brokers (no `[broker.wraps]` block)

A greenfield broker (not wrapping anything) authors its own `catalog` directly + writes
Rust impls for its `Sensor::analyze()`-equivalent (Internal Service projection logic) +
its leaf-op step bodies. The framework wires the role-set scaffolding from the manifest
+ the pipeline catalog from the YAML; everything else is inherited.

---

## Pipeline catalog schema (referenced from `broker.catalog`)

The catalog is a YAML file declaring all pipelines this broker can emit. Per
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §5 (the Work broker worked example) for the
full example. Per-pipeline shape:

```yaml
- id: <pipeline-id>                        # kebab-case; unique within this broker
  visibility: surfaced | internal           # Tier 1 (LLM sees) vs Tier 2 (broker plumbing)
  audit_class: capability | governance | meta-observation
  tunability: untunable | operator-only | operator-confirmed | autonomous
  description: "<≤1200 chars>"              # required for visibility: surfaced
  when_to_use: "<≤336 chars>"               # required for visibility: surfaced
  params:                                   # optional; required if pipeline takes parameters
    <param-name>: { type: <type>, source: <model | state-fill = <expr> | overlay.<path>> }
  preconditions:                            # optional; evaluated against hot store
    - <name>: <expr>
  steps:                                    # ordered sequence of Steps
    - leaf: <leaf-op-id>                    # Tier 3 plain function
      OR
    - sub_pipeline: <pipeline-id>           # composition
      OR
    - guard: { predicate: <expr>, then: <step> }
      OR
    - branch: { predicate: <expr>, then: <step>, else: <step> }
  governance:                               # optional; framework composes defaults
    compose: [<governance-pipeline-id>, ...]
  on_cancel:                                # optional; per BB #25 Pipeline Cancellation Handler
    steps: [<cleanup-step-id>, <log-step-id>, ...]
    terminal_state: cancelled | paused-for-resume    # default: cancelled (no auto-resume)
  expected_effect: <effect-class>           # for idempotency reasoning + audit grouping
```

**Validation rules:**
- `id` unique within broker; combined `<broker.id>/<pipeline.id>` unique within registry.
- `visibility: surfaced` REQUIRES `description` + `when_to_use` per
  [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §1 (≤1,536 chars combined).
- `audit_class` must be present; defaults to `capability` if omitted (with warning).
- `tunability` defaults to `operator-only` per
  [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4.
- `preconditions` expressions reference the broker's hot-store schema; framework
  validates references at load time.
- `governance.compose` defaults to `[check-trust-budget, check-kill-switch,
  record-dispatch, record-outcome]` for `visibility: surfaced`; empty for `internal`.
- Pipelines that carry `audit_class: governance` are exposed via
  `governance_pipelines()` sidecar channel, not `legal_pipelines()` ranking (per the
  reachability channel split in [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4).

---

## Cluster-orchestration extension fields (IAB-relevant)

When the broker participates in a cluster (the cognitive-agent is a peer-agent in an
IAB cluster — see cereGrim's `INTER-AGENT-BROKER.md`), the manifest may declare
cluster-orchestration fields:

```toml
[cluster]
cluster_roles = ["Coder", "Reviewer", ...]   # cluster-roles this peer-agent carries
cluster_pipelines_exposed = ["pipeline-id", ...]  # which surfaced pipelines are cluster-callable
agent_card_path = ".well-known/agent-card.json"   # A2A agent-card path
```

These fields are optional (single-agent deployments ignore them); they extend
NeuroGrim's existing A2A Agent Card with the cluster-orchestration metadata IAB needs.
Full spec lives in the IAB stub at
[`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md)
Q2 (agent definition) — the substrate-level schema migrates upstream once stable.

---

## Open follow-ons

- **Manifest schema validation tool** — a CLI (`neurogrim broker validate <manifest>`)
  that runs all the load-time checks without requiring full broker startup. Useful for
  CI + operator iteration.
- **Schema migration discipline** — when the manifest schema evolves (new required
  fields, type changes), how do existing manifests migrate? Likely a `schema_version`
  field at the top + a per-version migration runner. Defer until v2 of the schema is
  actually needed.
- **Per-broker `displaces / deprecates` field** (per [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md)
  §3 cross-cutting governance) — when a broker is added, what existing broker (if any)
  does it replace? Field will land alongside the building-block deprecation field; same
  discipline applied per-broker.
