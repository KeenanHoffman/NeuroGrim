# V5-FOUND-4 — TestRunner trait (minimal modular testing surface)

**Epic:** [`roadmap/epics/v5-foundation.md`](../../roadmap/epics/v5-foundation.md) § V5-FOUND-4 · **Theme A** · Effort S (~1 day actual; 2–3d estimate) · Depends on V5-FOUND-2 ✅ (V5-FOUND-3 listed as dep but soft — § Soft-dependency on V5-FOUND-3)
**Successor:** Closes V5-SDK-2 deliverable 2 (TestRunner conformance suite re-export). After V5-FOUND-4 lands, V5-SDK-2 flips ◐ PARTIAL COMPLETE → ✅ COMPLETE.
**Status:** drafted 2026-05-04; **plan-critic v1 REVISED 2026-05-04** (2 🔴 blockers absorbed: AgentDrivenRunner stub fails proposed-#20 reshape rule + Fork D1 silent-green-CI hazard → AgentDrivenRunner DROPPED from v5.0 scope, deferred to v5.5 BACKLOG; 5 🟡 technical concerns absorbed: 2 problematic conformance tests dropped, `runner_name()` removed from trait, object-safety + Send compile-tests added, span ownership + match-arm specs documented, byte-identical-ledger verification added; 2 🟡 methodology concerns absorbed: V5-SDK-2 closure framing + V5-DOC-1 recipe deferral); fork decisions pending operator pin.

## Context

Theme A's fourth epic. V5-FOUND-1 ✓, V5-FOUND-2 ✓, V5-FOUND-3 ⏸ DEFERRED (Windows toolchain gap). V5-FOUND-4 is the smallest remaining piece in Theme A — a single-method `TestRunner` trait that gives users a modular testing surface without inviting god-object scope.

**Posture: smallest modular surface that supports adopters' "users create their own testing surface" goal without inventing aspirational pluggability.** Trait surface is `async fn run(&self, selection: &TestSelection) -> Result<TestRunReport>` — single method. Single concrete impl at v5.0 (`NextestRunner`); the wrapper dispatches via `Box<dyn TestRunner>` internally to validate trait integration. AgentDrivenRunner is **DROPPED from V5-FOUND-4 scope** and tracked as a v5.5 BACKLOG entry alongside the agent-orchestration work that would make it a real impl (no Rust LLM client exists today; building one is M-L effort, decoupled from this epic).

**Why this advances v5:**
- Theme A goes from 2/4 → 3/4 epics complete. V5-FOUND-3 stays deferred; V5-FOUND-4 doesn't structurally need it.
- V5-SDK-2 partial (◐ PARTIAL COMPLETE 2026-05-04) had its TestRunner conformance suite deliverable gated on V5-FOUND-4. After this epic ships, V5-SDK-2 flips to ✅ COMPLETE — the conformance suite re-export pins a real impl (NextestRunner) against a real conformance suite, not a no-op stub.
- Theme D's V5-DOC-1 composition guide can include a "wrap tests in your own runner" recipe — limited to NextestRunner-style usage at v5.0; "drive tests via an agent" pattern explicitly deferred to v5.5 in the recipe text.

## Reshape-rule alignment (v5-roadmap §A)

The v5 master roadmap's reshape rule: a seam becomes a trait only when **(i)** ≥2 plausible alternate impls already exist or are in scope, OR **(ii)** an external user has asked for it, OR **(iii)** leaving it concrete is provably blocking adoption.

V5-FOUND-4 clears via **clause (iii)**: V5-SDK-2's promise of a TestRunner conformance suite re-export is the adoption signal. Without `TestRunner` as a trait, the SDK cannot ship the conformance suite (there's nothing for adopters to implement); V5-SDK-2 deliverable 2 stays PARTIAL forever. Trait extraction is the unblock.

(Clause (i) is **not** cleared at v5.0 because the second-impl AgentDrivenRunner is intentionally deferred to v5.5. The plan-critic methodology lens correctly identified this; the fix was to defer the second impl, not to add an aspirational stub. When v5.5 ships AgentDrivenRunner, clause (i) is cleared retroactively — reshape-rule-honest.)

