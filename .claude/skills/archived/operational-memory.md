# Operational Memory

Query historical operational data from GCS: score trends, incident history, gate clearance
patterns, and deploy outcomes.

Role: diagnostic . planning . retrospective
Governs: scripts/utility/Sync-BrainMemory.ps1

Trigger phrases: "what happened last deploy", "score history", "incident history",
Domain: brain
Methodology-step: skills
"gate clearance times", "how has health trended", "deploy outcomes", "when did this
last fail", "operational memory", "brain history"

---

## Score Trends

```powershell
pwsh -NonInteractive -File scripts/dev/Find-Brain.ps1 -Mode trend -Plain
```

Shows score trajectory over recent sessions with domain-level breakdowns, trend direction
(improving/declining/stable), delta, worst domain, and recurring incidents. Degrades
gracefully to `no-data` when GCS is unreachable.

## Incident History

```powershell
# List all incident records
pwsh -NonInteractive -File scripts/utility/Sync-BrainMemory.ps1 -Action list -Category incidents -Plain

# Download the most recent incident
pwsh -NonInteractive -File scripts/utility/Sync-BrainMemory.ps1 -Action latest -Category incidents -LocalPath ./latest-incident.json -Plain
```

Incident records contain: timestamp, matched pattern IDs, and commit SHA.

## Gate Clearance Patterns

```powershell
pwsh -NonInteractive -File scripts/utility/Sync-BrainMemory.ps1 -Action list -Category gate-ledger -Plain
```

Shows when gates were cleared, how long they took (`duration_seconds`), and which commit
they were cleared against. Use this to identify slow gates or recurring failures.

## Deploy Outcomes

```powershell
pwsh -NonInteractive -File scripts/utility/Sync-BrainMemory.ps1 -Action list -Category deploy-outcomes -Plain
```

Shows post-deploy score snapshots — answers "did this deploy improve or degrade health?"

## When to Use

- **Session start:** Run `-Mode trend` alongside `session-recap.md` to see score trajectory
- **Pre-deploy:** Compare current score against last deploy's post-score
- **Post-incident:** Check incident history for recurring patterns
- **Retrospective:** Analyze gate clearance times to optimize the development loop
- **Planning:** Use trend data to justify remediation priority in propose/plan modes

## GCS Categories

| Category | Retention | Written by |
|----------|-----------|-----------|
| scores | 90 days | `-Mode agent -Persist` |
| incidents | 365 days | `-Mode health` (when patterns fire) |
| gate-ledger | 90 days | `update-gate.ps1` |
| deploy-outcomes | 180 days | `health-check-after-apply.sh` |
| trends | 30 days | (future: aggregated trend snapshots) |
