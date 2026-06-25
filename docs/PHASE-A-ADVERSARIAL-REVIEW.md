# Phase A Adversarial-Hat Review (B5 deliverable)

> **Hat:** Adversary on the Phase A substrate surfaces.
> Per plan §B5: review the Phase A APIs before Phase C bulk migration
> pressures them at IDE scale. Findings are SHIP-OR-NOT decisions, not
> hand-wavy nits — each one is paired with a concrete recommendation
> (close inline, defer to S1-T with rationale, or rework now).

---

## Substrate surfaces under review

1. `GovernanceComposer` + `PreDispatchSubgate` trait + registration order (A1, A1.5, A4)
2. `Pipeline.bypasses_kill_switch` field + catalog policy (A1.5, B-64)
3. `Pipeline` visibility classes — `Surfaced` / `Internal` / `AuditOnly` (A14)
4. `BrokerHost` boot + dispatch + tick API (A6a, B-62)
5. `Frame` + precondition DSL `{frame.X}` substitution (A13, BB #35)
6. Per-broker materializer budget allocation + truncation marker (A10)
7. JSON Schema validator subset (A11)
8. Cluster manifest schema evolution (A12)
9. `RateLimitSubgate` / `SystemPressureSubgate` / `CapabilitySubgate` (A7/A8/A9)
10. Trace audit-completeness contract (A1/C8 fix)

---

## Findings

### F1. GovernanceComposer subgate registration is one-shot, not idempotent

**Surface:** `GovernanceComposer::register_pre_dispatch_subgate(Arc<dyn …>)`
appends to an internal Vec under RwLock. Calling it twice with the same
subgate (e.g., due to a Tauri restart cycle that re-runs `setup()`) registers
TWO copies, doubling rate-limit accounting + capability checks for every dispatch.

**Real-world risk at IDE scale:** the IDE's restart flows (single-instance
plugin focus, dev-mode HMR, `--dev-agent-control` flag) could trigger
multiple `setup()` calls in a long-lived dev session. Operator would see
quota refusals at half the declared limit because both copies decrement
the window. Hard to debug — both subgates carry the same `name`, so the
refusal message looks identical.

**Severity:** Medium. Common-mode failure path for IDE-side wiring.

**Recommendation (inline):** add `register_pre_dispatch_subgate` idempotency
via a `name -> Arc<dyn ...>` HashMap keyed by `subgate.name()` — second
registration with the same name REPLACES the first (with a `tracing::warn`
so the operator sees the silent replacement). Closes F1 with ~15 LOC.

**Status:** Deferred to follow-up commit (this review documents the
finding; the fix is small but should be its own commit with a regression
test).

---

### F2. `bypasses_kill_switch` policy validates at catalog load but not at registration

**Surface:** `validate_catalog_with_policy()` rejects user-authored pipelines
with `bypasses_kill_switch=true` AND `audit_class != Governance`. But the
`BrokerHost::boot` path constructs pipelines from Rust code (`broker.catalog()`),
NOT from YAML/TOML. Rust-constructed pipelines bypass `validate_catalog`
entirely unless the operator opts in.

**Real-world risk:** an IDE-side broker could ship a Rust-literal Pipeline
with `bypasses_kill_switch=true` + `audit_class=Capability` (e.g., a
well-meaning operator forgetting B-64 policy) and the runtime would accept
it silently. The agent then has a bypass route.

**Severity:** Medium. Defense-in-depth gap; reachable only by broker authors
who haven't read B-64 docs (not by agents directly).

**Recommendation (inline):** `BrokerRegistry::register_with_catalog` should
call `validate_catalog_with_policy(...)` internally with the host's
configured policy. Closes F2 — Rust-literal pipelines now hit the same
validation as YAML-loaded ones. ~10 LOC + regression test.

**Status:** Deferred to follow-up commit.

---

### F3. `Visibility::AuditOnly` introduced (A14) but not yet enforced consistently

**Surface:** `Visibility::AuditOnly` excludes a pipeline from awareness
routing (per A14 closure). But:
- `legal_pipelines()` in tests / WorkBroker filters by
  `matches!(p.visibility, Visibility::Surfaced)` — AuditOnly correctly
  excluded.
- `governance` subgates skip for `!Surfaced` — AuditOnly correctly skipped.
- But `record_dispatch` (trust budget consumption) ALSO skips for
  `!Surfaced` — AuditOnly dispatches don't consume budget.

**Question:** SHOULD AuditOnly dispatches consume budget? The plan §C8 case
(`browser-overlay-broker` with AuditOnly pipelines for DOM annotation) is
high-frequency (every overlay highlight). If those consume budget, the
operator's 10k default ceiling exhausts in minutes of normal browsing.

