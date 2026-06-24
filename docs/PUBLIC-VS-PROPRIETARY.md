# Public vs. Proprietary — IP Boundary Policy

This document pins where the IP boundary lives for the **broker pattern** documented in
[`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) + [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md).
It exists because the obvious mechanical test ("can you replace 'broker' with 'well-designed
module' and have the sentence still parse?") **fails on its own target** — the test gives a
false-negative on the very paragraphs it was designed to catch. So this doc replaces the
mechanical test with an **enumerated list of claims** that stay in consuming-project
proprietary documentation and not in this public NeuroGrim spec.

---

## The line

**NeuroGrim publishes the broker pattern as architectural infrastructure.** The trait, the
Pipeline primitive, the role-set scaffolding, the Overlay contract, the governance composer,
the 34 building blocks, the canonical brokers, the contract that consuming projects
implement against. All of this is **public** because NeuroGrim is a public open-source
project and the broker pattern is the substrate it provides.

**Consuming projects publish their own design articulation in their own documentation,
under whatever access controls they prefer.** For example, cereGrim is a *private*
subproject of the Brains-ecosystem repo; its `thesis/` directory holds the design
articulation explaining why cereGrim adopted the broker pattern in a dual-lobe agent
harness — that articulation is **proprietary to cereGrim** and never appears in NeuroGrim
public docs.

The line: NeuroGrim documents *what a broker is and how to build one*. Consuming projects
document *why they chose to adopt brokers and what makes that adoption commercially or
architecturally valuable to them*.

---

## Enumerated claims that stay in consuming-project documentation

The following claim patterns are **excluded from this NeuroGrim public spec** and remain in
consuming projects' own documentation (e.g., `cereGrim/thesis/tenets/`):

| # | Claim pattern | Why it's proprietary | Where it stays |
|---|---|---|---|
| 1 | *"Brokers are the load-bearing lever for sub-frontier compute"* | This is a cost-thesis claim — it asserts brokers are THE commercial differentiator that lets a small model match a frontier model. Specific to consuming projects' commercial positioning. | cereGrim's `thesis/tenets/01-brokered-cognition.md` |
| 2 | *"Every deterministic decision a broker absorbs is compute the small model never spends"* | Same cost-thesis framing — explicit articulation of the savings mechanism. | cereGrim's `thesis/` |
| 3 | *"This is how a 9B model punches up"* (or similar size-relative claims) | A specific commercial framing of broker-pattern's value to consuming-project size/cost decisions. | cereGrim's `thesis/` |
| 4 | *"That is the whole point"* (in reference to broker preconditioning) | Identifies WHICH property is the secret sauce. The substrate spec documents the property (preconditioning) without naming it as THE lever. | cereGrim's `thesis/` |
| 5 | *"Primitive vs pattern"* coined-distinction language | A thesis-coined framing that the broker is a primitive (not just a design pattern) because it's the unit of value composition. Commercial-positioning language. | cereGrim's `thesis/` |
| 6 | Dual-lobe specifics (Primary Lobe / Meta Lobe semantics) | Specific consuming-project architecture; not framework substrate. | cereGrim's `docs/ARCHITECTURE.md` + `thesis/` |
| 7 | Memory broker architecture depth (two-tier hot/cold + salience-driven eviction + escalation contract) | Consuming-project's design articulation for its memory broker; the framework provides the Broker trait + Pipeline primitive; the specific memory-broker design is consuming-project's. | cereGrim's `docs/MEMORY-BROKER.md` (when authored) |

---

## How the rephrasing works

When a claim from the proprietary list appears in a phrase pattern in this NeuroGrim
public spec, it gets **rephrased to preserve the substrate fact without the proprietary
framing**. Examples from the migration that landed this document:

| Original (proprietary framing) | Rephrasing (substrate fact only) |
|---|---|
| "The proprietary cost-thesis articulation (why this pattern is the load-bearing lever for sub-frontier compute) lives in..." | "Consuming-project-specific design articulation and adoption rationale live in those projects' own documentation..." |
| "Brokers as substrate can live in NeuroGrim publicly; brokers as cost-thesis stay in the thesis." | "The broker pattern as architectural infrastructure lives in NeuroGrim publicly; consuming-project-specific design articulation lives in those projects' own documentation." |
| "The LLM never sees a capability whose preconditions aren't met. **That is the whole point.**" | "The LLM never sees a capability whose preconditions aren't met. **This is the broker's central invariant.**" |
| "cereGrim owns: ...The cost-thesis articulation in `thesis/`. The architectural narrative for *why* brokers are the load-bearing primitive." | "Consuming projects own: ...consuming-project-specific design articulation (e.g. cereGrim's `thesis/` for *why* it adopted this primitive)." |

The substrate fact survives. The cost-thesis framing does not.

---

## Three-tier responsibility map

| Tier | Lives in | Contents |
|---|---|---|
| **NeuroGrim public** (this repo) | `D:\Brains\NeuroGrim\` | Broker trait, Pipeline primitive, role-set scaffolding, Overlay contract, governance composer, canonical broker impls, contract spec (BROKER-CONTRACT, BROKER-INTERNALS), public IP-boundary policy (this doc) |
| **Consuming project public** | e.g., `D:\Brains\cereGrim\docs\` (private to that subproject, but not thesis-level) | Project's role-set declarations for canonical brokers, project-specific cold-store schemas, project-specific curation policies, project-specific leaf-op functions, project's BROKER-COMPOSITION.md pointing at NeuroGrim's spec |
| **Consuming project proprietary** | e.g., `D:\Brains\cereGrim\thesis\` | Cost-thesis articulation, dual-lobe specifics, memory-broker design depth, commercial-positioning language, the *why this primitive matters competitively* |

---

## Boundary crossing rules

- **NeuroGrim public → Consuming project proprietary:** strictly forbidden. NeuroGrim
  public docs never reproduce thesis content, never reference thesis files by content
  (referencing by *path* is fine — "see `../../cereGrim/thesis/` for that project's
  rationale" is acceptable; quoting from the thesis is not).
- **Consuming project proprietary → NeuroGrim public:** also forbidden. Thesis files
  may reference NeuroGrim public spec content (they implement against it), but the
  thesis articulation never lands in NeuroGrim's public docs.
- **Consuming project public ↔ NeuroGrim public:** open. Consuming project public docs
  reference NeuroGrim public spec freely; NeuroGrim public spec references "consuming
  projects" in the abstract.

---

## How to apply this policy

When authoring or reviewing any NeuroGrim public doc that mentions the broker pattern:

1. **Read the enumerated claim list above.** If your wording matches any pattern, apply
   the rephrasing column.
2. **The grep test (mechanical):** `grep -i "cost thesis\|load-bearing lever\|punches up\|the whole point\|every deterministic decision absorbed\|primitive vs pattern" D:\Brains\NeuroGrim\` should return zero matches outside this policy doc itself. If it returns matches in other NeuroGrim docs, those are leakage; rephrase.
3. **The semantic test (human judgment):** does this sentence land because brokers are
   well-designed substrate, or because brokers are *the load-bearing lever for
   sub-frontier compute*? The first is public; the second is consuming-project-proprietary.

---

## Why not the mechanical "replace 'broker' with 'well-designed module'" test?

That test was proposed earlier and **fails its own target**. Tried on `BROKER-CONTRACT.md`
line 31 ("The LLM never sees a capability whose preconditions aren't met. That is the whole
point."):

> "A well-designed module never lets the LLM see a capability whose preconditions aren't
> met. That is the whole point."

Sentence still parses. Sentence is still "safe-for-public" by the mechanical test. But the
next sentence — "that is the whole point" — is a thesis-grade claim (it identifies WHICH
property is THE lever). The mechanical test gives a false-negative on the exact thing it
was supposed to catch.

The real test is **"could a competing AI-infra company read this paragraph and clone the
cost-thesis posture?"** — and that's a human judgment call, not a mechanical rule. The
enumerated list + grep above gives consuming-project authors a clear floor without
requiring them to make the call from scratch each time.

---

## Audit by example — structure-as-leakage

The grep test is **necessary but not sufficient.** A document can be free of every
thesis-grade *phrase* on the enumerated list and still leak the cost-thesis posture via
**the structure of its design choices.** Choosing to wrap skills as Overlay content
rather than as pipelines, choosing to split governance from capability ranking via
`governance_pipelines()`, choosing role-set composition over role-class partitioning,
choosing cold-store-as-truth for workflow atomicity — these are *design positions* that
a thesis-aware author would take, and that a thesis-naive author would either skip or
get wrong.

**Implication:** when reviewing a new broker design (or a new substrate doc) for IP
leakage, ask not only "does this paragraph quote a thesis-grade phrase?" but also
"would the *shape* of this design be obvious to an author who hadn't internalized the
cost-thesis?" If the answer is no — if the design only makes sense once you've accepted
the cost-thesis premise — then the *structure* is leaking the posture even if no
phrase from the enumerated list appears.

**Mitigation:** when documenting a design choice with this property, frame the choice
in terms of its **substrate-level rationale** (consistency, atomicity, safety,
composability) rather than its **cost-thesis rationale** (compute savings, frontier
parity, judgment-vs-determinism trade-off). The substrate-level rationale is true at
the framework level; the cost-thesis rationale is true at the *why this framework
matters competitively* level — and that's the line.

**Example — wrapping path choice:** the decision to wrap skills as Overlay content
rather than as pipelines is documented in
[`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) Path 3 in terms of the semantic-weight test
(substrate-level: pipelines must carry execution-time semantic weight). The
substrate-level rationale stands without invoking the cost-thesis. The cost-thesis
rationale (skill-wrapping-as-pipeline would burn governance overhead for zero capability
gain, eroding the small-model expressivity margin) is the *why this matters
competitively* angle — and it stays in consuming-project thesis docs, not in the
substrate spec.

This audit-by-example pass complements the grep test: grep catches lexical leakage;
audit-by-example catches structural leakage. Both passes are needed for the policy to
hold.

---

## Maintenance

When a new proprietary claim pattern is identified (in a consuming project's thesis review,
in a NeuroGrim doc audit, in an external review), add it to the enumerated list. The list
grows; the policy stays the same.

Cross-referenced from:
- [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) header (the IP-boundary callout)
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) header (the IP-boundary callout)
- `D:\Brains\NeuroGrim\CLAUDE.md` (broker-framework section)
- `D:\Brains\cereGrim\CLAUDE.md` (the project's IP-boundary discipline)
