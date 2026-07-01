---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# `sensor-readme-quality` — Third-Party `Sensor` Example

V5-MOD-2 (2026-05-02) lifted the `cast` dispatch from a hand-coded
21-arm `match` in `neurogrim-cli` into a public trait + factory
registry in `neurogrim-core`. **This crate is the proof.** It
implements a third-party `Sensor` for README-quality scoring and
passes the same conformance suite the 21 built-in sensors pass.

If you're authoring your own sensor — for Jira issue counts, GitHub
PR latency, custom telemetry, anything that produces a CMDB envelope —
read this crate end-to-end as your template, then copy the patterns.
The architecture deliberately makes "ship a new sensor without
forking NeuroGrim" a copy-and-edit job, not a fork-and-rewrite job.

## Why README quality?

Two reasons:

1. **It's a real signal.** A project's README is its front door. A
   missing, stub, or stale README is a documentation health red flag
   that operators care about.
2. **It's a clean demo.** Pure FS, no env vars, no network, runs on
   any project. Pairs with V5-MOD-1's `scoring-source-prom` (HTTP-fetch
   pattern) so the V5-SDK has two complementary patterns: HTTP-fetch
   and FS-read.

## What this crate ships

- **`ReadmeQualitySensor`** — a `Sensor` impl that reads the project's
  README and scores documentation quality on a 0-100 scale via 6
  heuristic features (see scoring rubric below).
- **`ReadmeQualitySensorFactory`** — produces `Box<dyn Sensor>` for
  the wire-name `"readme-quality"`. Stateless; consuming binaries
  register it once at startup.
- **`tests/conformance.rs`** — the canonical contract check. Runs
  the cross-crate suite from `neurogrim_core::sensor_conformance`
  against `ReadmeQualitySensorFactory`. **Copy this file verbatim
  into your own crate**, rename the factory type, and you have the
  same guarantee.

## Scoring rubric

| Feature                               | Points |
| ------------------------------------- | -----: |
| README file present                   |     30 |
| First non-empty line is `# H1` header |     15 |
| Has at least one `## Section` heading |     15 |
| Body length ≥ 500 characters          |     15 |
| Has at least one ` ``` ` code block   |     15 |
| Mentions install / usage / start      |     10 |
| **Total**                             |    100 |

Each feature produces a finding entry with `status: "found" |
"missing"` and explicit `points`. Operators reading the CMDB envelope
see exactly which checks passed and which didn't.

## Wire contract — operator-facing

| Config field | Notes                                                         |
| ------------ | ------------------------------------------------------------- |
| `endpoint`   | unused (this sensor reads files only)                         |
| `path`       | unused (the sensor finds README at project root automatically) |
| All others   | unused                                                         |

The sensor takes ZERO config. It looks for `README.md`, `Readme.md`,
`readme.md`, or `README` (in that order) at the project root. If none
exist, returns a degraded envelope with `score: 0` + `readme:missing`
finding.

## How a consuming binary registers it

```rust
use neurogrim_core::sensor::SensorRegistry;
use sensor_readme_quality::ReadmeQualitySensorFactory;

fn build_registry() -> SensorRegistry {
    let mut registry = SensorRegistry::new();
    // Built-in sensors first.
    registry.register_all(neurogrim_sensory::built_in_factories());
    // Third-party README-quality factory.
    registry.register(Box::new(ReadmeQualitySensorFactory));
    registry
}
```

Then `neurogrim cast readme-quality` produces a CMDB envelope. To
consume the score in scoring, add a domain entry to
`brain-registry.json`:

```json
{
  "documentation-health": {
    "weight": 0.5,
    "scoring_source": {
      "type": "cmdb",
      "path": ".claude/readme-quality-cmdb.json"
    }
  }
}
```

## Cargo.toml template for true third-party use

This crate's own `Cargo.toml` uses `workspace = true` because it
lives inside the NeuroGrim workspace. A third-party crate **outside**
the NeuroGrim workspace would write:

```toml
[package]
name = "my-sensor"
version = "0.1.0"
edition = "2021"

