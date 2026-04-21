# NeuroGrim — Deployment Assets

This directory holds the non-Rust deployment glue: reverse-proxy config,
cert provisioning helpers, and shell-based smoke tests for the networked
topology. The Rust side (Brain code, A2A protocol, CLI) lives in
`../neurogrim/`.

```
deploy/
├── caddy/
│   ├── Caddyfile              # TLS terminator + reverse proxy config
│   └── generate-certs.sh      # optional mkcert helper
├── tests/
│   └── test_tls_endpoints.sh  # shell test: TLS + proxy round-trip
└── README.md                  # this file
```

## What the stack looks like

```
 host (127.0.0.1)
 │
 ├─ :80, :443 ─►  caddy                  (TLS terminator)
 │                  │
 │                  ├─► neurogrim-local       :8421  (Docker DNS name)
 │                  └─► neurogrim-external    :8421
 │
 ├─ :8421  ──►  neurogrim-local        (direct; kept for debug)
 └─ :8422  ──►  neurogrim-external     (direct; kept for debug)
```

Two reachable layers:

- **Through Caddy (preferred):**
  `https://neurogrim-local.localhost/.well-known/agent-card.json`
- **Direct (debug only):**
  `http://127.0.0.1:8421/.well-known/agent-card.json`

The `.localhost` TLD is reserved by RFC 6761 and resolves to 127.0.0.1 on
every modern OS — no `/etc/hosts` edits needed.

## Bring-up

```bash
cd D:/Brains/NeuroGrim
docker compose up -d --build
```

This builds the `neurogrim:dev` image (if not cached), starts both agent
containers, and starts Caddy. Bring-down:

```bash
docker compose down
```

To wipe Caddy's auto-generated root CA and re-issue fresh:
```bash
docker compose down -v      # -v removes the named volumes
```

## Trusting the dev CA

Caddy's built-in `tls internal` issues certs signed by a root CA it
generates on first run. That root lives in the `caddy_data` volume and
is stable across restarts as long as you don't pass `down -v`.

### Option A — trust Caddy's root in your OS (recommended for browsers)

```bash
# Copy the root cert out of the container
docker compose exec caddy cat /data/caddy/pki/authorities/local/root.crt > caddy-root.crt

# Import on Windows (admin PowerShell):
Import-Certificate -FilePath .\caddy-root.crt -CertStoreLocation Cert:\LocalMachine\Root

# OR on Linux:
sudo cp caddy-root.crt /usr/local/share/ca-certificates/caddy-root.crt
sudo update-ca-certificates
```

After that, browsers + `curl` trust `https://*.localhost` without
`--insecure` / `--cacert`.

### Option B — per-call `--cacert`

No admin rights? Hand the root cert to `curl` per call:

```bash
curl --cacert caddy-root.crt https://neurogrim-local.localhost/.well-known/agent-card.json
```

Same file works with `reqwest`, `httpx`, etc. — just point them at it.

### Option C — use mkcert instead

`mkcert` is a popular dev-CA provisioner that hooks into your OS trust
store directly (one-time install; subsequent cert issuance is trust-free).

```bash
bash deploy/caddy/generate-certs.sh
# Then follow the printed steps to patch Caddyfile + compose volumes.
```

See `generate-certs.sh --help` for the full workflow.

## Smoke test

The `tests/test_tls_endpoints.sh` script verifies:

- Caddy responds on 443 (HTTPS works)
- HTTP requests redirect to HTTPS (Caddy's built-in redirect)
- The Agent Card is reachable through TLS + valid JSON
- Unknown hostnames on Caddy are refused (no leaky default route)
- Direct loopback (bypassing Caddy) still works for debug

Run it after `docker compose up -d --build`:

```bash
bash deploy/tests/test_tls_endpoints.sh
```

Exit code is non-zero on any scenario failure.

## Production

This stack is local-dev posture by design. To run it in production:

1. **Public hostnames.** Replace `*.localhost` with real DNS names and
   make sure each hostname resolves to the host running Caddy.
2. **Real TLS.** In the Caddyfile:
   - Remove the `local_certs` from the global block.
   - Replace each site's `tls internal` with either:
     - `tls <email>` — Caddy auto-provisions Let's Encrypt via HTTP-01/
       TLS-ALPN-01. Requires ports 80 + 443 reachable from the public
       internet.
     - `tls /path/to/cert.pem /path/to/key.pem` — use certs you issued
       elsewhere.
3. **Loopback publication.** Change the `"127.0.0.1:443:443"` publishes
   in `docker-compose.yml` to `"0.0.0.0:443:443"` (and 80) so they
   bind externally.
4. **Bearer auth ON.** Run each agent with `--require-bearer
   --token-store /data/a2a-tokens.sqlite`. The Caddy layer gives you
   TLS + routing; bearer-auth gates **access** (Phase 1 of this plan).
   Issue tokens with `neurogrim a2a-token issue --label <who>`.
5. **Remove the loopback `ports:` on agents.** Once all access flows
   through Caddy, direct publication of 8421/8422 is an attack surface
   with no legitimate use.

## Troubleshooting

| Symptom | Likely cause + fix |
|---|---|
| `curl: (7) Failed to connect to neurogrim-local.localhost:443` | Caddy container isn't up. `docker compose ps` / `docker compose logs caddy`. |
| `curl: (60) SSL certificate problem` | Dev root not trusted — use `--cacert caddy-root.crt` or import the root per §Trusting the dev CA. |
| Browser shows `NET::ERR_CERT_AUTHORITY_INVALID` | Same cause as (60). Trust the dev root in the OS store. |
| `bad gateway` from Caddy | Agent container isn't up, or its port changed. `docker compose logs neurogrim-local`. |
| DNS for `*.localhost` fails | Only on very old / custom stub resolvers. Add an `/etc/hosts` entry (`127.0.0.1 neurogrim-local.localhost neurogrim-external.localhost`) as a workaround. |
| Changed the Caddyfile and nothing updated | `docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile` — Caddy reloads without dropping connections. |
