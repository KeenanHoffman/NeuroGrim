# supply-chain-sca — Native-Rust Software Composition Analysis

The `supply-chain-sca` sensor is NeuroGrim's Layer 1 supply-chain
awareness — native-Rust, no external scanner binaries, pinned trust
surface. Three ecosystems supported as of E-SC-4 (2026-04-25):

- **Rust** via `Cargo.lock` (E-SC-2; shipped 2026-04-24).
- **Python** via `uv.lock` + `requirements*.txt` (E-SC-3;
  shipped 2026-04-25).
- **Node** via `package-lock.json` v2/v3 + `yarn.lock` (Classic +
  Berry) + `pnpm-lock.yaml` (E-SC-4; shipped 2026-04-25).

Per the supply-chain security scaffolding
(`audit/ROLLBACK-PLAYBOOK.md`, ecosystem repo).

## Why native-Rust, not `cargo audit` / `trivy` / etc.

On 2026-04-23 a PyPI supply-chain incident surfaced where a
*trojanized security scanner binary* was the attack vector: the
scanner itself ran in CI with credentials, exfiltrated them, and
attackers then published compromised releases of an otherwise-
legitimate package. The class of attack is "scanner-chain": the
very tools meant to protect against supply-chain compromise became
the compromise.

NeuroGrim's Layer 1 SCA deliberately avoids that class. It does not
shell out to `cargo audit`, `trivy`, `grype`, `osv-scanner`, or
`pip-audit`. The sensor is written in Rust, ships inside the
`neurogrim` binary, and queries OSV.dev directly over HTTPS.
External scanner-tool output is supported only as an opt-in
cross-check — never as a primary data source.

## Trust surface

What the sensor depends on:

- `neurogrim` binary itself (we built it, we audit it).
- `cargo-lock` v11 (RustSec-maintained; parses `Cargo.lock`).
- `reqwest` v0.12 with `rustls-tls-native-roots` (OS trust store,
  no OpenSSL).
- `sha2` v0.10 (RustCrypto; cache key hashing).
- `toml` v0.8 (dtolnay; advisory-frontmatter parsing).
- `semver` v1 (RustLang team; version-range matching).
- `chrono` v0.4 (cache TTL).
- OSV.dev HTTPS endpoint (Google-operated; open-source backend).
- `vendor/rustsec-advisory-db/` submodule pinned to a specific
  commit in `.gitmodules` (local advisory cross-reference + OSV-
  miss coverage + offline capability).

No scanner binary is in that list. That is the design.

## What the sensor does (pipeline)

```
Cargo.lock
  │
  ├─► cargo-lock parse + filter to crates.io-sourced deps
  │
  ├─► OSV.dev /v1/querybatch (cached 24h per dep)
  │       └─► (network optional: sensor degrades to cache-only if
  │            OSV is unreachable; osv_reachable=false flagged)
  │
  ├─► vendor/rustsec-advisory-db/ TOML walk (offline; union with OSV)
  │       └─► (submodule optional: sensor skips if not present)
  │
  ├─► .claude/supply-chain-accepted-advisories.toml filter
  │       └─► (file optional; hygiene filter skips entries without
  │            a non-empty `note`)
  │
  ├─► Count-based non-linear scoring (see table below)
  │
  └─► CMDB envelope JSON (conforms to cmdb-envelope-v1.schema.json)
```

## Score rubric (v1 — count-based)

| Unaccepted advisories | Score |
|---|---|
| 0 | 100 |
| 1 | 75 |
| 2 | 50 |
| 3 | 25 |
| 4+ | 0 |

Rationale: OSV batch responses don't carry per-advisory severity,
and many RustSec advisories (especially `informational =
"unmaintained"`) have no severity at all. A count-based rubric is
honest about what the sensor can measure reliably. Severity-
weighted scoring is a candidate upgrade in E-SC-8 (calibration).

