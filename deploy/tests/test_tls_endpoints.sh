#!/usr/bin/env bash
# NeuroGrim — TLS endpoint smoke test for the Caddy reverse proxy.
#
# Runs against an already-up compose stack. Assumes:
#   docker compose up -d --build     (run this before the test)
#
# What we verify (Phase 2 verification matrix):
#   1. Caddy is reachable on 443 with a valid-shape TLS handshake.
#   2. `https://neurogrim-local.localhost/.well-known/agent-card.json`
#      returns 200 + valid JSON when the dev root CA is trusted.
#   3. Plain `https://…` (no custom CA) is refused — the dev root is NOT
#      in the system store by default, which is the honest posture.
#   4. HTTP → HTTPS redirects work (Caddy's built-in behavior).
#   5. Unknown virtual-host names return 4xx, not a leaked default route.
#   6. The direct loopback (bypassing Caddy) still works — a debug escape
#      hatch we preserve on purpose until production hardening.
#
# Exit codes:
#   0 — every scenario passed
#   >0 — count of failed scenarios
#
# Usage:
#   bash deploy/tests/test_tls_endpoints.sh

set -uo pipefail

# Resolve paths relative to the repo root so we work regardless of CWD.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
# Cert extraction target. Using a path under REPO_ROOT (not /tmp) keeps
# `docker cp` happy on Windows — Git Bash would otherwise translate /tmp
# into an unreachable Windows path.
CERT_TMP="${REPO_ROOT}/.caddy-root-test.crt"
trap 'rm -f "${CERT_TMP}"' EXIT
# NOTE: we do NOT set MSYS_NO_PATHCONV globally. On Git Bash that variable
# disables MSYS's `/dev/null` → NUL translation, which breaks curl's
# `-o /dev/null`. Instead we set it inline on individual `docker` calls
# whose container-side paths (like `/data/…`) would otherwise be mangled.

pass_count=0
fail_count=0

pass() {
    echo "  ✓ $1"
    pass_count=$((pass_count + 1))
}
fail() {
    echo "  ✗ $1" >&2
    fail_count=$((fail_count + 1))
}

# Windows curl (schannel) quirks vs Linux curl (openssl/gnutls):
#   - schannel wants OCSP revocation info the Caddy internal CA doesn't
#     publish → pass `--ssl-no-revoke` when on Windows.
#   - schannel takes Windows-style paths for `--cacert`; Git Bash's
#     Unix-style paths confuse it. We translate through cygpath when
#     available.
# Detect once, apply throughout.
if curl --help all 2>/dev/null | grep -q -- "--ssl-no-revoke"; then
    CURL_EXTRA_FLAGS=(--ssl-no-revoke)
else
    CURL_EXTRA_FLAGS=()
fi

to_host_path() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$1"
    else
        echo "$1"
    fi
}

# -----------------------------------------------------------------------------
# Pre-flight
# -----------------------------------------------------------------------------
echo "==> Pre-flight"
command -v docker >/dev/null || { fail "docker CLI missing"; exit 1; }
command -v curl   >/dev/null || { fail "curl missing";       exit 1; }

compose_cmd=(docker compose -f "${REPO_ROOT}/docker-compose.yml")

# Wait briefly for Caddy to be ready — give it up to 20s to respond. We
# probe via a real hostname (not https://127.0.0.1/) because Windows curl
# uses schannel, which doesn't support SNI with IP addresses — and Caddy
# requires SNI to pick a site block. Using neurogrim-local.localhost
# (which resolves to 127.0.0.1 per RFC 6761) avoids that complication.
echo "==> Waiting for Caddy to respond on https://neurogrim-local.localhost…"
ready=0
for i in $(seq 1 20); do
    if curl -k -s -o /dev/null --max-time 2 "https://neurogrim-local.localhost/.well-known/agent-card.json"; then
        ready=1
        break
    fi
    sleep 1
done
if [[ "${ready}" != "1" ]]; then
    echo "  ✗ Caddy did not become ready within 20s — check \`docker compose logs caddy\`." >&2
    exit 1
fi
pass "Caddy reachable via https://neurogrim-local.localhost"

# -----------------------------------------------------------------------------
# Extract Caddy's internal root CA so we can validate the TLS chain.
# `docker cp` works uniformly across host OSes (no shell required inside
# the image) and side-steps the need for the container to have `cat` on
# PATH. The container path is `/data/caddy/pki/authorities/local/root.crt`.
#
# Windows wrinkle: Git Bash' `$(pwd)` yields a Unix-style path like
# `/d/Brains/NeuroGrim`. Docker CLI on Windows doesn't understand that
# form — it prepends the current drive letter and ends up with
# `D:\d\Brains\...`. Using `cygpath -w` translates to the native Windows
# path (`D:\Brains\...`) that docker expects. On Linux/macOS `cygpath`
# won't exist; fall back to the original path in that case.
# -----------------------------------------------------------------------------
echo "==> Extracting Caddy root CA"
rm -f "${CERT_TMP}"
CERT_TMP_HOST="$(to_host_path "${CERT_TMP}")"
if MSYS_NO_PATHCONV=1 docker cp "neurogrim-caddy:/data/caddy/pki/authorities/local/root.crt" "${CERT_TMP_HOST}" >/dev/null 2>&1; then
    # Sanity: did we actually get a PEM block?
    if grep -q "BEGIN CERTIFICATE" "${CERT_TMP}"; then
        pass "Extracted /data/caddy/pki/authorities/local/root.crt"
    else
        fail "Root CA extract produced non-PEM content"
    fi