**Recommendation:** AuditOnly correctly skips budget — high-frequency
IDE-internal infrastructure shouldn't deplete the agent's dispatch budget.
But document this explicitly in `BROKER-INTERNALS.md` §1.3 so operators
don't expect budget enforcement on AuditOnly.

**Severity:** Low. Already-correct behavior; just needs spec entry.

**Status:** Doc-only; close inline in PHASE-PROGRESS.md.

---

### F4. `Frame` substitution is string-based; opens injection-via-frame-value risk

**Surface:** `Frame::substitute_placeholders(predicate)` does string replace
of `{frame.<key>}` with `frame.values[key].to_string()`. If a frame value
contains DSL operators (`=`, `!=`, `.0.status`, etc.), the substituted
predicate would parse differently than intended.

**Example:** if operator sets `frame.foo = "active_work = ready"` and a
pipeline has precondition `"{frame.foo}"`, the resolved predicate becomes
`"active_work = ready"` — which is a valid DSL expression that evaluates
to true/false against overlay. That's NOT what the operator intended if
they meant frame.foo to be opaque data.

**Real-world risk:** frame values are operator-declared (not agent-declared)
so the attack surface is narrow. But an Autonomous-tuned frame value
(future) could cause unintended predicate behavior.

**Recommendation:** when substituted into a predicate, frame values should
be QUOTED if they contain DSL meta-characters (`=`, `.`, `!`, `{`, `}`).
Or alternatively, document explicitly that frame values are inlined
verbatim and operators must escape themselves.

**Severity:** Low for V0 (frame is operator-declared); Medium for S1-T
Autonomous tuning where the agent could influence frame values.

**Status:** Defer to S1-T when Autonomous tuning lands; document the
inline-verbatim semantics in BROKER-INTERNALS.md §1.3 frame substitution.

---

### F5. Per-broker materializer budget assumes equal segment cost

**Surface:** A10 divides remaining budget evenly across N non-governance
segments. But brokers have wildly different segment sizes — a broker with
1 Surfaced pipeline produces ~500 chars; a broker with 30 Surfaced
pipelines produces ~3000+ chars. Equal allocation under-uses the small
broker's allocation + over-truncates the large broker's.

**Real-world risk at IDE scale:** the IDE's IdeAction consolidation (C9)
will create one or two LARGE brokers (40+ pipelines on a handful of broker
homes) sharing budget with many SMALL brokers. The large brokers truncate
heavily; the small ones have room to spare.

**Recommendation:** S2-T per BB #20 Skill Filter (D3 primitive already
shipped) — the proper answer is relevance-ranked top-K, not even split.
Until S2-T, document the equal-split tradeoff so operators know IDE-scale
will need BB #20.

**Severity:** Medium for IDE scale; the A10 fallback is at least non-
catastrophic (vs V0's governance-only fallback which lost everything).

**Status:** Deferred to S2-T BB #20 Composer integration; documented in
PHASE-PROGRESS.md D3 row.

---

### F6. JSON Schema validator is a strict subset; pipelines authoring `oneOf` / `format` would silently lose enforcement

**Surface:** A11 validator handles `type`, `enum`, `required`. Other JSON
Schema constructs (`oneOf`, `anyOf`, `allOf`, `format`, `pattern`,
`minimum`, `maximum`, `minLength`, `maxLength`) are silently ignored.

**Real-world risk:** a broker author writes a sophisticated schema with
`"format": "uri"` thinking the runtime will reject malformed URIs — it
won't. The leaf-op might fail when actually parsing the URI but the
agent gets a less-helpful error.

**Recommendation:** add a `tracing::warn` at catalog load when the
validator encounters an unsupported JSON Schema construct. Operator sees
the warning + knows to either simplify the schema or wait for full
validator (which would land as a new dep on `jsonschema` crate when
needed).

**Severity:** Low. Pipelines whose schemas use only `type`+`enum`+`required`
work correctly today; everything else is best-effort. Most broker pipelines
in the plan use the simple subset.

**Status:** Defer; document in BROKER-INTERNALS.md §1.3 param-schema
section that the validator is a subset.

---

### F7. `BrokerHost::dispatch` doesn't expose the trust-budget consumption to the caller

**Surface:** `BrokerHost::dispatch` returns `Result<DispatchOutcome, DispatchError>`.
`DispatchOutcome` carries `trace_id`, `output`, `duration_ms`. It does NOT
expose the trust-budget state (used / ceiling). Operator-facing UIs that
want to show "you have 9457 / 10000 dispatches left" must call
`host.governance.trust_budget_state()` separately.

**Real-world risk:** the IDE's operator UI for showing dispatch
budgets is a likely Phase C concern. Two API surfaces (host.dispatch +
host.governance.trust_budget_state) is fine but worth documenting.

**Severity:** Low. Two-API split is correct (dispatch is hot path; budget
read is observability).

**Status:** No change; document the pattern in BROKER-INTERNALS.md §1.3
governance section.

---

### F8. Cluster manifest `frame.values` is `HashMap<String, Value>` — no validation, no schema

**Surface:** A12 ships `FrameConfig.values: HashMap<String, serde_json::Value>`.
The cluster manifest can declare arbitrary keys with arbitrary value types.
Frame substitution stringifies whatever it finds; pipelines that read
`{frame.X}` without checking get whatever the operator wrote.

**Real-world risk:** operator typos a frame key in cluster manifest;
pipeline silently sees an empty substitution + the precondition fails;
operator debugs "why is everything refusing" for an hour.

**Recommendation:** add a cluster manifest validation step (could be its
own pipeline) that lists every `{frame.X}` reference in every pipeline's
preconditions + cross-checks against the declared frame keys. Warn (not
error) on unknowns — operators may intentionally leave optional frame
keys undeclared. ~30 LOC + walk-the-catalog.

**Severity:** Low-Medium. Operator-experience concern; not a correctness bug.

**Status:** Defer to S1-T; flag in IDE-LIFT-TEMPLATES.md as something to
add when broker count gets large.

---

### F9. RateLimitSubgate uses `tokio::time::Instant` via `std::time::Instant`

**Surface:** A7's `RateLimitSubgate` imports `std::time::Instant`. Doc-
comment says "tokio::time::Instant for testability (test code can use
tokio::time::pause() + advance())" — but the code uses `std::time::Instant`.
Test that exercises window expiry uses `std::thread::sleep(80ms)`
instead of tokio pause/advance.