**Accepted advisories** — entries in
`.claude/supply-chain-accepted-advisories.toml` — appear in CMDB
findings with `status=accepted` / `points=0` and do NOT contribute
to the unaccepted count.

## Running the sensor

### Quick one-off

```bash
neurogrim sensory supply-chain-sca --project-root <path>
```

This prints the CMDB envelope to stdout. `<path>` should point at
the directory containing `Cargo.lock`. If `.claude/` and `vendor/`
live one level up (unusual layouts like NeuroGrim's own workspace-
subdir arrangement), the sensor falls back to the parent
automatically.

### Refresh the CMDB for Brain scoring

```bash
neurogrim sensory supply-chain-sca --project-root . \
    > .claude/supply-chain-sca-cmdb.json
```

Then `neurogrim score` reads the CMDB via the registry declaration
(see "Brain registry declaration" below).

### Force fresh OSV queries (bypass cache)

```bash
NEUROGRIM_OSV_NO_CACHE=1 neurogrim sensory supply-chain-sca \
    --project-root .
```

Useful when:
- A new advisory landed in OSV < 24h ago and you want it NOW.
- You're debugging cache corruption.
- You're running the publish gate (fresh queries > cached).

Any truthy value works (`1`, `true`, `yes`, `anything-non-empty`);
the empty string, `0`, `false`, `no` preserve cached behavior.

## File layout

```
<project_root>/
├── Cargo.lock                                  (required)
├── .claude/
│   ├── supply-chain-accepted-advisories.toml   (optional; operator
│   │                                            triage)
│   ├── supply-chain-sca-cmdb.json              (output; per-scan)
│   └── brain/cache/osv/                        (24h cache; gitignored)
│       └── <sha256>.json                       (per dep+version)
└── vendor/
    └── rustsec-advisory-db/                    (optional; git submodule
                                                 pinned to a commit)
```

## `.claude/supply-chain-accepted-advisories.toml`

Operator-curated list of advisories that have been reviewed and
accepted as not-currently-actionable. Shape:

```toml
[[accepted]]
id = "RUSTSEC-2024-0436"
package = "paste"                              # optional (informational)
note = """
Unmaintained-notice from RustSec. Transitive via rmcp; proc-macro
only, no runtime attack surface. rmcp upstream owns the migration.
Escalation trigger: if RUSTSEC upgrades to CVE or crates.io
compromise is reported, re-evaluate immediately.
"""
# expires_at = "2026-10-24"                    # optional ISO-8601
```

**Hygiene lever:** the `note` field is required to be non-empty.
Entries without a non-empty rationale are silently skipped
(tracing warning is emitted). This is by design — acceptance
without a documented reason is the failure mode the file is
supposed to prevent.

**Expiration:** `expires_at` is optional. Past expirations are
skipped (tracing warning); the advisory continues to deduct from
the score until the operator re-reviews.

A richer 2-phase append-only ledger
(`supply-chain-decision-ledger.jsonl`) lands in E-SC-6. Until then,
this TOML file is the v1 triage surface.

## Brain registry declaration

To include the sensor in `neurogrim score`:

```json
"domain_weights": {
  …,
  "supply-chain-sca": 0.0
},
"domain_definitions": {
  …,
  "supply-chain-sca": {
    "scoring_source": {
      "type": "cmdb",
      "path": ".claude/supply-chain-sca-cmdb.json"
    }
  }
}
```

Advisory weight `0.0` is the v1 default — own-dep cleanliness is
used as a publish-gate binary check (must score 100), not a
numeric contribution to the unified score. Operators who want
supply-chain health to materially affect the unified score can
promote the weight after calibration.

## CMDB output shape

Key fields beyond the standard envelope:

