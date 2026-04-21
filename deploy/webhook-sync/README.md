# neurogrim-webhook-sync

Signed-push → `git fetch && git reset` sidecar for NeuroGrim agent
workspaces. Part of the S6-DB-5 remote-agent topology (Phase 3 of the
plan in `roadmap/epics/` or the top-level plan file).

## Why it exists

Agents running in containers need their project source to stay current
with the upstream repo — but we don't want every agent container to
hold git credentials or run its own syncer. This service centralizes
the network-fetch surface behind a single audited endpoint, mounted
into the same named volume the agent reads from.

Flow:

```
GitHub push →  Caddy →  webhook-sync  →  git fetch  →  shared volume
                            │                              │
                            └─ audit log (stdout JSON)     └─ agent sees new HEAD
```

## Scope (v1)

- **IN:** HMAC-SHA256 verification (GitHub-format `X-Hub-Signature-256`),
  idempotency by `X-GitHub-Delivery`, per-agent debounce, hard-reset
  sync, JSON-lines audit log.
- **OUT:** GitLab/Bitbucket payload shapes, private-repo auth
  (deploy keys / PAT — public repos or pre-authorized SSH only in v1),
  delivery retries beyond GitHub's built-in policy, A2A "config.changed"
  notification to the agent, multi-tenant isolation.

Each exclusion is a known-scoped gap; add as needed.

## Quick start (local dev, outside Docker)

```bash
cd deploy/webhook-sync
pip install -e '.[dev]'

# Provide secrets via env
export NEUROGRIM_LOCAL_WEBHOOK_SECRET=$(openssl rand -hex 32)
cp config.example.toml config.toml
# edit config.toml to match your workspaces

WEBHOOK_SYNC_CONFIG=$(pwd)/config.toml \
uvicorn webhook_sync.app:app --host 127.0.0.1 --port 4747
```

Then from another shell:
```bash
BODY='{"ref":"refs/heads/main"}'
SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$NEUROGRIM_LOCAL_WEBHOOK_SECRET" | awk '{print "sha256="$2}')
curl -fsS -X POST http://127.0.0.1:4747/webhooks/neurogrim-local \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-GitHub-Delivery: $(uuidgen)" \
  -H "X-Hub-Signature-256: $SIG" \
  -d "$BODY"
```

## Running the test suite

```bash
cd deploy/webhook-sync
pip install -e '.[dev]'
pytest -q
```

Tests cover: HMAC verify/reject, idempotent-replay of duplicate
deliveries, debounce under rapid-fire pushes, ignored-event-type
and ignored-branch paths, and a true git-sync round-trip against a
tempdir bare repo.

## Docker

Build + run standalone:
```bash
docker build -t neurogrim-webhook-sync:dev deploy/webhook-sync
docker run --rm -p 4747:4747 \
  -v $(pwd)/deploy/webhook-sync/config.toml:/etc/webhook-sync/config.toml:ro \
  -v webhook-sync-state:/data \
  -v $(pwd)/workspaces/neurogrim-local:/workspaces/neurogrim-local \
  -e NEUROGRIM_LOCAL_WEBHOOK_SECRET=... \
  neurogrim-webhook-sync:dev
```

In the compose stack, the service is wired up automatically and Caddy
routes `https://webhooks.localhost/<agent>` to it — see the
top-level `docker-compose.yml` and `deploy/caddy/Caddyfile`.

## Audit log

One JSON object per line to stdout (container logs). Fields:

| Field | Notes |
|---|---|
| `ts` | wall-clock seconds since epoch |
| `event` | currently only `"webhook"` |
| `agent` | label from the URL path |
| `delivery_id` | `X-GitHub-Delivery` header |
| `branch` | configured sync branch |
| `action` | `git-sync`, or the event type if ignored |
| `result` | `ok` / `unauthorized` / `idempotent-replay` / `debounced` / `ignored-event-type` / `ignored-branch` / `bad-json` / `error` |
| `duration_s` | wall time for the sync (when applicable) |
| `head_sha` | post-sync HEAD sha (when applicable) |
| `error` | diagnostic string on failure (never user-submitted) |

No payload content ever appears. Push events can contain branch-name
privacy leakage — we log only the configured `branch`, not the ref
from the incoming payload, except when we explicitly ignore it.

## Security posture

- **HMAC is load-bearing.** Broken signature check = anyone can force
  a `git reset --hard` on the agent's workspace. `signature.verify`
  uses `hmac.compare_digest` (constant-time).
- **Secrets from env only.** The config file carries the *name* of the
  env var; the secret itself never hits disk inside the container.
  Rotate by changing the env var + restarting; no config edit.
- **Loopback → Caddy only.** The service binds inside the container
  and is only reachable via Caddy. Public exposure of 4747 is a
  misconfiguration.
- **Read-only config mount.** The TOML is mounted `:ro`; the service
  has no write path to its own config.
- **Non-root process.** Runs as UID 1000 matching the NeuroGrim image,
  so shared named volumes have consistent ownership.
