# Everything as Code (EaC)

The principle that every artifact governing system behavior — infrastructure, configuration,
policy, documentation, security — should be version-controlled, peer-reviewed, and
machine-processable. This skill explains the "why" behind EaC, maps it to this project's
concrete implementation, and provides a decision framework for when to codify.

Role: philosophy

Trigger phrases: "everything as code", "EaC", "infrastructure as code philosophy",
Methodology-step: skills
"config as code", "policy as code", "documentation as code", "why is this in a file",
"why not just configure it manually"

---

## The EaC Principle

Every piece of system state that matters should live in a file, in a repository, under
version control. This is not about aesthetics — it creates four concrete guarantees:

1. **Auditability** — `git log` answers "who changed what, when, and why" for every
   system decision. No guessing, no "I think someone changed that setting last Tuesday."
2. **Reproducibility** — Any environment can be rebuilt from source. If the building
   burns down, checkout and apply. If a region goes dark, spin up in another.
3. **Review** — Every change goes through PR review before it takes effect. The blast
   radius of a mistake is limited to what a reviewer missed, not what someone fat-fingered.
4. **Automation** — Code can be linted, tested, validated, and applied by machines.
   CI/CD pipelines can enforce invariants that humans forget.

The alternative — manual configuration, wiki pages, tribal knowledge, ClickOps — fails
silently. You don't know what you don't know until 3 AM on a Saturday.

---

## The Five Pillars

EaC isn't just Infrastructure as Code. It's five pillars, each governing a different
class of system state.

### 1. Infrastructure as Code (IaC)

**What it covers:** Cloud resources, networking, IAM, DNS, load balancers, databases.

**In this project:** Terraform modules in `terraform/` with environment isolation via
state paths (`dev/`, `prod/`, `sandbox/<username>/`). Every Cloud Run service, IAP
backend, URL map rule, and service account is declared in `.tf` files.

**The test:** Can you recreate the entire infrastructure from a fresh GCP project by
running `terraform apply`? In this repo, yes.

### 2. Configuration as Code (CaC)

**What it covers:** Application configuration, feature flags, tool registries, metadata.

**In this project:**
- `brain-registry.json` — Tool registry with domain weights, data sources, correlations
- `test-gates.json` — Gate state machine with tiers, scopes, expiry, clear commands
- `artifact-cmdb.json` — Container image freshness metadata
- `deploy-state.json` — Environment deployment state

**The test:** If a config file is deleted, can you rebuild it from the schema + current
system state? The update scripts (e.g., `update-gate.ps1`, `update-artifact-cmdb.ps1`)
answer yes.

### 3. Policy as Code (PaC)

**What it covers:** Rules that govern what is allowed, blocked, or required before an
action can proceed.

**In this project:**
- Gate system (`test-gates.json`) — Defines which tests must pass before commit, merge,
  or deploy. `gate-advisor.ps1` enforces the policy.
- Pre-commit hooks (`install-hooks.ps1`) — Block commits that skip gate checks
- Tier system — `immediate`, `before-merge`, `pre-deploy`, `advisory` determine when
  each gate fires and what it blocks
- Claude Code hooks (`.claude/settings.json`) — Pre/post tool use hooks that enforce
  operational policies (SA liveness, destroy confirmation, upstream freshness)

**The test:** Can a new team member understand what's blocked and why, without asking
anyone? Read `test-gates.json` + `gate-advisor.ps1 -Action commit` output.

### 4. Documentation as Code (DaC)

**What it covers:** Operational knowledge, runbooks, decision records, skill definitions.

**In this project:**
- `CLAUDE.md` — Single source of truth for agent behavior, skill index, gate reference
- `.claude/skills/*.md` — 50+ structured skills with frontmatter, trigger phrases, procedures
- `skill-index.md` — Discovery map with categories and cross-references
- `apps/devops-whitepaper/index.html` — Published DevOps methodology

**The test:** Can the documentation be validated programmatically? Skills have structural
quality checks (`assess-skill-on-edit.sh`), and the skill index is kept in sync with
`CLAUDE.md`.

### 5. Security as Code (SaC)

**What it covers:** Authentication, authorization, encryption, access policies.

