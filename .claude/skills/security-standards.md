# Security Standards Domain

Scans a project's file tree for compliance evidence across 5 SOC2 Common Criteria (CC)
control groups. Each finding is tagged with its SOC2 CC, ISO27001 Annex A, and NIST CSF
control IDs so teams can use output directly in audit evidence packages.

Role: operational · reference
Methodology-step: health

Trigger phrases: "security compliance", "soc2", "iso27001", "nist csf", "security posture",
Domain: security-standards
"compliance evidence", "security standards score", "run security standards",
"security-standards domain", "what security controls", "check security"

---

## What This Domain Scores

**Evidence density** — how much compliance evidence is visible in the project's file tree.

This is NOT:
- A live infrastructure audit (no cloud API calls, no runtime checks)
- A penetration test or vulnerability scan
- A guarantee of SOC2 certification readiness

A score of 0 means no evidence is visible in the file tree. It does not mean the project
is insecure. A score of 100 means all 5 control groups have detectable evidence — a strong
signal that security practices are formalized and documented.

---

## 5 Control Groups (20 pts each = 100 max)

### Group 1 — Vulnerability Disclosure Policy (20 pts)
**Files checked:** `SECURITY.md`, `.github/SECURITY.md`

Documents the process for reporting and handling security vulnerabilities.

| Framework | Controls |
|---|---|
| SOC2 CC | CC2.2, CC9.1 |
| ISO27001 | A.5.1 (Information security policies), A.6.8 (Information security event reporting) |
| NIST CSF | GV.RM-01 (Risk management policy), RS.CO-02 (Incident reporting) |

**Remediation:** Create `SECURITY.md` describing how to report vulnerabilities and how
you respond. GitHub displays this to reporters automatically.

---

### Group 2 — Secrets & Credential Management (20 pts)
**Files checked (presence):** `.env.example`, `.env.template`, `.env.sample`
**Files checked (content):** `.gitignore` for patterns: `*.pem`, `*.key`, `.env`,
`credentials`, `*.secret`, `*.pfx`, `*.p12`

- `.env.example` present → +10: credential template documents expected secrets
- `.gitignore` excludes secret patterns → +10: prevents accidental secret commits

| Framework | Controls |
|---|---|
| SOC2 CC | CC6.1 (Logical access — credential management) |
| ISO27001 | A.7.1 (Screening), A.8.20 (Networks security) |
| NIST CSF | PR.AA-01 (Identities managed), PR.DS-02 (Data-in-transit protected) |

**Remediation:** Add `.env.example` with placeholder values. Add secret file patterns
to `.gitignore`. Consider adding `gitleaks` or `trufflehog` to pre-commit hooks.

---

### Group 3 — Dependency Vulnerability Management (20 pts)
**Files checked (presence):** `.github/dependabot.yml`, `renovate.json`, `.renovaterc`,
`renovate.json5`, `.github/renovate.json`
**Workflow content patterns:** `audit`, `trivy`, `snyk`, `grype`, `pip-audit`,
`cargo audit`, `npm audit`, `yarn audit`, `osv-scanner`

- Dependency update tool configured → +10
- CI workflow runs dependency audit → +10

| Framework | Controls |
|---|---|
| SOC2 CC | CC7.1 (Vulnerability detection and remediation) |
| ISO27001 | A.7.3 (Information security awareness), A.8.8 (Management of technical vulnerabilities) |
| NIST CSF | ID.RA-01 (Asset vulnerabilities identified), PR.PS-06 (Software created securely) |

**Remediation:** Add `.github/dependabot.yml` for automated dependency PRs. Add a
workflow step running `trivy`, `cargo audit`, or `npm audit` in CI.

---

### Group 4 — Access Control & Change Authorization (20 pts)
**Files checked (presence):**
- `CODEOWNERS`, `.github/CODEOWNERS`, `.gitlab/CODEOWNERS` → +15
- `.github/workflows/*.yml` (any workflow file) → +5

CODEOWNERS defines who must approve changes to sensitive paths (access control +
segregation of duties). CI workflows demonstrate a systematic change deployment process.

