# Hats System

Use this skill when you need to adjust the Brain's operational focus for a specific task.
A hat shifts which domains the Brain emphasizes in recommendations — the security hat
amplifies least-privilege and supply-chain signals; the operator hat amplifies gates and
artifacts. The unified score stays the same; only recommendation priority changes.

Role: meta

Trigger phrases: "hat", "wear the operator hat", "security focus", "architect focus",
"switch hat", "put on", "which hat", "hat suggestion", "recommend a hat"

Governs: scripts/dev/brain/correlation.ps1, scripts/dev/brain/modes-display.ps1, scripts/dev/brain/modes-agent.ps1
Domain: brain
Methodology-step: skills

---

## What a Hat Is

Hats are Step 6 of the LSP Brains adoption ramp — the consciousness layer that
makes the nervous system context-aware. Without hats, the operator agent treats all signals
with equal weight.

A hat is a focus lens for the same agent. It adjusts three things:

- **Domain emphasis** — which Brain domains get amplified in recommendations (multiplier on priority)
- **Operational focus** — what questions to ask first based on the hat's concern area
- **Memory tagging** — proposals made while wearing a hat are tagged for later recall

Hats do NOT change the unified score, confidence, or effective scores. They only
re-prioritize recommendations. Think of it as "same dashboard, different sort order."

Hats coexist with personas (see `personas.md`). Personas change *who you are*;
hats change *what you focus on*. The operator hat can be combined with the
incident-commander persona — they operate at different layers.

---

## Hat Catalog

| Hat | Focus | Amplified Domains | Suggested When |
|-----|-------|-------------------|----------------|
| `operator` | Deploy readiness and operational health | gates (x2.0), artifacts (x1.5), gitops-integrity (x1.5) | Deploy-blocking gates >= 2, or any artifact stale |
| `security` | Access control, supply chain, defense posture | least-privilege (x2.0), supply-chain (x2.0), defense-in-depth (x1.5) | Unreviewed existential binding, or IAM penalty > 30 |
| `architect` | Code quality, governance, structural health | everything-is-code (x2.0), defense-in-depth (x1.5), topology (x1.5) | 3+ expired-clean gates, or drift never run |

Domain emphasis multipliers are applied to recommendation priority scores. A domain with
emphasis x2.0 will have its recommendations sorted twice as high; x0.5 means half as high.

---

## How to Wear a Hat

### Via the Brain CLI

```powershell
# Run recommendations through the operator lens
pwsh -File scripts/dev/Find-Brain.ps1 -Mode recommend -Hat operator -Plain

# Agent mode with security focus
pwsh -File scripts/dev/Find-Brain.ps1 -Mode agent -Hat security -Plain

# Propose actions with architect emphasis
pwsh -File scripts/dev/Find-Brain.ps1 -Mode propose -Hat architect -Plain
```

### Announcement Convention

When wearing a hat, announce it visibly to the user on its own line:

```
Wear Hat: operator
```

When spawning a subagent that should wear a hat:

```
Subagent Wear Hat: security
```

Announce every time a hat is worn — not just the first time in a session. This makes hat
usage observable to the human operator, who can watch for patterns and judge hat selection.

### Mid-Session

Hats switch instantly — no ceremony needed. Just pass `-Hat` on the next invocation.
Previous hat state doesn't persist between invocations; the hat is always explicit.

### Hat Suggestion

The Brain evaluates domain variables and suggests a hat when signals are unambiguous:

```powershell
# Health mode shows hat suggestion at the bottom
pwsh -File scripts/dev/Find-Brain.ps1 -Mode health -Plain

# Agent mode includes suggested_hat in JSON output
pwsh -File scripts/dev/Find-Brain.ps1 -Mode agent -Plain | ConvertFrom-Json | Select-Object suggested_hat
```

The suggestion is non-intrusive — shown only when one hat clearly matches better than others.

---

## When to Wear a Hat

Hats are not optional accessories — they are how agents orient to a task. Reach for a hat
at the start of any focused workflow:

| Trigger | Hat | Why |
|---------|-----|-----|
| Before any `apply-*.ps1` or deploy dispatch | `operator` | Deploy-blocking gates and stale artifacts surface first |
| `suggest-security-auditor.sh` fires, or any IAM change | `security` | Least-privilege and supply-chain signals amplified |
| `suggest-architect.sh` fires, or writing/reviewing a plan | `architect` | EaC coverage and structural health amplified |
| Brain `-Mode health` shows a hat suggestion | (suggested hat) | Automatic trigger — the Brain detected unambiguous signals |
| Incident response (Phase 2: Assess) | `operator` | Prioritize deployment health during incidents |
| Access topology review | `security` | Amplify unreviewed bindings and IAM risk |
| Drift check remediation | `operator` or `architect` | Operator for infra drift, architect for governance drift |

**Default:** If no specific trigger applies, run without a hat (default emphasis). Hats are
for focused work, not background awareness.

---

## Hat-Aware Skill Usage

When a skill invokes `Find-Brain.ps1`, include the appropriate `-Hat` parameter. This is
the concrete mechanism that makes hats integral rather than optional.

**Pattern:** Where a skill says "run Brain recommend for priorities," the hat-aware version
specifies which hat:

| Skill | Brain invocation | Hat | Rationale |
|-------|-----------------|-----|-----------|
| `apply-infra.md` (Step 0) | `-Mode recommend -Hat operator` | operator | Surface deploy-blocking gates before applying |
| `incident-response.md` (Phase 2) | `-Mode health -Hat operator` | operator | Prioritize deployment health during incidents |
| `access-topology.md` (review) | `-Mode recommend -Hat security` | security | Amplify unreviewed bindings in recommendations |
| `drift-check.md` (remediation) | `-Mode recommend -Hat operator` | operator | Focus on gitops-integrity and gate signals |
| `post-deploy-verify.md` | `-Mode recommend -Hat operator` | operator | Verify deploy-critical signals after apply |
| `rollback-deployment.md` | `-Mode health -Hat operator` | operator | Confirm deployment health before rollback |

Skills not in this table use default emphasis (no hat). Teaching skills (`devops-for-developers.md`,
`setup.md`) intentionally omit hats to maintain breadth.

---

## Hat-Persona Pairing

Hats coexist with personas at different layers: personas change *who you are* (mindset,
tone); hats change *what you focus on* (domain priority). Certain pairings are natural:

| Hat | Natural persona pairings | Rationale |
|-----|--------------------------|-----------|
| `operator` | `incident-commander`, `rubber-duck` | Deploy-focus aligns with incident response; rubber-duck explains deploy state to newcomers |
| `security` | `security-auditor`, `adversary` | Security focus aligns with audit posture; adversary finds access gaps |
| `architect` | `architect`, `visionary` | Structure focus aligns with design; visionary explores with architecture lens |

When both a persona and hat are active, state them together:
```
> Persona: incident-commander | Hat: operator — assessing deploy health during incident.
```

Personas without a natural hat pairing (`lsp-reader`) use default emphasis.

---

## Hat Memory

When wearing a hat, proposal ledger entries are tagged with the hat name. This enables:

- **Hat-specific recall**: "What did we do last time as operator?" — filter recent_outcomes by hat
- **Cross-hat comparison**: See which hat was most effective at improving scores
- **Session continuity**: Resume where you left off with the same operational focus

Agent mode automatically filters recent_outcomes to the current hat when `-Hat` is specified.
Without `-Hat`, all outcomes are returned regardless of hat tag.

---

## Interaction with Other Systems

| System | Interaction |
|--------|-------------|
| Unified Score | NOT affected — hat emphasis only changes recommendation priority |
| Confidence | NOT affected — hats don't change confidence or effective scores |
| Proposal Ledger | Tagged with hat field when active |
| Recommendation Boosting | Stacks with hat emphasis (boost x emphasis) |
| Incident Patterns | NOT affected — patterns fire regardless of hat |
| Personas | Coexist — personas change mindset, hats change focus |

---

## See Also

- `personas.md` — persona system (who to be) vs hats (what to focus on)
- `brain.md` — full Brain skill reference
- `operational-memory.md` — query historical data including hat-tagged outcomes
