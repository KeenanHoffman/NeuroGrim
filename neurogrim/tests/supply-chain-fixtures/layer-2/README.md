---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# layer-2/ — Vigilance fixtures

Each subdirectory is one fixture exercising the
`supply-chain-vigilance` sensor's seven sub-sensors. The fixture
ships a **frozen registry-metadata snapshot** so the test is
deterministic — no live network, no time-of-day variability.

## Fixture-specific files

Each layer-2 fixture's directory contains:

- `fixture.toml` — metadata (required).
- `metadata.json` — registry-API response captured at fixture-
  authoring time. Format: a JSON object with one entry per package,
  matching `crate::supply_chain_vigilance::registry::PackageMetadata`.
  ```json
  {
    "PyPI:litellm-synthetic": {
      "ecosystem": "PyPI",
      "name": "litellm-synthetic",
      "versions": [...],
      "owners": [...],
      "repository_url": null,
      "homepage_url": null
    }
  }
  ```
- (optional) `prior-state.json` — per-package historical state
  for delta sensors (`maintainer_delta`, `transitive_surface_delta`).
  Format matches `crate::supply_chain_vigilance::state::PackageState`.
- (optional) `lockfile` — a minimal lockfile listing the packages
  the harness should scan. If absent, the harness uses every
  package in `metadata.json`.

## Expected outputs format

```toml
[expected]
# List of expected findings. Each must match a finding produced
# by the sensor (kind + package). Extra fields ignored.
findings = [
  { kind = "publish-cadence-acceleration", package = "fakepkg", ecosystem = "PyPI" },
  { kind = "signature-gap", package = "fakepkg", ecosystem = "PyPI" },
]

# Optional: minimum score the sensor should report.
min_score = 60

# Optional: maximum score (for known-bad fixtures where a clean
# 100 would be wrong).
max_score = 90

# Optional: forbidden findings — kinds that MUST NOT fire on
# this fixture. Used for false-positive control.
forbidden_findings = [
  { kind = "exfil-indicator" },
]
```

## Known-good control set

Each `known-good` fixture is a frozen snapshot of healthy package
metadata. The harness expects ZERO findings on these fixtures;
any finding is a false positive.

Examples of healthy patterns to capture:
- Steady release cadence (no acceleration).
- Long-stable maintainer set (no maintainer-delta).
- Consistent attestation status across versions (no signature-gap).
- Widely-used package names (no typosquat — they ARE the popular
  target).

## Capturing live registry metadata

To author a new layer-2 fixture from a live package:

1. Identify the package + version of interest.
2. Run a one-off helper (TBD-on-execute; for v1 hand-capture):
   ```bash
   curl https://crates.io/api/v1/crates/<name> | jq . > fixture.toml.dir/raw-cratesio.json
   curl https://pypi.org/pypi/<pkg>/json | jq .  > fixture.toml.dir/raw-pypi.json
   ```
3. Convert to `metadata.json` matching the
   `PackageMetadata` shape. Match the field names + types from
   `crate::supply_chain_vigilance::registry`.
4. Record the capture date in `fixture.toml`'s `authored_at`.
5. Verify by running the harness; iterate.

## v1 limitation: live-payload fixtures

Per the 2026-04-26 E-SC-8 user-locked decision, v1 ships
SYNTHETIC reproductions only — no live malicious tarballs. The
LiteLLM-style fixtures recreate the pattern signatures (publish-
cadence acceleration, attestation drop) without committing the
actual compromised release.

Operators who want strictest tests can fetch the live payload
out-of-band; instructions live in the per-fixture
`fixture.toml`'s `references` field.

## Layer 2 calibration semantics

L2 is **probabilistic-heuristic** semantics. Target rates (per
scaffolding): <5% FP, <20% FN. v1 cannot measure these at
statistical-validity thresholds.

The seven sub-sensors are independently calibrated: a fixture
may exercise one or more of them. The harness reports per-kind
FP/FN rates plus an aggregate.