**Real-world risk:** the test takes 80ms of real wall-clock per run.
Across CI matrix runs the lag adds up. More importantly, the doc-comment
contradicts the implementation — future contributors may be confused.

**Recommendation:** decide one way. Option A (keep std::time, fix
doc-comment to reflect reality) — the simpler fix; tests stay
sleep-based. Option B (switch to tokio::time::Instant + use pause/advance
in tests) — faster tests + cleaner test discipline, but requires
`RateLimitSubgate` to be `async` (which it's not).

**Severity:** Low (cosmetic; tests pass).

**Status:** Quick fix to doc-comment; submit alongside F1+F2 follow-up.

---

### F10. Trace audit-completeness contract: `pipeline_id` is always recorded but `broker_id` may be empty for some refusals

**Surface:** A1/C8 fix made trace records fire on refusal branches too.
For PipelineNotFound (catalog lookup failed), the record carries
`broker_id = broker.id()` (we have the broker; just not the pipeline).
For other refusals (governance, precondition), same. **All trace records
have non-empty broker_id.** Confirmed by reading runner.rs.

**No issue found.** Audit completeness is correct.

**Status:** Closed — false alarm, contract holds.

---

## Severity rollup + recommended actions

| # | Finding | Severity | Recommendation |
|---|---|---|---|
| F1 | Subgate registration not idempotent | Medium | Follow-up commit: name-keyed HashMap |
| F2 | `bypasses_kill_switch` policy at registration | Medium | Follow-up commit: `register_with_catalog` validates |
| F3 | AuditOnly skips budget | Low | Doc-only |
| F4 | Frame string-substitution injection | Low (V0) | Defer to S1-T + doc |
| F5 | Equal-split materializer budget | Medium at scale | Defer to S2-T BB #20 |
| F6 | JSON Schema subset | Low | Doc + tracing::warn on unsupported |
| F7 | Two-API budget read | Low | Doc-only |
| F8 | Frame key validation | Low-Medium | Defer to S1-T |
| F9 | RateLimitSubgate doc/impl mismatch | Low | Quick doc-fix |
| F10 | Audit broker_id non-empty | (no issue) | Closed |

**Two follow-up commits worth landing inline** (F1 + F2 + F9): name-keyed
subgate registration with idempotency warning, `register_with_catalog`
validation, RateLimitSubgate doc-comment fix. ~30-50 LOC total + ~3
regression tests.

The rest defer to S1-T / S2-T with explicit doc entries flagging the
trade-offs. None of the findings block IDE-side Phase C migration; the
substrate surfaces are sound for the lift.

---

## Adversarial verdict on Phase A as a whole

**Phase A substrate is fit for Phase C IDE lift.** The findings above are
hardening opportunities, not showstoppers. The architecture (governance
composer + subgate slot mechanism + AuditOnly visibility + Frame + per-
broker budget allocation) holds up at the scrutiny the IDE lift will
apply.

The biggest single risk for Phase C is F5 (materializer scale at IDE
broker count) — but the V0 fallback is non-catastrophic + D3's Skill
Filter primitive is ready for the S2-T Composer integration when needed.
