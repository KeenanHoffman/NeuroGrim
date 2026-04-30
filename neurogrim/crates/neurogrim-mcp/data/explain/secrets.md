<!-- topic: secrets — bundled in neurogrim-cli v3.5 -->
# Secrets — never plaintext

Encrypted secrets are the v4.2 epic — closing the threat-model gap
that's been documented since the claude-proxy MVP. The discipline:
**a memory dump of the running dashboard or proxy yields encrypted
blobs, not plaintext secrets**. Plaintext only exists for the
microseconds during the actual upstream API call.

This topic covers v4.2 S14's foundation stories. The cross-repo
claude-proxy migration (S-4) and the operator-facing UI page (S-6)
ship in follow-up sessions.

<!-- anchor: four-layer -->
## Four-layer encryption model

| Layer | Where | What this crate provides |
|---|---|---|
| **Wire** | TCP between browser and dashboard | TLS via self-signed cert (S14-S-4.5 v1: cert lifecycle; v2: HTTPS server binding via axum-server + rustls; v3: HTTPS banner + browser TOFU fingerprint pinning + 426 path-level enforcement on the HTTP listener; v4: HTTP→HTTPS auto-redirect for the Secrets page) |
| **Process boundary** | JSON in/out | dashboard zeroizes request buffers (paired with S-4.5) |
| **In-memory** | runtime values | `EncryptedSecretValue` + `MasterSessionKey` (this stage) |
| **At-rest** | OS / disk | `OsNativeBackend` or `EncryptedFileBackend` (this stage) |

<!-- anchor: in-memory -->
## In-memory encryption

The `MasterSessionKey` is a 32-byte ChaCha20Poly1305 key derived
from the OS credential store at process startup. Every
`EncryptedSecretValue` in process memory is wrapped under this
master key. A memory dump yields ciphertext.

```rust
use neurogrim_secrets::{MasterSessionKey, SecretValue, EncryptedSecretValue};

// At process startup:
let master = MasterSessionKey::load_or_generate("neurogrim")?;

// On a fresh secret arrival (e.g., from operator's UI form):
let plaintext = SecretValue::from_string("sk-ant-…".to_string());
let in_memory = plaintext.into_encrypted(&master)?;
//                       ^^^^^^^^^^^^^^^ plaintext drops here, zeroized

// On use (microseconds-long window):
let bytes: Zeroizing<Vec<u8>> = in_memory.decrypt_for_use(&master)?;
upstream_api_call(&bytes);
//                     ^^^^^ bytes drops at scope exit, zeroized
```

`SecretValue` and `EncryptedSecretValue` deliberately do NOT
implement `Display`. `Debug` is implemented but redacts content:

```text
SecretValue { plaintext: [REDACTED; len=32] }
EncryptedSecretValue { ciphertext: [REDACTED; len=48], nonce: [REDACTED] }
```

<!-- anchor: at-rest -->
## At-rest backends

Two implementations:

### `OsNativeBackend`

Wraps the [`keyring`](https://crates.io/crates/keyring) crate:

| Platform | Underlying API |
|---|---|
| Windows | DPAPI (Credential Manager) |
| macOS | Keychain |
| Native Linux (with seahorse) | libsecret over D-Bus |

Service-name convention: `neurogrim-{brain_id}-{secret_id}`.
Adopters' credentials show up in the OS native credential UI (e.g.,
Windows Credential Manager) under that name.

**Failure modes:**

- WSL without seahorse → `apt install gnome-keyring libsecret-1-0
  dbus-x11` to enable libsecret, OR fall back to the encrypted-file
  backend.
- Container / CI → no credential store; encrypted-file fallback.
- Headless Linux → same.

### `EncryptedFileBackend`

ChaCha20Poly1305 for content + PBKDF2-HMAC-SHA256 for key
derivation from operator passphrase. One file per secret under
`<project>/.claude/brain/secrets/{brain_id}__{secret_id}.enc`.

**v1 file format:**

```json
{
  "version": 1,
  "salt":       "<64 hex chars = 32 bytes>",
  "nonce":      "<24 hex chars = 12 bytes>",
  "ciphertext": "<2N hex chars = N bytes (auth tag baked in)>",
  "metadata": {
    "created_at": "RFC3339",
    "updated_at": "RFC3339",
    "rotation_days": null
  }
}
```

PBKDF2 iterations: 600,000 (OWASP 2023 guidance for SHA-256).
Salt + nonce rotate on every `set()`. The version field is
reserved for future format migration.

Wrong-passphrase failures return `SecretError::BadPassphrase` —
distinguished from `MalformedFile` so operators can debug
incident-response without ambiguity.