## Soft-dependency on V5-FOUND-3 (epic file revision needed)

The epic file (`roadmap/epics/v5-foundation.md:139`) currently lists V5-FOUND-4 as `Depends on: V5-FOUND-2, V5-FOUND-3`. V5-FOUND-3 is deferred. The dependency is **soft** because:

- `TestSelection` ships as `non_exhaustive` enum; v5.1 adds `ByCoverage(...)` variant non-breakingly.
- `TestRunReport` is `non_exhaustive` struct; coverage-related fields out of scope here.
- The 4-test conformance suite is orthogonal to coverage selection.

Phase 6 will edit the epic line to `Depends on: V5-FOUND-2 ✅; V5-FOUND-3 ⏸ DEFERRED (soft — see plan § Soft-dependency)`. Avoids the contradiction the methodology lens flagged.

## Architectural anchors (extending, not inventing)

- **V5-MOD-1/2/3 trait+factory pattern.** `ScoringSource` (`crates/neurogrim-core/src/scoring_source.rs:83`) + `ScoringSourceFactory` (line 136) + `ScoringSourceRegistry` (line 178) is the canonical shape. `TestRunner` mirrors it. V5-MOD-2's `Sensor` (where the trait has NO `name()` method — only the factory does) is the closer reference: `runner_name()` is on the factory, **not** the trait (per plan-critic technical agent C4).
- **`build_cargo_args(args, retry_names)`** at `commands::test` (V5-FOUND-3 Phase 0c, commit `39d7295`) — single source of truth for cargo-nextest argv. NextestRunner uses it via `pub use` (Fork B1).
- **`parse_nextest_output()`** (V5-FOUND-2 Phase 1, commit `52356f0`) — parser for nextest 0.9.133 output. NextestRunner uses it via `pub use` (Fork C1).
- **Conformance suite pattern.** V5-MOD-1/2/3 each ship a feature-gated conformance module (`#[cfg(feature = "conformance")]`). V5-SDK-2 partial established the SDK re-export precedent. V5-FOUND-4 mirrors exactly — `test_runner_conformance` joins the existing 4 modules.
- **Diagnostics ledger discipline (V5-FOUND-1).** The wrapper retains ownership of the outer `test.run` tracing span; NextestRunner's body emits a child `cargo.invoke` span (matching the V5-FOUND-1 instrumentation pattern). Span ownership specified explicitly in Phase 2 to address plan-critic technical agent C3.

## Recon-confirmed state

- `commands::test::run` (`crates/neurogrim-cli/src/commands/test.rs`, ~1,610 lines post V5-FOUND-3 Phase 0c) handles: ledger reads, cargo argv construction (via `build_cargo_args`), cargo invocation, output capture, parsing (`parse_nextest_output`), failure-ledger append, summary print. Phases 2 + 3 surgically extract the cargo-invocation+parse subset into `NextestRunner`; the wrapper retains ledger envelope construction (`run_id`/`ts`/`commit`), `--show-only-new` filtering, summary print, and `std::process::exit`.
- `Args` struct (line 77) carries: `keep_last`, `show_only_new`, `retry_failed`, `slow`, `verbose`, `e2e`, `project_root`, `profile`. **No new flag added** — this epic does not add `--runner=` because there's only one runner at v5.0. The trait dispatch is internal.
- `parse_nextest_output()` returns `ParsedCargoOutput { totals, failures }` — NextestRunner translates this to `TestRunReport` directly (1:1 field mapping; no logic change).
- `neurogrim-core/Cargo.toml`'s `conformance` feature already exists (V5-SDK-2 Phase 1, commit `7bafe59`). V5-FOUND-4 extends it: `test_runner_conformance` joins the gated module list.
- `neurogrim-sdk/src/lib.rs` already has 4 conformance re-exports gated behind `conformance` feature (V5-SDK-2 Phase 2, commit `c410eb2`). V5-FOUND-4 adds a 5th: `test_runner_conformance`.

