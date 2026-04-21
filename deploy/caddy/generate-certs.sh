#!/usr/bin/env bash
# NeuroGrim — optional mkcert-based cert provisioning.
#
# Alternative to Caddy's built-in `tls internal` CA. Use this when you want
# the dev certs signed by a root your OS already trusts (so browsers, curl,
# and language SDKs all just work without an extra `--cacert` flag).
#
# How it works:
#   1. `mkcert -install` adds a local root CA to the system trust store.
#      (one-time; subsequent runs are a no-op)
#   2. `mkcert <hostnames>` issues a cert signed by that root.
#   3. The issued cert + key land in `deploy/caddy/certs/`.
#   4. Uncomment the `tls ./certs/...` lines in the Caddyfile (or run
#      this script's `--patch-caddyfile` flag) and restart compose.
#
# If you don't want to install mkcert: stick with the default Caddyfile
# (`tls internal`). The only difference is you have to import Caddy's
# auto-generated root once — see deploy/README.md §Trusting the dev CA.
#
# Usage:
#   bash deploy/caddy/generate-certs.sh          # provision certs
#   bash deploy/caddy/generate-certs.sh --help   # show options

set -euo pipefail

HOSTNAMES=(
    "neurogrim-local.localhost"
    "neurogrim-external.localhost"
    "*.localhost"
)

CERT_DIR="$(dirname "$0")/certs"
CERT_FILE="${CERT_DIR}/neurogrim.pem"
KEY_FILE="${CERT_DIR}/neurogrim-key.pem"

show_help() {
    cat <<'EOF'
Provision mkcert-issued TLS certs for the NeuroGrim Caddy gateway.

Prerequisites:
  - mkcert installed (https://github.com/FiloSottile/mkcert)
  - admin/root rights on first run (mkcert -install modifies the OS trust store)

What this writes:
  deploy/caddy/certs/neurogrim.pem
  deploy/caddy/certs/neurogrim-key.pem

To use the issued certs, edit deploy/caddy/Caddyfile and replace
`tls internal` with:
  tls /etc/caddy/certs/neurogrim.pem /etc/caddy/certs/neurogrim-key.pem

Then mount the certs directory into Caddy by adding to docker-compose.yml:
  volumes:
    - ./deploy/caddy/certs:/etc/caddy/certs:ro

Restart the stack:
  docker compose up -d --force-recreate caddy
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    show_help
    exit 0
fi

if ! command -v mkcert >/dev/null 2>&1; then
    echo "error: mkcert not found on PATH" >&2
    echo "       install it: https://github.com/FiloSottile/mkcert#installation" >&2
    echo "       OR skip this script and use the Caddyfile's built-in 'tls internal' CA." >&2
    exit 2
fi

mkdir -p "${CERT_DIR}"

# Idempotent: mkcert -install skips if already installed.
echo "==> Installing mkcert root CA to OS trust store (may prompt for admin rights)…"
mkcert -install

echo "==> Issuing cert for: ${HOSTNAMES[*]}"
mkcert -cert-file "${CERT_FILE}" -key-file "${KEY_FILE}" "${HOSTNAMES[@]}"

echo ""
echo "✓ Cert written to ${CERT_FILE}"
echo "✓ Key  written to ${KEY_FILE}"
echo ""
echo "Next steps:"
echo "  1. Edit deploy/caddy/Caddyfile: replace each 'tls internal' with:"
echo "       tls /etc/caddy/certs/neurogrim.pem /etc/caddy/certs/neurogrim-key.pem"
echo "  2. Edit docker-compose.yml: add a volume mount to the caddy service:"
echo "       - ./deploy/caddy/certs:/etc/caddy/certs:ro"
echo "  3. Restart: docker compose up -d --force-recreate caddy"
