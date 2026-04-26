# layer-1/ — Mechanical SCA fixtures

Each subdirectory is one fixture exercising the
`supply-chain-sca` sensor. Per the parent README's
`fixture.toml` schema:

- `label = "known-bad"` → lockfile contains a known advisory
  match. Sensor MUST detect it.
- `label = "known-good"` → lockfile has no advisories. Sensor
  MUST NOT flag anything.
- `label = "edge-case"` → boundary cases (yanked packages,
  accepted-advisory entries, no-source-deps, etc.).

## Fixture-specific files

Each layer-1 fixture's directory contains:

- `fixture.toml` — metadata (required).
- One of:
  - `Cargo.lock` (Rust)
  - `uv.lock` or `requirements.txt` (Python)
  - `package-lock.json` (npm) or `yarn.lock` or `pnpm-lock.yaml`
- (optional) `.claude/supply-chain-accepted-advisories.toml` —
  accepted-advisory file the harness should honor.

The harness invokes `analyze_supply_chain_sca` with the fixture
directory as `project_root`. The sensor's normal `.claude/`
fall-back logic applies (the test harness sets up the right
directory shape).

## Expected outputs format

```toml
[expected]
# Advisory IDs the sensor should detect AS UNACCEPTED.
# Order does not matter; the harness compares as a set.
advisory_ids = ["RUSTSEC-2024-0436", "CVE-YYYY-NNNNN"]

# Optional: advisory IDs the sensor should detect AS ACCEPTED
# (i.e., they're in the accepted-advisories file and should not
# deduct from score).
accepted_advisory_ids = ["RUSTSEC-2024-0436"]

# Optional: expected sensor_status (default: not set; sensor
# should produce a normal score).
# Set to "lockfile_unreadable" for fixtures testing degradation.
sensor_status = ""

# Optional: minimum score the sensor should report. Useful for
# known-good fixtures where you want to assert "score == 100"
# without listing every absent advisory.
min_score = 100
```

## Known-good control set

Each `known-good` fixture is a snapshot of a healthy lockfile
with no current advisories. NeuroGrim's own `Cargo.lock` is the
prototype. Adopters' clean lockfiles are encouraged contributions.

The harness uses known-good fixtures to compute false-positive
rates: if the sensor flags ANYTHING on a known-good fixture, that
finding is a false positive.

## Layer 1 calibration semantics

L1 is **regression-check** semantics, not statistical-modeling.
The sensor is exact-match deterministic (lockfile parse + OSV
batch + RustSec local + accepted filter). Any non-zero FP/FN
indicates a bug, not a calibration miss.

Target rates (per scaffolding): <1% FP + <1% FN. v1 cannot
measure these at statistical-validity thresholds with ~10
fixtures.