## Phases

### Phase 0 — plan + plan-critic + fork pins (this revision commit)

Plan v1 written; plan-critic v1 returned (technical = REVISE; methodology = REVISE — converged on AgentDrivenRunner stub problem). Plan v2 absorbs findings: AgentDrivenRunner dropped from v5.0 scope; conformance suite reduced to 4 tests; `runner_name()` removed from trait; object-safety + Send compile-tests added; span ownership specified; epic-file dep edit scheduled. Operator pinning awaited.

### Phase 1 — define `TestRunner` trait + types + conformance suite in `neurogrim-core`

1. `neurogrim-core/src/test_runner.rs` (NEW): trait + types.
   ```rust
   #[non_exhaustive]
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum TestSelection {
       All,
       Names(Vec<String>),
       Packages(Vec<String>),
       // ByCoverage(...) — added non-breakingly in V5-FOUND-3 follow-up
   }

   #[non_exhaustive]
   #[derive(Debug, Clone)]
   pub struct TestRunReport {
       pub passed: u32,
       pub failed: u32,
       pub ignored: u32,
       pub filtered_out: u32,
       pub duration_ms: u64,
       pub failures: Vec<TestFailure>,
       pub raw_exit_code: i32,
   }

   #[derive(Debug, Clone)]
   pub struct TestFailure {
       pub test_name: String,
       pub binary: String,
       pub detail: String,    // ANSI-stripped panic / assertion text
   }

   /// Pluggable contract for running a workspace test selection.
   ///
   /// Object-safe (`Box<dyn TestRunner>` dispatched in production).
   /// `Send + Sync` to allow shared dispatch across the wrapper's
   /// span-tracking flow. Implementations correspond 1:1 to the
   /// `runner_name` strings exposed by their factories — no
   /// `name()` method on the trait itself (factories carry the
   /// dispatch identity, mirroring V5-MOD-2's `Sensor` pattern).
   #[async_trait]
   pub trait TestRunner: Send + Sync {
       /// Execute `selection`. Errors map to runner-internal failures
       /// (cargo not found, parse failure, etc.); test failures are
       /// surfaced via `TestRunReport.failures`, not `Err`.
       async fn run(&self, selection: &TestSelection) -> anyhow::Result<TestRunReport>;
   }

   /// Factory producing a [`TestRunner`] for a given runner name.
   pub trait TestRunnerFactory: Send + Sync {
       /// Stable wire-name (`"nextest"`, future: `"agent"`, etc.).
       fn name(&self) -> &'static str;
       /// Build a runner instance. May return distinct instances
       /// across calls; trait does not require interior mutability.
       fn build(&self) -> Box<dyn TestRunner>;
   }

   pub struct TestRunnerRegistry {
       factories: HashMap<&'static str, Box<dyn TestRunnerFactory>>,
   }
   // ... new() / register() / get() methods (mirror V5-MOD-1 ScoringSourceRegistry)

   // ── Compile-time safety ────────────────────────────────────────
   // Plan-critic technical agent S1 + C2: explicit object-safety +
   // Send guard. These are zero-runtime-cost compile gates.

   #[allow(dead_code)]
   fn _object_safety_check_test_runner(_: Box<dyn TestRunner>) {}

   #[allow(dead_code)]
   fn _send_check_test_run_report(_: TestRunReport)
   where TestRunReport: Send {}
   ```