## Agent-side surface: `secret_fetch` MCP tool

```
secret_fetch(secret_id: String, scope?: String) -> {token, expires_at}
```

The agent **never** sees the underlying secret value. It receives an
opaque proxy token that authorizes ONE upstream API call through
claude-proxy and expires in 60 seconds. Pass via
`X-Scope-Token: <token>` header.

**Default autonomy:** `Approve`. Every `secret_fetch` call lands on
the S13 approvals queue and requires explicit operator approval via
the Approvals UI page (`/brains/:id/approvals`). The agent calls
`await_approval(action_id)` to poll for the operator's decision.

Adopters can downgrade per-secret to `Notify` for low-sensitivity
public APIs via the registry's `autonomy.action_types` override.

<!-- anchor: tls-cert -->
## TLS on the dashboard's secret endpoints (S14-S-4.5)

The dashboard's secret-management surface (`/api/brains/:id/
secrets/...`) carries plaintext secret values over the wire on
the operator's request. Loopback-only deployments make this
safe in practice; defense-in-depth + multi-host deployments
motivate TLS.

### v1 — cert lifecycle (this stage)

```bash
# One-time setup: generate a self-signed ECDSA P-256 cert
# valid for 5 years. SAN includes 127.0.0.1, ::1, localhost,
# and the brain_id. Persists cert.pem + key.pem under
# <project>/.claude/brain/tls/.
neurogrim secrets tls-cert generate

# Inspect: print the SHA-256 fingerprint operators paste into
# browser trust prompts. Lowercase hex, no separators.
neurogrim secrets tls-cert fingerprint

# Status (JSON): cert+key file presence, fingerprint, ready flag.
neurogrim secrets tls-cert status

# Rotate: back up existing cert/key to .bak files, generate
# fresh. Operators re-pin the new fingerprint in the browser.
neurogrim secrets tls-cert rotate
```

The private key is written `0600` on Unix. On Windows the
default user-profile ACLs on `.claude/brain/tls/key.pem` are
sufficient for single-user adopters; multi-user hosts get the
`SecretBackend` upgrade in v2.

### v2 — HTTPS server binding (this stage)

The cert + key live on disk after v1; v2 wires the runtime so the
dashboard binds an HTTPS listener serving the same router as
HTTP. When `<project>/.claude/brain/tls/{cert,key}.pem` exist,
`neurogrim ui` binds:

- HTTP on `<bind>:<port>` (existing)
- HTTPS on `<bind>:<port + 1>` (new — same app, TLS-wrapped)

```
✦ NeuroGrim Dashboard
  Listening: http://127.0.0.1:8420/
  Listening: https://127.0.0.1:8421/  (S14-S-4.5 v2)
