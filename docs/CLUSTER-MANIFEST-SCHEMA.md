# NeuroGrim — Cluster Manifest Schema

The canonical schema for cluster manifest TOML files. Where per-broker manifests
([`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md)) declare individual broker
shape, **cluster manifests declare deployment-level governance + Frame defaults +
IAB bootstrap + ACL grants + lifecycle policies** across the cluster of brokers (and,
when IAB lands, the cluster of peer-agents).

Referenced from:
- BB #29 (Broker Lifecycle) — shutdown timeout + cluster-pipeline `allowed_during_shutdown`
- BB #31 (Cluster Federation Topology) — inter-cluster ACL + version cascade + bootstrap
- BB #34 (Workflow-Pipeline Versioning Contract) — contract-evolution policy per broker
- BB #35 (Frame stack) — per-cluster Frame defaults + conflict-precedence overrides
- [`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md) Q3 — bootstrap modes
- [`../../cereGrim/docs/COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md) — `MaxCognitionCycleIterations` per Tempo

---

## Required fields

Every cluster manifest declares at minimum:

```toml
[cluster]
id = "<unique-cluster-id>"           # kebab-case; unique across the federation
name = "<human-readable-name>"        # shown in operator telemetry + Brain UI
brokers_dir = "<path>"                # path to dir holding per-broker manifests
```

| Field | Type | Validation |
|---|---|---|
| `id` | string | kebab-case; `^[a-z][a-z0-9-]*$`; unique across federation |
| `name` | string | UTF-8; ≤120 chars; no newlines |
| `brokers_dir` | string | Path relative to project root; framework discovers per-broker manifests here at startup |

**Load-time validation:** the Broker Registry validates every cluster manifest at
startup; missing required fields = framework startup failure (loud, not silent).

---

## Bootstrap declaration

Per [`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md)
Q3, one of four bootstrap modes:

```toml
[cluster.bootstrap]
mode = "federated-mesh"  # | "role-led" | "arbiter-service" | "static"

# Mode-specific fields:

# federated-mesh:
gossip_rounds = 3                              # K gossip rounds before topology converges (default 3)
peer_majority_threshold = 0.5                  # fraction of peers each agent must see (default majority)

# role-led:
lead_role = "PM"                               # role-name that takes topology lead
tie_breaker = "lower-port-wins"                # | "higher-port-wins" | "operator-specified"

# arbiter-service:
arbiter_endpoint = "http://arbiter:8500/v1"
arbiter_retry_backoff = ["1s", "5s", "30s"]    # retry sequence on startup unreachability

# static:
static_config_path = "cluster-topology.toml"   # operator-declared topology file
allow_extra_agents = false                     # framework refuses unknown agents joining
```

---

## Frame defaults

Per [`BROKER-FRAMES.md`](BROKER-FRAMES.md) §1 inheritance hierarchy (cluster is the
outermost level). Declare default values for any of the seven Frame types:

```toml
[cluster.frame_defaults]
hat = "architect"                              # default mindset for cluster-wide work
stakes = "production"                          # default risk profile
tempo = "deliberate"                           # default cadence
mode = "implementation"                        # default lifecycle phase
confidence = "tentative"                       # default certainty
audience = "operator-direct"                   # default output framing
scope = "local"                                # default blast radius

[cluster.frame_conflict_precedence]
# Override default precedence matrix per BROKER-FRAMES §7.1
order = ["Stakes", "Hat", "Mode", "Confidence", "Tempo", "Audience", "Scope"]
```

---

## Cluster-pipeline ACL grants

Per BB #27 (Cross-Broker Composition Policy) and IAB ACL governance:

```toml
[[cluster.acl_grants]]
from_broker = "work-broker"
to_broker = "sensory-broker"
allowed_pipelines = ["read-awareness-summary"]
trust_budget_unit_required = "token-spend"      # cross-broker dispatches must use this unit

[[cluster.acl_grants]]
from_broker = "context-broker"
to_broker = "memory-broker"
allowed_pipelines = ["recall-by-salience", "pin-to-hot"]
trust_budget_unit_required = "dispatch-count"
```

---

## Per-cluster trust-budget policy

Per [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4 Trust Budget formalization:

```toml
[cluster.trust_budget]
global_unit = "token-spend"                    # cluster-wide unit (per-broker may override but composition refuses mismatch)
global_ceiling = 1_000_000                     # cluster-wide ceiling (sum of per-broker budgets cannot exceed)
allocation_strategy = "proportional"           # | "fixed-ceiling" | "time-decaying"
replenishment = "time-decay-per-hour"          # | "manual-operator-reset" | "metric-driven"
replenishment_rate = 50_000                    # for time-decay: tokens replenished per hour

# Per-broker overrides
[cluster.trust_budget.per_broker]
sensory-broker = { ceiling = 200_000, allocation_strategy = "fixed-ceiling" }
work-broker = { ceiling = 300_000 }
context-broker = { ceiling = 400_000 }
```

---

## Inter-cluster federation config (BB #31)

When this cluster participates in a federation of clusters (Cluster Federation
Topology):

```toml
[cluster.federation]
inter_cluster_bootstrap = "federated-mesh"     # | "arbiter-service" | "static"

# Inter-cluster ACL (transitive composition control)
[[cluster.federation.inter_cluster_acl]]
from_cluster = "cluster-1"
to_cluster = "cluster-2"
allowed_cluster_pipelines = ["handoff-workflow"]
version_cascade_policy = "strict-match"        # | "allow-newer-target" | "allow-older-target"

[[cluster.federation.peers]]
cluster_id = "cluster-2"
a2a_endpoint = "http://cluster-2.local:8500/a2a/v1"
trust_level = "high"                           # | "medium" | "low" — informs ACL grant tightness
```

---

## Lifecycle policies

Per BB #29 (Broker Lifecycle):

```toml
[cluster.lifecycle]
shutdown_timeout_per_pipeline_ms = 5000        # default 5000ms; per-pipeline force-kill threshold
hot_swap_allowed = true                        # operator can transition broker version mid-cluster
graceful_drain_enabled = true                  # cluster-wide drain coordination via cluster-pipelines

# Per-cluster-pipeline `allowed_during_shutdown` overrides
[cluster.lifecycle.allowed_during_shutdown]
"work-broker/dispatch-shutdown" = true         # this cluster-pipeline must complete even during shutdown
"sensory-broker/emit-final-snapshot" = true
# All other cluster-pipelines default to false (rejected with peer_shutting_down)
```

---

## Contract-evolution policy (BB #34)

Per Workflow-Pipeline Versioning Contract:

```toml
[cluster.contract_evolution]
default_policy = "allow_backward_compatible_only"
# Options:
# - "allow_backward_compatible_only" — new contracts must be supersets of old
# - "allow_forward_compatible_upgrades" — workflows can adopt newer pipeline contracts
# - "manual-operator-approval-per-contract" — each contract version requires operator sign-off

# Per-pipeline overrides
[cluster.contract_evolution.per_pipeline]
"work-broker/dispatch-work-unit" = "manual-operator-approval-per-contract"  # high-stakes pipeline
"sensory-broker/read-awareness-summary" = "allow_forward_compatible_upgrades"
```

---

## Cognition-channel speaker authorization (R-S-8 closure, Phase 9; generalized post-vision-audit)

For consuming projects using the reserved `_neurogrim/cognition` bus topic (per
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) BB #4 + the cereGrim peer-dialogue use
case at [`../../cereGrim/docs/COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md)
Q2), declare the set of speakers authorized to write to the channel. Framework
rejects messages from speakers not listed here. **This substrate is general** —
NeuroGrim's contract is "operator declares N authorized speaker IDs; framework
authenticates against the list." Consuming-project-specific role semantics (e.g.,
cereGrim's `primary` + `meta` lobe roles) are layered on top in the consuming
project's own composition docs, not in this manifest schema.

```toml
[cluster.cognition_cycle]
authorized_speakers = ["<speaker_id>", "<speaker_id>", ...]   # operator-declared set; framework authenticates
per_speaker_messages_per_iteration_max = 1                    # default; bound per-cycle injection
```

**Example (cereGrim's dual-lobe usage)** — cereGrim declares two speakers, names them
per its dual-lobe pattern, and binds Primary/Meta roles in its own composition doc:

```toml
[cluster.cognition_cycle]
authorized_speakers = ["ceregrim-primary", "ceregrim-meta"]
per_speaker_messages_per_iteration_max = 1
```

Then cereGrim's [`COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md)
declares that `ceregrim-primary` is the Primary Lobe ID and `ceregrim-meta` is the
Meta Lobe ID — the speaker-role binding is consuming-project-specific. A future
consumer with three lobes (Reviewer, Synthesizer, Critic) declares three speakers
without changing this schema. NeuroGrim substrate stays general.

Rejected messages emit to BB #28 Diagnostics with `audit_class: governance` and
`failure_reason: unknown_speaker_id` (or `cognition_speaker_rate_exceeded` for
quota violations).

---

## CognitionCycle parameters (cereGrim-specific extension)

Per [`../../cereGrim/docs/COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md)
Q3 + the [`../../cereGrim/docs/FRAMES-COMPOSITION.md`](../../cereGrim/docs/FRAMES-COMPOSITION.md)
§5.3 Tempo override caps. cereGrim deployments declare:

```toml
[cluster.cognition_cycle]
max_iterations_default = 3                     # baseline recursion guard
max_iterations_per_tempo = { rapid-prototype = 1, deliberate = 3, campaign = 5 }
convergence_test = "no-rejects-two-consecutive-cycles"  # per COGNITION-LOOP Q3 Layer 2

[cluster.cognition_cycle.frame_rotation_caps]
rapid_prototype = 2
deliberate = 5
campaign = 7
steady = 4
```

(NeuroGrim base substrate provides the mechanism; cereGrim's cognition-cycle is one
consuming-project use case. Other consuming projects with their own peer-dialogue
models would declare their own cycle parameters.)

---

## Per-parameter tunability surface (M-13 closure)

The framework's parameters are tunable at different scopes; not every parameter has a
per-broker override. This table enumerates which parameters are tunable at which level
+ what the inheritance/override semantics are:

| Parameter | Cluster-level | Per-broker-level | Per-pipeline-level | Per-dispatch-level | Override semantics |
|---|---|---|---|---|---|
| **Trust-budget ceiling** | required | optional override | — | — | Per-broker overrides cluster; cluster-global is ceiling sum-cap |
| **Trust-budget unit** | required (global) | — | — | — | Cluster-only; per-broker mismatch refused at composition per §4 unit-composition rule |
| **Trust-budget allocation strategy** | required | optional override | — | — | Per-broker overrides cluster default |
| **Unit-conversion table** | optional | — | — | — | Cluster-only AND Untunable; Autonomous tuners cannot modify |
| **Frame defaults (Hat/Stakes/Tempo/Mode/Confidence/Audience/Scope)** | optional | optional | optional | optional | Innermost wins per BROKER-FRAMES.md §7.2 inheritance order |
| **Frame conflict-precedence matrix** | optional override | — | — | — | Cluster-only; defaults to BROKER-FRAMES.md §7.1 |
| **MaxFrameRotationDepth** | required | — | — | — | Cluster-only |
| **MaxBrokerDepth** | required | — | — | — | Cluster-only |
| **MaxCrossBrokerCompositionDepth** | required | — | — | — | Cluster-only |
| **MaxCognitionCycleIterations** | required default | — | — | — | Cluster-only; per-Tempo overrides per cluster manifest §cognition_cycle |
| **rotation_budget_ceiling** | required | optional override | — | — | Per-broker overrides cluster |
| **Skill Filter weight cells** | — | required defaults | optional override | — | Operator-tunable (per declared bounds); Autonomous bounds enforced at write |
| **Governance composition (per Surfaced pipeline)** | optional defaults | optional defaults | required declaration | — | Innermost wins; Untunable governance always composed regardless |
| **Pipeline `audit_class`** | — | — | required declaration | — | Per-pipeline; framework defaults documented in §3 |
| **Pipeline `tunability` tier** | — | — | required declaration | — | Per-pipeline; default is OperatorOnly |
| **Cancellation depth max** | optional default | — | optional override | — | Per-pipeline overrides cluster default |
| **Schema coexistence window** | — | required | — | — | Per-broker only |
| **Cluster-pipeline `allowed_during_shutdown`** | optional defaults | — | required declaration | — | Per-cluster-pipeline; default false |
| **Inter-cluster ACL grants** | required | — | — | — | Cluster-only; transitive composition per BB #31 |
| **Materializer composition order** | required | — | — | — | Cluster-only |
| **Deprecated pipelines registry** | required | — | — | — | Cluster-only (per BB #37) |
| **Quarantined sensors registry** | required | — | — | — | Cluster-only (per BB #38) |
| **Action-ledger retention window** | required | — | — | — | Cluster-only (per BB #36) |

Parameters not listed here are framework-internal (not operator-tunable; code change
required). When in doubt: cluster manifest declares cluster-wide defaults; broker
manifest declares per-broker overrides where the per-broker column is non-empty above;
pipeline-declaration in the YAML catalog declares per-pipeline values where the
per-pipeline column is non-empty.

---

## Field-level tunability annotations (R-S-18 closure, Phase 9)

Every manifest field carries a **`tunability` classification** validated at load time.
Spec declares `Untunable` parameters; framework enforces by refusing to load a manifest
that attempts to set them. Closes the gap where "Untunable" was convention-not-code —
operator could edit the manifest to change governance-bearing parameters that the spec
declared immutable, and the framework wouldn't catch it.

Classifications (per [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4 tunability tiers):

| Annotation | Manifest semantics | Loader enforcement |
|---|---|---|
| **Untunable** | Field MUST NOT appear in manifest; framework-internal value only (code-change required) | Loader rejects manifest with `failure_reason: untunable_field_set: <path>` if field is present |
| **OperatorOnly** | Operator may set in manifest; default applies if omitted; runtime cannot mutate | Loader accepts; runtime tuning attempts on this field are rejected |
| **OperatorConfirmed** | LLM can propose changes via Proposal Ledger; operator must approve | Loader accepts manifest value as the operator-confirmed state |
| **Autonomous** | Tuner may adjust within declared bounds; bounds themselves are OperatorOnly | Loader accepts; runtime tuning within bounds is permitted |

**Tunability table for documented fields** (canonical; loader validates against this
table at startup):

| Field | Tunability | Notes |
|---|---|---|
| `cluster.id` | OperatorOnly | Cluster identity; rare changes |
| `cluster.bootstrap.mode` | OperatorOnly | Federation topology choice |
| `cluster.trust_budget.global_unit` | Untunable-once-set | First-set wins; subsequent attempts refused (unit mismatch breaks composition) |
| `cluster.trust_budget.unit_conversion` | OperatorOnly (Untunable to Autonomous tuners) | Per §4 unit-conversion rule |
| `cluster.frame_conflict_precedence.order` | OperatorOnly + Stakes-floor | Operator may reorder BUT Stakes-with-governance-implications can never be suppressed (R-S-17 rule, Phase 9) |
| `cluster.lifecycle.shutdown_timeout_per_pipeline_ms` | OperatorOnly | Default 5000ms |
| `cluster.cognition_cycle.authorized_speakers` | OperatorOnly | R-S-8 speaker authorization; operator-declared N-speaker set; consuming projects layer their own role semantics atop |
| `cluster.cognition_cycle.max_iterations_default` | OperatorOnly (bounded 1-7) | Per COGNITION-LOOP Q3 |
| `cluster.bootstrap.gossip_rounds` | OperatorOnly (bounded ≤10) | Bootstrap-DoS guard per R-S-14 |
| Awareness Service rate-limit enforcer config | **Untunable** | Per BROKER-CONTRACT §"Sensory Queue contract enforcer" — code change required |
| Topology Broker self-bypass logic | **Untunable** | Per BROKER-INTERNALS.md §"Topology Broker self-bypass invariant" — code change required |
| Materializer Composer governance-first override | **Untunable** | Per BB #22a; framework places `governance-pipelines` segment first regardless of operator-declared `materializer_composition_order`. Reachability invariant is structurally guaranteed; cannot be disabled by manifest (R-O-3 closure; vision-audit follow-up). Operator's declared order still controls relative ordering of non-governance segments. Override is auditable: BB #32 Telemetry projects "governance-segment-position: pinned-by-framework" so operators see when override is active. |
| `materializer_composition_order` (non-governance segments) | OperatorOnly | Operator declares order of non-governance segments; framework reorders only to place governance first. Within that constraint, operator's order is preserved. |
| Pipeline Runner internals | **Untunable** | Tier 3 bootstrap; code change required |

**Adding new fields to the manifest** requires:
1. Declaring the tunability classification in this table.
2. Updating the loader's validation schema to enforce the classification.
3. If `Untunable`, the field must NOT appear in operator-facing manifest examples —
   it lives in the framework binary only.

---

## Validation rules

Framework validates at startup:

1. **All required fields present.** `cluster.id`, `cluster.name`, `cluster.brokers_dir`,
   `cluster.bootstrap.mode`.
2. **Bootstrap mode-specific fields present** per the four-mode table above (e.g.,
   `lead_role` required when `mode = "role-led"`).
3. **ACL grants reference real brokers + real pipelines** — framework loads per-broker
   manifests and validates every grant resolves.
4. **Trust-budget allocation sums** — per-broker ceilings can't exceed cluster-global
   ceiling.
5. **Frame conflict precedence covers all seven Frame types** — partial orderings
   refused.
6. **Inter-cluster federation peers reach** — at least one A2A roundtrip succeeds per
   declared peer at startup (warning only; failed peers logged but framework continues).

---

## Open follow-ons

- **Cluster-manifest hot-reload** — operator updates the manifest mid-deployment;
  framework reloads atomically without restart. Deferred until S0-T base implementation
  exists and the reload surface is empirically visible (analogous to BB #9 catalog
  hot-reload).
- **Cluster-manifest version migration** — when the cluster-manifest schema itself
  evolves (new fields, validation rules), how do operators migrate? Likely a
  `schema_version` field at top + a per-version migration runner parallel to BB #26.
  Defer until v2 is actually needed.
- **Cross-cluster manifest discovery** — for BB #31 Cluster Federation Topology, how
  do clusters discover each other's manifests for inter-cluster ACL validation?
  Likely via a registry service or via the A2A protocol; specify when S0-C
  implementation begins.
