# Publish-Day Runbook — NeuroGrim `3.0.0`

The exact sequence of commands the operator runs on publish day.
Follow this top-to-bottom. Do not skip steps. Do not run from
memory.

---

## Scope (2026-04-24)

**This runbook covers the Rust (crates.io) publish path only.**

The Python SDK is **not published** — dogfood-only per the
2026-04-24 reframe. `neurogrim-core` + `neurogrim-sensory` are
the canonical Rust SDK. See `BEFORE-PUBLIC-RELEASE.md` §7,
BACKLOG B-20, and [`docs/sdk.md`](sdk.md). No PyPI section is
planned for this runbook in the current release track.

---

## Preconditions

Before touching any `cargo publish` command:

1. **Master gate 11 — Supply-chain security — CLOSED.** All eleven
   epics of the supply-chain security scaffolding (E-SC-0 through
   E-SC-10) complete. Phase 0 self-audit green against NeuroGrim's
   own deps AND Layers 1-3 shipped AND calibrated AND dogfooded
   across all four Brains. See
   [`../audit/ROLLBACK-PLAYBOOK.md`](../audit/ROLLBACK-PLAYBOOK.md)
   for per-epic verification. **This is the new master gate**;
   nothing below proceeds until this closes.
2. **Every gate in [`../BEFORE-PUBLIC-RELEASE.md`](../BEFORE-PUBLIC-RELEASE.md)
   that blocks this release is closed** — operator has walked it end to
   end and every relevant checkbox is `[x]`.
3. **Legal / trademark clearance** obtained (gate 1).
4. **Security audit** completed (gate 5 — now the narrow subset not
   covered by gate 11):
   - `cargo audit` clean or all findings triaged (by our native
     SCA, not by shelling out to scanner binaries — see
     `audit/TOOL-TRUST-NOTES.md`).
   - `cargo deny check` clean (via embedded `cargo-deny-core`
     library, not a separate binary).
   - secret-scanner sweep clean.
   - git history reviewed for secrets.
5. **Phase 0 self-audit run on the publish commit** — final
   `bash audit/phase0-run.sh` exits 0 on the tree that will be
   packaged. Do NOT trust a week-old audit report.
6. **Name re-check done today** (gate 2): confirm every crate name
   still `AVAILABLE` on crates.io. Do NOT trust the 2026-04-17
   snapshot.
7. **`crates.io` API token** in `~/.cargo/credentials.toml`. If the
   token is scoped per crate, verify it can publish all six names.
8. **Clean working tree.** `git status` is empty on a fresh checkout
   of `main`.
9. **Supply-chain CMDBs present.** `.claude/supply-chain-{sca,
   vigilance,review}-cmdb.json` exist (NeuroGrim tracks these in git
   so a fresh clone is sufficient). If any are absent, see
   § First-run bootstrap below — the strict-with-bypass L2 + L3
   gates fail-closed when their CMDBs are missing.

Do not proceed if any precondition is open.

---

## First-run bootstrap

The L1 (SCA), L2 (Vigilance), and L3 (Review) strict gates in
`scripts/prepublish-check.sh` fail-closed when their respective
CMDBs are absent (per the 2026-04-26 PRE-RELEASE assessment C4
fix). For NeuroGrim itself, the three CMDBs are tracked in git
so a fresh clone is sufficient and you can skip this section.

For **adopter projects** (or NeuroGrim post-deletion recovery),
bootstrap the three CMDBs in this order from the repo root:

```bash
cd neurogrim

# Layer 1 — Mechanical SCA. Queries OSV.dev; needs network.
cargo run --release -p neurogrim-cli -- \
    sensory supply-chain-sca --project-root . \
    > ../.claude/supply-chain-sca-cmdb.json

# Layer 2 — Vigilance. Queries crates.io / PyPI / npm; needs network.
cargo run --release -p neurogrim-cli -- \
    sensory supply-chain-vigilance --project-root . \
    > ../.claude/supply-chain-vigilance-cmdb.json

# Layer 3 — Review framework. Reads tickets + ledger; offline.
cargo run --release -p neurogrim-cli -- \
    sensory supply-chain-review --project-root . \
    > ../.claude/supply-chain-review-cmdb.json

cd ..
```

Then re-run `./scripts/prepublish-check.sh`. The first run after
bootstrap may surface findings that require triage via the L3
review flow before publish — that is the strict-with-bypass
posture working as intended.

For air-gapped operators: Layer 1 + Layer 2 require initial
network access to populate their caches at
`.claude/brain/cache/{osv,vigilance}/`. Once cached, subsequent
runs work offline (cache TTL is 24h for OSV, 7d for vigilance
registries). See `docs/supply-chain-sca.md` and
`docs/supply-chain-vigilance.md` for cache override env vars.

---

## Step 0 — Fresh checkout

