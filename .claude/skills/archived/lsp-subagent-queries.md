# LSP Subagent Queries

Use this skill when the operator agent needs 3 or more independent domain queries from the
Find-*.ps1 toolkit. Delegates bulk LSP work to ≤5 concurrent subagents, each running
assigned queries and returning structured output. The operator agent synthesizes results and
runs orchestrators (Find-Brain, Find-SessionContext) inline afterward.

Role: operational · reference
Responsibility: investigation
Required hat: engineer
Governs: scripts/dev/Find-GateSymbol.ps1, scripts/dev/Find-ArtifactSymbol.ps1,
         scripts/dev/Find-TopoSymbol.ps1, scripts/dev/Find-WorkflowSymbol.ps1,
         scripts/dev/Find-SkillSymbol.ps1, scripts/dev/Find-TFStateSymbol.ps1,
         scripts/dev/Find-TreeSymbol.ps1, scripts/dev/Find-ShellSymbol.ps1,
         scripts/dev/Find-Symbol.ps1, scripts/dev/Find-TFSymbol.ps1,
         scripts/dev/Find-TSSymbol.ps1, scripts/dev/Find-PySymbol.ps1,
         scripts/dev/Find-SCASymbol.ps1,
         scripts/dev/Find-MermaidSymbol.ps1, scripts/dev/Find-DockerSymbol.ps1,
         scripts/dev/Find-NginxSymbol.ps1, scripts/dev/Find-EaCSymbol.ps1

Trigger phrases: "lsp subagent", "parallelize lsp", "delegate lsp queries",
Domain: brain, artifacts
Methodology-step: skills
"bulk health check", "fan-out lsp", "parallel lsp", "run these find tools concurrently"

---

## Interface Contract

```yaml
protocol: lsp-brains/agent/1.0
responsibility: investigation
capability: lsp-symbol-scan
schema_version: "1"
required_hat: engineer

input_schema:
  type: object
  properties:
    tools:
      type: array
      description: "List of Find-*.ps1 tool names to run (e.g. Find-GateSymbol)"
      items: { type: string }
    project_root:
      type: string
  required: [tools]

output_schema:
  type: object
  properties:
    results:
      type: object
      description: "Keyed by domain/tool name"
      additionalProperties:
        type: object
        properties:
          exit_code: { type: integer }
          output: { type: string }
          symbols:
            type: array
            nullable: true
    root_cause: { type: string, nullable: true }
    evidence:
      type: array
      items:
        type: object
        properties:
          source: { type: string }
          observation: { type: string }
    recommended_next: { type: string }
  required: [results, evidence, recommended_next]

feeds_domain: null
feeds_cmdb: null
```

---

## Delegation Threshold

Subagent spawn overhead is ~5–15s. Delegation only pays when the work being parallelized
exceeds that overhead. Tier A (fast CMDB) tools run in <200ms each — 4 × 200ms = 800ms
serial, still faster than one subagent spawn. Tier C tools (pyright, tsc, npm audit) take
1–10s each and dominate the calculation.

| Condition | Run mode |
|-----------|---------|
| 1–4 queries, all Tier A | **Inline** — serial < spawn overhead |
| Any batch with ≥1 Tier C tool | **Delegate** at 3+ queries — Tier C costs dominate |
| 5+ queries (any mix) | **Delegate** — bulk volume exceeds spawn overhead |
| `Find-Brain.ps1` | Always inline — synthesizer |
| `Find-SessionContext.ps1` | Always inline — synthesizer; run after subagents return |

Note: the `Governs:` field above is intentionally broad (this skill governs when and how
the tools are invoked, not just a single script). `skill-context-on-read.sh` will emit
"no gates govern these scripts" for diagnostic tools that don't appear in test-gates.json —
this is expected and harmless.

---

## Query Speed Tiers

Used for bucketing queries across subagents:

| Tier | Tools | Typical time |
|------|-------|-------------|
| **A — Fast CMDB** | Find-GateSymbol, Find-ArtifactSymbol, Find-TopoSymbol, Find-SkillSymbol, Find-TreeSymbol, Find-TFStateSymbol, Find-WorkflowSymbol, Find-MermaidSymbol, Find-DockerSymbol, Find-NginxSymbol, Find-EaCSymbol | <200ms each |
| **B — Medium** | Find-ShellSymbol, Find-Symbol, Find-TFSymbol | 100ms–1s |
| **C — Slow subprocess** | Find-PySymbol (`-Check` via pyright), Find-TSSymbol (tsc ×3 apps), Find-SCASymbol (npm+pip audit) | 1–10s each |

---

## Bucketing Rules (N queries → ≤5 buckets)

1. Each Tier C query gets its own bucket (never co-locate two Tier C tools unless the
   5-bucket limit forces it)
2. Group Tier A queries: up to 4 per bucket (4 × 200ms < 1s)
3. Tier B tools can share a bucket with each other or with Tier A (max 2 Tier B per bucket)
4. Total buckets ≤ 5
5. If N > 20 Tier A queries: distribute evenly, floor(N/5) per bucket

**Full health check example (11 delegatable tools → 5 buckets):**

