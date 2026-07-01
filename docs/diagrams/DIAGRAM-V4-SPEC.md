---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# NeuroGrim — Broker Pattern Diagram v4 Spec

**Status:** DIAGRAM UPDATED 2026-06-30. The v3 diagram (`broker-pattern.drawio.svg`)
predated the Phase 1–5 substrate deepenings and has been retired to
[`archived/broker-pattern.drawio.svg`](archived/broker-pattern.drawio.svg). This doc
is the **textual source-of-truth**; the v4 visual is now rendered as a
version-controllable Mermaid source at [`broker-pattern.mmd`](broker-pattern.mmd).
This doc remains as the running rationale (what changed and why).

The v3 visual shows the original 6-piece LLM-level + 3-piece terminal pattern with
4 brokers + Workspace Manager + Effectors. v4 needs to incorporate the 10 building
blocks added since (#25-#35), the Topology rename, the Overlay tier vocabulary,
peer-dialogue cycles, governance vs legal channel split, and the authority hierarchy.

---

## What changed since v3 (delta list)

### Renames + retired terms
- **Federation Broker → Topology Broker** (rename Phase 2; cylinder label, broker box label)
- **role-class → role-set** (composable subset, not partition; visual: brokers carry
  badges of their declared roles, e.g., Browser shows `[Sense, Embodiment]`)

### New canonical brokers (visual additions to LLM-level cluster)
None — the canonical broker set is stable (Context, Workspace, Sensory, Topology,
Work, Workspace Manager + Effectors). What's new is the *infrastructure* around them.

### New substrate primitives (visual additions surrounding the broker pattern)
| BB | Add to v4 |
|---|---|
| #22a Materializer Composer | New small box near Hot-Store + Awareness Materializers; arrows show "concatenates per operator-declared order into current-projection.md" |
| #25 Pipeline Cancellation Handler | Annotate on broker capsules: small "⛔ on_cancel" tag indicating cancellation handler attachment point |
| #26 Schema Migration Runner | New "startup-time" box outside the broker capsule with arrows to cold stores; marked "Tier 3 bootstrap" |
| #27 Cross-Broker Composition (extended in Phase 4) | Cross-broker arrows between broker capsules with annotation "via Topology Broker ACL"; cycle-detection icon at startup |
| #28 Diagnostics Collector | New box near the broker cluster; arrows OUT to `_neurogrim/diagnostics/*` topics; arrows IN to operator + Sensory Broker |
| #29 Broker Lifecycle | Lifecycle state indicator on each broker capsule (starting/running/draining/stopped); arrows between states |
| #30 Onboarding Projection | Distinct segment of Materializer Composer's input flow, marked "once-per-agent" |
| #31 Cluster Federation Topology | NEW top-level visual layer: cluster boundary + inter-cluster federation arrows + transitive ACL annotations |
| #32 Operator Telemetry Summarizer | New "operator-facing" box reading from Diagnostics + projection + audit; arrow out to operator CLAUDE.md |
| #33 Pipeline Proposal Mechanism | Extends BB #21 visual: proposal-ledger gains "pipeline-proposal" entry type icon |
| #34 Workflow-Pipeline Versioning Contract | Annotate workflow checkpoint boxes with "compatible_contracts: [N, N-1]" badge |
| #35 Frame stack | NEW visual layer: Frame stack box overlaying the broker cluster; arrows showing inheritance from cluster → broker → role → pipeline → dispatch; Frame-rotation pipelines highlighted as multi-pass loops |

### Overlay tier vocabulary (new visual distinction)
The v3 diagram shows "Overlay" as a single concept. v4 splits visually:
- **Overlay** (tier-1; per broker; atomic-swap, no-torn-read) — same cylinder as v3
- **OverlayView** (tier-2; Topology Broker's per-consumer ACL-filtered topology) —
  new cylinder showing layered derivation from one or more Overlays
- **OverlayMesh** (tier-3; cluster-aggregated; cluster-Sense projection) — new
  cylinder at the cluster level showing aggregation across peer-agents' Overlays

### Peer-dialogue Meta-Primary visualization (NEW)
The v3 diagram doesn't show the dual-lobe peer-dialogue. v4 needs:
- Primary Lobe and Meta Lobe boxes (distinct from each other; Primary is the LLM that
  consumes broker projections; Meta is the consideration partner)
- Bidirectional arrows on `_neurogrim/cognition` channel between them
- Annotation: "peer-dialogue — neither overrides; mutual visibility; consensus
  emerges from convergence (or recursion-guard fires)"
- A separate visual showing the cognition channel schema (cycle_id + iteration +
  speaker + payload_type + incorporates/rejects)

### Governance vs Legal channel split (NEW visual)
Per BB #20 Skill Filter + the reachability invariant:
- Two distinct channels emerging from the broker: `legal_pipelines(state)` (capability
  ranking, top-K only) AND `governance_pipelines()` (sidecar, always-reachable)
- LLM reads BOTH; capability ranking is for the choice menu; governance pipelines are
  always-on safety surface
- Visual: split arrow from broker → LLM with the two channels labeled

### Authority hierarchy (NEW callout)
A visual indicator showing precedence:
- Kill-switch (Untunable governance) > Broker authority (cold/hot store decisions) >
  peer-dialogue Meta-lobe consideration
- Render as nested boxes (kill-switch is the outermost cap; broker authority sits
  inside; Meta consideration is innermost)
- Annotation: "When authority claims collide, the outer layer wins."

---

## Suggested v4 layout (textual sketch)

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                              CLUSTER BOUNDARY                                   │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                    CLUSTER FEDERATION TOPOLOGY (#31)                      │  │
│  │  [inter-cluster ACL + version cascade arrows to peer clusters]           │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
│  ╔═══════════════════════════════════════════════════════════════════════════╗ │
│  ║  KILL-SWITCH AUTHORITY (Untunable governance — outermost)                  ║ │
│  ║  ┌───────────────────────────────────────────────────────────────────────┐║ │
│  ║  │  BROKER AUTHORITY (cold/hot store decisions)                          │║ │
│  ║  │  ┌─────────────────────────────────────────────────────────────────┐  │║ │
│  ║  │  │  PEER-DIALOGUE META CONSIDERATION (innermost; advisory only)    │  │║ │
│  ║  │  │                                                                  │  │║ │
│  ║  │  │  ┌──────────────┐         ┌──────────────┐                       │  │║ │
│  ║  │  │  │ Primary Lobe │ ◄───►  │  Meta Lobe   │                       │  │║ │
│  ║  │  │  └──────────────┘  cognition  └────────────┘                     │  │║ │
│  ║  │  │                  channel                                          │  │║ │
│  ║  │  └─────────────────────────────────────────────────────────────────┘  │║ │
│  ║  │                                                                       │║ │
│  ║  │  CANONICAL BROKERS (with role-set badges)                            │║ │
│  ║  │  ┌────────┐ ┌──────────┐ ┌─────────┐ ┌──────────┐ ┌──────┐          │║ │
│  ║  │  │Context │ │Workspace │ │ Sensory │ │ Topology │ │ Work │          │║ │
│  ║  │  │[Sense] │ │[Sense]   │ │[Sense]  │ │[Sense]   │ │[InAb]│          │║ │
│  ║  │  └───┬────┘ └────┬─────┘ └────┬────┘ └────┬─────┘ └──┬───┘          │║ │
│  ║  │      │           │            │           │          │              │║ │
│  ║  │  Each broker exposes:                                                 │║ │
│  ║  │  - Overlay (tier-1) cylinder + Working State (private)                │║ │
│  ║  │  - legal_pipelines() ┐                                                │║ │
│  ║  │                       ├── to LLM                                      │║ │
│  ║  │  - governance_pipelines() ┘ (sidecar; always reachable)               │║ │
│  ║  │                                                                       │║ │
│  ║  │  Cross-broker composition arrows (BB #27, via Topology Broker ACL)    │║ │
│  ║  │  Cycle-detection icon at startup                                      │║ │
│  ║  │                                                                       │║ │
│  ║  │  ┌────────────────────────────────────────────────────────────────┐  │║ │
│  ║  │  │  WORKSPACE MANAGER (Embodiment role)                            │  │║ │
│  ║  │  │  ┌──────┐ ┌──────────┐ ┌────────────────┐                       │  │║ │
│  ║  │  │  │ IDE  │ │ Browser  │ │ Custom Sensor  │ ◄─ Effectors          │  │║ │
│  ║  │  │  │[Emb] │ │[Sense,   │ │[Embodiment-    │                       │  │║ │
│  ║  │  │  │      │ │ Emb]     │ │ afferent]      │                       │  │║ │
│  ║  │  │  └──────┘ └──────────┘ └────────────────┘                       │  │║ │
│  ║  │  └────────────────────────────────────────────────────────────────┘  │║ │
│  ║  └───────────────────────────────────────────────────────────────────────┘║ │
│  ╚═══════════════════════════════════════════════════════════════════════════╝ │
│                                                                                 │
│  FRAME STACK (BB #35; per-agent view; surfaced in L1 awareness)                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ hat | stakes | tempo | mode | confidence | audience | scope               │  │
│  │  (inheritance: dispatch → pipeline → role → broker → cluster — innermost  │  │
│  │   wins; conflict precedence per Stakes > Hat > Mode > Confidence > ...)   │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
│  AWARENESS LAYER                                                                │
│  ┌───────────────────────────────────────────────────────────────────────────┐ │
│  │ Materializer Composer (#22a)                                              │ │
│  │  ┌──────────────────┐ ┌──────────────────┐ ┌─────────────────────────┐  │ │
│  │  │ Hot-Store Mat'r  │ │ Awareness Mat'r  │ │ Onboarding Projection   │  │ │
│  │  │ (#22)            │ │ (#24)            │ │ (#30; once-per-agent)   │  │ │
│  │  └────────┬─────────┘ └────────┬─────────┘ └────────────┬────────────┘  │ │
│  │           └────────── concatenates ──────────────────────┘                │ │
│  │           into .claude/brain/broker/current-projection.md (auto-loaded)  │ │
│  └───────────────────────────────────────────────────────────────────────────┘ │
│                                                                                 │
│  OBSERVABILITY LAYER                                                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ Diagnostics Collector (#28) ──► _neurogrim/diagnostics/*                 │   │
│  │                                  ↓                                         │   │
│  │ Operator Telemetry Summarizer (#32) ──► operator CLAUDE.md auto-load     │   │
│  │ Trace Sink (#12) + Replay (#13; subsumes test fixtures)                  │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  LIFECYCLE + STARTUP                                                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ Schema Migration Runner (#26; startup) → cold stores                     │   │
│  │ Broker Lifecycle (#29; startup/shutdown/drain/hot-swap)                  │   │
│  │ Pipeline Proposal Mechanism (#33; extends Proposal Ledger #21)           │   │
│  │ Workflow-Pipeline Versioning Contract (#34; per-workflow checkpoint)     │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────────────┘
```

(ASCII sketch is illustrative; the operator's drawio will be more polished and use
visual hierarchy + color coding.)

---

## Color coding (suggested)

- **Pink/magenta** — broker capsules (preserved from v3)
- **Cyan** — Hot Store / Overlay cylinders (preserved from v3)
- **Green** — Service tier (preserved from v3)
- **Dark green / forest** — Cold Store cylinders (preserved from v3)
- **Purple** — peer-dialogue cognition channel (NEW; visually distinct from substrate)
- **Yellow/orange** — governance pipelines sidecar (NEW; safety surface)
- **Gray/silver** — diagnostic + observability surfaces (NEW; meta-layer)
- **Red** — kill-switch authority outer boundary (NEW; outermost cap)
- **Blue** — Frame stack overlay (NEW; cross-cutting concern)

---

## What's deliberately NOT in v4

- **Per-Frame-type details** (Hat values, Stakes values, etc.) — too granular for the
  pattern diagram; lives in BROKER-FRAMES.md §2 table.
- **Specific cluster-pipeline examples** — too project-specific; lives in cereGrim's
  composition docs.
- **MCP-wrapping path detail** — fits better in BROKER-WRAPPING.md illustrations.
- **The 35-BB table** — that's a tabular reference, not a visual.

The diagram's job is to show the **pattern's shape** at a glance — a contributor or
operator looking at v4 should see: the broker capsule, the awareness layer, the
governance/legal channel split, the peer-dialogue cycle, the cluster boundary, the
authority hierarchy. Everything else is in the prose.

---

## Versioning

The v4 visual now lives as `broker-pattern.mmd` (Mermaid source), rendered from this
spec. This doc remains as the running rationale. Future diagram versions (v5, v6) get
their own spec docs (`DIAGRAM-V5-SPEC.md`, etc.) describing the deltas from the prior
version. v4 spec preserves the deltas from v3 forever.

## Status (P-4)

**DIAGRAM UPDATED 2026-06-30 (doc-v5 upgrade, Phase 3).** The v4 diagram was authored
as `broker-pattern.mmd` (Mermaid flowchart) rendered from this spec — Topology Broker
label, the Overlay/OverlayView/OverlayMesh tier split, the governance-vs-legal channel
split, the peer-dialogue cycle, the authority hierarchy, the Frame stack, and building
blocks #22a–#35. The stale v3 `broker-pattern.drawio.svg` was retired to
[`archived/broker-pattern.drawio.svg`](archived/broker-pattern.drawio.svg) per the
`skill-deprecation` archival-with-provenance convention. Prose still wins when diagram
and prose disagree; `broker-pattern.mmd` is now the current visual source.
