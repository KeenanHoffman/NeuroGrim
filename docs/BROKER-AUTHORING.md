# Broker Authoring Guide

> Operator-facing reference for adding behavior to a NeuroGrim Brain.
> Two paths: **Tier 1** (declarative TOML — no Rust) and **Tier 2** (Rust
> impls — the existing factory pattern). Pick the path that matches your
> need.

**Companion docs:**
- `BROKER-CONTRACT.md` — what brokers are + the role-set
- `BROKER-INTERNALS.md` — substrate building blocks
- `BROKER-MANIFEST-SCHEMA.md` — cluster.toml + per-broker manifest shape
- `BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` — design decisions for the
  workspace + sensory + extension primitives

---

## The two tiers

| Tier | When to use | What you write | Lives at |
|---|---|---|---|
| **Tier 1** — declarative extension | Add a fact, a recommendation, a simple pipeline that returns a fact, OR a sensor that scores via a built-in pattern (file presence, glob count, derived from other CMDBs) | A `.toml` file | `<project_root>/.claude/brain/broker/extensions/<broker-id>/<name>.toml` |
| **Tier 2** — Rust broker impl | Anything outside the built-in patterns: custom sensor logic, novel broker type, Embodiment surface, etc. | A Rust crate or in-workspace module implementing `Broker` (+ optionally `Extensible`, `SensoryBroker`, `WorkspaceBroker`) | Anywhere; register the factory with `BrokerHostConfig::broker_factories` |

> **Rule of thumb:** if your need fits a Tier 1 template the CLI can
> emit, use Tier 1. If you find yourself wanting conditionals, loops, or
> external API calls inside an extension, you've hit Tier 2 territory.

---

## Tier 1 — declarative extensions

### Anatomy of an extension config

Every Tier 1 TOML must include an `[extension]` envelope:

```toml
[extension]
schema_version = "1"        # required — must match the target broker's supported version
authored_by    = "operator" # optional — surfaced in audit / error messages
```

Plus one or more broker-specific sections (see templates below).

### Discovery + apply

The substrate scans `<cluster_manifest_dir>/extensions/<broker-id>/*.toml` at
host boot, deterministically (alphabetical by filename), and calls each
broker's `apply_extension` for each config targeting it. Schema-version
mismatch fails boot loudly with a clear file:line error. Configs targeting
a broker that doesn't implement `Extensible` are logged via
`tracing::warn!` but don't block boot.

### Scaffolding templates with the CLI

```bash
# Workspace fact
neurogrim broker-extension-scaffold \
    --broker workspace --kind fact --name team-conventions

# Workspace terminal recommendation (don't burn cycles rediscovering local quirks)
neurogrim broker-extension-scaffold \
    --broker workspace --kind terminal-rec --name shell-quirks

# Workspace declarative pipeline (returns a fact value)
neurogrim broker-extension-scaffold \
    --broker workspace --kind pipeline --name get-deploy-region

# Sensory broker — file-presence-score
neurogrim broker-extension-scaffold \
    --broker sensory --kind sensor --pattern file-presence --name doc-quality

# Sensory broker — glob-count
neurogrim broker-extension-scaffold \
    --broker sensory --kind sensor --pattern glob-count --name todo-backlog

# Sensory broker — cmdb-derived (composite scores)
neurogrim broker-extension-scaffold \
    --broker sensory --kind sensor --pattern cmdb-derived --name release-readiness
```

Add `--out <path>` to write directly to disk; default emits to stdout for
review-before-paste.

### Workspace fact example

```toml
[extension]
schema_version = "1"
authored_by = "ops-team"

[[facts]]
key = "deployment.primary_region"
value = "us-west-2"
category = "general"
note = "Locked in 2026-Q2; do not change without ops review."

[[facts]]
key = "deployment.staging_canary"
value = "us-east-1"
category = "general"
```

After boot, agents see these via `workspace/get-fact { key = "..." }`.

### Sensory Tier 1 patterns (V1)