else
    fail "docker cp from neurogrim-caddy failed — is the container up?"
fi

# -----------------------------------------------------------------------------
# Scenario 1 — HTTPS agent-card through Caddy with the dev root trusted.
# -----------------------------------------------------------------------------
echo "==> Scenario 1: HTTPS + trusted root → 200 + JSON"
for host in neurogrim-local neurogrim-external; do
    url="https://${host}.localhost/.well-known/agent-card.json"
    body="$(curl --cacert "${CERT_TMP_HOST}" "${CURL_EXTRA_FLAGS[@]}" -fsS --max-time 5 "${url}" 2>/dev/null || true)"
    if [[ -z "${body}" ]]; then
        fail "${url} did not return a body"
        continue
    fi
    # Validate it's JSON with the expected AgentCard shape.
    if echo "${body}" | grep -q '"schema_version"' && echo "${body}" | grep -q '"capabilities"'; then
        pass "${url} → 200 with AgentCard JSON"
    else
        fail "${url} returned a body but not an AgentCard shape"
    fi
done

# -----------------------------------------------------------------------------
# Scenario 2 — same request WITHOUT the dev root must fail the TLS chain.
# -----------------------------------------------------------------------------
# We pass --cacert /dev/null so curl doesn't pick up OS trust — isolating
# the test from whether the operator already ran "trust the dev root" in
# their OS store.
echo "==> Scenario 2: HTTPS + untrusted root → TLS error"
url="https://neurogrim-local.localhost/.well-known/agent-card.json"
if curl --cacert /dev/null -fsS --max-time 5 "${url}" >/dev/null 2>&1; then
    fail "${url} succeeded without custom CA — dev root is leaking into OS trust?"
else
    pass "${url} refused without --cacert (healthy)"
fi

# -----------------------------------------------------------------------------
# Scenario 3 — HTTP → HTTPS redirect.
# -----------------------------------------------------------------------------
echo "==> Scenario 3: HTTP redirects to HTTPS"
url="http://neurogrim-local.localhost/.well-known/agent-card.json"
# -I fetches headers only; -s silent. We want the 30x status without
# auto-following (so we can see the redirect, not the final body).
status_line="$(curl -sI --max-time 5 "${url}" 2>/dev/null | head -1 || true)"
case "${status_line}" in
    *" 301 "*|*" 302 "*|*" 307 "*|*" 308 "*)
        pass "HTTP returns ${status_line%$'\r'}"
        ;;
    *)
        fail "expected HTTP redirect, got: ${status_line:-<empty>}"
        ;;
esac

# -----------------------------------------------------------------------------
# Scenario 4 — unknown subdomain must NOT get served (no leaky default).
# -----------------------------------------------------------------------------
echo "==> Scenario 4: unknown virtual host → refused"
url="https://does-not-exist.localhost/"
# With strict SNI, Caddy rejects the handshake (curl prints http_code 000).
# A 4xx/5xx status would also be acceptable (means the site block didn't
# match but the handshake went through a fallback). A 2xx here would be a
# real problem — the honest posture is "no leaky default route."
out="$(curl --cacert "${CERT_TMP_HOST}" "${CURL_EXTRA_FLAGS[@]}" -sS -o /dev/null -w "%{http_code}" --max-time 5 "${url}" 2>/dev/null)"
# `curl` prints `000` when the connection/handshake fails. Treat that the
# same as a refusal here.
if [[ "${out}" == "000" ]] || [[ "${out}" =~ ^4 ]] || [[ "${out}" =~ ^5 ]]; then
    pass "unknown host → http_code ${out} (refused or rejected)"
else
    fail "unknown host returned ${out} — expected refusal or 4xx/5xx"
fi

# -----------------------------------------------------------------------------
# Scenario 5 — direct loopback still works (debug escape hatch preserved).
# -----------------------------------------------------------------------------
echo "==> Scenario 5: direct loopback bypass → 200"
for port in 8421 8422; do
    url="http://127.0.0.1:${port}/.well-known/agent-card.json"
    if curl -fsS --max-time 5 "${url}" >/dev/null 2>&1; then
        pass "${url} reachable"
    else
        fail "${url} not reachable — direct-publish was removed?"
    fi
done

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
total=$((pass_count + fail_count))
echo ""
echo "========================================"
echo "  ${pass_count} / ${total} checks passed"
if [[ "${fail_count}" -gt 0 ]]; then
    echo "  ${fail_count} failure(s) — see ✗ lines above" >&2
fi
echo "========================================"
exit "${fail_count}"
