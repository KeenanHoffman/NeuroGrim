---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Dual Brain via A2A

**Stage:** 6
**Status:** Preview — story S6-DB-1 ready to start; others depend on S6-DB-1
**Goal:** The external Brain responds to CI/CD events, Jira tickets, and other external
changes. The architecture designed in S5-TP-7 becomes operational via the A2A Peer
Protocol (spec v2.1 §13 + Appendix G). Local and external Brains coordinate as A2A peers
rather than via a bespoke event protocol.

**Prior name:** "Dual Brain Implementation" — retained in archived historical references.
Renamed 2026-04-17 when A2A was adopted as the normative transport.

**Depends on:**
- S5-TP-7 (dual brain architecture design) — complete
- S5-TP-8 (spec v2.1 publication with A2A schemas) — in progress

---

## Stage 6 Is Done When

- [ ] `neurogrim-a2a` crate exists and passes cargo test
- [ ] `neurogrim-core/src/ecosystem.rs` dispatches on `ChildTransport` (subprocess / A2A)
- [ ] Parent Brain produces identical ecosystem score across both transports
- [ ] `neurogrim a2a-serve` CLI subcommand serves this Brain as an A2A peer
- [ ] Dual brain pair integration test passes in CI
- [ ] External Brain reference deployment documented (one working example)
- [ ] No MCP imports on the dual-brain code path (boundary enforcement)

---

### S6-DB-1: neurogrim-a2a Crate Scaffold

**Status:** **Complete** (2026-04-17)
**Effort:** L
**Depends on:** S5-TP-8 (schemas must exist)

Created the `neurogrim-a2a` Rust crate with the A2A protocol primitives: envelope,
Agent Card, task client, task server, transport abstraction.

**Key decisions (as shipped):**
- Dependencies: `axum 0.7`, `reqwest 0.12`, `tower 0.5`, `uuid 1`, `futures 0.3`, `url 2`,
  `async-trait 0.1` — all from workspace where applicable, no exotic deps
- Agent Card served at `/.well-known/agent-card.json` per spec §13.2
- Task endpoints under `/a2a/v1/tasks` (POST = create, GET by id = poll, `/events` = SSE)
- Envelope types hand-written with `schemars` + `serde` derives
- `Transport` trait is `async_trait`-based with `HttpSseTransport` (real) + `JsonRpcTransport`
  (v1 stub with `todo!()`, explicitly permitted by spec §13.5)
- In-memory idempotency cache via `Arc<RwLock<HashMap<String, A2aEnvelope>>>` on both
  client and server — documented as non-persistent (v1)