Three built-in declarative patterns cover ~80% of operator-authored sensors:

#### `file_presence_score`

Score based on presence + (optional) freshness of required files. Linear:
score = 100 × (present / total); freshness penalty applies to stale files.

```toml
[extension]
schema_version = "1"

[sensor]
broker_id = "sensor-doc-quality"
role = "sense"
domain = "documentation"
pattern = "file_presence_score"

[sensor.config]
required_files = ["README.md", "docs/ARCHITECTURE.md", "CONTRIBUTING.md"]
scoring = "linear"                # or "all-or-nothing"
freshness_window_days = 90
freshness_penalty = 25            # percentage points
```

#### `glob_count`

Score based on count of files matching a glob. `scoring = inverse` (fewer =
higher score, capped at `ceiling`) or `direct` (more = higher score, capped).

```toml
[sensor]
broker_id = "sensor-todo-backlog"
role = "sense"
domain = "todos"
pattern = "glob_count"

[sensor.config]
glob = "**/*.todo.md"
scoring = "inverse"
ceiling = 50
```

#### `cmdb_derived`

Composite scores derived from other sensors' CMDBs.

```toml
[sensor]
broker_id = "sensor-release-readiness"
role = "sense"
domain = "release-readiness"
pattern = "cmdb_derived"

[sensor.config]
sources = ["deploy-readiness", "security-standards", "test-health"]
combinator = "min"   # or "max" | "mean" | "median"
```

### Sensory Queue enforcer (V1 scope)

Tier 1 sensor extensions write through the Sensory Queue enforcer
(per `BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` Gate 3). V1 enforces:

- **Rate limit** per source (default 12 writes per 60s window;
  per-source overrides via cluster.toml).
- **Schema validation** against `cmdb-envelope-v1.schema.json`.

V1 does NOT yet enforce **redaction** (secret pattern stripping, PII
detection). **You are responsible for ensuring your sensor doesn't emit
secrets in CMDB payloads.** Redaction lands in V2.

### Schema versioning

Extensions declare `schema_version` in `[extension]`. Brokers declare
which version they support via `Extensible::extension_schema_version`.
Mismatches FAIL BOOT with a clear error message pointing at the
offending file. To support multiple versions, brokers can implement
their own migration during `apply_extension`.

---

## Tier 2 — Rust broker impls

When a Tier 1 template doesn't fit, implement the [`Broker`] trait
directly.

### Minimum surface

```rust
use async_trait::async_trait;
use neurogrim_brokers::{
    Broker, BrokerError, Pipeline, Role, RoleSet, WorldEvent,
    LeafContext, LeafError,
};

pub struct MyBroker {
    id: String,
    // your state here — typically `Arc<Mutex<...>>` for interior mutability
}

#[async_trait]
impl Broker for MyBroker {
    fn id(&self) -> &str { &self.id }
    fn role_set(&self) -> RoleSet { RoleSet::single(Role::Sense) }
    async fn read_overlay(&self) -> serde_json::Value { /* JSON projection */ }
    async fn legal_pipelines(&self) -> Vec<Pipeline> { /* surfaced pipelines */ }
    async fn governance_pipelines(&self) -> Vec<Pipeline> { /* governance pipelines */ }
    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> { Ok(()) }
    async fn execute_leaf(
        &self, name: &str, ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> { /* match name { ... } */ }
}
```

### Optional opt-ins

| Trait | When to implement | What it does |
|---|---|---|
| `Extensible` | You want operators to extend your broker via Tier 1 TOML | Override `as_extensible() -> Some(self)`; implement `apply_extension(&self, &ExtensionConfig)` |
| `SensoryBroker` (A.2) | Your broker is a sensor that emits a CMDB | Override `cmdb_path() -> Some(path)`; substrate's `CmdbMaterializer` writes your overlay JSON to disk |
| `WorkspaceBroker` (A.0.2) | Your broker is a per-Brain workspace projection | Implement the 14 canonical pipelines; substrate provides the trait shape |

