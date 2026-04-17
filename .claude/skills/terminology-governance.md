# Terminology Governance

Validate and enforce canonical terminology across the LaaS codebase using the
terminology catalog and Find-TerminologySymbol.ps1 LSP tool.

Role: meta
Governs: scripts/dev/Find-TerminologySymbol.ps1, .claude/terminology-catalog.json, .claude/terminology-cmdb.json
Persona: architect

Trigger phrases: "terminology", "language alignment", "canonical term", "drift term",
Domain: brain, terminology
Methodology-step: skills
"term governance", "vocabulary", "naming convention", "hat persona pairing",
"check terminology", "terminology compliance"

---

## Quick Reference

| What you want | Command |
|---------------|---------|
| Full compliance scan | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Check -Plain` |
| Look up a term | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Name <term> -Plain` |
| List category terms | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Category <cat> -Plain` |
| Scan a single file | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -File <path> -Plain` |
| List all drift variants | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Drift -Plain` |
| Hat-persona pairing table | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Pairings -Plain` |
| Per-category statistics | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Stats -Plain` |
| Filter by severity | `pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Check -Severity error -Plain` |

---

## Terminology Categories

| Category | Canonical Terms | Governs |
|----------|----------------|---------|
| agent-context | hat, persona, mode | How agents orient: focus (hats) vs communication (personas) |
| system-state | CMDB, registry, topology | Data structures tracking infrastructure and config state |
| scoring | score, confidence, unified score | Numeric health assessment and data completeness |
| governance | gate, tier | Checkpoints and enforcement tiers |
| process | skill, chain | Documented procedures and multi-skill sequences |
| automation | hook, trigger | Event handlers and firing conditions |
| communication | agent, operator agent, subagent, consumer | System actors and information consumers |
| architecture | Brain, fractal, nervous system, sensory tool, methodology, implementation, absorption | Structural composition and methodology transfer |

---

## Key Distinctions

**Hat vs persona:** A hat changes *what you focus on* (domain emphasis multipliers). A persona
changes *how you communicate* (tone, priority ordering). They pair naturally but are distinct:
- operator hat + incident-commander persona
- security hat + security-auditor persona
- architect hat + architect persona

**Gate vs check:** A gate is a formal pass/fail checkpoint with expiry and tier. "Check" is
acceptable in compound nouns (health-check, preflight-check) but should not substitute for gate.

**Brain (capital B):** Always capitalize when referring to the system. Lowercase only in file
paths (`brain-registry.json`, `brain/`) and code identifiers.

---

## How to Add a New Term

1. Edit `.claude/terminology-catalog.json`
2. Add entry under `terms` with: canonical, category, definition, drift_variants
3. Run `Find-TerminologySymbol.ps1 -Check -Plain` to validate
4. If drift count is high, add exception patterns for legitimate uses

## How to Add an Exception

When a drift variant is legitimate in certain contexts:

1. Find the term in `terminology-catalog.json`
2. Add or update `exception_pattern` regex on the drift variant
3. Add `exception_note` explaining why the exception exists
4. Run `-Check` to verify the exception reduces false positives

---

## Brain Integration

The terminology domain is advisory (weight 0.00) — it appears in Brain health output but
does not affect the unified score. This is intentional: the domain tracks language governance
health without penalizing the overall system score during initial rollout.

The score and confidence appear in:
- `Find-Brain.ps1 -Mode health` (advisory section)
- `Find-Brain.ps1 -Mode score` (score line)
- `Find-Brain.ps1 -Mode agent` (domain map)

---

## See Also

- `hats.md` — hat definitions and domain emphasis multipliers
- `personas.md` — persona definitions and communication contracts
- `brain.md` — Brain scoring engine and tool registry
- `lsp.md` — LSP tool reference for all Find-*Symbol.ps1 tools
