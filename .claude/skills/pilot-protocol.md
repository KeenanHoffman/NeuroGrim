# Pilot Protocol

Defines the bidirectional interface language between the pilot agent and any subagent
it spawns. Schemas are defined once in skill manifests, are the authority, and are enforced
hard. Non-conformant responses are retried once, then aborted — no silent degradation.

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

Breaking changes bump the minor version. Both the request and the response carry this
identifier so any consumer can detect version mismatches immediately.

---

## Responsibility Types

Each subagent has exactly one responsibility type. The type defines the required hat,
the shape of the `data` field, and what Brain component the output typically feeds.

| Type | Required Hat | Purpose | Feeds |
|------|-------------|---------|-------|
| `sensory` | none | Collect raw data, emit a score | CMDB directly |
| `analysis` | `engineer` or `reviewer` | Reason over data → findings | recommendations |
| `investigation` | `engineer` | Root cause research → evidence trail | incident-ledger |
| `remediation` | `engineer` | Prepare or execute fixes | proposal-ledger |
| `synthesis` | persona-aligned | Aggregate subagent outputs → unified view | agent output |
| `validation` | `reviewer` | Check conformance/correctness → pass/fail | gate state |

The type is declared in the skill manifest's `responsibility` field. The pilot reads
the manifest to know which type to expect and which hat to request.

---

## Common Request Envelope (Pilot → Subagent)

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
    "persona": "<pilot's current persona, or null>",
    "brain_snapshot": null
  }
}
```

`wear_hat` is a first-class field, not context. It is copied verbatim from the manifest's
`required_hat`. The subagent system prompt opens with `Wear Hat: <wear_hat>`.
`brain_snapshot` may be populated with the current `AgentOutput` from `get_health_score`
when the subagent needs health context without querying the Brain independently.

---

## Open-Form Response Format

Subagents write freely in natural language, shaped by the hat they wear. A reviewer
reading a module writes like a reviewer. An engineer investigating a build failure writes
like an engineer. The structured envelope is embedded after the narrative in a clearly
delimited block — it is the machine interface; the narrative is the hat's primary output.

```
[Hat-shaped narrative — freely written, as long as needed]

The reviewer lens reveals three structural concerns in this module...
[findings, reasoning, context expressed naturally]

