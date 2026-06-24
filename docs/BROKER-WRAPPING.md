# NeuroGrim — Broker Wrapping

How adopters wrap existing capabilities (MCP tools, NeuroGrim sensors) as brokers — and
why **skills are NOT wrapped** but instead surface as Sense-role Overlay content via the
Context Broker.

Companion to [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) (the named primitive),
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) (the framework internals),
[`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md) (the manifest format the
wrapping paths reference), and [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) (how
wrapped capabilities become agent-visible).

---

## The two wrapping paths (and the one non-wrapping path)

| Substrate | Wrapping path | Why |
|---|---|---|
| **MCP tool** | Pipeline (thick wrapper) | MCP tools have real preconditions + state-parameterizable inputs + outputs that warrant audit anchoring. Brokering adds substantial value. |
| **Sensor** | Pipeline (near-identity wrapper) | Sensors are already deterministic + pluggable + emit to a structured store. The wrap is structural only — no semantic change. |
| **Skill** | **NOT wrapped — surfaces as Sense Overlay content via Context Broker** | Skills are agent-prose-readable. A "pipeline that invokes the agent with skill body as context" is a no-op pipeline that fails the semantic-weight test (see [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §1.1) — the broker adds governance/audit overhead for zero pipeline-shaped value. |

---

## Path 1 — MCP → Broker pipeline (thick wrapper)

**The work:** MCP tools have no declared preconditions, no state-parameterization
(parameters are model-generated), no audit anchoring, and inline governance checks. A
wrapper translates the MCP shape into the broker shape — most of which is operator work,
not framework work.

### Manifest shape

(Full schema at [`BROKER-MANIFEST-SCHEMA.md`](BROKER-MANIFEST-SCHEMA.md). Illustrative
TOML excerpt:)

```toml
[broker]
id = "mcp-neurogrim-tools"
name = "NeuroGrim Tools (brokered)"
roles = ["Sense", "Embodiment"]
cold_store = ".claude/brain/brokers/mcp-tools/"

[broker.wraps]
type = "mcp"
mcp_server_name = "neurogrim-mcp"
mcp_server_discovery = "via-.mcp.json"

[[pipelines]]
tool = "get_health_score"
as_pipeline = "check-health"
visibility = "surfaced"
audit_class = "capability"
preconditions = []
parameter_sourcing = { hat = "model", human_persona = "model" }
tunability = "operator-only"
description = "Pull the current unified health score for the project's Brain"
when_to_use = "when the user asks how the project is doing; when about to recommend a substantive change and need grounding; when an agent feels overconfident"
# governance is auto-composed: check-trust-budget + check-kill-switch + record-dispatch + record-outcome

[[pipelines]]
tool = "domain_new"
as_pipeline = "add-domain"
visibility = "surfaced"
audit_class = "capability"
preconditions = [{ name = "domain_not_exists", expr = "!registry.domains.contains(params.name)" }]
parameter_sourcing = {
  name = "model",
  description = "model",
  weight = "state-fill = registry.defaults.domain_weight",
  force = "state-fill = false"
}
tunability = "operator-confirmed"
description = "Scaffold a new Brain domain in the registry"
when_to_use = "when the operator wants a new domain wired up; never autonomously — always operator-confirmed first"
governance_extras = ["require-operator-confirmation"]
```

### What's operator work (per-tool)

For each MCP tool the operator wants to broker:

1. **Declare preconditions.** MCP tools don't carry them. The operator must understand
   the tool's semantics + dispatcher logic to write the precondition expression. (E.g.,
   `domain_new` should declare `!registry.domains.contains(params.name) OR params.force`
   — derived from reading the dispatcher's body.) Without this, the broker has nothing
   to filter against and `legal_pipelines()` becomes the entire MCP catalog (no value
   added).
2. **Decide parameter sourcing per parameter.** `model` re-exposes verbatim (LLM
   generates the value); `state-fill = <expr>` fills from current hot store. Thin-wrap
   (all `model`) is cheap but adds little value. Thick-wrap (state-fill where possible)
   reduces the surface where the model can invent.
3. **Pick tunability tier per pipeline.** `operator-only` (config edits), `operator-confirmed`
   (LLM proposes, operator approves), `autonomous` (LLM-tunable within bounds). Per
   [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4, default `operator-only`.
4. **Author routing signal.** `description` + `when_to_use` ≤1,536 chars (per
   [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §1). MCP tool descriptions often need
   rewriting to add the "when to use" trigger phrases.
5. **Decide audit_class.** Per
   [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §2.3: `capability` for agent-facing
   work, `governance` for safety/ACL pipelines, `meta-observation` for self-observation
   (excludes from hygiene feed).

### What's framework work (automatic)

- Pipeline Runner dispatches the wrapped tool via existing MCP transport.
- Governance Composer composes `check-trust-budget`, `check-kill-switch`,
  `record-dispatch`, `record-outcome` around the dispatch (the layer inversion: MCP
  tools push governance into the tool handler; brokers pull it into the pipeline
  composition layer).
- Audit anchoring (the `dispatch_anchor` carrying tool_name + parameters + projection
  snapshot) is added automatically.
- Routing signals flow into L1 awareness per
  [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §2.
- Invocation ledger records the dispatch per `type: "pipeline"` schema.
- Skill Filter ranks the pipeline; hygiene classifies alive/dead/new.

The MCP tool itself is unchanged. Its handler runs as-before; the framework layers
governance + audit + awareness around it.

---

## Path 2 — Sensor → Broker (near-identity wrapper)

**The work:** sensors implement `Sensor::analyze(project_root) → Result<Value>` already
— deterministic, pluggable, structured output. Wrapping is almost identity:

### Manifest shape

```toml
[broker]
id = "sensor-coherence"
name = "Coherence (brokered)"
roles = ["Sense"]
cold_store = ".claude/coherence-cmdb.json"

[broker.wraps]
type = "sensor"
sensor_wire_name = "coherence"

[[pipelines]]
as_pipeline = "run-coherence-tick"
visibility = "internal"
audit_class = "capability"
preconditions = []  # sensors always legal
tunability = "operator-only"
# step body: call coherence sensor's analyze(); write to cold_store
```

### What's operator work

- Manifest declaration (~10-20 lines TOML).
- Cold-store path (often already exists; sensor wrote the CMDB before brokering).
- Role-set declaration (`[Sense]` for almost all sensors; `[Sense, InnateAbility]` for
  sensors that also expose tuning surfaces).

### What's framework work

- The `Sensor::analyze()` call becomes the Internal Service's projection step.
- Awareness Service wiring is automatic by role (Sense role → Sensory Queue + Awareness
  Service enforcer mediation per [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md)).
- Overlay materialization, governance composition, audit anchoring, awareness
  injection: all automatic.

### Reference wrapper (for S1-T calibration)

The `coherence` sensor is the reference example for the sensor-wrapping path: small,
real, exercises the path without duplicating the Work Broker worked example. Authored
during S1-T calibration measurement.

### Future macro

`fallible_sensor!` macro in `neurogrim-sensory/src/sensor_impls.rs` is the precedent
for wrapping an existing free function into a trait impl with minimal boilerplate. A
`wrap_sensor_as_broker!` macro is queued — but **defer until the reference wrapper
lands and the boilerplate is empirically visible**. Premature macro = wrong shape.

---

## Path 3 — Skills surface as Sense Overlay content (NOT a wrapping path)

**Skills are agent-prose-readable instructions, not deterministic pipelines.** Wrapping
a skill as a pipeline (`invoke-<skill-name>` with skill body as context) produces a
no-op pipeline: the broker adds full governance + audit + tunability overhead but the
pipeline's actual behavior is "hand control to the agent who reads the prose and does
what it would have done anyway." This **fails the semantic-weight test** (see
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §1.1) — the pipeline's *execution* carries
no information beyond what the agent would have done reading the skill directly.

### The right pattern

Skills surface as **Sense-role Overlay content** via the **Context Broker**'s curation
policy. The skill body remains agent-prose-readable in `.claude/skills/<name>/SKILL.md`;
the Context Broker's projection function selects the relevant skill body slice into the
Overlay based on current sub-task state.

**What this means concretely:**

- The Context Broker's cold store includes the skill directory (or symlinks to it).
- The Context Broker's curation policy (per
  [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"The Overlay contract") decides which
  skill bodies are relevant at the current tick and projects them into the Overlay.
- The LLM reads the skill body inline in its context per the existing skill mechanism;
  no pipeline dispatch happens.
- Skills retain their existing routing signals (frontmatter `description` +
  `when_to_use`) and existing `capability-hygiene` scoring.
- Skills retain their existing invocation ledger entries (`type: "skill"`).

### Why this matters for adopters

If you have skills you want "brokered awareness" over, you don't write a wrapper —
you let the Context Broker curate them into the Overlay. Skills + pipelines coexist in
the agent's capability surface:

- Skills: agent-prose-readable; routed via Claude Code's native skill mechanism; surface
  through Context Broker Overlay curation.
- Pipelines: typed, precondition-checked, audit-anchored; dispatched via broker
  framework; surface through Awareness Materializer routing signals.

The agent sees both in L1 context; the routing-signal budget is shared per
[`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §1. Different mechanisms for different
substrates; both unified at the awareness layer.

---

## Hat-contract precedent

Wrapping is the hat-contract pattern applied to a capability surface:
- **Named interface** — pick from an enumerated set (Path 1 / Path 2 / Path 3-not-wrapping).
- **When-to-use signal** — declare which substrate this wrapping pattern fits.
- **Operational checklist** — per-path: what's operator work vs framework work.

See `D:\Brains\NeuroGrim\.claude\skills\hats\SKILL.md` for the precedent. Wrapping
inherits the discipline.

---

## Open follow-ons

- **`wrap_sensor_as_broker!` macro** (Path 2) — deferred until reference wrapper lands.
- **Reference MCP wrapper** (Path 1) — `get_health_score` is a candidate first wrap;
  full schema authoring + governance composition exercises the thick-wrapper path.
- **Skill → Context Broker projection function** (Path 3 non-wrapping path) — needs
  cereGrim-side composition design (currently noted in
  [`../../cereGrim/docs/BROKER-COMPOSITION.md`](../../cereGrim/docs/BROKER-COMPOSITION.md)
  Context Broker section).
- **Cross-substrate routing-signal budget composition** — when an agent's L1 context
  carries N skill routing signals + M pipeline routing signals + K Overlay segments,
  budget arithmetic + curation policy at the cross-substrate level need explicit design.
