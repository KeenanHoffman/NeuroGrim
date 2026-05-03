# V5-FOUND-2 nextest adoption + build cache — Implementation Plan

**Epic:** `roadmap/epics/v5-foundation.md` § V5-FOUND-2
**Effort estimate (epic):** M, ~3–4 days
**Status:** **COMPLETE 2026-05-03** — 4 phase commits landed (Phase 0 → 60eb3b6; Phase 1 → 52356f0; Phase 4 → 6bc386f; Phase 5 → 2078dfd); Phase 6 close-out commit pending. Phases 2 + 3 absorbed into other phase commits (Phase 2 nextest.toml shipped with Phase 1; Phase 3 sccache deferral was a Phase 0 decision with B-47 already filed). Drafted 2026-05-03; **plan-critic REVISED 2026-05-03** (2 🔴 blockers absorbed: Fork B sccache→B3 deferral, Fork C C1→JUnit XML; 1 🔵→🟢 win: `flaky-result = "fail"` adopted at v5.0 not v5.5; Phase 1 budget tightened); 6 forks operator-pinned 2026-05-03.
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note
**Substrate:** Theme A V5-FOUND-1 closed 2026-05-02 (diagnostics ledger + report); V5-FOUND-2 has no upstream story dependency.

## Context

V5-FOUND-2 lands one core speed-up + three discipline deliverables:

1. **cargo-nextest** replaces cargo's plain libtest harness as the wrapper invoked by `neurogrim test`. Smarter scheduling (CPU/mem budgeting, per-test isolation, retry-on-flake with `flaky-result = "fail"` for honesty), better default reporting, faster wall-time on suites large enough to benefit (workspace runs ~1,470 tests post-V5-MOD-3; nextest's parallel execution dominates here).
2. ~~**sccache** configured as a rustc wrapper in `.cargo/config.toml`.~~ — **DEFERRED to v5.5 (plan-critic finding 2026-05-03):** sccache forces `CARGO_INCREMENTAL` off (errors otherwise), which regresses the small-edit dev-loop case where incremental dominates. The "second-largest dev-loop bottleneck" framing was correct for cold builds, wrong for the actual edit-rerun pattern. CI cold-build caching is already covered by `Swatinem/rust-cache@v2`. Sccache may still belong in CI release-build paths; that's v5.5 backlog (B-47, added at Phase 0).
3. **Per-test wall-time SLO**: `≥5s investigate / ≥10s violate` (Fork D1 vs the epic-stated 1s/5s — see Forks section). Audit existing tests; **tag offenders only at V5-FOUND-2 — non-trivial fixes go to v5.5**.
4. **Baseline JSON** at `roadmap/data/v5-test-baseline-<date>.json` captures current wall-time on a representative laptop. V5-FOUND-3 (coverage selection) and V5-FOUND-4 (TestRunner) will compare against this.

The substantial work is **NOT** the new tool (well-documented, stable) — it's:

1. **Migrating `crates/neurogrim-cli/src/commands/test.rs`** (1,084 lines, 11 parser tests). The wrapper currently parses cargo libtest's text output to extract failures. Nextest emits a different format; we adopt **JUnit XML** output (Fork C revised: stable, documented `<flakyFailure>`/`<rerunFailure>` elements; not the experimental `libtest-json-plus`). The migration must keep the failure ledger, `--show-only-new`, `--retry-failed` (libtest-compat path: `cargo nextest run -- --exact <name>`), and `--slow` behaviors working.
2. **Verifying CI doesn't regress**. `.github/workflows/ci.yml` currently runs `cargo test --workspace --all-targets` directly. Switching to `cargo nextest run --profile ci` requires nextest to be installed in the runner; uses `taiki-e/install-action@v2` for prebuilt-binary fetch (~10s vs ~3-5min for `cargo install`). Doctests run via separate `cargo test --doc` step (nextest doesn't run doctests; this is documented and idiomatic).
3. **Audit pass for ≥5s tests**. Some integration tests (dual_brain_pair, frontend builds invoked from Rust tests, schema_conformance) are long by nature. Tagging vs moving requires per-test judgment. **Scope discipline:** V5-FOUND-2 *identifies and tags*; non-trivial fixes are v5.5 backlog work (avoids scope creep into the M budget).

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| Existing `neurogrim test` wrapper | Migrate the parser layer; keep the failure-ledger + flags surface |
| Existing `.claude/brain/test-failures.jsonl` ledger | Schema unchanged; only the line-extraction logic switches |
| Existing `.cargo/config.toml` | UNCHANGED at v5.0 (sccache deferred to v5.5 per plan-critic 🔴 finding) |
| Existing `dependency-discipline` skill | 4-point pre-flight pass on cargo-nextest before adoption (sccache pre-flight when v5.5 picks it up) |
| Existing CI structure (`.github/workflows/ci.yml`) | Append nextest install step + swap `cargo test` → `cargo nextest run` |

