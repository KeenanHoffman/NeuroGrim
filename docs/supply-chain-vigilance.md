---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# supply-chain-vigilance — Layer 2 Deep-Signal Sensors

The `supply-chain-vigilance` sensor is NeuroGrim's Layer 2
implementation of LSP-Brains v2.6 §16.3 — seven advisory sub-
sensors that detect publishing-behavior patterns preceding a
confirmed compromise. Native-Rust, no external scanner binaries
in the primary scoring path, registry-metadata only as the data
source for most sensors (tarball downloads opt-in for the two
heavier sensors).

Shipped 2026-04-25 as part of E-SC-5.

## Why Layer 2 exists

Layer 1 (`supply-chain-sca`, E-SC-2/3/4) catches **known-bad**
with high precision — it queries vulnerability databases for
exact-match advisories on each `(name, version, ecosystem)`
tuple. It does NOT catch supply-chain compromise that hasn't yet
propagated to a public advisory database.

The 2026-04-23 PyPI / LiteLLM incident motivates Layer 2: the
malicious payload was visible in the source diff hours before
the attack was confirmed in CVE databases. Deep-signal vigilance
detects "this package's release pattern looks anomalous" or
"this version contains base64 payloads that prior versions
didn't" — signals that PRECEDE the database-confirmed compromise.

Per spec §16.3, Layer 2 is **advisory by default**. Findings
inform operators; they never auto-gate. Promotion past advisory
weight requires §15.5-equivalent calibration evidence (E-SC-8).

## The seven sub-sensors

### Group A — registry metadata only (always-on)

1. **`typosquat_proximity`** — Levenshtein distance ≤ 1 to a
   top-popular package on the same registry. Flags packages one
   edit away from a popular name AND not themselves in the popular
   list. Static asset (top-N lists compiled-in via `data/typosquat-
   popular-<ecosystem>.txt`); refreshed quarterly via
   `scripts/refresh-typosquat-list.sh` (TBD-on-execute).
2. **`publish_cadence`** — Two signals on release frequency:
   - **Acceleration**: any inter-release gap in the last 30 days
     that is < 0.1× the historical median (10x speed-up).
   - **Post-dormancy**: any release within the last 30 days
     after a gap ≥ 365 days.
3. **`transitive_surface_delta`** — Dep-count delta between the
   currently-installed version and the immediate predecessor.
   Flags if absolute delta ≥ 10 OR relative delta ≥ +50%. Pure-
   data: computed from registry metadata.
4. **`maintainer_delta`** — New maintainer added within the last
   30 days. Compares against per-package observation history at
   `.claude/brain/cache/vigilance/state/`. **First-run posture:**
   no findings on the very first scan of a package; the run is a
   calibration scan that records observations. Subsequent scans
   flag genuinely new arrivals.
5. **`signature_gap`** — Attestation status drop between the
   currently-installed version and any prior version. Flags when
   prior versions had sigstore/trustpub but current does not.
   Pure-data: registry metadata only.

### Group B — opt-in (cryptographic verification)

6. **`binary_reproducibility`** — Verifies registry-claimed
   checksum matches what would actually be downloaded. Catches
   MITM-during-download and registry-internal tampering. Disabled
   by default; activate with `NEUROGRIM_VIGILANCE_REPRO=1`. Per-
   package result cached 7 days at
   `.claude/brain/cache/vigilance/repro/`. Reasoning for opt-in
   posture: tarball downloads add multi-MB per package on cold
   cache; let operators choose when to pay the cost.

### Group C — opt-in (source-content static analysis)

