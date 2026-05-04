# v5 Composition Guide

> Modularity at NeuroGrim's six trait surfaces — written from shipped
> reality, not aspiration. Every recipe lifts working code from an
> in-tree example crate; the workspace CI exercises those crates on
> every PR, so a broken recipe surfaces as a CI failure, not a doc-rot
> mystery.

**Audience:** third-party adopters writing plugins for NeuroGrim —
custom queue backends, scoring sources, sensors, test runners.

**Prerequisites:** working knowledge of Rust 1.75+, async/await,
`#[async_trait]`. Familiarity with cargo workspaces helps.

**Companion docs:** [`neurogrim/crates/neurogrim-sdk/README.md`](../neurogrim/crates/neurogrim-sdk/README.md)
(SDK API reference, with the writing-a-conformant-Sensor walkthrough
inlined); [`docs/sdk.md`](sdk.md) (v3-era SDK overview; superseded by
this guide for v5+ trait-shape questions).

## Contents

1. [Everything is Lego — what v5 ships](#everything-is-lego--what-v5-ships)
2. [What this guide is NOT](#what-this-guide-is-not)
3. [Architecture: where the trait surfaces live](#architecture-where-the-trait-surfaces-live)
4. **Recipe 1: Swap the queue backend** *(coming in Phase 2)*
5. **Recipe 2: Add a custom scoring source** *(coming in Phase 2)*
6. **Recipe 3: Ship a sensor as a crate** *(coming in Phase 2)*
7. **Recipe 4: Drive tests with your own runner** *(coming in Phase 3)*
8. **v5.5 / v6 horizon — what's not possible at v5.0** *(coming in Phase 4)*

---

## Everything is Lego — what v5 ships

NeuroGrim's v5 north star is *"core defines the shape; impls ship as
crates"* (see [`roadmap/v5-roadmap.md`](../roadmap/v5-roadmap.md) §
"North-star reframe"). v5 promotes **six trait surfaces** to a stable
contract you can implement in your own crate:

| Trait | Home crate | Purpose | Built-in impls |
|-------|------------|---------|----------------|
| `ScoringSource` | `neurogrim-core` | Load a domain's CMDB envelope | `cmdb`, `a2a`, `function` |
| `Sensor` | `neurogrim-core` | Produce a CMDB envelope from project state | 21 sensors in `neurogrim-sensory` |
| `QueueBackend` | `neurogrim-core` | Persist bus messages for a topic | `JsonlBackend`, `SqliteBackend` |
| `TestRunner` | `neurogrim-core` | Execute a workspace test selection | `NextestRunner` (in `neurogrim-cli`) |
| `Transport` | `neurogrim-a2a` | A2A peer protocol transport | HTTP / file-fixture for tests |
| `SecretBackend` | `neurogrim-secrets` | Encrypted-secrets backend | OS-native + encrypted-file |

All six are exposed through one crate — `neurogrim-sdk` — as a thin
re-export layer. Adopters depend on `neurogrim-sdk = "0.1"` and reach
every trait via `use neurogrim_sdk::*;` without coupling to internal
NeuroGrim crates. Versioned independently from `neurogrim-core` per
the V5-SDK epic; pre-1.0 explicit allowance for trait-shape changes
between minor bumps.

### The reshape rule

A new trait is added only when **(i)** ≥2 plausible alternate impls
already exist or are in scope, **(ii)** an external user has asked for
it, OR **(iii)** leaving it concrete is provably blocking adoption
(see `v5-roadmap.md` § Adversary findings A). v5 trims aggressively —
plenty of "this could be an interface" surfaces stay concrete (per-
domain CMDB types, agent-card versioning, trajectory model
abstraction) until the reshape rule fires for them. The
[v5.5 / v6 horizon section](#v55--v6-horizon--whats-not-possible-at-v50)
is where those trims live.

### Conformance is the contract — not a recommendation

Each trait ships with a **conformance test suite** that adopters MUST
run against their impls to ship a real plugin. The suites live in
`neurogrim-core` (gated by the `conformance` cargo feature) and are
re-exported through `neurogrim-sdk::*_conformance` for adopters:

```rust,ignore
// In your crate's tests/conformance.rs:
use neurogrim_sdk::sensor_conformance::run_factory_conformance;
use my_sensor_crate::MySensorFactory;

#[tokio::test]
async fn passes_full_conformance_suite() {
    let factory = MySensorFactory;
    let report = run_factory_conformance(&factory, /*tempdir*/).await;
    assert!(report.all_passed(), "{:#?}", report.failures());
}
```

Without the conformance test, you have an impl that *might* honor the
contract — that's the failure mode the v5 roadmap calls "modular
middleware ships degraded" (every alternate impl 80% feature-complete;
sum of features across "any combination" is less than the union of any
one). Each recipe in this guide includes the conformance wiring as a
load-bearing step, not an optional one.

The conformance suites are feature-gated to keep `tokio` (which
`tokio::spawn` + `tokio::time::timeout` inside the suites pull in) out
of production builds:

```toml
[dependencies]
neurogrim-sdk = "0.1"
# ... your trait-specific runtime deps

[dev-dependencies]
# REQUIRED to run the conformance suite at test time:
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
# ... your test deps (tokio, tempfile)
```

Cargo unifies feature spec across the two `neurogrim-sdk` entries; the
test build sees `conformance`, the lib build sees the minimal trait
surface. Production `cargo build` stays tokio-clean unless your own
crate independently depends on tokio.

---

## What this guide is NOT

Adopter-facing scope discipline. The guide deliberately excludes:

- **Trait-shape rationale.** Why `TestSelection` is `#[non_exhaustive]`,
  why `runner_name()` lives on the factory and not the trait, why
  `ConformanceReport` was hoisted from per-suite to a shared type —
  these decisions live in [`roadmap/epics/v5-foundation.md`](../roadmap/epics/v5-foundation.md)
  and the corresponding plan files (`.claude/plans/v5-*.md`). Adopters
  use the trait shapes as shipped; the *why* is retrospective material.
- **A v4 → v5 migration guide.** v4 didn't ship the SDK crate; there's
  no migration path to document. v4 sensors live in `neurogrim-sensory`
  by virtue of being in-tree; v5 is the first version where third-party
  crates can extend NeuroGrim without forking.
- **Performance characteristics.** V5-MOD-1's perf-gate work captured
  the scoring round-trip baseline (`p95 ≤ 19ms`) at
  [`roadmap/data/v5-scoring-baseline-2026-05-02.json`](../roadmap/data/v5-scoring-baseline-2026-05-02.json).
  Trait-impl perf
  is dominated by the consumer's own work (HTTP latency, filesystem
  IO, etc.), not the dispatch overhead.
- **A tour of every built-in impl.** The SDK README + rustdoc on
  docs.rs cover that depth. This guide focuses on the *composition
  pattern* — wiring a third-party impl into NeuroGrim — not on
  documenting the in-tree impls themselves.

If you're hitting one of these excluded topics, the cross-references
above point you at the right doc surface.

---

## Architecture: where the trait surfaces live

Cargo workspace shape, with the dependency direction shown by arrows:

```
┌──────────────────┐    ┌──────────────────┐
│  neurogrim-sdk   │───▶│  neurogrim-core  │
│ (contract crate) │    │ (4 traits below) │
│  re-export only  │    └────────┬─────────┘
└──────────────────┘             │
         │                       ├── ScoringSource → cmdb / a2a / function
         │                       ├── Sensor        → 21 built-ins (neurogrim-sensory)
         │                       ├── QueueBackend  → JsonlBackend / SqliteBackend
         │                       └── TestRunner    → NextestRunner (neurogrim-cli)
         │
         ├──▶ neurogrim-a2a      (1 trait: Transport — A2A peer protocol)
         └──▶ neurogrim-secrets  (1 trait: SecretBackend — encrypted-secrets)
```

`neurogrim-sdk` is the single crate adopters depend on. It re-exports
every trait surface verbatim — no wrapper types, no façades, no
ergonomic helpers (per VISION principle #8 *absorption over
invention*). Arrows from SDK to the impl-home crates mean
"`pub use`-style re-exports". The SDK is versioned independently
from `neurogrim-core` (`0.1.x` at v5.0); core can break internals
between patch bumps, but the SDK cannot break trait shapes between
minor bumps.

The four V5-MOD-1/2/3 + V5-FOUND-4 trait pairs (factory + impl-base)
live in `neurogrim-core` because they're shape-stable and zero-IO.
`Transport` and `SecretBackend` live in their own crates because they
carry network and crypto concerns respectively (and pre-date the v5
modularity push — they shipped in v3.x and v4.2 and were already
trait-shaped).

The recipes below show how to wire a third-party impl into each of
these surfaces. They're listed in order of "simplest to ship" — start
with whichever matches your use case.

---

## Recipe 1: Swap the queue backend

NeuroGrim's bus persists messages via the `QueueBackend` trait —
built-ins are `JsonlBackend` (file fan-out) and `SqliteBackend`
(transactional + ack-capable). Use this recipe when you need a
different persistence shape: in-memory ring buffer for tests, Redis
or PostgreSQL for cross-process coordination, DynamoDB for
serverless.

Reference impl: [`neurogrim/examples/queue-backend-memory/`](../neurogrim/examples/queue-backend-memory/) —
in-memory ring buffer with full ack semantics. Lift this pattern.

`Cargo.toml`:

```toml
[dependencies]
neurogrim-sdk = "0.1"
anyhow = "1"
tracing = "0.1"

[dev-dependencies]
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tempfile = "3"
```

Minimum-viable impl:

```rust,ignore
use neurogrim_sdk::{QueueBackend, QueueBackendFactory, QueueMessage, StoredMessage};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, RwLock};

pub struct MemoryQueueBackend {
    log:         RwLock<Vec<StoredMessage>>,
    acks:        RwLock<HashMap<String, BTreeSet<u64>>>,
    next_offset: RwLock<u64>,
}

impl QueueBackend for MemoryQueueBackend {
    fn append(&self, msg: &QueueMessage) -> anyhow::Result<u64> {
        let mut next = self.next_offset.write().unwrap();
        let off = *next;
        *next += 1;
        drop(next);
        self.log.write().unwrap().push(StoredMessage {
            offset:  off,
            message: msg.clone(),
        });
        Ok(off)
    }

    fn read_from(&self, since: u64, limit: usize) -> anyhow::Result<Vec<StoredMessage>> {
        Ok(self.log.read().unwrap().iter()
            .filter(|sm| sm.offset >= since)
            .take(limit)
            .cloned()
            .collect())
    }

    fn len(&self) -> anyhow::Result<u64> {
        Ok(self.log.read().unwrap().len() as u64)
    }
    // Override `supports_ack`/`read_unacked`/`ack`/`last_acked` for
    // ack-capable backends. See queue-backend-memory's full impl.
}

pub struct MemoryQueueBackendFactory;

impl QueueBackendFactory for MemoryQueueBackendFactory {
    fn name(&self) -> &'static str { "memory" }
    fn build(&self, _queue_root: &Path, _topic: &str)
        -> anyhow::Result<Arc<dyn QueueBackend>>
    {
        Ok(Arc::new(MemoryQueueBackend {
            log:         RwLock::new(Vec::new()),
            acks:        RwLock::new(HashMap::new()),
            next_offset: RwLock::new(0),
        }))
    }
}
```

Conformance test (`tests/conformance.rs`):

```rust,ignore
use neurogrim_sdk::queue_backend_conformance::run_factory_conformance;
use my_queue_crate::MemoryQueueBackendFactory;
use tempfile::TempDir;

#[tokio::test]
async fn factory_passes_full_conformance_suite() {
    let factory = MemoryQueueBackendFactory;
    let dir = TempDir::new().unwrap();
    let report = run_factory_conformance(&factory, dir.path()).await;
    assert!(report.all_passed(), "{:#?}", report.failures());
}
```

Register at startup in your consuming binary's `main.rs`:

```rust,ignore
use neurogrim_sdk::QueueBackendRegistry;

let mut registry = QueueBackendRegistry::new();
// Built-ins: jsonl + (sqlite under the `sqlite` feature).
registry.register_all(neurogrim_sdk::queue_built_in_factories());
// Your factory.
registry.register(Box::new(MemoryQueueBackendFactory));
```

**What's NOT possible at v5.0:** dynamic `.so` / `.dll` plugin
loading — at v5.0 plugins are cargo-feature-gated at compile time
(BACKLOG B-40, v5.5 successor pipeline). For runtime registration
of factories from an external library, fork the bus dispatch path
and wait for B-40 to land.

---

## Recipe 2: Add a custom scoring source

`ScoringSource` loads a domain's pre-computed CMDB envelope for the
unified-score aggregation in `neurogrim score`. Built-ins are `cmdb`
(JSON file under the project root), `a2a` (peer Brain over HTTP),
and `function` (no-op marker). Use this recipe when you want to
plug a new score-source pattern: HTTP fetch from a metrics service,
database lookup, custom binary format.

Reference impl: [`neurogrim/examples/scoring-source-prom/`](../neurogrim/examples/scoring-source-prom/) —
Prometheus instant-query HTTP-fetch pattern. Lift this.

`Cargo.toml`:

```toml
[dependencies]
neurogrim-sdk = "0.1"
# `ScoringSourceConfig` + `CmdbData` are not yet re-exported by the
# SDK at 0.1.0 (cyclic-dep concern with neurogrim-ecosystem; tracked
# for SDK 0.2.0 polish). Until then, take a direct neurogrim-core
# dep alongside neurogrim-sdk for these support types:
neurogrim-core = "5"
async-trait = "0.1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls-native-roots"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
url = "2"
tracing = "0.1"

[dev-dependencies]
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tempfile = "3"
```

Minimum-viable impl:

```rust,ignore
use async_trait::async_trait;
use neurogrim_core::registry::ScoringSourceConfig;
use neurogrim_core::scoring::CmdbData;
use neurogrim_sdk::{ScoringSource, ScoringSourceFactory};
use std::path::Path;

pub struct PromSource;

#[async_trait]
impl ScoringSource for PromSource {
    fn source_type_name(&self) -> &'static str { "prom" }

    async fn load(
        &self,
        _domain_key: &str,
        config:      &ScoringSourceConfig,
        _project_root: &Path,
    ) -> Option<CmdbData> {
        let endpoint = config.endpoint.as_ref()?;
        let query    = config.path.as_ref()?;     // PromQL via `path`
        // ... fetch via reqwest, parse, clamp 0-100, build CmdbData.
        // Failure modes (missing endpoint, bad URL, HTTP error,
        // malformed response, empty result) ALL surface as None,
        // never panic. See scoring-source-prom for the full body.
        Some(/* CmdbData { ... } */ unimplemented!("see reference"))
    }
}

pub struct PromSourceFactory;

impl ScoringSourceFactory for PromSourceFactory {
    fn source_type_name(&self) -> &'static str { "prom" }
    fn build(&self) -> Box<dyn ScoringSource> { Box::new(PromSource) }
}
```

Conformance test follows the same pattern as Recipe 1 — substitute
`scoring_source_conformance` for `queue_backend_conformance` and
your factory type.

Register at startup:

```rust,ignore
use neurogrim_sdk::ScoringSourceRegistry;

let mut registry = ScoringSourceRegistry::with_core_built_ins();
registry.register(Box::new(PromSourceFactory));
```

A `brain-registry.json` domain entry like `{"scoring_source":
{"type": "prom", "endpoint": "https://prom.example.com/api/v1/query",
"path": "avg(node_load1)"}}` then routes through `PromSource`.

**What's NOT possible at v5.0:** `ScoringSourceConfig` carries a
closed shape (`endpoint`, `path`, `score_field`, `updated_at_field`)
that all source types share — adding new typed fields requires
schema changes in `neurogrim-core`. Per-source-type custom config
is v6 horizon (BACKLOG B-41 — per-domain custom CMDB types).

---

## Recipe 3: Ship a sensor as a crate

Sensors produce CMDB envelopes that the scoring pipeline consumes —
the most common third-party plugin shape. Built-ins live in
`neurogrim-sensory` and cover ~21 domains; third-party sensors
register alongside them.

Reference impls: [`neurogrim/examples/sensor-readme-quality/`](../neurogrim/examples/sensor-readme-quality/)
(file-system pattern; scores `README.md` quality across 6 features)
and [`neurogrim/examples/sensor-constant-score/`](../neurogrim/examples/sensor-constant-score/)
(minimal-deps reference; always reports `score: 42`).

The full Sensor walkthrough — minimum-viable impl, conformance test,
contract pitfalls — is **inlined in the SDK README** at
[`neurogrim/crates/neurogrim-sdk/README.md`](../neurogrim/crates/neurogrim-sdk/README.md)
§ "Writing a conformant Sensor". This guide doesn't duplicate that
content; instead it covers the cross-cutting registration story that
applies to every third-party sensor:

`Cargo.toml` (matches the SDK README exactly — load-bearing
`[dev-dependencies]` posture for the conformance feature):

```toml
[dependencies]
neurogrim-sdk = "0.1"
async-trait = "0.1"
anyhow = "1"
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tempfile = "3"
```

Register at startup:

```rust,ignore
use neurogrim_sdk::SensorRegistry;
use my_sensor_crate::MySensorFactory;

let mut registry = SensorRegistry::new();
// Built-ins: 21 sensors gated by neurogrim-sensory's per-sensor
// feature flags (sensor-git-health, sensor-code-quality, etc.).
registry.register_all(neurogrim_sensory::built_in_factories());
// Your factory.
registry.register(Box::new(MySensorFactory));
```

**What's NOT possible at v5.0:** dynamic sensor discovery (e.g.,
"register every sensor crate in the `sensors/` directory at
startup") is v5.5 work (BACKLOG B-38 — MCP tool plugin loading;
B-40 — dynamic `.so`/`.dll`). At v5.0 every sensor is statically
registered at startup. The `SensorRegistry` is the dispatch layer;
adding more sensors is one `registry.register(...)` line per crate.

---

*Recipe 4 (custom test runner) lands in Phase 3 — see the
[V5-DOC-1 plan](../.claude/plans/v5-doc-1-composition-guide.md) §
Phase 3.*