| Framework | Controls |
|---|---|
| SOC2 CC | CC6.3 (Role-based access), CC8.1 (Change management authorization) |
| ISO27001 | A.5.3 (Segregation of duties), A.8.2 (Privileged access rights) |
| NIST CSF | PR.AA-05 (Access permissions managed), PR.PS-04 (Logs created) |

**Remediation:** Create `CODEOWNERS` or `.github/CODEOWNERS` mapping sensitive
directories to required reviewer teams. Example:
```
/terraform/   @org/infra-team
/src/auth/    @org/security-team
```

---

### Group 5 — Static Security Analysis / SAST (20 pts)
**Workflow content patterns:** `codeql`, `semgrep`, `sast`, `sonarqube`, `sonarcloud`,
`checkmarx`, `veracode`, `snyk code`, `bearer`, `horusec`

CI workflow runs a SAST tool → +20. The strongest single signal: automated code
scanning that detects vulnerabilities before they reach production.

| Framework | Controls |
|---|---|
| SOC2 CC | CC7.1 (Vulnerability detection) |
| ISO27001 | A.7.3 (Awareness), A.8.25 (Secure development lifecycle) |
| NIST CSF | PR.DS-08 (Integrity checking), DE.CM-01 (Networks monitored) |

**Remediation:** Add CodeQL (free for public repos, built into GitHub) via
`.github/workflows/codeql.yml`. Or add Semgrep, Sonar, or Snyk Code.

---

## Running the Tool

```bash
# Score the current project
neurogrim sensory run security-standards .

# Write CMDB (required before Brain reads the domain)
neurogrim sensory run security-standards . > .claude/security-standards-cmdb.json

# View score in health output
neurogrim health
```

The CMDB file at `.claude/security-standards-cmdb.json` is committed to git — it is
source-adjacent truth, not a runtime artifact.

---

## Interpreting the Score

| Score | Meaning |
|---|---|
| 0–19 | No compliance evidence visible — start with SECURITY.md and CODEOWNERS |
| 20–39 | Basic hygiene only — add dependency management and secrets controls |
| 40–59 | Partial evidence — key control (SAST or disclosure policy) still missing |
| 60–79 | Good posture — fill remaining gaps for complete SOC2 CC evidence set |
| 80–100 | Strong compliance evidence — all 5 control groups documented |

### For developers
Look at `findings` in the CMDB JSON — each missing finding has a `detail` field with
the exact remediation step and the control ID it satisfies.

### For compliance officers
The `control_references` map in the CMDB JSON lists all three framework control IDs
per group. Use it to populate audit evidence tables directly.

### For managers
`controls_evidenced` (0–5) is the headline number: how many of the 5 SOC2 CC control
groups have at least one piece of evidence in the codebase.

---

## Promoting to Weighted Domain

By default `security-standards` is advisory (`weight: 0.0`) — it appears in
`neurogrim health` but does not affect the unified score. To include it in scoring:

```json
// .claude/brain-registry.json
"domain_weights": {
  "test-health":        0.35,
  "code-quality":       0.30,
  "deploy-readiness":   0.20,
  "security-standards": 0.15,   // ← promote here
  "git-health":         0.0,
  "subagent-health":    0.0,
  "rust-health":        0.0
}
```

Recommended: promote to 0.10–0.15 once the team has resolved the first round of
findings and stabilized the score above 60.

---

## CMDB Field Reference

| Field | Type | Description |
|---|---|---|
| `score` | u8 | 0–100 compliance evidence score |
| `has_security_policy` | bool | SECURITY.md present |
| `has_secrets_management` | bool | At least one secrets control evidenced |
| `has_dependency_management` | bool | At least one dependency control evidenced |
| `has_codeowners` | bool | CODEOWNERS file present |
| `has_sast` | bool | SAST tool detected in CI workflow |
| `controls_evidenced` | u8 | Count of groups with any evidence (0–5) |
| `control_references` | object | SOC2/ISO27001/NIST CSF control IDs per group |
| `findings` | array | Per-check result with status, points, remediation detail |

---

## See Also

- `archived/brain.md` — how to read the Brain score and run health checks
- `archived/hats.md` — wear the `reviewer` hat to emphasize security findings in recommendations
- `archived/lsp-subagent-queries.md` — delegate security investigation to a subagent fleet
- `archived/what-next.md` — get a prioritized action list after running this domain
