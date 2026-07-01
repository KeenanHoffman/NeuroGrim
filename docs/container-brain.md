---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Running NeuroGrim in containers (optional)

NeuroGrim's day-one usage runs your Brain natively on the host —
`cargo build` + invoke via CLI/MCP. Container deployment and the
companion `claude-proxy` are **opt-in capabilities** for
deployments where they earn their complexity:

- Multi-host A2A peer topologies (one container per Brain).
- Multi-tenant or multi-agent environments where credential
  isolation matters.
- CI/CD environments that prefer a sealed runtime.
- Operators who want to share a Brain endpoint between local + remote
  agents without distributing build toolchains.

**You do not need any of this to use NeuroGrim.** This document
exists so the opt-in posture is explicit + so operators choosing
to enable containers know what's involved.

---

## When to enable containers

Reach for the containerized path when ≥1 of these is true:

1. **You're running NeuroGrim as an A2A peer** that other agents
   reach over HTTP(S). The reference deployment is at
   [`docs/EXTERNAL-BRAIN-DEPLOYMENT.md`](EXTERNAL-BRAIN-DEPLOYMENT.md)
   (S6-DB-5). One Dockerfile + one docker-compose.yml; runs
   anywhere Docker runs.
2. **You're running multiple agents** that each need Anthropic
   API access, and you don't want to ship the real API key into
   each one. Containers get per-scope tokens (`nb_sct_…`); the
   real `x-api-key` lives only on the host. See `claude-proxy`
   below.
3. **Your CI/CD prefers a sealed runtime.** A container with the
   `neurogrim` binary baked in builds once + replicates anywhere.
4. **The `docker-topology` Brain domain matters to you.**
   Containers + an explicit topology are visible to Brain scoring
   when this domain is enabled.

If none of those apply, the native build is simpler. Skip this
document and stick with the [getting-started](getting-started.md)
flow.

---

## Two related-but-separable opt-ins

NeuroGrim's container story has two pieces. They're related (both
matter for sealed multi-agent deployments) but separable (you can
use either independently).

### 1. The NeuroGrim Dockerfile (S6-DB-5 reference deployment)

Lives at `D:/Brains/NeuroGrim/Dockerfile`. Packages
`neurogrim a2a-serve` so the Brain itself runs as a standalone
HTTP peer.

- **Same binary** as the host's `cargo run -p neurogrim-cli --
  a2a-serve`.
- **Same wire protocol** — A2A spec §13 + Appendix G.
- **Bind-mount your project root** as `/brain` (read-only). The
  scoring pipeline reads CMDBs + writes nothing inside the
  container.
- Reference docs: [`docs/EXTERNAL-BRAIN-DEPLOYMENT.md`](EXTERNAL-BRAIN-DEPLOYMENT.md)
  walks through the build + run + verify cycle.

```bash
docker build -t neurogrim:dev .
docker run -p 8421:8421 \
  -v "$(pwd)/neurogrim-local-project:/brain:ro" \
  neurogrim:dev
neurogrim a2a-discover http://127.0.0.1:8421/a2a/v1/
```

**What this Dockerfile does NOT do** (deliberately):

- No TLS termination. Add a reverse proxy (caddy / nginx / cloud LB)
  for HTTPS.
- No auth beyond network-layer. v2.1 spec mandates `authentication: none`
  by default; adopters MUST gate access at the network layer.
  Bearer-auth is supported (spec v2.2+) but operator-configured.
- Not multi-tenant-hardened. Reference, not production kit.

### 2. The `claude-proxy` (host-side credential mediator)

Lives at `D:/Brains/claude-proxy/`. A small HTTP proxy that:

- Holds the real Anthropic API key on the host (under
  `CLAUDE_PROXY_UPSTREAM_KEY` env var — deliberately NOT
  `ANTHROPIC_API_KEY` to avoid flipping Claude Code CLI from
  Max-subscription billing to pay-per-token).
- Issues per-container scope tokens (`nb_sct_…`) that containers
  send via `X-Scope-Token` header.
- Validates + rate-limits + audits each request. Audit metadata
  only — model, token counts, timestamps. **No prompts, no
  responses on disk.**
- Loopback-only by default. Requests from non-loopback addresses
  are rejected.
- Instant per-token revocation: `proxy-cli revoke <token-id>`
  cuts off one container without touching the host's Claude
  access.

```
┌──────────────────────┐         ┌──────────────────────┐         ┌────────────────────┐
│ Container            │         │ claude-proxy (host)  │         │ api.anthropic.com  │
│                      │  POST   │                      │  POST   │                    │
│  X-Scope-Token: nb_… │────────▶│ validate + rate-limit│────────▶│  x-api-key: <real> │
│                      │         │ + audit              │         │                    │
└──────────────────────┘         └──────────────────────┘         └────────────────────┘
         ▲                                │
         │                                ▼
         │                        ┌───────────────┐
         │                        │ audit.log     │
         └────────────────────────┤ (JSONL,       │
              response             │  no prompts)  │
                                   └───────────────┘