2. `neurogrim-core/src/test_runner_conformance.rs` (NEW, feature-gated by `conformance`): **4-test** cross-cutting suite + `run_factory_conformance(&dyn TestRunnerFactory) -> ConformanceReport` entry point. Mirrors V5-MOD-2 sensor_conformance pattern but right-sized for runners (which spawn subprocesses, not pure IO).

   The 4 tests (reduced from 6 — plan-critic technical agent B1 + B2 dropped):

   - `factory_name_non_empty` — `factory.name()` returns non-empty `&'static str`.
   - `factory_name_stable_across_calls` — multiple `factory.name()` calls return the same string.
   - `factory_build_repeatable` — calling `factory.build()` 10 times succeeds; no global-state corruption.
   - `run_with_malformed_selection_returns_ok_or_err_no_panic` — `TestSelection::Names(vec!["::::nonexistent::::".into()])` returns `Ok(report_with_zero_matches)` OR `Err(...)` within 60s — must not panic. (NextestRunner: cargo emits `0 tests run, 0 passed, 0 failed`; report has zero failures + zero passed; exit code is `0` because nextest treats "no matches" as success. Acceptable.)

   **Dropped from the original 6 (plan-critic v1 B1+B2):**
   - ~~`run_with_empty_selection_completes`~~ — `Names(vec![])` against NextestRunner spawns cargo with `-- --exact` and zero names; behavior is undefined and slow. The conformance suite shouldn't budget cargo-spawn time per test.
   - ~~`run_is_concurrent_safe`~~ — 5 parallel `run()` calls against NextestRunner serialize on `.cargo-lock`; not analogous to the V5-MOD-2 sensor pattern (sensors are pure IO, runners spawn subprocesses). Object-safety + Send is verified at compile-time via the `_object_safety_check_test_runner` + `_send_check_test_run_report` functions in `test_runner.rs` instead.

3. `neurogrim-core/src/lib.rs`: add `pub mod test_runner;` (always-on) and `#[cfg(feature = "conformance")] pub mod test_runner_conformance;` (gated).

### Phase 2 — `NextestRunner` impl (surgical extraction)

1. `neurogrim-core/src/test_runner_impls/mod.rs` + `nextest.rs` (NEW): `NextestRunner` + `NextestRunnerFactory`.
   ```rust
   pub struct NextestRunner {
       project_root: PathBuf,
       profile: String,
       slow: bool,
   }

   #[async_trait]
   impl TestRunner for NextestRunner {
       async fn run(&self, selection: &TestSelection) -> anyhow::Result<TestRunReport> {
           // 1. Translate TestSelection → cargo argv.
           //    Plan-critic technical agent C1 — explicit match arms
           //    against #[non_exhaustive] enum; `_ => Err(…)` arm
           //    keeps future v5.1 ByCoverage variant compile-safe:
           let retry_names = match selection {
               TestSelection::All => None,
               TestSelection::Names(names) => Some(names.as_slice()),
               TestSelection::Packages(_) => {
                   // V5-FOUND-4 v1: NextestRunner does not yet wire
                   // package-scoped selection; future patch maps to
                   // `cargo nextest run -p X -p Y`.
                   anyhow::bail!("TestSelection::Packages not yet supported by NextestRunner — tracked as v5.1 work");
               }
               _ => anyhow::bail!("TestSelection::<unknown> — recompile NextestRunner against current TestSelection variants"),
           };

           // 2. Build args via existing build_cargo_args (Fork B1).
           //    `synthetic Args` carries profile + slow needed by build_cargo_args.
           let args = synthetic_args(&self.profile, self.slow);
           let cargo_args = build_cargo_args(&args, retry_names);

           // 3. Spawn cargo + capture stdout/stderr.
           //    Child span `cargo.invoke` emits under the wrapper's
           //    outer `test.run` span — V5-FOUND-1 instrumentation.
           //    Plan-critic technical agent C3: span ownership is
           //    explicit — wrapper owns test.run; runner owns
           //    cargo.invoke (child).
           let invoke_span = tracing::info_span!("cargo.invoke", runner = "nextest");
           let _enter = invoke_span.enter();
           let output = Command::new("cargo").args(&cargo_args).output().context("failed to spawn cargo")?;
           drop(_enter);

           // 4. Parse output (Fork C1 — leave parse_nextest_output
           //    in commands::test; pub use here).
           let parsed = parse_nextest_output(&output.stdout, &output.stderr);

           // 5. Translate ParsedCargoOutput → TestRunReport.
           //    1:1 field mapping; no logic change vs. pre-V5-FOUND-4
           //    behavior. `raw_exit_code` carries the cargo exit code
           //    so wrapper can dispatch its existing exit-code logic.
           Ok(TestRunReport {
               passed: parsed.totals.passed,
               failed: parsed.totals.failed,
               ignored: parsed.totals.ignored,
               filtered_out: parsed.totals.filtered_out,
               duration_ms: invoke_span.elapsed().as_millis() as u64,
               failures: parsed.failures.into_iter().map(|f| TestFailure {
                   test_name: f.test_name,
                   binary: f.binary,
                   detail: f.detail,
               }).collect(),
               raw_exit_code: output.status.code().unwrap_or(-1),
           })
       }
   }
   ```

