# Test SLO

V5-FOUND-2 (2026-05-03) introduced a per-test wall-time SLO for the NeuroGrim
workspace. Two thresholds, both measured warm (no rebuild between runs):

| Band | Threshold | Action at v5.0 |
|------|-----------|----------------|
| **Investigate** | ≥ 5.0s | Comment `// SLO-investigate: <duration>` above the test. No tag — test still runs by default. |
| **Violate** | ≥ 10.0s | Tag `#[ignore]` + comment `// SLO-violation: <duration>`. Test skipped by default; runs via `--slow` (which passes `--include-ignored` to the runner). |

The thresholds come from V5-FOUND-2 Fork D1 (operator-pinned 2026-05-03).

## v5.0 audit (2026-05-03)

Captured under `cargo nextest run --workspace --all-targets --profile default
--color never --no-fail-fast` on the V5-FOUND-2 baseline host (Intel i5-8300H,
8 GB RAM, Win10/MSYS2; see `roadmap/data/v5-test-baseline-2026-05-03.json`).

### SLO violations (tagged `#[ignore]` at v5.0)

All 9 violations are in `neurogrim-secrets` and share a single root cause: the
Argon2id KDF parameters (memory cost, iterations, parallelism) are fixed at
production levels for security, and tests run the real KDF. Fix tracked in
[BACKLOG B-48](../roadmap/BACKLOG.md): parameterize the KDF cost so tests can
opt into a reduced-cost profile.

| sec | crate | test |
|-----|-------|------|
| 98.236 | neurogrim-secrets | `encrypted_file::tests::list_returns_only_brain_id_scoped_secrets` |
| 89.104 | neurogrim-secrets | `encrypted_file::tests::smoke_check_file_succeeds_for_valid_passphrase` |
| 71.878 | neurogrim-secrets | `master_key::tests::derive_from_passphrase_different_passphrase_yields_different_key` |
| 69.991 | neurogrim-secrets | `encrypted_file::tests::wrong_passphrase_returns_bad_passphrase_error` |
| 64.432 | neurogrim-secrets | `encrypted_file::tests::set_then_get_round_trip` |
| 64.180 | neurogrim-secrets | `encrypted_file::tests::each_set_uses_fresh_salt_and_nonce` |
| 61.705 | neurogrim-secrets | `master_key::tests::derive_from_passphrase_different_salt_yields_different_key` |
| 52.033 | neurogrim-secrets | `master_key::tests::derive_from_passphrase_is_deterministic` |
| 45.468 | neurogrim-secrets | `encrypted_file::tests::delete_is_idempotent` |

### SLO investigate (commented only)

| sec | crate | test |
|-----|-------|------|
| 5.279 | neurogrim-sensory | `sensor_behavior::git_health_dirty_repo_scores_below_clean` |

## Operator notes

**The encrypted-secrets backend is intentionally excluded from default
`neurogrim test` runs at v5.0.** This is a known security/discipline trade-off
documented in V5-FOUND-2 Fork D1. To validate encrypted-secrets functionality
explicitly:

```bash
# Local: include all #[ignore]'d tests via --slow
neurogrim test --slow

# Or run the secrets crate alone with --include-ignored
cargo nextest run -p neurogrim-secrets --run-ignored all
```

**Recommendation for release prep:** run `neurogrim test --slow` at least once
before any release that touches `neurogrim-secrets`. This is currently a
discipline expectation, not an automated gate. B-48's fix (parameterize KDF
cost for tests) closes this gap by allowing the encrypted-file tests to run
fast in default mode while still using real Argon2id under `--slow`.

## Why audit-only at v5.0

The plan-critic 2026-05-03 review of V5-FOUND-2 flagged Phase 4 as a known
scope-creep magnet: investigating each ≥10s test and rewriting it to fit
under-SLO would have blown the M-budget by days. V5-FOUND-2 commits to
**tag-only** at v5.0 and queues the actual fixes to v5.5 (B-48).

Tag discipline lets the SLO machinery exist and surface the violations without
forcing the v5.0 release to also fix them.

## Re-running the audit

Capture a fresh `_audit_run.txt` and extract violations:

```powershell
cd D:/Brains/NeuroGrim/neurogrim
cargo nextest run --workspace --all-targets --profile default --color never --no-fail-fast 2>&1 |
  Out-File -Encoding utf8 D:/Brains/_audit_run.txt
```

Then parse PASS/FAIL lines for tests ≥ 5.0s. The current pattern is in the
PowerShell snippet from V5-FOUND-2 Phase 4 (see
`.claude/plans/v5-found-2-nextest-sccache.md`).

## Cross-references

- Plan: [`.claude/plans/v5-found-2-nextest-sccache.md`](../.claude/plans/v5-found-2-nextest-sccache.md) § Phase 4
- Backlog (fix queue): [`roadmap/BACKLOG.md`](../roadmap/BACKLOG.md) § B-48
- Baseline data: [`roadmap/data/v5-test-baseline-2026-05-03.json`](../roadmap/data/v5-test-baseline-2026-05-03.json)
- Fork pin: V5-FOUND-2 Fork D1 (5s/10s thresholds, tag-only consequence)