### Registering your broker with the host

In your binary's setup (typically `neurogrim broker-serve` or the
operator's own host wrapper):

```rust
use neurogrim_brokers::{BrokerFactoryRegistry, BrokerHostConfig};

let mut factories = BrokerFactoryRegistry::new();
factories.register(
    "my-broker-type",
    std::sync::Arc::new(|broker_id, governance, project_root| {
        let broker = MyBroker::new(broker_id.to_string());
        let pipelines = broker.catalog();   // your inherent method
        Ok((std::sync::Arc::new(broker) as std::sync::Arc<dyn Broker>, pipelines))
    }),
);

let host = BrokerHost::boot(
    &cluster_manifest_path,
    BrokerHostConfig {
        project_root: Some(project_root),
        trust_budget_ceiling: 10_000,
        broker_factories: factories,
    },
).await?;
```

Then declare your broker in `cluster.toml`:

```toml
[cluster.brokers.my-broker-instance]
manifest_path = "my-broker.toml"
```

And in `my-broker.toml`:

```toml
[broker]
id = "my-broker-instance"
name = "My Broker"
roles = ["sense"]
cold_store_path = "./my-broker-cold/"
catalog_path = "./my-broker-catalog.yaml"
broker_type = "my-broker-type"   # MUST match the factory registration key
```

### Scaffolding a starter

```bash
neurogrim broker-scaffold \
    --broker-id my-broker --pipeline-name do-something \
    --visibility surfaced --audit-class capability \
    --leaf-op do_something
```

Emits a Pipeline literal + leaf-op match-arm stub. Paste, fill in the
FIXMEs, register the factory.

---

## Audit + observability

All broker dispatches — Tier 1 or Tier 2 — generate a `TraceRecord` in
the substrate's `trace.jsonl` audit ledger. Includes broker_id,
pipeline_id, params, outcome, optional snapshot delta. Operators can
tail the ledger for real-time visibility OR consume it post-hoc for
forensic analysis. There is currently no separate dashboard for broker
audit (the dashboard at port 8420 reads the scoring engine's CMDB output,
not the trace ledger directly); building one is on the roadmap.

---

## When Tier 1 isn't enough

Sometimes a Tier 1 pattern *almost* fits but you need one extra
operation. Resist the urge to add a special-case field to the Tier 1
schema. Instead, write a small Tier 2 broker that wraps the same logic
+ adds your special case. This keeps Tier 1 patterns small, audit-clean,
and Turing-incomplete. Each new Tier 1 pattern becomes a stable contract
once shipped (`schema_version = 1` users depend on it), so only add
patterns when you've seen the same shape requested 3+ times.

---

## Common pitfalls

- **Forgot to mark broker `Extensible`.** Default `as_extensible()` returns
  None; extensions targeting your broker get silently warned, not applied.
  Override `as_extensible(&self) -> Option<&dyn Extensible>` to opt in.
- **Schema version drift.** Bump `extension_schema_version()` when you
  change the extension shape in a way that breaks old configs. Don't try
  to silently accept old shapes; FAIL LOUDLY so operators fix their TOML.
- **Tier 1 sensor names not unique.** Sensor extension `broker_id` MUST
  be cluster-unique; collisions fail at registry validation.
- **Forgot to register the factory.** Per-broker manifest's `broker_type`
  must match a key in `BrokerFactoryRegistry`; otherwise host boot fails
  with "no registered factory for broker_type X."
- **Tier 1 sensor that needs control flow.** Don't reach for Tier 1;
  write a Tier 2 broker. Tier 1 is intentionally Turing-incomplete.

---

## Reference

- Substrate API: `neurogrim-brokers` crate docs.rs surface
- Trait reference: `BROKER-INTERNALS.md` §3
- Manifest shape: `BROKER-MANIFEST-SCHEMA.md` + `CLUSTER-MANIFEST-SCHEMA.md`
- Design rationale for the workspace + sensory + extension primitives:
  `BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md`