## Recon-confirmed state of the world

- `.cargo/config.toml`: only `xtask` alias today. No rustc-wrapper, no sccache.
- `.config/nextest.toml`: does not exist.
- `Cargo.toml` (workspace): no nextest, no llvm-cov, no sccache deps.
- `crates/neurogrim-cli/src/commands/test.rs`: 1,084 lines; 11 unit tests covering libtest output parsing; parser at lines 351–558.
- `.github/workflows/ci.yml`: runs `cargo test --workspace --all-targets` in the `rust:` job. No nextest hint.
- No prior baseline at `roadmap/data/v5-test-baseline-*.json`.
- Workspace test count post-V5-MOD-3: ~1,470 tests (citation: V5-MOD-3 close commit).

## Phases (incremental delivery)

### Phase 0 — Plan + recon + fork pins (Day 1, ~0.5 day) — PREREQUISITE

**Goal:** Lock fork decisions (post-revision), install nextest locally, capture pre-V5-FOUND-2 baseline, file v5.5 sccache backlog.

**Steps:**
1. **Install cargo-nextest** locally via `cargo install cargo-nextest --locked` (~5 min). No sccache install (deferred).
2. **Capture baseline JSON** of `cargo test --workspace --all-targets` wall-time on this laptop (3 cold + 3 warm runs; record p50, p95, max + host info: CPU, memory, OS). This is the "before" number that V5-FOUND-3 + V5-FOUND-4 will compare against.
3. **Pin fork decisions** — see revised "Forks" section below.
4. **Run `dependency-discipline` 4-point pre-flight** on cargo-nextest. (sccache deferred — pre-flight when v5.5 picks it up.)
5. **File backlog entry B-47** in `roadmap/BACKLOG.md`: "v5.5-FOUND-CACHE — sccache for CI cold/release builds (not local dev)" — document the plan-critic finding (sccache + CARGO_INCREMENTAL conflict) and the conditions under which sccache wins vs hurts.

**Ship criterion:** plan-internal; no production code changes. Baseline JSON exists; nextest installed; forks pinned; B-47 filed.

### Phase 1 — nextest wrapper migration (Day 1–2, ~1.5 days) — **DONE 2026-05-03**

**Goal:** `neurogrim test` invokes `cargo nextest run --workspace --all-targets` (default profile) and continues to write the failure ledger correctly. All existing flags (`--show-only-new`, `--retry-failed`, `--slow`, `--keep-last`, `--verbose`) work.

**Outcome (post-implementation):**
- `crates/neurogrim-cli/src/commands/test.rs` `run()` switched from `cargo test` to `cargo nextest run --workspace --all-targets --profile <name> --color never`. `--retry-failed` uses libtest-compat `-- --exact <names>` per Fork F1.
- New `parse_nextest_output()` parser added alongside preserved `parse_cargo_output()`. 8 new unit tests; 18/18 test.rs unit tests + 244/244 neurogrim-cli unit tests pass.
- Smoke test against the live workspace confirmed: 1 injected `panic!()` extracted from real nextest output (1671 passed / 1 failed / 3 ignored), panic detail captured with `value.rs:320:9` line ref, ledger appended at `<project>/.claude/brain/test-failures.jsonl`, `--retry-failed` correctly re-ran only the injected test.

