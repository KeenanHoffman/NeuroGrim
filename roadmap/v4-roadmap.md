---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# v4.x Roadmap — Command Post

**Span:** Stages 12 → 15, releases v4.0 → v4.3, ~7-8 months end-to-end
**Approved on:** 2026-04-29 via four strategic questions (sequencing, test selection depth, secret backend ordering, dashboard shape)
**Posture:** plan-critic worn throughout — adversarial review is baked into every stage rather than appended at the end.

> **Why a roadmap rather than one mega-epic:** the user's original v4.0 ask covered 7 distinct concerns (publish gates, message bus, hard gates, multi-page dashboards, secret management, coverage-aware testing, command-post settings UI). Pre-mortem honest read: that's 6+ months of work bundled into "v4.0" with no path to early value, no contained-blast-radius releases, and no way to course-correct after the first concrete user feedback. We split it.

---

## North-star reframe: from "tool agents use" to "command post for humans + agents"

Through v3.5, NeuroGrim has been **a tool that agents use** — MCP/CLI surfaces are first-class; the dashboard is read-mostly with edit-mode for layout customization. The agent is the active driver; the human observes.

v4.x flips the orientation:
- **Humans drive policy.** Operators set what gets gated, what auto-runs, what's queued for review. Edits go through the dashboard, not text-editor JSON sessions.
- **Agents observe and ask.** Agents propose changes, fetch secrets only via the proxy, and route approval-required actions through the message bus. They cannot bypass gates.
- **Trust is enforced, not advised.** The autonomy block (declared but unenforced through v3.5) becomes load-bearing: `resolve_autonomy()` is wired into MCP dispatch; "approve" levels actually block.

Three foundations make this work, and they have to land in this order:

1. **Pre-publish gates** (S12 / v4.0). Without dogfoodable gates, every other v4.x feature ships unverified. Foundation first.
2. **Message bus + hard-gate enforcement** (S13 / v4.1). Bus is the spine; every other v4.x feature emits or consumes on it. Hard gates close the autonomy gap by piggybacking on the bus's request-response shape.
3. **Encrypted secrets** (S14 / v4.2). Closes the threat-model gap (upstream API key plaintext in process memory; audit logs unencrypted). Routes through the bus for human approval of secret access.
4. **Command post UI** (S15 / v4.3). Operator-driven settings + multi-page dashboards. Edits emit on the bus; agents observe.

---

## Stage / release table

| Stage | Release | Theme | Effort | Strict deps |
|-------|---------|-------|--------|-------------|
| **S12** | **v4.0 — Publish Gates** | Manual + automated pre-publish validation; Playwright E2E smoke; mark slow benchmarks `#[ignore]`; dogfooded by NeuroGrim | ~3-4 weeks | None beyond v3.5 |
| **S13** | **v4.1 — Agent Coordination Bus + Hard Gates** | JSONL queue (default) + optional SQLite; HTTP/SSE pubsub; `resolve_autonomy()` wired into MCP dispatch; UI surfaces approval-required actions | ~6-8 weeks | S12 (publishes use the gates from this point on) |
| **S14** | **v4.2 — Encrypted Secrets** | OS-native credential storage (DPAPI/Keychain/libsecret); claude-proxy upstream key migration; audit-log encryption; `secret.fetch` MCP tool routed through approval bus | ~5-7 weeks | S13 (secret-fetch approval flows through the bus) |
| **S15** | **v4.3 — Command Post UI** | Curated registry editor; built-in Services + Logs + Settings pages; operator-defined custom pages; edits emit on the bus | ~8-10 weeks | S13 (UI edits emit on the bus); S14 (settings UI handles secret entry without exposing values to agents) |

The dependency edges are real, not nice-to-have. **Skipping S13 to start S15 first would force re-architecting the UI's edit pipeline.** Plan-critic verdict: do not parallelize S13/S14/S15 even with multiple operators.

---

## Stage 12 (v4.0) — Publish Gates

**Theme:** Ship without surprise. NeuroGrim's own publish process becomes the proof that gates work; adopters get them as a generic capability.

**Goal:** Replace today's "manual operator review + `methodology_drift` test only" pre-publish posture with a structured gate pipeline that runs (a) fast automated checks, (b) curated Playwright E2E smoke tests for key features, and (c) a manual operator-validation checklist with explicit verification steps per declared feature.

### Architectural anchors (already exist)

