---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# `scoring-source-prom` — Third-Party `ScoringSource` Example

V5-MOD-1 (2026-05-02) lifted the scoring-source dispatch from a
hand-coded `match` arm in `neurogrim-mcp` into a public trait +
factory registry in `neurogrim-core`. **This crate is the proof.**
It implements a third-party `ScoringSource` for Prometheus and
passes the same conformance suite the built-in `cmdb`, `a2a`, and
`function` factories pass.

If you're authoring your own scoring source — for Datadog,
CloudWatch, a custom REST API, an internal SQL query, anything —
read this crate end-to-end as your template, then copy the
patterns. The architecture deliberately makes "ship a new source
type without forking NeuroGrim" a copy-and-edit job, not a
fork-and-rewrite job.

## What this crate ships

- **`PromSource`** — a `ScoringSource` impl that fetches a
  Prometheus instant-query result and converts the scalar value
  (clamped 0–100) into a `CmdbData` envelope.
- **`PromSourceFactory`** — produces `Box<dyn ScoringSource>` for
  the wire-name `"prom"`. Stateless; consuming binaries register
  it once at startup.
- **`tests/conformance.rs`** — the canonical contract check.
  Runs the cross-crate suite from
  `neurogrim_core::scoring_source_conformance` against
  `PromSourceFactory`. **Copy this file verbatim into your own
  crate**, rename the factory type, and you have the same
  guarantee.

## How a consuming binary registers it

```rust
use neurogrim_core::scoring_source::ScoringSourceRegistry;
use neurogrim_ecosystem::scoring_source::A2aSourceFactory;
use scoring_source_prom::PromSourceFactory;

fn build_registry() -> ScoringSourceRegistry {
    let mut registry = ScoringSourceRegistry::with_core_built_ins();
    // A2A factory ships in neurogrim-ecosystem (see its docs for why
    // it lives there and not in core).
    registry.register(Box::new(A2aSourceFactory));
    // Third-party Prom factory.
    registry.register(Box::new(PromSourceFactory));
    registry
}
```

A `brain-registry.json` domain entry then routes through
`PromSource`:

```json
{
  "service-saturation": {
    "weight": 1.0,
    "scoring_source": {
      "type": "prom",
      "endpoint": "http://prom.example.com/api/v1/query",
      "path": "100 - avg(node_load1{job=\"api\"}) * 10"
    }
  }
}
```

The example repurposes the `path` field for the PromQL expression.
`ScoringSourceConfig` is a closed shape shared by every source
type; reusing existing fields for type-specific meaning is the
intended extension pattern.

## Cargo.toml template for true third-party use

This crate's own `Cargo.toml` uses `workspace = true` because it
lives inside the NeuroGrim workspace. A third-party crate
**outside** the NeuroGrim workspace would write:

```toml
[package]
name = "my-scoring-source"
version = "0.1.0"
edition = "2021"

[dependencies]
neurogrim-core = "5"
async-trait = "0.1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls-native-roots"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
url = { version = "2", features = ["serde"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
tempfile = "3"
```

The only NeuroGrim dependency is `neurogrim-core`. You do **not**
depend on `neurogrim-mcp`, `neurogrim-ecosystem`, or any internal
crate. The trait + registry contract lives in `neurogrim-core` so
plugin authors get a stable, narrow public surface.

## Wire contract — Prometheus configuration

| Config field         | Required | Purpose                                                       |
| -------------------- | -------- | ------------------------------------------------------------- |
| `endpoint`           | yes      | Prometheus query API URL, e.g. `http://prom/api/v1/query`     |
| `path`               | yes      | PromQL expression to evaluate (repurposed from `path`)        |
| `interface_version`  | unused   | Reserved for future Prometheus API version negotiation        |
| `score_field`        | unused   | Prometheus values are scalar; no JSON field-name to override  |
| `updated_at_field`   | unused   | `updated_at` is `Utc::now()` (point-in-time queries)          |
| `no_file_score`      | unused   | Default fallback handled by NeuroGrim's scoring pipeline      |

## Failure modes (all surface as `None`, never panic)