`cargo publish` packages the working tree. Any stray file leaks.
Work from a freshly-cloned copy of main in a disposable directory.

```bash
cd /tmp
git clone git@github.com:KeenanHoffman/NeuroGrim.git neurogrim-publish
cd neurogrim-publish
git log -1  # confirm you're on the expected commit
```

---

## Step 1 — Run the pre-publish check

```bash
./scripts/prepublish-check.sh
```

Must exit 0. If any gate fails, stop. Fix it. Re-run. Do not
override.

The script:
- Verifies workspace version = `3.0.0`.
- Confirms `CHANGELOG.md` has a `[3.0.0]` entry.
- Confirms `LICENSE` + `docs/getting-started.md` +
  `docs/release-notes/v3.0.0.md` + `examples/hello-brain/*` +
  whitepaper exist.
- Runs `cargo check --workspace`.
- Runs `cargo test --workspace --all-targets`.
- Runs `cargo publish --dry-run` on each crate (bottom-up).
- Runs `cargo audit` if installed.
- Skips Python SDK (no current plan to publish; see B-20).

---

## Step 2 — Tag the release

```bash
git tag v3.0.0 -m "NeuroGrim 3.0.0 — first public stable release"
git push origin v3.0.0
```

Tag after the pre-publish check and before publishing, so if
publish fails partway, the tag still points at a known-tested
commit.

---

## Step 3 — Publish to crates.io (bottom-up)

**Order is dependency-forced.** A dependent crate cannot publish
until its deps are indexed on crates.io (takes ~30s to
propagate after each publish).

```bash
cd neurogrim

cargo publish -p neurogrim-core
# wait ~30s for the index to update before the next one
sleep 30

cargo publish -p neurogrim-a2a
sleep 30

cargo publish -p neurogrim-sensory
sleep 30

cargo publish -p neurogrim-mcp
sleep 30

cargo publish -p neurogrim-ecosystem
sleep 30

cargo publish -p neurogrim-cli
```

**Check after each step** that crates.io shows the new version
at `https://crates.io/crates/<crate>/versions`. If one fails,
STOP; don't continue the chain. See "Recovery" below.

### Optional — placeholder `neurogrim` crate

The CLI binary is named `neurogrim`, but the crate is
`neurogrim-cli`. If you want to reserve the plain `neurogrim`
crate name (an empty crate re-exporting from `neurogrim-cli`),
publish it last. Otherwise, skip this — the name stays reserved
by whoever claims it first, so plan before the publish window
opens.

---

## Step 4 — Verify the published crates

```bash
# Install the CLI from crates.io into a clean location
cargo install neurogrim-cli --version 3.0.0 --root /tmp/neurogrim-install

# Confirm it runs
/tmp/neurogrim-install/bin/neurogrim --version
# Expected: neurogrim 3.0.0

# Confirm the docs.rs build kicks off
# Visit: https://docs.rs/neurogrim-cli/3.0.0
# (may take 5-10 min after publish)
```

If `cargo install` fails or `--version` doesn't match, stop and
investigate before announcing.

---

## Step 5 — GitHub Release

```bash
# Create the release from the release-notes file
gh release create v3.0.0 \
  --title "NeuroGrim 3.0.0" \
  --notes-file docs/release-notes/v3.0.0.md
```

This is the first public stable release; do NOT pass
`--prerelease`. Future RC tags (e.g., `v3.1.0-rc.1`) would re-add
that flag.

---

## Step 6 — Update ecosystem + starter READMEs

The current READMEs say "install from source" for the Rust CLI
because crates.io publish hadn't happened yet. Now they can say
`cargo install neurogrim-cli`:

1. Edit `D:/Brains/NeuroGrim/README.md` to add `cargo install
   neurogrim-cli` as an alternative to building from source in the
   getting-started section.
2. Edit `D:/Brains/README.md` (ecosystem) similarly.
3. Python-starter README: leave the SDK framing as "install from
   source" — Python SDK remains dogfood-only (no current plan to
   publish; see B-20 + `docs/sdk.md`).
4. Commit + push.

---

## Step 7 — Announce

Wherever announcements happen (project README news banner,
Twitter/X, LinkedIn, Reddit r/rust, Hacker News, etc.). The
release notes are the canonical source; link them rather than
re-summarize.

Honest framing: this is the **first public stable release** with
**every gate intentionally closed**. Don't oversell. The v3.0.0
release notes already model the tone.

---

## Recovery

### One cargo publish fails mid-chain

- **`neurogrim-core` fails:** stop. Nothing has been published
  irreversibly. Fix the issue, bump the patch (3.0.1), re-tag,
  restart.
- **A dependent fails after `neurogrim-core` published:** the
  published `neurogrim-core` is permanent. Options:
  1. Yank `neurogrim-core` (does NOT free the name; later
     versions of the same crate can still publish). Fix and
     re-publish as `3.0.1`.
  2. Fix the dependent and publish the rest. Leaves `3.0.0` as a
     partial release.

