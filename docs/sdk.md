---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# NeuroGrim SDK

> Reference for extending NeuroGrim — writing custom sensors,
> consuming the Brain programmatically, building new domains.

## Canonical SDK: Rust

NeuroGrim's canonical SDK is **Rust**, via two crates from the
workspace:

| Crate | Purpose |
|---|---|
| [`neurogrim-core`](../neurogrim/crates/neurogrim-core) | Pure scoring engine. Zero I/O. Types + scoring functions + CMDB envelope model. |
| [`neurogrim-sensory`](../neurogrim/crates/neurogrim-sensory) | Built-in sensors + `Sensor` trait surface + CMDB helpers (`crate::cmdb::build_cmdb`). |

Any new sensor, scoring extension, or programmatic Brain consumer
should depend on these two. Both are published to crates.io once
the v3.0 supply-chain gate closes (see
[`BEFORE-PUBLIC-RELEASE.md`](../BEFORE-PUBLIC-RELEASE.md)).

### Minimum viable custom sensor

```toml
# your-crate/Cargo.toml
[dependencies]
neurogrim-core = "3.0"
neurogrim-sensory = "3.0"
# ... plus whatever your sensor needs (tokio, reqwest, etc.)
```

```rust
// your-crate/src/lib.rs
use neurogrim_sensory::cmdb::build_cmdb;
use serde_json::{json, Value};

pub async fn analyze_your_domain(project_root: &str) -> Value {
    // 1. Inspect the project.
    let findings = detect_something(project_root).await;

    // 2. Compute a score.
    let score = compute_score(&findings);

    // 3. Return a CMDB envelope.
    build_cmdb(
        "your-domain",                // updated_by
        score,                        // 0-100
        findings,                     // Vec<Finding>
        Some(json!({ "extra": "…" })) // domain-specific fields
    )
}
```

The `cmdb-envelope-v1` schema lives in the LSP Brains spec repo
(`schemas/cmdb-envelope-v1.schema.json`) and is backwards-compatible
across all v3.x NeuroGrim releases.

## Python SDK

An internal Python SDK (`lsp_brains` under
[`sdk-python/`](../sdk-python/)) exists. **It is not a shipped
artifact.** The ecosystem's locked decision:

- Canonical SDK for downstream extension is Rust.
- Python SDK remains in-repo for internal dogfooding, the 7
  spec-sensory-tool examples in LSP-Brains, and adopters who
  genuinely need Python — installable from source only
  (`pip install -e sdk-python/`).
- No PyPI publish is planned in the current release track. See
  `roadmap/BACKLOG.md` entry **B-20** for the full framing.

Rationale: the 2026-04-23 PyPI supply-chain incident — a
second-order compromise of a scanner binary in CI, which was then
used to publish trojanized releases of an otherwise-legitimate
package — surfaced a class of attack we would not confidently
defend against if we shipped our own PyPI artifact today. Rust's
supply-chain surface (crates.io publish token, workspace trust
chain, native SCA with OSV.dev-direct sensors) is narrower, better-
understood by our team, and audited to self-green by epic E-SC-0
of the supply-chain security scaffolding. Python will rejoin the
published-artifact roadmap only when: (a) we have equivalent
native SCA coverage, (b) PyPI's trusted-publishing /
attestation / SBOM story is mature enough to raise our integrity
posture, and (c) a concrete demand signal from users who couldn't
be served by the Rust SDK exists.

## SDK stability

- `neurogrim-core` public API is stable across `3.x` minor
  versions. Breaking changes earn a major-version bump.
- `neurogrim-sensory` public API is stable for the sensor
  framework (`Sensor` trait, `build_cmdb`, domain registration).
  Individual sensor implementations may evolve within minor
  versions as the LSP Brains spec does.
- The CMDB envelope schema is versioned separately (currently
  `v1`); schema bumps are signposted in the LSP Brains spec
  changelog.

## See also

- [`docs/getting-started.md`](getting-started.md) — clone → first
  score walkthrough.
- [`docs/cli-mode.md`](cli-mode.md) — bypass MCP; invoke the Brain
  via Bash subcommands.
- [`docs/cli-sensory-surface.md`](cli-sensory-surface.md) — MCP↔CLI
  surface mapping.
- [`roadmap/BACKLOG.md`](../roadmap/BACKLOG.md) — B-20 (Python SDK
  on PyPI — no current plan).
- LSP Brains spec: https://github.com/KeenanHoffman/LSP-Brains —
  normative contracts for Brain implementations.
