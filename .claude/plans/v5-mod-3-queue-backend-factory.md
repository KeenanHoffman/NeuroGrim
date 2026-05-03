# V5-MOD-3 Queue Backend Factory — Implementation Plan

**Epic:** `roadmap/epics/v5-modular-conversions.md` § V5-MOD-3
**Effort estimate (epic):** S, ~3–5 days
**Status:** drafted 2026-05-02; closes Theme B once shipped
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note
**Substrate:** V5-MOD-1 + V5-MOD-2 closed 2026-05-02 — same trait + factory + registry + conformance + example pattern

## Context

V5-MOD-3 is the smallest of Theme B's three stories: the trait already exists
(`neurogrim_core::queue_backend::QueueBackend`); only the dispatch is hardcoded.
After V5-MOD-3, third-party crates can ship custom backends (PostgreSQL, Redis,
DynamoDB, in-memory) without forking core, and per-topic `queue-config.yaml`
opens to arbitrary backend names.

Following the V5-MOD-1 / V5-MOD-2 substrate exactly: object-safe trait + factory
trait + hand-rolled `HashMap` registry + Phase 5-style conformance suite +
out-of-tree example crate.

## File-anchor corrections (vs. the epic file)

The V5-MOD-3 epic story names a stale anchor:

| Epic says | Reality |
|---|---|
| "`BackendHandle` enum at queue_backend.rs:65–72" | Lines 65–72 of `neurogrim-core/src/queue_backend.rs` are the **`QueueBackend` trait header**. The actual `BackendHandle` enum lives at `neurogrim-dashboard/src/bus.rs:65-72` — wraps `JsonlBackend` (path) vs `SqliteBackend` (Arc<Mutex<...>>) for the bus's per-topic dispatch. |

Same epic-anchor pattern as V5-MOD-1's `registry.rs:135-157` and V5-MOD-2's
"22 sensors" — corrected at V5-MOD-3 close-out.

## Recon-confirmed surface (revised post-plan-critic 2026-05-02)

- **Trait:** `neurogrim_core::queue_backend::QueueBackend` (`Send`-only — see Fork A) with `append` / `read_from` / `len` + ack methods (`supports_ack` / `read_unacked` / `ack` / `last_acked`).
- **`unsafe impl Send for SqliteBackend`** at `queue_backend.rs:451` — manual unsafe Send because `Connection` is conditionally Send only on certain SQLite builds. After Fork A2 internalizes the Mutex, this becomes auto-derived `Send` via `Mutex<Connection>`; **the unsafe impl must be DELETED in Phase 2**, not retained.
- **Built-in impls:** `JsonlBackend` (always-on) + `SqliteBackend` (gated by `sqlite` feature).
- **THREE dispatch sites** (plan-critic finding 🔴 B1 — initial draft only named one):
  1. `dashboard/src/bus.rs:65-72` — `BackendHandle` enum + ~100 lines of dispatch boilerplate at `:74-173`. **Primary V5-MOD-3 target.**
  2. `cli/commands/queue.rs:480-494` — `open_backend()` with a private `BackendChoice` enum + direct `JsonlBackend::new` / `SqliteBackend::open` constructors. **Same scope as V5-MOD-3 (small, mechanically similar).**
  3. `mcp/src/server.rs:866` (`queue_publish`) + `:892-905` (`queue_consume`) — write/read JSONL directly, **bypassing `queue-config.yaml` entirely**. Pre-existing v2 bug: MCP queue tools ignore SQLite-configured topics. **Out of V5-MOD-3 scope** — closes a separate concern; queued as v5.5 follow-up `mcp-queue-config-aware`.