Prefer option 1 for correctness. Yank instructions:
`cargo yank --version 3.0.0 neurogrim-core`.

### All crates published but verification fails

Cargo crates are immutable once published. Fix locally, bump to
`3.0.1`, publish again. Do NOT try to re-publish the same
version.

### Discovery: name was claimed in the hours between snapshot and publish

This is the single biggest squatting-risk window. If a name was
claimed since the 2026-04-17 snapshot: stop. Choose a new name,
update every `Cargo.toml` (`package.name`), update README +
CHANGELOG, bump, re-run the full pre-publish check. Do not try to
work around the collision.

### Supply-chain rollback window between tag and publish

(E-SC-10, 2026-04-26.) Between **Step 2 (tag)** and **Step 3
(publish)**, a window exists where new advisories could surface
in OSV.dev or registry-side trust signals could shift. The
`prepublish-check.sh` script defends this window with three
gates (LSP-Brains v2.6 §16):

- **L1 strict** — fresh `NEUROGRIM_OSV_NO_CACHE=1` rerun of
  supply-chain-sca; score MUST be 100.
- **L2 strict-with-bypass** — every Layer 2 vigilance finding
  must have a matching `review-triaged` ledger entry (bypass
  via canonical `sca-review resolve` flow).
- **L3 strict** — `tickets_open == 0` in supply-chain-review-
  cmdb.json.

If a NEW advisory or vigilance finding surfaces AFTER tag is
pushed but BEFORE `cargo publish` runs:

1. **Stop.** Do not run `cargo publish`.
2. **Re-run `bash scripts/prepublish-check.sh`** to confirm the
   gate is failing on the new signal. The fresh-OSV-rerun
   should have caught it.
3. **Force-delete the tag** locally + remotely. Yes, force —
   the alternative is publishing compromised code:
   ```bash
   git tag -d v3.0.0
   git push origin :refs/tags/v3.0.0
   ```
   This is destructive but necessary. Future operators reading
   the git history will see the deletion + the documented
   reason.
4. **Triage the new signal:**
   - **L1 advisory:** evaluate; either remediate via
     `cargo update -p <crate>` or accept via
     `.claude/supply-chain-accepted-advisories.toml` with
     non-empty `note`.
   - **L2 finding:** triage via `neurogrim sca-review resolve`
     through the canonical L3 flow.
   - **L3 ticket:** resolve via `neurogrim sca-review resolve`.
5. **Re-run `prepublish-check.sh`** until clean.
6. **Re-tag:**
   - If state materially changed (rebased, dep bumped, new
     commits): bump to `3.0.1`, update CHANGELOG, re-tag.
     Semver discipline.
   - If state did NOT change (you're confident the gate flapped
     transiently): re-tag the same version. Document in the
     commit log why this is safe.
7. **Continue from Step 3.**

**Why force-delete a pushed tag is the right call here:** the
alternative is publishing a version of NeuroGrim that contains
(or transitively depends on) a known-compromised dep. Yanking
post-publish is harder + leaves a permanent published artifact
on crates.io. Deleting an unused tag is recoverable; publishing
a bad version is not.

Cross-reference: `audit/ROLLBACK-PLAYBOOK.md` § E-SC-2/3/4
(Layer 1 procedures), § E-SC-5 (Layer 2), § E-SC-6 (Layer 3),
§ E-SC-8 (calibration regression), § Universal playbook.

### A2A `supply-chain-signal` failure handling (advisory-only)

(E-SC-10 + Round-2 R2-5, 2026-04-26.) When a Brain emits a
`supply-chain-signal` to peer Brains around publish time
(spec §16.6), the signal-flow is **advisory + best-effort**.
A2A signal failures DO NOT block publish — the local L1 / L2 /
L3 gates above are the authoritative publish-blockers.

**Failure modes to expect + the operator response for each:**