- **`deploy_readiness` sensor** (`crates/neurogrim-sensory/src/deploy_readiness.rs`): scores 0–100 based on CI/CD presence, container layer, version control, IaC, env templates. Today: not gated. v4.0: becomes one input.
- **`methodology_drift.rs`**: 4-test integration suite that gates BUNDLED_VERSION coherence. Already runs as part of `cargo test`. v4.0: extend, don't replace.
- **`plan-critic`, `dual-review`, `review-loop` skills**: pull-based adversarial reviews. v4.0: codify their structure into a push-based gate definition format.
- **Append-only ledger pattern** (`disposition.rs`, `calibration_ledger.rs`, `supply_chain_review/ledger.rs`): use for the gate-result ledger.

### Stage 12 stories

**S12-G-1: Slow-benchmark surgery (1 day)**
- Mark `context_overhead.rs` and `phase_15_benchmark.rs` integration tests with `#[ignore]` and `#[cfg(feature = "benchmarks")]` gating.
- Add `cargo test --workspace` baseline runtime to a snapshot file; CI fails if it exceeds 90s without explanation.
- Drops integration suite from 3m38s → ~45s. Enables "run every test before publish" as a viable default.

**S12-G-2: `neurogrim test` quiet wrapper (3 days)**
- Carries over from v3.5.1 backlog. Wraps `cargo test --workspace --all-targets`; suppresses success spam; appends failures to `.claude/brain/test-failures.jsonl`.
- Flags: `--keep-last N`, `--show-only-new`, `--retry-failed`.
- Used by Stage 12's `publish-gate run` as one of its automated steps.

**S12-G-3: Gate definition format (3 days)**
- New file `<brain>/.claude/brain/publish-gates.yaml` declaring gate IDs, descriptions, gate-type (`automated` | `manual` | `e2e`), and per-gate checks/instructions.
- Schema-versioned. Validated by `neurogrim doctor`.
- Example gates ship for NeuroGrim itself: `tests-pass`, `methodology-version-coherent`, `changelog-dated`, `frontend-typecheck-clean`, `dashboard-loads-locally`.

**S12-G-4: `neurogrim publish-gate run` CLI (5 days)**
- Reads `publish-gates.yaml`; executes automated gates in dependency order; emits per-gate findings to `.claude/brain/publish-gate-ledger.jsonl`; surfaces manual gates as a checklist with copy-paste verification steps.
- `--gate <id>` runs a single gate.
- `--mode pre-commit | pre-publish | full` selects which gate sets run.
- Exit code: 0 if all passed; 1 if any failed; 2 if any are pending operator validation.

**S12-G-5: Playwright E2E foundation (4 days)**
- New directory `crates/neurogrim-dashboard/frontend/e2e/` with `playwright.config.ts`. Headless Chromium only.
- Constraint enforced in config: total run time must stay under 3 minutes (test files >30s fail the build).
- Three initial smoke specs:
  - `overview-loads.spec.ts` — Brain loads, identity card renders, score gauge or N/A
  - `federation-page.spec.ts` — peer table renders; clicking a peer doesn't crash (regression for the React #310 we hit during v3.5 polish)
  - `layout-edit.spec.ts` — Customize → add a widget → Save → page reflects change
- `neurogrim test --e2e` runs them.