- **Direct `SqliteBackend::open` callers (NOT dispatch sites — read/write score snapshots at known paths):** `mcp/src/context.rs:438`, `mcp/src/services.rs:302`, `mcp/src/logs.rs:296`. **Out of V5-MOD-3 scope** — these are internal score-history readers/writers, not third-party-extensible. Documented as "remaining direct constructors, intentional" (plan-critic 🔵 S2). They DO benefit from Fork A2's `&self` change (no longer need `&mut SqliteBackend`).
- **Wire-format:** `BackendKind` enum at `queue_config.rs:49` (closed-set `Jsonl | Sqlite`). Serialized in `queue-config.yaml::topics::<topic>::backend`. **`validate()` at `queue_config.rs:159-171`** does a type-system invariant check (`ack_required && backend != BackendKind::Sqlite`) — Fork B1 makes this a registry-runtime check (see "Fork B revised" below).
- **`backends` cache** at `bus.rs:185` (`Arc<RwLock<HashMap<String, Arc<BackendHandle>>>>`) — type ripples through 5+ test sites that `assert!(matches!(handle.as_ref(), BackendHandle::Jsonl(_)))` (plan-critic 🟡 C3). After conversion, `Arc<dyn QueueBackend>` has no enum to match — tests rewrite to **observable-shape** assertions (e.g., "stats reports backend name 'jsonl'").
- **`TopicStats::for_topic`** at `bus.rs:467-499` matches on `BackendHandle` variants to extract a backend-name string for display. After conversion, `Arc<dyn QueueBackend>` has no name accessor by default — addressed by Fork D (see below).
- **Conformance:** test functions at `queue_backend.rs:472-550+` taking `make: fn(&TempDir) -> Box<dyn QueueBackend>`. Already polymorphic in shape, BUT current callers use `let mut be = make(&dir)` (Fork A2 changes them to `let be`); ~40 mechanical edits across 13 test fns (plan-critic 🔵 S1). The public `run_factory_conformance` wrapper Phase 4 ships is genuinely additive on top.

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| V5-MOD-1's `ScoringSource` / V5-MOD-2's `Sensor` trait + factory + registry | Mirror the trait shape (object-safe, hand-rolled `HashMap` registry, last-write-wins on duplicate `register()`). |
| V5-MOD-2's `sensor_conformance::run_factory_conformance` | Generalize the existing internal test fns into a public `queue_backend_conformance::run_factory_conformance` with the same `ConformanceReport` / `TestResult` shape. |
| V5-MOD-2's `examples/sensor-readme-quality/` | Same template for V5-MOD-3: `examples/queue-backend-memory/` (or similar — see Fork C). |

### Fork D — Trait surface: `name()` accessor on `QueueBackend` (NEW post-plan-critic 🟡 C3)

`bus.rs::TopicStats::for_topic` at `:467-499` matches on `BackendHandle`
variants to extract a backend-name string ("jsonl" / "sqlite") for the
operator-facing dashboard display. After Fork A2 conversion, `Arc<dyn
QueueBackend>` has no enum to match — name extraction needs a different
path.

| Option | Where the name lives | Cost |
|---|---|---|
| **D1 — Add `fn name(&self) -> &'static str` to `QueueBackend` trait** | The backend itself reports its kind. Diverges from V5-MOD-2's `Sensor` trait (which deliberately has no `name()` — factory's name is canonical). | Adds one method to the trait surface; small SDK consumer-facing inconsistency vs `Sensor`. |
| **D2 — Tuple the cache slot: `(String, Arc<dyn QueueBackend>)`** | Cache holds the backend-name alongside the handle; `TopicStats` reads from the tuple. | No trait change; tuple ripples through call sites. |
| **D3 — Re-resolve from `QueueConfig` at display time** | `TopicStats` reads `cfg.lookup(topic).backend` directly. | Cleanest — no trait change, no tuple. The cache stays handle-only. |

**Plan default: D3** — re-resolve from config. Reasons:
- No trait surface addition; SDK consistency with V5-MOD-2's no-`name()`-on-trait pattern preserved.
- The config lookup is already cached in `bus.rs::config: Arc<RwLock<Option<QueueConfig>>>`; no extra IO.
- The backend-name display is operator-facing metadata, not behavior — config is the right source of truth.

**Alternative arguments:**
- D1 (trait method) is more direct but breaks the V5-MOD-2 precedent. Rejected: SDK consumers reading both traits notice the inconsistency.
- D2 (tuple) is mechanically simple but ugly — adds a tuple ceremony to every cache lookup.

### Fork E — Phase 3 scope: 2-of-3 dispatch sites or all 3 (NEW post-plan-critic 🔴 B1)

Plan-critic surfaced that V5-MOD-3 has **3 dispatch sites**, not 1:
1. `bus.rs::BackendHandle` (primary; ~100 lines of dispatch boilerplate)
2. `cli/commands/queue.rs::open_backend` (private `BackendChoice` enum + direct constructors; ~30 lines)
3. `mcp/src/server.rs::queue_publish` + `queue_consume` (writes/reads JSONL directly, **bypassing `queue-config.yaml` entirely** — pre-existing v2 bug)

| Option | V5-MOD-3 scope | Cost |
|---|---|---|
| **E1 — All 3 sites** | Phase 3 converts bus + cli + mcp. Closes the pre-existing MCP gap as a side effect. | +1.5 days. Doubles Phase 3's scope. |
| **E2 — bus + cli (2 sites); MCP deferred** | Phase 3 converts bus + cli. MCP queue tools remain JSONL-only; queued as v5.5 follow-up `mcp-queue-config-aware` (closes pre-existing bug separately). | +0.5 days vs single-site plan. Honest scope. |
| **E3 — bus only (1 site, original plan)** | bus.rs converts; cli + mcp untouched. | Smallest scope. Two known parallel dispatch sites remain; cli's private enum becomes increasingly out-of-sync. |

