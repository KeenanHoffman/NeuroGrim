<!-- topic: federation — bundled in neurogrim-cli v3.4 -->
# Federation — Brains as A2A peers

A Brain can stand alone, or it can participate in a federation
with other Brains. Federation enables **fractal composition**: a
parent Brain queries child Brains for their scores via the A2A
protocol, then aggregates them into a unified ecosystem score.
The children remain authoritative; the parent is a conductor, not
a CEO.

This document covers when to federate, how peer relationships
work, and the read-only sibling pattern.

## The model

Brains are **peers at the protocol level** (spec §13). One Brain
queries another by:

1. Discovering the peer's Agent Card at
   `<peer-url>/.well-known/agent-card.json`
2. Sending an A2A envelope (`snapshot.requested`) to
   `<peer-url>/a2a/v1/tasks`
3. Receiving the peer's `AgentOutput` JSON in response

A Brain has no protocol privileges over its children. The "parent"
relationship is purely topological — the parent declares the child
in `config.children`; that declaration is voluntary on both sides.
A child can refuse, can rate-limit, can authenticate, can serve a
filtered view.

## The four ecosystem Brains

This NeuroGrim ecosystem ships with four Brains arranged in a
two-level tree:

```
Ecosystem Brain (D:/Brains/)
├── A2A → NeuroGrim Brain (port 8421)
│         └── A2A → Python Starter Brain (port 8423)
├── A2A → LSP-Brains Brain (port 8422)
└── A2A → job-hunt Brain (port 8424, read-only sibling)
```

The ecosystem Brain reaches python-starter via NeuroGrim — a
two-hop fractal-composition example. To see this Brain's federation
peers, run `neurogrim agent --prose`.

## Read-only siblings

A **read-only sibling** is a peer Brain registered with `read_only:
true` in the parent's `config.children`. The flag codifies a
non-influencing posture: the parent reads the sibling's scores
into its observation space but never originates outbound writes
toward the sibling.

```json
"job-hunt": {
  "a2a_endpoint": "http://localhost:8424/a2a/v1/",
  "weight": 0.0,
  "read_only": true
}
```

Common reasons to use read-only siblings:
- Observing a project that's not part of the ecosystem's mandate
- Pilot projects (test the methodology against a sibling without
  pulling it into the parent's governance)
- Cross-team observability (your Brain can watch your peer's
  Brain without their Brain having to trust yours)

## Adding a federation peer

The automated path:

```bash
# From the parent Brain's project root:
neurogrim federation register --name <peer-id> --path <peer-path>

# For a read-only sibling:
neurogrim federation register --name <peer-id> --path <peer-path> --read-only
```

This mutates `config.children` atomically, allocates an unused A2A
port (8421 + N for the Nth peer), and bumps the registry's
`schema_version` to `2.1` if the `read_only` flag was used (the
flag landed schema-additively).

To run the peer Brain:

```bash
cd <peer-path>
neurogrim a2a-serve --port <allocated-port> --project-root .
```

To verify discoverability:

```bash
neurogrim a2a-discover http://localhost:<port>/a2a/v1/
```

## Scoring with A2A children

For the parent to actually aggregate child scores, declare each
child as an A2A *scoring source* in the parent's
`config.domain_definitions`:

```json
"child-neurogrim": {
  "scoring_source": {
    "type": "a2a",
    "endpoint": "http://localhost:8421/a2a/v1/",
    "interface_version": "1"
  }
}
```

When the parent runs `neurogrim score`, it'll fetch the child's
current `AgentOutput` over A2A and use its unified score as that
domain's raw score. If the child is unreachable, the domain falls
back to `no_file_score` (default 0) and a warning is logged —
same semantics as a missing CMDB.

## Cultural substrate across federation

Every Brain in a federation ships with a byte-identical
`culture.yaml`. The `culture-coherence` domain (one of the
ecosystem-only domains) verifies that all federation copies stay
aligned. Drift triggers a finding; agents acting on it should
restore byte-identity.

Why byte-identical? Culture is a shared invariant. If your culture
file says "honesty is a floor" and your peer's says "honesty is
optional," your federation has a value misalignment that
recommendations alone won't surface.

## Cross-references

- `neurogrim explain methodology` — the overlay model
- `neurogrim explain culture` — the culture.yaml invariant
- `neurogrim federation register --help` — the scaffolder
- `neurogrim a2a-serve --help` — running this Brain as a peer
- `neurogrim a2a-discover --help` — fetching peer Agent Cards
- `.claude/skills/peer-brain/SKILL.md` — operating-as-a-peer guide
- `.claude/skills/a2a/SKILL.md` — A2A protocol summary
- Spec §13 — A2A protocol; §9 — fractal composition