**Deviations from the original step list (each justified, none material to ship criterion):**
1. **Stdout parser instead of JUnit XML parser** (Step 1). The original plan called for a JUnit XML parser using `quick-xml`. After the smoke test ran, nextest's stdout already exposes everything the wrapper needs (status lines, panic detail blocks, summary totals); adding an XML dep + extracting `target/nextest/<profile>/junit.xml` would be ceremony. JUnit XML is still emitted (Phase 2's `.config/nextest.toml` configures it) and is what Phase 5 will upload as a CI artifact — just not what the wrapper itself parses.
2. **Real-format fixture correction.** First parser pass used a fabricated `--- STDERR: <binary> <test> ---` block-marker format; live nextest 0.9.133 actually uses `  stdout ───` / `  stderr ───` markers terminated by a `────────────` divider, with the `FAIL` line **echoed once after the Summary** (must dedup by `(test_name, binary)`). Both bugs caught by smoke test, both fixed.
3. **Cleanly-bounded smoke test simplified.** Used a single `#[test] fn smoke_inject_failure_v5_found_2()` injection in `crates/neurogrim-secrets/src/value.rs`, then reverted via `Edit`. Did not branch — the injection + revert was atomic enough that a temp branch would have been ceremony.

**Effort note (revised from ~1 day):** 1,084-line wrapper + 11 existing parser tests + new JUnit-XML parser + 6–8 new tests + cleanly-bounded smoke test = realistically 1.5 days.

