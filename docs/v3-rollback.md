---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# v3.0 Per-Primitive Rollback Procedures

**Status:** Authored 2026-04-27 alongside the E-B2-8 release-readiness epic.
**Scope:** v3.0 stability-marker release. The seven Brains-2.0 primitives
ship advisory (weight 0.0) across all four Brains. This doc describes per-
primitive rollback in case post-publish dogfooding surfaces a problem with a
specific primitive without invalidating the whole release.

---

## Posture

v3.0 = stability marker. All seven Brains-2.0 primitives ship at advisory
weight 0.0 — meaning their score does not affect the unified Brain score.
This is **already the rollback posture**: each primitive's machinery is in
place, but it has zero effect on operational scoring until the operator
flips the weight.

**Implication:** v3.0 release is structurally low-risk. The rollback paths
below are for cases where the primitive's mere PRESENCE (e.g., emitting
ledger rows, generating CMDBs, registering in brain-registry.json) creates
a problem — not score-related issues, since scores are already advisory.

---

## Primitive disable-knobs

| Primitive | Spec § | CMDB | Disable-knob |
|-----------|--------|------|--------------|
| Confidence as first-class envelope | §3.8 | (envelope-level; no CMDB) | Cannot be disabled — additive envelope field. Consumers ignore unknown fields per spec §16.9. |
| Self-coherence + domain-calibration | §17 | `domain-calibration-cmdb.json` | Set `weight: 0.0` on `domain-calibration` in `brain-registry.json:config.domain_weights`. Already 0.0; no action needed for v3.0 rollback. |
| Hat-as-formal-contract | §5.4.1 | (no CMDB; per-hat .md frontmatter) | Remove `forbidden_tools` / `allowed_tools` / `network_targets` blocks from `.claude/skills/hats/*.md`. v1 is advisory (no runtime cross-reference per Q10), so removal is cosmetic but signals intent. |
| Trust-budget primitive | §16.8 | `trust-budget-cmdb.json` | Set `weight: 0.0` on `trust-budget`. Optionally delete `trust-budget.toml` from the affected Brain (sensor reports `trust_budget:degraded:no_cargo_lock` and falls back to advisory floor). |
| METH-EV §16 multi-round assessment | METH-EV | (methodology doc; no CMDB) | Cannot be disabled — methodology guidance, not a runtime construct. |
| Operator-calibration | §17.12 | `operator-calibration-cmdb.json` | Set `weight: 0.0` on `operator-calibration`. Optionally disable the PostToolUse hook in `.claude/settings.local.json` to stop emitting disposition records. |
| Federated patterns A2A | §16.6.1 | `federated-patterns-cmdb.json` | Set `weight: 0.0` on `federated-patterns`. Optionally remove the federated-pattern message-type from `agent-card.json:capabilities.accepts[]` to opt out at the protocol layer (bidirectional opt-in posture per the locked decision). |

---

## Campaign-level rollback (v3.0 → v2.12)

If multiple primitives are problematic and the per-primitive disable-knob
isn't sufficient:

1. **Spec-side:** revert the v3.0 stability-marker stanza + restore `Status:
   Active` + `Version: 2.12`. v3.0 has no section-content changes, so
   reverting the stability marker carries forward zero structural changes.
2. **Workspace-side:** revert the `Cargo.toml` workspace + intra-crate
   version pins from `3.0.0` to `3.0.0-rc.1` (or a fresh `3.0.0-rc.2` if
   the v3.0.0 tag was already pushed and yanked).
3. **Primitives stay shipped at advisory weight 0.0** in all 4 Brains —
   no data deletion, no schema rollback, no submodule pointer churn beyond
   what step 2 forces.
4. **CHANGELOG-side:** add a `[3.0.0-yanked]` entry citing the yank
   rationale and pointing forward to the v3.1 calibration-report milestone
   (per Charter Amendment 2026-04-27).

**Yank window:** v3.0.0 cargo-publish is operator-decision per
`docs/publish-day-runbook.md`. If the operator opts to publish v3.0.0 and
later yanks, the rollback procedure above applies. Per
`BEFORE-PUBLIC-RELEASE.md` introduction, "cargo publish is *irrevocable* —
published crate names cannot be reused, yanking a version does not free
the name, and third-party tooling caches versions long after yank."
Treat yanked-and-re-released as a forward-only path (next semver bump, not
a re-publish of the same version).

---

## Verification of rollback

After applying any rollback above:

1. Run `bash scripts/prepublish-check.sh` from the NeuroGrim repo root.
   Gate-12 strict checks (CMDB presence + advisory-weight invariant +
   cross-Brain integration) MUST still pass after rollback — the rollback
   doesn't remove the structural surface, only its operational effect.
2. Re-score each affected Brain via `cargo run --release -p neurogrim-cli
   -- sensory <domain> --project-root <brain-path>` and confirm the score
   landed where you expected.
3. Document the rollback in `audit/BRAINS-2-0-RETROSPECTIVE-2026-04-27.md`
   under a new "Post-publish rollbacks" section so the campaign retro
   stays accurate.

---

## See also

- `audit/BRAINS-2-0-CHARTER.md` — campaign locked decisions + Charter
  Amendment 2026-04-27 (calibration-window reframe).
- `audit/BRAINS-2-0-RETROSPECTIVE-2026-04-27.md` — campaign retrospective
  + METH-EV §16 multi-round assessment cadence applied to campaign close.
- `docs/publish-day-runbook.md` — publish-day procedures + yank rollback
  window (currently scoped to gate-11 supply-chain).
- `BEFORE-PUBLIC-RELEASE.md` gate 12 — master gate for the Brains-2.0
  self-observability campaign.