<!-- LSP-ENVELOPE:req-abc123 -->
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "analysis",
  "capability": "code-analysis",
  "schema_version": "1",
  "request_id": "req-abc123",
  "status": "ok",
  "worn_hat": "reviewer",
  "data": { ... },
  "symbols": [ ... ],
  "metadata": { "confidence": 0.88, "sources_consulted": [...], "warnings": [] }
}
<!-- /LSP-ENVELOPE:req-abc123 -->
```

**Delimiter collision prevention:** The delimiter embeds the `request_id`, which is chosen
per-invocation by the pilot. The subagent echoes it. Collision with narrative prose is
near-impossible because the `request_id` is unique to this call.

**Narrative has no constraints.** Length, structure, and voice are at the subagent's
discretion, shaped by its hat. The envelope block is required, delimited, and validated
exactly as before. Narrative outside the block is not a violation — it is the point.

---

## Response Envelopes (Subagent → Pilot)

All responses share a common wrapper. The `data` field shape differs by responsibility type.
The wrapper is embedded in the delimited block (see Open-Form Response Format above).

**Common wrapper fields (all types):**

```json
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "<type>",
  "capability": "<capability>",
  "schema_version": "1",
  "request_id": "<same as in request>",
  "status": "ok | partial | error",
  "worn_hat": "<hat name or null>",
  "data": { ... },
  "symbols": [ ... ],
  "metadata": {
    "confidence": 0.0,
    "sources_consulted": [],
    "warnings": []
  }
}
```

`symbols` is the **universal observation layer** — every type emits it. The pilot
can always scan `symbols` without knowing the capability-specific `data` schema.

**Symbol shape:**
```json
{
  "key": "<stable identifier, e.g. src/lib.rs:42>",
  "type": "finding | gate | artifact | resource",
  "status": "ok | issues | stale | dirty | clean | unknown",
  "severity": "critical | high | medium | low | info",
  "message": "<one-line human description>",
  "file": "<path or null>",
  "line": "<integer or null>"
}
```

---

### Type: `sensory`

Role: observe, collect, score. No editorial judgment. `worn_hat` is null.

```json
"data": {
  "score": 82,
  "updated_at": "2026-04-12T10:00:00Z",
  "raw": { "<tool-specific fields>" }
}
```

Required: `score` (0–100), `updated_at` (ISO 8601).
Output is directly writable to a CMDB file.

---

### Type: `analysis`

Role: read code or data, apply the declared hat's lens, produce structured findings.

```json
"data": {
  "findings": [
    {
      "id": "<stable id>",
      "severity": "critical | high | medium | low | info",
      "message": "<description>",
      "file": "<path or null>",
      "line": "<integer or null>",
      "hat_lens": "<hat that shaped this finding>"
    }
  ],
  "score": 78,
  "summary": "<one sentence>",
  "hat_emphasis": "<domain the hat weighted most>"
}
```

Required: `findings`, `score`, `summary`, `hat_emphasis`.

---

### Type: `investigation`

Role: trace a problem backward, collect evidence, identify root cause. Always `engineer` hat.

```json
"data": {
  "incident_id": "<id or description>",
  "root_cause": "<one sentence>",
  "evidence": [
    { "source": "<file or tool>", "observation": "<what was found>" }
  ],
  "contributing_factors": ["<factor 1>"],
  "timeline": [
    { "at": "<ISO 8601>", "event": "<what happened>" }
  ],
  "recommended_next": "<one action>"
}
```

Required: `root_cause`, `evidence`, `recommended_next`.

---

### Type: `remediation`

Role: prepare or execute fixes. Engineer hat. Produces an action log, not opinions.

```json
"data": {
  "actions_taken": [
    { "action": "<description>", "file": "<path or null>", "line": null, "outcome": "success | failed | skipped" }
  ],
  "actions_pending": [],
  "blocked_by": [],
  "verification_command": "<command to verify the fix>",
  "proposal_id": "<id written to proposal-ledger>"
}
```

Required: `actions_taken`, `blocked_by`, `verification_command`.

---

### Type: `synthesis`

Role: receive multiple subagent outputs, produce a unified narrative. Hat is persona-aligned.

```json
"data": {
  "unified_score": 74,
  "confidence": 0.87,
  "narrative": "<2-3 sentences>",
  "priorities": [
    { "rank": 1, "action": "<action>", "impact": "high | medium | low" }
  ],
  "inputs_consumed": ["<capability name>"],
  "hat_persona": "<hat or persona name>"
}
```

Required: `unified_score`, `narrative`, `priorities`, `inputs_consumed`, `hat_persona`.

---

### Type: `validation`

Role: check that something is correct/conformant. Reviewer hat. Binary outcome.

```json
"data": {
  "passed": false,
  "violations": [
    {
      "rule": "<rule name>",
      "severity": "critical | high | medium | low",
      "message": "<description>",
      "location": "<file:path.to.field>",
      "remediation": "<how to fix>"
    }
  ],
  "checked_rules": ["<rule name>"],
  "gate_outcome": "clean | dirty | unknown"
}
```

Required: `passed`, `violations` (may be empty), `checked_rules`, `gate_outcome`.

---

### Error Response

When status is `"error"`, `data` is null:

```json
{
  "status": "error",
  "data": null,
  "symbols": [],
  "metadata": {
    "confidence": 0.0,
    "error": {
      "code": "CAPABILITY_UNAVAILABLE | SCHEMA_MISMATCH | AUTHORIZATION | TIMEOUT",
      "message": "<human-readable explanation>",
      "recoverable": true,
      "suggested_action": "<what to do>"
    }
  }
}
```

---

## Enforcement Model

Schemas are the authority. The pilot enforces this on every received response.
Narrative outside the delimited block is expected and is never retried.

```
receive_subagent_response(raw_text, request_id, required_hat, responsibility):

1. Extract delimited block:
   Find "<!-- LSP-ENVELOPE:{request_id} -->" ... "<!-- /LSP-ENVELOPE:{request_id} -->"
   → not found → RETRY ONCE:
     "Your response must include the LSP envelope block after your narrative:
      <!-- LSP-ENVELOPE:{request_id} -->
      { ... your structured output ... }
      <!-- /LSP-ENVELOPE:{request_id} -->
      Your narrative is fine — just add the block after it."
   → not found again → ABORT. Record MISSING_ENVELOPE in proposal-ledger.

2. parse_json(block_content)
   → fail → RETRY ONCE:
     "The content inside the envelope block is not valid JSON. Raw JSON only — no code fences."
   → fail again → ABORT. Record SCHEMA_MISMATCH in proposal-ledger.

3. Validate required envelope fields:
   protocol, responsibility, capability, schema_version, request_id,
   status (ok|partial|error), worn_hat, data (or null if error), symbols (array), metadata
   → any missing → RETRY ONCE with explicit list of missing fields.
   → fail again → ABORT. Record SCHEMA_MISMATCH.

4. If status == "ok":
   Validate data contains all required fields for this responsibility type (see above).
   → missing → RETRY ONCE: "data is missing required fields: <list>"
   → fail again → ABORT.

5. Validate worn_hat matches required_hat from manifest (null == null for sensory).
   → mismatch → RETRY ONCE: "You must wear the <required_hat> hat. Set worn_hat accordingly."
   → fail again → ABORT.

6. Append outcome to .claude/brain/subagent-outcomes.jsonl.
   Return (narrative_text, validated_envelope).
