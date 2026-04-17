# External Brain Reference Deployment (S6-DB-5)

This is the **reference deployment** for running Moth(er):Br+AI+n as an A2A peer
in Docker. The container runs the same `motherbrain a2a-serve` binary the
Phase E integration test (`motherbrain/crates/motherbrain-cli/tests/dual_brain_pair.rs`)
exercises — same wire protocol, same scoring pipeline, different process
boundary. Works on any Docker host; not tied to any cloud provider.

The scope is deliberately small: a Dockerfile, a docker-compose file, two
example project roots, and a verification script. "A Dockerfile that works
locally is the same one that works anywhere."

---

## 1. What this is for

You have a Brain engine that produces scorecards (`motherbrain score`, `motherbrain agent`,
etc.) and you want another agent — a parent Brain, a sibling Brain, or a human
tool — to fetch those scorecards over HTTP using the A2A protocol (spec §13).
`motherbrain a2a-serve` is the server that does that; this deployment just
packages it so you can run it anywhere Docker runs, without rebuilding the
host toolchain on every machine.

This is **not** a cloud-specific deployment (no Cloud Run manifests, no k8s
charts, no Terraform). The produced image runs on every Docker-compatible
runtime — if you need cloud specifics, write them downstream.

---

## 2. Build and run (one paste)

```bash
cd /path/to/Moth-er-Br-AI-n

# 1. Build the image (first build: ~3-5 min for dep compile; cached after).
docker build -t motherbrain:dev .

# 2. Run it, mounting the example project root read-only.
docker run --rm -p 127.0.0.1:8421:8421 \
  -v "$(pwd)/motherbrain-local-project:/brain:ro" \
  motherbrain:dev

# 3. From another terminal on the host, fetch the agent card:
curl -s http://127.0.0.1:8421/.well-known/agent-card.json | jq

# 4. Or with the local motherbrain binary:
./motherbrain/target/release/motherbrain a2a-discover http://127.0.0.1:8421/a2a/v1/
./motherbrain/target/release/motherbrain a2a-invoke  http://127.0.0.1:8421/a2a/v1/ \
  --message-type snapshot.requested
```

The `snapshot.requested` invocation returns the same `AgentOutput` JSON that
`motherbrain agent` produces locally — it's the same scoring pipeline, just
wrapped in an A2A envelope.

---

## 3. Dual-brain local pair with docker-compose

The `docker-compose.yml` at the repo root starts two peer containers:

| Container             | Host port | Project root                        | Scores (approx)    |
|-----------------------|-----------|-------------------------------------|--------------------|
| `motherbrain-local`   | `8421`    | `./motherbrain-local-project`       | 85 / 78 / 90       |
| `motherbrain-external`| `8422`    | `./motherbrain-external-project`    | 60 / 72 / 55       |

The two fixtures have different CMDB scores so their `AgentOutput` is visibly
distinguishable — a round-trip that returns the wrong Brain's data is caught
on inspection.

```bash
docker compose up --build    # first run compiles the image
# ... wait for "A2A server starting" logs on both services ...

# In another terminal, invoke each peer from the host:
./motherbrain/target/release/motherbrain a2a-invoke http://127.0.0.1:8421/a2a/v1/ \
  --message-type snapshot.requested
./motherbrain/target/release/motherbrain a2a-invoke http://127.0.0.1:8422/a2a/v1/ \
  --message-type snapshot.requested

docker compose down          # tear down
```

This is the same pattern as `dual_brain_pair.rs` —  two A2A peers, each with
its own project root, reachable via distinct host:port pairs — but the peers
are now isolated in containers rather than running as local subprocesses.

---

## 4. Authentication — read this before exposing a port

**Spec §13.6 mandates `authentication: none` for v2.1.** The Agent Card
advertises an `authentication` field with the value `"none"` and no mutation
path exists in the v2.1 wire contract. This is a deliberate choice: the spec
defers auth to the deployment layer so adopters pick the scheme that matches
their topology (mTLS, bearer tokens, SPIFFE, whatever). The Brain itself
never parses a credential.

**Threat model, stated plainly:** *anyone who can reach the TCP port can
request a scorecard.* A scorecard includes domain scores, correlations,
incident patterns, and top recommendations — a reasonable summary of the
project's current health. Don't expose this on the public internet.

**Acceptable access-control patterns — pick one:**

1. **Docker bridge network isolation** (default for `docker compose`).
   Only containers on the same Docker network can reach each other's service
   names; the host publishes only what's in the `ports:` block. In the
   reference `docker-compose.yml` we bind published ports to `127.0.0.1:*`
   explicitly, so even on a multi-user host the A2A ports are loopback-only.
2. **Host firewall.** `iptables -A INPUT -p tcp --dport 8421 -s 10.0.0.0/8 -j ACCEPT`
   on Linux; Windows Firewall inbound rules on Windows. Reject everything
   that isn't on a known peer network.
3. **VPN or service mesh.** Tailscale, WireGuard, Istio mTLS, Linkerd — any
   of these put the auth step above the A2A wire. The Brain stays `auth: none`;
   the mesh proves the caller is allowed.
