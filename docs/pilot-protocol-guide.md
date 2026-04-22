# Pilot Protocol — Full Guide

Deep reference for the `pilot-protocol/SKILL.md` skill. The skill carries
the decision surface (protocol identifier, responsibility type
table, request envelope shape, response format with the delimited-
block contract, enforcement/retry model, See Also). This guide
carries the depth: per-responsibility-type `data` schemas, error
response shape, full subagent system-prompt template, capability
discovery mechanisms, skill manifest Interface Contract block
example, hat chain traceability, and integration points.

Read the skill first. Come here when authoring a new subagent
capability, debugging a specific data shape, or wiring capability
discovery.

---

## Per-Type `data` Shapes (Response Envelopes)

All responses share the common wrapper in the skill body. The
`data` field shape differs by responsibility type. Every `data`
shape below slots into the wrapper inside the delimited envelope
block.

### Type: `sensory`

Role: observe, collect, score. No editorial judgment. `worn_hat`
is null.

```json
"data": {
  "score": 82,
  "updated_at": "2026-04-12T10:00:00Z",
  "raw": { "<tool-specific fields>" }
}
```

Required: `score` (0–100), `updated_at` (ISO 8601).
Output is directly writable to a CMDB file.

### Type: `analysis`

Role: read code or data, apply the declared hat's lens, produce
structured findings.

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

### Type: `investigation`

Role: trace a problem backward, collect evidence, identify root
cause. Always `engineer` hat.

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

### Type: `remediation`

Role: prepare or execute fixes. Engineer hat. Produces an action
log, not opinions.

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

### Type: `synthesis`

Role: receive multiple subagent outputs, produce a unified
narrative under the pilot's hat.

```json
"data": {
  "unified_score": 74,
  "confidence": 0.87,
  "narrative": "<2-3 sentences>",
  "priorities": [
    { "rank": 1, "action": "<action>", "impact": "high | medium | low" }
  ],
  "inputs_consumed": ["<capability name>"],
  "hat": "<hat name>"
}
```

Required: `unified_score`, `narrative`, `priorities`,
`inputs_consumed`, `hat`.

### Type: `validation`

Role: check that something is correct/conformant. Reviewer hat.
Binary outcome.

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

Required: `passed`, `violations` (may be empty), `checked_rules`,
`gate_outcome`.

### Error Response

When `status` is `"error"`, `data` is null:

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

## Symbol Shape (Universal Observation Layer)

`symbols` is the universal observation layer — every type emits
it. The pilot can always scan `symbols` without knowing the
capability-specific `data` schema.

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

## Subagent System Prompt Template

Build this from the skill manifest before spawning. Fill in `<>`
placeholders.

```
Wear Hat: <required_hat — or omit this line for sensory type>

You are operating under the LSP Brains agent protocol.
Responsibility type: <responsibility>
Capability: <capability>
Protocol: lsp-brains/agent/1.0

Request:
<paste JSON request envelope>

Response format:
Write freely and naturally about what you found. Express your
analysis, reasoning, and findings in your own voice, shaped by
the <required_hat> hat. Your narrative should reflect how that
hat sees the problem — an engineer notices build quality and
test failures; a reviewer notices structural concerns and
completeness gaps.

After your narrative, embed your structured output in this
delimited block. Raw JSON only inside the block — no code
fences, no markdown:

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

1. `Read .claude/skills/<name>.md` — get the `## Interface Contract` section.
2. Parse `responsibility`, `required_hat`, `input_schema`, `output_schema`.
3. Build request envelope, construct system prompt, spawn subagent.

**Dynamic (at runtime via Brain MCP):**

```
Tool: list_capabilities
Returns: {
  capabilities: [
    { key, responsibility, required_hat, schema_version,
      input_schema, output_schema, feeds_domain }
  ]
}
```

Call `list_capabilities` when you need to discover what
capabilities are available without knowing the skill file name
in advance.

---

## Skill Manifest Interface Contract Block

Each skill file that describes a subagent capability includes an
`## Interface Contract` section with a YAML block:

````yaml
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
```
````

`required_hat` is mandatory for all non-sensory types. Omitting
it on an `analysis`, `investigation`, `remediation`, or
`validation` capability is a registry validation error.

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

Every finding is traceable to the hat that produced it. The
pilot validates the hat chain at convergence: `worn_hat !=
required_hat` triggers a retry.

---

## Integration Points

| Component | Role |
|---|---|
| `subagent-patterns/SKILL.md` | Envelope construction + validation wraps Patterns 1–5 |
| `proposal-ledger.json` | Records SCHEMA_MISMATCH events per capability |
| `brain-registry.json` `subagent_capabilities` | Capability registry, lists required_hat |
| Brain MCP `list_capabilities` | Dynamic capability discovery |
| `local-awareness.json` | Populates `context` in request envelopes |

---

## See Also

- `.claude/skills/pilot-protocol.md` — the decision surface (the
  skill body that points here).
- `.claude/skills/subagent-patterns.md` — coordination patterns
  that wrap the envelope protocol.
- `.claude/skills/hats.md` — hat system, hat catalog, and
  synthesis-type hat selection.
- `archived/lsp-subagent-queries.md` — investigation-type
  subagent for LSP symbol queries (historical reference).
