#!/usr/bin/env bash
# verify-compose.sh — smoke test for the dual-brain docker compose setup.
#
# Complements verify-external-brain.sh (single-container) with the
# documented but previously-untested two-container topology from
# docker-compose.yml (S6-DB-5). Builds the image via compose, starts
# both containers, waits for each to publish its Agent Card, invokes
# `snapshot.requested` against both, tears down.
#
# Exit codes:
#   0  both containers built, served, responded correctly
#   1  docker not available or fixture missing
#   2  either container failed to become ready within the timeout
#   3  invoke failed or response shape was wrong on at least one peer
#
# Usage:
#   ./scripts/verify-compose.sh
#
# CI note: GitHub's ubuntu-latest runners include Docker by default,
# so this script is safe to run as a CI job without extra setup.

set -euo pipefail

# Git Bash on Windows rewrites volume mount paths; disable for this script.
export MSYS_NO_PATHCONV=1

# --- Config ---
LOCAL_PORT=8421
EXTERNAL_PORT=8422
READY_TIMEOUT_SECS=45
TMPDIR_VERIFY=".verify-tmp-compose"
mkdir -p "$TMPDIR_VERIFY"

# --- Script context ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# --- Logging helpers ---
log()  { printf '\033[1;36m[compose]\033[0m %s\n' "$*" >&2; }
ok()   { printf '\033[1;32m[  ok   ]\033[0m %s\n' "$*" >&2; }
fail() { printf '\033[1;31m[ fail  ]\033[0m %s\n' "$*" >&2; }

# --- Cleanup on exit ---
# compose down always runs; removes containers + default network. The
# bind-mounted project dirs are untouched (read-only mounts + left in
# place because they're fixture data the script didn't create).
cleanup() {
    local rc=$?
    log "cleanup: docker compose down"
    docker compose down --remove-orphans >/dev/null 2>&1 || true
    rm -rf "$TMPDIR_VERIFY" 2>/dev/null || true
    if [ "$rc" -eq 0 ]; then
        ok "verify-compose.sh PASSED"
    else
        fail "verify-compose.sh FAILED (exit $rc)"
    fi
}
trap cleanup EXIT INT TERM

# --- Preflight ---
if ! command -v docker >/dev/null 2>&1; then
    fail "docker is not on PATH"
    exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
    fail "docker compose plugin not available (try docker-compose v1 → update to v2)"
    exit 1
fi
for fixture in neurogrim-local-project neurogrim-external-project; do
    if [ ! -d "$fixture/.claude" ]; then
        fail "fixture missing: expected $fixture/.claude/"
        exit 1
    fi
done

# --- 1. Bring up the stack ---
# `--build` rebuilds the image if the Dockerfile or source changed.
# `--wait` blocks until healthchecks pass, but we don't have
# healthchecks declared in compose (documented in docker-compose.yml
# §7), so we use `-d` + our own readiness loop below.
log "docker compose up --build -d"
if ! docker compose up --build -d >/dev/null 2>&1; then
    # Re-run with full output so the failure is visible.
    docker compose up --build -d >&2
    fail "docker compose up failed — see output above"
    exit 1
fi
ok "stack started"

# --- 2. Wait for both containers to publish their Agent Cards ---
wait_for_ready() {
    local url="$1"
    local name="$2"
    for i in $(seq 1 "$READY_TIMEOUT_SECS"); do
        if curl -fsS "$url" -o "$TMPDIR_VERIFY/${name}-card.json" 2>/dev/null; then
            return 0
        fi
        sleep 1
    done
    return 1
}

log "waiting for neurogrim-local on :${LOCAL_PORT}"
if ! wait_for_ready "http://127.0.0.1:${LOCAL_PORT}/.well-known/agent-card.json" local; then
    fail "neurogrim-local did not become ready within ${READY_TIMEOUT_SECS}s"
    docker compose logs neurogrim-local >&2 || true
    exit 2
fi
ok "neurogrim-local ready"

log "waiting for neurogrim-external on :${EXTERNAL_PORT}"
if ! wait_for_ready "http://127.0.0.1:${EXTERNAL_PORT}/.well-known/agent-card.json" external; then
    fail "neurogrim-external did not become ready within ${READY_TIMEOUT_SECS}s"
    docker compose logs neurogrim-external >&2 || true
    exit 2
fi
ok "neurogrim-external ready"

# --- 3. Invoke snapshot.requested against both, shape-check responses ---
# Prefer local binary → falls back to curl POST. Same pattern as
# verify-external-brain.sh.
invoke_peer() {
    local port="$1"
    local name="$2"
    local out="$TMPDIR_VERIFY/${name}-response.json"
    local payload='{
      "schema_version": "1",
      "message_id": "smoke-'"$$"'-'"$name"'",
      "timestamp": "2026-04-17T00:00:00Z",
      "brain_id": "smoke-verifier",
      "message_type": "snapshot.requested",
      "payload": {}
    }'
    if ! curl -fsS -X POST \
        -H 'Content-Type: application/json' \
        -d "$payload" \
        "http://127.0.0.1:${port}/a2a/v1/tasks" \
        -o "$out" 2>/dev/null; then
        fail "$name: POST to tasks endpoint failed"
        docker compose logs "$name" >&2 || true
        return 1
    fi
    # 202 Accepted body contains task_id — verify we got one.
    if ! grep -q '"task_id"' "$out"; then
        fail "$name: response missing task_id"
        head -200 "$out" >&2
        return 1
    fi
    return 0
}

log "invoking snapshot.requested on neurogrim-local"
if ! invoke_peer "$LOCAL_PORT" neurogrim-local; then exit 3; fi
ok "neurogrim-local accepted task"

log "invoking snapshot.requested on neurogrim-external"
if ! invoke_peer "$EXTERNAL_PORT" neurogrim-external; then exit 3; fi
ok "neurogrim-external accepted task"

# --- 4. Done ---
exit 0