2. `neurogrim-core/Cargo.toml`: NextestRunner needs `tokio` for `#[async_trait]` + `tokio::process` if used (but `std::process::Command` works fine here; no new tokio dep needed). The runner crate already pulls `async-trait`.
3. The `synthetic_args` helper bridges the trait's lean `(profile, slow)` carriage to `build_cargo_args`'s `&Args` signature; trivial.

### Phase 3 — wire wrapper to dispatch via `Box<dyn TestRunner>`

1. Modify `commands::test::run` to construct + dispatch through the trait:
   ```rust
   // After resolving to_retry from the ledger:
   let selection = if args.retry_failed {
       TestSelection::Names(to_retry)
   } else {
       TestSelection::All
   };

   // Trait dispatch — no clap flag needed since v5.0 has only one runner.
   // Future v5.5 adds `--runner=` once a second runner exists.
   let runner: Box<dyn TestRunner> = Box::new(NextestRunner::new(
       args.project_root.clone(),
       args.profile.clone(),
       args.slow,
   ));

   // Wrapper retains the outer test.run span (V5-FOUND-1 instrumentation).
   let test_run_span = tracing::info_span!("test.run");
   let _enter = test_run_span.enter();
   let report = runner.run(&selection).await?;
   drop(_enter);

   // Existing post-run logic uses report directly — no changes to
   // ledger envelope construction, --show-only-new filtering,
   // summary print, or exit code dispatch.
   let failures = report.failures.iter().map(|f| FailureEntry {
       schema_version: SCHEMA_VERSION.into(),
       run_id: run_id.clone(),
       ts,
       test_name: f.test_name.clone(),
       binary: f.binary.clone(),
       outcome: "failed".into(),
       output: f.detail.clone(),
       commit: commit.clone(),
   }).collect();
   // ... existing append_failures + rotate_ledger_if_needed + print_summary + exit code
   ```

2. **Byte-identical-ledger verification (plan-critic methodology suggestion).** Phase 3 verification command:
   ```bash
   neurogrim test --workspace > pre.out 2>&1; cp .claude/brain/test-failures.jsonl pre.jsonl
   # apply Phase 3 changes
   neurogrim test --workspace > post.out 2>&1; diff pre.jsonl post.jsonl
   ```
   Expected: `diff` returns empty (the surgical refactor is invisible to operator-facing artifacts). Not run literally — used as the conceptual gate; in practice we verify by checking the test count + the failure ledger schema is unchanged after each phase.

### Phase 4 — SDK re-exports + V5-SDK-2 deliverable 2 closure

1. `neurogrim-sdk/src/lib.rs`: add 5 new `pub use` lines (always-on per Fork F1):
   ```rust
   // V5-FOUND-4 — TestRunner trait surface.
   pub use neurogrim_core::test_runner::TestRunner;
   pub use neurogrim_core::test_runner::TestRunnerFactory;
   pub use neurogrim_core::test_runner::TestRunnerRegistry;
   pub use neurogrim_core::test_runner::TestSelection;
   pub use neurogrim_core::test_runner::TestRunReport;
   pub use neurogrim_core::test_runner::TestFailure;
   ```
   And one new gated re-export (mirrors the existing 4 conformance modules):
   ```rust
   /// V5-FOUND-4 conformance suite for [`TestRunner`] impls.
   #[cfg(feature = "conformance")]
   pub mod test_runner_conformance {
       pub use neurogrim_core::test_runner_conformance::*;
   }
   ```

