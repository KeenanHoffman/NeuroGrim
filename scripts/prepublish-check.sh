#!/usr/bin/env bash
#
# prepublish-check.sh — Mechanical pre-flight gates for a NeuroGrim
# release. Run this from the repo root before `cargo publish`.
#
# Exits 0 if all gates pass; non-zero on the first failure.
#
# What this does NOT do: anything irreversible. No `cargo publish`,
# no `twine upload`, no git push.
#
# Why Python SDK / PyPI steps are SKIPPED here (2026-04-23): a PyPI
# supply-chain incident in that window prompted deferral of the PyPI
# publish gate pending incident review + supply-chain audit. The
# Python SDK continues to ship as "install from source." When PyPI
# publish resumes (BACKLOG B-20), add:
#
#     (cd "$REPO_ROOT/sdk-python" && python -m build --sdist --wheel)
#     (cd "$REPO_ROOT/sdk-python" && twine check dist/*)
#
# to the PYTHON block below and flip CHECK_PYTHON=1.
#
set -euo pipefail

# ---------------------------------------------------------------
# Config
# ---------------------------------------------------------------
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE="$REPO_ROOT/neurogrim"
EXPECTED_VERSION="3.0.0"
CHECK_PYTHON=0  # PyPI publish deferred — see header comment.

# Crates in dependency order (for future `cargo publish` scripting).
CRATES=(
  neurogrim-core
  neurogrim-sensory
  neurogrim-a2a
  neurogrim-ecosystem
  neurogrim-mcp
  neurogrim-cli
)

# ---------------------------------------------------------------
# Pretty printers
# ---------------------------------------------------------------
red()   { printf '\033[0;31m%s\033[0m\n' "$*"; }
green() { printf '\033[0;32m%s\033[0m\n' "$*"; }
yellow(){ printf '\033[0;33m%s\033[0m\n' "$*"; }
blue()  { printf '\033[0;34m%s\033[0m\n' "$*"; }

pass() { green   "  [PASS] $*"; }
fail() { red     "  [FAIL] $*"; exit 1; }
skip() { yellow  "  [SKIP] $*"; }
info() { blue    "  [INFO] $*"; }

# ---------------------------------------------------------------
# Checks
# ---------------------------------------------------------------

