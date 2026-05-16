# LSP-Brains spec proposals

This directory holds proposals for additions or refinements to the LSP-Brains
spec (and the canonical `neurogrim-core` types that implement it). Each
proposal is a self-contained document that motivates a change, sketches the
schema/convention/protocol, considers alternatives, and lists acceptance
criteria.

Proposals here are **not yet accepted**. Once a proposal is reviewed and
accepted, the relevant changes land in:
- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` (methodology / wire shapes)
- `neurogrim-core` crate (Rust types + helpers)
- The relevant adopter (e.g., `D:\local-pc-operational-management\children\neurogrim-ide\`)
  picks up the new behavior when it next builds against the new
  `neurogrim-core` version.

## Active proposals

These proposals were authored 2026-05-01 by the NeuroGrim IDE team during
Phase B/C of the IDE's data-layer rollout. They were surfaced by the four-
round adversarial review of `data-layer-plan-v4.md` and are formally listed
in plan §15.

| # | Title | Status | Blocks (IDE) |
|---|---|---|---|
| [15.1](./15.1-adopter-metric-events-topic.md) | Adopter metric-events topic convention | DRAFT | `ide.*` series ingestion (Phase E self-observability) |
| 15.2 | ~~Module performance budget~~ | **IDE-INTERNAL** | (n/a — demoted by round-2 critique; revisit when 2nd adopter wants modules) |
| [15.3](./15.3-topic-visibility-classification.md) | Topic visibility classification | DRAFT | Defense A9 enforcement (Phase C4) |
| [15.4](./15.4-tsdb-series-namespacing.md) | TSDB adopter-series namespacing | DRAFT | Forward-compat for any adopter authoring metrics |
| [15.5](./15.5-requires-neurogrim-version-pin.md) | `requires_neurogrim` version-pin | DRAFT | Defense in depth against silent v5 breakage |
| [15.6](./15.6-schema-additive-evolution.md) | Schema additive-evolution enforcement | DRAFT | Defense A10 enforcement (Phase C4) |
| [15.7](./15.7-local-awareness-key-visibility.md) | Per-key LocalAwareness visibility | DRAFT | Defense A11 enforcement (federation reads of LocalAwareness) |

## Process expectations

These are proposals from one adopter (the IDE) to the spec. They are
**candidates** for becoming spec text. The maintainer (Keenan) is both the
proposer and the spec editor — but the discipline of writing them as
adversary-reviewable documents matters even so. It forces:
- An honest motivation that survives a critic's "why not just X?"
- Explicit alternatives so the chosen design doesn't look inevitable in retrospect
- Backwards-compat reasoning so existing Brains aren't broken
- A clear acceptance bar so "is this done?" has a defined answer

## Cadence

Proposals don't need to all land at once. Suggested order based on
critical-path-to-IDE-feature:

1. **15.7** first (blocks A11 — federation defense; highest-stakes)
2. **15.1** next (unblocks Phase E `ide.*` self-observability)
3. **15.3** next (unblocks A9 — topic-private defense)
4. **15.4 / 15.5 / 15.6** can land in any order after the critical three

## See also

- IDE plan: `D:\local-pc-operational-management\children\neurogrim-ide\docs\data-layer-plan-v4.md` §15
- Round-2 critique that surfaced these (especially §VII triage table)
- v4 Roadmap: `roadmap/v4-roadmap.md`
- Active epics: `roadmap/epics/`
