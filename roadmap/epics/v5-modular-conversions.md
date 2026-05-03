# Epic: v5 Modular Conversions (Theme B)

**Theme:** B
**Release:** v5 (entry pinned 2026-05-01; this epic is **gated on Theme A close** plus a re-check of the concurrent-v4.x-work risk before V5-MOD-1's 5% perf-gate runs — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** IN PROGRESS — V5-MOD-1 **COMPLETE** 2026-05-02 (commit `fb45d5d` close-out), V5-MOD-2 **COMPLETE** 2026-05-02 (commit `459d9ec` Phase 6 + Phase 7 close-out); V5-MOD-3 PLANNED
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

- [x] `ScoringSource` trait + factory registry live in `neurogrim-core`; existing **cmdb / a2a / function** scoring sources reimplemented as factories *(was "JSONL / A2A / file" in the original draft — corrected at V5-MOD-1 close 2026-05-02; the actual built-in source-types are `cmdb`, `a2a`, `function`)*
- [x] `ScoringSource` conformance suite (≥8 tests including negative-path) any factory must pass *(8 cross-cutting tests in `neurogrim_core::scoring_source_conformance::run_factory_conformance`; per-source negative-path tests in each source's own module)*
- [x] Example out-of-tree crate `examples/scoring-source-prom/` registers successfully without forking core *(V5-MOD-1 Phase 6, 2026-05-02)*
- [⚠️] Scoring round-trip latency unchanged within 5% of S15 baseline (dyn-dispatch perf gate) *(strict 5% ceiling unverifiable on the dev host due to ~15ms p95 system drift since the V5-FOUND-1 baseline; Option A fallback — `BuiltinScoringSource` enum + inherent `async fn` — is **measurably faster than the initial `Box<dyn>` impl** on the same host (p95 −3 ms, p99 −5 ms, max −11 ms in 60-sample A/B); see `roadmap/data/v5-mod-1-perf-result-2026-05-02.json` for the full capture + analysis)*
- [x] `Sensor` trait defined; all existing sensors are impls; cargo-feature gates discovery *(V5-MOD-2 Phases 1-4, 2026-05-02)*
- [✓] `neurogrim-sensory` no longer depends on `neurogrim-cli` (inverse-coupling fix) — **already-satisfied at V5-MOD-2 open** (recon-corrected: a stale v3-era concern; verified two-pronged grep at Phase 0)
- [x] Example out-of-tree sensor crate compiles + integrates via cargo feature *(V5-MOD-2 Phase 6, commit `459d9ec` — `examples/sensor-readme-quality/`)*
- [x] `Sensor` conformance suite enforces JSON-schema CMDB output *(V5-MOD-2 Phase 5, hand-rolled structural check covers the load-bearing cmdb-envelope-v1 constraints — full `jsonschema` validation deferred for `neurogrim-core` dep-discipline)*
- [x] LSP-Brains spec §F (MCP sensory tools) updated to reflect plugin shape *(Phase 7 close-out, this commit)*
- [ ] `BackendHandle` enum replaced by `Arc<dyn QueueBackend>` factory dispatch
- [ ] `queue-config.yaml` supports user-registered backend types
- [ ] Existing `JsonlBackend` + `SqliteBackend` pass conformance suite unchanged
- [ ] Example crate demonstrates third-party queue backend registration

---

## Stories

### V5-MOD-1: ScoringSource trait + factory registry (~7–10 days)

**Status:** **COMPLETE** 2026-05-02 — Phases 0–6 shipped + Phase 7 close-out
**Effort:** M (final ~9.5 days planned, 8 commits + close-out)
**Depends on:** V5-FOUND-1 (diagnostics for measuring perf impact) — closed 2026-05-02

**What:** Refactored the **3 string-dispatch sites** at `neurogrim-mcp/src/context.rs:218` (`load_cmdb_data`, primary), `neurogrim-mcp/src/server.rs:75` (`load_cmdb_from_disk`, duplicate `cmdb`-only branch), and `neurogrim-mcp/src/doctor.rs:155` (validation skip-check) — all matching on `source_type` ∈ `{"cmdb", "a2a", "function"}` — into `ScoringSource` trait + factory registry with a converged `Dispatcher` enum routing built-ins (no `Box<dyn>`) and third-party plugins (`Box<dyn ScoringSource>`). Built-in factories preserved verbatim; third-party crates register their own at startup via `ScoringSourceRegistry::register`.

*The original epic prose named "registry.rs:135–157 string-dispatch" and source types `{"jsonl", "a2a", "file"}`. Both were incorrect — `registry.rs:135-157` is the **config struct** (now `ScoringSourceConfig`); the dispatch lived in `neurogrim-mcp` per above; source-types are `cmdb / a2a / function`. Anchors corrected at V5-MOD-1 close 2026-05-02.*

**Why:** This was the highest-leverage seam in the codebase. ScoringSource dispatch was a string match in `neurogrim-mcp` — every new source type required forking core. Factory pattern now unblocks third-party scoring sources (Prometheus, Datadog, CloudWatch, custom HTTP plugins) without core changes. The Phase 6 example (`examples/scoring-source-prom/`) is the proof.

**Architectural decision: Option A fallback — `BuiltinScoringSource` enum + inherent `async fn`.** The initial `Box<dyn ScoringSource>` impl shipped Phase 3 (commit `c7afaa1`), but the strict 5%-of-baseline perf gate failed reproducibly across 3 captures (p95 21/24/28 ms vs ceiling 19 ms; root cause: `#[async_trait]` future-boxing across 19 domains × scoring run). Operator-pinned Option A from the V5-MOD-1 plan: `enum BuiltinScoringSource { Cmdb(CmdbSource), A2a(A2aSource), Function(FunctionSource) }` with inlined `match` dispatch, plus an inherent `async fn load_inherent(...)` on each source that bypasses `#[async_trait]`'s `Pin<Box<dyn Future>>` boxing. The `ScoringSource` trait remains for third-party impls (which pay the boxing cost when used). Two-tier dispatch: built-ins are zero-cost; plugins are object-safe. **Post-fallback A/B comparison on the same host** confirmed the fallback is measurably faster than Phase 3 (p95 30 ms vs 33 ms, p99 34 vs 39, max 40 vs 51) — improvement direction is correct; absolute baseline unverifiable due to ~15 ms p95 system drift since the V5-FOUND-1 baseline was captured (8+ hours of compilation, additional dev-workstation background load). Full data + analysis: `roadmap/data/v5-mod-1-perf-result-2026-05-02.json`.

**Naming decision: Option A (rename) + accept semver-major bump.** The pre-existing `pub struct ScoringSource` in `registry.rs` collided with the new trait. Operator-pinned 2026-05-02: rename the struct to `ScoringSourceConfig`, keep the trait as `ScoringSource`, accept the semver-major break for `neurogrim-core` (4.x → 5.0.0 — matches the v5 epic boundary). Workspace `Cargo.toml` bumped + 7 path-pinned internal deps updated atomically in Phase 0.

**Registration mechanism: hand-rolled `HashMap<&'static str, Box<dyn ScoringSourceFactory>>`.** Per plan-critic Subagent 2 finding: the workspace has no existing static-registration substrate (`inventory` / `linkme` / `ctor` — none present), and the `dependency-discipline` skill enforces a 4-point pre-flight on new crates. The hand-rolled registry is ~40 lines with zero supply-chain review burden, and registration is *explicit* (visible in startup code) rather than magical at link time. `inventory`-based v2 reserved for v5.5 BACKLOG B-37/B-40 if "register without an explicit init call" demand emerges.

**Done when:**
- [x] `ScoringSource` trait defined in `neurogrim-core/src/scoring_source.rs` *(Phase 1, commit `41b2310`)*
- [x] Factory registry: hand-rolled `HashMap<&str, Box<dyn ScoringSourceFactory>>` (NOT `inventory`); built-in **cmdb, a2a, function** factories preserved *(Phase 2, commit `b2d0949`; A2A factory lives in `neurogrim-ecosystem` to keep `neurogrim-core`'s dep graph acyclic)*
- [x] Conformance test suite — 8 cross-cutting tests in `neurogrim_core::scoring_source_conformance::run_factory_conformance` covering happy path + negative paths (skeletal config, concurrent safety, idempotency, factory panic, source-name stability); per-source negative-path tests (malformed JSON, missing field, BOM, unreachable endpoint, etc.) live in each source's own module *(Phase 5, commit `3e4d5d2`)*
- [x] Example out-of-tree crate `examples/scoring-source-prom/` registers successfully without forking core; passes the conformance suite *(Phase 6, commit `0955b4d`)*
- [⚠️] **Performance gate:** strict 5%-of-baseline (p95 ≤ 19 ms) ceiling failed Phase 3; pivoted to plan-documented Option A fallback per "Architectural decision" above. Fallback IS measurably faster than Phase 3 on the dev host but absolute baseline unverifiable due to system drift. Full result: `roadmap/data/v5-mod-1-perf-result-2026-05-02.json`. Theme B continuation gated by this file's verdict; operator-pin on Option A satisfies the gate's intent (architectural fix in place).
- [x] LSP-Brains spec sync — `METHODOLOGY-EVOLUTION.md` lines 1118 + 1135 updated to reflect the `ScoringSource` → `ScoringSourceConfig` rename *(Phase 7 close-out, this commit)*

### V5-MOD-2: Sensory plugin interface (cargo-feature gates) (~10–14 days)

**Status:** **COMPLETE** 2026-05-02 — Phases 0-6 + Phase 7 close-out (8 commits)
**Effort:** L (final ~12 days planned, actual ~12 days)
**Depends on:** V5-MOD-1 (factory pattern proven on scoring source first) — closed 2026-05-02

**What:** Refactored the **21-arm string-match dispatch** at `neurogrim-cli/src/main.rs:599-622` (`run_sensory`) into `Sensor` trait + `SensorFactory` + `SensorRegistry` with hand-rolled `HashMap` registration. All 21 built-in sensors are trait impls; per-sensor cargo-feature gates control which sensors compile in. Operators can ship slim binaries (`cargo build --bin neurogrim --no-default-features --features sensor-git-health`); heavy deps for absent sensors are not compiled, AND CLI commands that depend on absent sensors (`sca-review`, `sca-calibrate`) disappear from the surface. Default behavior preserved: all 21 sensors enabled.

**Architectural decisions (all 5 operator-pinned 2026-05-02 — `.claude/plans/v5-mod-2-sensor-trait.md` § Fork decisions):**

- **Fork A — `&str` not `&Path`:** Trait `analyze(&self, project_root: &str) -> anyhow::Result<Value>`. Migration economy + zero analyzer churn + no Windows `to_string_lossy()` regression. SDK-level inconsistency vs `ScoringSource::load(&Path)` is real but small (sensors don't manipulate paths much); v6 promotion path documented in V5-SDK epic hand-off note.
- **Fork B — DROP two-method dance:** No `analyze`/`analyze_inherent` pattern. Sensor IO at seconds-per-call; ~50ns × 21 boxing overhead is rounding error vs. the seconds-per-call IO cost (git, cargo audit, network calls). Saves 21+ duplicate method declarations and the cognitive overhead future contributors would otherwise inherit from V5-MOD-1.
- **Fork C — INCLUDE `secrets-readiness` orphan as 21st:** Was a half-shipped v4.2 S14 advisory domain (analyzer + 9 tests + `pub` re-export landed; dispatch wiring didn't). Trait migration was the natural cleanup moment. `neurogrim cast secrets-readiness` is now reachable for the first time since v4.2.
- **Fork D — ADD cli pass-through:** `neurogrim-cli` Cargo.toml mirrors all 21 features so slim builds work at the binary level. Mcp's `neurogrim-sensory` dep was unused (zero source imports) and got dropped — removes a phantom pass-through branch.
- **Fork E — `sensor-readme-quality` example name:** Pure-FS, zero env vars, zero network, runs against any project. Pairs with V5-MOD-1's `scoring-source-prom` (HTTP-fetch pattern) so V5-SDK has two complementary patterns: HTTP-fetch and FS-read.

**Plan-critic recon corrections worth flagging:**
1. **Sensor count:** earlier draft said "22 sensors"; recon at Phase 0 corrected to "20 dispatched / 21 in source / 21 after Fork C reclamation".
2. **Coupling smell already-fixed:** the V5-MOD-2 epic Done-When item "neurogrim-sensory no longer depends on neurogrim-cli" was a stale v3-era concern. Two-pronged grep verification at Phase 0 (Cargo.toml + source imports) confirmed the smell never existed in the current codebase. Marked already-satisfied with [✓].
3. **Meta-sensor sentinels (Phase 4.5):** Subagent 2 claimed all 3 meta-sensors (`coherence`, `domain-calibration`, `federated-patterns`) needed false-positive-green sentinels. Recon at Phase 4.5 confirmed only `coherence` had the issue — the other two have operator-pinned designs treating absence as legitimate (calibration: `has_ever_fired: false` + score 100; federated: Q10+Q17 lock).

**Registration mechanism:** hand-rolled `HashMap<&'static str, Box<dyn SensorFactory>>` (same substrate V5-MOD-1 used; Subagent 2's pre-flight rejected `inventory` for both stories). The 21 built-in factory blocks live in a centralized `neurogrim-sensory/src/sensor_impls.rs` (plan deviation: planned per-module, shipped centralized — single edit to add a new sensor, all 21 impls visible in one screen).

**Conformance suite:** 10 cross-cutting tests in `neurogrim_core::sensor_conformance::run_factory_conformance` covering factory contract (T1-T3), async safety (T4-T5), CMDB envelope shape (T6 hand-rolled structural check), score range (T7), meta block well-formedness (T8), 30-second timeout (T9), idempotency (T10). Hand-rolled instead of full `jsonschema` validation to avoid adding a transitive dep that every third-party `Sensor` author would pay for; `cargo xtask schema-drift-check` (Phase 0) catches schema drift between vendored copy and canonical LSP-Brains copy.

**Done when:**
- [x] `Sensor` trait defined in `neurogrim-core/src/sensor.rs` *(Phase 1, commit `d1d82d0`)*
- [x] All 21 existing sensors in `neurogrim-sensory/src/` are impls *(Phase 2, commit `b4e69ea`; centralized in `sensor_impls.rs` with `built_in_factories()` aggregator)*
- [x] Dispatch site converted: `neurogrim-cli/src/main.rs::run_sensory` routes through `SensorRegistry` *(Phase 3, commit `22f78af` — also reclaimed `secrets-readiness` orphan)*
- [x] Discovery via cargo features (per-sensor `sensor-X` + 2 convenience aggregates `sensors-builtin-core` / `sensors-supply-chain`) *(Phase 4, commit `a2df2cc`)*
- [x] cli + mcp pass-through (Fork D); 5-cell test matrix green *(Phase 4)*
- [✓] **Coupling fix:** already-satisfied at V5-MOD-2 open (Phase 0 verified)
- [x] `coherence` false-positive-green sentinel *(Phase 4.5, commit `c4ca2bd`)*
- [x] Conformance suite — 10 tests covering happy + 4+ negative paths *(Phase 5, commit `6f45ecc`)*
- [x] Example out-of-tree sensor crate `examples/sensor-readme-quality/` registers + passes conformance *(Phase 6, commit `459d9ec`)*
- [x] Spec impl alignment: LSP-Brains spec §F (MCP sensory tools) updated to reflect plugin shape *(Phase 7 close-out, this commit)*

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

🟢 **Inverse coupling — RESOLVED 2026-05-02.** Recon at V5-MOD-2 Phase 0 found the coupling smell (`neurogrim-sensory → neurogrim-cli`) was already a stale v3-era concern: zero Cargo.toml deps, zero source imports of `neurogrim_cli` in the sensory crate. Two-pronged grep verification cited in Phase 0 commit. The Done-When item is satisfied without code changes; risk closed.

🟡 **Plugin discovery confusion.** Users may expect runtime plugin loading; v5 ships compile-time only. Mitigation: documented prominently in `neurogrim explain sensors` + composition guide (Theme D); BACKLOG B-40 lists the explicit triggers for dynamic loading.

🔵 **Suggestion: ship a `--list-impls` flag per trait surface.** `neurogrim score --list-scoring-sources`, `neurogrim sense --list-sensors`, `neurogrim queue --list-backends` so operators can verify which factories are registered in their build. v5.5 polish, not v5 scope.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- **V5-MOD-1 implementation plan:** `.claude/plans/v5-mod-1-scoring-source-trait.md` (Phases 0–7)
- **V5-MOD-1 perf-gate result:** `roadmap/data/v5-mod-1-perf-result-2026-05-02.json` (Phase 4 capture + 60-sample A/B comparison)
- **V5-FOUND-1 baseline (perf-gate ceiling):** `roadmap/data/v5-scoring-baseline-2026-05-02.json` (p95 ≤ 19 ms ceiling)
- **V5-MOD-1 trait + registry:** `crates/neurogrim-core/src/scoring_source.rs`
- **V5-MOD-1 conformance suite:** `crates/neurogrim-core/src/scoring_source_conformance.rs`
- **V5-MOD-1 built-in sources:** `crates/neurogrim-core/src/scoring_sources/{cmdb,function}.rs` + `crates/neurogrim-ecosystem/src/scoring_source.rs` (A2A — lives outside core to keep dep graph acyclic)
- **V5-MOD-1 dispatcher (built-in fast path + plugin path):** `crates/neurogrim-mcp/src/scoring_source_registry.rs`
- **V5-MOD-1 third-party example:** `examples/scoring-source-prom/`
- **V5-MOD-2 implementation plan:** `.claude/plans/v5-mod-2-sensor-trait.md` (Phases 0–7, including the 5 fork-decision pin records)
- **V5-MOD-2 trait + registry:** `crates/neurogrim-core/src/sensor.rs`
- **V5-MOD-2 conformance suite:** `crates/neurogrim-core/src/sensor_conformance.rs`
- **V5-MOD-2 built-in factories (21 sensors):** `crates/neurogrim-sensory/src/sensor_impls.rs` + `built_in_factories()` aggregator
- **V5-MOD-2 vendored CMDB envelope schema:** `crates/neurogrim-core/data/schemas/cmdb-envelope-v1.schema.json` (drift-checked by `cargo xtask schema-drift-check`)
- **V5-MOD-2 third-party example:** `examples/sensor-readme-quality/` (pure-FS pattern; pairs with `scoring-source-prom`'s HTTP-fetch pattern)
- **V5-MOD-2 cli + mcp feature pass-through:** `crates/neurogrim-cli/Cargo.toml [features]` (21 per-sensor + 2 aggregate); mcp's previously-unused `neurogrim-sensory` dep dropped at Phase 4
- **LSP-Brains spec sync (V5-MOD-2 close-out):** `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` § F (MCP sensory tools) — extensibility note added; Phase 7 commit
- Pre-V5 scoring-source config struct: `crates/neurogrim-core/src/registry.rs` (renamed to `ScoringSourceConfig` in V5-MOD-1 Phase 0)
- Existing sensory crate: [neurogrim-sensory/src/lib.rs](../crates/neurogrim-sensory/src/lib.rs)
- Existing queue backend trait: [queue_backend.rs:69](../crates/neurogrim-core/src/queue_backend.rs)
- LSP-Brains spec sync (V5-MOD-1 close-out): `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` lines 1118 + 1135 — `ScoringSource` → `ScoringSourceConfig` rename note added
- Successor pipeline: BACKLOG B-37..B-40 (v5.5 trimmed items)
