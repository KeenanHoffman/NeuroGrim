# Pilot Protocol

**When to use this skill:** You are about to spawn a subagent, implement a new
subagent-facing skill, or debug a subagent response that isn't conforming.
This skill defines the bidirectional interface language between the pilot agent
and any subagent it spawns — the `lsp-brains/agent/1.0` envelope, the request
shape, the response shape, and the retry-then-abort semantics. Schemas are
defined once in skill manifests, are the authority, and are enforced hard;
non-conformant responses are retried once, then aborted — no silent degradation.

Role: operational · reference

Trigger phrases: "spawn a subagent", "subagent protocol", "agent interface", "what schema",
Domain: brain
Methodology-step: skills
"agent schema", "subagent response format", "agent envelope", "lsp-brains protocol",
"what hat should the subagent wear", "enforce subagent output", "validate subagent response"

---

## Protocol Identifier

```
lsp-brains/agent/1.0
```

Breaking changes bump the minor version. Both the request and the response
carry this identifier so any consumer can detect version mismatches
immediately.

---

## Responsibility Types

Each subagent has exactly one responsibility type. The type defines the
required hat, the shape of the `data` field, and what Brain component the
output typically feeds. Per-type `data` schemas live in
`docs/pilot-protocol-guide.md` § Per-Type `data` Shapes.

| Type | Required Hat | Purpose | Feeds |
|------|-------------|---------|-------|
| `sensory` | none | Collect raw data, emit a score | CMDB directly |
| `analysis` | `engineer` or `reviewer` | Reason over data → findings | recommendations |
| `investigation` | `engineer` | Root cause research → evidence trail | incident-ledger |
| `remediation` | `engineer` | Prepare or execute fixes | proposal-ledger |
| `synthesis` | hat-aligned | Aggregate subagent outputs → unified view | agent output |
| `validation` | `reviewer` | Check conformance/correctness → pass/fail | gate state |

The type is declared in the skill manifest's `responsibility` field. The pilot
reads the manifest to know which type to expect and which hat to request.

---

## Request Envelope (Pilot → Subagent)

The pilot constructs this before spawning the subagent:

```json
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "<type>",
  "capability": "<capability key from manifest>",
  "schema_version": "1",
  "request_id": "<short unique id, e.g. req-abc123>",
  "wear_hat": "<required_hat from manifest, or null for sensory>",
  "parameters": {
    "<key>": "<value conforming to manifest input_schema>"
  },
  "context": {
    "project_root": "<absolute path>",
    "hat": "<pilot's current hat, or null>",
    "brain_snapshot": null
  }
}
```

`wear_hat` is a first-class field, not context. It is copied verbatim from the
manifest's `required_hat`. The subagent system prompt opens with
`Wear Hat: <wear_hat>`. `brain_snapshot` may be populated with the current
`AgentOutput` from `get_health_score` when the subagent needs health context
without querying the Brain independently.

Full system-prompt template: `docs/pilot-protocol-guide.md` § Subagent System
Prompt Template.

---

## Response Format (Subagent → Pilot)

Subagents write freely in natural language, shaped by the hat they wear. The
structured envelope is embedded after the narrative in a clearly delimited
block — it is the machine interface; the narrative is the hat's primary
output.

```
[Hat-shaped narrative — freely written, as long as needed.
 Findings, reasoning, context expressed naturally.]

<!-- LSP-ENVELOPE:req-abc123 -->
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "<type>",
  "capability": "<capability>",
  "schema_version": "1",
  "request_id": "<same as request>",
  "status": "ok | partial | error",
  "worn_hat": "<hat name or null>",
  "data": { <per-type shape — see guide> },
  "symbols": [ <universal observation layer — see guide> ],
  "metadata": {
    "confidence": 0.0,
    "sources_consulted": [],
    "warnings": []
  }
}
<!-- /LSP-ENVELOPE:req-abc123 -->
```

**Delimiter collision prevention:** the delimiter embeds the `request_id`,
which is chosen per-invocation by the pilot. Collision with narrative prose
is near-impossible.

**Narrative has no constraints.** Length, structure, and voice are at the
subagent's discretion, shaped by its hat. The envelope block is required,
delimited, and validated exactly. Narrative outside the block is not a
violation — it is the point.

Per-type `data` shapes (sensory, analysis, investigation, remediation,
synthesis, validation, error) + the symbol shape live in the guide.

---

## Enforcement Model

Schemas are the authority. The pilot enforces this on every received
response. Narrative outside the delimited block is expected and is never
retried.

```
receive_subagent_response(raw_text, request_id, required_hat, responsibility):

1. Extract delimited block. Not found → RETRY ONCE (name the missing
   block), then ABORT. Record MISSING_ENVELOPE in proposal-ledger.
2. parse_json(block_content). Fail → RETRY ONCE ("raw JSON only, no
   code fences"), then ABORT. Record SCHEMA_MISMATCH.
3. Validate required envelope fields: protocol, responsibility,
   capability, schema_version, request_id, status, worn_hat, data,
   symbols, metadata. Any missing → RETRY ONCE with explicit list,
   then ABORT.
4. If status == "ok": validate data contains required fields for the
   responsibility type. Missing → RETRY ONCE, then ABORT.
5. Validate worn_hat == required_hat (null == null for sensory).
   Mismatch → RETRY ONCE, then ABORT.
6. Append outcome to .claude/brain/subagent-outcomes.jsonl.
   Return (narrative_text, validated_envelope).
```

**One retry per violation type.** The retry names exactly what was wrong.
After one retry, the schema wins. ABORT means: set envelope to error,
record in proposal-ledger, surface to pilot for human decision. There is
no "degrade gracefully" path.

**The pilot returns both parts:** narrative to the human reader, envelope
to the Brain.

---

## Why This Matters

This protocol enforces **Fail Fast / Shift Left** at the subagent boundary.
Silent schema degradation is the single biggest source of quality rot in
multi-agent systems: when a subagent returns malformed output and the
parent shrugs and moves on, the error compounds invisibly across
convergence. One-retry-then-abort makes every conformance failure a
surfaced event — recorded in `proposal-ledger.json`, visible to the
operator, and correlated with capability authorship. Schema wins every
time.

---

## See Also

- `docs/pilot-protocol-guide.md` — full reference: per-type `data` schemas
  (sensory, analysis, investigation, remediation, synthesis, validation,
  error), symbol shape, full subagent system prompt template, capability
  discovery (static + dynamic), skill manifest Interface Contract YAML
  example, hat chain traceability, integration points.
- `subagent-patterns.md` — coordination patterns (fan-out, convergence,
  hand-off) that wrap this envelope protocol.
- `hats/SKILL.md` — hat system, hat catalog, and synthesis-type hat selection.