**Steps:**
1. **Add JUnit XML output parser** alongside the libtest parser. Don't delete the libtest parser — keep it as fallback for `--verbose` mode (raw cargo passthrough). JUnit XML is stable, documented, has `<flakyFailure>` and `<rerunFailure>` elements that map directly to the existing failure-ledger schema.
2. **Switch the default invocation** from `cargo test` to `cargo nextest run --message-format-version=0.1 --message-format=junit-summary` (or equivalent JUnit-emitting flag). Confirmed-stable format vs the experimental `libtest-json-plus`.
3. **Pin `--retry-failed` migration** to the libtest-compat path: `cargo nextest run -- --exact <name>` for each ledger entry. (Filterset DSL `-E 'test(=name)'` exists but quoting `::` in test names adds risk; defer DSL adoption to v5.5 if needed.)
4. **Map nextest exit codes** to wrapper exit codes (nextest uses 100 for test failures vs libtest's 101; standardize on cargo-style 1).
5. **Update unit tests** — keep the 11 libtest parser tests (they cover the legacy/verbose path); add ~6–8 JUnit XML parser tests for the new path. Use a static fixture XML (committed, not generated).
6. **Smoke test (cleanly bounded):**
   - Create a temp git branch (`smoke/v5-found-2-fail-injection`).
   - Add `#[test] fn smoke_inject_failure() { panic!("v5-found-2-smoke"); }` to a test file.
   - Run `neurogrim test`; confirm the wrapper extracts the failure into the ledger.
   - Run `--show-only-new` against a prior clean run; confirm only the new failure surfaces.
   - Run `--retry-failed`; confirm the wrapper re-runs the smoke test by name.
   - **Cleanup:** `git checkout main`; run `neurogrim test --keep-last 1` to rotate the polluting smoke entry off the live ledger; `git branch -D smoke/v5-found-2-fail-injection`.

**Ship criterion:** `neurogrim test` runs cleanly via nextest; failure ledger writes correctly; all original flags behave as documented; smoke-test branch cleanly removed.

### Phase 2 — `.config/nextest.toml` profiles (Day 2–3, ~0.5 day)

**Goal:** Two named profiles, `default` (developer-friendly) and `ci` (strict, retry-on-flake-but-still-fail).

**Steps:**
1. **Create `neurogrim/.config/nextest.toml`**:
   - `[profile.default]`: `test-threads = "num-cpus"`, `slow-timeout = { period = "60s", terminate-after = 2 }`, `fail-fast = false` (developer prefers seeing all failures), `retries = 0`.
   - `[profile.ci]`: `test-threads = 4` (matches GitHub Actions default), `slow-timeout = { period = "90s", terminate-after = 2 }`, `fail-fast = true` (CI prefers fast-fail), `retries = 2`, **`flaky-result = "fail"`** (passes-on-retry are STILL marked as run failures — closes the false-negative concern at v5.0 cost; nextest 0.9.131+).
   - `[profile.ci.junit]`: `path = "junit.xml"` so CI can publish JUnit results as an artifact.
2. **`neurogrim test` defaults to `--profile default`**; `--profile ci` exposed via existing `--profile` arg or new flag.
3. **Document** in CLAUDE.md "Run Tests" section, including the `flaky-result = "fail"` rationale (no silent retries-rescue real failures).

**Ship criterion:** Both profiles run cleanly locally; `cargo nextest run --profile ci` succeeds; deliberate-flake test (sleeps random + assertion) proves `flaky-result = "fail"` causes the run to fail even when the test eventually passes.

### Phase 3 — Build cache: deferral documentation (Day 3, ~0.1 day)

**Status: DEFERRED to v5.5** per plan-critic 2026-05-03.

**Original goal:** sccache configured as rustc wrapper; warm-rebuild speedup.

**Why deferred:** Plan-critic finding — sccache forces `CARGO_INCREMENTAL=0` (errors otherwise per `mozilla/sccache#236`). For NeuroGrim's actual dev-loop pattern (small edit → re-run nearby tests), incremental compilation dominates the win envelope; sccache's cold-build advantage is irrelevant when cargo already has rmeta from 30 seconds ago. CI cold builds ARE a win-case for sccache, but our CI already uses `Swatinem/rust-cache@v2` which restores `target/` between runs — overlapping wins, complexity not justified at v5 scope.

**Plus** Windows-side friction: MSVC sccache has known preprocessing bugs (`mozilla/sccache#1725`); Defender real-time scan on `~/.cache/sccache/` is documented friction.

**Steps:**
1. **No `.cargo/config.toml` change.** Existing `xtask` alias preserved as-is.
2. **Document the finding** in V5-FOUND-2 retrospective (Phase 6).
3. **B-47 backlog entry** filed at Phase 0: tracks v5.5 evaluation of sccache for CI release-build paths only (where there's no overlap with `rust-cache@v2` and no incremental-compilation conflict).

**Ship criterion:** B-47 exists in `roadmap/BACKLOG.md`; no production config changed; plan retrospective documents the deferral rationale.

### Phase 4 — SLO audit + tagging only (Day 3, ~0.5 day) — **DONE 2026-05-03 (commit 6bc386f)**

**Goal:** Document the per-test wall-time SLO; **identify and tag** existing ≥5s tests. **Do NOT fix in V5-FOUND-2** — non-trivial fixes go to v5.5 backlog (B-48).

**Scope discipline:** This phase has historically been a scope-creep magnet. Plan-critic 🟡 concern: discovering that 8 tests are ≥10s and "fixing" them blows the M budget by days. **V5-FOUND-2 commits to tag-only.**

**Steps:**
1. **Run `cargo nextest run --workspace --all-targets --profile default --status-level slow`** — surfaces tests exceeding the SLO.
2. **For each test ≥10s (the violation threshold per Fork D1):** tag `#[ignore]` with a `// SLO-violation: <measured-duration>` comment; keep manually-runnable via existing `--slow` flag (which passes `--include-ignored` to cargo).
3. **For each test 5s–10s:** add a `// SLO-investigate: <measured-duration>` comment but DO NOT tag — tracking only.
4. **Document SLO** in `docs/test-slo.md` (new): the 5s investigate / 10s violate rule, the audit log, and pointers to v5.5 backlog B-48 for the fix queue.
5. **File backlog entry B-48**: "v5.5-FOUND-SLO — Fix tagged SLO violations from V5-FOUND-2 audit". Lists each tagged test + measured time.
6. **Update `neurogrim explain test`** topic if it exists.

**Ship criterion:** All ≥10s tests tagged with `#[ignore]` + comment; `docs/test-slo.md` exists with audit log; B-48 filed; default `cargo nextest run --profile default` runs without exceeding SLO violation threshold.

### Phase 5 — CI integration (Day 3–4, ~0.5 day) — **DONE 2026-05-03 (commit 2078dfd)**

**Goal:** CI runs `cargo nextest run --profile ci`; flake-with-fail honesty (no false negatives); doctests preserved; JUnit artifact published.

**Steps:**
1. **Edit `.github/workflows/ci.yml`** — `rust:` job:
   - Install nextest via the official action: `taiki-e/install-action@v2` with `tool: cargo-nextest`.
   - Replace `cargo test --workspace --all-targets` with `cargo nextest run --workspace --all-targets --profile ci`.
   - Add a separate step: `cargo test --doc --workspace` for doctests (nextest doesn't run them; verified in plan-critic). Per-upstream-guidance, this is NOT a double-rebuild — `cargo nextest run && cargo test --doc` produces no more cargo work than plain `cargo test` did.
   - Add a step to publish the JUnit XML as a GitHub Actions artifact (`actions/upload-artifact@v4` with `path: neurogrim/target/nextest/ci/junit.xml`). Free CI dashboard surfacing.
   - Confirm Swatinem/rust-cache@v2 still caches the right paths (target + ~/.cargo/registry).
2. **Smoke test in PR**: open a branch with a deliberately-flaky test (random sleep + assertion that fails ~50% of the time); confirm `retries = 2` retries are visible in CI logs AND `flaky-result = "fail"` causes the run to fail despite the eventual pass. Revert.
3. **Document the doctest-retries gap**: doctests via `cargo test --doc` don't inherit nextest's retries. Minor: doctests are stable historically. Note in `docs/test-slo.md`.

**Ship criterion:** CI passes via nextest on green PRs; deliberate-flake PR shows `flaky-result = "fail"` triggering a CI red; doctests still run; JUnit artifact published in CI run details.

### Phase 6 — Epic story close-out (Day 4, ~0.25 day) — **DONE 2026-05-03**

- Update `roadmap/epics/v5-foundation.md` § V5-FOUND-2: status → COMPLETE; check off Done-When items; add commit references.
- Update `roadmap/v5-roadmap.md` Theme A status row.
- LSP-Brains spec: tooling is implementation-specific (no spec sync needed).

## Forks (operator pins required at Phase 0 close)

**Post-plan-critic state (2026-05-03):** Forks A, D, E, F retain their original plan defaults; Forks B and C are revised per 🔴 plan-critic findings (see in-line REVISED markers).

### Fork A — Adoption posture: full-replace vs additive

| Option | What | Cost |
|---|---|---|
| **A1 — Full replace** (plan default) | `neurogrim test` defaults to nextest; cargo's libtest parser stays in --verbose mode | Single canonical path; clear migration story |
| A2 — Additive | Add `--runner=nextest` flag; default stays cargo test | Backward-compat for muscle memory; two parsers permanent |

**Plan default: A1.** Full adoption is the V5-FOUND-2 epic's intent ("nextest adopted", not "available"). Rolling back is one-line if needed.

### Fork B — Build cache choice (REVISED 2026-05-03 per plan-critic 🔴)

| Option | Tool | Notes |
|---|---|---|
| ~~B1 — sccache~~ | Mozilla's sccache | **Disqualified at v5.0** — forces `CARGO_INCREMENTAL=0`, regresses dev-loop small-edit case; MSVC/Defender Windows friction |
| B2 — cachepot | parlerusage/cachepot fork | Less active; same incremental conflict |
| **B3 — None at v5.0; `Swatinem/rust-cache@v2` for CI; defer sccache to v5.5** (revised plan default) | — | Honors the actual dev-loop pattern; CI cold builds covered by rust-cache; v5.5 evaluates sccache for CI release-build paths only (B-47) |

**Revised plan default: B3.** Plan-critic finding (sccache + `CARGO_INCREMENTAL=0` conflict per `mozilla/sccache#236`) makes B1's "≥30% local warm rebuild improvement" Ship Criterion unreachable for the actual dev-loop pattern. CI cold-build wins are covered by `Swatinem/rust-cache@v2` (already in CI; restores `target/` between runs). v5.5 backlog B-47 tracks "sccache for CI release-build paths" as a follow-on with no incremental-conflict and no rust-cache overlap.

### Fork C — Nextest output format for failure parsing (REVISED 2026-05-03 per plan-critic 🔴)

| Option | What | Notes |
|---|---|---|
| ~~C1 — `libtest-json-plus`~~ | Nextest's libtest-compatible JSON output | **Disqualified at v5.0** — explicitly experimental (`NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1` required); spec section literally `TODO`; mid-stream behavior changes per recent changelog |
| C2 — Native nextest JSON | Nextest's own structured format | Stable but newer to the parser; bigger migration |
| **C3 — JUnit XML** (revised plan default) | Industry-standard JUnit format | Stable, documented `<flakyFailure>`/`<rerunFailure>` elements; consumed natively by GitHub Actions, Codecov, most CI dashboards (free CI surfacing); maps cleanly to existing failure-ledger schema |

**Revised plan default: C3.** Plan-critic finding (libtest-json-plus is on a moving target). JUnit XML wins on three axes simultaneously: stability, explicit retry/flake semantics, and CI dashboard compatibility. The existing failure-ledger schema maps directly to JUnit's element shape.

### Fork D — SLO threshold and consequence

| Option | Threshold | Consequence |
|---|---|---|
| **D1 — 5s investigate / 10s violate** (plan default) | Investigate at 5s; require fix/`#[ignore]`/move at 10s | Conservative; gives operator a triage window |
| D2 — 1s investigate / 5s violate (epic default) | The exact rule the V5-FOUND-2 epic Done-When says | Stricter; may flag many tests at first audit |
| D3 — Defer threshold pinning to post-audit | Run audit first; pin threshold based on findings | Empirical; risk of perpetual deferral |

**Plan default: D1** — slightly looser than the epic's stated 1s/5s. Reasoning: NeuroGrim's workspace has integration tests that do real I/O (filesystem, sometimes git, sometimes SQLite); 1s is too tight for honest tests of those behaviors. Plan-critic 2026-05-03 did not push back on D1; accepted as plan default (with the reinforcing "tag-only at V5-FOUND-2; fixes to v5.5" scope discipline).

### Fork E — CI nextest installation method

| Option | Method | Cost |
|---|---|---|
| **E1 — `taiki-e/install-action@v2`** (plan default) | Pre-built binary fetch (~10s in CI) | Fast, well-maintained; matches our dtolnay/Swatinem pattern |
| E2 — `cargo install cargo-nextest --locked` | Build from source (~3–5 min in CI) | Avoids extra third-party action; significant CI wall-time hit |
| E3 — cargo-binstall | Two-tool path: install binstall, then binstall nextest | Most flexible; one extra dep to vet |

**Plan default: E1** — matches existing CI pattern (we already use `dtolnay/rust-toolchain@stable` and `Swatinem/rust-cache@v2`, both third-party actions). Adding one more from a maintained author is consistent.

### Fork F — Doctest handling under nextest

| Option | What | Cost |
|---|---|---|
| **F1 — Separate `cargo test --doc` step in CI** (plan default) | Keep doctests covered explicitly | One extra CI step; minor overhead |
| F2 — Skip doctests entirely | Drop `cargo test --doc`; rely on `cargo doc --no-deps -D warnings` for compile-only check | Loses doctest assertion coverage; cheaper but weaker |

**Plan default: F1.** Keeps the existing test surface intact. The V5-SDK-1 lib.rs has 5 ignore-tagged doctests today, but other crates may have asserting doctests we shouldn't drop silently.

## Risks (from epic + new ones surfaced by this plan; updated post-plan-critic)

🟡 **Test parser migration broke a corner case.** The 1,084-line libtest parser handles edge cases (interleaved stderr, multiple binaries, ANSI codes, panic with newlines). JUnit XML output is structurally different but well-documented. Mitigation: keep the libtest parser intact as `--verbose` fallback; add ≥6 new JUnit XML parser tests with static fixture XML covering passes/failures/flakes/retries.

✅ ~~**sccache + Windows path cache.**~~ — RESOLVED: Fork B revised to B3 (sccache deferred to v5.5; no local config change in V5-FOUND-2).

✅ ~~**CI flake-retry hides genuine flakes.**~~ — RESOLVED: nextest 0.9.131+ ships `flaky-result = "fail"` which causes the CI run to fail when a test passes only on retry. Adopted in Phase 2 ci profile. The v5.5 "scan logs for retry-success" deferral is unnecessary.

🟡 **Doctest parity.** Nextest doesn't run doctests; if Phase 5 forgets the separate step, doctests stop running in CI without a clear failure signal. Mitigation: explicit Phase 5 step; smoke test verifies a deliberately-failing doctest IS caught. **Doctests don't inherit nextest's `retries`** — minor: doctests are stable historically. Document in `docs/test-slo.md`.

🟡 **JUnit XML format drift.** JUnit XML is a de-facto standard with multiple competing variants. Nextest emits one specific dialect. Mitigation: pin against nextest's documented schema; static fixture XML in tests so format-drift in a future nextest minor bump fails our parser tests, not silently.

🟡 **Phase 4 SLO audit reveals more violations than v5.5 budget can absorb.** If 20+ tests are ≥10s, the v5.5 fix queue (B-48) becomes its own epic. Mitigation: scope-discipline already pinned (V5-FOUND-2 tags only); B-48 can split if needed.

🟢 **Reproducibility of baseline.** Wall-time on a laptop is sensitive to thermal state, background processes. Mitigation: 3 cold + 3 warm samples; record host info (CPU, memory, OS) in baseline JSON.

🟢 **JUnit-as-CI-artifact bonus.** The JUnit XML emitted by `[profile.ci.junit]` is consumed natively by GitHub Actions test summary, Codecov, etc. Phase 5 publishes it via `actions/upload-artifact@v4` — free dashboard surfacing with zero extra parsing.

🔵 **Suggestion — promote SLO to a publish-gate later.** S12-G-4 publish gates currently run tests; could enforce the SLO at gate time. Out of v5 scope; v5.5 polish.

🔵 **Suggestion — revisit `flaky-result` policy at v5.5.** Today: `"fail"` (zero false-negatives, even occasional CI re-run for genuine race-condition tests). If retry-and-fail proves hostile to CI throughput, the alternative is `"pass"` with a separate post-run flake-counter alert. Empirically resolve at v5.5 if it bites.

## Iteration boundaries (revised post-plan-critic)

| Iter | Phases | Shippable? | Rough duration |
|---|---|---|---|
| 0 | Phase 0 (install + baseline + forks + B-47 backlog) | Yes — plan-only outcomes | ~0.5 day |
| 1 | Phase 1 + 2 (nextest wrapper + profiles) | Yes — `neurogrim test` runs via nextest, two profiles work | ~2 days (was 1.5) |
| 2 | Phase 3 + 4 (sccache deferral docs + SLO tagging) | Yes — SLO tagged; B-48 backlog filed | ~0.6 day (was 1) |
| 3 | Phase 5 + 6 (CI + close-out) | Yes — Theme A V5-FOUND-2 marked complete | ~0.75 day |

Total: ~3.85 days. Within epic M estimate (3–4 days). Phase 3 reduction (sccache deferral) ~ offsets Phase 1 expansion (1 → 1.5 day).

## Verification (end-to-end, after Iter 3)

1. `neurogrim test` runs via nextest cleanly; failure ledger writes correctly.
2. `neurogrim test --profile ci` runs the strict profile; deliberate-flake test causes a CI red despite passing on retry (proves `flaky-result = "fail"`).
3. ~~Local warm-rebuild benchmarks better than pre-sccache (≥30% improvement target).~~ — **REMOVED** per Fork B revision; no sccache in V5-FOUND-2.
4. SLO audit tags each ≥10s test with `#[ignore]` + comment; B-48 backlog lists every triaged test with measured duration; default `cargo nextest run --profile default` runs without violations.
5. CI passes via nextest; flake-retry-with-fail verified by deliberate-failure smoke test (revert).
6. Doctests still run in CI via separate `cargo test --doc --workspace` step.
7. JUnit XML artifact uploaded by CI on every run; visible in GitHub Actions UI.

## What this plan does NOT do

- Does **not** ship coverage selection (V5-FOUND-3 scope).
- Does **not** ship the `TestRunner` trait (V5-FOUND-4 scope).
- Does **not** configure sccache anywhere — deferred to v5.5 backlog B-47 per plan-critic finding (sccache + `CARGO_INCREMENTAL=0` conflict).
- Does **not** fix tests that violate the SLO — V5-FOUND-2 tags only; fixes go to v5.5 backlog B-48.
- Does **not** retire the libtest parser; it stays as `--verbose` fallback.
- Does **not** modify `cargo test` behavior outside the `neurogrim test` wrapper.

## Cross-references

- Epic: `roadmap/epics/v5-foundation.md` § V5-FOUND-2
- Existing wrapper: `crates/neurogrim-cli/src/commands/test.rs`
- Existing CI: `.github/workflows/ci.yml`
- nextest docs: <https://nexte.st/>
- nextest JUnit format: <https://nexte.st/docs/machine-readable/junit/>
- nextest `flaky-result` (0.9.131+): <https://nexte.st/docs/configuration/per-test-overrides/#retries>
- sccache (deferred): <https://github.com/mozilla/sccache>
- sccache + CARGO_INCREMENTAL conflict: <https://github.com/mozilla/sccache/issues/236>
- Predecessor: V5-FOUND-1 (`.claude/plans/v5-found-1-diagnostic-monitor.md`)
- v5.5 successor (will be filed Phase 0):
  - `BACKLOG.md` § B-47 — sccache for CI release-build paths
  - `BACKLOG.md` § B-48 — fix queue for SLO-violation tests tagged in V5-FOUND-2 Phase 4