| Field | Meaning |
|---|---|
| `score` | 0–100 per rubric above |
| `findings[]` | One per advisory (accepted + unaccepted) |
| `total_packages_scanned` | crates.io-sourced count (excludes workspace-local + git + alternative registries) |
| `advisories_found` | Count including accepted |
| `advisories_unaccepted` | Count that deducted |
| `advisories_accepted` | Count that matched the accepted-advisories file |
| `accepted_advisory_ids` | Array of matched IDs |
| `osv_reachable` | `false` means we degraded to cache + local RustSec only |
| `osv_cache_hits` | Cache hit count this run |
| `osv_live_queries` | Fresh OSV calls this run |
| `osv_oldest_cache_age_seconds` | Oldest cache entry consulted (null if none) |
| `osv_cache_bypassed` | `true` if `NEUROGRIM_OSV_NO_CACHE` was set |
| `rustsec_local_unique_hits` | Advisories RustSec-local caught that OSV did NOT return |
| `rustsec_local_unique_ids[]` | Per-advisory detail for the above |

The `rustsec_local_unique_hits` counter is the **OSV ingestion-lag
signal**. Non-zero means OSV hasn't yet propagated something that
our pinned RustSec submodule knows about — operator guidance is
usually "accept it into the advisories file OR remediate, don't
ignore."

## Graceful degradation

The sensor never panics on missing files:

- **No `Cargo.lock`** → valid CMDB with `sensor_status =
  "lockfile_unreadable"` + score 0 + one error finding.
- **No `vendor/rustsec-advisory-db/`** → silent skip of the local
  cross-reference. OSV is still consulted.
- **OSV unreachable** → falls through to cache + local RustSec.
  `osv_reachable: false` flagged in the CMDB.
- **Malformed advisory TOML** → logged + skipped; one bad file
  doesn't halt the scan.
- **Non-semver package version** (vanishingly rare on crates.io)
  → logged + skipped; OSV still sees the package.
- **Malformed accepted-advisories.toml** → logged + empty set;
  conservative posture (all advisories unaccepted).

## Out of scope for Layer 1 (post-E-SC-4)

These are NOT what this sensor does:

- **License + ban-list compliance** (what `cargo-deny` does) —
  deferred to BACKLOG B-21. License compliance is distinct from
  supply-chain *attack surface*; separating concerns keeps E-SC-2
  tight.
- **poetry.lock + Pipfile.lock** (Python) — deferred to BACKLOG
  B-22. uv.lock + requirements*.txt cover NeuroGrim's own usage;
  poetry/Pipfile add complexity without dogfood signal.
- **package-lock.json v1** (npm 5/6 era) — npm 7+ auto-upgrades v1
  on `npm install`; rare in 2026. Sensor logs + skips v1 with a
  warning telling the user to re-run `npm install`.
- **Deep-signal vigilance** (publish-cadence, maintainer-delta,
  binary-reproducibility, typosquat-proximity, exfil-indicator) —
  Layer 2, epic E-SC-5.
- **Agent-assisted human review** (LLM-as-judge on flagged deps) —
  Layer 3, epic E-SC-6.
- **Active blocking / auto-rollback** — v1 is advisory + operator-
  gated. The publish-day runbook enforces the gate via
  `prepublish-check.sh`.
- **Cross-Brain finding sharing** — E-SC-7 spec work; opt-in
  peer-Brain A2A.
- **Severity-weighted scoring** — see rubric rationale above;
  calibration candidate in E-SC-8.

## Cross-references

- Ecosystem plan: `~/.claude/plans/parallel-hugging-eich.md` (E-SC-2)
- Per-epic plan: `~/.claude/plans/parallel-hugging-eich-e-sc-2.md`
- Rollback procedures: `audit/ROLLBACK-PLAYBOOK.md § E-SC-2`
- Trust-chain notes: `audit/TOOL-TRUST-NOTES.md`
- BEFORE-PUBLIC-RELEASE.md gate 11 (master supply-chain gate)
- `scripts/prepublish-check.sh` — asserts
  `supply-chain-sca-cmdb.json` is present + score 100 before publish.