**Verification (2026-04-17):**
- Toolchain: Rust 1.95.0 stable-x86_64-pc-windows-gnu, MinGW-w64 (POSIX UCRT) — both on `D:\`
- `cargo check --workspace` — clean (pre-existing `dead_code` warnings only)
- `cargo check -p neurogrim-a2a` — clean
- `cargo test -p neurogrim-a2a` — **19/19 tests pass**:
  - envelope: 3 tests (roundtrip, wire format, all 10 message types)
  - agent_card: 2 tests (minimal roundtrip, auth defaults)
  - client: 3 tests (response validation, idempotency)
  - transport: 5 tests (construction, SSE parsing, URL joining)
  - server: 4 tests (handler lookup, well-known URL, POST+GET roundtrip, idempotency)
  - mock_peer.rs: 1 integration test (full end-to-end)

**Acceptance criteria:**
- [x] `cargo test -p neurogrim-a2a` passes — 19/19 green
- [x] Emitted envelopes validate against `a2a-envelope-v1.schema.json` — conformance test in envelope tests
- [x] Agent Card serializes/validates against `agent-card-v1.schema.json` — agent_card tests
- [x] Mock peer fixture in `tests/mock_peer.rs` exercises client + server round-trip

**FIXMEs left for future polish (not blockers):**
- `transport.rs` SSE parsing is hand-rolled; a production client would use
  `eventsource-stream` or `reqwest-eventsource` (v1 works for single-terminal-event shape)
- `TaskClient::invoke` always polls; streaming-preferred path (when peer's
  `capabilities.streaming = true`) is a mechanical follow-on
- Async task lifecycle: server runs handlers synchronously inside POST; wire contract
  (202 then GET) is honored but long-running tasks aren't actually deferred

---

### S6-DB-2: Ecosystem Refactor to A2A + Subprocess Dispatch

**Status:** **Complete** (2026-04-17)
**Effort:** XL
**Depends on:** S6-DB-1, S4-FC-* (Stage 4 complete)

Implemented fractal composition dispatch across both transports. Shipped as **two
coordinated pieces** to honor the "zero I/O" invariant on `neurogrim-core`:

1. **`neurogrim-core/src/ecosystem.rs`** (~700 lines) — pure types + algorithms:
   - `ChildTransport { Subprocess { brain_path }, A2A { endpoint, agent_card_url? } }`
   - `ChildEntry { id, display_name, transport, interface_version, depends_on, weight, enabled }`
   - `EcosystemRegistry` with custom `Deserialize` that validates exactly one transport selector per child
   - `ChildStatus { Ok, Error, Stale, Disabled }` (§9.5)
   - `topological_sort` — Kahn's algorithm with cycle detection (§9.3)
   - `aggregate` — weighted sum with `freshness_multiplier` reused from `confidence.rs` (§9.4, §4.8)
   - `merge_cross_project_variables` — `child.<project_id>.` prefix per §9.6

2. **`neurogrim-ecosystem`** — NEW async I/O crate:
   - `invoke_child(entry) → Result<AgentOutput, EcosystemError>` — dispatches on transport
   - Subprocess branch: `tokio::process::Command`, parses stdout as `AgentOutput`
   - A2A branch: `neurogrim_a2a::TaskClient` + `snapshot.requested` → `score.updated`
   - `score_ecosystem(parent_output, registry) → EcosystemScore` — full pipeline
   - `EcosystemError` enum (subprocess fail, A2A fail, invalid output, cycle)
   - `examples/stub_child_brain.rs` — cross-platform fixture for contract tests

**Architectural decision:** Keep `neurogrim-core` strictly pure (zero I/O). All dispatch
I/O lives in the new `neurogrim-ecosystem` crate. Deserialization into the Rust
`AgentOutput` type IS the `agent-output-v1` schema validation boundary — surfaced as
`EcosystemError::InvalidOutput` when malformed.

**Verification (2026-04-17):**
- `cargo check --workspace` — clean
- `cargo test --workspace --no-fail-fast` — **159 tests pass, 0 fail**
- `cargo test -p neurogrim-core ecosystem::tests` — 21/21 pass
- `cargo test -p neurogrim-ecosystem` — 2 lib + 4 contract tests = **6/6 pass**
  - `invoke_child_subprocess_and_a2a_return_identical_output` — the conformance claim
  - `score_ecosystem_identical_across_transports` — integer-exact equivalence
  - `two_children_mixed_transports_hand_computed_aggregate` — ecosystem score = 68 (hand-computed)
  - `boundary_no_mcp_imports_in_lib_source` — grep-based invariant
- **Boundary check:** zero `rmcp` / `neurogrim_mcp` imports in `neurogrim-ecosystem` or `neurogrim-core::ecosystem`

**Collateral fix:** three pre-existing `neurogrim-core::registry::tests` tests referenced
`starter-kit/.claude/brain-registry.json` — a path archived on 2026-04-17. Replaced with
inline `THREE_DOMAIN_FIXTURE` constant preserving the original test shape (exactly 3
weighted core domains summing to 1.0, test-health with floor {min=25, cap=50}). Tests
are now self-contained; no filesystem dependency on archived content.

**Acceptance criteria:**
- [x] Parent produces identical ecosystem score with subprocess vs. A2A transport for identical child inputs (contract test)
- [x] `brain-registry-v2.schema.json` validation still passes
- [x] Integration test with 2 synthetic children — one subprocess, one A2A
- [x] Child output validated against `agent-output-v1` schema in both paths
- [x] Zero MCP imports on the ecosystem code path (grep + in-test invariant)
- [x] Subprocess retained as conformant fallback per spec §9.1

**FIXMEs left for S6-DB-3:**
- `invoke_a2a` does NOT currently fetch/validate the child Agent Card (spec §9.7 steps 1–2).
  `TaskClient::discover` exists; wiring it in belongs naturally with S6-DB-3 Agent Card serving.
- `score_ecosystem` dispatches children sequentially. Topologically-aware parallel
  dispatch (children with no dep can run concurrently) is a future optimization.

---

### S6-DB-3: Brain A2A Server (Serve Self as Peer)

**Status:** **Complete** (2026-04-17)
**Effort:** L
**Depends on:** S6-DB-1

Shipped three new CLI subcommands under `neurogrim-cli` plus closed the Phase C
Agent Card discovery FIXME in `neurogrim-ecosystem::invoke_a2a`.

**Files:**
- `crates/neurogrim-cli/src/commands/a2a_serve.rs` (~260 lines) — Agent Card builder from `BrainRegistry` + `TaskServer` wiring + handlers
- `crates/neurogrim-cli/src/commands/a2a_invoke.rs` (~195 lines) — `discover → invoke` one-shot client
- `crates/neurogrim-cli/src/commands/a2a_discover.rs` (~85 lines) — pretty-print Agent Card
- `crates/neurogrim-cli/tests/a2a_cli.rs` (~105 lines) — end-to-end integration test
- `crates/neurogrim-a2a/src/client.rs` — added `discover_at(endpoint, override_url)` alongside existing `discover` (honors registry `agent_card_url` override per Appendix G.3 step 1)
- `crates/neurogrim-ecosystem/src/lib.rs` — closed FIXME: `invoke_a2a` now discovers Agent Card + validates `capabilities.accepts` + validates `interface_version` before POST; new error variants `CapabilityMismatch`, `InterfaceVersionMismatch`
- `crates/neurogrim-ecosystem/tests/contract.rs` — two new discovery-validation tests
- `crates/neurogrim-cli/{Cargo.toml, src/main.rs, src/commands/mod.rs}` — wiring

**Key decisions (as shipped):**
- Agent Card identity: `id` from `registry.meta.updated_by` with UUID fallback; `name` from `registry.meta.description` (80-char truncate); `interface_version: "1"`; `capabilities.accepts = [SnapshotRequested, ScoreUpdated]` (matches registered handlers exactly, no phantom capabilities); `authentication.scheme = "none"` per spec §13.6
- Transport: `http+sse` at `http://127.0.0.1:{port}/a2a/v1/` with tasks at `/tasks`
- `discover` API extended with `discover_at(endpoint, override_url)` rather than replaced — back-compat with existing tests preserved
- Registered handlers: `SnapshotRequested` (see placeholder note below) + `ScoreUpdated` no-op ack (so peers emitting don't 405)

**Verification (2026-04-17):**
- `cargo check --workspace` — clean
- `cargo test --workspace --no-fail-fast` — **171 tests pass, 0 fail** (was 159; +12 new)
- New ecosystem contract tests pass: `invoke_a2a_rejects_peer_missing_snapshot_capability`, `invoke_a2a_rejects_peer_interface_version_mismatch` (plus existing 4 still green)
- CLI integration test passes: `tests/a2a_cli.rs` spins up `TaskServer` in-process, invokes via `TaskClient`, confirms Agent Card + round-trip
- All three subcommands present with valid `--help` output: `a2a-serve`, `a2a-invoke`, `a2a-discover`

**Acceptance criteria:**
- [x] Agent Card served at `/.well-known/agent-card.json` with valid schema
- [x] Peer can request `snapshot.requested` and receive a response envelope
- [x] Peer can request `score.updated` (acknowledged by no-op handler)
- [x] Idempotency: `idempotent_post_returns_cached_without_rehandler` in `neurogrim-a2a` still passes
- [x] **Bonus:** `neurogrim a2a-invoke` and `neurogrim a2a-discover` CLI subcommands exist
- [x] FIXME from S6-DB-2 closed: Agent Card discovery + capability + interface_version validation before A2A invocation

**Placeholder closed (2026-04-17, same day):**
- **`snapshot.requested` handler now runs the real scoring pipeline.** The handler
  calls `BrainContext::load(registry_path, None, None).await` on every invocation,
  which parses the registry, reads CMDBs, builds the scorecard, evaluates
  correlations + incident patterns, computes trajectory, ranks recommendations,
  and produces a full `AgentOutput`. That output is serialized as the
  `snapshot.delivered` payload. Fresh-load per call is deliberate: sensory tools
  update CMDBs between calls, and the cost is acceptable.
- **Live end-to-end verified against the Meta Brain registry** —
  `neurogrim a2a-serve` + `neurogrim a2a-invoke` produces a real scorecard
  with all 8 domain keys (code-quality, coherence, deploy-readiness, git-health,
  human-comms, secret-refs, security-standards, test-health), real unified
  score, populated `correlations_fired`. No `_placeholder` marker anywhere.

**Two URL bugs surfaced and fixed during the end-to-end smoke test:**
- `TaskClient::discover` used relative `Url::join(".well-known/agent-card.json")`
  which appends to the endpoint path when the endpoint ends in `/`. Per RFC 5785,
  well-known URIs sit at the authority root. Fixed with absolute-path join
  (`/.well-known/...`). Regression test: `wellknown_url_ignores_endpoint_path`.
- `HttpSseTransport::{tasks,task,events}_url` had the same bug pattern on
  `.join("a2a/v1/tasks")`, producing doubled-prefix URLs like
  `http://host/a2a/v1/a2a/v1/tasks` that 404'd. Fixed with absolute-path join.
  Regression test: `url_joining_strips_endpoint_path`.

**One validation relaxation:**
- `TaskClient::discover` no longer rejects Agent Cards with empty `capabilities.emits`.
  A Brain that only serves responses (accepts requests, doesn't proactively emit)
  is a legitimate pattern. Forcing such a Brain to lie about emissions it doesn't
  produce would violate `culture.yaml` integrity. The `accepts` array still must
  be non-empty (a Brain that can't receive anything can't participate as a peer).

**Final test counts:** 173 tests pass, 0 fail across workspace (+2 URL regression
tests added during this closure).

---

### S6-DB-4: Dual Brain Pair Integration Test

**Status:** **Complete** (2026-04-17)
**Effort:** M
**Depends on:** S6-DB-2, S6-DB-3

Shipped: end-to-end fractal composition proof — two actual `neurogrim a2a-serve`
subprocesses on loopback, invoked by a parent harness via `score_ecosystem`. This is
the §9.7 request-response direction validated on real wire, not in-process.

**File:** `crates/neurogrim-cli/tests/dual_brain_pair.rs` (~493 lines including helpers)

**Tests added (3):**
1. **`fractal_composition_end_to_end_over_loopback`** — spawns two `neurogrim a2a-serve`
   subprocesses on separately-allocated ephemeral ports, polls each `/.well-known/agent-card.json`
   until 200 OK, builds an `EcosystemRegistry` with two `ChildTransport::A2A` entries,
   calls `score_ecosystem`, asserts both children return `ChildStatus::Ok`, no
   `child_errors`, `ecosystem_score ∈ (0, 100]`, neither subprocess stderr contains
   "panicked at".
2. **`dual_brain_peer_unreachable_is_reported_cleanly`** — one live peer + one dead
   port. Live child succeeds; dead child surfaces as `ChildStatus::Error` with a
   message naming "a2a"/"unreachable"/"connection". Pipeline doesn't crash.
3. **`dual_brain_envelope_validates_against_schema_at_every_hop`** — calls `invoke_child`
   directly; asserts `schema_version == "1"`, `scored_at` parses as RFC 3339 within
   5 min of now, `domains` contains the fixture's `test-health` key, `score ≤ 100`.
   Documents the S6-DB-2 equivalence: Rust deserialization IS schema validation.

**Helper architecture (reusable for S6-DB-5):**
- `find_free_loopback_port()` — ephemeral bind → read port → drop → reuse. Race
  window documented honestly in doc comment.
- `build_minimal_project_root()` — tempdir with 3-domain registry + test-health CMDB stub.
- `spawn_peer_server()` / `ChildGuard` — RAII subprocess lifecycle; `Drop` calls
  `Child::kill()` so test panics don't leak zombies.
- `wait_for_ready()` — 10s poll with 100ms cadence, names the failure on timeout.

**Verification (2026-04-17):**
- `cargo test -p neurogrim-cli --test dual_brain_pair` — **3/3 pass in 2.60s**, three consecutive runs
- `cargo test --workspace` — **176 pass, 0 fail** (baseline 173, +3 new)
- Boundary grep (`rmcp|neurogrim_mcp`) on `dual_brain_pair.rs` — clean

**Acceptance criteria:**
- [x] Round-trip passes in CI (local proxy: 3/3 runs green, stable 2.6s)
- [x] No MCP imports in the dual-brain code path (grep clean)
- [x] Payloads validate against `a2a-envelope-v1.schema.json` at every hop (deserialization
      equivalence documented inline in the schema-validation test)
- [x] Two actual `neurogrim` processes (spawned via `env!("CARGO_BIN_EXE_neurogrim")`)

**Scope honestly deferred (not in this story):**
- **Proactive emission direction** — the §10.4 `score.updated` (push from child) →
  `ecosystem.scored` (push from parent) flow requires server-initiated emission
  infrastructure (timer or hook firing `score.updated` to peers). Our current server
  is request-response only. Flagged as `TODO(S6-DB-5+)` in the test module's top doc.
  What THIS phase proves: the pull direction works end-to-end. Push direction is a
  follow-on feature, not a correctness gap.
- **Cross-host transport** — loopback only for this story; remote-peer testing belongs
  with S6-DB-5 (deployment).

**Proposal surfaced but not implemented (scope discipline):**
- A `--port 0 --print-bound-port` CLI flag would eliminate the ephemeral-bind race
  window entirely by letting the test read the bound port from subprocess stdout
  instead of guessing via pre-bind. Not flaky enough to block ship; worth picking up
  if port collisions ever do flake in CI.

---

### S6-DB-5: External Brain Reference Deployment

**Status:** **Complete** (2026-04-17)
**Effort:** L (scoped down from original XL by targeting local Docker over cloud)
**Depends on:** S6-DB-4

**Scoping decision:** User chose local Docker as the deployment target rather than
Cloud Run / GitHub Actions / a specific cloud provider. Same wire protocol,
Docker-compatible runtimes are all equivalent — the reference pattern ships
faster, verifies completely on the dev machine, and any Docker-compatible runtime
(Cloud Run, Fargate, Fly.io, k8s, Nomad) can run the same image. The deployment
doc points at those options without pretending we ship cloud-specific IaC.

**Files shipped:**
- `Dockerfile` (172 lines) — multi-stage: `rust:1.89-slim-bookworm` builder →
  `debian:bookworm-slim` runtime. Linux target (`x86_64-unknown-linux-gnu`), not
  the Windows-GNU target we use locally. Non-root user (`brain:brain`, UID 1000).
  `EXPOSE 8421`. `ENTRYPOINT ["neurogrim"]` + default `CMD ["a2a-serve", "--port", "8421", "--bind", "0.0.0.0", "--project-root", "/brain"]`.
- `.dockerignore` (66 lines) — excludes `target/`, `.git`, `archive/`, whitepaper,
  sdk-python, starter-kit stub. Build context transferred: 6 KB (source tree is 686 KB).
- `docker-compose.yml` (81 lines) — dual-brain pair: `neurogrim-local` on host
  port 8421, `neurogrim-external` on host port 8422. Same image, different mount
  point. Bound to `127.0.0.1:*` explicitly (not 0.0.0.0 on host) — safe default.
- `docs/EXTERNAL-BRAIN-DEPLOYMENT.md` (240 lines, 10 sections) — §4 "Authentication
  — read this" enumerates network-layer options (Docker bridge, host firewall,
  VPN/mesh, cloud VPC) with an explicit threat model. §7 "What this does NOT do"
  names TLS/auth/multi-tenancy deferrals honestly. §8 lists cloud runtimes without
  claiming we support any specifically.
- `scripts/verify-external-brain.sh` (225 lines) — one-shot POSIX shell verifier:
  build → run → poll `/.well-known/agent-card.json` → invoke `snapshot.requested`
  → tear down. Cleans up on interrupt via `trap EXIT`.
- `neurogrim-local-project/.claude/` + `neurogrim-external-project/.claude/` —
  committed fixture registries + CMDBs so `docker compose up` works out of the box.
  Scores differ between the two (85/78/90 vs 60/72/55) so it's obvious the two
  containers return distinguishable snapshots.

**CLI addition that shipped with this phase** (small, called out honestly):
- `neurogrim a2a-serve --bind <addr>` — default `127.0.0.1` (preserves every
  existing test). Container overrides with `0.0.0.0`. Non-loopback binds log a
  `WARN` citing spec §13.6 and the network-layer auth requirement.

**Reqwest TLS migration** (shipped in this phase):
- Switched `reqwest` in `neurogrim-a2a`, `neurogrim-ecosystem`, `neurogrim-cli`
  from default (native-tls/OpenSSL) to `rustls-tls`. No `libssl-dev` in builder, no
  libssl in runtime image — smaller and pure-Rust TLS. Workspace tests all still pass.

**Verification (2026-04-17):**
- `docker build -t neurogrim:dev .` — **145 MB image** (target <200 MB: PASS)
- First cold build: 4m 6s; incremental rebuild: ~2s
- `docker run neurogrim:dev` + `curl /.well-known/agent-card.json` — real Agent Card served
- `docker compose up` — both containers running, `neurogrim a2a-invoke` against
  each returns distinguishable scores matching each mounted fixture
- `scripts/verify-external-brain.sh` — exit 0
- `cargo test --workspace` — **176 pass, 0 fail** (baseline preserved)
- Boundary check: deployment artifacts don't create a new MCP dependency path;
  MCP references in Dockerfile are the legitimate workspace-member references
  (neurogrim-mcp is part of the workspace and must be buildable)

**Acceptance criteria:**
- [x] Reference deployment doc exists
- [x] Dockerfile builds a working `neurogrim a2a-serve` container
- [x] Worked example: dual-brain pair via `docker compose up` + host `neurogrim a2a-invoke` proves round-trip
- [x] Network auth setup is explicit (§4 enumerates options + names the threat model)

**What broke during Phase F (honest account):**
1. **Stub-binary-shipped bug:** the `cargo build --release` deps-caching trick
   (copy Cargo.toml, build a `fn main(){}` stub, then copy real src) didn't
   invalidate cargo's content fingerprint properly in the workspace setup. The
   first image shipped a 436 KB stub instead of the real CLI — caught only by
   the Dockerfile's sanity-check (`neurogrim --version`) being empty. Fixed
   with `cargo clean --release -p <each-crate>` between the two build stages,
   plus a beefier sanity check that also requires `a2a-serve` in `--help`.
2. **Rust version floor:** prompt suggested `rust:1.83`, but `rmcp 0.8` requires
   Rust ≥ 1.85 (edition2024 stabilized there). Bumped builder to `rust:1.89-slim-bookworm`.
3. **Git Bash path translation:** the verify script's original `curl -o /tmp/file`
   hit MSYS path translation on Windows. Fixed with a cwd-relative `.verify-tmp/` dir.

**Footprint honesty:**
- Image: 145 MB runtime. Fine.
- **Docker's builder cache on C:\ grew to ~3.5 GB during this phase.** C:\ went
  from 6.9 GB free to 2.9 GB free. Not critical, but worth naming. Operators can
  run `docker builder prune` any time to reclaim ~3.5 GB without touching the
  final image. Long-term fix: Docker Desktop → Settings → Resources → Advanced
  → move disk image to D:\.

**Deliberately out-of-scope (named in the deployment doc §7):**
- TLS termination (use reverse proxy — nginx / Traefik / Caddy)
- Bearer / mTLS auth (spec v2.1 is `authentication: none`; §13.6)
- Production restart/supervision tuning
- Multi-tenancy hardening
- Prometheus/metrics endpoint
- Cloud-provider-specific IaC (Cloud Run YAML, GHA workflow, k8s manifests, etc.)

---

### S6-DB-6: (stretch) Python SDK A2A Helper

**Status:** Not started
**Effort:** M
**Depends on:** S6-DB-1

Add `sdk-python/lsp_brains/a2a.py` with a `run_peer()` helper mirroring the existing
`run_server()` helper for sensory tools. Lowest priority in this stage — peer Brains
are more likely to be implemented in Rust/Go/TypeScript than Python.

**Key decisions:**
- Defer until demand appears
- If implemented: thin wrapper around `fastapi` or `starlette` exposing Agent Card +
  task endpoints, same envelope validation as Rust

**Acceptance criteria:**
- [ ] `run_peer(agent_card, handlers)` helper exists in Python SDK
- [ ] Example peer Brain in `sdk-python/examples/peer.py`
- [ ] Contract tests against `a2a-envelope-v1.schema.json`

### S6-DB-7: Ecosystem Brain at Session Root

**Status:** In progress (bootstrap) / Not started (operational A2A wiring)
**Effort:** L
**Depends on:** S5-TP-8 (schemas), S5-TP-9 (culture), S5-TP-10 (LSP-Brains Brain to talk to)

`D:\Brains\` becomes an **ecosystem Brain** that coordinates NeuroGrim (code) and
LSP-Brains (spec) as peers. **Pure A2A** transport — no subagent middle tier (rejected
for added cost without proportional benefit). The ecosystem Brain has its own domains
that measure things only visible at the meeting point of both children; it does not
re-implement child-level concerns. Management role semantically — conductor, not CEO.
Agents are peers.

**Key decisions:**
- **Pure A2A** to children, per spec §9; subprocess retained as conformant fallback
- **Six ecosystem-only advisory domains** — none overlap with child-level domains
- Subagent-transport variant dropped — mechanical aggregation (§9.4) + ecosystem-only
  domains gives more than intelligent middle-tier would have
- Preserved pre-existing `.claude/` content: `settings.local.json` (session-managed),
  `human-comms.yaml` + CMDB, `secret-refs.yaml` + CMDB (these belong to the ecosystem agent)
- Overwrote stale NeuroGrim-copy content: `brain-registry.json` + 5 stale CMDBs

**Six ecosystem-only domains:**

| Domain | What it sees |
|--------|--------------|
| `spec-impl-alignment`   | NeuroGrim conforms to LSP-Brains spec at current version |
| `terminology-coherence` | Key terms (A2A, MCP, hat, culture, peer Brain) used consistently across subprojects |
| `protocol-boundary`     | MCP/A2A separation enforced across both repos — grep-based invariant |
| `north-star-alignment`  | Recent changes in both children advance VISION principles |
| `ecosystem-trajectory`  | Unified velocity across children — only visible at ecosystem level |
| `culture-coherence`     | Three `culture.yaml` copies byte-identical at same version |

**Deliverables:**
- `D:\Brains\.claude\brain-registry.json` — ecosystem registry with both A2A children + 6 domains
- `D:\Brains\.claude\*-cmdb.json` (6 stubs)
- `D:\Brains\.claude\skills\sync-ecosystem.md` — drift check across subprojects
- `D:\Brains\.claude\skills\rubber-duck.md` (copy from S5-TP-9)
- `D:\Brains\.claude\culture.yaml` (copy from S5-TP-9)
- `D:\Brains\CLAUDE.md` — ecosystem agent guide

**Acceptance criteria:**
- [x] `D:\Brains\.claude\brain-registry.json` validates against `brain-registry-v2.schema.json`
- [x] Both children declared as A2A peers with `a2a_endpoint` + `brain_path` fallback
- [x] Six ecosystem-only domains declared (all advisory, weight 0.0)
- [x] Six ecosystem-only CMDB stubs created, validate against `cmdb-envelope-v1.schema.json`
- [x] `sync-ecosystem.md` skill present
- [x] `D:\Brains\CLAUDE.md` present with topology diagram and workflow guidance
- [x] Stale NeuroGrim-copy CMDBs removed; preserved files (settings, human-comms, secret-refs) untouched
- [ ] (Future) `culture-coherence` sensory tool lands — byte-identity check across the three copies
- [ ] (Future) A2A wire-up operational — requires S6-DB-3 (Brain A2A Server) to be in place on both children first

---

## North Star Check

- **Does this make the pattern more general?** Yes — peer Brains communicate via an
  open, standardized protocol (A2A) rather than a bespoke event format. Any Brain
  implementation in any language can participate.
- **Does this make the ecosystem Brain easier?** Yes — parent/child invocation becomes
  "fetch Agent Card, POST task" instead of "shell out to child's entry point."
- **Does this separate methodology from product?** Yes — the spec names A2A; the Rust
  implementation is one of many possible conformant implementations.

## Relationship to Other Stages

- **Stage 4 (Fractal Composition)** — shipped with subprocess child invocation; S6-DB-2
  adds A2A as the RECOMMENDED transport alongside. Subprocess is preserved.
- **Stage 5 (Transferable Practice)** — S5-TP-8 ships the spec + schemas; Stage 6 ships
  the Rust implementation that exercises them.
- **Stage 5 (Adoption)** — S5-TP-3 (external adoption) can proceed in parallel with
  Stage 6; adopters can use subprocess until A2A ships.