[dependencies]
neurogrim-core = "5"
async-trait = "0.1"
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
tempfile = "3"
```

The only NeuroGrim dependency is `neurogrim-core`. You do **not**
depend on `neurogrim-sensory`, `neurogrim-cli`, `neurogrim-mcp`, or
any internal crate. The trait + registry contract lives in
`neurogrim-core` so plugin authors get a stable, narrow public surface.

## Failure modes (all preserved as `Ok(degraded envelope)`)

The sensor follows the silent-degrade convention used by 18 of the 21
built-in sensors:

| Trigger                                    | Outcome                                                |
| ------------------------------------------ | ------------------------------------------------------ |
| No README at any candidate path            | `Ok` envelope, score 0, `readme:missing` finding       |
| README present but unreadable (IO error)   | `Ok` envelope, score 0, `readme:read_error` + warn-log |
| README empty (zero non-whitespace bytes)   | `Ok` envelope, score 30 (file-present points only) + `readme:empty` finding |

These are exactly the negative-path conditions the conformance suite
exercises (T4 skeletal-input, T6 envelope shape, T10 idempotency).
Following the same discipline gives you a verifiable third-party impl.

## Authoring your own — checklist

1. **Read** `neurogrim_core::sensor` rustdoc — the trait and registry
   contracts are documented there in full.
2. **Pick a wire-name** for your sensor (e.g., `"jira-issues"`,
   `"github-pr-latency"`). Conventionally lowercase ASCII with
   hyphens; must be unique across all factories registered in any
   consuming binary.
3. **Implement `Sensor`** with `#[async_trait]`. The single method is
   `async fn analyze(&self, project_root: &str) -> anyhow::Result<Value>`.
   Return `Ok(envelope)` for silent-degrade; `Err(...)` for fallible.
   Either is acceptable — pick what matches your sensor's semantics.
4. **Implement `SensorFactory`**. Usually a unit struct with
   `Box::new(YourSensor)` in `build()`. If your sensor holds heavy
   state (an HTTP client, a connection pool), cache it on the factory
   and clone-on-build.
5. **Add the conformance test** at `tests/conformance.rs`, copying
   this crate's verbatim. **This is non-optional** — passing the suite
   is the verifiable contract that makes your impl safe to plug in.
6. **Document the wire contract** in your crate's README — what
   config fields the sensor uses (or doesn't), the failure modes
   table, and the scoring rubric.
7. **Publish to crates.io** with a name like `sensor-foo` for
   discoverability. There is no central plugin registry in v5;
   consumers register your factory explicitly in their own
   `main.rs`.

## What this example does NOT do

- **No real-world README parser library** (e.g., `pulldown-cmark`).
  The heuristics are deliberately simple string matching to keep the
  example small. A production sensor that wants to score actual
  Markdown structure would pull in a parser library — note this
  triggers the `dependency-discipline` skill's pre-flight check in
  any NeuroGrim-aware project.
- **No language detection.** The sensor checks for English keywords
  (`install`, `usage`, `getting started`). A multi-language production
  variant would need a localization layer.
- **No CI integration.** The sensor produces a CMDB envelope; the
  consuming Brain integrates it via `brain-registry.json`.

## Cross-references

- **V5-MOD-2 plan:**
  `.claude/plans/v5-mod-2-sensor-trait.md` § Phase 6
- **`Sensor` trait + registry:**
  `crates/neurogrim-core/src/sensor.rs`
- **Conformance suite:**
  `crates/neurogrim-core/src/sensor_conformance.rs`
- **Built-in sensor references:** all 21 sensors in
  `crates/neurogrim-sensory/src/`, with trait impls aggregated in
  `crates/neurogrim-sensory/src/sensor_impls.rs`.
- **Companion HTTP-fetch example:** `examples/scoring-source-prom/`
  (V5-MOD-1 Phase 6 — Prometheus instant-query third-party
  scoring source).
