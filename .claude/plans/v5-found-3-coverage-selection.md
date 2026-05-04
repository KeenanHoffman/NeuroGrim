# V5-FOUND-3 — Per-binary coverage selection (LSP-brain blast radius)

**Epic:** [`roadmap/epics/v5-foundation.md`](../../roadmap/epics/v5-foundation.md) § V5-FOUND-3 · **Theme A** · Effort L (5–7 days estimated; revised L→M post-plan-critic, see below) · Depends on V5-FOUND-2 ✅
**Absorbs:** BACKLOG B-28 (will flip to ABSORBED on close)
**Successor:** BACKLOG B-44 (v6 promotion to a Brain domain, AND v6 promotion to per-test granularity)
**Status:** **⏸ DEFERRED 2026-05-03 to v5.1/v6** — Windows host coverage-toolchain gap (see § Deferral note at end of file). Phase 0a (`llvm-tools-preview` install) and Phase 0c (`build_cargo_args` extraction + `--retry-failed --slow` `--include-ignored` propagation fix + 6 unit tests) shipped at commit `39d7295`. Phase 0b smoke (per-binary `.profdata` capture under `cargo llvm-cov nextest --no-report`) blocked: `stable-x86_64-pc-windows-gnu` lacks `profiler_builtins`; `stable-x86_64-pc-windows-msvc` has it but lacks `link.exe` / Windows SDK / VC CRT `.lib` files. Originally drafted 2026-05-03; plan-critic REVISED 2026-05-03 (3 🔴 blockers absorbed: Fork B per-test→binary-level, mutual-exclusion guards, exit-code semantics; 1 🟡→🟢 mtime→mtime+blake3 hybrid; effort revised L→M after binary-level simplification); operator pinned 6 forks (A1/B1'/C1/D2/E1/F1). Plan content preserved as design record for re-entry.

## Context

Theme A's third epic. V5-FOUND-1 gave us instrumentation; V5-FOUND-2 gave us nextest + SLO discipline. V5-FOUND-3 adds a **map** from changed files to the *test binaries* that exercise them, so `neurogrim test --select-by-coverage --since HEAD~1` runs only the binaries in the blast radius instead of the full ~1,670-test suite.

**Posture:** map first, score later. v5 ships the map and the selector. v6 (BACKLOG B-44) may promote it to a Brain domain *if* the map proves predictive over ≥6 months of soak.

**Granularity choice driven by plan-critic:** v5.0 maps changed files → *binaries* (not individual tests). Per-test granularity requires either serializing test execution (`--test-threads=1` + setup-script) or running each test as its own `cargo nextest run --exact <name>` invocation — both impose 5–50× wall-time penalties on the collection pass. nextest natively produces one `.profraw` per binary; binary-level granularity is what the tooling already gives us. For a workspace with ~10 test binaries, selection narrows from "1,670 tests" to "tests in 1–3 binaries" — the right order of magnitude for v5.0. Per-test promotion is v6 work (B-44).

## Architectural anchors (extending, not inventing)

- **`neurogrim test` wrapper** (`crates/neurogrim-cli/src/commands/test.rs`, 1,548 lines post-V5-FOUND-2) is the only entry point operators reach. New flags compose with existing ones (with explicit mutual-exclusion guards added — see below).
- **Ledger pattern** at `.claude/brain/*.jsonl` is consistent (test-failures, diagnostics, calibration, supply-chain-review). The new coverage map at `.claude/brain/test-coverage-map.jsonl` slots in alongside, gitignored same as siblings, with a `schema_version` field per existing convention. Reuses `append_failures` + `rotate_ledger_if_needed` helpers from `test.rs`.
- **nextest profiles** (`neurogrim/.config/nextest.toml` from V5-FOUND-2) get a new `coverage` profile: `slow-timeout` relaxed (instrumentation slows tests ~1.5×), `fail-fast = false` (collect coverage even on test failure — broken tests still tell us which files they touched), `retries = 0` (retries pollute attribution).
- **Diagnostics from V5-FOUND-1** capture wall-time + exit codes via the `test.run` span. Coverage runs reuse the same instrumentation.

## Recon-confirmed state of the world

- `cargo llvm-cov` is **NOT installed** locally (`error: no such command llvm-cov`). Same for `llvm-tools-preview` rustup component. Phase 1 installs both.
- `cargo-llvm-cov nextest --no-report` produces one `.profraw` per binary (the natural output unit). For binary-level granularity, this is exactly what we need — no per-test aggregation work.
- `llvm-cov export --instr-profile=<binary>.profdata --format=text` emits per-source-file coverage hit counts. Files with non-zero hits = files covered by that binary.
- `cargo metadata --format-version 1` enumerates the workspace's packages. Each package's test target produces one binary; we can map binary ID → crate name deterministically.
- nextest filterset DSL supports `binary(=<id>)` and `package(=<name>)` for selection. `cargo nextest run -p <crate1> -p <crate2>` is the simplest dispatch (one `-p` per crate that owns a binary in the selection set).
- Cross-platform note: `llvm-tools-preview` provides shim binaries for Windows MSVC + GNU. Our host (Windows GNU + MSYS2) has historical PATH-resolution friction with `llvm-profdata`/`llvm-cov`. Phase 0 smoke validates this; if shims misresolve, set `LLVM_COV` and `LLVM_PROFDATA` to absolute paths via env.

## Mutual-exclusion matrix (NEW — addresses plan-critic 🔴)

`test.rs` has no flag-validation today; the plan adds one. The matrix below is enforced before any `cargo_args` construction (early in `run()`):

| Flag combination | Behavior |
|---|---|
| `--instrument-coverage` + `--retry-failed` | **REJECT** at parse time. Coverage collection requires the full suite (else map is partial); retry expects to subset. Operator must pick one. |
| `--instrument-coverage` + `--select-by-coverage` | **REJECT**. Two different selection mechanisms; combining them is undefined. |
| `--retry-failed` + `--select-by-coverage` | **REJECT**. Both override the workspace run; precedence is unspecified. |
| `--instrument-coverage` + `--show-only-new` | **REJECT**. New-failure filtering against an instrumented baseline poisons the next run's diff (the prior run was the *coverage* run, not a normal run). |
| `--instrument-coverage` + `--slow` | Allow. `--slow` passes `--include-ignored` to nextest; SLO-violation tests should be in the coverage map for completeness (operators rerunning `--slow` post-coverage need the map to know about them). |
| `--instrument-coverage` + `--verbose` | Allow but warn. Verbose passes nextest's raw output to stderr; operator may want this when diagnosing instrumentation failures. |
| `--instrument-coverage` + `--e2e` | **REJECT**. `--e2e` diverts to Playwright entirely; coverage instrumentation doesn't apply. |
| `--select-by-coverage` without `--since` | **REJECT** at parse time (clap requires `--since` paired with `--select-by-coverage`). |
| `--select-by-coverage` + `--show-only-new` | Allow. Coverage selection narrows the run; new-filter is applied to the narrowed set. |
| `--select-by-coverage` + `--retry-failed` | Already covered above (**REJECT**). |
| Pre-existing `--retry-failed` + `--show-only-new` | **DOCUMENT existing behavior** — see line 54–57 comment in `test.rs`. Not in scope to fix here; flagged in retrospective for v5.5 if operator wants it. |
| Pre-existing `--retry-failed` + `--slow` | **FIX** — `--slow` is silently dropped today when `--retry-failed` is set (lines 211/222 — `--include-ignored` only added in the non-retry branch). Add the `--include-ignored` to the retry branch too (small fix; bundle into Phase 0 since it's adjacent to the new validation logic). |

Implementation: a small `validate_flag_combinations(args: &Args) -> Result<()>` function at the top of `run()`. Returns an `anyhow::Error` with a clear "X is incompatible with Y; pass --help" message. Tested with 8 unit tests (one per REJECT row above).

## Exit-code semantics for `--instrument-coverage` (NEW — addresses plan-critic 🔴)

Today `test.rs:349` mirrors nextest's exit code directly. Coverage collection has different semantics — what we care about is "did the map get written," not "did all tests pass." Spec:

| Outcome | Exit code |
|---|---|
| Map written successfully; all instrumented tests passed | **0** |
| Map written successfully; ≥1 instrumented tests failed | **0** (the map is the artifact; failures land in the failure ledger; operator checks ledger for failures) |
| Map write failed (filesystem error, schema-version mismatch on existing map, etc.) | **1** |
| Coverage tooling failed (cargo-llvm-cov missing, profraw merge errored, etc.) | **2** |
| Mutual-exclusion violation at parse time | **64** (per `sysexits.h`'s `EX_USAGE`) |

Rationale: callers that script `--instrument-coverage` for periodic refresh (e.g., a v5.5 cron job per Fork E2) need a clean signal for "the map updated" independent of test pass/fail. Test failures during coverage runs are *expected* (the whole point of coverage on a failing branch is to know which tests touched the broken file).

Operator-facing behavior:
- Coverage run prints a final line: `✦ Coverage map written: <N> binaries, <M> source files. Test failures: <K> (see .claude/brain/test-failures.jsonl).`
- If `K > 0`, also prints: `Note: tests failed during coverage collection; use --select-by-coverage to re-run them under standard semantics.`

The standard `--instrument-coverage`-less `neurogrim test` keeps its existing exit-code behavior.

## Phases (incremental delivery)

### Phase 0 — Plan + recon + fork pins + flag-validation refactor (Day 1, ~0.75 day) — PREREQUISITE

**Goal:** This document lands; plan-critic absorbed; 5 forks pinned by operator; **dry-run smoke** of `cargo-llvm-cov` install + per-binary profraw capture against `neurogrim-secrets` (post-SLO-tag, 32 tests in one binary) to validate the approach is structurally feasible. **Plus** the small flag-validation refactor described in the matrix above (zero feature change; cleans up the silent-misbehavior pre-existing bug surface that the new flags would otherwise inherit).

**Steps:**
1. Draft plan (DONE).
2. Plan-critic absorb (DONE — this document is the v2).
3. Operator pins forks A–E.
4. Smoke install: `rustup component add llvm-tools-preview` + `cargo install cargo-llvm-cov`. Validate `cargo llvm-cov --version` succeeds.
5. Smoke run: `cargo llvm-cov nextest --no-report -p neurogrim-secrets --profile default` and confirm a `.profraw` (or `.profdata` post-merge) appears under `target/llvm-cov-target/`. Set `LLVM_COV` + `LLVM_PROFDATA` env vars to absolute paths if shim resolution fails (Windows GNU friction).
6. Implement `validate_flag_combinations()` per the matrix; add 8 unit tests; bundle the `--retry-failed --slow` `--include-ignored` propagation fix in the same commit. Land as Phase 0 commit.

**Ship criterion:** plan committed; forks pinned; smoke produced ≥1 `.profdata` for `neurogrim-secrets`; flag-validation function committed with green tests.

### Phase 1 — Per-binary coverage extraction (~0.5 day, was 1 day pre-pivot)

**Goal:** Demonstrate, by-hand, the full pipeline for ONE crate: per-binary `.profdata` → list of covered source files. Establishes the data shape Phase 2 will persist.

**Steps:**
1. Configure a `coverage` nextest profile in `neurogrim/.config/nextest.toml`: `slow-timeout = { period = "180s", terminate-after = 2 }` (instrumentation can push secrets-tagged tests over 60s even when they're already `#[ignore]`'d-out; this profile is for `--slow` coverage collection too), `fail-fast = false`, `retries = 0`. JUnit XML still emitted.
2. Write `crates/neurogrim-cli/src/commands/test_coverage.rs` (NEW module, sibling of `test.rs`):
   - `pub fn run_coverage_collection(args: &Args) -> Result<CoverageMap>`
   - Spawns `cargo llvm-cov nextest --no-report --workspace --all-targets --profile coverage`
   - For each `.profdata` produced, runs `llvm-cov export --instr-profile=<.profdata> --format=text` and parses the JSON to extract `[source_file]` for files with non-zero line hits
   - Builds a `CoverageMap` of `Vec<CoverageEntry>` where `CoverageEntry { binary_id, crate_name, covered_files: Vec<PathBuf>, ... }`
3. Write 4 unit tests against fixture `.profdata` + JSON pairs (committed under `tests/fixtures/coverage/`): empty (no tests ran), one-binary-one-file, one-binary-multiple-files, multiple-binaries-shared-file.

**Ship criterion:** `neurogrim test --instrument-coverage --dry-run` (new flag pair) emits a JSON blob to stdout listing `(binary_id, [file_path, …])` tuples for the workspace. No persistence yet — that's Phase 2.

### Phase 2 — Persistent map writer + mtime+blake3 staleness (~1 day)

**Goal:** The map persists at `.claude/brain/test-coverage-map.jsonl`, survives across runs, and invalidates correctly when source files change *content* (not just mtime).

**Steps:**
1. Define schema (revised post-plan-critic for content-hash hybrid):
   ```json
   {
     "schema_version": "1",
     "binary_id": "neurogrim-secrets::tests",
     "crate_name": "neurogrim-secrets",
     "covered_files": ["crates/neurogrim-secrets/src/value.rs", "..."],
     "git_commit": "abcd1234",
     "captured_at": "2026-05-03T18:00:00Z",
     "source_state": {
       "crates/neurogrim-secrets/src/value.rs": {"mtime": 1730664000, "blake3": "ab12cd34..."},
       "...": "..."
     }
   }
   ```
   One JSONL entry per `binary_id`. Re-emitting overwrites the prior entry for that key (latest wins on read).
2. Add `blake3` workspace dep (production-quality, ~600 LOC, no transitive bloat). Compute hash via `blake3::hash(file_contents)`; ~2ms per typical source file.
3. Writer: `append_coverage_entries()` — reuse `test.rs`'s `append_failures` atomic-append helper directly (same 4KB POSIX/Windows atomicity guarantee). No new I/O primitives.
4. Staleness check (two-tier per `dashboard/src/cache.rs:19–49` philosophy — fast trigger, accurate confirmation):
   - **Fast tier:** if any covered file's current mtime > recorded mtime → potentially stale.
   - **Confirm tier:** for each potentially-stale file, compute current blake3; compare to recorded. If hash matches → mark fresh, update mtime in-place (cheap recovery from `git checkout`-like mtime-only churn). If hash differs → entry is stale.
   - **Eviction:** missing files mark the entry stale (file deleted); the entry is removed from the in-memory map and a fresh-map file emitted.
5. Invalidation: `--instrument-coverage` re-runs binaries with stale entries (or `--instrument-coverage --full` forces full re-run).
6. Add `.claude/brain/test-coverage-map.jsonl` (and `.archive.jsonl`) to gitignore via the existing pattern.
7. 7 unit tests: write→read round trip; mtime-only-change with same-hash → entry refreshed not stale; mtime+content change → stale; missing file → entry evicted; concurrent writers don't corrupt; schema_version mismatch logged + entry skipped; rotate to archive at threshold.

**Ship criterion:** `--instrument-coverage` twice in a row only re-runs binaries whose covered files actually changed *content* between runs. `git checkout HEAD -- <some-file>` does NOT trigger re-collection (validated by an automated test or a manual smoke).

### Phase 3 — `neurogrim test --instrument-coverage` opt-in (~0.5 day, was 1 day pre-pivot)

**Goal:** Bake Phase 1+2 into the wrapper as an opt-in flag. Default `neurogrim test` is unchanged — no instrumentation overhead unless asked.

**Steps:**
1. Add `--instrument-coverage` and `--full` (paired) boolean flags to `Args`. Add `--dry-run` for the Phase 1 mode (skip persistence).
2. `validate_flag_combinations()` from Phase 0 already enforces the mutual-exclusion matrix. No changes here.
3. Branch in `run()`: if `instrument_coverage`, call `test_coverage::run_coverage_collection`, persist result, exit per the exit-code spec above. If not, fall through to V5-FOUND-2's nextest path unchanged.
4. Print operator-facing summary: `✦ Coverage map written: <N> binaries, <M> source files. Test failures: <K> (see .claude/brain/test-failures.jsonl).`
5. CLI doc + a CLAUDE.md entry under "Run Tests" explaining when to run with `--instrument-coverage` (after non-trivial refactors; before trusting `--select-by-coverage`).

**Ship criterion:** `neurogrim test --instrument-coverage` runs cleanly on the workspace, persists the map, exits with the appropriate code per the exit-code spec.

### Phase 4 — `--select-by-coverage --since <git-rev>` selector (~1 day)

**Goal:** The whole-point feature. `neurogrim test --select-by-coverage --since HEAD~1` runs only binaries covering files changed since `HEAD~1`.

**Steps:**
1. Add `--select-by-coverage` boolean + `--since <git-rev>` string flags (clap requires `--since` paired with `--select-by-coverage` per the matrix).
2. Branch in `run()` (`select-by-coverage` path):
   - Read map; evict entries via the staleness logic from Phase 2.
   - `git diff --name-only <since>...HEAD` → list of changed files. Tolerate stderr from git missing/non-repo cases — print friendly error if `<since>` doesn't resolve.
   - For each changed file, look up `binaries where covered_files contains <file>`. Union all matches → set of `crate_name`s.
   - **Empty-selection branches:**
     - No changed files (HEAD == `<since>`): `eprintln!("✦ --select-by-coverage: no changes since <since> — nothing to run."); return Ok(());`
     - All changed files in the diff have no map entry covering them: per Fork D2, **fall back to running the full suite** with a warning: `eprintln!("✦ --select-by-coverage: <K> changed files but none in coverage map — map may be stale. Falling back to full suite. Re-run with --instrument-coverage to refresh.");` Then proceed as standard nextest run.
     - Map empty (no .jsonl exists): `eprintln!("✦ --select-by-coverage: no coverage map at <path>. Run --instrument-coverage first."); return Err(...)` (exit 1).
   - Else: dispatch via `cargo nextest run -p <crate1> -p <crate2> ... --profile <profile>` (one `-p` per crate in the selection set). The existing `cargo_args` Vec construction extends naturally — add the `-p`s before the `--`.
3. Edge cases:
   - File in diff with no map entry: triggers the Fork D2 fallback (full suite) per above.
   - File in diff that's outside any tracked crate (e.g., docs/): silently ignored (no test covers docs).
   - Integration tests (e.g., `cli_smoke.rs`) cover MANY files and would always be selected. **Trade-off accepted at v5.0** — see Fork F below.
4. Regression test: against `neurogrim-secrets` post-coverage, edit `value.rs`; assert `--select-by-coverage` selects ONLY `neurogrim-secrets` package's tests. Smoke against the live workspace.

**Ship criterion:** the V5-FOUND-3 Done-When item — `neurogrim test --select-by-coverage --since HEAD~1` selects a strict subset for a single-file change AND that subset includes ≥1 binary verified to cover the change — passes the smoke. Empty-selection branches all exit cleanly with operator-facing messages.

### Phase 5 — Docs, ledger close, epic close-out (~0.5 day)

**Goal:** V5-FOUND-3 is shippable; B-28 marked ABSORBED; CLAUDE.md + AGENT-PRIMER updated; v5 roadmap reflects Theme A 3/4.

**Steps:**
1. Document `--instrument-coverage` + `--select-by-coverage` in CLAUDE.md "Run Tests" section + `docs/test-slo.md` (cross-reference).
2. Add a new `coverage` topic to `neurogrim explain` (alongside the 15 existing topics in `neurogrim-mcp/src/explain.rs`).
3. Mark BACKLOG B-28 as ABSORBED into V5-FOUND-3.
4. Update epic Done-When boxes; write retrospective; mark V5-FOUND-3 status COMPLETE.
5. Note any deferred work into BACKLOG (anticipated: per-test granularity → B-44 v6; integration-test-skip discipline if Fork F1 default proves too coarse → new v5.5 entry; CI integration → B-49 if not already deferred per Fork E1).
6. Bump v5-roadmap.md to "Theme A 3/4 epics complete".

**Ship criterion:** epic status flipped; BACKLOG flipped; CLAUDE.md updated; close-out commit lands.

## Forks (operator pins required at Phase 0 close)

### Fork A — Map granularity

| Option | What | Cost |
|---|---|---|
| **A1 — Binary-level (plan default, REVISED post-plan-critic)** | Map records `(binary_id, [files])`. Selection runs whole binaries (`cargo nextest run -p <crate>`). | Map ~10 entries; selection coarser than per-test but matches what the tooling natively produces. v5.0 ships at this granularity. |
| A2 — Per-test (deferred to v6/B-44) | Map records `(test_name, [files])`. Selection runs individual tests. | Requires either `--test-threads=1` + per-test profraw env injection (5–50× collection-pass slowdown) OR custom test harness. Not justified at v5.0 effort budget. |

**Plan default: A1.** Per-test promotion is v6 work after the binary-level map proves predictive.

### Fork B — Coverage collection mechanism (REVISED post-plan-critic)

| Option | What | Cost |
|---|---|---|
| **B1' — `cargo llvm-cov nextest --no-report` (plan default, post-pivot)** | Tool natively produces one `.profdata` per binary. We extract per-binary covered files via `llvm-cov export`. | Standard cargo-llvm-cov flow. Reliable. Binary-level granularity baked in. |
| B2' — Per-test via serialized `--exact <name>` invocations | One process per test; `LLVM_PROFILE_FILE` set per invocation. | Works but ~50× collection-pass slowdown. Reserved for v6 per-test promotion (B-44). |
| B3' — Per-test via custom harness with `__llvm_profile_set_file_path()` | Test harness calls LLVM C runtime to swap profile path between tests. | Requires custom `#[test]` macro or harness wrapper; invasive; rejected for v5.0. |

**Plan default: B1'.** B1 (the original "per-test env injection") was structurally broken — nextest runs many tests per binary in one process; `LLVM_PROFILE_FILE` is read at process-start, not per-test. B1' uses the same tool's natural output (one profraw per binary).

### Fork C — Map storage format

| Option | What | Cost |
|---|---|---|
| **C1 — One JSONL line per binary (plan default)** | Mirrors V5-FOUND-1's diagnostics-ledger pattern. Reader dedups on read by binary_id, keeps newest. | Standard pattern; reader handles dedup. ~10 lines for the workspace. |
| C2 — Single JSON file (not JSONL) | Atomic write replaces the whole thing. | Breaks the codebase's append-only pattern; can't reuse the existing ledger writer. |

**Plan default: C1.** Consistency with the ledger family is a strong default.

### Fork D — Selection semantics for files-not-in-map

| Option | What | Cost |
|---|---|---|
| D1 — Strict: only binaries in map for changed files | Smallest selection. | Misses tests for new files (untested) — they would NEVER run via `--select-by-coverage` until the map is regenerated. Risk: silently-untested code. |
| **D2 — Conservative-additive (plan default)** | If ANY changed file has no map entry → fall back to running the **full suite**. Print a warning. | Safe default; operator sees the fallback warning and re-runs `--instrument-coverage`. |
| D3 — Permissive: only binaries in map; skip files not in map | Smaller selection than D2; faster. | Same risk as D1 but more silent — no warning. Rejected. |

**Plan default: D2.**

### Fork E — CI integration posture

| Option | What | Cost |
|---|---|---|
| **E1 — Opt-in only at v5.0 (plan default)** | CI runs `--profile ci` as today; coverage map regeneration is a local-only flow until the map proves valuable. | Smallest CI footprint. |
| E2 — Scheduled nightly map regeneration | A cron-style CI job runs `--instrument-coverage` and commits the map back. | Map gitignored per existing pattern; commit-back conflicts with that pattern; defer to v5.5. |
| E3 — Coverage-aware CI on PRs | PRs run `--select-by-coverage --since main` to fast-feedback only blast-radius. | Bootstraps a stale-map question on the CI runner; defer to v5.5. |

**Plan default: E1.**

### Fork F — Integration-test handling under selection (NEW post-plan-critic)

Plan-critic flagged that integration tests like `cli_smoke.rs` cover many files and would be selected on virtually every change — defeating selection's value.

| Option | What | Cost |
|---|---|---|
| **F1 — Accept at v5.0 (plan default)** | Integration tests select frequently; that's correct for blast-radius (they really do exercise many files). | Selection is "less narrow than per-test ideal" but still narrower than full suite. Deferred refinement. |
| F2 — Tag integration tests as "always-run" or "never-select" | Map metadata flag; selection always includes them, OR selection always excludes them (run only on `--full`). | Requires test-binary classification; introduces a discipline burden. Defer to v5.5 backlog. |
| F3 — Separate `coverage-integration` profile that always runs full | Two-tier selection: unit tests via map; integration tests always run. | Slight additional CI complexity; not justified for v5.0. |

**Plan default: F1.** v5.0 accepts that integration tests select often; v5.5 may add F2 if the operator finds it noisy.

## Risks (revised post-plan-critic)

🟡 **Map staleness on git checkout/rebase/stash.** RESOLVED by Phase 2 mtime+blake3 hybrid. Mtime is the cheap fast-trigger; hash confirms content actually changed. Same-content `git checkout` does NOT invalidate.

🟡 **Cross-crate edges.** A test in crate A may exercise code in crate B via dependency. Binary-level map captures this correctly via `llvm-cov export`'s full-source-tree output. Phase 1 fixture explicitly tests this.

🟡 **Subprocess-spawning tests.** Tests that `Command::new("neurogrim").spawn()` would record coverage of the child process under a separate profraw. RESOLVED by binary-level granularity: `cargo llvm-cov nextest` aggregates per-binary regardless of subprocess fan-out. (Per-test granularity would have had this problem; binary-level avoids it.)

🟡 **Instrumentation slows tests ~1.5×.** Existing 95s warm baseline → ~140s under instrumentation. Operator-acceptable for an opt-in.

🟡 **Windows + LLVM tools fragility.** Mitigated by Phase 0 smoke test + explicit `LLVM_COV` / `LLVM_PROFDATA` env-var fallback if PATH resolution fails.

🟡 **Map size for ~1,670 tests across 10 binaries.** With binary-level granularity, expect ~10 JSONL entries with ~50–500 file paths each — total <1MB. No compression needed.

🟡 **`commands::init_scaffold::tests::scaffold_full_writes_expected_files` is failing.** Pre-existing failure noted in V5-FOUND-2 close-out. Coverage profile sets `fail-fast = false`; the binary's coverage data is still recorded even when one of its tests fails.

🟢 **Plan-critic findings absorbed.** Three 🔴 blockers (B1, exit-code, mutual-exclusion) all addressed in v2. Two 🟡 (mtime, integration tests) upgraded to explicit forks (B+F) with operator-pinnable defaults. Effort revised L → M (~3 days vs original 5–7 days).

🟢 **`flaky-result` from V5-FOUND-2's CI profile** does NOT apply to coverage runs (the `coverage` profile sets `retries = 0` per Phase 1 step 1). No retry-rescue masking failures during attribution.

## Iteration boundaries (revised post-plan-critic)

| Iteration | Phases | Done When | Estimate |
|---|---|---|---|
| 1 | Phase 0 + 1 (plan + flag-validation refactor + per-binary extraction) | Plan committed, forks pinned, smoke proved per-binary `.profdata` works against `neurogrim-secrets`, validation fn green | ~1.25 days |
| 2 | Phase 2 + 3 (map writer + opt-in flag) | `neurogrim test --instrument-coverage` produces a persistent, mtime+hash-invalidated map | ~1.5 days |
| 3 | Phase 4 + 5 (selector + close-out) | `neurogrim test --select-by-coverage --since HEAD~1` runs only blast-radius binaries; epic closed | ~1.5 days |

Total: ~3 days (revised from original ~5d post-pivot to binary-level granularity). Within M estimate.

## Verification (end-to-end, after Iter 3)

1. Cold environment: `cargo clean && rm -f .claude/brain/test-coverage-map.jsonl`.
2. `neurogrim test --instrument-coverage` — wrapper exits 0 (per spec: map written = success regardless of test pass/fail); map file appears with ~10 entries.
3. Edit one source file (e.g., `crates/neurogrim-secrets/src/value.rs`).
4. `neurogrim test --select-by-coverage --since HEAD` — exits 0; runs only the `neurogrim-secrets` binary's tests; print summary mentions selection size + total wall-time.
5. `git checkout HEAD -- crates/neurogrim-secrets/src/value.rs` (mtime-only change, no content change). Re-run `--instrument-coverage`. Confirm: NO binaries re-instrumented (mtime+blake3 hybrid validates same-content fast-path).
6. Delete a covered source file (move it). Re-run `--select-by-coverage`. Confirm: the entry is evicted; if all entries evicted, `--select-by-coverage` falls back to full suite with warning.
7. `neurogrim test --instrument-coverage --retry-failed` — REJECTED at parse time (mutual-exclusion guard); exit 64.

## What this plan does NOT do

- It does **not** ship per-test granularity. v6 work (BACKLOG B-44).
- It does **not** integrate coverage into CI (Fork E1 = opt-in only).
- It does **not** add HTML/lcov reports. cargo-llvm-cov has those; we reuse the underlying `.profdata` infrastructure but don't render artifacts.
- It does **not** fix the pre-existing `commands::init_scaffold::tests::scaffold_full_writes_expected_files` failure.
- It does **not** classify integration tests separately (Fork F1 default — accept at v5.0; F2/F3 deferred to v5.5).
- It does **not** fix the pre-existing `--retry-failed --show-only-new` silent-misbehavior bug. Documented; deferred.

## Cross-references

- Theme A epic: [`roadmap/epics/v5-foundation.md`](../../roadmap/epics/v5-foundation.md) § V5-FOUND-3
- Predecessor: V5-FOUND-1 [`v5-found-1-diagnostic-monitor.md`](v5-found-1-diagnostic-monitor.md), V5-FOUND-2 [`v5-found-2-nextest-sccache.md`](v5-found-2-nextest-sccache.md)
- Successor: V5-FOUND-4 (TestRunner trait — depends on this epic landing)
- BACKLOG: B-28 (absorbed on close), B-44 (v6 successor — both per-test granularity AND domain promotion)
- Test wrapper: [`crates/neurogrim-cli/src/commands/test.rs`](../../neurogrim/crates/neurogrim-cli/src/commands/test.rs)
- Nextest profiles: [`neurogrim/.config/nextest.toml`](../../neurogrim/.config/nextest.toml)
- Diagnostics ledger pattern: V5-FOUND-1's `diagnostics_ledger.rs` (atomic-append helper to reuse)
- Plan-critic prior art: V5-FOUND-2 plan (`v5-found-2-nextest-sccache.md`) used the same Fork-pin → 🔴-blocker-absorb cadence.

## Plan-critic round (2026-05-03) summary

Three 🔴 blockers absorbed into v2:

1. **Fork B (per-test profile collection) → binary-level (B1').** Original B1 (`LLVM_PROFILE_FILE` per-test env injection) was structurally broken — nextest runs many tests per binary in one process; `LLVM_PROFILE_FILE` is read at process-start, not per-test. v5.0 ships binary-level granularity using `cargo llvm-cov nextest`'s native one-profraw-per-binary output. Per-test granularity deferred to v6 (B-44).
2. **Mutual-exclusion guards added.** `test.rs` had no flag validation; new flags would have inherited silent-misbehavior bugs from the `--retry-failed` + `--show-only-new` pre-existing case. Phase 0 now bundles a `validate_flag_combinations()` refactor + 8 unit tests.
3. **Exit-code semantics specified.** Coverage runs exit 0 when the map is written, regardless of test pass/fail — the map is the artifact, not the suite outcome. Test failures land in the failure ledger as before.

Plus one 🟡 upgraded:

4. **mtime → mtime+blake3 hybrid.** Codebase audit (`dashboard/src/cache.rs:19–49`, `sensory/src/git_health.rs:137–174`) showed NeuroGrim avoids mtime-only invalidation. Hybrid stays cheap (mtime fast-trigger) but content-correct (hash confirms). Adds `blake3` workspace dep.

Plus one 🔵 absorbed:

5. **Integration-test handling explicit (Fork F).** Integration tests cover many files and would be selected on most changes. v5.0 accepts this (F1 default); v5.5 may refine.

Effort revised L → M (~3 days vs original 5–7), driven entirely by the binary-level pivot dropping Phase 1's per-test aggregation work.