```

Use `claude-proxy` whenever a containerized agent needs Claude
access. Do NOT pass the real Anthropic API key into containers.

Full operator reference: [`claude-proxy/README.md`](../../claude-proxy/README.md).

---

## Should I enable the `docker-topology` Brain domain?

The `docker-topology` domain (registered in
`.claude/brain-registry.json`) scores the Brain on docker/compose
configuration health when present. It's `weight: 0.0` (advisory)
by default and silently scores 100 when there's no Docker
content in the project.

Enable it when:

- Your project IS containerized + you want the Brain to surface
  drift in Dockerfile/compose configuration.
- You're using NeuroGrim's reference deployment + want the
  ecosystem-level Brain to aggregate Docker-related signals.

Skip it when:

- Your project doesn't ship Docker artifacts. The domain is
  always-on-but-quiet; nothing breaks if you leave it
  registered. But the trade-off is that an explicit domain in
  the registry is one more thing operators reading the registry
  see.

---

## Container vs native — operator decision matrix

| Scenario | Native | Container | Both |
|---|---|---|---|
| Local dev, single operator, build natively | ✓ | | |
| CI runs `cargo test` | ✓ | | |
| Score collection on a hosted CI runner | ✓ | possible | |
| Brain published as an A2A peer for remote agents | | ✓ | |
| Multiple agents each making Anthropic API calls | (insecure) | ✓ + `claude-proxy` | |
| Operators who don't want to install Rust toolchain | | ✓ | |
| Reproducible-deployment requirement | | ✓ | |

The defaults bias toward simplicity: **native is the path of
least resistance**. Containers earn their place when one of the
above checkpoints is binding.

---

## Threat-model considerations

When containers DO matter, they sit at a deliberate trust boundary:

- **API-key blast radius.** Without `claude-proxy`, every container
  with the API key has full access. Compromise one, compromise the
  spend. With the proxy, scope-token revocation is instant.
- **Audit trail.** `claude-proxy` writes a JSONL audit log of every
  upstream call (no prompts, no responses). Native usage doesn't
  produce this audit log.
- **Network isolation.** A containerized A2A peer can be reached
  over the loopback interface only (`127.0.0.1` bind), or over a
  Docker-internal network. The reference deployment binds
  loopback by default; deliberate operator action enables broader
  reach.
- **Supply-chain trust** (LSP-Brains v2.6 §16; METHODOLOGY-EVOLUTION
  §15). The container image inherits whatever supply-chain trust
  the Dockerfile pulls in. Layer 1 SCA (`supply-chain-sca`) is
  the operator's check on what the image contains. Run
  `neurogrim sensory supply-chain-sca` BEFORE building production
  images.

---

## What's NOT included

- **Cloud-specific manifests.** The reference deployment
  intentionally avoids Cloud Run / Kubernetes / Terraform. Run
  it on any Docker host; downstream rendering for cloud is the
  adopter's choice.
- **Production hardening.** TLS, auth-beyond-bearer, multi-tenant
  isolation, secret-management beyond the host env var — all
  adopter concerns. The reference is honest about this.
- **A `neurogrim init --with-container` flag.** Considered + deferred
  to a v2 follow-on. Operators who want containers add the
  Dockerfile + compose by hand from the reference deployment;
  that's the supported path at v1.
- **Monitoring / observability sidecar.** Out of scope for v1.
  The container emits structured tracing logs; operators wire
  whatever observability backend they already use.

---

## Cross-references

- [`docs/EXTERNAL-BRAIN-DEPLOYMENT.md`](EXTERNAL-BRAIN-DEPLOYMENT.md)
  — the reference deployment walkthrough (Dockerfile + compose +
  verify).
- [`claude-proxy/README.md`](../../claude-proxy/README.md) —
  operator reference for the credential proxy.
- [`Dockerfile`](../Dockerfile) — the production-shaped image
  build.
- [`docker-compose.yml`](../docker-compose.yml) (if present) —
  example multi-Brain deployment.
- [Spec §13 A2A Peer Protocol](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md)
  + [Appendix G A2A Integration](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md)
  — the wire protocol the containerized Brain serves.
- [`docs/getting-started.md`](getting-started.md) — the native
  flow that doesn't require any of this.