**In this project:**
- IAP configuration in Terraform — Who can access what, enforced by infrastructure
- WIF (Workload Identity Federation) — CI/CD auth without long-lived keys, declared in `.tf`
- Service account bindings — Principle of least privilege, codified in `foundation/` module
- Access topology (`access-topology.json`) — Snapshot of IAM state, diffable for drift

**The test:** Can you audit every access path from a single file? `access-topology.json`
+ `check-access-drift.ps1` answer yes.

---

## Truth Separation

EaC has a nuance: not everything declared-as-code belongs in git. The methodology recognizes
three distinct truth layers, each with different lifecycle rules:

### Source Truth

Lives in git. Committed, reviewed, authoritative. This is what humans declare:
Terraform configs, skills, hooks, scripts, brain-registry.json, test-gates.json.

**The test:** If you `git checkout` this file, do you get the authoritative version? If yes,
it's source truth.

### Runtime Truth

Lives in external systems (cloud state, pipeline results, issue trackers). Snapshotted into
local CMDBs by update scripts. The CMDB pattern bridges the gap:

```
External source → snapshot script → local JSON → LSP tool → Brain domain
```

**In this project:** artifact-cmdb.json (container image state from GCR), deploy-state.json
(Cloud Run revision state), topology JSONs (network/access state from GCP), Terraform state
(in GCS bucket). Each follows the same CMDB pattern.

**The test:** Does this data describe something external to source code? If yes, it's runtime
truth. Store the source externally; snapshot locally for speed.

### Derived Truth

Compiled on demand from source + runtime. Never committed, always reproducible. Gitignored.

**In this project:** tag-index.json (compiled from skill/hook/script metadata),
adoption-index.json (compiled from corpus analysis), chain-index.json (compiled from
skill-chain.md).

**The test:** Can you delete this file and regenerate it from existing source + runtime data?
If yes, it's derived truth. Don't commit it — compile it.

### Decision Heuristic

When classifying a new data artifact, ask:

| Question | Answer | Truth Layer |
|----------|--------|-------------|
| Is this declared by us? | Yes | Source — commit it |
| Is this observed from an external system? | Yes | Runtime — snapshot it via CMDB pattern |
| Is this computed from source + runtime? | Yes | Derived — compile it, gitignore it |

---

## Anti-Patterns

These are the failure modes EaC prevents:

| Anti-Pattern | What goes wrong | EaC alternative |
|-------------|----------------|-----------------|
| **ClickOps** | Cloud console changes with no record | Terraform + `check-drift.ps1` |
| **Snowflake environments** | "Works on my machine" / "Dev and prod are different" | Shared Terraform modules with env-specific state paths |
| **Wiki runbooks** | Stale day 1, no review process, no test | Skills with hooks + Pester tests |
| **Slack-channel config** | "Ask Jamie, she knows the setting" | CaC files with update scripts |
| **Manual access grants** | Forgotten service accounts, privilege creep | IAM in Terraform + access topology drift |
| **Undocumented hotfixes** | Fix deployed but not in code, drift on next apply | `hotfix-deploy.yml` with audit trail |

---

## EaC Decision Framework

Not everything needs to be codified immediately. Use these thresholds:

**Codify when:**
- The action is performed more than once
- The blast radius of getting it wrong is high (production access, billing, data)
- More than one person needs to understand or reproduce it
- The state needs to survive across sessions, machines, or team members

**Manual is OK when:**
- It's a one-time exploratory action (e.g., inspecting a log in Cloud Console)
- The result doesn't affect system state (e.g., reading metrics)
- The action is inherently interactive (e.g., debugging a live session)

**The progression:** Manual exploration → documented procedure → codified automation.
Most things start manual and graduate to code once the pattern stabilizes.

**Truth layer check:** When codifying, also ask which truth layer the artifact belongs to.
Source truth gets committed. Runtime truth gets a CMDB snapshot pattern. Derived truth gets
a compile script and a `.gitignore` entry.

---

## See Also

- `devops-philosophy.md` — The 8 core principles that EaC implements
- `deployment-flow.md` — How codified environments flow from local → sandbox → dev → prod
- `apply-infra.md` — IaC in action: the full deploy sequence
- `gate-system-overview.md` — PaC in action: the gate state machine
- `brain.md` — CaC in action: the brain registry and unified scoring
