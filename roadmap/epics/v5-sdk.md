# Epic: v5 SDK Extraction (Theme C)

**Theme:** C
**Release:** v5 (entry decide-later; sequenced after Theme B)
**Status:** PLANNED (drafted 2026-05-01)
**Priority:** Stabilization — extracts the trait surface from Theme B as a versioned contract
**Goal:** Stand up `neurogrim-sdk` as a thin re-export crate of the stable contract types from Theme B. Versioned independently from `neurogrim-core` with semver discipline — core can break internals, SDK cannot break trait shapes without major-version bump. Conformance suites distributed via the SDK as `#[cfg(feature = "conformance")]` test fixtures.

**Depends on:**
- Theme B complete (V5-MOD-1..3 — trait shapes must be real and stable before extraction)

**Blocks:**
- Theme D (composition guide describes the SDK API)

**Master roadmap:** `roadmap/v5-roadmap.md`
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

---

## Theme C Is Done When

- [ ] `neurogrim-sdk` crate exists as a thin re-export layer
- [ ] Public surface documented: every type has a doc comment + example
- [ ] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk`
- [ ] Semver gate in CI: any change to a re-exported trait shape blocks merge without explicit major bump
- [ ] Conformance fixtures exposed for: `Sensor`, `ScoringSource`, `QueueBackend`, `TestRunner`
- [ ] Documented: how a third-party crate runs the conformance suite against its own impls
- [ ] CI in this repo runs every built-in impl against its conformance suite

---

## Stories

### V5-SDK-1: neurogrim-sdk crate (extraction, not invention) (~7–10 days)

**Status:** Planned
**Effort:** M
**Depends on:** V5-MOD-1, V5-MOD-2, V5-MOD-3, V5-FOUND-4

**What:** Extract a thin SDK crate from `neurogrim-core`. Re-exports stable contract types only: `Sensor`, `ScoringSource`, `QueueBackend`, `Transport`, `TestRunner`, plus core types (`DomainDefinition`, `BrainRegistry`, etc.). Versioned independently; follows semver. `neurogrim-core` can break internals; `neurogrim-sdk` cannot break trait shapes without major-version bump.

**Why:** No `neurogrim-sdk` crate today; `neurogrim-core` is the de-facto SDK. Building a brand-new SDK with novel ergonomics on top of unstable trait shapes would lock in mistakes. Extraction is the right move once Theme B's trait shapes are real. Gives third-party module authors a stable surface to depend on.

**Architectural decision: 0.x first, promote to 1.0 only after external adopter validates.** Pre-1.0 explicit allowance for trait-shape changes if Theme B reveals a flaw post-ship. Promotion to 1.0 requires (a) ≥6 weeks of soak post-Theme-B-completion, (b) at least one external adopter confirming the surface works for their use case.

**Done when:**
- [ ] `neurogrim-sdk` crate exists at `crates/neurogrim-sdk/` as a thin re-export layer
- [ ] Public surface documented: every type has a doc comment + at least one usage example
- [ ] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk`
- [ ] Semver gate in CI (`cargo-semver-checks` or equivalent): any change to a re-exported trait shape blocks merge without explicit major bump
- [ ] Workspace `Cargo.toml` lists `neurogrim-sdk` as workspace member
- [ ] Initial version `0.1.0` published; CHANGELOG documents the contract

### V5-SDK-2: SDK conformance suites (distributed) (~3–5 days)

**Status:** Planned
**Effort:** S
**Depends on:** V5-SDK-1

**What:** Promote per-trait conformance suites from Theme B epics into the SDK crate as `#[cfg(feature = "conformance")]` test fixtures. Any third-party impl can add `neurogrim-sdk` with `--features conformance` and run the same tests the built-ins pass.

**Why:** "Modular middleware ships degraded" — the adversary concern that alternate impls are 80% feature-complete. Conformance suites distributed via SDK make "passes the same tests as built-ins" a checkable claim. Lifts third-party module quality bar to match in-tree.

**Done when:**
- [ ] Conformance fixtures exposed for: `Sensor` (≥6 tests), `ScoringSource` (≥8 tests), `QueueBackend` (≥10 tests), `TestRunner` (≥6 tests)
- [ ] All fixtures include negative-path tests (malformed input, panic recovery, timeout)
- [ ] Documented: how a third-party crate runs the conformance suite against its own impls — `cargo test --features conformance` recipe in SDK docs
- [ ] CI in this repo runs every built-in impl against its conformance suite (gates regression)
- [ ] `neurogrim-sdk` README has a "writing a conformant Sensor" walkthrough

---

## Verification (end-to-end smoke per story)

**V5-SDK-1 neurogrim-sdk crate:**
- Outside the repo (e.g., a fresh `cargo new`), write a 30-line sensor crate that depends only on `neurogrim-sdk`; confirm it compiles and runs against a local NeuroGrim instance
- Force a trait-shape change in CI (rename a method on `Sensor`); confirm `cargo-semver-checks` (or equivalent) blocks the merge
- Verify `neurogrim-sdk` builds standalone (without `neurogrim-core` available as a path dep) — the contract-integrity check

**V5-SDK-2 SDK conformance suites:**
- From a third-party crate, add `neurogrim-sdk` with `--features conformance` and run the test fixtures; confirm they execute against the third-party impls
- Verify CI in this repo runs every built-in impl against its conformance suite (Sensor, ScoringSource, QueueBackend, TestRunner) — gates regression
- Walk the "writing a conformant Sensor" walkthrough end-to-end; produce a working sensor that passes the conformance fixtures

---

## Risks (adversary concerns brought forward)

🟡 **Premature stability.** A trait shape might still be wrong when SDK extracts it. Mitigation: 6-week soak between Theme B last ship and SDK extraction is built into the dependency graph. SDK ships as `0.x` first; promotion to `1.0` requires external-adopter validation.

🟡 **Re-export bloat.** SDK might balloon into "everything in core re-exported" if not disciplined. Mitigation: SDK only re-exports types that appear in trait surface signatures. Internal helpers stay in core.

🟡 **Semver-checks false positives.** `cargo-semver-checks` flags some legitimate changes (e.g., adding a non-required trait method) as breaking. Mitigation: document override path; require dual-review on any semver gate override.

🔵 **Suggestion: SDK + core version-pin docs.** Ship a compatibility matrix (`neurogrim-core 4.5.x ⇄ neurogrim-sdk 0.1.x`) so adopters know which versions work together. v5.5 polish.

🔵 **Suggestion: pre-publish dry-run for `neurogrim-sdk`.** S12 publish-gate pipeline gains a `sdk-publish-dryrun` gate that validates the SDK can be published cleanly. Reuses S12 infrastructure.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Theme B traits (extracted by Theme C): V5-MOD-1, V5-MOD-2, V5-MOD-3, V5-FOUND-4
- Existing publish-gate infra: S12-G-3, S12-G-4 (semver-check gate added here)
- Existing workspace pattern: `crates/neurogrim-core/Cargo.toml`, `crates/neurogrim-a2a/Cargo.toml`
