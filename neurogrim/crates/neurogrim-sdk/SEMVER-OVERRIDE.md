# `neurogrim-sdk` semver gate — override path

The semver gate for this crate is the **compile-test at
[`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)**.
It pins every re-exported trait method's signature by *using* it.
Any change that breaks the SDK's contract surface — re-export
removed/renamed, trait method renamed/retyped upstream — fails to
compile when the workspace's standard `cargo test --workspace --all-targets`
job runs in CI.

This document is the **legitimate-bypass procedure** for that gate.

## Why a compile-test instead of `cargo-semver-checks`?

`cargo-semver-checks` was the V5-SDK-1 Phase 4 plan default. **Smoke
tests on 2026-05-03 confirmed it does not detect breaking changes
through pure re-exports.** Three textbook breakages were
introduced; the tool reported "no semver update required" on all
three:

| Mutation | Expected | `cargo-semver-checks` reported |
|---|---|---|
| Re-export renamed (`pub use foo::Bar` → `as Baz`) | FAIL | PASS ❌ |
| Re-export deleted | FAIL | PASS ❌ |
| Required method (no default) added to a re-exported trait | FAIL | PASS ❌ |

Root cause confirmed by the maintainer (obi1kenobi) on issue #355,
also discussed in #167, #291, #629, and Predrag's blog post
[*Four challenges cargo-semver-checks has yet to tackle*](https://predr.ag/blog/four-challenges-cargo-semver-checks-has-yet-to-tackle/):
*"items defined in foreign crates are no longer inlined into the
crate that re-exports them … `cargo-semver-checks` really can't
find the re-exported item because it really isn't present in the
file presented to the tool."*

This is **blocked upstream** in rustc/rustdoc (rust#94338); no
configuration or flag works around it. `rust-semverver` is
unmaintained (last release for nightly-2020).

**The compile-test approach catches the same three mutations
mechanically** — verified by the same smoke-test methodology. See
[`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)
for the rationale and the full pinning surface.

## Stability tier (re-stated)

`neurogrim-sdk` is `0.x` until promoted to `1.0`. Promotion criteria
(from the V5-SDK epic):

1. ≥6 weeks of soak post-Theme-B-completion (Theme B closed
   2026-05-02 → earliest `1.0` ~2026-06-13).
2. At least one external adopter confirming the surface works for
   their use case.

Until `1.0`, **trait-shape changes are explicitly allowed in minor
bumps** (`0.1.0` → `0.2.0`). After `1.0`, breaking changes require
a major bump (`1.0.0` → `2.0.0`).

The compile-test gate enforces detection mechanically — but the
operator controls the version-bump decision. The gate **does not
decide when to break**; it ensures that breaks are visible at PR
time.

## Decision tree

```
Did this PR change a re-exported trait shape, type signature, or
removed/renamed any public item in neurogrim-sdk?
│
├── No  → Gate passes. Nothing to do.
│
└── Yes → Gate fails. Choose ONE of A, B, C below.
```

### A — Bump the version (the standard path)

The intentional break is a real surface change. Take three actions
in one PR:

1. **Update [`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)**
   to reflect the new signature. The pinning function must still
   bind the return value to the new expected type.
2. **Bump `Cargo.toml`** `version`:

   | Pre-1.0 (now)              | Bump  | Post-1.0                | Bump  |
   |----------------------------|-------|-------------------------|-------|
   | `0.1.0` → `0.2.0` (any break) | minor | `1.0.0` → `2.0.0` (any break) | major |
   | `0.1.0` → `0.1.1` (additive)  | patch | `1.0.0` → `1.1.0` (additive)  | minor |

3. **Re-run `cargo test -p neurogrim-sdk`** locally; it should now
   pass. Push the PR.

The CHANGELOG (when the SDK starts shipping one — out of scope for
0.1.0) gets a corresponding entry.

### B — Suppress a confirmed false positive

The compile-test approach has effectively zero false-positive
surface (it's not a tool's heuristics, it's literal Rust type
checking). If you encounter what looks like a false positive,
something else is wrong — e.g., an upstream rustc version change
modified type inference or trait-resolution behavior.

In practice: if the wrapper at
[`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)
fails to compile but you believe the re-exported trait's contract
hasn't actually changed, treat it as a **bug in the pin, not a
contract change**. Update the pin to match the (still-stable)
trait shape. No version bump needed if the trait's contract is
genuinely unchanged from a downstream-consumer perspective.

