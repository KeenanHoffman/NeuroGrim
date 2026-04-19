# Skill: A2A Peer Protocol

**When to read this:** You are about to invoke, implement, or debug peer-Brain
communication. This covers fractal composition (parent↔child) and dual brain
(local↔external). For sensory tools or LLM-facing Brain tools, use the MCP skills
(`archived/brain.md`), not this one.

## Protocol boundary (one-line rule)

- **MCP** — sensory tool invocation + Brain exposure to LLM agents. Tool-call shape.
- **A2A** — Brain-to-Brain peer communication. Peer-agent shape.
- **If the other end is a sensor or an LLM:** MCP. **If the other end is another Brain:** A2A.

See spec §13 (A2A Peer Protocol) and Appendix G (A2A Integration).

## Decide whether A2A is the right answer

Ask these in order:

1. **Am I invoking a peer Brain?** (parent invoking child; local sending to external;
   one Brain asking another Brain for a snapshot.)
   - **Yes** → A2A.
   - **No** → stop; this skill does not apply.
2. **Is the peer reachable via HTTP?**
   - **Yes** → A2A (RECOMMENDED, v2.1+).
   - **No / offline / starter-kit / CI one-shot** → subprocess transport (legacy, conformant
     per spec §9.1). Use `ChildTransport::Subprocess` in `ecosystem.rs`.
3. **Does the peer publish an Agent Card?**
   - **Yes** → fetch it at `{endpoint}/.well-known/agent-card.json`, validate against
     `agent-card-v1.schema.json`, check the `capabilities.accepts` list includes the
     message type you need.
   - **No** → either the peer is non-conformant, or you are about to misuse A2A for a
     non-peer role. Double-check.

## Canonical message types (10)

| message_type | Direction | Payload summary |
|--------------|-----------|-----------------|
| `score.updated` | Either | Full Interface Contract output (spec §6) |
| `gate.changed` | Either | `gate_key`, `old_status`, `new_status`, `blocks`, `run_command` |
| `ecosystem.scored` | External → Local | Interface Contract output with `children[]` |
| `incident.detected` | Either | `pattern_id`, `severity`, `domain_variables`, `recurrence_count` |
| `incident.resolved` | Either | `pattern_id`, `resolved_at`, `resolved_by` |
| `snapshot.requested` | External → Local | `{scope, domain_filter?}` |
| `snapshot.delivered` | Local → External | Response; `reply_to` matches request `message_id` |
| `proposal.created` | Either | Proposal object (spec §12) |
| `proposal.resolved` | Either | `proposal_id`, `pre_score`, `post_score`, `action_types` |
| `config.changed` | Either | `registry_path`, `changed_sections`, `committed_at` |

If you need a message type not on this list, you are inventing new wire vocabulary. Add
it to the spec first; do not emit non-canonical types ad hoc.

## Minimum workflow to invoke a peer

```
1. Resolve Agent Card:
     GET {endpoint}/.well-known/agent-card.json
     Validate vs agent-card-v1.schema.json.

2. Check capabilities:
     Agent Card must declare the message_type you need in capabilities.accepts.
     If not, route to a different peer or fail loudly — do not fall back silently.

3. Construct envelope:
     {
       "schema_version": "1",
       "message_id": "<new UUID v4>",
       "timestamp": "<ISO 8601 UTC>",
       "brain_id": "<this Brain's id>",
       "message_type": "<one of 10>",
       "payload": { ... }
     }
     Validate vs a2a-envelope-v1.schema.json BEFORE sending.

4. POST to {endpoint}/a2a/v1/tasks (or peer's tasks_path from Agent Card):
     Expect 202 Accepted with {"task_id": "..."}.

5. For streaming tasks:
     GET {endpoint}/a2a/v1/tasks/{task_id}/events (SSE).
     Otherwise:
     Poll GET {endpoint}/a2a/v1/tasks/{task_id} until terminal.

6. Validate final envelope vs a2a-envelope-v1.schema.json.
     If payload is an Interface Contract output, also validate vs agent-output-v1.schema.json.

7. If you retry (e.g., transient network error): reuse the SAME message_id.
     Server will return the cached response — idempotency is enforced.
```

## Anti-patterns

- **Don't use MCP to call a peer Brain.** MCP is tool-semantics; peer Brains are agents.
  If you find yourself wrapping a Brain as an MCP tool for another Brain, stop — use A2A.
- **Don't use A2A for sensory tools.** Sensory tools publish CMDBs; they don't need Agent
  Cards or task lifecycles. Use MCP (spec §3.7).
- **Don't silently fall back from A2A to shared-file event-log.** Shared-file transport
  is *degraded mode* per spec §10.4, only for air-gapped environments. Surface the
  failure; don't mask it.
- **Don't invent message types.** If a scenario isn't covered by the 10 canonical types,
  extend the spec first. Ad hoc types defeat interoperability.
- **Don't set `authentication.scheme` to anything but `"none"` in v2.1.** Auth is deferred
  to a future spec version. Gate access at the network layer (VPN, service mesh).

## Debugging checklist

If A2A invocation fails, check in this order:

1. **Agent Card reachable?** `curl {endpoint}/.well-known/agent-card.json` returns 200?
2. **Agent Card valid?** Validates against `agent-card-v1.schema.json`?
3. **Capability declared?** Your `message_type` in `capabilities.accepts` on the peer?
4. **Envelope valid?** Your outgoing envelope validates against `a2a-envelope-v1.schema.json`?
5. **Network path?** Can you reach the peer's `transport.endpoint`? (No firewall, correct
   DNS, correct port.)
6. **Idempotency collision?** If you reused `message_id` unintentionally, server returns
   cached response — looks like the request "succeeded" but nothing happened. Use new UUIDs.
7. **Logs on both ends** — A2A servers should log `message_id` + `message_type` on every
   request.

## Related skills

- `peer-archived/brain.md` — running NeuroGrim as an A2A peer (serve, discover, troubleshoot).
- `archived/brain.md` — Brain scoring and MCP-facing workflows.
- `coherence.md` — how cross-domain signals and incident patterns use A2A messages in
  dual-brain coordination.

## Related reading

- `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` §13 + Appendix G
- `D:\Brains\LSP-Brains\spec\DUAL-BRAIN-DESIGN.md` §5 (A2A Message Vocabulary)
- `D:\Brains\LSP-Brains\spec\METHODOLOGY-EVOLUTION.md` §6 (rationale)
- `D:\Brains\NeuroGrim\roadmap\epics\S6-dual-brain-a2a.md`