```

**One retry per violation type.** The retry names exactly what was wrong.
After one retry, the schema wins. ABORT means: set envelope to error, record in
proposal-ledger, surface to pilot for human decision.
There is no "degrade gracefully" path.

**The pilot returns both parts:** narrative to the human reader, envelope to the Brain.

---

## Subagent System Prompt Template

Build this from the skill manifest before spawning. Fill in `<>` placeholders.

```
Wear Hat: <required_hat — or omit this line for sensory type>

You are operating under the LSP Brains agent protocol.
Responsibility type: <responsibility>
Capability: <capability>
Protocol: lsp-brains/agent/1.0

Request:
<paste JSON request envelope>

Response format:
Write freely and naturally about what you found. Express your analysis, reasoning,
and findings in your own voice, shaped by the <required_hat> hat. Your narrative
should reflect how that hat sees the problem — an engineer notices build quality and
test failures; a reviewer notices structural concerns and completeness gaps.

After your narrative, embed your structured output in this delimited block.
Raw JSON only inside the block — no code fences, no markdown:

<!-- LSP-ENVELOPE:<request_id> -->
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "<responsibility>",
  "capability": "<capability>",
  "schema_version": "1",
  "request_id": "<request_id>",
  "status": "ok",
  "worn_hat": "<required_hat or null>",
  "data": { <see output_schema below> },
  "symbols": [
    { "key": "string", "type": "finding|gate|artifact|resource",
      "status": "ok|issues|stale|dirty|clean", "severity": "critical|high|medium|low|info",
      "message": "string", "file": "string or null", "line": "integer or null" }
  ],
  "metadata": {
    "confidence": 0.0,
    "sources_consulted": [],
    "warnings": []
  }
}
<!-- /LSP-ENVELOPE:<request_id> -->

Output schema for data:
<paste output_schema from skill manifest>

Required fields in data: <list from output_schema.required>

If you cannot complete the task, set status "error", data null, and
metadata.error: { "code": "CAPABILITY_UNAVAILABLE|SCHEMA_MISMATCH|AUTHORIZATION",
"message": "...", "recoverable": true|false, "suggested_action": "..." }
```

---

## Capability Discovery

**Static (preferred — zero latency):**

1. `Read .claude/skills/<name>.md` — get the `## Interface Contract` section
2. Parse `responsibility`, `required_hat`, `input_schema`, `output_schema`
3. Build request envelope, construct system prompt, spawn subagent

**Dynamic (at runtime via Brain MCP):**

```
Tool: list_capabilities
Returns: { capabilities: [{ key, responsibility, required_hat, schema_version,
           input_schema, output_schema, feeds_domain }] }
```

Call `list_capabilities` when you need to discover what capabilities are available
without knowing the skill file name in advance.

---

## Skill Manifest Interface Contract Block

Each skill file that describes a subagent capability includes an `## Interface Contract`
section with a YAML block:

```yaml
## Interface Contract

```yaml
protocol: lsp-brains/agent/1.0
responsibility: analysis          # one of: sensory, analysis, investigation, remediation, synthesis, validation
capability: code-analysis
schema_version: "1"
required_hat: reviewer            # hat the subagent MUST wear (null for sensory)

input_schema:
  type: object
  properties:
    target_path: { type: string }
  required: [target_path]

output_schema:
  type: object
  properties:
    findings: { type: array }
    score: { type: integer, minimum: 0, maximum: 100 }
    summary: { type: string }
    hat_emphasis: { type: string }
  required: [findings, score, summary, hat_emphasis]

feeds_domain: code-quality
feeds_cmdb: .claude/code-quality-cmdb.json
\```
```

`required_hat` is mandatory for all non-sensory types. Omitting it on an `analysis`,
`investigation`, `remediation`, or `validation` capability is a registry validation error.

---

## Hat Chain Traceability

```
Manifest:  required_hat: "reviewer"
    ↓ pilot reads manifest
Request:   wear_hat: "reviewer"
    ↓ pilot injects into system prompt: "Wear Hat: reviewer"
Subagent executes wearing reviewer hat
    ↓
Response:  worn_hat: "reviewer"
    ↓ pilot validates worn_hat == required_hat
Findings:  finding.hat_lens: "reviewer"  (for analysis type)
```

Every finding is traceable to the hat that produced it. The pilot validates the hat
chain at convergence: `worn_hat != required_hat` triggers a retry.

---

## Integration Points

| Component | Role |
|---|---|
| `subagent-patterns.md` | Envelope construction + validation wraps Patterns 1–5 |
| `proposal-ledger.json` | Records SCHEMA_MISMATCH events per capability |
| `brain-registry.json` `subagent_capabilities` | Capability registry, lists required_hat |
| Brain MCP `list_capabilities` | Dynamic capability discovery |
| `local-awareness.json` | Populates `context` in request envelopes |

---

## See Also

- `subagent-patterns.md` — coordination patterns (fan-out, convergence, hand-off)
- `archived/lsp-subagent-queries.md` — investigation-type subagent for LSP symbol queries
- `archived/hats.md` — hat system and available hats
- `personas.md` — persona definitions for synthesis-type hat selection