This path requires **two reviewer approvals** to confirm the trait
contract is genuinely stable from a consumer perspective.

### C — Skip the gate (escape hatch)

The gate runs as part of the standard `cargo test --workspace
--all-targets` CI job. Skipping it requires skipping the whole
workspace test job — which is hostile to repository hygiene and
should never be done on `main`.

If you genuinely need to land a change while the gate is broken
for an external reason (e.g., a transient toolchain bug):

1. Open a PR with `[broken-toolchain]` in the title (case-sensitive)
   describing the external blocker. Include a link to the
   upstream issue.
2. Two human reviewers must approve.
3. Add a footer to the squash-merge commit:

   ```
   Skipped-Workspace-Tests: <upstream-issue-link>
   Co-Reviewed-By: @<reviewer-1>
   Co-Reviewed-By: @<reviewer-2>
   ```

4. Open a follow-on issue to re-enable the test once the upstream
   blocker resolves.

This path should be rare. CI bypass is not a routine operation.

## What does NOT count as a break (gate stays green)

These changes don't trip
[`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs):

- Adding a new public type, function, or trait to the SDK
  (the new item simply isn't pinned yet — add a wrapper).
- Adding a new variant to a re-exported `#[non_exhaustive]` enum.
- Adding a new field to a re-exported `#[non_exhaustive]` struct.
- Adding a new method **with a default implementation** to a
  re-exported trait. (Without a default impl: every `Sensor` /
  `ScoringSource` / `QueueBackend` / etc. impl across the
  workspace fails to compile, including conformance suite stubs.
  That fails before our gate — workspace-wide build fail.)
- Implementing an additional trait for a re-exported type.

When you add a new re-export to `lib.rs`, **add matching pins to
[`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)
in the same PR**. The gate's coverage is only as complete as the
pin file.

## What DOES count as a break

These trip the gate (require Path A, version bump):

- Removing a `pub use` from `lib.rs` — pin's path resolution fails.
- Renaming a `pub use ... as ...` — same.
- Renaming a method on a re-exported trait — the wrapper's
  `s.method_name(...)` no longer resolves.
- Changing a re-exported trait method's parameters — type mismatch
  at the wrapper's call site.
- Changing a re-exported trait method's return type — the
  wrapper's `let _: ExpectedT = ...` binding fails.
- Tightening a generic bound on a re-exported trait method.
- Removing a public field on a re-exported struct (catches via
  any wrapper that constructs/uses the struct; less complete than
  trait-method coverage — see "Known gaps" below).

## Known gaps (deliberate scope)

The compile-test gate does NOT catch:

- **Visibility-only changes** to re-exported items (e.g., a field
  on a re-exported struct goes from `pub` to `pub(crate)`). Wrappers
  here pin trait methods, not struct field reads.
- **Changes to re-exported types we don't currently pin** — e.g.,
  if `AgentOutput` (a struct) gains a new required field without
  `#[non_exhaustive]`, the gate doesn't catch it. Adding
  field-pinning wrappers is straightforward when needed.
- **Cross-crate-module `pub use` reorganization** that ends at the
  same nominal type — the rustdoc JSON limitation that defeated
  `cargo-semver-checks` doesn't affect us, but our pins use type
  identity, not path identity.

These are tracked in `roadmap/BACKLOG.md` § B-MOD-SDK-SEMVER-GAP
("Re-export-aware semver gate when rustdoc inlining is fixed
upstream — full automation via cargo-semver-checks").

## Local rehearsal

Before opening a PR that intentionally breaks the SDK surface:

```bash
cd D:/Brains/NeuroGrim/neurogrim
cargo test -p neurogrim-sdk
```

The compiler error message tells you exactly which pin failed and
where the type mismatch is. Fix the pin, bump the version, re-run.

## Cross-references

- Pin file: [`tests/sdk_surface_assertion.rs`](tests/sdk_surface_assertion.rs)
- Epic: `roadmap/epics/v5-sdk.md` § V5-SDK-1 Risks (false-positive
  mitigation), Done-When (semver gate enforced).
- Plan: `.claude/plans/v5-sdk-1-thin-reexport.md` § Phase 4.
- Known-gap backlog: `roadmap/BACKLOG.md` § B-MOD-SDK-SEMVER-GAP
- Upstream tooling tracker: <https://github.com/obi1kenobi/cargo-semver-checks>
  (issues #167, #291, #355, #629)
- rustc blocker: <https://github.com/rust-lang/rust/issues/94338>