```

When cert files are absent, only HTTP binds — backwards-compat
for adopters who haven't run `tls-cert generate`.

Implementation: `axum-server` + `rustls` (ring crypto provider).
Both servers share state (`AppState`) so SSE channels, the bus,
and the cache are coherent across listeners.

### v3 — UX hardening (this stage)

Closes the secret-management security loop:

- **`GET /api/tls-status`** exposes `{ https_available, https_port,
  fingerprint_sha256 }` so the Secrets page can render the right
  banner without trying to introspect the browser's cert
  acceptance state directly.
- **HTTPS banner on the Secrets page** — defense-in-depth
  fallback when an operator somehow lands on the page over HTTP
  (the auto-redirect added in v4 normally prevents this). Shows
  a "Switch to HTTPS" button with the expected fingerprint.
  When no TLS is configured, a hint points operators at
  `tls-cert generate`.
- **Browser TOFU fingerprint pinning** — first visit on HTTPS
  shows a "Trust this fingerprint" banner with the cert SHA-256.
  Operator compares it to the browser's "View certificate"
  dialog and clicks Trust to pin it in localStorage. Subsequent
  visits compare silently; mismatches surface a warning with
  pinned + current values + a "Clear pin" action for after a
  legitimate `tls-cert rotate`.
- **HTTP listener rejects secret writes** — when both HTTP +
  HTTPS are bound, the HTTP listener returns
  `426 Upgrade Required` for POST/DELETE on
  `/api/brains/:id/secrets/*` (GET stays available so agents
  hitting the API for metadata keep working over HTTP).
  Adopters who haven't run `tls-cert generate` see no
  enforcement — HTTP-only deployments keep working.

### v4 — auto-redirect HTTP → HTTPS for the Secrets page (this stage)

When both HTTP and HTTPS are bound, the HTTP listener now
auto-redirects GET requests on `/brains/<id>/secrets` to the
HTTPS equivalent with a `307 Temporary Redirect`. Removes a
manual click from the operator workflow — they no longer have to
read the "switch to HTTPS" banner and re-type the URL with
port +1.

The redirect targets only the **page route** (`/brains/<id>/secrets`),
not the **API route** (`/api/brains/<id>/secrets/*`). Agents
hitting GET on the API for metadata keep working over HTTP;
writes still get the existing `426 Upgrade Required` from v3.
This separation:

- Keeps agent-driven scripting working without needing to follow
  redirects.
- Keeps the human-facing page UX seamless.
- Matches the security model: HTTP is fine for read-only metadata
  but never for value writes.

`307` rather than `301`: browsers cache `301` permanently, which
would survive a `tls-cert rotate` or a dashboard restart without
HTTPS. `307` re-checks each visit so the redirect status follows
the dashboard's actual config.

### v5 — deferred

- **`tls-cert import <path>`** — load operator-supplied certs
  from a real CA (production deployments fronted by a reverse
  proxy). The bundled cert lifecycle handles the dev-loopback
  case end-to-end.
- **Storing the private key in `SecretBackend`** instead of a
  `0600` file (relevant for multi-user host deployments).

<!-- anchor: single-use-tokens -->
## Single-use tokens

Tokens are tracked in-process by the `BrainServer`'s
`ProxyTokenStore`. Properties:

- **Single-use** — `redeem()` flips a `used` flag; second redeem
  returns `None`.
- **60s TTL** — expired tokens are swept; redeem returns `None`.
- **In-process only** — process restart wipes the registry.
  Outstanding tokens become invalid.
- **Audit-friendly** — `audit_summary()` surfaces metadata
  (token_id, brain, secret_id, scope, used) but never the
  underlying value.

claude-proxy migration (S-4, deferred) wires the redeem path: when
an agent presents `X-Scope-Token`, claude-proxy looks it up,
fetches the underlying secret from the `SecretStore`, and forwards
the upstream call.

## `secrets-readiness` advisory domain

Reads `<root>/.claude/secret-refs.yaml` (the human-authored
manifest of declared secrets) and emits findings for:

- **Missing**: secret declared in manifest but no encrypted-file
  entry on disk. Score docked 10 points each.
- **Rotation overdue**: `updated_at` past the manifest's
  `rotation_days` threshold. Score docked 5 points each.

CMDB extras include:

- `declared_count` — total secrets in manifest
- `present_count` — on-disk
- `missing_count`, `rotation_overdue_count`
- `secrets[]` — per-secret summary (id, present, age_days,
  rotation_days, overdue) — **never the value**

Domain is **advisory** (weight 0.0 in v4.2). Operators promote to
weighted via `neurogrim domain promote secrets-readiness …` once
they trust the signal.

## What's deferred

- **S14-S-4** — claude-proxy migration to `OsNativeBackend` for
  `CLAUDE_PROXY_UPSTREAM_KEY`. Cross-repo (touches
  `D:\Brains\claude-proxy\`); follow-up session.
- **S14-S-4.5** — TLS on secret-management endpoints via `rcgen`
  + browser-side cert pinning. Heavy + UX-bound; follow-up session.
- **S14-S-6** — Settings UI page at `/brains/:id/secrets` for
  operator entry/rotation. Depends on S-3 passphrase entry flow;
  follow-up session.
- **S14-S-7** — audit-log decryption CLI (`audit decrypt`).
  Depends on S-4 encryption-at-rest landing first.

## Discipline (don't bypass)

- **Never `Display`-format a secret.** `SecretValue::Debug` redacts;
  if you need to print it for diagnosis, the bug is in the call
  site (re-frame the diagnostic to use metadata).
- **Drop `Zeroizing<Vec<u8>>` immediately after the upstream call.**
  Holding it across `await` points or in long-lived state defeats
  zeroize.
- **Don't add `Serialize` to `SecretValue` or `EncryptedSecretValue`.**
  At-rest encryption is the backend's concern; in-memory values
  don't cross process boundaries.
- **Test for sentinel-value leaks.** A sentinel like
  `SECRET_LEAK_SENTINEL_DO_NOT_LEAK` injected through the
  pipeline + a CI grep across all logs/errors/responses verifies
  no path formats it.

## See also

- `neurogrim explain methodology` — the conceptual model
- `neurogrim explain queues` — the v4.1 sibling (autonomy gate
  routes secret_fetch through the bus's approvals queue)
- `roadmap/epics/S14-encrypted-secrets.md` — story-level plan
- `crates/neurogrim-secrets/` — the secrets library
- `crates/neurogrim-mcp/src/proxy_tokens.rs` — single-use token store