| Bucket | Tools | Tier |
|--------|-------|------|
| 1 | Find-GateSymbol + Find-ArtifactSymbol + Find-TFStateSymbol | A + A + A |
| 2 | Find-TopoSymbol + Find-TreeSymbol + Find-WorkflowSymbol | A + A + A |
| 3 | Find-SkillSymbol + Find-ShellSymbol + Find-TFSymbol | A + B + B |
| 4 | Find-Symbol + Find-PySymbol | B + C |
| 5 | Find-TSSymbol + Find-SCASymbol | C + C (sequential inside subagent) |

Bucket 5 co-locates two Tier C tools; total ~20–35s inside one subagent. If either tool
is consistently slow (>20s), split into a 6th bucket dispatched in a second wave.

---

## lsp-reader Prompt Template

Use this template to brief each subagent. Follows the `agent-protocol.md` open-form
format with `responsibility: investigation` and `required_hat: engineer`.

```
Wear Hat: engineer

You are operating under the LSP Brains agent protocol.
Responsibility type: investigation
Capability: lsp-symbol-scan
Protocol: lsp-brains/agent/1.0

Rules:
- Do NOT edit files, run apply scripts, or commit anything.
- Do NOT call Find-Brain.ps1 or Find-SessionContext.ps1.
- Pass -Plain to every command (suppresses ANSI color codes).
- If any command exits non-zero, record the exit code and continue the rest.
- Truncate any single tool output exceeding 2000 characters; append [TRUNCATED].

Run these commands in sequence:
  pwsh -NonInteractive -File scripts/dev/Find-XSymbol.ps1 -Check -Plain
  pwsh -NonInteractive -File scripts/dev/Find-YSymbol.ps1 -Check -Plain

Write freely about what you observed. As an engineer, your narrative should lead with
build quality, test health, and actionable findings. Note anything that looks stale,
broken, or needs immediate attention.

After your narrative, embed the structured envelope in the delimited block below.
Raw JSON only inside the block — no code fences, no markdown:

<!-- LSP-ENVELOPE:<REPLACE with request_id from request> -->
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "investigation",
  "capability": "lsp-symbol-scan",
  "schema_version": "1",
  "request_id": "<REPLACE with request_id from request>",
  "status": "ok",
  "worn_hat": "engineer",
  "data": {
    "results": {
      "x": {
        "exit_code": 0,
        "output": "<full plain-text output — always present>",
        "symbols": [
          {
            "key": "<symbol identifier>",
            "type": "<gate|artifact|binding|resource|skill|workflow|finding>",
            "status": "<clean|dirty|needs-run|stale|fresh|unknown|issues>",
            "severity": "<critical|high|medium|low|info>",
            "message": "<one-line human description>",
            "file": "<path or null>",
            "line": null
          }
        ]
      },
      "y": { "exit_code": 0, "output": "...", "symbols": null }
    },
    "evidence": [
      { "source": "<Find-XSymbol output>", "observation": "<what was notable>" }
    ],
    "recommended_next": "<one action based on findings>"
  },
  "symbols": [
    { "key": "<stable id>", "type": "gate|finding|artifact", "status": "ok|issues|dirty",
      "severity": "critical|high|medium|low|info", "message": "<description>",
      "file": null, "line": null }
  ],
  "metadata": {
    "confidence": 0.9,
    "sources_consulted": ["Find-XSymbol.ps1", "Find-YSymbol.ps1"],
    "warnings": []
  }
}
<!-- /LSP-ENVELOPE:<REPLACE with request_id from request> -->
```

`data.results[key].symbols` is populated when the tool outputs structured data or the
subagent can parse structured output reliably. `symbols: null` means only `output` is
available — the operator reads `output` as fallback. `output` is always the full
plain-text result.

**On non-conformant output:** The operator validates the envelope on receipt. If the
response does not parse as JSON or is missing required fields, the operator retries once
with an explicit correction, then aborts and runs the bucket inline. See `agent-protocol.md`
Enforcement Model.

---

## Convergence Logic

After all subagent results arrive, the operator agent:

1. Checks `"passed"` on each bucket — failed buckets get their tools re-run inline
2. Reads the `"results"` map for each domain's `"output"` field
3. Runs `Find-Brain.ps1 -Mode score -Plain` inline (synthesized score)
4. Runs `Find-SessionContext.ps1 -Action <mode> -Plain` inline if needed

Never abort on partial failure — partial LSP data is better than none.

---

## Why This Matters

The same principle behind Chains 14–16 in `skill-chain.md` applies here: independent
observations should never be serialized when wall-clock time can be compressed. A bulk
health scan across 11 domains runs in ~20s parallel vs ~40–50s serial. This implements
**Fail Fast / Shift Left** from `devops-philosophy.md` at the LSP layer — the agent closes
its observation loop faster without increasing token cost.

---

## See Also

- `subagent-patterns.md` — Pattern 5 (LSP Fan-Out): formal definition with full example
- `lsp-grounded.md` — delegation threshold decision rule (inline vs. delegate)
- `lsp.md` — full Find-*.ps1 command reference
- `personas.md` — lsp-reader persona definition and operational checklist
- `brain.md` — Find-Brain.ps1 as post-convergence synthesizer