check_version_consistency() {
  echo
  blue "== Version consistency =="
  local ws_version
  ws_version=$(grep -E '^version *=' "$WORKSPACE/Cargo.toml" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
  if [[ "$ws_version" == "$EXPECTED_VERSION" ]]; then
    pass "Workspace version = $EXPECTED_VERSION"
  else
    fail "Workspace Cargo.toml version is '$ws_version', expected '$EXPECTED_VERSION'"
  fi
}

check_changelog() {
  echo
  blue "== CHANGELOG.md =="
  local changelog="$REPO_ROOT/CHANGELOG.md"
  [[ -f "$changelog" ]] || fail "CHANGELOG.md missing at repo root"
  pass "CHANGELOG.md present"
  if grep -q "\[${EXPECTED_VERSION}\]" "$changelog"; then
    pass "Entry for [$EXPECTED_VERSION] present"
  else
    fail "No entry for [$EXPECTED_VERSION] in CHANGELOG.md"
  fi
}

check_license_files() {
  echo
  blue "== LICENSE files =="
  for path in "$REPO_ROOT/LICENSE"; do
    if [[ -f "$path" ]]; then
      if head -1 "$path" | grep -qi "MIT License"; then
        pass "$path (MIT)"
      else
        fail "$path exists but does not begin with 'MIT License'"
      fi
    else
      fail "Missing: $path"
    fi
  done
}

check_adoption_surface() {
  echo
  blue "== Adoption surface =="
  # Note: release-notes file uses the v<MAJOR.MINOR.PATCH>.md
  # convention (v3.0.0.md, not 3.0.0.md) per CHANGELOG +
  # BEFORE-PUBLIC-RELEASE + publish-day-runbook cross-references.
  # The previous rc.1 short-version convention (v3.0-rc.1.md) was
  # retired with E-B2-8's drop of the rc.1 suffix. Hardcoded to
  # match the actual filename convention; bump alongside
  # EXPECTED_VERSION for releases.
  local files=(
    "$REPO_ROOT/docs/getting-started.md"
    "$REPO_ROOT/docs/release-notes/v3.0.0.md"
    "$REPO_ROOT/examples/hello-brain/README.md"
    "$REPO_ROOT/examples/hello-brain/brain-registry.json"
    "$REPO_ROOT/examples/hello-brain/src/main.py"
    "$REPO_ROOT/examples/hello-brain/tests/test_main.py"
    "$REPO_ROOT/whitepaper/WHITEPAPER.md"
  )
  for f in "${files[@]}"; do
    [[ -f "$f" ]] && pass "$f" || fail "Missing: $f"
  done
}

check_cargo_build() {
  echo
  blue "== cargo check --workspace =="
  ( cd "$WORKSPACE" && cargo check --workspace --quiet 2>&1 )
  pass "cargo check clean"
}

check_cargo_test() {
  echo
  blue "== cargo test --workspace =="
  ( cd "$WORKSPACE" && cargo test --workspace --all-targets --quiet 2>&1 | tail -5 )
  pass "cargo test green"
}

check_cargo_publish_dryrun() {
  echo
  blue "== cargo publish --dry-run (per crate) =="
  for crate in "${CRATES[@]}"; do
    info "  $crate"
    if ( cd "$WORKSPACE" && cargo publish --dry-run -p "$crate" --quiet --allow-dirty 2>&1 | tail -5 ); then
      pass "  $crate dry-run OK"
    else
      fail "  $crate dry-run failed"
    fi
  done
}

check_cargo_audit() {
  echo
  blue "== cargo audit =="
  if command -v cargo-audit >/dev/null 2>&1; then
    ( cd "$WORKSPACE" && cargo audit 2>&1 | tail -20 )
    pass "cargo audit completed (review output above)"
  else
    skip "cargo-audit not installed — run 'cargo install cargo-audit' to enable"
  fi
}

check_python_sdk() {
  echo
  blue "== Python SDK (PyPI publish DEFERRED per B-20) =="
  if [[ "$CHECK_PYTHON" -eq 0 ]]; then
    skip "PyPI publish gate deferred (see script header + BACKLOG B-20)."
    skip "SDK source install still works: pip install -e sdk-python/"
    return
  fi
  # Re-enabling path (see header):
  # ( cd "$REPO_ROOT/sdk-python" && python -m build --sdist --wheel )
  # ( cd "$REPO_ROOT/sdk-python" && twine check dist/* )
}

check_metadata_completeness() {
  echo
  blue "== Cargo metadata completeness =="
  local required_fields=(description repository license authors)
  local required_workspace=(keywords categories readme homepage documentation rust-version)

  # Workspace-level fields
  for field in "${required_fields[@]}" "${required_workspace[@]}"; do
    if grep -qE "^${field} *=" "$WORKSPACE/Cargo.toml"; then
      pass "workspace.package.$field"
    else
      fail "workspace.package.$field missing"
    fi
  done

  # Per-crate: each must inherit or declare description
  for crate in "${CRATES[@]}"; do
    local manifest="$WORKSPACE/crates/$crate/Cargo.toml"
    if grep -qE '^description *=' "$manifest"; then
      pass "$crate has description"
    else
      fail "$crate is missing 'description'"
    fi
  done
}

check_supply_chain_sca() {
  # BEFORE-PUBLIC-RELEASE.md gate 11: master supply-chain gate.
  # The supply-chain-sca CMDB at .claude/supply-chain-sca-cmdb.json
  # MUST exist + score 100 before any cargo publish.
  #
  # E-SC-10 (2026-04-26): adds the LiteLLM-equivalent rollback
  # discipline. We FIRST run a fresh OSV scan (NEUROGRIM_OSV_NO_CACHE=1)
  # BEFORE checking the CMDB. This catches advisories that surfaced in
  # OSV between the last cached scan and now. The fresh scan
  # OVERWRITES the cached CMDB; if a new advisory landed, the score
  # drops and prepublish fails — which is the correct rollback signal
  # for the tag-vs-publish window.
  echo
  blue "== Supply-chain SCA (gate 11 master gate, E-SC-10 fresh-OSV-rerun) =="
  local cmdb="$REPO_ROOT/.claude/supply-chain-sca-cmdb.json"

  info "Running fresh OSV-bypassed SCA scan (LiteLLM-equivalent rollback gate)..."
  local fresh_tmp="$cmdb.fresh.$$"
  if (cd "$WORKSPACE" && \
      NEUROGRIM_OSV_NO_CACHE=1 cargo run --release --quiet -p neurogrim-cli -- \
        sensory supply-chain-sca --project-root . 2>/dev/null) > "$fresh_tmp"; then
    if [[ -s "$fresh_tmp" ]]; then
      mv -f "$fresh_tmp" "$cmdb"
      pass "Fresh SCA scan complete; $cmdb updated"
    else
      rm -f "$fresh_tmp"
      info "Fresh scan produced empty output; falling back to cached $cmdb"
    fi
  else
    rm -f "$fresh_tmp"
    info "Fresh OSV-bypassed scan failed (network unreachable?). Falling back to cached CMDB."
    info "  Manual fresh scan: cd neurogrim && NEUROGRIM_OSV_NO_CACHE=1 cargo run --release -p neurogrim-cli -- \\"
    info "      sensory supply-chain-sca --project-root . > ../.claude/supply-chain-sca-cmdb.json"
  fi

  if [[ ! -f "$cmdb" ]]; then
    fail "Missing $cmdb — run: \
      cd neurogrim && cargo run --release -p neurogrim-cli -- sensory supply-chain-sca --project-root . \
      > ../.claude/supply-chain-sca-cmdb.json"
  fi
  pass "$cmdb present"

  # Extract score via Python stdlib (no jq dependency; matches the
  # Phase 0 helpers' approach in audit/scripts/).
  local score
  score="$(python -c "
import json, sys
d = json.load(open('$cmdb'))
print(d.get('score', -1))
" 2>/dev/null)" || score="$(py -3 -c "
import json, sys
d = json.load(open('$cmdb'))
print(d.get('score', -1))
" 2>/dev/null)"

  if [[ "$score" != "100" ]]; then
    fail "supply-chain-sca score is $score, must be 100. \
Inspect findings in $cmdb; either remediate the unaccepted advisories \
(\`cargo update -p <crate>\`) or accept them with rationale in \
.claude/supply-chain-accepted-advisories.toml. See \
$REPO_ROOT/docs/supply-chain-sca.md."
  fi
  pass "supply-chain-sca score = 100"
}

check_supply_chain_vigilance_strict_with_bypass() {
  # E-SC-10 (2026-04-26): Layer 2 strict-with-bypass gate.
  #
  # Each L2 vigilance finding (typosquat / publish-cadence /
  # signature-gap / etc.) MUST have a corresponding RESOLVED entry
  # in the supply-chain-decision-ledger.jsonl. Bypass: the auto-
  # create bridge in supply_chain_review creates a ticket for each
  # finding; operator runs `neurogrim sca-review resolve` to triage
  # via the canonical L3 flow, which writes a `review-triaged`
  # entry that satisfies this gate.
  #
  # Match key: (ecosystem, package_name, signal_kind) where
  # signal_kind = "vigilance:<finding-kind>".
  echo
  blue "== Supply-chain Vigilance (gate 11 strict-with-bypass, E-SC-10 L2) =="
  local vig_cmdb="$REPO_ROOT/.claude/supply-chain-vigilance-cmdb.json"
  local ledger="$REPO_ROOT/.claude/supply-chain-decision-ledger.jsonl"

  # 2026-04-26 PRE-RELEASE C4 fix: previously this branch logged
  # an info + returned 0 (silent skip). The strict-with-bypass
  # posture requires fail-closed when the gate's CMDB is absent —
  # operators who deleted/bootstrapped fresh must regenerate
  # before re-running prepublish-check.sh.
  if [[ ! -f "$vig_cmdb" ]]; then
    fail "L2 vigilance gate requires $vig_cmdb (strict-with-bypass posture). \
Bootstrap: cd neurogrim && cargo run --release -p neurogrim-cli -- \
sensory supply-chain-vigilance --project-root . > ../.claude/supply-chain-vigilance-cmdb.json. \
Then re-run prepublish-check.sh. \
See docs/publish-day-runbook.md § First-run bootstrap."
  fi
  pass "$vig_cmdb present"

  # Walk findings + cross-check ledger via the extracted helper at
  # scripts/_supply-chain-bypass-check.py. The helper enforces:
  #   * strict JSONL parse (corrupt ledger lines fail the gate
  #     instead of being silently skipped — 2026-04-26 C5 fix)
  #   * `<kind>:<eco>:<pkg>` finding-name format validation
  #     (unknown formats fail the gate instead of being silently
  #     skipped — 2026-04-26 C6 fix)
  # Exit codes from the helper:
  #   0 = all findings triaged
  #   1 = un-triaged findings present
  #   2 = script error (parse/format/usage)
  local helper="$REPO_ROOT/scripts/_supply-chain-bypass-check.py"
  local untriaged_output rc
  if untriaged_output="$(py -3 "$helper" "$vig_cmdb" "$ledger" 2>&1)"; then
    rc=0
  else
    rc=$?
    if untriaged_output="$(python3 "$helper" "$vig_cmdb" "$ledger" 2>&1)"; then
      rc=0
    else
      rc=$?
    fi
  fi

  if [[ "$rc" -eq 0 ]]; then
    pass "L2 strict gate: $untriaged_output"
  elif [[ "$rc" -eq 2 ]]; then
    red "  [FAIL] L2 strict gate: helper reported a script-level error"
    echo "$untriaged_output" | sed 's/^/    /'
    info "Likely causes: corrupt ledger JSONL line OR finding-name format drift."
    info "Inspect the helper output above + repair the underlying file."
    fail "supply-chain-vigilance gate failed — see helper output"
  else
    red "  [FAIL] L2 strict gate: un-triaged findings present"
    echo "$untriaged_output" | sed 's/^/    /'
    info "Bypass path: triage each finding via the L3 review flow:"
    info "  1. List open tickets:   neurogrim sca-review list --open-only --project-root ."
    info "  2. Review each ticket;  see docs/supply-chain-review.md for the auditor checklist."
    info "  3. Resolve each:        neurogrim sca-review resolve --id <id> \\"
    info "                            --decision <accept|reject|pin-to-last-good|no-action> \\"
    info "                            --note '<rationale>' --operator <handle>"
    info "  4. Re-run prepublish-check.sh."
    fail "supply-chain-vigilance gate failed — see bypass path above"
  fi
}

check_supply_chain_review_strict() {
  # E-SC-10 (2026-04-26): Layer 3 strict gate.
  #
  # supply-chain-review-cmdb.json's tickets_open MUST be 0 before
  # publish. Open tickets are pending operator decisions; publishing
  # while un-triaged would mean publishing without operator
  # acknowledgement of every flagged dep.
  #
  # Bypass: resolve each ticket via `neurogrim sca-review resolve`.
  echo
  blue "== Supply-chain Review (gate 11 strict, E-SC-10 L3) =="
  local rev_cmdb="$REPO_ROOT/.claude/supply-chain-review-cmdb.json"

  if [[ ! -f "$rev_cmdb" ]]; then
    # 2026-04-26 PRE-RELEASE C4 fix (L3 side): the strict-with-bypass
    # posture requires fail-closed when the gate's CMDB is absent.
    # See docs/publish-day-runbook.md § First-run bootstrap.
    fail "L3 review gate requires $rev_cmdb (strict-with-bypass posture). \
Bootstrap: cd neurogrim && cargo run --release -p neurogrim-cli -- \
sensory supply-chain-review --project-root . > ../.claude/supply-chain-review-cmdb.json. \
Then re-run prepublish-check.sh. \
See docs/publish-day-runbook.md § First-run bootstrap."
  fi
  pass "$rev_cmdb present"

  local open_count
  open_count="$(py -3 -c "
import json
d = json.load(open(r'$rev_cmdb'))
print(d.get('tickets_open', -1))
" 2>/dev/null)" || open_count="$(python3 -c "
import json
d = json.load(open(r'$rev_cmdb'))
print(d.get('tickets_open', -1))
" 2>/dev/null)"

  if [[ "$open_count" == "0" ]]; then
    pass "L3 strict gate: 0 open tickets"
  elif [[ "$open_count" == "-1" ]]; then
    fail "Could not parse tickets_open from $rev_cmdb"
  else
    red "  [FAIL] L3 strict gate: $open_count open ticket(s)"
    info "Bypass path: resolve each open ticket via the canonical L3 flow:"
    info "  1. List open tickets:   neurogrim sca-review list --open-only --project-root ."
    info "  2. Review each ticket;  see docs/supply-chain-review.md."
    info "  3. Resolve each:        neurogrim sca-review resolve --id <id> \\"
    info "                            --decision <accept|reject|pin-to-last-good|no-action> \\"
    info "                            --note '<rationale>' --operator <handle>"
    info "  4. Re-run prepublish-check.sh."
    fail "supply-chain-review gate failed — see bypass path above"
  fi
}

check_brains_2_0_cmdb_presence() {
  # E-B2-8 (2026-04-27): gate 12 strict CMDB-presence check.
  #
  # Brains-2.0 introduced four new advisory domains across all four
  # Brains (ecosystem + NeuroGrim + LSP-Brains + python-starter):
  #   * domain-calibration   (E-B2-2)
  #   * trust-budget         (E-B2-4)
  #   * operator-calibration (E-B2-6)
  #   * federated-patterns   (E-B2-7)
  #
  # Each Brain MUST have a parseable CMDB for each of these four
  # domains before publish (4 Brains x 4 CMDBs = 16 files). The
  # three supply-chain CMDBs are gate 11's domain — not double-
  # counted here.
  #
  # Confidence-as-envelope (E-B2-1), hat-contract (E-B2-3), and
  # METH-EV §16 (E-B2-5) are envelope/spec deliverables and do not
  # have separate CMDBs.
  echo
  blue "== Brains-2.0 CMDB presence (gate 12, E-B2-8 strict) =="

  local brains=(
    "ecosystem:$REPO_ROOT/../.claude"
    "neurogrim:$REPO_ROOT/.claude"
    "lsp-brains:$REPO_ROOT/../LSP-Brains/.claude"
    "python-starter:$REPO_ROOT/NeuroGrim-python-starter/.claude"
  )
  local cmdbs=(
    domain-calibration-cmdb.json
    trust-budget-cmdb.json
    operator-calibration-cmdb.json
    federated-patterns-cmdb.json
  )

  local entry brain_name brain_dir cmdb_name cmdb_path parse_rc
  for entry in "${brains[@]}"; do
    brain_name="${entry%%:*}"
    brain_dir="${entry#*:}"
    for cmdb_name in "${cmdbs[@]}"; do
      cmdb_path="$brain_dir/$cmdb_name"
      if [[ ! -f "$cmdb_path" ]]; then
        fail "Brains-2.0 CMDB missing for $brain_name: $cmdb_path. \
Bootstrap the corresponding sensory tool for that Brain (see \
docs/publish-day-runbook.md § Brains-2.0 CMDBs) and re-run \
prepublish-check.sh."
      fi
      if py -3 -c "
import json
json.load(open(r'$cmdb_path'))
" >/dev/null 2>&1; then
        parse_rc=0
      elif python3 -c "
import json
json.load(open(r'$cmdb_path'))
" >/dev/null 2>&1; then
        parse_rc=0
      else
        parse_rc=1
      fi
      if [[ "$parse_rc" -ne 0 ]]; then
        fail "Brains-2.0 CMDB unparseable for $brain_name: $cmdb_path. \
Inspect the file; regenerate via the corresponding sensory tool."
      fi
    done
  done
  pass "Brains-2.0 CMDBs present + parseable across 4 Brains (16 CMDBs total)"
}

check_brains_2_0_advisory_weights() {
  # E-B2-8 (2026-04-27): gate 12 advisory-weight invariant.
  #
  # All four Brains-2.0 domains are advisory (weight 0.0) per the
  # locked decision in the layer-2 plan. If any Brain's
  # brain-registry.json declares one of these domains with a
  # non-zero weight, it has been escalated to gating without spec
  # amendment — fail-closed before publish.
  #
  # Domains that are NOT declared in a given Brain are skipped
  # (a Brain may legitimately not adopt every Brains-2.0 domain).
  echo
  blue "== Brains-2.0 advisory-weight invariant (gate 12, E-B2-8) =="

  local registries=(
    "ecosystem:$REPO_ROOT/../.claude/brain-registry.json"
    "neurogrim:$REPO_ROOT/.claude/brain-registry.json"
    "lsp-brains:$REPO_ROOT/../LSP-Brains/.claude/brain-registry.json"
    "python-starter:$REPO_ROOT/NeuroGrim-python-starter/.claude/brain-registry.json"
  )

  local entry brain_name registry_path violations rc
  for entry in "${registries[@]}"; do
    brain_name="${entry%%:*}"
    registry_path="${entry#*:}"
    if [[ ! -f "$registry_path" ]]; then
      fail "Brain registry missing for $brain_name: $registry_path"
    fi
    if violations="$(py -3 -c "
import json, sys
d = json.load(open(r'$registry_path'))
weights = d.get('config', {}).get('domain_weights', {}) or {}
b2_domains = ('domain-calibration', 'trust-budget', 'operator-calibration', 'federated-patterns')
bad = []
for name in b2_domains:
    if name in weights:
        w = weights[name]
        if w != 0.0 and w != 0:
            bad.append('%s=%s' % (name, w))
if bad:
    print(';'.join(bad))
    sys.exit(1)
" 2>&1)"; then
      rc=0
    else
      rc=$?
      if violations="$(python3 -c "
import json, sys
d = json.load(open(r'$registry_path'))
weights = d.get('config', {}).get('domain_weights', {}) or {}
b2_domains = ('domain-calibration', 'trust-budget', 'operator-calibration', 'federated-patterns')
bad = []
for name in b2_domains:
    if name in weights:
        w = weights[name]
        if w != 0.0 and w != 0:
            bad.append('%s=%s' % (name, w))
if bad:
    print(';'.join(bad))
    sys.exit(1)
" 2>&1)"; then
        rc=0
      else
        rc=$?
      fi
    fi
    if [[ "$rc" -ne 0 ]]; then
      red "  [FAIL] $brain_name: non-zero weight on Brains-2.0 advisory domain(s): $violations"
      info "Brains-2.0 domains MUST be advisory (weight 0.0) until a"
      info "spec amendment escalates them. Either revert the weight in"
      info "$registry_path, or land the spec amendment first."
      fail "Brains-2.0 advisory-weight invariant violated in $brain_name registry"
    fi
  done
  pass "All declared Brains-2.0 domains at advisory weight 0.0"
}

check_brains_2_0_cross_brain_integration() {
  # E-B2-8 (2026-04-27): gate 12 cross-Brain integration compile.
  #
  # The federated-pattern loopback E2E test (BR-6 cross-Brain
  # mandate) MUST compile cleanly before publish. We use --no-run
  # for a fast compile-check; W5's full workspace test run
  # exercises it for real.
  echo
  blue "== Brains-2.0 cross-Brain integration test (gate 12, E-B2-8) =="

  local cargo_output rc
  if cargo_output="$(cd "$WORKSPACE" && cargo test -p neurogrim-a2a --test federated_pattern_loopback_e2e --no-run 2>&1)"; then
    rc=0
  else
    rc=$?
  fi
  if [[ "$rc" -eq 0 ]]; then
    pass "Cross-Brain federated-pattern integration test compiles cleanly"
  else
    red "  [FAIL] federated_pattern_loopback_e2e failed to compile (rc=$rc)"
    echo "$cargo_output" | tail -20 | sed 's/^/    /'
    info "Inspect the cargo output above; the test lives at"
    info "  neurogrim/crates/neurogrim-a2a/tests/federated_pattern_loopback_e2e.rs"
    fail "Cross-Brain integration test failed to compile"
  fi
}

# ---------------------------------------------------------------
# Main
# ---------------------------------------------------------------
main() {
  blue "=== prepublish-check.sh (target version: $EXPECTED_VERSION) ==="
  check_version_consistency
  check_changelog
  check_license_files
  check_adoption_surface
  check_metadata_completeness
  check_cargo_build
  check_cargo_test
  check_cargo_publish_dryrun
  check_cargo_audit
  check_python_sdk
  check_supply_chain_sca
  check_supply_chain_vigilance_strict_with_bypass
  check_supply_chain_review_strict
  check_brains_2_0_cmdb_presence
  check_brains_2_0_advisory_weights
  check_brains_2_0_cross_brain_integration
  echo
  green "=== All non-skipped gates passed. ==="
  echo "Next step: review SKIPs above, then follow docs/publish-day-runbook.md."
}

main "$@"
