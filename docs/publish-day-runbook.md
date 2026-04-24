# Publish-Day Runbook — NeuroGrim `3.0.0-rc.1`

The exact sequence of commands the operator runs on publish day.
Follow this top-to-bottom. Do not skip steps. Do not run from
memory.

---

## Scope (2026-04-24)

**This runbook covers the Rust (crates.io) publish path only.**

The PyPI publish gate is **deferred** pending incident review +
supply-chain audit (see `BEFORE-PUBLIC-RELEASE.md` §7 and BACKLOG
B-20). When that gate reopens, append a PyPI section to this
runbook rather than inlining it now.

---

## Preconditions

Before touching any `cargo publish` command:

1. **Every gate in [`BEFORE-PUBLIC-RELEASE.md`](../BEFORE-PUBLIC-RELEASE.md)
   that blocks this release is closed** — operator has walked it end to
   end and every relevant checkbox is `[x]`.
2. **Legal / trademark clearance** obtained (gate 1).
3. **Security audit** completed (gate 5):
   - `cargo audit` clean or all findings triaged.
   - `cargo deny check` clean.
   - secret-scanner sweep clean.
   - git history reviewed for secrets.
4. **Name re-check done today** (gate 2): confirm every crate name
   still `AVAILABLE` on crates.io. Do NOT trust the 2026-04-17
   snapshot.
5. **`crates.io` API token** in `~/.cargo/credentials.toml`. If the
   token is scoped per crate, verify it can publish all six names.
6. **Clean working tree.** `git status` is empty on a fresh checkout
   of `main`.

Do not proceed if any precondition is open.

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
- Verifies workspace version = `3.0.0-rc.1`.
- Confirms `CHANGELOG.md` has a `[3.0.0-rc.1]` entry.
- Confirms `LICENSE` + `docs/getting-started.md` +
  `docs/release-notes/v3.0-rc.1.md` + `examples/hello-brain/*` +
  whitepaper exist.
- Runs `cargo check --workspace`.
- Runs `cargo test --workspace --all-targets`.
- Runs `cargo publish --dry-run` on each crate (bottom-up).
- Runs `cargo audit` if installed.
- Skips Python SDK (PyPI deferred).

---

## Step 2 — Tag the release

```bash
git tag v3.0.0-rc.1 -m "NeuroGrim 3.0.0-rc.1 — first public release candidate"
git push origin v3.0.0-rc.1
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
cargo install neurogrim-cli --version 3.0.0-rc.1 --root /tmp/neurogrim-install

# Confirm it runs
/tmp/neurogrim-install/bin/neurogrim --version
# Expected: neurogrim 3.0.0-rc.1

# Confirm the docs.rs build kicks off
# Visit: https://docs.rs/neurogrim-cli/3.0.0-rc.1
# (may take 5-10 min after publish)
```

If `cargo install` fails or `--version` doesn't match, stop and
investigate before announcing.

---

## Step 5 — GitHub Release

```bash
# Create the release from the release-notes file
gh release create v3.0.0-rc.1 \
  --title "NeuroGrim 3.0.0-rc.1" \
  --notes-file docs/release-notes/v3.0-rc.1.md \
  --prerelease
```

Use `--prerelease` because this is an RC. Switch to final release
when `3.0.0` ships.

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
   source" — PyPI is still deferred.
4. Commit + push.

---

## Step 7 — Announce

Wherever announcements happen (project README news banner,
Twitter/X, LinkedIn, Reddit r/rust, Hacker News, etc.). The
release notes are the canonical source; link them rather than
re-summarize.

Honest framing: this is an **RC** with **open gates documented**.
Don't oversell. The v3.0-rc.1 release notes already model the
tone.

---

## Recovery

### One cargo publish fails mid-chain

- **`neurogrim-core` fails:** stop. Nothing has been published
  irreversibly. Fix the issue, bump the patch (3.0.0-rc.2), re-tag,
  restart.
- **A dependent fails after `neurogrim-core` published:** the
  published `neurogrim-core` is permanent. Options:
  1. Yank `neurogrim-core` (does NOT free the name; later
     versions of the same crate can still publish). Fix and
     re-publish as `3.0.0-rc.2`.
  2. Fix the dependent and publish the rest. Leaves `-rc.1` as a
     partial release.

Prefer option 1 for correctness. Yank instructions:
`cargo yank --version 3.0.0-rc.1 neurogrim-core`.

### All crates published but verification fails

Cargo crates are immutable once published. Fix locally, bump to
`3.0.0-rc.2`, publish again. Do NOT try to re-publish the same
version.

### Discovery: name was claimed in the hours between snapshot and publish

This is the single biggest squatting-risk window. If a name was
claimed since the 2026-04-17 snapshot: stop. Choose a new name,
update every `Cargo.toml` (`package.name`), update README +
CHANGELOG, bump, re-run the full pre-publish check. Do not try to
work around the collision.

---

## Post-publish checklist

- [ ] Tag visible on GitHub at `v3.0.0-rc.1`.
- [ ] All six crates visible on crates.io at `3.0.0-rc.1`.
- [ ] docs.rs builds green for at least `neurogrim-cli` +
      `neurogrim-core`.
- [ ] GitHub Release created with `--prerelease` flag.
- [ ] `BEFORE-PUBLIC-RELEASE.md` gate 3 checkboxes flipped to `[x]`
      with "published 2026-MM-DD at commit <sha>" inline notes.
- [ ] This runbook annotated with "executed YYYY-MM-DD; notes:
      …" so future releases benefit from the post-mortem.

---

## When PyPI gate reopens

Append a "Step 3.5 — Publish to PyPI" section here when B-20
closes. Include the TestPyPI dry-run. Do NOT skip straight to
real PyPI.

---

## References

- `BEFORE-PUBLIC-RELEASE.md` — gate status
- `scripts/prepublish-check.sh` — automated pre-flight
- `docs/release-notes/v3.0-rc.1.md` — what shipped
- `CHANGELOG.md` — keep-a-changelog format
- `roadmap/BACKLOG.md` — B-20 (PyPI post-incident-review)
