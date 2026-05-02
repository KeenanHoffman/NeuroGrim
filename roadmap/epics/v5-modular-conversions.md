# Epic: v5 Modular Conversions (Theme B)

**Theme:** B
**Release:** v5 (entry pinned 2026-05-01; this epic is **gated on Theme A close** plus a re-check of the concurrent-v4.x-work risk before V5-MOD-1's 5% perf-gate runs — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** PLANNED (drafted 2026-05-01)
**Priority:** Core scope of v5 — three high-leverage trait conversions; "everything is an interface" was rejected as wider scope
**Goal:** Convert three highest-leverage seams to trait + factory pattern. `ScoringSource` becomes `Box<dyn ScoringSource>` with factory registry; `Sensor` trait converts the existing sensors with cargo-feature-gate discovery (dynamic loading deferred to v5.5); `QueueBackend` factory replaces `BackendHandle` enum. Each ships a conformance suite.

**Depends on:**
- Theme A complete (V5-FOUND-1..4 — diagnostics for measuring perf impact, fast test loop for iteration)
- S12 publish gates (CI semver discipline)

**Blocks:**
- Theme C (SDK extraction can only stabilize trait shapes after they're real)

**Master roadmap:** `roadmap/v5-roadmap.md`
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

---

## Theme B Is Done When

- [ ] `ScoringSource` trait + factory registry live in `neurogrim-core`; existing JSONL/A2A/file scoring sources reimplemented as factories
- [ ] `ScoringSource` conformance suite (≥8 tests including negative-path) any factory must pass
- [ ] Example out-of-tree crate `examples/scoring-source-prom/` registers successfully without forking core
- [ ] Scoring round-trip latency unchanged within 5% of S15 baseline (dyn-dispatch perf gate)
- [ ] `Sensor` trait defined; all existing sensors are impls; cargo-feature gates discovery
- [ ] `neurogrim-sensory` no longer depends on `neurogrim-cli` (inverse-coupling fix)
- [ ] Example out-of-tree sensor crate compiles + integrates via cargo feature
- [ ] `Sensor` conformance suite enforces JSON-schema CMDB output
- [ ] LSP-Brains spec §F (MCP sensory tools) updated to reflect plugin shape
- [ ] `BackendHandle` enum replaced by `Arc<dyn QueueBackend>` factory dispatch
- [ ] `queue-config.yaml` supports user-registered backend types
- [ ] Existing `JsonlBackend` + `SqliteBackend` pass conformance suite unchanged
- [ ] Example crate demonstrates third-party queue backend registration

---

## Stories

### V5-MOD-1: ScoringSource trait + factory registry (~7–10 days)

**Status:** Planned
**Effort:** M
**Depends on:** V5-FOUND-1 (diagnostics for measuring perf impact)

**What:** Refactor [registry.rs:135–157](../crates/neurogrim-core/src/registry.rs) string-dispatch (currently matches on `source_type` ∈ {"jsonl", "a2a", "file"}) into `Box<dyn ScoringSource>` with a factory-registration pattern (`inventory` crate or static registry table). Built-in factories preserved verbatim; third-party crates can register their own.

**Why:** This is the highest-leverage seam in the codebase. ScoringSource dispatch is currently a string match — every new source type requires forking core. Factory pattern unblocks third-party scoring sources (Python, Wasm, HTTP plugin, Prometheus) without registry hardcoding.

**Architectural decision: dyn vs generic-bounded.** Default to `Box<dyn ScoringSource>` (object-safe trait, easier registration). If V5-MOD-1 perf gate fails, fall back to generic-bounded with a small enum for built-ins. Decision recorded in epic when V5-MOD-1 ships.

**Done when:**
- [ ] `ScoringSource` trait defined in `neurogrim-core/src/scoring_source.rs`
- [ ] Factory registry (`inventory` crate or static table); built-in JSONL, A2A, file factories preserved
- [ ] Conformance test suite (≥8 tests covering happy path + 4 negative paths: malformed config, unreachable endpoint, schema violation, factory panic)
- [ ] Example out-of-tree crate `examples/scoring-source-prom/` registers successfully without forking core
- [ ] **Performance gate:** scoring round-trip latency unchanged within 5% of S15 baseline (measured via V5-FOUND-1 diagnostics ledger). If gate fails, revisit dispatch pattern before continuing Theme B.
- [ ] LSP-Brains spec §9 (fractal composition) cross-reference updated if needed

### V5-MOD-2: Sensory plugin interface (cargo-feature gates first) (~10–14 days)

**Status:** Planned
**Effort:** L
**Depends on:** V5-MOD-1 (factory pattern proven on scoring source first)

**What:** Define a `Sensor` trait in `neurogrim-core`. Convert existing sensors in `neurogrim-sensory` to impls. First iteration: discovery via cargo-feature gates (static, compile-time). Defer dynamic loading (cdylib + libloading) to v5.5 successor pipeline (BACKLOG B-40) — adversary check: dynamic loading is high-risk for a small win at this stage. Also fix the inverse-coupling smell: `neurogrim-sensory` currently depends on `neurogrim-cli`; flip the dependency direction.

**Why:** Sensors are the second-highest-leverage modularity seam. Today's compile-time-only catalogue means users cannot author new sensors (custom CMDB producers) without forking. Cargo-feature gates give static-time discovery without the ABI risks of dynamic loading.

**Architectural decision: static cargo features over dynamic loading.** Dynamic loading via `cdylib` + `libloading` is harder than it sounds (ABI churn, sandboxing, panic safety). Static cargo features cover 90% of use cases (users who want custom sensors can fork-and-feature) without the operational risk. Dynamic loading deferred to BACKLOG B-40 with explicit triggers.

**Done when:**
- [ ] `Sensor` trait defined in `neurogrim-core/src/sensor.rs`
- [ ] All existing sensors in `neurogrim-sensory/src/` are impls
- [ ] Discovery via cargo features (e.g. `--features sensors-rustsec,sensors-npm`)
- [ ] **Coupling fix:** `neurogrim-sensory` no longer depends on `neurogrim-cli`
- [ ] Conformance suite: any `Sensor` impl produces a CMDB entry matching the JSON schema (≥6 tests including negative paths: malformed CMDB, panic, timeout)
- [ ] Example out-of-tree sensor crate compiles + integrates via cargo feature
- [ ] Spec impl alignment: LSP-Brains spec §F (MCP sensory tools) updated to reflect plugin shape

### V5-MOD-3: Queue backend factory (low-cost win) (~3–5 days)

**Status:** Planned
**Effort:** S
**Depends on:** V5-MOD-1 (factory pattern reused)

**What:** Convert `BackendHandle` enum at [queue_backend.rs:65–72](../crates/neurogrim-core/src/queue_backend.rs) to `Arc<dyn QueueBackend>` factory dispatch. Already trait-based; this is registration plumbing only. Document custom-backend authoring; ship a third-party PostgreSQL example (or stub) as proof.

**Why:** Lowest-cost modularity win. Trait already exists; only the dispatch is hardcoded. Generalizes the per-topic backend choice from {jsonl, sqlite} to "any registered backend" — opens enterprise storage flexibility (PostgreSQL, DynamoDB, Kafka) without changing core.

**Done when:**
- [ ] `BackendHandle` enum replaced by `Arc<dyn QueueBackend>` factory
- [ ] `queue-config.yaml` supports user-registered backend types
- [ ] Existing `JsonlBackend` + `SqliteBackend` pass conformance suite unchanged
- [ ] Example crate `examples/queue-backend-postgres/` (or stub) demonstrates third-party backend registration
- [ ] Conformance suite generalized from current 2-backend tests (≥10 tests covering append, read, ack, malformed entry, factory failure, retention)

---

## Verification (end-to-end smoke per story)

**V5-MOD-1 ScoringSource trait + factory:**
- Build the example out-of-tree crate `examples/scoring-source-prom/`; verify it loads in a real `neurogrim score` run
- Run scoring round-trip against the diagnostics ledger (V5-FOUND-1); confirm latency within 5% of the V5-FOUND-1-captured S15 baseline (PERF GATE — if fails, halt Theme B and revisit dispatch pattern)
- Run conformance suite against built-in JSONL/A2A/file factories; confirm 8/8 + 4 negative-path tests pass

**V5-MOD-2 Sensor plugin interface:**
- Build the example out-of-tree sensor crate; verify CMDB entry produced matches JSON schema
- Run `cargo build --features sensors-rustsec,sensors-npm` (subset of features); confirm only enabled sensors compile in
- Verify `neurogrim-sensory/Cargo.toml` no longer declares `neurogrim-cli` as a dep (inverse-coupling fix)
- Run conformance suite against built-in sensors; confirm all pass + ≥3 negative-path tests

**V5-MOD-3 Queue backend factory:**
- Build the example out-of-tree queue backend crate (PostgreSQL or stub); confirm it registers via `queue-config.yaml`
- Run conformance suite against built-in `JsonlBackend` + `SqliteBackend`; confirm both pass unchanged
- Append-then-read a message via the new dispatch path; confirm round-trip parity with prior enum dispatch

---

## Risks (adversary concerns brought forward)

🔴 **BLOCKING — performance regression risk on scoring path.** Dyn dispatch on hot scoring path could regress latency. V5-MOD-1 acceptance criterion: scoring round-trip latency unchanged within 5% of S15 baseline. **If it regresses, revisit dispatch pattern (generic-bounded + small enum) before continuing Theme B.** Diagnostics ledger from V5-FOUND-1 is the measurement instrument.

🟡 **Conformance-suite coverage gaps.** Each trait's conformance must include negative-path tests (impl returns malformed CMDB; impl panics; impl times out). "Passes happy path only" is too weak a guarantee for third-party impls. Mitigation: each story's Done When requires ≥4 negative paths.

🟡 **Inverse coupling.** `neurogrim-sensory → neurogrim-cli` is reverse of ideal. V5-MOD-2 fixes it as part of the trait conversion. Risk: cli has helpers sensory uses; flipping requires extracting those helpers to a third location.

🟡 **Plugin discovery confusion.** Users may expect runtime plugin loading; v5 ships compile-time only. Mitigation: documented prominently in `neurogrim explain sensors` + composition guide (Theme D); BACKLOG B-40 lists the explicit triggers for dynamic loading.

🔵 **Suggestion: ship a `--list-impls` flag per trait surface.** `neurogrim score --list-scoring-sources`, `neurogrim sense --list-sensors`, `neurogrim queue --list-backends` so operators can verify which factories are registered in their build. v5.5 polish, not v5 scope.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Existing scoring source dispatch: [registry.rs:135–157](../crates/neurogrim-core/src/registry.rs)
- Existing sensory crate: [neurogrim-sensory/src/lib.rs](../crates/neurogrim-sensory/src/lib.rs)
- Existing queue backend trait: [queue_backend.rs:69](../crates/neurogrim-core/src/queue_backend.rs)
- LSP-Brains spec §9 (fractal composition), §F (MCP sensory tools) — updates expected
- Successor pipeline: BACKLOG B-37..B-40 (v5.5 trimmed items)
