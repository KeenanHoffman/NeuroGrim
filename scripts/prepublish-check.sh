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
EXPECTED_VERSION="3.0.0-rc.1"
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
  local files=(
    "$REPO_ROOT/docs/getting-started.md"
    "$REPO_ROOT/docs/release-notes/${EXPECTED_VERSION}.md"
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
  echo
  green "=== All non-skipped gates passed. ==="
  echo "Next step: review SKIPs above, then follow docs/publish-day-runbook.md."
}

main "$@"