**S12-G-6: Manual gate UI surface (3 days)**
- When `publish-gate run` encounters a manual gate, it prints a numbered checklist + per-item URL or CLI command for the operator to verify.
- Each checked item logs to the ledger with operator handle (`$NEUROGRIM_OPERATOR`).
- Operator says yes/no per item; the gate aggregates.
- Read-only UI surface in dashboard: a dedicated `/brains/:id/publish-gates` page (this lives here, not in the multi-page S15 work, because it's gate-specific).

**S12-G-7: Self-hosting milestone (2 days)**
- Define the actual gate set NeuroGrim itself uses for v4.x publishes.
- First v4.x publish (v4.0 itself) goes through these gates manually as a validation pass.
- Document in CHANGELOG that v4.0+ publishes require `publish-gate run` to pass.

### Stage 12 is done when

- [ ] `cargo test --workspace` runs in <90s baseline with slow benchmarks `#[ignore]`d
- [ ] `neurogrim test` CLI ships, integrating with publish-gate flow
- [ ] `publish-gates.yaml` schema authored + validated by `doctor`
- [ ] `neurogrim publish-gate run` CLI ships with both automated + manual modes
- [ ] Playwright E2E foundation runs 3 smoke specs in <3 minutes total
- [ ] NeuroGrim's own `publish-gates.yaml` is authored + the v4.0 publish goes through it
- [ ] Documentation: `neurogrim explain publish-gates` topic added (12th explain topic)
- [ ] Adopter walkthrough: how to set up gates in a fresh adopter Brain

### Plan-critic concerns

🟡 **Playwright on Windows can be flaky.** Headless Chromium fonts, antivirus interference, intermittent timeouts. Mitigation: pin Playwright version; use `webkit` fallback if Chromium times out; document the troubleshooting in the explain topic.

🟡 **Manual gates have a "did the operator actually verify?" trust problem.** A bored operator clicks ✓ on everything. Mitigation: ledger entries are timestamped; CHANGELOG references gate IDs; dual-review skill can re-verify on a sample.

🔵 **Suggestion:** the gate-result ledger should be input to a future `gate-coverage` domain (advisory) — measures % of declared gates that have actually run in the last N publishes.

---

## Stage 13 (v4.1) — Agent Coordination Bus + Hard Gates

**Theme:** Pub/sub spine for all v4.x feature traffic. Closes the autonomy-block enforcement gap simultaneously by routing approval-required actions through the bus.

**Goal:** Provide a generic, append-only-by-default queue surface that humans, agents, and dashboards all consume. Wire `resolve_autonomy()` from `crates/neurogrim-core/src/governance.rs:244` into MCP tool dispatch so "approve" levels actually block.

### Architectural anchors (already exist)

- **`resolve_autonomy()`** at `governance.rs:244`: 5-step algorithm with `AutonomyLevel` Ord-derived comparison. Fully tested. **Never called by MCP server.** This is the single most important architectural gap to close in v4.x.
- **Append-only ledger pattern**: invocation, calibration, supply-chain ledgers all share the writer signature. New queue reuses it.
- **HTTP/SSE transport**: claude-proxy + A2A both use it. Bus's pubsub layer adopts the same pattern; no new transport.
- **`a2a-token` SQLite store** (`neurogrim-a2a/src/token_store.rs`): precedent for opt-in SQLite. The bus's optional persistent variant follows the same shape.

### Architecture (in brief)

**Default path (zero new infra):** queues are `.claude/brain/queues/<topic>.jsonl` files. Producers append; consumers `tail -f` or read at offsets. Inspectable via `cat`. Git-trackable when desired.

**Optional persistent path (opt-in):** SQLite-backed queues for high-volume or transactional needs. Configured per-topic in `<brain>/.claude/brain/queue-config.yaml`. Same producer/consumer API; storage backend swaps.

**Optional pubsub path:** HTTP/SSE endpoint at `/api/brains/:id/queues/:topic/events` (server-sent events). Cross-process consumers (other agents, other dashboards) subscribe. Message ordering is per-topic.

**Hard gates:** when an agent calls a tool whose autonomy level resolves to `Approve` or `Blocked`, the dispatcher emits a message on the well-known `_neurogrim/approvals` queue containing `{action_id, action_type, payload, requires_approval_by, blast_radius}`. The dashboard's Approvals widget surfaces it; operator clicks Approve/Deny; dispatcher unblocks. `Blocked` actions are never approvable — they reject deterministically.

### Stage 13 stories

**S13-B-1: queue module in `neurogrim-core` (5 days)**
- New module `crates/neurogrim-core/src/queue.rs` (sync, no I/O dependencies pulled into core).
- `QueueMessage` struct: `id` (uuid), `topic`, `payload` (Value), `produced_at`, `priority` (low/normal/high), `expires_at`?
- `JsonlQueueWriter::append()` mirrors existing ledger writers.
- `JsonlQueueReader` iterator with `since_offset(u64)` for resume-able consumption.
- 10+ unit tests covering append, iterate, tail, malformed-line skip.

**S13-B-2: Bus service in `neurogrim-dashboard` (4 days)**
- New module `crates/neurogrim-dashboard/src/bus.rs` wrapping `JsonlQueueWriter`/`Reader`.
- HTTP endpoints:
  - `POST /api/brains/:id/queues/:topic` — publish (gated by `--allow-mutations`)
  - `GET /api/brains/:id/queues/:topic?since=N&limit=M` — read
  - `GET /api/brains/:id/queues/:topic/events` — SSE pubsub
  - `GET /api/brains/:id/queues` — list configured topics + stats

**S13-B-3: SQLite persistent backend (5 days, opt-in)**
- Trait `QueueBackend` in `neurogrim-core`. Implementations: `JsonlBackend` (default), `SqliteBackend`.
- `SqliteBackend` reuses the `a2a-token` store's WAL-mode + sqlite pattern.
- Per-topic configuration via `queue-config.yaml`:
  ```yaml
  topics:
    _neurogrim/approvals:
      backend: jsonl
      retention_days: 30
    pc-state/alerts:
      backend: sqlite
      retention_messages: 10000
      ack_required: true
  ```
- `neurogrim queue migrate <topic> <from> <to>` for backend transitions.

**S13-B-4: MCP queue tools (3 days)**
- `queue_publish(topic, payload, priority?, expires_in_ms?)` — agent publishes
- `queue_consume(topic, since_offset, limit)` — agent reads (does NOT mark consumed; offset-based)
- `queue_peek(topic, count)` — read without offset advance
- Each tool's autonomy default: `notify` (cheap, low-blast); operator can tighten in registry.

**S13-B-5: Wire `resolve_autonomy()` into MCP dispatch (4 days, the load-bearing one)**
- Middleware in `crates/neurogrim-mcp/src/server.rs` that wraps every tool call.
- Maps tool name → declared `action_type` from `config.autonomy.action_types`. Registry must declare an action_type per MCP tool (defaulted via a new `tool_action_types.yaml` shipped with the explain bundle).
- Calls `resolve_autonomy(action_type, autonomy_config, confidence)`:
  - `Auto` → execute immediately
  - `Notify` → execute, then publish on `_neurogrim/notifications` queue
  - `Approve` → publish on `_neurogrim/approvals` queue, return `pending_approval` response with action_id, agent waits or polls
  - `Blocked` → reject with `{"error":"blocked","reason":"..."}`. Never executes.
- 15+ unit tests covering each level + safety invariants.

**S13-B-6: Approvals UI widget + page (5 days)**
- New widget `approvals-feed` in the v3.5 widget catalog: shows pending approvals with Approve / Deny buttons.
- New page `/brains/:id/approvals` for the full list + history.
- Approving emits on `_neurogrim/approval-resolutions` queue, which the MCP dispatcher's pending-approval poll consumes.

**S13-B-7: CLI inspection (`neurogrim queue ...`) (3 days)**
- `neurogrim queue list` — list configured topics
- `neurogrim queue tail <topic> [--follow]` — tail messages
- `neurogrim queue publish <topic> <payload>` — manual produce (operator-only flow)
- `neurogrim queue stats <topic>` — message rate, oldest pending, retention status

**S13-B-8: Documentation (3 days)**
- New explain topic: `neurogrim explain queues` (13th topic).
- Hard-gate flow diagram in `dashboard-layouts.md`.
- Migration guide: how an adopter Brain wires action_types to the new dispatch middleware.

### Stage 13 is done when

- [ ] `queue.rs` module + 10 tests + JSONL writer/reader green
- [ ] Bus HTTP endpoints + SSE pubsub work end-to-end
- [ ] SQLite backend optional + migration CLI shipped
- [ ] 3 new MCP queue tools + agent can publish/consume
- [ ] Autonomy enforcement wired: `Approve`-level actions actually block until operator resolves
- [ ] `Blocked`-level actions deterministically reject; cannot be circumvented
- [ ] Approvals UI widget renders pending requests with Approve/Deny
- [ ] `neurogrim queue ...` CLI shipped
- [ ] 13th explain topic ships
- [ ] NeuroGrim's own publish gates from S12 are dogfood-restructured to use the bus where useful

### Plan-critic concerns

🔴 **Blocking concern: `resolve_autonomy()` in MCP dispatch is a behavior change.** Existing adopter Brains have autonomy blocks declared but no enforcement; suddenly enforcing them means agents that worked yesterday might block today. **Mitigation: ship the dispatch middleware behind an opt-in `--enforce-autonomy` flag for one minor release; document the change loudly; flip the default in v4.2.**

🟡 **Concern: SQLite locking on Windows can be flaky.** Three v4.x stages would suffer if the bus's optional persistent variant crashes the dashboard. Mitigation: WAL-mode (already proven by a2a-token store); document the failure mode; allow per-topic fallback to JSONL.

🟡 **Concern: queue retention without a janitor.** JSONL files grow unbounded. Mitigation: ship `neurogrim queue compact <topic>` that rotates old entries to an archive file (mirrors the `test-failures.archive.jsonl` pattern from v3.5.1 plans).

🟡 **Concern: SSE clients accumulating.** Each browser tab connects forever. Mitigation: cap concurrent SSE connections per Brain; reuse the v3.4 `events.rs` pattern (broadcast::Sender with bounded channel).

🔵 **Suggestion: build a "queue-health" advisory domain in S13.** Reads queue stats; emits findings if any topic has zero consumers AND non-zero producers (silent drop), or if approval queue has pending items >24h. Cheap once the bus exists.

🔵 **Suggestion: the MCP autonomy middleware should also check `safety_invariants` against blast_radius.** Today's `resolve_autonomy()` already does this in step 4; the middleware just plumbs it through.

---

## Stage 14 (v4.2) — Encrypted Secrets

**Theme:** Close the threat-model gap. Upstream API keys never plaintext on disk; secrets fetched only via the proxy; secret operations require approval through the S13 bus.

**Goal:** Stand up an OS-native credential storage layer that claude-proxy migrates to (Windows DPAPI / macOS Keychain / Linux libsecret). Encrypt audit logs at rest. Provide a generic `SecretStore` that NeuroGrim's MCP `secret.fetch` tool queries through the proxy. Never expose secret values to agents — they get opaque tokens scoped to a single use.

### Architectural anchors (already exist)

- **claude-proxy already does hash-only token storage**, audit-log allowlist filtering, and constant-time hash comparison. Solid foundation; gap is encryption at rest + OS-native key sourcing.
- **`a2a-token` store**: precedent for SQLite-backed secret-adjacent storage. v4.2 generalizes it.
- **`secret-refs` sensor**: catalogs documented secrets per-project. v4.2 wires it into the new fetch flow.
- **S13 bus**: secret-fetch operations that need approval emit on `_neurogrim/approvals`. Reuses existing infra.

### Stage 14 stories

**S14-S-1: New `neurogrim-secrets` crate (5 days)**
- New workspace member.
- Trait `SecretBackend` with implementations: `OsCredentialStore` (default), `EncryptedFileStore` (fallback for headless / containerized deployments).
- `SecretStore` struct manages active secret keys; uses `zeroize::Zeroize` to overwrite memory on drop.
- 12+ unit tests; integration test that writes via OS-native, reads via OS-native, deletes.

**S14-S-2: OS-native credential adapter (4 days)**
- Use the [`keyring` crate](https://crates.io/crates/keyring) — wraps DPAPI / Keychain / libsecret behind a single API.
- Service-name convention: `neurogrim-{brain_id}-{secret_id}`.
- Failure modes documented: WSL without seahorse (libsecret unavailable), CI containers (no credential store), headless Linux. Each falls back to `EncryptedFileStore` with a loud warning.

**S14-S-3: Encrypted file fallback (4 days)**
- ChaCha20Poly1305 for content; PBKDF2-derived master key; salt + nonce per secret.
- Master key sourced from operator-provided passphrase (entered once per session into the dashboard's secret-entry form, held only in encrypted memory after).
- Format documented; backward-compat plan for future key rotation.

**S14-S-4: claude-proxy migration to OS-native (5 days)**
- Migrate `CLAUDE_PROXY_UPSTREAM_KEY` from env var to OS-native lookup.
- Provide a one-time `proxy-cli secret import-from-env` migration path for existing operators.
- Audit log encryption: rotating session keys; keys themselves stored in OS-native; one log file per rotation period (default daily).
- Update README + threat-model section.

**S14-S-5: `secret.fetch` MCP tool (4 days)**
- `secret_fetch(key, scope?)` returns an opaque proxy token, NOT the secret value.
- Default autonomy: `Approve` (every secret fetch requires explicit operator approval through the S13 bus).
- Per-secret override allows `Notify` for low-sensitivity secrets (e.g., public API endpoints with rate limits but no auth).
- Returned token is single-use, expires in 60s, can only be passed to claude-proxy.

**S14-S-6: UI secret-entry surface (5 days)**
- New page `/brains/:id/secrets` (lives in the v3.5 multi-page navigation).
- Lists declared secrets (from `secret-refs.yaml`) with status: `present | missing | expired | rotated_at <date>`.
- "Add" / "Rotate" forms route values through encrypted POST to dashboard server, which writes to `SecretStore` and never persists or logs the plaintext.
- "Test" button validates the stored secret against its declared use-case (e.g., test API call with the credential).
- Critically: secret values are **never** displayed back. Operator can rotate or delete; cannot read.

**S14-S-7: Audit-log decryption tooling (2 days)**
- `neurogrim audit decrypt --key-file <path>` for incident-response use.
- Key file is itself OS-native-stored; only operators with credential-store access can decrypt.

**S14-S-8: `secrets-readiness` advisory domain (3 days)**
- Reads `secret-refs.yaml` + `SecretStore` state; emits findings:
  - Declared secrets that aren't present in the store
  - Secrets past `rotation_days` threshold
  - Backend-mismatch (declared `keychain` but found in `encrypted-file`)
- Joins existing `secret-refs` sensor's CMDB output, doesn't replace it.

### Stage 14 is done when

- [ ] `neurogrim-secrets` crate ships, both backends green
- [ ] `keyring` crate integrated; OS-native works on Windows + WSL + macOS + native Linux
- [ ] Encrypted file fallback works headless
- [ ] claude-proxy uses OS-native for `CLAUDE_PROXY_UPSTREAM_KEY`
- [ ] claude-proxy audit logs encrypted at rest
- [ ] `secret.fetch` MCP tool ships, gated by S13 approvals
- [ ] Secrets management UI page (`/brains/:id/secrets`) ships
- [ ] Secret values never visible to agents (regression-tested)
- [ ] `secrets-readiness` advisory domain registered
- [ ] 14th explain topic: `neurogrim explain secrets` ships
- [ ] Threat-model write-up: README + claude-proxy README both updated

### Plan-critic concerns

🟡 **Concern: WSL libsecret unavailability.** Many users run Brains under WSL where `seahorse` isn't installed. Encrypted file fallback exists, but the master-key-passphrase entry is annoying. Mitigation: detect WSL; recommend `apt install gnome-keyring libsecret-1-0` in setup docs; cache the unlocked master key in encrypted memory for the session.

🟡 **Concern: CI environments have no credential store.** Container-based deployments need a path that works without DPAPI/Keychain. Mitigation: encrypted file fallback works; document that CI flow.

🟡 **Concern: passphrase entry through dashboard creates a "where does the passphrase come from" recursion.** Operator types it into the UI; UI sends it over local HTTP to dashboard; dashboard derives master key. Concern: keylogger / browser MITM. Mitigation: dashboard binds 127.0.0.1 only; passphrase entry uses HTTPS in production deployments (cert generation TBD); audit-log records when secrets are unlocked but not the passphrase itself.

🔴 **Blocking concern: secret leakage via error messages or stack traces.** A panic message that includes a secret value would be catastrophic. Mitigation: code-review pass during S14 implementation specifically for any path that could format secret content into a string; integration test that injects a known sentinel value and greps logs/errors for it; document the invariant.

🔵 **Suggestion: post-S14, the v3.5 `--allow-mutations` flag should be split.** "mutations" today bundles service-lifecycle + layout edits + secret operations. Should be `--allow-service-lifecycle`, `--allow-layout-edits`, `--allow-secret-management` so an operator can grant least-privilege scopes.

---

## Stage 15 (v4.3) — Command Post UI

**Theme:** The dashboard becomes the primary editing surface. Operators don't touch JSON files for routine work; they use forms, dropdowns, and curated views. Edits emit on the bus so agents observe.

**Goal:** Ship 3 new built-in pages (Services, Logs, Settings) AND an operator-defined custom-pages system that reuses the v3.4 widget catalog. Settings UI provides curated views of every config file an operator might edit, with validation on save and edit-via-bus emission.

### Architectural anchors (already exist)

- **v3.4 widget catalog + LayoutEditor**: foundation for custom pages.
- **TanStack Router with five built-in pages**: 3-line addition for new pages.
- **MCP tools `domain_new`, `federation_register`, `awareness_add`, `record_subagent_outcome`**: existing surface that the Settings UI wraps as forms.
- **S13 bus**: every UI edit emits on `<brain>/_neurogrim/config-changes` so agents observing the Brain see the diff in real time.
- **S14 SecretStore**: settings UI's secret-entry forms route through it.

### Stage 15 stories

**S15-C-1: Multi-page dashboard infrastructure (5 days)**
- Extend v3.5 widget catalog: a "page" is now a named layout. Brain config has `pages: { overview: [...], services: [...], settings: [...], custom-foo: [...] }`.
- Sidebar navigation auto-populates from declared pages.
- Per-page persistence in `.claude/brain/dashboard-pages.json` (replaces `dashboard-layout.json` with backward-compat read).
- Default pages for fresh Brains: `overview`, `services`, `settings`.

**S15-C-2: Built-in Services page (5 days)**
- v3.5 `PeerActions` extracted into a full page.
- Process list per peer (the v3.5.1 service runtime ledger from `services.jsonl`).
- Per-service log tail (5-second poll OR SSE) — uses the v3.6 backlog item pulled forward.
- Manual re-probe + sensor refresh (also v3.5.1 carry-over).

**S15-C-3: Built-in Logs page (3 days)**
- Reads `services.jsonl`, `invocation-ledger.jsonl`, `score-history.json`, `publish-gate-ledger.jsonl`.
- Filterable timeline view; clickable links into the originating widget.
- Toast notifications for new SSE events while user is on this page.

**S15-C-4: Built-in Settings page — registry editor (8 days, the load-bearing one)**
- Curated forms for each section of `brain-registry.json`:
  - Domain weights: slider per domain (0.0–1.0); preview unified-score impact.
  - Domain definitions: principle text edit; `_todo_<name>` authoring intent.
  - Autonomy: per-action_type level dropdown (Auto/Notify/Approve/Blocked); safety invariants list editor.
  - Hats: declare/remove; multipliers; description editing.
  - Federation children: add/remove peers; the v3.5 `federation rewire` action exposed as a button.
- Schema source: Rust struct → JSON Schema (auto-generate via `schemars` crate, already in workspace deps) → form generator on the frontend.
- Save flow: validate → write atomically → emit `RegistryEdited` event on `_neurogrim/config-changes` queue.
- Conflict detection: if registry changed externally between load and save, surface a 3-way diff.

**S15-C-5: Built-in Settings page — other configs (4 days)**
- `culture.yaml` viewer (read-only — culture changes are a contract, not a setting).
- `secret-refs.yaml` editor (declared secrets only; values via S14 path).
- `publish-gates.yaml` editor (define your own gates from S12).
- `queue-config.yaml` editor (per-topic backend + retention from S13).

**S15-C-6: Operator-defined custom pages (4 days)**
- "Add page" flow: operator names a page, picks an icon, adds widgets via the v3.4 catalog.
- Custom pages persist alongside built-ins; sidebar rendering treats them identically.
- Anchor links extend: `/brains/:id/<page-name>/#widget-<id>` works across pages.

**S15-C-7: Edit-via-bus integration (3 days)**
- Every UI mutation emits on `_neurogrim/config-changes`.
- Standard payload: `{action_type, before, after, operator, timestamp}`.
- Documented as the way for agents to observe operator activity.
- Sample agent: "PC-state pilot" can subscribe to its own Brain's queue to know when the operator changes thresholds or adds domains.

**S15-C-8: Inline help integration (2 days)**
- Each settings field has a `?` icon linking to the relevant `neurogrim explain` topic anchor.
- Anchor linking to specific sections within explain topics: `<brain>/explain/scoring#weighted-mean`.

**S15-C-9: Mobile-responsive breakpoints (3 days)**
- v3.4-v3.5 dashboard works on desktop + tablet; mobile is broken.
- Audit each page against 375px viewport; fix layout overflow; collapsing sidebar at <768px.
- Not aiming for Mobile-First, just "doesn't break."

### Stage 15 is done when

- [ ] Multi-page infrastructure: 3 built-in pages (Overview, Services, Logs, Settings) + custom pages
- [ ] Settings page edits `brain-registry.json` via curated forms; validates on save
- [ ] Edits emit on `_neurogrim/config-changes` queue; agents can observe
- [ ] `culture.yaml` rendered read-only; `secret-refs.yaml` editor uses S14 fetch flow; `publish-gates.yaml` editor wired
- [ ] Operator can add a custom page from the widget gallery
- [ ] Inline help links to explain topics with section anchors
- [ ] Mobile-responsive at 375px; no horizontal scroll on any page
- [ ] 15th explain topic: `neurogrim explain command-post` ships
- [ ] Documentation walks an adopter through their first custom-page authoring

### Plan-critic concerns

🟡 **Concern: schema → form is harder than it looks.** `schemars` produces JSON Schema; turning that into ergonomic forms requires custom UI components per primitive type, anyOf/oneOf handling, and graceful degradation when the schema gets exotic. Mitigation: ship form support for the 80% case (object, array, string, number, boolean, enum); fall back to text-area for complex shapes; document the limitation. Estimated effort doubles if we try to handle every JSON Schema feature.

🟡 **Concern: edit conflicts.** Operator A edits via UI; Operator B edits same registry via text editor; both Save. Last-writer-wins loses data. Mitigation: ETag-style versioning on registry reads; settings page detects conflict and shows 3-way merge UI.

🟡 **Concern: custom-page proliferation.** Operators add pages until the sidebar is unmanageable. Mitigation: limit to 8 custom pages per Brain (configurable); group into folders if more declared.

🔴 **Blocking concern: putting Settings UI on dashboard means dashboard-down = can't edit.** The CLI must remain canonical. Every Settings UI action must map to a documented CLI invocation. We don't deprecate any CLI surface in S15.

🔵 **Suggestion: undo/redo for the last N edits.** Cheap given the bus already records every change. v4.4 work, not S15.

🔵 **Suggestion: a "what changed" audit view per Brain.** Reads `_neurogrim/config-changes` queue; renders a timeline of operator + agent edits. v4.4+ candidate.

---

## Cross-cutting concerns + methodology fit

### Methodology fit

The four stages preserve the LSP Brains methodology's core invariants:
- **Honesty over plausibility:** failures surface (`neurogrim publish-gate run` returns nonzero on real fail; secret-fetch denials are explicit, not silent fallback).
- **Cumulative project awareness:** every stage adds an append-only ledger that other Brains can observe (S12: gate-result; S13: queue messages; S14: audit log; S15: config-changes).
- **Cultural substrate:** culture.yaml stays read-only in S15. Hard gates (S13) provide the enforcement for cultural invariants that previously were aspirational.
- **Fractal composition:** the bus (S13) is per-Brain by default; ecosystem-Brain can subscribe to children's `_neurogrim/notifications` queues for cross-Brain awareness without becoming a centralized broker.

### Backward compatibility commitments

- **CLI remains canonical.** Every UI action maps to a CLI command. S15 doesn't deprecate CLI surfaces.
- **JSON files remain editable.** `dashboard-pages.json`, `brain-registry.json`, etc. all stay text-editable. UI is convenience, not gatekeeper.
- **Existing adopter Brains keep working.** S13's autonomy enforcement ships behind `--enforce-autonomy` for one minor release before flipping default.
- **`dashboard-layout.json` (v3.4) is read-compatible.** S15's `dashboard-pages.json` reads it as the Overview page if present.

### Known SQLite posture (decision: opt-in only)

- v3.4 already has SQLite (a2a-token store). Precedent exists.
- v4.x adds it in two places, both opt-in:
  - S13: queue persistent backend (per-topic configuration)
  - S14: encrypted secret store (encrypted-file fallback uses sqlite-wrapped encryption when keyring unavailable)
- **JSONL remains the default for new infrastructure.** "Everything inspectable as files" stays true for the default path.

### Decisions explicitly deferred (BACKLOG additions)

- **Coverage-aware test selection** — deferred per AskUserQuestion. Mark slow benches, revisit if pain returns.
- **Cloud secret backends (Azure KV, GCP Secret Manager)** — deferred until a real adopter needs cross-machine secret sharing. Pluggable trait shipped in S14, adapters can ship as crates later.
- **Drag-and-drop layout editor** — v3.5 uses ↑/↓ buttons. DnD is v4.4+ if operators ask.
- **Dashboard authentication** — currently 127.0.0.1-only. Multi-user / network-exposed dashboard is a separate stage (S16+).
- **Undo/redo for settings edits** — v4.4 candidate.
- **What-changed audit view** — v4.4+ candidate.
- **Settings field-level RBAC** — beyond v4.x.

---

## Plan-critic verdict (final)

🔴 **Blocking concerns:** None at the roadmap level. Each stage has its own blocking concerns flagged; addressing them is part of the stage work.

🟡 **Major concerns:**
- S13's autonomy enforcement is a behavior change for existing adopters. **Must ship behind a flag for one release before default-on.**
- S14's encrypted-file passphrase entry creates a UX bottleneck. **Mitigated by OS-native default; document the WSL/CI fallback flow loudly.**
- S15's schema → form generator is a real engineering project. **Scope should be capped at the 80% case (basic types); complex shapes get textarea fallback.**

🔵 **Suggestions (cross-cutting):**
- **Build the publish-gate ledger as the canonical "what shipped" log.** Future audit, support, and debugging benefit from a single source of truth for every release.
- **Add a `bus-health` advisory domain in S13** that observes the queue ecosystem.
- **Document the intentional non-goals** in each explain topic so adopters know what NOT to expect.

🟢 **Strengths:**
- Sequencing matches dependency graph; no parallel paths needed.
- Each stage is self-contained value (S12 alone is shippable); release-frequency stays high.
- All four stages reuse existing architectural anchors rather than reinventing.
- Decisions made via AskUserQuestion (split, secret backend, dashboards, test runtime) reduce ambiguity through the rest of v4.x.
- Adversarial review baked into each stage rather than appended.

---

## Per-stage epic files (depth)

- `roadmap/epics/S12-publish-gates.md` — story-level breakdown, dependencies, "Done When" checklist
- `roadmap/epics/S13-agent-coordination-bus.md` — same shape
- `roadmap/epics/S14-encrypted-secrets.md` — same shape
- `roadmap/epics/S15-command-post-ui.md` — same shape

Each epic file follows the existing convention from S7-S10 epic files. Read them before starting the stage; revise them as work reveals reality.

---

## What this roadmap is NOT

- **Not a calendar commitment.** Effort estimates are weeks of focused work, not wall-clock dates.
- **Not a substitute for plan-critic before implementation.** Each stage's epic file should get its own plan-critic pass when work begins.
- **Not the final answer on hard architectural questions.** SQLite posture, encrypted-file format, schema-to-form generator scope — all are committed-with-known-uncertainty. Revise as we learn.

When operators have feedback after using a stage's output (especially S13's bus and S15's command-post UI), revise the next stage in light of what was real vs. what we predicted.