7. **`exfil_indicator`** — Static-analysis heuristics on the
   package's source tarball. Disabled by default; activate with
   `NEUROGRIM_VIGILANCE_EXFIL=1`. Patterns scanned:
   - **Long base64 strings** (≥ 200 chars) — encoded-payload
     indicator. Threshold: ≥ 3 long strings in package source
     triggers a finding.
   - **eval / exec / subprocess / Command::new / child_process /
     Function() / dynamic require** — language-specific patterns
     associated with runtime code generation or process
     spawning. Threshold: ≥ 8 aggregate hits.

   **Safe-extraction discipline:** uses `tar` + `flate2` (RustSec-
   tracked, well-audited) with explicit guards:
   - `set_preserve_permissions(false)` — don't honor +x bits.
   - `set_preserve_mtime(false)` — don't preserve timestamps.
   - **Path-traversal guard** rejects symlinks pointing outside
     the extraction root, absolute paths, and `..` components.
   - **Caps**: 50 MB tarball, 100 MB extracted total, 5 MB per
     file, 5000 files max.
   - **No execution** — extraction does NOT run `setup.py`,
     `npm install`, `cargo build`, or any post-install hooks.
   - Extraction directory cleaned up after analysis.

   Per-package result cached 7 days at
   `.claude/brain/cache/vigilance/exfil/`. v1 limitation: scans
   only the currently-installed version; does NOT diff against
   prior versions yet (a v2 enhancement; calibration in E-SC-8
   will validate v1's threshold-based heuristic first).

## Trust surface

What the sensor depends on (in addition to Layer 1's deps):

- `reqwest` v0.12 with `rustls-tls-native-roots` (already in
  Layer 1 — same posture).
- `sha2` v0.10 (already in Layer 1).
- `chrono` v0.4 (already in workspace).
- `tar` v0.4 (alexcrichton-maintained; RustSec-tracked) — used
  ONLY by `exfil_indicator` when activated.
- `flate2` v1 (alexcrichton-maintained; RustSec-tracked) — same
  scope.
- **HTTPS endpoints** (newly-trusted relative to Layer 1):
  - `https://crates.io/api/v1/crates/<n>` (publish history,
    owners, repository URL).
  - `https://pypi.org/pypi/<pkg>/json` (publish history,
    sigstore status, sdist URL).
  - `https://registry.npmjs.org/<n>` (publish history,
    maintainers, attestation, tarball URL).

All HTTPS endpoints are documented in
`audit/TOOL-TRUST-NOTES.md` per the 2026-04-25 E-SC-5 entry.
No scanner binary is in the trust surface — the §16.2 / §16.3
"no external scanner binaries in primary scoring path"
prohibition is honored.

## Score rubric (v1 — count-based)

Starting at 100. Each finding deducts a fixed amount per kind:

| Finding kind | Deduction |
|---|---|
| `typosquat-proximity` | 25 |
| `publish-cadence-acceleration` | 15 |
| `publish-cadence-post-dormancy` | 15 |
| `maintainer-delta` | 15 |
| `transitive-surface-delta` | 10 |
| `signature-gap` | 10 |
| `binary-reproducibility-mismatch` | 20 |
| `exfil-indicator` | 25 |
| `sensor-degradation` | 0 (informational) |

Score floor at 0; never negative. Severity-weighted scoring is
an E-SC-8 calibration candidate (see METHODOLOGY-EVOLUTION §15
"Deferred").

The bare kind names above (`typosquat-proximity`, `publish-
cadence-acceleration`, etc.) appear in CMDB findings'
`name` field as `<kind>:<ecosystem>:<package>`. When Layer 3
auto-creates a ticket from one of these findings, the ticket's
`signal_kind` carries the `vigilance:` prefix —
e.g., `vigilance:typosquat-proximity`. The canonical
signal_kind reference (across all three families: `vigilance:`,
`manual:`, future `agent-review:`) lives in
[`docs/supply-chain-review.md`](supply-chain-review.md) §
Signal-kind reference.

## Calibration targets (per spec §16.3 + §15.5)

All seven sub-sensors share the same Layer 2 calibration
contract:

| Metric | Target |
|---|---|
| False-positive rate | ≤ 5% |
| False-negative rate | ≤ 20% |
| Sample size for statistical validity | ≥ 30 fixtures per sensor |
| Default weight | 0.0 (advisory) |
| Promotion past advisory | requires §15.5-equivalent calibration evidence + LSP-Brains v2.6 §16.3 conformance |

**Documented per 2026-04-26 PRE-RELEASE C13** to give all seven
sub-sensors uniform calibration framing rather than implicit-
per-sensor targets. The §15.5 promotion path is the same gating
mechanism Layer 1 (`supply-chain-sca`) uses — all three Layers
are advisory by default and promote only on calibration
evidence.

Run calibration for vigilance (and the other layers):

```bash
neurogrim sca-calibrate --project-root . \
    --output .claude/supply-chain-calibration-report.json
```

See `docs/supply-chain-calibration.md` for the full harness +
fixture-library convention + promotion-readiness gating.

## Running the sensor

### Quick scan (always-on sensors only)

```bash
neurogrim sensory supply-chain-vigilance --project-root <path>
```

Activates Group A (typosquat + publish-cadence + transitive-
surface-delta + maintainer-delta + signature-gap). Prints the
CMDB envelope to stdout.

### With binary-reproducibility verification

```bash
NEUROGRIM_VIGILANCE_REPRO=1 \
  neurogrim sensory supply-chain-vigilance --project-root <path>
```

Adds Sensor 6. Each package's tarball is fetched once and its
SHA-256 verified against the registry-claimed checksum. Cached
7 days.

### With exfil-indicator static analysis

```bash
NEUROGRIM_VIGILANCE_EXFIL=1 \
  neurogrim sensory supply-chain-vigilance --project-root <path>
```

Adds Sensor 7. Each package's source tarball is downloaded,
extracted with safe-extraction discipline, and scanned for the
patterns described above. Cached 7 days.

### Both opt-ins together

```bash
NEUROGRIM_VIGILANCE_REPRO=1 NEUROGRIM_VIGILANCE_EXFIL=1 \
  neurogrim sensory supply-chain-vigilance --project-root <path>
```

### Bypass the 7-day cache (force fresh registry queries)

```bash
NEUROGRIM_VIGILANCE_NO_CACHE=1 \
  neurogrim sensory supply-chain-vigilance --project-root <path>
```

Useful when:
- A new release just landed and you want the freshest data.
- Debugging cache corruption.
- Acute investigation — the 7-day TTL would otherwise hide a
  signal that just emerged.

### Refresh CMDB for Brain scoring

```bash
neurogrim sensory supply-chain-vigilance --project-root . \
    > .claude/supply-chain-vigilance-cmdb.json
```

## File layout

```
<project_root>/
├── Cargo.lock / uv.lock / requirements*.txt /             (any supported lockfile)
│   package-lock.json / yarn.lock / pnpm-lock.yaml
├── .claude/
│   ├── supply-chain-vigilance-cmdb.json                   (output; per-scan)
│   └── brain/cache/vigilance/                             (gitignored)
│       ├── registry/<ecosystem>/<sha256>.json             (7-day registry-metadata cache)
│       ├── state/<ecosystem>/<sha256>.json                (per-package observation history;
│       │                                                   used by maintainer-delta)
│       ├── repro/<ecosystem>/<sha256>.json                (binary-repro results, opt-in)
│       └── exfil/<ecosystem>/<sha256>.json                (exfil-indicator results, opt-in)
└── crates/neurogrim-sensory/data/                         (compiled-in static lists)
    ├── typosquat-popular-cratesio.txt
    ├── typosquat-popular-pypi.txt
    └── typosquat-popular-npm.txt
```

## CMDB output shape

Key fields beyond the standard envelope:

| Field | Meaning |
|---|---|
| `score` | 0–100 per rubric above |
| `findings[]` | One per finding (across all 7 sensors) |
| `total_packages_scanned` | Count of unique `(name, version, ecosystem)` tuples |
| `findings_total` | Count of findings across all sensors |
| `findings_by_kind` | Per-kind breakdown (e.g., `{"typosquat-proximity": 2, "publish-cadence-acceleration": 1}`) |
| `ecosystems_scanned[]` | List of ecosystems present in the scanned graph |
| `packages_by_ecosystem` | Per-ecosystem package count |
| `vigilance_reachable` | `false` means we degraded to cache only (registries unreachable) |
| `registry_cache_hits` | Cache hit count this run |
| `registry_live_queries` | Fresh registry queries this run |
| `registry_oldest_cache_age_seconds` | Oldest cache entry consulted (null if none) |
| `registry_cache_bypassed` | `true` if `NEUROGRIM_VIGILANCE_NO_CACHE` was set |
| `registry_unreachable_ecosystems[]` | Ecosystems with zero successful fetches this run |
| `lockfile_parse_errors[]` | Any per-lockfile parse errors (mirror of Layer 1) |

## Graceful degradation

The sensor never panics on missing files or unreachable
registries:

- **No lockfile** → CMDB with `sensor_status = "lockfile_unreadable"`
  + score 0 + one error finding (mirror of Layer 1).
- **Registry unreachable** for a single package → silent skip.
  `vigilance_reachable: true` if any registry call succeeded.
- **Registry unreachable** for all packages in an ecosystem →
  ecosystem listed in `registry_unreachable_ecosystems`. The
  sensor still reports findings from cached metadata for any
  packages where cache was warm.
- **First-run on a package** → maintainer-delta records
  observations but emits no findings. Documented behavior.
- **Tarball oversized** (binary-repro / exfil) → silent skip.
- **Malformed cache file** → treated as a cache miss; live
  fetch attempted.

## Brain registry declaration

The `supply-chain-vigilance` domain ships in NeuroGrim's
`brain-registry.json` at weight 0.0 (advisory). To promote past
advisory weight, follow the §15.5 promotion protocol with
calibration evidence — E-SC-8 is the gating epic.

```json
"domain_weights": {
  …,
  "supply-chain-vigilance": 0.0
},
"domain_definitions": {
  …,
  "supply-chain-vigilance": {
    "scoring_source": {
      "type": "cmdb",
      "path": ".claude/supply-chain-vigilance-cmdb.json"
    }
  }
}
```

## Performance

Cold-cache scan on NeuroGrim's own dep graph (~187 cargo + 49
PyPI + 0 npm = ~236 unique packages):

- Group A only: ~3-5 minutes (one fetch per unique package, rate-
  limited 250ms inter-request).
- + Group B (binary-repro): adds ~5-10 minutes (tarball downloads
  for each package).
- + Group C (exfil): adds ~5-15 minutes (tarball + extract + scan).

Warm-cache scan (subsequent runs within 7 days): seconds for
Group A; near-instant for Groups B+C since their results are
cached.

## Cross-references

**Spec (LSP-Brains v2.6, 2026-04-25):**

- §16.3 Layer 2 — Vigilance — the contract this sensor
  implements.
- §16.6 A2A signal sharing — the consent model for cross-Brain
  finding sharing under bidirectional opt-in.
- METHODOLOGY-EVOLUTION §15 — the LiteLLM motivating incident +
  the no-scanner-binaries posture.

**Plans + scaffolding:**

- Ecosystem plan: `~/.claude/plans/parallel-hugging-eich.md` (E-SC-5)
- Per-epic plan: `~/.claude/plans/parallel-hugging-eich-e-sc-5.md`
- Rollback procedures: `audit/ROLLBACK-PLAYBOOK.md § E-SC-5`
- Trust-chain notes: `audit/TOOL-TRUST-NOTES.md` 2026-04-25 entry
- Companion sensor: `docs/supply-chain-sca.md` (Layer 1)
- BEFORE-PUBLIC-RELEASE.md gate 11 (master supply-chain gate)

## Out of scope (for v1; deferred to E-SC-5b / E-SC-8)

- **Prior-version diffing for exfil_indicator** — v1 uses
  threshold-based heuristics on the current scanned version
  only. Prior-version diffing requires per-version source-code
  caching; deferred until calibration (E-SC-8) validates the
  threshold approach.
- **Source-tag vs registry-tarball reproducibility** — v1's
  `binary_reproducibility` only verifies the registry-claimed
  checksum matches actual download. True source-vs-tarball
  comparison (which would catch pre-publish payload injection)
  is a v2 candidate; many packages legitimately differ between
  source-tag and registry tarball due to build artifacts.
- **Severity-weighted scoring** — count-based v1 is honest about
  data quality; severity weighting is an E-SC-8 calibration
  candidate.
- **Cross-Brain finding aggregation** — §16.6 specifies the A2A
  signal shape; aggregation rules are implementation-defined in
  v1 and a candidate for normative spec in v2.7+.
- **Active blocking / auto-rollback** — v1 is advisory + operator-
  gated per §16.3 MUST.
- **Reputation analysis (`download_count`, `github stars`, etc.)**
  — out of scope; reputation is a different signal class than
  vigilance and would invite different attack vectors.