2. `neurogrim-sdk/tests/sdk_surface_assertion.rs`: add pin functions for `TestRunner::run` + `TestRunnerFactory::name` + `TestRunnerFactory::build` (mirrors the V5-MOD-1 pattern).

3. `neurogrim-sdk/tests/compile_test_re_exports.rs`: extend `theme_b_traits_are_object_safe_via_sdk` (or add a sibling fn) to verify `TestRunner` is object-safe via the SDK path.

4. **Update lib.rs rustdoc:**
   - Line 23–29 trait table: add a `TestRunner` row with source `neurogrim-core`, theme `V5-FOUND-4`.
   - Line 316 ("`TestRunner` (V5-FOUND-4): unshipped at V5-SDK-1 release") — flip from "unshipped" to "shipped at V5-FOUND-4 / V5-SDK-2 close-out".

5. **Update `roadmap/epics/v5-sdk.md` § V5-SDK-2:**
   - Status: `◐ PARTIAL COMPLETE 2026-05-04` → `✅ COMPLETE 2026-05-XX`.
   - Flip the TestRunner deliverable's checkbox `[ ]` → `[x]`.
   - Add retrospective bullet noting the V5-FOUND-4 closure.

6. **Update `roadmap/v5-roadmap.md` Theme C row:**
   - "V5-SDK-2 ◐ PARTIAL COMPLETE..." → "V5-SDK-2 ✅ COMPLETE 2026-05-XX (feature-gate + walkthrough + TestRunner conformance suite shipped)".

### Phase 5 — Theme A V5-FOUND-4 close-out + v5.5 BACKLOG