**Plan default: E2** — bus + cli (2 sites), MCP deferred with explicit follow-up. Reasons:
- cli's `open_backend` is small (~30 lines) and shares the same enum-based pattern as bus.rs — mechanical to convert in the same Phase 3.
- MCP's gap is a different concern (queue-config.yaml awareness, not just registry dispatch); fixing it requires plumbing the bus's registry into the MCP tools' handler. Realistically a separate epic.
- Documented in V5-MOD-3 close-out as a known v5.5 follow-up; not a regression introduced by V5-MOD-3.

**Alternative arguments:**
- E1 closes the MCP bug atomically; Theme B fully done. Rejected: scope creep — V5-MOD-3 was sized as S (3-5 days); E1 pushes it to M.
- E3 leaves cli with a parallel private enum that drifts. Rejected: false economy.

## Forks — RESOLVED 2026-05-02 (operator pin: all 5 plan defaults)

| Fork | Resolution | Rationale (recap) |
|---|---|---|
| **A** | **A2 — `Send + Sync`** trait bound; internalize `SqliteBackend` Mutex; methods `&mut self` → `&self` | SDK consistency with V5-MOD-1/2 (both promote to `Send + Sync`); call-site syntax stays clean (no `lock()` ceremony) |
| **B** | **B1 — `BackendKind` enum → `String`** + factory gains `supports_ack()`; `validate()` becomes registry-runtime check | YAML wire format unchanged (lowercase strings). Matches V5-MOD-1's `ScoringSourceConfig.source_type: String` pattern. Backward compat preserved |
| **C** | **C2 — `examples/queue-backend-memory/`** (pure logic + `RwLock<HashMap<String, BTreeSet<u64>>>` ack tracking) | Third SDK example covers ack semantics; pure-logic pairs with V5-MOD-1's HTTP-fetch + V5-MOD-2's FS-read |
| **D** | **D3 — re-resolve backend-name from `QueueConfig`** at display time | No trait surface addition; preserves V5-MOD-2's no-`name()`-on-`Sensor` precedent |
| **E** | **E2 — bus + cli (2 sites)**; MCP deferred to v5.5 follow-up `mcp-queue-config-aware` | Honest scope (V5-MOD-3 stays S effort). cli's parallel-enum drift closed; MCP gap is a separate pre-existing concern |

## Forks — pre-pin debate (kept on file for traceability)

### Fork A — `Send + Sync` trait bound (the load-bearing decision)

The trait today is `pub trait QueueBackend: Send`. `SqliteBackend` holds a
`rusqlite::Connection` which is `!Sync` (rusqlite is single-threaded by design),
so `SqliteBackend` itself is `Send` but not `Sync`. The current code compensates
by wrapping in `Arc<Mutex<SqliteBackend>>` inside `BackendHandle::Sqlite`.

V5-MOD-3 needs to dispatch via some `dyn` form of the backend. Three options:

| Option | Trait bound | What backends look like | Cost |
|---|---|---|---|
| **A1 — `Send` only** | `pub trait QueueBackend: Send` | Registry returns `Arc<Mutex<dyn QueueBackend>>` per topic; ALL backends pay the Mutex cost (including JSONL which doesn't need it). Caller-side: `handle.lock().unwrap().append(...)` everywhere. | Most permissive; no impl breakage. Ugliest call-site syntax. |
| **A2 — `Send + Sync`** | `pub trait QueueBackend: Send + Sync` | Registry returns `Arc<dyn QueueBackend>` directly. JSONL backends naturally fit (stateless or `RwLock`-internal). SQLite backends need to internalize the `Mutex` — `pub struct SqliteBackend { conn: Mutex<Connection> }` and method receivers become `&self` instead of `&mut self`. | Cleaner call sites. Breaks `SqliteBackend`'s public surface (no longer has `&mut self` methods). Third-party authors must internalize their own !Sync state. |
| **A3 — Two traits, dispatch on category** | `pub trait QueueBackend: Send + Sync` + `pub trait MutQueueBackend: Send` | Registry holds either kind; callers pick. | Most flexible. Worst complexity — two parallel trait surfaces in the SDK. |

**Plan default: A2** — promote to `Send + Sync`, internalize the SQLite Mutex.
Reasons:
- Matches V5-MOD-1's `ScoringSource: Send + Sync` and V5-MOD-2's `Sensor: Send + Sync`. SDK consistency wins.
- Call-site syntax is clean: `arc_backend.append(msg)` — no `lock()` ceremony.
- The `SqliteBackend` refactor is small (~30 lines: move Mutex inside, change method receivers, update internal callers). One commit.
- Third-party authors who hold !Sync state internalize their own Mutex; matches Rust convention for shared-state types.

**Alternative arguments:**
- A1 (Send-only) preserves `SqliteBackend`'s current surface verbatim. Rejected: `&mut self` on a trait is hostile to SDK consumers; shared-state types should hide their lock.
- A3 (two traits) doubles the surface and the conformance suite. Rejected: cargo-culted complexity for a marginal flexibility win.

### Fork B — `BackendKind` enum vs open string (REVISED post-plan-critic 🔴 B2)

Today `BackendKind` is a closed-set serde enum (`Jsonl | Sqlite`, lowercased
in YAML). To accept third-party backend names (`postgres`, `redis`, `memory`),
the wire format opens.

| Option | Wire shape | Migration |
|---|---|---|
| **B1 — Replace enum with `String`** | `pub backend: String` (and `Option<String>` in YAML) | Accept any string. Registry lookup at dispatch time decides whether the named backend is registered; unknown names error at startup. |
| **B2 — Keep enum, add `Other(String)` variant** | `BackendKind::Jsonl \| Sqlite \| Other(String)` | Closed-set + escape hatch. `serde(untagged)` or custom Deserialize to accept raw strings as `Other`. |
| **B3 — Hybrid: enum aliases for built-ins, string for third-party** | Two-level: `BackendKind::Builtin(BuiltinKind) \| Custom(String)` | Type-safety for built-ins; flexibility for third-party. |

**Plan default: B1** — replace with `String`. Reasons:
- Simplest. The "is this backend registered?" check moves from the type system to runtime, but `queue-config.yaml` validation already runs at startup.
- Matches how `ScoringSourceConfig.source_type` works in V5-MOD-1 (raw String, registry-validated).
- Backward compat preserved: existing YAML strings (`jsonl`, `sqlite`) continue to deserialize. The wire format doesn't change for adopters; only the in-memory type does.

**Alternative arguments:**
- B2 (Other variant) preserves type-safety for built-ins. Rejected: doubles `match` arms everywhere built-ins are dispatched (`Jsonl | Sqlite | Other(s)`); same ergonomic as B1 with extra structure.
- B3 (two-level) is over-engineered for the use case.

**Plan-critic 🔴 B2 fix — `validate()` rewrite required:** the existing
`queue_config::validate()` at `:159-171` enforces a type-system invariant:

```rust
if cfg.ack_required && cfg.backend != BackendKind::Sqlite {
    return Err(...);
}
```

After Fork B1, `cfg.backend` is `String` and the right-hand side is no
longer expressible at the type level. The cleanest rewrite makes the
invariant a registry-runtime check by adding a `supports_ack()` method to
the factory trait:

```rust
pub trait QueueBackendFactory: Send + Sync {
    fn name(&self) -> &'static str;
    fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>>;

    /// True iff backends produced by this factory support ack semantics.
    /// `queue_config::validate()` checks this when a topic declares
    /// `ack_required: true` — declaring ack on a non-ack backend is a
    /// startup-time configuration error.
    fn supports_ack(&self) -> bool { false }
}
```

`validate()` is then promoted to `validate(&self, registry: &QueueBackendRegistry)`
— takes the registry as a parameter so it can resolve names. Existing
callers (the YAML loader at `queue_config.rs:228+` + `neurogrim doctor`
checks if any) get the registry threaded through.

**Compile-time follow-on:** `BackendKind` was `Copy + Hash`. After flipping
to `String`, every downstream `.backend` clone/copy in tests
(`queue_config.rs:225`, `:341`, `:777`, `:807`, `:965`) and `bus.rs:336`
(`cfg.lookup(topic).backend`) loses Copy semantics. ~6 mechanical sites,
list captured in Phase 0 audit.

### Fork C — Example crate name + scope

Plan default name from the epic: `queue-backend-postgres`. Real PostgreSQL is
heavy (full DB connection pool, schema migration). Alternatives:

| Option | Crate | Scope |
|---|---|---|
| **C1 — `queue-backend-postgres` (stub)** | Real Postgres-shaped API; uses `tokio-postgres` but with stub-only behavior (no actual DB connect). | Epic-mentioned. Demonstrates real-world deployment shape; pulls heavy dep. |
| **C2 — `queue-backend-memory`** | In-memory `Vec<QueueMessage>` ring buffer with ack tracking. Pure logic, no I/O. | Minimal deps. Useful for testing third-party code paths. Pairs with V5-MOD-2's `sensor-readme-quality` (pure-FS) and V5-MOD-1's `scoring-source-prom` (HTTP-fetch) — three complementary patterns. |
| **C3 — `queue-backend-redis`** | Real Redis-shaped via `redis` crate. Common in operator deployments. | Heavy dep. Realistic. |

**Plan default: C2** — `queue-backend-memory`. Reasons:
- Pure logic, zero deps beyond `neurogrim-core`. Mirrors V5-MOD-2's `sensor-readme-quality` minimal-deps approach.
- Demonstrates the FULL trait surface including ack semantics (the V5-MOD-1's HTTP-fetch + V5-MOD-2's FS-read examples didn't exercise ack — this fills the gap).
- Genuinely useful for tests / integration: a `MemoryQueueBackend` with bounded capacity is the obvious choice when consumers want to avoid the disk in unit tests.
- Three SDK example crates with three complementary patterns: HTTP-fetch (`scoring-source-prom`), FS-read (`sensor-readme-quality`), in-memory (`queue-backend-memory`).

**Alternative arguments:**
- C1 (postgres-stub) follows the epic literally but ages poorly + the stub-vs-real mismatch is confusing.
- C3 (redis) is realistic but heavy.

**Plan-critic 🟡 C2 fix — ack data shape correction:** the initial draft said
`MemoryQueueBackend` would use `RwLock<HashMap<String, u64>>` for "per-consumer-group
ack offsets" (a high-water mark). **This is wrong** — SQLite's ack schema at
`queue_backend.rs:251-256` stores per-(group, offset) ROWS because acks may
arrive out of order (the test `sqlite_last_acked_tracks_max` at `:729-742`
exercises this: acks 2, 4, 1 in that order). A high-water mark cannot
represent "acked: {1, 2, 4}; pending: {3, 5+}".

**Corrected data shape:**
```rust
pub struct MemoryQueueBackend {
    log: RwLock<Vec<StoredMessage>>,
    /// Per-consumer-group set of acked offsets. `BTreeSet` so
    /// `read_unacked` can iterate ascending.
    acks: RwLock<HashMap<String, BTreeSet<u64>>>,
    capacity: Option<usize>,
}
```

`read_unacked` filters `log` by "offset not in `acks[group]`". `last_acked`
returns `acks[group].iter().max().copied()` (matches SQLite semantics
exactly — confirmed by re-reading the SQLite test).

## Phases (incremental delivery)

### Phase 0 — Setup + audits (Day 1, ~0.5 day) — PREREQUISITE

1. **`Send + Sync` audit.** Read `queue_backend.rs` end-to-end. Verify which methods are `&self` vs `&mut self`. List all internal callers + tests. Capture in commit message.
2. **`BackendHandle` call-site audit.** Run `grep -rn BackendHandle:: neurogrim/crates/` to enumerate every dispatch site that gets touched in Phase 3. Scope check.
3. **Anchor correction note** for the epic file.

**Ship criterion:** audit captured; no code changes.

### Phase 1 — `QueueBackendFactory` + `QueueBackendRegistry` (~0.5 day)

**Goal:** Define the new types in `neurogrim-core/src/queue_backend.rs` (same module). Pure additive; no dispatch wired yet.

**Files (modified):**
- `neurogrim-core/src/queue_backend.rs` (additive)

**Trait shape (assumes Fork A2 + A1's rejected; trait promoted to `Send + Sync`):**
```rust
pub trait QueueBackend: Send + Sync {
    // existing methods, but receivers change `&mut self` → `&self`
    fn append(&self, msg: &QueueMessage) -> Result<u64>;  // was &mut self
    fn read_from(&self, since_offset: u64, limit: usize) -> Result<Vec<StoredMessage>>;
    fn len(&self) -> Result<u64>;
    // ack methods unchanged shape; receivers change too
    fn supports_ack(&self) -> bool { false }
    fn read_unacked(&self, _consumer_group: &str, _limit: usize) -> Result<Vec<StoredMessage>> { … }
    fn ack(&self, _offset: u64, _consumer_group: &str) -> Result<()> { … }
    fn last_acked(&self, _consumer_group: &str) -> Result<Option<u64>> { Ok(None) }
}

pub trait QueueBackendFactory: Send + Sync {
    fn name(&self) -> &'static str;
    /// Construct a backend bound to a topic at `topic_path`. The
    /// factory is responsible for any per-topic state initialization
    /// (open SQLite connection, etc.).
    fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>>;
}

pub struct QueueBackendRegistry {
    factories: HashMap<&'static str, Box<dyn QueueBackendFactory>>,
}
// new / register / register_all / get / build / has / registered_names / len / is_empty
```

**Why factory takes `&Path` not just registers stateless:** Queue backends are
*per-topic* — you need one `SqliteBackend` per `.sqlite` file, one `JsonlBackend`
per `.jsonl` file. The factory's `build(&self, topic_path: &Path)` produces a
backend bound to that file. This matches the existing `BackendHandle::Jsonl(PathBuf)` /
`BackendHandle::Sqlite(Arc<Mutex<...>>)` shape — backends ARE per-topic.

**Tests (Phase 1):**
- Object-safety guards (compile-time)
- Mock `QueueBackend` + `QueueBackendFactory` exercise `Arc<dyn>` + `Box<dyn>` round-trip
- Registry empty / register / get / has / len / last-write-wins (mirror V5-MOD-2 Phase 1's tests)

**Ship criterion:** new types compile; tests green; no dispatch changes yet.

### Phase 2 — Promote `SqliteBackend` to `Send + Sync` (~0.5 day)

**Goal:** Internalize the SQLite Mutex. `SqliteBackend` becomes
`pub struct SqliteBackend { conn: Mutex<Connection>, … }`; methods change from
`&mut self` to `&self` and `lock()` internally.

**Files (modified):**
- `neurogrim-core/src/queue_backend.rs` (the `SqliteBackend` impl block under `#[cfg(feature = "sqlite")]`)

**Migration:**
```rust
pub struct SqliteBackend {
    // Was `conn: Connection`; now wrapped to make the type Sync.
    conn: std::sync::Mutex<Connection>,
}

impl QueueBackend for SqliteBackend {
    fn append(&self, msg: &QueueMessage) -> Result<u64> {  // was &mut self
        let conn = self.conn.lock()
            .map_err(|_| anyhow!("sqlite mutex poisoned"))?;
        // existing logic against `conn`
    }
    // …same for ack methods
}
```

`JsonlBackend` is already `Send + Sync` (holds only `PathBuf`); no changes
needed.

**Caller updates:** anyone holding `&mut SqliteBackend` switches to `&SqliteBackend`.
The Phase 0 audit captures the call sites; from recon, only `dashboard/src/bus.rs`
+ the in-module tests touch this surface.

**Tests:** existing 8+ shared property suite tests must pass unchanged. The
Phase 5 conformance suite (later phase) is the more rigorous check, but at
Phase 2 we just need green builds.

**Ship criterion:** `SqliteBackend` is `Send + Sync`; existing tests green;
`bus.rs::BackendHandle` may need `&mut`/`&` adjustments to compile (one-line
fixes).

### Phase 3 — Convert `bus.rs::BackendHandle` to registry-based dispatch (~1 day)

**Goal:** Replace the `match kind { ... }` at `bus.rs:340-348` with
`registry.build(name, topic_path)?` lookup. Remove `BackendHandle` wrapper enum.

**Files (modified):**
- `neurogrim-dashboard/src/bus.rs` — drop `BackendHandle` enum; replace its 6 dispatch methods (~100 lines) with direct calls on `Arc<dyn QueueBackend>`.
- `neurogrim-core/src/queue_config.rs` — `BackendKind` enum becomes `pub backend: String` (Fork B1). Existing YAML serialization preserves backward compat.
- `neurogrim-cli/src/main.rs` (or wherever the registry is built) — initialize the registry with built-in factories at startup.

**Migration shape:**
```rust
// dashboard/src/bus.rs — was:
let handle = match kind {
    BackendKind::Jsonl => Arc::new(BackendHandle::Jsonl(jsonl_topic_path(...))),
    BackendKind::Sqlite => {
        let path = sqlite_topic_path(...);
        let be = SqliteBackend::open(&path)?;
        Arc::new(BackendHandle::Sqlite(Arc::new(Mutex::new(be))))
    }
};

// after:
let backend_name = cfg.lookup(topic).backend.as_str();  // String, was BackendKind
let topic_path = topic_path_for_backend(project_root, topic, backend_name);
let handle: Arc<dyn QueueBackend> = registry.build(backend_name, &topic_path)
    .ok_or_else(|| anyhow!("unknown queue backend: {backend_name}"))?;
```

**Risk: existing operators with `queue-config.yaml` files** — `BackendKind` was
serde'd as a serde-enum. After Fork B1, it's a plain string. Same wire format
on both sides (lowercase strings); existing YAML files deserialize unchanged.
Verify with the existing parse tests in `queue_config.rs`.

**Tests (Phase 3):**
- All existing `bus.rs` tests pass (regression bar)
- New: register a fake `MockQueueBackend` factory + topic config using `backend: "mock"` + verify dispatch routes through the mock

**Ship criterion:** workspace tests green; smoke test: `neurogrim queue stats`
on a real project produces identical output to pre-V5-MOD-3.

### Phase 4 — Conformance suite (~1 day)

**Goal:** Generalize the existing internal test fns into a public
`queue_backend_conformance::run_factory_conformance` matching V5-MOD-2's
shape. ≥10 tests.

**Files (new):**
- `neurogrim-core/src/queue_backend_conformance.rs`

**Test count target ≥10:**

Cross-cutting (3 — port from V5-MOD-2):
1. `factory_name_non_empty`
2. `factory_name_stable_across_calls`
3. `factory_build_repeatable` — repeated `build(topic_path)` doesn't panic

Backend-specific (7+ — generalize the existing `queue_backend.rs::run_*` suite):
4. `appended_messages_round_trip` — append N, read all back, verify ordering
5. `read_from_offset` — append, read with `since_offset > 0`, verify slice
6. `read_with_limit` — limit honored
7. `len_after_append` — len() reflects appended count
8. `concurrent_append_safety` — N parallel append() calls; offsets are unique + ascending
9. `ack_methods_consistent_with_supports_ack` — if `supports_ack()` returns true, `read_unacked` / `ack` / `last_acked` work; if false, default impls fire
10. `factory_build_returns_send_sync` — compile-time check that `Arc<dyn QueueBackend>` from `build()` is `Send + Sync`

**Self-validation tests** (in-module, exercise the suite against `Good` /
`Bad` / `Err` mock backends — V5-MOD-2 pattern).

**Built-in factory tests:** `JsonlBackend` + `SqliteBackend` factories pass
the suite. Tests gated by `#[cfg(feature = "sqlite")]` for the SQLite case.

**Ship criterion:** suite green against both built-ins; documented as the
contract any third-party impl must pass.

### Phase 5 — Out-of-tree example: `examples/queue-backend-memory/` (~1 day)

**Goal:** Pure-logic in-memory queue backend that demonstrates the full trait
surface (including ack semantics). Pairs with V5-MOD-1's `scoring-source-prom`
(HTTP) and V5-MOD-2's `sensor-readme-quality` (FS-read) — three SDK examples,
three complementary patterns.

**Files (new):**
- `examples/queue-backend-memory/Cargo.toml`
- `examples/queue-backend-memory/src/lib.rs`
- `examples/queue-backend-memory/tests/conformance.rs`
- `examples/queue-backend-memory/README.md`

**Behavior:** `MemoryQueueBackend` holds `RwLock<Vec<StoredMessage>>` for the
log + `RwLock<HashMap<String, u64>>` for per-consumer-group ack offsets.
Bounded capacity via constructor; oldest messages dropped when full (FIFO).
Ack semantics fully supported (`supports_ack: true`).

**Failure modes:** `Err(...)` only on capacity-exceeded with `bail_on_full: true`;
default behavior is silent FIFO eviction with a `tracing::warn!` per drop.

**Conformance:** `tests/conformance.rs` runs `run_factory_conformance(...)`
against `MemoryQueueBackendFactory`; asserts all ≥10 tests pass.

**Ship criterion:** `cargo build -p queue-backend-memory` clean; conformance
test green; README has a third-party Cargo.toml template.

### Phase 6 — Epic close-out + LSP-Brains spec sync (~0.5 day)

- Update `v5-modular-conversions.md`: mark V5-MOD-3 status COMPLETE; check off
  Done-When; **mark Theme B as a whole COMPLETE** (V5-MOD-1 + V5-MOD-2 +
  V5-MOD-3 all shipped).
- Update `v5-sdk.md`: V5-MOD-3 hand-off note (mirror of V5-MOD-1/2's notes);
  re-export contract for `QueueBackend` + `QueueBackendFactory` +
  `QueueBackendRegistry` + `queue_backend_conformance::run_factory_conformance`.
- LSP-Brains spec: queue/bus is implementation-specific (NeuroGrim-only — not
  spec'd); no spec sync needed beyond a one-line cross-reference if the spec
  mentions persistence anywhere. Recon at Phase 6.

## Files inventory

### New
- `neurogrim-core/src/queue_backend_conformance.rs` (Phase 4)
- `examples/queue-backend-memory/{Cargo.toml,src/lib.rs,tests/conformance.rs,README.md}` (Phase 5)

### Modified
- `neurogrim-core/src/queue_backend.rs` (Phase 1: add factory + registry; Phase 2: SqliteBackend Send+Sync)
- `neurogrim-core/src/queue_config.rs` (Phase 3: `BackendKind` enum → String)
- `neurogrim-core/src/lib.rs` (Phase 1 + 4: re-exports)
- `neurogrim-dashboard/src/bus.rs` (Phase 3: drop `BackendHandle`, route through registry)
- `neurogrim-cli/src/main.rs` (Phase 3: register built-in factories at startup)
- `neurogrim/Cargo.toml` (Phase 5: workspace member add)
- `roadmap/epics/v5-modular-conversions.md` (Phase 6: V5-MOD-3 + Theme B both COMPLETE)
- `roadmap/epics/v5-sdk.md` (Phase 6: V5-MOD-3 hand-off note)

## Risks (from V5-MOD-1/2 lessons + new ones)

🟡 **`SqliteBackend` Send+Sync conversion** — moving Mutex inside changes the
public surface (no more `&mut self` methods). Mitigation: Phase 0 audit
confirms only `dashboard/src/bus.rs` + in-module tests use the type;
Phase 2 + Phase 3 land together if needed.

🟡 **`BackendKind` String migration** — `queue-config.yaml` is operator-edited.
Wire format unchanged (still lowercase strings); only in-memory type
flips from enum to String. Backward compat preserved by reusing existing
serde paths. Mitigation: existing `queue_config.rs` parse tests catch any
deserialization regression.

🟡 **Registry initialization site** — V5-MOD-1 + V5-MOD-2 both register
factories from `main.rs`. The bus runs in dashboard + cli + maybe MCP;
ensuring all three register the same set is the consistency hazard.
Mitigation: a `neurogrim_sensory::built_in_factories()`-style helper —
`neurogrim_core::queue_backend::built_in_factories()` returning the
JSONL + SQLite factories. Single canonical list; consumers call
`registry.register_all(...)`.

🟢 **Perf** — no perf gate this story (dispatch is per-topic, not per-message;
the hot path doesn't change). Same posture as V5-MOD-2.

🔵 **Suggestion — `--list-queue-backends` CLI flag** — operator visibility into
which factories are registered. Forwarded from V5-MOD-1/2's same suggestion;
v5.5 polish.

## Iteration boundaries

| Iter | Phases | Shippable? | Rough duration |
|---|---|---|---|
| 0 | Phase 0 (audit) | Yes — audit only, no code change | ~0.5 day |
| 1 | Phase 1 + 2 (factory + registry + SqliteBackend Send+Sync) | Yes — additive trait + impl change; existing tests green | ~1 day |
| 2 | Phase 3 (dispatch conversion + BackendKind String) | Yes — semantics unchanged, dispatch through registry | ~1 day |
| 3 | Phase 4 (conformance suite) | Yes — third-party-impl contract documented | ~1 day |
| 4 | Phase 5 (example crate) | Yes — modularity proven | ~1 day |
| 5 | Phase 6 (close-out + Theme B mark) | Yes — Theme B closed | ~0.5 day |

Total: ~5 days. Within epic S estimate (3–5 days).

## Verification (end-to-end, after Iter 5)

1. `cargo test --workspace -- --test-threads=1` green.
2. `neurogrim queue stats` against a real project produces identical output to pre-V5-MOD-3 (smoke).
3. `cargo build -p queue-backend-memory` succeeds; conformance test green.
4. Third-party backend can be registered via `registry.register(Box::new(MemoryQueueBackendFactory))` and used via `queue-config.yaml`'s `backend: "memory"` entry.
5. Conformance suite passes against `JsonlBackendFactory`, `SqliteBackendFactory` (under `sqlite` feature), and `MemoryQueueBackendFactory`.

## What this plan does NOT do

- Does **not** add dynamic plugin loading (cdylib/libloading) — same v5.5
  BACKLOG B-40 deferral as V5-MOD-1/2.
- Does **not** add the `--list-queue-backends` CLI flag — v5.5 polish.
- Does **not** generalize the YAML config schema beyond opening the
  `backend` field — third-party backends might want backend-specific options
  (e.g., Postgres connection string) that the current schema doesn't model.
  That's a v5.5 follow-up if real adopters demand it; for now,
  backend-specific options come via env vars or a parallel
  `<topic>-options.yaml` file.

## Cross-references

- Epic: `roadmap/epics/v5-modular-conversions.md` § V5-MOD-3
- V5-MOD-1 plan (substrate): `.claude/plans/v5-mod-1-scoring-source-trait.md`
- V5-MOD-2 plan (substrate): `.claude/plans/v5-mod-2-sensor-trait.md`
- Existing trait: `neurogrim-core/src/queue_backend.rs:69`
- Existing dispatch: `neurogrim-dashboard/src/bus.rs:65-72` (BackendHandle) + `:340-348` (build site)
- Existing wire-format: `neurogrim-core/src/queue_config.rs:49` (BackendKind enum)
- V5-MOD-1 + V5-MOD-2 trait + factory + registry + conformance + example pattern is the V5 modularity substrate; V5-MOD-3 follows the same template.