| Failure mode | Symptom | Response |
|---|---|---|
| **Peer unreachable** (network / down) | `TaskClient::invoke` returns `A2aError::Transport(...)` or `AgentCardUnreachable` | Signal drops silently. Peer-side `received-signals.jsonl` is unchanged. **No publish-block.** Resend manually after the peer recovers (operators typically do this via a dedicated `neurogrim a2a-invoke ...` re-issue). |
| **Bidirectional opt-in fails** | Sender's pre-flight `bidirectional_opt_in_satisfied(&local, &peer)` returns false | Sender does NOT emit. Verify BOTH peers' Agent Cards declare `supply-chain-signal` (sender in `emits[]`, peer in `accepts[]`). Re-run the discovery + retry. **No publish-block.** |
| **Peer ack-then-error** (peer's `default_handle_received` returns error) | Sender's `invoke` returns `A2aError::PeerError { status, body }` | Local emit succeeded over the wire; peer's append-to-log failed. Inspect peer's logs (operator-side); usually disk-full or schema-validation failure on the inbound payload. Local Brain's gates remain authoritative. **No publish-block.** |
| **Schema-validation rejection** at peer | Peer returns 400 with validation error in body | Sender's payload didn't match `a2a-supply-chain-signal-v1.schema.json`. This is a sender-side bug, not a publish blocker; defer to a follow-up commit. **No publish-block.** |

**Why advisory-only.** A2A signals are cross-Brain aggregation
input (spec §16.6: "Receivers MUST treat signals as advisory
input"). The local Brain's L1+L2+L3 gates already saw the
underlying findings before the signal was emitted; if a peer
is offline or rejecting, the local Brain's published version
is still based on its own complete picture. Signal-flow
failures are operator-action items for follow-up, not gate
flips.

**Operator action when signal-flow consistently fails:**

1. Run `neurogrim a2a-discover --peer-url <url>` against each
   declared peer; verify Agent Cards parse + advertise
   `supply-chain-signal`.
2. Inspect the local sender's outbox log (if any; v1 has no
   built-in outbox — failures are tracing-warn-logged only).
3. Inspect the peer's `received-signals.jsonl` for the most
   recent entry to confirm the connection was previously
   working.
4. File a separate commit / ticket; do NOT roll back the
   publish gate on signal-flow alone.

Cross-reference: `crates/neurogrim-a2a/src/supply_chain_signal.rs`
(impl), `crates/neurogrim-cli/tests/a2a_cli.rs`
(`supply_chain_signal_e2e_over_loopback` regression test).
Spec normative behavior: LSP-Brains v2.6 §16.6.

### v2 candidates (deferred work, post-publish)

These are documented as candidate work for v2 of the supply-
chain stack:

- **Cross-Brain A2A `supply-chain-signal` aggregation** — the
  payload shape, opt-in helpers, and default receive handler
  shipped in E-SC-10; the E2E loopback test landed in Round-2
  R2-5 (commit `4226265`). What remains for v2: a default
  AGGREGATOR that consumes a Brain's `received-signals.jsonl`
  + computes per-`(advisory_id, package)` `cross_brain_count`
  for the "two independent peers flagged this package" use
  case (spec §16.6). v1 ships the transport; the aggregation
  rollup is the v2 candidate. Not a publish-gate dependency.
- **L1 fixture-library OSV pre-caching** — calibration v1
  ships clean (no advisory matching against pre-cached OSV
  responses); v2 candidate adds a per-fixture `.osv-cache/`
  directory so calibration runs become deterministic.
- **L3 human-agreement metric ratification** — collect ≥30
  days of operator triage data; compute against fixture
  reference decisions; promote L3 to `pass` status.
- **Fixture-library quarterly refresh discipline** — automated
  drift detection on fixture freshness (last refresh date,
  reference-stale-CVE).
- **`supply-chain-calibration-report-v1` schema spec-promotion**
  — lift the NeuroGrim-internal schema to LSP-Brains v2.7 once
  cross-implementation adoption surfaces.

---

## Post-publish checklist

- [ ] Tag visible on GitHub at `v3.0.0`.
- [ ] All six crates visible on crates.io at `3.0.0`.
- [ ] docs.rs builds green for at least `neurogrim-cli` +
      `neurogrim-core`.
- [ ] GitHub Release created (no `--prerelease` flag — this is
      the first public stable release).
- [ ] `BEFORE-PUBLIC-RELEASE.md` gate 3 checkboxes flipped to `[x]`
      with "published 2026-MM-DD at commit <sha>" inline notes.
- [ ] This runbook annotated with "executed YYYY-MM-DD; notes:
      …" so future releases benefit from the post-mortem.

---

## If B-20 ever reactivates

Python SDK publication is dormant (no current plan). If a
reactivation trigger fires (see B-20 § Reactivation triggers),
this runbook gets a new "Step 3.5 — Publish to PyPI" section
authored at that time. Do NOT pre-author — the incident landscape
+ PyPI governance + our own SCA maturity will have changed by
then.

---

## References

- `BEFORE-PUBLIC-RELEASE.md` — gate status (including gate 11
  master supply-chain gate)
- `audit/ROLLBACK-PLAYBOOK.md` — supply-chain remediation
  procedures; per-epic populated as findings surface
- `scripts/prepublish-check.sh` — automated pre-flight
- `docs/release-notes/v3.0.0.md` — what shipped in this release
- `docs/sdk.md` — canonical Rust SDK; Python SDK framing
- `CHANGELOG.md` — keep-a-changelog format
- `roadmap/BACKLOG.md` — B-20 (Python SDK on PyPI; dormant)