1. **Update `roadmap/epics/v5-foundation.md` § V5-FOUND-4:**
   - Status: `Planned` → `✅ COMPLETE 2026-05-XX`.
   - Edit Depends-on line: `V5-FOUND-2, V5-FOUND-3` → `V5-FOUND-2 ✅; V5-FOUND-3 ⏸ DEFERRED (soft — see plan § Soft-dependency)`.
   - Flip Done-When checkboxes: trait + 1 impl land (deviation from "2 impls" documented as scope-honest reshape rule alignment); conformance suite shipped (4 tests, not 6 — runtime-spawning runners can't honestly fit the cancellation+timeout shape today); `--runner=` flag deliberately not added (only one runner at v5.0).
   - Add retrospective bullet: AgentDrivenRunner deferred to v5.5 BACKLOG B-XX; methodology lens caught the aspirational-pluggability hazard pre-implementation.

2. **Update `roadmap/v5-roadmap.md` Theme A row:**
   - "V5-FOUND-3..4 planned" → "V5-FOUND-3 ⏸ DEFERRED to v5.1/v6; V5-FOUND-4 ✅ COMPLETE 2026-05-XX". Theme A 3/4 complete (V5-FOUND-3 deferred remains the missing 4th).

3. **Add v5.5 BACKLOG entry — B-51:** `V5.5-FOUND-AGENT-RUNNER` — Make AgentDrivenRunner real. Trigger: agent-orchestration test runner pattern proves useful + a Rust-side LLM client lands (cf. V5-FOUND-1.1 deferred). Specification: `--runner=agent` clap flag + factory registration + TestRunner impl that delegates to an LLM-orchestrated runner (e.g., the agent decides which tests to run based on diff context). Tracked alongside V5-FOUND-1.1 (Diagnostic Synthesis) as part of the same Rust-LLM-client epic in v5.5.

4. **Add v5.5 BACKLOG entry — B-52:** `V5.5-FOUND-RUNNER-FLAG` — Add `--runner=<name>` CLI flag dispatch via the registry. Trigger: ≥1 second runner registered (B-51 ships, OR an external adopter contributes a runner crate). Specification: extend `Args` with a clap String/value-enum, look up via `TestRunnerRegistry::get(name)`, error with `EX_USAGE = 64` on unknown name.

## Forks (operator-pinnable)

(Fork D dropped — AgentDrivenRunner removed from v5.0 scope; the stub posture question is moot. Fork E dropped — `--runner=` clap flag is a v5.5 deliverable per B-52.)

- **Fork A — TestSelection variant set**:
  - **A1** = `All` / `Names(Vec<String>)` / `Packages(Vec<String>)` *(default)*. Minimal; `Packages` covers `cargo nextest -p X` future-compat. NextestRunner errors on `Packages` at v5.0 (documented above); v5.1 wires it.
  - A2 = A1 + `Filterset(String)`. Binds trait shape to nextest-specific syntax — bad for future runners.
  - A3 = `All` + `Filterset(String)` only. Limits non-nextest impls.

- **Fork B — `build_cargo_args` location**:
  - **B1** = leave in `commands::test`; NextestRunner uses `pub use commands::test::build_cargo_args` *(default)*. Preserves V5-FOUND-3 Phase 0c's 6 unit tests in place; minimal blast radius.
  - B2 = move to `neurogrim-core::test_runner_impls::nextest`. ~50 lines of test-file rearrangement.

- **Fork C — `parse_nextest_output` location**:
  - **C1** = leave in `commands::test`; NextestRunner uses `pub use` *(default)*. Same logic as B1.
  - C2 = move to `neurogrim-core`. Larger refactor; ~30 parser unit tests follow the move.

- **Fork F — SDK trait re-export feature gate**:
  - **F1** = trait + types re-exported **always-on** (like V5-MOD-1/2/3); `test_runner_conformance` module gated *(default)*. Adopters write `use neurogrim_sdk::TestRunner;` without flipping any feature. Verified against V5-SDK-2's existing always-on trait re-export pattern.
  - F2 = entire surface gated. Surprising for a contract trait that doesn't itself pull in tokio.

- **Fork G — Conformance suite shape** (REVISED post plan-critic — dropped 2 problematic tests):
  - **G1'** = 4 tests: `factory_name_non_empty`, `factory_name_stable_across_calls`, `factory_build_repeatable`, `run_with_malformed_selection_returns_ok_or_err_no_panic` *(default)*. Right-sized for runners that spawn subprocesses. Object-safety + Send is compile-time-checked, not runtime-tested.
  - G2 (REJECTED) = 6 tests including empty-selection + concurrent-safety. Plan-critic v1 B1+B2 made these untenable for runtime-spawning runners.

Defaults pinned: **A1 / B1 / C1 / F1 / G1'**. Five forks; user-pinnable.

## Mutual-exclusion + conflict checks

| Combination | Behavior |
|---|---|
| `neurogrim test` (default — workspace run via NextestRunner) | OK; identical behavior to pre-V5-FOUND-4 (selection = All, runner = NextestRunner internally). |
| `neurogrim test --retry-failed` | OK; selection = `Names(to_retry)`; NextestRunner replays via libtest-compat `--exact <name>`. |
| `neurogrim test --slow --retry-failed` | OK; carries V5-FOUND-3 Phase 0c's `--include-ignored` propagation fix unchanged. |
| `neurogrim test --runner=<anything>` | NOT YET A FLAG — clap rejects `--runner=` as unknown. Adding the flag is v5.5 (BACKLOG B-52). |
| Future `TestSelection::ByCoverage(...)` (V5-FOUND-3 unblock) | Adds non-breakingly via `non_exhaustive` enum; downstream `match` arms must include `_` (NextestRunner's `_` arm at Phase 2 covers this). |

## Exit-code spec

The wrapper's exit-code dispatch is unchanged:
- `0` — all tests passed
- `1` — at least one test failed
- (no new codes; the trait surfaces `raw_exit_code: i32` on the report so the wrapper preserves cargo's exit code verbatim)

## Verification (consolidated)

- Phase 1: `cargo check -p neurogrim-core --no-default-features` ✓; `cargo check -p neurogrim-core --features conformance` ✓; `cargo test -p neurogrim-core --features conformance test_runner_conformance` → 4/4 PASS against NextestRunner factory.
- Phase 2: `cargo build -p neurogrim-core --features conformance` ✓; `cargo nextest run -p neurogrim-core --features conformance` includes the 4 new conformance tests.
- Phase 3: `cargo nextest run --workspace --profile ci` → identical test count + failure-ledger entries to pre-V5-FOUND-4 (the surgical refactor is invisible to the operator). Compare via diff of `.claude/brain/test-failures.jsonl` schema entries.
- Phase 4: `cargo test -p neurogrim-sdk --features conformance` → all 7 existing tests + 1 new `test_runner_object_safe_via_sdk` PASS; `cargo doc -p neurogrim-sdk --features conformance` → clean rustdoc (new TestRunner table row + walkthrough cross-ref).
- Phase 5: `roadmap/epics/v5-foundation.md` § V5-FOUND-4 status = `✅ COMPLETE`; `roadmap/v5-roadmap.md` Theme A row = "3/4 complete; V5-FOUND-3 deferred"; `roadmap/BACKLOG.md` has B-51 + B-52.

## Deliverable shape

6 phase commits per established cadence:

1. Phase 0 — plan v2 + fork pins (this commit).
2. Phase 1 — `TestRunner` trait + types + 4-test conformance suite in `neurogrim-core`.
3. Phase 2 — `NextestRunner` impl (extracts cargo invocation + parse from `commands::test::run`).
4. Phase 3 — wrapper dispatches via `Box<dyn TestRunner>`; preserves byte-identical operator-facing artifacts.
5. Phase 4 — SDK re-exports (4 always-on + 1 gated) + V5-SDK-2 PARTIAL → COMPLETE flip.
6. Phase 5 — Theme A V5-FOUND-4 close-out + v5.5 BACKLOG B-51 (AgentDrivenRunner) + B-52 (`--runner=` flag).

(Phase 4 may bundle into Phase 5 if both close-outs are small.)

## Risks / adversary concerns brought forward

🟡 **Surgical-refactor scope underestimation.** Plan-critic technical agent C3 flagged that `commands::test::run` interleaves ledger reads, span ownership, ledger envelope construction, `--show-only-new` filtering, and exit-code dispatch — the cargo-invocation-plus-parse extraction is the only piece that moves. Mitigation: span ownership specified explicitly above (wrapper owns `test.run`; runner owns `cargo.invoke`). Phase 3 verification asserts byte-identical operator-facing artifacts.

🟡 **`TestSelection::Packages` is a NextestRunner ergonomics gap at v5.0** (NextestRunner errors on this variant pending v5.1 wiring). Acceptable because no current consumer uses it; v5.1 adds `cargo nextest -p X -p Y` translation.

🟡 **Object-safety + Send guards are compile-time only.** A future PR could regress these silently if the explicit `_object_safety_check_test_runner` and `_send_check_test_run_report` functions are removed. Mitigation: those functions live in `test_runner.rs` with `#[allow(dead_code)]` and Rust-style "DO NOT REMOVE" comments. The conformance suite at v5.5 may add a runtime check once spawning is no longer in scope (e.g., AgentDrivenRunner being a non-spawning runner).

🟡 **The trait shape might still be wrong if AgentDrivenRunner reveals shape needs at v5.5.** Mitigation: trait surface is one method (`async fn run`); `non_exhaustive` enum variants extend non-breakingly; SDK is at 0.1.0 with `publish = false` (V5-SDK-1 Phase 0 plan-critic 🔴 fix), explicit allowance for trait-shape changes.

🔵 **Suggestion — register with the diagnostics ledger.** When `runner.run()` invokes, emit a tracing field `runner_name = "nextest"` (or the registered factory name). V5-FOUND-1's diagnostics layer captures this; `neurogrim diag report --kind test.run --since HEAD~1` shows runner-attributed wall-time. Phase 3 wires this via the existing `test.run` span.

🔵 **Suggestion — V5-DOC-1 recipe limitation note.** The composition guide's "wrap tests in your own runner" recipe at v5.0 demonstrates trait dispatch + factory registration only — not a "second runner that does something different." Recipe text should explicitly note "second-runner pattern arrives in v5.5 (BACKLOG B-51) once the agent-orchestration work lands." Honest framing avoids over-promising the modular surface at v5.0.
