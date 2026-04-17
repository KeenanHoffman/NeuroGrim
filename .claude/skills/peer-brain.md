# Skill: Peer Brain — Running Moth(er):Br+AI+n as an A2A Peer

**When to read this:** You are configuring or running a Moth(er):Br+AI+n instance to
participate in a peer topology — either as a child in fractal composition, or as one
half of a dual-brain pair. For the protocol itself, read `a2a.md` first.

## TL;DR

```bash
# Serve this Brain as an A2A peer on port 8421
motherbrain a2a-serve --port 8421 --project-root .

# Discover another peer's Agent Card
motherbrain a2a-discover https://peer.example.com/a2a/v1/

# Send a one-shot A2A message to a peer
motherbrain a2a-invoke https://peer.example.com/a2a/v1/ \
    --message-type snapshot.requested \
    --payload '{"scope": "score"}'
```

All three subcommands land in Stage 6 (S6-DB-3). Until then, these CLI commands are
aspirational — the plumbing exists in `motherbrain-a2a` (S6-DB-1).

## Configure this Brain as a peer

Two things need to be true:

1. **Agent Card is publishable.** Your `brain-registry.json` needs a `dual_brain` section
   (or `children` entries) and the Brain needs to know its own identity. Minimum config:

   ```json
   {
     "meta": { "schema_version": "2", "description": "...", "updated_by": "hand" },
     "config": {
       "dual_brain": {
         "enabled": true,
         "local_brain_id": "project-alpha-local",
         "peer_endpoint": "https://alpha-external.internal/a2a/v1/",
         "event_transport": { "mode": "a2a" }
       }
     }
   }
   ```

2. **HTTP endpoint is reachable** from peer Brains. A2A is not designed to work through
   NAT punchthrough — use a service mesh, VPN, or public hostname with firewall rules.

## Configure a child Brain (fractal composition)

In the parent's `ecosystem-registry.json` (or the `children` section of
`brain-registry.json`):

```json
{
  "children": {
    "project-alpha": {
      "display_name": "Alpha",
      "a2a_endpoint": "https://alpha.internal/a2a/v1/",
      "interface_version": "1",
      "depends_on": [],
      "weight": 1.0,
      "enabled": true
    },
    "project-legacy": {
      "display_name": "Legacy (subprocess)",
      "brain_path": "/opt/legacy-brain/bin/brain-entry",
      "interface_version": "1",
      "weight": 1.0,
      "enabled": true
    }
  }
}
```

- `a2a_endpoint` present → A2A transport. RECOMMENDED.
- `a2a_endpoint` absent, `brain_path` present → subprocess transport. Conformant but legacy.
- Both present → undefined behavior per spec (ambiguous). Don't.

## Operational workflows

### "The parent Brain can't get a score from this child."

1. `motherbrain a2a-discover <child's a2a_endpoint>` — is the Agent Card reachable?
2. Does the child's `capabilities.accepts` list include `snapshot.requested` or
   `score.updated`?
3. Does the child's `interface_version` match what the parent expects?
4. Check the child's logs for the `message_id` the parent sent — did it arrive?

### "Dual-brain events seem to be fired but the other side never acts."

1. Check idempotency — is the same `message_id` being reused? Server returns cached
   response; looks like success but nothing changes on receiver side.
2. Check message type — is the receiver declaring `accepts` for the type you're sending?
3. Check timestamps — if the receiver deduplicates based on `message_id` + `timestamp`
   and the clock is badly skewed, dedup may be wrong.
4. Fall back to shared-file mode ONLY for debugging — set
   `dual_brain.event_transport.mode = "shared_file"`. This is degraded mode per spec §10.4
   and should not be permanent.

### "I need auth on this peer link."

v2.1 supports only `authentication.scheme = "none"`. Your options:

1. **Network-layer auth** (RECOMMENDED) — VPN, service mesh (Istio, Linkerd), cloud IAM,
   firewall allow-list. Keep A2A endpoints off the public internet.
2. **Wait for a future spec version** — bearer tokens and mutual TLS are planned but not
   scheduled yet (tracked as future work in spec §13.6).

Do NOT invent an auth scheme in the payload or metadata fields — that breaks interop.

## Boundary enforcement (code rule)

In this codebase, these two invariants are CI-enforced:

1. The `motherbrain-a2a` crate MUST NOT import from `rmcp` or `motherbrain-mcp`.
2. The `motherbrain-mcp` crate MUST NOT import from `motherbrain-a2a` or `axum`.

If you find yourself wanting to cross these boundaries, you're probably misusing one of
the protocols. Re-read `a2a.md` and the spec's protocol boundary in §1.1.

## Observability

Every A2A message in/out should log:
- `message_id`, `message_type`, peer `brain_id`, direction (in/out)
- Latency from request to response
- Idempotency hit/miss

Idempotency hit rate >~5% suggests either a badly-configured client retrying aggressively
or a client generating non-unique `message_id`s. Investigate.

## Related skills

- `a2a.md` — protocol reference and message shapes.
- `brain.md` — health scoring and the MCP side of the interface.
- `operational-memory.md` — where peer-exchange events land (score-history, incident-ledger).
- `coherence.md` — cross-domain reasoning that consumes peer signals.

## Related reading

- `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` §10, §13, Appendix G
- `D:\Brains\LSP-Brains\spec\DUAL-BRAIN-DESIGN.md`
- `D:\Brains\Moth-er-Br-AI-n\roadmap\epics\S6-dual-brain-a2a.md`
