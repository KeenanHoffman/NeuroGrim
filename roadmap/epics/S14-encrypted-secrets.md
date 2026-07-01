---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Encrypted Secrets — Stage 14

**Stage:** 14
**Release:** v4.2 — "Never Plaintext"
**Status:** PLANNED (drafted 2026-04-29)
**Priority:** Security-critical — closes the threat-model gap that's been documented since claude-proxy MVP
**Goal:** Stand up an OS-native credential storage layer (Windows DPAPI / macOS Keychain / Linux libsecret). Migrate `CLAUDE_PROXY_UPSTREAM_KEY` away from plaintext env. Encrypt audit logs at rest. Provide a generic `SecretStore` that NeuroGrim's MCP `secret_fetch` tool queries through the proxy. Never expose secret values to agents — they get opaque single-use tokens.

**Depends on:**
- S13 (secret-fetch approval flows through the bus's approval queue)
- Existing `secret-refs` sensor (catalogs declared secrets per-project)
- Existing `a2a-token` store (precedent for opt-in SQLite + WAL-mode)
- claude-proxy infrastructure (hash-only token storage, audit allowlist)

**Blocks:**
- S15 (Settings UI's secret-entry forms route through this stage's `SecretStore`)

**Master roadmap:** `roadmap/v4-roadmap.md`

---

## Architectural refinements (2026-04-29 conversation)

After review, "always encrypted in transit" was added as a defense-in-depth requirement. The four-layer model:

| Layer | What it covers | Implementation |
|-------|----------------|----------------|
| **Wire** | TCP between browser/UI and dashboard server | **TLS even on loopback** for secret-management endpoints. Self-signed cert generated at first run via `rcgen`, stored OS-native, pinned in browser localStorage on first acceptance. Other dashboard endpoints stay HTTP for simplicity. |
| **Process boundary** | JSON serialization between client and server | TLS layer above + dashboard server zeroizes the request body buffer immediately after parsing. |
| **In-process** | `SecretValue` lifetime in dashboard / proxy memory | **`SecretValue` holds an encrypted blob in memory by default.** Explicit `decrypt_for_use()` returns a `Zeroizing<&[u8]>` with a short scope. Plaintext window is microseconds, not seconds. Master session key derived from OS credential store at process startup; key itself wrapped in `zeroize::Zeroizing`. |
| **At rest** | OS-native credential store / encrypted file fallback | (Original S14 plan; unchanged.) |

**Net effect:** a memory dump of the running dashboard or proxy yields encrypted blobs, not plaintext secrets. Plaintext only exists for the microseconds during the actual upstream API call.

**Cost:** ~3-4 extra days for TLS cert tooling + in-memory wrapping layer. Browser warns about self-signed cert on first localhost connection — operator clicks once, cert pinned. Production deployments point to a real cert.

**Story changes:**
- **S14-S-1** absorbs the in-memory `EncryptedSecretValue` design.
- **NEW: S14-S-4.5** — TLS on secret-management endpoints (3-4 days).
- "Done When" gains the TLS + in-memory encryption milestones.

---

## Stage 14 Is Done When

**Foundation (shipped):**
- [x] `crates/neurogrim-secrets/` workspace member ships with `SecretBackend` trait + 2 implementations + `EncryptedSecretValue` in-memory wrapper *(S-1; 31 unit tests covering value round-trips, master-key derivation, both backends)*
- [x] `keyring` crate integrated; OS-native works on Windows, macOS, native Linux, and WSL *(S-2 — Windows DPAPI smoke verified; cross-platform smoke documented as operator's responsibility; tests gated by `NEUROGRIM_TEST_OS_NATIVE=1` env to keep CI hermetic)*
- [x] Encrypted file fallback ships (ChaCha20Poly1305 + PBKDF2) for headless / CI scenarios *(S-3; v1 file format documented; 12 tests covering round-trip, wrong-passphrase = `BadPassphrase`, malformed-file detection, salt+nonce rotation per write, list scoped by brain_id)*
- [x] In-memory encryption: `SecretValue` holds encrypted blob; explicit `decrypt_for_use()` returns short-lived `Zeroizing<Vec<u8>>` *(S-1; the public crate API is structured around this)*
- [x] Master session key sourced from OS credential store at process startup; wrapped in `Zeroizing` + overwritten on shutdown *(S-1 + S-2; `MasterSessionKey::load_or_generate(brain_id)` + PBKDF2 fallback for headless)*
- [x] `secret_fetch` MCP tool ships; default autonomy `Approve`; routes through S13 approval queue *(S-5; tool wrapped via the autonomy gate from S13-B-5)*
- [x] Returned tokens are single-use, expire in 60s, can only be passed to claude-proxy *(S-5; `ProxyTokenStore` mint/redeem with single-use guarantee + 60s default TTL + 10 unit tests)*
- [x] `secrets-readiness` advisory domain registered (reads `secret-refs.yaml` + on-disk state) *(S-8; sensor at `crates/neurogrim-sensory/src/secrets_readiness.rs`; 9 tests; CMDB shape documented)*
- [x] 14th explain topic: `neurogrim explain secrets` *(`crates/neurogrim-mcp/data/explain/secrets.md`; covers four-layer model, in-memory encryption, both backends, agent-side surface, single-use tokens, secrets-readiness; methodology_drift TOPICS extended)*

**Deferred (heavy follow-ons):**
- [ ] `claude-proxy` migrates `CLAUDE_PROXY_UPSTREAM_KEY` from env to OS-native lookup with `proxy-cli secret import-from-env` migration helper *(S-4 — cross-repo, touches `D:\Brains\claude-proxy\`)*
- [ ] `claude-proxy` audit logs encrypted at rest with rotating session keys *(S-4)*
- [ ] TLS on secret-management endpoints: self-signed cert via `rcgen` generated at first run, stored OS-native, pinned in browser *(S-4.5; needs browser-side cert-pinning UX)*
- [ ] Secrets management UI page (`/brains/:id/secrets`) ships; values never displayed back *(S-6; depends on S-3 passphrase flow + UI work)*
- [ ] Regression test: known sentinel value injected, greps logs/errors/responses prove never leaked *(deferred until S-4 + S-6 land — needs the full pipeline plumbed end-to-end to test against)*
- [ ] `--allow-mutations` flag from v3.5 split into `--allow-service-lifecycle | --allow-layout-edits | --allow-secret-management` *(S-6 expansion; the per-capability split lands with the secret-management surface)*
- [ ] Threat-model write-up: README + claude-proxy README both updated *(S-4 + S-7 follow-up)*
- [ ] `neurogrim audit decrypt` tooling *(S-7; depends on S-4 encryption-at-rest)*

---

## Stories

### S14-S-1: New `neurogrim-secrets` crate (5 days) — SHIPPED

**What:** New workspace member at `crates/neurogrim-secrets/`.

```rust
pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &SecretKey) -> Result<Option<SecretValue>>;
    fn set(&self, key: &SecretKey, value: SecretValue) -> Result<()>;
    fn delete(&self, key: &SecretKey) -> Result<()>;
    fn list(&self) -> Result<Vec<SecretMetadata>>;  // metadata only, never values
}

pub struct SecretValue(zeroize::Zeroizing<Vec<u8>>);  // overwrites on drop

pub struct SecretStore {
    backend: Box<dyn SecretBackend>,
    // ... per-secret cache with expiration
}
```

**Done when:**
- [ ] Crate workspace member registered
- [ ] Trait + 2 backends + zeroize integration
- [ ] 12+ unit tests covering set/get/delete/list, zeroize-on-drop, missing-key
- [ ] Integration test: round-trip a known value through OS-native, verify it's there, delete, verify gone

### S14-S-2: OS-native credential adapter (4 days) — SHIPPED

**What:** Use the [`keyring` Rust crate](https://crates.io/crates/keyring) (mature, ~10M downloads). Wraps DPAPI / Keychain / libsecret behind a single API.

Service-name convention: `neurogrim-{brain_id}-{secret_id}`. Failure modes documented:
- WSL without seahorse: libsecret unavailable → fall back to `EncryptedFileStore` with `tracing::warn!`
- Container / CI: no credential store → encrypted file fallback
- Headless Linux: same

**Done when:**
- [ ] OS-native adapter complete; manual smoke on Windows + WSL + macOS + native Linux
- [ ] Fallback behavior documented per platform
- [ ] WSL setup doc: `apt install gnome-keyring libsecret-1-0 dbus-x11`

### S14-S-3: Encrypted file fallback (4 days) — SHIPPED

**What:** ChaCha20Poly1305 for content (via `chacha20poly1305` crate); PBKDF2-derived master key (via `pbkdf2` crate); salt + nonce per secret. Master key sourced from operator-provided passphrase (entered into dashboard's secret-entry form once per session; held only in encrypted memory after).

**Format documented:**
```
.claude/brain/secrets/{secret_id}.enc:
  version: 1
  salt: <32 bytes>
  nonce: <12 bytes>
  ciphertext: <variable>
  auth_tag: <16 bytes>
```

**Done when:**
- [ ] Encryption + decryption round-trip + 8 tests
- [ ] Failure path: wrong passphrase returns explicit `BadPassphrase` error (not `InvalidData`)
- [ ] Forward-compat: version field allows future format migration
- [ ] Documentation: format reference + threat-model section

### S14-S-4: claude-proxy migration to OS-native (5 days) — DEFERRED

**What:** Migrate `CLAUDE_PROXY_UPSTREAM_KEY` from env var to OS-native lookup on startup. Provide one-time `proxy-cli secret import-from-env` helper for existing operators. Encrypt audit log entries at rest with rotating session keys (one log file per rotation period; default daily). Update README + threat-model.

**Why this story is in this stage rather than claude-proxy directly:** the secret store crate provides the canonical `SecretBackend`; reusing it keeps both projects aligned. claude-proxy depends on `neurogrim-secrets` after this story.

**Done when:**
- [ ] claude-proxy reads upstream key via `SecretStore` instead of env at startup
- [ ] `proxy-cli secret import-from-env` migrates env-resident keys with operator confirmation
- [ ] Audit log encryption + rotation + session-key management ships
- [ ] `proxy-cli audit decrypt` decrypts old logs for forensic review
- [ ] Cross-project integration test: start proxy, confirm upstream key fetched from OS-native
- [ ] claude-proxy README updates threat-model section

### S14-S-4.5: TLS on secret-management endpoints (3-4 days, post-refinement) — 🟢 v1+v2+v3+v4 SHIPPED (v5 deferred: real-CA cert import + private key in SecretBackend)

**What:** Self-signed cert generated at first dashboard run via the [`rcgen`](https://crates.io/crates/rcgen) crate. Cert + private key stored on disk at `<project>/.claude/brain/tls/{cert,key}.pem` with chmod 0600 on Unix. Browser hits the dashboard's secret-management endpoints over `https://127.0.0.1:<port + 1>/api/brains/:id/secrets/...`; other dashboard endpoints stay on HTTP for simplicity.

**First-run UX:** browser warns about self-signed cert; operator clicks "Advanced → proceed"; cert fingerprint pinned in localStorage via the TOFU "Trust" button. Subsequent visits silent. v4 auto-redirect removes the manual "switch to HTTPS" step entirely.

**Done when:**
- [x] `rcgen` integration; cert generated on first run if missing *(v1 — `neurogrim-secrets/src/tls.rs` + `neurogrim secrets tls-cert generate|fingerprint|status|rotate`)*
- [x] HTTPS server binding via axum-server + rustls *(v2 — `neurogrim-dashboard/src/tls_serve.rs`; HTTP and HTTPS share the router via `try_join`)*
- [x] HTTP listener rejects secret writes with 426 Upgrade Required *(v3 — path-level enforcement via `reject_http_secret_writes` middleware)*
- [x] Browser TOFU fingerprint pinning UX *(v3 — TlsBanner with switch / first-visit / mismatch / no-tls cases)*
- [x] HTTP→HTTPS auto-redirect for the Secrets page *(v4 — `redirect_secrets_page_to_https` middleware; 307 Temporary Redirect on GET `/brains/<id>/secrets`; preserves Host header for hostname mapping; 9 tests cover the predicate + middleware behavior)*
- [x] 6+ tests cover the full surface *(v1: 10 tests in `neurogrim-secrets/src/tls.rs`; v2: 7 backend tests + tls-serve module tests; v3: 16 frontend pure-helper tests + 4 banner integration tests + 7 path-enforcement tests; v4: 9 redirect tests; methodology drift covers explain topic accuracy)*

**v5 (deferred):** `neurogrim secrets tls-cert import <path>` for operator-supplied real-CA certs (production behind a reverse proxy); private key stored via `SecretBackend` instead of a 0600 file (multi-user host deployments). The bundled cert lifecycle covers the dev-loopback case end-to-end without these.

### S14-S-5: `secret_fetch` MCP tool (4 days) — SHIPPED

**What:** New MCP tool `secret_fetch(key: String, scope?: String) -> {proxy_token, expires_at}`. Default autonomy `Approve` (every secret fetch requires explicit operator approval through the S13 approvals queue). Per-secret override allows `Notify` for low-sensitivity secrets (public API endpoints with rate limits but no auth).

Returned token is single-use, expires in 60s, can only be passed to claude-proxy via `X-Scope-Token` header.

**Why opaque tokens:** agents never see real secret values. The proxy holds them; the agent receives an opaque token that authorizes one upstream call.

**Done when:**
- [ ] Tool registered + ts-rs bindings + 8+ tests
- [ ] Approval round-trip: agent calls `secret_fetch` → MCP middleware (S13) routes to approvals queue → operator approves via UI → tool returns proxy token → agent uses token in single API call → token expires
- [ ] Documentation: end-to-end flow diagram in `secrets.md` explain topic

### S14-S-6: UI secret-entry surface (5 days) — DEFERRED

**What:** New page `/brains/:id/secrets` (lives in v3.5 multi-page routing). Lists declared secrets from `secret-refs.yaml` with status: `present | missing | expired | rotated_at <date>`.

"Add" / "Rotate" forms route values through encrypted POST to dashboard server, which writes to `SecretStore` and never persists or logs the plaintext. "Test" button validates the stored secret against its declared use-case (e.g., test API call).

**Critically:** secret values are **never** displayed back. Operator can rotate or delete; cannot read.

**Done when:**
- [ ] Page route + component
- [ ] Entry form encrypts client-side before POST (using ephemeral session key derived from passphrase)
- [ ] Server writes to `SecretStore` and zeroizes the request payload immediately
- [ ] Test action verifies stored secret without exposing it
- [ ] vitest covers the form + state transitions
- [ ] Manual smoke verifies value never appears in browser console, server logs, or dashboard ledger

### S14-S-7: Audit-log decryption tooling (2 days) — DEFERRED

**What:** `neurogrim audit decrypt --key-file <path> [--from <ts>] [--to <ts>]` for incident-response. Key file is OS-native-stored; only operators with credential-store access can decrypt. Output is human-readable JSONL stream.

**Done when:**
- [ ] CLI subcommand + tests
- [ ] Documentation: incident-response runbook in `secrets.md`

### S14-S-8: `secrets-readiness` advisory domain (3 days) — SHIPPED

**What:** New domain registered in NeuroGrim's own + adopter Brain registries. Reads `secret-refs.yaml` + `SecretStore` state; emits findings:

- Declared secrets that aren't present in the store
- Secrets past their `rotation_days` threshold
- Backend mismatch (declared `keychain`, found in `encrypted-file`)
- Secrets fetched by agents in last 24h (cross-references S13 approvals queue history)

**Done when:**
- [ ] Domain registered as advisory (weight 0.0)
- [ ] Sensor implementation in `crates/neurogrim-sensory/src/secrets_readiness.rs`
- [ ] CMDB shape documented
- [ ] 8+ tests covering each finding type

---

## Risks (plan-critic concerns brought forward)

🔴 **Blocking concern: secret leakage via error messages or stack traces.** A panic that includes a secret value would be catastrophic. **Mitigation:**
- Code-review pass during S14 implementation specifically auditing every path that could format secret content into a string
- Integration test injects a known sentinel value (`SECRET_LEAK_SENTINEL_DO_NOT_LEAK`); CI greps all logs/errors/responses for it
- `SecretValue` type does NOT implement `Debug`/`Display`; explicit redaction required
- `tracing` filter strips known secret-id keys from formatted events

🟡 **WSL libsecret unavailability.** Many users run Brains under WSL where `seahorse` isn't installed. Encrypted file fallback exists; document `apt install gnome-keyring libsecret-1-0` in setup; cache unlocked master key in encrypted memory for the session.

🟡 **CI environments have no credential store.** Container deployments need a path that works without DPAPI/Keychain. Encrypted file fallback works; document the CI flow.

🟡 **Passphrase-entry recursion.** Operator types passphrase into UI; UI sends it over local HTTP to dashboard; dashboard derives master key. Concern: keylogger / browser MITM. Mitigation: dashboard binds 127.0.0.1 only; passphrase entry uses HTTPS in production deployments (cert generation TBD); audit-log records when secrets are unlocked but not the passphrase itself.

🔵 **Suggestion: split `--allow-mutations` into per-capability flags.** v3.5's `--allow-mutations` bundles service-lifecycle + layout edits + (now) secret operations. Should be `--allow-service-lifecycle`, `--allow-layout-edits`, `--allow-secret-management`. Ship in S14-S-6 as part of the secret-management surface.

---

## Cross-references

- Master roadmap: `roadmap/v4-roadmap.md`
- Existing claude-proxy: `D:\Brains\claude-proxy\` (README + threat model docs)
- Existing secret-refs sensor: `crates/neurogrim-sensory/src/secret_refs.rs`
- Existing a2a-token store (SQLite precedent): `crates/neurogrim-a2a/src/token_store.rs`
- S13 dependency (approval flow): `roadmap/epics/S13-agent-coordination-bus.md`
- S15 dependency consumer: `roadmap/epics/S15-command-post-ui.md`
- `keyring` crate: https://crates.io/crates/keyring
- `zeroize` crate: https://crates.io/crates/zeroize
- `chacha20poly1305` crate: https://crates.io/crates/chacha20poly1305