The `ScoringSource` contract is **never panic; return `None` on any
failure**. `PromSource` honors it through these branches:

| Trigger                                | Outcome  | Log level   |
| -------------------------------------- | -------- | ----------- |
| Missing `endpoint` or `path`           | `None`   | silent      |
| Bad URL                                | `None`   | warn        |
| Connect failure / timeout / DNS error  | `None`   | warn        |
| HTTP non-2xx                           | `None`   | warn        |
| Response not parseable as Prom JSON    | `None`   | warn        |
| `status != "success"`                  | `None`   | warn        |
| Empty result vector                    | `None`   | silent      |
| Multi-element result vector            | first elt + warn | warn |
| Value not parseable as `f64`           | `None`   | warn        |
| Value is NaN or ±Inf                   | `None`   | warn        |

These are exactly the negative-path conditions the conformance
suite exercises (T6 skeletal-config, T7 concurrent-safety, T8
idempotency). Following the same discipline gives you a
verifiable third-party impl.

## Authoring your own — checklist

1. **Read** `neurogrim_core::scoring_source` rustdoc — the trait
   and registry contracts are documented there in full.
2. **Pick a wire-name** for your `source_type` (e.g. `"datadog"`,
   `"cloudwatch"`). It must be unique across all factories
   registered in any consuming binary; conventionally lowercase
   ASCII.
3. **Implement `ScoringSource`** with `#[async_trait]`. The two
   methods are `source_type_name(&self) -> &'static str` (returns
   your wire-name) and `async fn load(...) -> Option<CmdbData>`.
   For perf-critical paths, also implement an inherent
   `load_inherent` that the trait method delegates to (see
   `PromSource` here, or `CmdbSource` / `A2aSource` in
   `neurogrim-core`).
4. **Implement `ScoringSourceFactory`**. Usually a unit struct with
   `Box::new(YourSource)` in `build()`. If your source holds heavy
   state (an HTTP client, a connection pool), cache it on the
   factory and clone-on-build.
5. **Add the conformance test** at `tests/conformance.rs`, copying
   this crate's verbatim. **This is non-optional** — passing the
   suite is the verifiable contract that makes your impl safe to
   plug in.
6. **Document the wire contract** in your crate's README — what
   `endpoint` / `path` / etc. mean for your source type, and the
   failure modes table.
7. **Publish to crates.io** with a name like `scoring-source-foo`
   for discoverability. There is no central plugin registry in
   v5; consumers register your factory explicitly in their own
   `main.rs`.

## What this example does NOT do

- **No mock-server happy-path test.** Adding `wiremock` (or
  similar) would balloon the dev-deps. The unit tests in
  `src/lib.rs` cover the negative paths that don't need a server;
  a production third-party crate should add a happy-path test
  with a real or mocked Prometheus endpoint.
- **No connection-pool caching on the factory.** The factory
  builds a fresh `reqwest::Client` per `load()`. For a real
  Prometheus deployment hit dozens of times per scoring run, cache
  the client on the factory and clone-on-build.
- **No PromQL validation.** The query string is passed through to
  Prometheus verbatim; if it's malformed, Prometheus returns an
  HTTP 400 with `status: "error"`, which `PromSource` correctly
  surfaces as `None` with a warn log.
- **No multi-series aggregation.** When a query returns `>1`
  result, the example takes the first and warns. A consumer who
  needs deterministic single-value scoring should write an
  aggregating PromQL (`avg`, `max`, `sum_by`, etc.) — let
  Prometheus do the math.

## Cross-references

- **V5-MOD-1 plan:**
  `.claude/plans/v5-mod-1-scoring-source-trait.md` § Phase 6
- **`ScoringSource` trait + registry:**
  `crates/neurogrim-core/src/scoring_source.rs`
- **Conformance suite:**
  `crates/neurogrim-core/src/scoring_source_conformance.rs`
- **Built-in references:** `CmdbSource`
  (`crates/neurogrim-core/src/scoring_sources/cmdb.rs`), `A2aSource`
  (`crates/neurogrim-ecosystem/src/scoring_source.rs`).