4. **Cloud VPC rules.** Once you promote this beyond local Docker, AWS
   security groups, GCP firewall rules, and Azure NSGs all do the same thing
   the local firewall does.

If none of the above are in place, **don't publish the port beyond
`127.0.0.1`.** The `docker-compose.yml` shipped here binds to `127.0.0.1`
explicitly for that reason.

---

## 5. Resource footprint

**Image size:** see build output (`docker images motherbrain:dev --format '{{.Size}}'`).
The runtime image is `debian:bookworm-slim` plus one statically-linked-against-
rustls binary; expect ~100–200 MB depending on whether the layer cache
still holds the `ca-certificates` base.

**RAM at idle:** negligible — the binary idles with a Tokio runtime and no
active connections. A single `snapshot.requested` handler runs
`BrainContext::load` which reads the registry plus one file per weighted
domain (O(domains) disk reads), so peak resident memory during a handler
invocation is bounded by the on-disk size of those files. For the reference
fixtures (3 CMDBs, ~200 bytes each), this is sub-MB.

**CPU:** single-core burst during scoring; idle otherwise. The scoring
pipeline is bounded by file I/O and JSON parsing, not CPU.

---

## 6. Logging and observability

`motherbrain a2a-serve` logs via the `tracing` crate with `tracing-subscriber`
configured from `RUST_LOG` (defaulting to `info`). Everything lands on
stdout/stderr, which Docker captures.

```bash
docker logs motherbrain-local               # tail the local peer
docker logs -f motherbrain-external         # follow the external peer
docker compose logs -f                      # both at once
```

Verbosity tuning is standard `tracing-subscriber`:

```bash
docker run -e RUST_LOG=debug ...            # one-off
```

Per-message log lines include the `message_id` and source `brain_id`, so if
a caller reports a stuck or failed request you can grep the server logs by
the envelope's `message_id`.

---

## 7. What this does NOT do (honest limits)

This is a reference deployment, not a hardened production kit. Known gaps:

- **No TLS.** The container speaks plain HTTP on 8421. For HTTPS, put nginx,
  Traefik, or Caddy in front as a reverse proxy. The A2A wire is all JSON
  over HTTP; any standard HTTP proxy works.
- **No auth beyond network-layer.** Per §4 above. When you need bearer-token
  or mTLS auth, wrap the container in a proxy that does the check *before*
  forwarding to 8421.
- **No restart tuning.** `restart: on-failure` in the compose file is a mild
  safety net only. For real supervision, run the image under Kubernetes,
  Nomad, or systemd with explicit restart backoff + liveness probes.
- **No multi-tenancy.** One container = one project root. Co-tenant isolation
  between multiple Brains belongs to the orchestrator, not this deployment.
- **No horizontal scaling.** The `snapshot.requested` handler re-reads the
  project root on every call (deliberate; see `a2a_serve.rs` docstring), so
  `n` replicas against the same `/brain` mount produce `n` concurrent file
  reads. Harmless for the small CMDBs we ship but revisit before scaling
  to large registries.
- **No metrics endpoint.** Logs only. Adding a Prometheus endpoint is future
  work (flagged in the roadmap).

---

## 8. Moving this image elsewhere

Any Docker-compatible runtime can run this image. Representative targets:

| Runtime          | How |
|------------------|-----|
| Cloud Run (GCP)  | `gcloud run deploy --image motherbrain:dev --port 8421` |
| Fargate (AWS)    | Publish to ECR; reference in an ECS task definition |
| Nomad (HashiCorp)| `docker` task driver with the image tag |
| Kubernetes       | A `Deployment` + `Service`; mount the project root as a ConfigMap or PVC |
| Fly.io           | `fly deploy` with a `fly.toml` pointing at the image |

We deliberately don't ship IaC for any specific target — writing one encourages
the illusion that the other five are unsupported. If you pick a specific
runtime, add its manifest under `deployment/<runtime>/` in your fork; don't
treat the reference image as runtime-specific.

---

## 9. Verifying the image still works

`scripts/verify-external-brain.sh` is a one-shot script that builds the image,
runs it briefly against the committed example project root, polls the
well-known card endpoint, invokes `snapshot.requested`, and asserts the
response has a non-empty `domains` map. Use it to confirm the pipeline after
any Dockerfile or reqwest change:

```bash
./scripts/verify-external-brain.sh
```

Expected exit code is 0. If it reports a failure, read its last line — it
names which step failed (build, readiness, invoke, or teardown) and which
log to inspect.

---

## 10. Related reading

- `motherbrain/crates/motherbrain-cli/src/commands/a2a_serve.rs` — server
  handler source.
- `motherbrain/crates/motherbrain-cli/tests/dual_brain_pair.rs` — Phase E
  dual-brain integration test. This deployment runs the same pattern in
  containers.
- LSP-Brains-SPEC.md §10 (Dual Brain Architecture), §13 (A2A protocol),
  §13.6 (Authentication). Read §13.6 before exposing a port.
