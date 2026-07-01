---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# `queue-backend-memory` — Third-Party `QueueBackend` Example

V5-MOD-3 (2026-05-02) lifted the queue backend dispatch from a
hardcoded `BackendHandle` enum + `match BackendKind` in
`neurogrim-dashboard::bus.rs` into a public trait + factory
registry in `neurogrim-core`. **This crate is the proof.** It
implements a third-party `QueueBackend` for an in-memory ring
buffer with full ack semantics, and passes the same conformance
suite the built-in `jsonl` and `sqlite` backends pass.

If you're authoring your own queue backend — for PostgreSQL, Redis,
DynamoDB, Kafka, anything — read this crate end-to-end as your
template.

## Why in-memory?

Three reasons:

1. **Genuinely useful for tests.** A `MemoryQueueBackend` lets
   integration tests exercise queue logic without touching disk
   or spinning up SQLite.
2. **Demonstrates ack semantics.** V5-MOD-1's `scoring-source-prom`
   (HTTP-fetch) and V5-MOD-2's `sensor-readme-quality` (FS-read)
   are read-only flows; neither exercises the `read_unacked` /
   `ack` / `last_acked` methods. This example fills the gap.
3. **Pure logic.** Zero deps beyond `neurogrim-core`. The full
   trait surface fits in ~250 lines, making it the smallest
   possible reference for "what does a conformant backend look
   like?"

## What this crate ships

- **`MemoryQueueBackend`** — append-only ring buffer with
  bounded capacity (default 10,000 messages, FIFO eviction on
  overflow) + per-consumer-group ack tracking
  (`HashMap<String, BTreeSet<u64>>` — out-of-order acks
  supported).
- **`MemoryQueueBackendFactory`** — produces
  `Arc<dyn QueueBackend>` for the wire-name `"memory"`. Default
  factory capacity is 10,000; `MemoryQueueBackendFactory::with_capacity(N)`
  produces backends with capacity N (useful for testing FIFO
  eviction with small numbers).
- **`tests/conformance.rs`** — runs the cross-crate conformance
  suite from `neurogrim_core::queue_backend_conformance` against
  `MemoryQueueBackendFactory`. **Copy this file verbatim into
  your own crate**, rename the factory type, and you have the
  same guarantee as the built-ins.

## Why `BTreeSet<u64>` for ack tracking (Subagent 1's 🟡 C2 finding)

Plan-critic during V5-MOD-3 design caught this:
`HashMap<String, u64>` (a per-group **high-water-mark**) cannot
represent out-of-order acks. If acks arrive as `1`, `4` (skipping
`0`, `2`, `3`), high-water = `4` makes `read_unacked` return ∅,
not `{0, 2, 3}`. That's wrong — and it would silently differ from
`SqliteBackend`'s per-row `acks` table semantics, surfacing as
mysterious test failures only under partial-ack workloads.

`BTreeSet<u64>` per group fixes it: acked offsets are a set, and
`read_unacked` filters by `!set.contains(offset)`. `last_acked`
returns the set's `next_back()` (max element). Idempotent acks
fall out naturally (`BTreeSet::insert` returns `false` on
duplicates and we don't care).

## How a consuming binary registers it

```rust
use neurogrim_core::queue_backend::QueueBackendRegistry;
use neurogrim_dashboard::bus::BusState;
use queue_backend_memory::MemoryQueueBackendFactory;
use std::sync::Arc;

fn build_bus() -> BusState {
    let mut registry = QueueBackendRegistry::new();
    registry.register_all(neurogrim_core::queue_backend::built_in_factories());
    // Third-party in-memory factory.
    registry.register(Box::new(MemoryQueueBackendFactory::default()));
    BusState::with_registry(Arc::new(registry))
}
```

Then a `queue-config.yaml` topic:

```yaml
schema_version: "1"
topics:
  _neurogrim/scratch:
    backend: memory
    ack_required: true
```

Note: in-memory state does NOT persist across process restarts —
appropriate only for ephemeral coordination patterns or test
fixtures.

## Cargo.toml template for true third-party use

```toml
[package]
name = "my-queue-backend"
version = "0.1.0"
edition = "2021"

[dependencies]
neurogrim-core = "5"
anyhow = "1"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
tempfile = "3"
```

The only NeuroGrim dependency is `neurogrim-core`. You do **not**
depend on `neurogrim-dashboard`, `neurogrim-cli`, or any internal
crate. The trait + registry contract lives in `neurogrim-core` so
plugin authors get a stable, narrow public surface.

## Authoring your own — checklist

1. **Read** `neurogrim_core::queue_backend` rustdoc — the trait
   and registry contracts are documented there in full. Note
   especially the `Send + Sync` bound (V5-MOD-3 Fork A2): your
   backend must use `Mutex` / `RwLock` for any interior
   mutability; methods are `&self`.
2. **Pick a wire-name** for your backend (e.g., `"postgres"`,
   `"redis"`, `"dynamodb"`). Conventionally lowercase ASCII with
   hyphens; must be unique across all factories registered in
   any consuming binary.
3. **Implement `QueueBackend`** with `&self` methods. Required:
   `append`, `read_from`, `len`. Optional (only when
   `supports_ack` returns `true`): `read_unacked`, `ack`,
   `last_acked`. The default impls return errors / `Ok(None)`.
4. **Implement `QueueBackendFactory`**. `name()` returns the
   wire-name; `supports_ack()` declares whether built backends
   support ack semantics; `build(queue_root, topic)` returns
   `Arc<dyn QueueBackend>`. The factory composes the per-topic
   path internally — JSONL appends `.jsonl`, SQLite appends
   `.sqlite`, in-memory ignores the path entirely.
5. **Add the conformance test** at `tests/conformance.rs`,
   copying this crate's verbatim. **This is non-optional** —
   passing the suite is the verifiable contract that makes your
   impl safe to plug in.
6. **Document** the wire contract in your README — what
   `queue_root` means for your backend (filesystem path? URI
   prefix? ignored?), retention/eviction behavior, ack
   semantics, and any backend-specific configuration.
7. **Publish to crates.io** with a name like
   `queue-backend-foo` for discoverability.

## What this example does NOT do

- **No persistence.** State lives in `RwLock<Vec<...>>` — process
  restart = empty queue. Real third-party backends that want
  durability use disk or remote storage.
- **No backpressure signaling.** Capacity exhaustion silently
  evicts oldest; producers don't know about the eviction beyond
  the `tracing::warn!` log. A production backend might want to
  signal backpressure via an error or async channel.
- **No retention by time/age.** Only count-based capacity. A
  production backend might support `retention_days` from
  `queue-config.yaml` (the field exists; the in-memory example
  ignores it).
- **No multi-process coordination.** `Arc<RwLock<...>>` is
  in-process only. Real third-party backends that span processes
  use external coordination (file locks, database transactions,
  Redis WATCH/MULTI, etc.).

## Cross-references

- **V5-MOD-3 plan:** `.claude/plans/v5-mod-3-queue-backend-factory.md`
- **`QueueBackend` trait + registry:**
  `crates/neurogrim-core/src/queue_backend.rs`
- **Conformance suite:**
  `crates/neurogrim-core/src/queue_backend_conformance.rs`
- **Built-in references:** `JsonlBackend` (file-based fan-out)
  + `SqliteBackend` (transactional, ack-capable) in the same
  module.
- **Companion examples:**
  `examples/scoring-source-prom/` (V5-MOD-1: HTTP-fetch pattern),
  `examples/sensor-readme-quality/` (V5-MOD-2: FS-read pattern).
