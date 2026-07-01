---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Domains

A domain is the unit of concern in LSP Brains. Each domain owns a single aspect of project health,
produces a numeric score (0–100), and publishes named variables that other domains and the Brain
engine can read. Domains are composable: plug in only what matters for your project, weight them
to match your risk model, and let the Brain surface the view that guides each agent decision.

This document covers:

1. [Why Domains Matter](#why-domains-matter)
2. [Weight Tiers](#weight-tiers)
3. [Built-In Domains](#built-in-domains)
4. [Potential Domains](#potential-domains)
5. [Writing Your Own Domain](#writing-your-own-domain)

---

## Why Domains Matter

A healthy project is not a single number — it is a collection of concerns that interact. Test
coverage is meaningless if CI is broken. Security posture means nothing if secrets leak through
tracked files. Deployment readiness depends on whether git is clean.

Domains solve this by separating concerns cleanly:

- Each domain scores one thing and publishes named variables for that thing.
- The Brain engine aggregates weighted domains into a unified health score.
- The coherence domain reads cross-domain correlations and flags when signals conflict.
- Agents query the Brain before acting, so every decision is grounded in the current state.

The result is a project-awareness layer that scales from a solo developer to a multi-agent
pipeline. Teams adopt the domains relevant to their stack, extend them with custom sensory tools,
and let the Brain make the invisible visible.

---

## Weight Tiers

Domains fall into two tiers: **weighted core** and **advisory**.

| Tier | Weight | Effect on unified score | Typical use |
|------|--------|------------------------|-------------|
| Weighted core | > 0.0 | Contributes directly | Concerns that gate releases or block agents |
| Advisory | 0.0 | Visible in health output, does not pull score | Informational signals, experimental domains, user-specific concerns |

Weights across all core domains must sum to 1.0. Advisory domains are shown alongside the unified
score but excluded from the calculation until you promote them by assigning a weight in
`brain-registry.json`.

The three built-in core domains ship with a default weighting that reflects a typical delivery
pipeline: test health carries the most weight because broken tests block everything downstream,
code quality is second because lint and formatting failures accumulate silently, and deploy
readiness is third because it catches the environment-level gaps that prevent actual shipment.

You can override any weight in your `brain-registry.json`. Promoting an advisory domain to core
is as simple as assigning it a non-zero weight and rebalancing the rest.

---

## Built-In Domains

### Quick Reference

| Domain | Tier | Default Weight | Concern |
|--------|------|---------------|---------|
| test-health | Core | 0.40 | Test existence, ratio, framework, failures |
| code-quality | Core | 0.35 | Lint config, formatting, editor standards |
| deploy-readiness | Core | 0.25 | CI, README, secrets hygiene, deploy config |
| git-health | Advisory | 0.0 | Uncommitted changes, staleness, stash count |
| rust-health | Advisory | 0.0 | Clippy, cargo audit, unused deps, MSRV |
| subagent-health | Advisory | 0.0 | Multi-agent task completion, protocol compliance |
| security-standards | Advisory | 0.0 | SECURITY.md, SAST, secret scanning |
| coherence | Advisory | 0.0 | Cross-domain correlation health |
| human-comms | Advisory | 0.0 | Agent-to-human communication model |
| secret-refs | Advisory | 0.0 | Credential reference catalog completeness |

---

### test-health

**Weight:** 0.40 (core) | **CMDB:** `.claude/test-health-cmdb.json`

test-health answers the most fundamental question about code quality: does the project have tests,
are they structured well, and are any of them failing right now? It detects test files across
common naming conventions, computes the ratio of test files to source files, identifies the test
framework in use, and reports the count of currently failing tests.

The scoring model rewards presence before perfection: you earn 40 points for having tests at all,
20 more if the test-to-source ratio clears a 0.1 threshold, and another 20 if a known framework
is detected. Each failing test deducts 10 points, clamped so the domain cannot go negative.
This means a project with no failures but thin coverage scores 80 — high enough to pass most
gates, but a visible signal that coverage investment is due.

| Variable | Meaning |
|----------|---------|
| `test-health:score` | Domain score 0–100 |
| `test-health:failing_tests` | Count of currently failing tests |
| `test-health:test_file_count` | Total detected test files |

---

### code-quality

**Weight:** 0.35 (core) | **CMDB:** `.claude/code-quality-cmdb.json`

code-quality tracks whether the project has the scaffolding in place to enforce consistent style
and catch common errors before they reach review. It looks for four configuration artifacts: a
lint config (ESLint, Clippy, Pylint, etc.), a formatting config (Prettier, rustfmt, Black, etc.),
an `.editorconfig` that pins whitespace and encoding rules, and a `.gitignore` that prevents
build artifacts and secrets from entering version control.

Each present config contributes 25 points, making the scoring linear and easy to reason about.
The domain does not run the linters — it confirms that the project has committed to running them.
That distinction matters: code-quality scoring says "this project enforces standards," which is
a weaker claim than a passing lint run but a stronger claim than silence.

| Variable | Meaning |
|----------|---------|
| `code-quality:score` | Domain score 0–100 |
| `code-quality:lint_errors` | Last-known lint error count (if sensory tool populated) |

---

### deploy-readiness

**Weight:** 0.25 (core) | **CMDB:** `.claude/deploy-readiness-cmdb.json`

deploy-readiness checks whether the project can actually be shipped. It looks at four categories:
CI configuration (GitHub Actions, GitLab CI, CircleCI, etc.), README presence (a minimum bar for
any deployable artifact), absence of secrets in tracked files, and deployment configuration (a
Dockerfile, Helm chart, serverless manifest, etc.).

The intent is to surface gaps that are invisible during development but become blockers at release
time. A developer can spend weeks on feature work without noticing that CI was never configured,
or that a credentials file crept into `.gitignore`-exempt paths. deploy-readiness catches those
gaps early and keeps them visible in the health score where agents and humans alike can see them.

| Variable | Meaning |
|----------|---------|
| `deploy-readiness:score` | Domain score 0–100 |

---

### git-health

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/git-health-cmdb.json`

git-health reflects the cleanliness of the working tree and the currency of the branch. It tracks
uncommitted changes, stash count, untracked files, and how far the current branch has drifted from
its upstream. A high stash count combined with many uncommitted changes often indicates a developer
who is context-switching faster than they are committing — a pattern that correlates with lost
work and merge conflicts downstream.

As an advisory domain, git-health never blocks a gate by itself, but it appears prominently in
health output and can participate in coherence correlations. For example, a coherence rule might
fire a warning when `git-health:uncommitted_changes > 10` and `deploy-readiness:score < 70`
are simultaneously true.

| Variable | Meaning |
|----------|---------|
| `git-health:score` | Domain score 0–100 |
| `git-health:uncommitted_changes` | Count of modified tracked files |

---

### rust-health

**Weight:** 0.0 (advisory) | **CMDB:** user-defined

rust-health is a language-specific advisory domain for Rust projects. It surfaces the clippy lint
count (errors and warnings separately), the number of CVEs reported by `cargo audit`, unused
dependency count from `cargo udeps`, and MSRV (minimum supported Rust version) compliance against
the declared `rust-version` field in `Cargo.toml`.

Rust projects often treat clippy warnings as soft failures during development and hard failures
at release. rust-health makes that policy explicit: an agent wearing a deploy hat can check
`rust-health:clippy_errors` against a gate before proceeding. The domain is advisory by default
because not all projects using the Brain engine are Rust projects — activate it by pointing its
sensory tool at your `cargo` output and assigning it a weight if you want it to gate releases.

---

### subagent-health

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/brain/subagent-health-cmdb.json`

subagent-health tracks the operational health of multi-agent workflows. It monitors task
completion rates across subagents, surfaces incomplete or timed-out tasks, and checks whether
subagents are adhering to the LSP Brains agent protocol (typed subagent interface, structured
JSON responses, gate compliance).

This domain is most relevant in orchestrated pipelines where an pilot agent spawns specialized
subagents for parallel work. If three of five subagents complete successfully but two time out,
subagent-health captures that partial-failure state and gives the pilot agent a clean signal
to retry, reroute, or escalate — rather than silently proceeding with incomplete context.

---

### security-standards

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/security-standards-cmdb.json`

security-standards scores the project's security posture documentation and tooling. It checks for
a `SECURITY.md` (vulnerability disclosure policy), SAST workflow configuration, secret scanning
enablement, and dependency scanning coverage. The scoring model is designed around the controls
that appear most frequently in SOC2 CC and ISO 27001 evidence requests.

Unlike a runtime security scanner, security-standards is a policy and configuration domain: it
confirms that the scaffolding for security automation is present and committed. Teams pursuing
formal compliance certification can use this domain to maintain continuous evidence that required
controls are in place, surfacing gaps before an audit rather than during one.

| Variable | Meaning |
|----------|---------|
| `security-standards:score` | Domain score 0–100 |

---

### coherence

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/coherence-cmdb.json`

coherence is the association cortex of the Brain. Individual domains stay single-concern by design
— test-health does not know about git-health, and neither knows about deploy-readiness. coherence
exists to name what their signals mean together.

It reads the `correlations` block of `brain-registry.json`, evaluates each rule against current
domain variable values, and fires alerts at three severity levels: critical (−35 pts), warning
(−20 pts), and info (−5 pts). A correlation might say: "if `test-health:failing_tests > 0` and
`deploy-readiness:score >= 80`, fire a critical alert — passing deploy checks with known test
failures is a dangerous state." Without coherence, both domains would report their individual
scores and nothing would surface the dangerous combination.

| Variable | Meaning |
|----------|---------|
| `coherence:score` | Domain score 0–100 |
| `coherence:correlations_evaluated` | Total rules evaluated this cycle |
| `coherence:correlations_fired` | Rules that matched and fired |
| `coherence:highest_severity` | Highest severity fired: critical / warning / info / none |

---

### human-comms

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/human-comms-cmdb.json`

human-comms is a persistent model of how a specific human wants agents to communicate. It is
structured as two-layer YAML: a user-scoped file at `~/.claude/human-comms.yaml` that captures
baseline preferences, and a project-scoped override at `.claude/human-comms.yaml` that can tune
those preferences for a specific codebase or team context.

The model tracks verbosity (terse / normal / verbose), preferred response format (bullets /
prose / mixed), code block style, lists-vs-prose preference, emoji policy, and per-hat overrides
so that a deploy hat can be more terse than a teaching hat. The project layer is safe to commit;
the user layer captures personal preferences and should stay out of version control.

Scoring is a completeness model: 25 points per fully defined block (communication, format,
signals, interaction) up to 100. A low score means agents are operating on incomplete preferences
and may produce responses that miss the mark for this user.

| Variable | Meaning |
|----------|---------|
| `human-comms:verbosity` | terse / normal / verbose |
| `human-comms:lead_with` | What agents lead responses with |
| `human-comms:code_blocks` | Code block style preference |

---

### secret-refs

**Weight:** 0.0 (advisory) | **CMDB:** `.claude/secret-refs-cmdb.json`

secret-refs is a safe credential reference catalog. Agents using it learn where secrets live and
how to access them — they never learn the values. The manifest at `.claude/secret-refs.yaml` is
safe to commit because it contains only references (provider type, secret name, access method) and
never values.

Built-in providers include GCP Secret Manager, AWS Secrets Manager, Azure Key Vault, HashiCorp
Vault, and environment variables. The Python SDK exposes `SecretProvider` and `SecretProviderSpec`
base classes for adding custom providers. The safety guarantee is positive containment: if a
secret is not in the manifest, the agent does not know it exists. This prevents agents from
hallucinating credential paths or attempting to read secrets that were not explicitly cataloged.

The scoring model rewards catalog completeness: 40 points for manifest existence, 20 for all
referenced secrets being described, 20 for rotation policy coverage, and 20 for scan coverage
(confirming that the manifest accounts for all env vars the sensory tool detected).

| Variable | Meaning |
|----------|---------|
| `secret-refs:has_manifest` | Whether `.claude/secret-refs.yaml` exists |
| `secret-refs:secrets_documented` | Count of documented secret references |
| `secret-refs:undocumented_env_vars` | Env vars detected but absent from manifest |

---

## Potential Domains

The built-in domains cover the core of a healthy delivery pipeline, but every team has concerns
that go further. The domains below are not included in the Brain engine — they are ideas to
illustrate what you can build. Each one maps cleanly to the domain contract: a sensory tool that
reads a data source, a scoring function, and named variables other domains and coherence rules
can reference.

### Infrastructure and Cloud

**terraform-health** — Tracks Terraform plan status, workspace drift, and state file freshness.
A stale plan combined with infra changes is a deployment risk that no amount of test coverage
will catch. Useful coherence rule: fire a critical alert when `terraform-health:drift_detected`
is true and `deploy-readiness:score >= 80`.

**kubernetes-health** — Pod readiness, deployment rollout status, resource limit utilization,
and PodDisruptionBudget compliance. In clusters with multiple environments, this domain gives
agents a real-time signal about whether the target environment can absorb a deployment.

**cloud-costs** — Budget alert status, cost trend direction, and anomaly detection against a
30-day baseline. Cost overruns are often invisible until the bill arrives; surfacing them as a
scored domain makes them a first-class concern alongside test health and deploy readiness.

**api-contracts** — OpenAPI/AsyncAPI spec freshness relative to implementation, and breaking
change detection against the last published spec version. APIs that drift from their contracts
break consumers silently; this domain makes contract compliance visible before release.

### Security and Compliance

**dependency-freshness** — Outdated dependency count, CVE count across the dependency graph,
and license compliance status. Combines what `cargo audit`, `npm audit`, and `pip-audit` surface
into a single scored signal.

**sca-health** — Software composition analysis coverage: SBOM generation status, SBOM component
count vs. detected packages (completeness), and known vulnerability counts at each CVSS tier.

**access-governance** — IAM policy drift detection, principle of least privilege adherence score,
and unused permission count. Particularly useful in multi-account cloud environments where IAM
sprawl accumulates between audits.

**compliance-posture** — SOC2, ISO 27001, or NIST CSF controls coverage and evidence freshness.
Scores whether required controls have current evidence attached, making continuous compliance
audit-ready rather than point-in-time.

### Development Process

**pr-hygiene** — PR size distribution, median review time, merge frequency, and description
completeness score. Large PRs with thin descriptions and long review times are a leading indicator
of integration friction. This domain makes that pattern legible before it becomes a velocity
problem.

**issue-tracker** — Stale issue count, sprint health (story points completed vs. committed), and
velocity trend over rolling windows. Connects delivery process health to the Brain engine so
agents can contextualize technical signals against delivery state.

**code-review-coverage** — Percentage of merges that received at least one approved review,
median review depth (comment count), and self-merge rate. A project where 40% of merges are
unreviewed has a fundamentally different risk profile than one at 95%.

**documentation-health** — README freshness (last modified relative to last code change),
API docs coverage against exported symbols, and changelog entry completeness. Documentation
debt is often invisible in daily work but compounds into onboarding and support costs.

**branch-strategy** — Long-lived branch count, stale branch count (no commits in N days),
and naming convention adherence rate. Repos with dozens of stale branches accumulate merge
conflicts and make it hard for agents to determine the canonical integration target.

### Performance and Quality

**performance-budget** — Frontend bundle size vs. defined budget, Lighthouse performance score,
and Core Web Vitals compliance. Performance regressions are easy to introduce and hard to notice
without a scored domain watching the numbers.

**error-rate** — Production error rate from APM or log aggregation, exception trend direction,
and MTTR for resolved errors. Bridges operational health back into the development workflow so
engineers see the production impact of their changes before the on-call rotation does.

**coverage-trend** — Test coverage percentage and its trajectory over the last N commits.
A coverage percentage alone is less informative than its direction: 72% trending upward is
healthier than 80% trending down.

**mutation-score** — Mutation testing effectiveness: the ratio of killed mutants to total
mutants. High line coverage with low mutation score means tests execute code but don't assert
its behavior — this domain surfaces that gap.

### Operational

**on-call-health** — Alert volume trend, MTTR distribution, and on-call rotation coverage
(no single person covering more than N% of shifts). High alert volume with long MTTR is a
burnout leading indicator; this domain makes it visible as a project health concern.

**incident-posture** — Open P1/P2 incident count, post-mortem completion rate (closed
incidents with a completed retro), and SLA breach rate. Teams with low post-mortem completion
rates repeat incidents; scoring completion makes the pattern visible.

**slo-health** — SLO error budget burn rate per service, reliability target compliance, and
budget-depleted service count. Gives agents a principled basis for throttling deployments when
error budgets are running low.

**pipeline-health** — CI/CD success rate over a rolling window, median build time and its
trend, and flaky test count. A pipeline that takes 45 minutes and fails 30% of the time shapes
developer behavior in ways that compound into larger quality problems.

### External Dependencies

**upstream-drift** — External API version pinning vs. latest available, SDK update lag in days,
and count of deprecated endpoint calls in the codebase. Dependencies that lag major versions
accumulate incompatibility debt that is expensive to pay down all at once.

**vendor-health** — Third-party service status page aggregation, recent incident count, and
contractual SLA compliance. When an external service is degraded, agents can factor that into
their decisions rather than treating failures as internal bugs.

**package-registry** — npm/PyPI/crates.io package ownership verification, vulnerability alerts
from package registry advisory feeds, and typosquatting proximity score for critical dependencies.

---

## Writing Your Own Domain

Custom domains follow the same contract as built-in ones: a sensory tool that reads a data source
and writes to a CMDB path, a scoring function that maps observations to a 0–100 score, and a set
of named variables that the Brain engine can read.

The Python SDK is the primary extension point. It exposes:

- `SensoryTool` — base class for tools that observe and write to a CMDB
- `ScoringFunction` — base class for functions that produce a domain score
- `SecretProvider` / `SecretProviderSpec` — base classes for custom credential providers
- `DomainSpec` — the manifest record that registers a domain in `brain-registry.json`

Start with a minimal domain: one sensory tool, one score variable, and one named variable that
a coherence rule can reference. Register it in `brain-registry.json` as advisory (weight 0.0),
observe it through a few development cycles, then promote it to core if it proves its value.

The LSP Brains specification at https://github.com/KeenanHoffman/LSP-Brains covers the full
domain contract, the sensory tool interface, and the CMDB schema in detail.
