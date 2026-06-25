# Broker Scaffolding — Pre-Execution Gates

**Companion to:** `C:\Users\koff0\.claude\plans\for-your-new-session-modular-pretzel.md`
(approved plan for substrate-side workspace + sensory + extensions + A2A + Frame scaffolding)

**Purpose:** Resolve the six design questions the plan's ultra-pass flagged as
"must resolve before coding starts." Each gate produces a sketch / criterion / template
that subsequent implementation phases consume directly.

**Status:** All 6 gates closed; phases A.0 / A.1 / A.2 / A.3 / B / D may proceed.

---

## Gate 1 (U1) — V1 WorkspaceBroker concrete sketch

**Question:** What does the V1 WorkspaceBroker concrete impl look like? (The trait
shape must be driven by real impl needs, not designed in a vacuum.)

**Decision:** Sketch the concrete impl first; derive trait shape after.

### Struct sketch

```rust
// crates/neurogrim-brokers/src/workspace_broker.rs
pub struct WorkspaceBrokerV1 {
    id: String,
    project_root: PathBuf,
    overlay: Arc<Overlay<WorkspaceOverlay>>,
    awareness: Arc<RwLock<LocalAwareness>>,  // existing neurogrim-core type
    governance: Arc<GovernanceComposer>,
    extension_pipelines: Vec<Pipeline>,  // populated by apply_extension()
    extension_facts: Vec<ExtensionFact>,
    extension_terminal_recs: Vec<ExtensionTerminalRec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceOverlay {
    pub project_root: PathBuf,
    pub terminal_profile: TerminalProfile,
    pub path_conventions: PathConventions,
    pub current_focus: Option<String>,
    pub active_processes: Vec<TrackedProcess>,
    pub facts_count: usize,
    pub notes_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalProfile {
    pub primary_shell: String,      // "powershell" | "bash" | "zsh"
    pub available_tools: Vec<String>,  // ["git", "cargo", "npm", ...]
    pub os: String,                 // "windows" | "linux" | "macos"
    pub gotchas: Vec<String>,       // operator-curated + agent-recorded
}
```

### 14 pipeline declarations

**Sense pipelines (Internal — agent reads, doesn't dispatch):**

| Pipeline | Params | Returns | Impl strategy |
|---|---|---|---|
| `workspace/get-terminal-profile` | none | `TerminalProfile` | Project from overlay; static facts cached at boot, agent-recorded gotchas appended |
| `workspace/get-path-conventions` | none | `PathConventions` (project_root, scratchpad, logs, artifacts, secrets, cmdb) | Project from overlay; defaults derived from project layout + operator overrides |
| `workspace/get-active-processes` | none | `Vec<TrackedProcess>` | Query `sysinfo` for child PIDs of this Brain's process group + supplement with agent-recorded entries |
| `workspace/get-wip-state` | none | `{branch, ahead, behind, modified_count, untracked_count}` | Shell out to `git status --porcelain` + `git rev-list --count`; cache 5s |
| `workspace/get-build-invariants` | none | `Vec<{name, command, description}>` | Operator-curated via extensions + sensible defaults (cargo check, npm run check) |
| `workspace/list-child-projects` | none | `Vec<ChildProject>` | Parse `.gitmodules` + brain-registry.json A2A peers + extension entries |
| `workspace/get-capability-profile` | none | `{registered_brokers: [...], registered_sensors: [...], a2a_peers: [...]}` | Query the BrokerHost's registry + factory list |
| `workspace/get-current-focus` | none | `Option<String>` (current epic/task description) | Read from overlay; mutated by `update-focus` |

**LocalAwareness facet pipelines (Surfaced — operator + agent mutate facts):**

| Pipeline | Params | Returns | Impl strategy |
|---|---|---|---|
| `workspace/set-fact` | `{key: str, value: str, category: enum, note?: str}` | `{ok: true, key}` | Delegates to `LocalAwareness::upsert_fact` (existing); broker writes disk first, then overlay swap (two-write coherence per existing PoC) |
| `workspace/add-note` | `{content: str, category: enum}` | `{ok: true}` | Delegates to `LocalAwareness::add_note` (existing) |
| `workspace/remove-fact` | `{key: str}` | `{ok: true, removed: bool}` | Delegates to `LocalAwareness::remove_fact` (existing) |

**Agent-contribution InnateAbility pipelines (Surfaced — agent records back):**

| Pipeline | Params | Returns | Impl strategy |
|---|---|---|---|
| `workspace/record-terminal-recommendation` | `{pattern: str, recommendation: str}` | `{ok: true}` | Append to overlay's `terminal_profile.gotchas`; persist as a fact under category=Patterns |
| `workspace/record-active-process` | `{pid: u32, kind: str, description: str}` | `{ok: true, tracking_token: uuid}` | Add to overlay's `active_processes`; reaper checks PID liveness on next tick |
| `workspace/update-focus` | `{focus: str}` | `{ok: true}` | Set overlay's `current_focus`; persist as fact under category=General with key=`workspace.current_focus` |

### Trait shape derived from the impl

```rust
// crates/neurogrim-brokers/src/workspace.rs
pub trait WorkspaceBroker: Broker + Extensible {
    /// Project root this workspace broker is rooted at.
    fn project_root(&self) -> &Path;

    /// The 14 canonical pipeline IDs every V1 workspace broker must declare.
    /// Implementors append extension-added pipelines to this list.
    fn canonical_pipeline_ids() -> &'static [&'static str] {
        &[
            "workspace/get-terminal-profile",
            "workspace/get-path-conventions",
            "workspace/get-active-processes",
            "workspace/get-wip-state",
            "workspace/get-build-invariants",
            "workspace/list-child-projects",
            "workspace/get-capability-profile",
            "workspace/get-current-focus",
            "workspace/set-fact",
            "workspace/add-note",
            "workspace/remove-fact",
            "workspace/record-terminal-recommendation",
            "workspace/record-active-process",
            "workspace/update-focus",
        ]
    }
}
```

**Gate 1 outcome:** Trait is small (just `project_root()` + canonical pipeline IDs).
Per-Brain impls bring concrete state (`LocalAwareness` handle, overlay, governance);
substrate provides nothing beyond the trait contract + a base `WorkspaceBrokerV1` struct
NeuroGrim's own Brain can use as-is.

---

## Gate 2 (U2) — CmdbMaterializer path resolution

**Question:** How does `CmdbMaterializer` resolve the CMDB output path for each broker?

**Decision:** `cmdb_path()` is a 3-step resolution chain. Registry override wins;
broker default is fallback.

### Resolution algorithm

```rust
// crates/neurogrim-brokers/src/materializer/cmdb_writer.rs
impl CmdbMaterializer {
    fn resolve_cmdb_path(
        &self,
        broker: &dyn Broker,
        broker_id: &str,
        project_root: &Path,
        registry: &BrokerRegistry,
    ) -> Option<PathBuf> {
        // Step 1: Registry override.
        // If the operator's domain registry declares a custom scoring_source.path
        // for this broker's domain, honor it. This is HARD CONSTRAINT — existing
        // operators can have customized CMDB paths via brain-registry.json's
        // domain_definitions[<domain>].scoring_source.path.
        if let Some(custom_path) = registry.scoring_source_path_for(broker_id) {
            return Some(project_root.join(custom_path));
        }

        // Step 2: Broker-declared default.
        // Broker can override the trait default by implementing its own cmdb_path().
        // SensoryBroker's default is `<project_root>/.claude/<broker_id>-cmdb.json`
        // (matches the legacy sensor path contract).
        if let Some(declared) = broker.cmdb_path() {
            return Some(project_root.join(declared));
        }

        // Step 3: No CMDB.
        // Broker doesn't export a CMDB (e.g., workspace broker). Materializer
        // skips it for this broker.
        None
    }
}
```

### New `Broker` trait method (default)

```rust
// crates/neurogrim-brokers/src/broker.rs — additive
pub trait Broker: Send + Sync {
    // ... existing methods ...

    /// Broker-declared CMDB output path, relative to project root.
    /// Default: None (broker doesn't export a CMDB).
    /// SensoryBroker overrides to return its canonical sensor CMDB path.
    fn cmdb_path(&self) -> Option<PathBuf> {
        None
    }
}
```

### Registry accessor (additive)

```rust
// crates/neurogrim-brokers/src/registry.rs — additive
impl BrokerRegistry {
    /// Look up an operator-customized CMDB path for a broker.
    /// Consults the underlying brain-registry.json (if loaded as part of
    /// host config) and returns the relative path string declared at
    /// `domain_definitions.<broker_id>.scoring_source.path`.
    /// Returns None if no override exists.
    pub fn scoring_source_path_for(&self, broker_id: &str) -> Option<&str> {
        self.domain_definitions
            .get(broker_id)
            .and_then(|d| d.scoring_source.path.as_deref())
    }
}
```

**Gate 2 outcome:** Registry override is consulted FIRST; broker default is fallback.
External CMDB consumers (scoring engine, dashboard, MCP, A2A) see no path change
regardless of which broker writes the file. Operators who customized paths via
brain-registry.json keep their customizations.

---

## Gate 3 (U3) — Sensory Queue enforcer V1 scope

**Question:** What's in V1 of the BB #18 Sensory Queue enforcer vs deferred to V2?

**Decision:** V1 = rate limit + schema validation. V2 = redaction.

### V1 scope (in this plan)

```rust
// crates/neurogrim-brokers/src/sensory_queue.rs
pub struct SensoryQueueEnforcerV1 {
    rate_limits: HashMap<SourceId, SlidingWindow>,
    schema: &'static jsonschema::JSONSchema,  // cmdb-envelope-v1
}

pub struct EnforceResult {
    pub allowed: bool,
    pub refusal_reason: Option<RefusalReason>,
    pub source_id: SourceId,
    pub timestamp: DateTime<Utc>,
}

pub enum RefusalReason {
    RateLimit { window: Duration, max: u32, current: u32 },
    SchemaInvalid { field: String, message: String },
    UnknownSource,
}

impl SensoryQueueEnforcerV1 {
    /// Called by Tier 1 sensor extensions + Tier 2 operator-authored sensors
    /// before they write their CMDB payload. Built-in sensors (the 26 in
    /// neurogrim-sensory) are pre-trusted and bypass the enforcer.
    pub fn enforce(
        &mut self,
        source: SourceId,
        payload: &Value,
    ) -> EnforceResult {
        // Step 1: Rate limit check
        if !self.check_rate_limit(&source) {
            return EnforceResult::refused(RefusalReason::RateLimit { ... });
        }

        // Step 2: Schema validation against cmdb-envelope-v1
        if let Err(e) = self.schema.validate(payload) {
            return EnforceResult::refused(RefusalReason::SchemaInvalid { ... });
        }

        // Step 3: Allowed
        EnforceResult::allowed(source)
    }
}
```

### Rate limit config (per source)

```toml
# Default rate limits; overridable per-source via cluster.toml
[sensory_queue.default_limits]
window_seconds = 60
max_writes = 12
```

### V1 explicit non-goals (documented in code)

```rust
/// V1 LIMITATION: This enforcer does NOT perform payload redaction
/// (secret pattern stripping, PII detection). V2 adds:
///   - operator-configurable redaction rules
///   - built-in secret pattern library (API keys, AWS access keys, etc.)
///   - schema-aware PII scrubbing
///
/// Until V2 ships, operators authoring sensor extensions are responsible
/// for ensuring their sensors don't emit secrets in CMDB payloads.
/// Document this in BROKER-AUTHORING.md.
```

### V2 deferral note

V2 redaction adds substantial scope (pattern library, operator config, schema-aware
detection). V1 ships without it so the broader sensory broker pattern can land + be
validated. V2 schedule: post-A.2; operator decides when redaction becomes load-bearing.

**Gate 3 outcome:** V1 enforcer scope = rate limit + schema validation. Saves ~1d on
A.2.5. Redaction is explicitly V2; documented in code + authoring guide. Built-in
sensors bypass the enforcer (pre-trusted).

---

## Gate 4 (U9) — Pilot sensor validation criteria

**Question:** What concrete checklist must the pilot sensor (`secret-refs`) satisfy
before A.2.4 bulk migration starts?

**Decision:** 9-item acceptance checklist. All items pass → bulk migration unblocks.

### Pilot validation checklist (`secret-refs` migrated as `SecretRefsBroker`)

| # | Check | Verification | Pass criteria |
|---|---|---|---|
| 1 | CMDB file path identical to legacy | `ls .claude/secret-refs-cmdb.json` before/after | Path string byte-identical |
| 2 | CMDB JSON output byte-identical | `diff <(legacy_path_output) <(broker_path_output)` | Zero diff (sort-stable keys; timestamp tolerance) |
| 3 | Scoring engine reads correctly | `neurogrim score --domain secret-refs` | Returns same score as legacy |
| 4 | Dashboard displays correctly | HTTP GET `/api/brains/<id>/domains/secret-refs` | Same findings + score JSON |
| 5 | CLI `neurogrim cast secret-refs` works | Run CLI | Exits 0; emits CMDB to stdout |
| 6 | MCP `brain_query` succeeds | Test MCP client query for secret-refs domain | Returns identical AgentOutput |
| 7 | At least one Tier 1 sensor extension successfully extends sensory | Author + register a `file_presence_score` extension; dispatch | Extension's sensor runs; CMDB written; enforcer accepts payload |
| 8 | Sensory Queue enforcer rejects malformed payload | Author broken extension with invalid envelope; attempt write | Enforcer returns SchemaInvalid; CMDB not written |
| 9 | All 8 existing CMDB regression tests pass | `cargo test --workspace --all-targets` | No test failures; no new warnings beyond pre-migration baseline |

### Byte-identity tolerance rules

- **Timestamp fields** (`meta.updated_at`, top-level `updated_at`): compared by ISO8601 parseability + within-5s tolerance (sensors regenerate timestamps every run)
- **Order-sensitive fields** (`findings` array order, etc.): expected stable; if order differs, INVESTIGATE before accepting
- **Float scores**: exact equality required (sensors are deterministic)

### Sequencing rule

A.2.4 bulk migration of remaining 25 sensors **does not begin** until all 9 pilot
checks pass. If any check fails, fix the pattern in A.2.2 + re-run all 9 checks
before proceeding. Each subsequent sensor migration uses the same 9-check template
(see Gate 6).

**Gate 4 outcome:** 9-item checklist defined. Bulk migration is sequenced behind
pilot success. Each sensor migration commit references the checklist as its
acceptance bar.

---

## Gate 5 (U6) — cereGrim public/private artifact split

**Question:** How does A.3's cereGrim consumption work respect the
public-NeuroGrim / private-cereGrim IP boundary?

**Decision:** A.3 produces TWO artifact streams; each finding routes to the
appropriate stream based on whether it's general-substrate or cereGrim-specific.

### The boundary rule

- **PUBLIC (NeuroGrim repo)**: Substrate design, broker traits, extension primitives,
  pipeline shapes, materializer extensions, manifest schemas. Any change to the
  substrate that benefits "any future consumer."
- **PRIVATE (cereGrim repo)**: Anything that reveals cereGrim's specific cost/reliability
  thesis, dual-lobe coordination semantics, Grimoire persistent-cache design,
  decision-loop bounding strategy, or any proprietary lever the Grimoire Thesis
  articulates. Per `D:\Brains\NeuroGrim\docs\PUBLIC-VS-PROPRIETARY.md`.

### A.3 artifact routing per substrate finding

| Finding type | Public artifact | Private artifact |
|---|---|---|
| "cereGrim's Workspace Broker needs X feature; substrate currently lacks it" | PR to neurogrim-brokers adding X, justified by "general broker authoring" | Memo noting cereGrim's specific use case |
| "cereGrim's Sensory Broker can consume substrate as-is" | (none — no substrate change needed) | Memo confirming fit |
| "cereGrim wants a specialized broker_type for Y" | (none — Tier 2 broker registration is already supported) | Memo + cereGrim-internal broker_type spec |
| "cereGrim's Topology Broker needs a `BrokerTopology` introspection primitive" | PR adding `BrokerTopology` query API; justified by "general operator introspection needs" | Memo confirming the substrate-side primitive unblocks cereGrim's Topology Broker |

### Required wording disciplines

When authoring NeuroGrim PRs that resulted from A.3 findings:
- **DO**: cite general design principles ("operators need broker introspection")
- **DO NOT**: cite cereGrim ("cereGrim's Topology Broker needs this")
- **DO NOT**: cite the Grimoire Thesis, dual-lobe semantics, or any proprietary lever
- **Grep test** (from NeuroGrim CLAUDE.md): "cost thesis", "load-bearing lever",
  "punches up", "the whole point", "every deterministic decision absorbed" should
  return zero matches in any public NeuroGrim PR

### A.3 deliverable structure

```
D:\Brains\NeuroGrim\               # PUBLIC
└── (any substrate-gap PR with general justification)

D:\Brains\cereGrim\docs\           # PRIVATE
├── workspace-broker-substrate-fit.md
├── sensory-broker-substrate-fit.md
├── topology-broker-substrate-fit.md
├── context-broker-substrate-fit.md
└── work-broker-substrate-fit.md
```

Each private memo follows the template:
```markdown
# <Broker> Substrate-Fit Memo

## cereGrim need
(cereGrim-specific requirements; can reference Grimoire Thesis)

## Substrate fit assessment
- Status: ready-to-consume | substrate-gap | cereGrim-bespoke

## Substrate gap (if any)
- General-design articulation: (what would benefit ALL consumers)
- Public PR: (link to NeuroGrim PR)

## cereGrim-bespoke implementation (if any)
- Will be authored in cereGrim's runtime when D-* gates clear
```

**Gate 5 outcome:** Two-stream artifact discipline defined. Public/private boundary
is enforceable via grep tests. Substrate-gap PRs have general justification; cereGrim
specifics stay private.

---

## Gate 6 (U10) — Sensor migration byte-identity test template

**Question:** How is per-sensor CMDB byte-identity verified during A.2.4 bulk migration?

**Decision:** Per-sensor integration test using shared `assert_cmdb_byte_identical!`
macro. Test file generated from template; one per migrated sensor.

### Template (clone per sensor)

```rust
// crates/neurogrim-sensory/tests/byte_identity_<sensor_name>.rs
//
// Sensor migration byte-identity regression test.
// Auto-generated from template at docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md
// per Gate 6.

use neurogrim_sensory::{analyze_<sensor_name>, brokers::<SensorName>Broker};
use neurogrim_brokers::{BrokerHost, BrokerHostConfig};
use serde_json::Value;
use tempfile::TempDir;

#[tokio::test]
async fn cmdb_output_byte_identical_legacy_vs_broker() {
    // 1. Set up a fixture project dir
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path();
    setup_fixture_project(project_root);  // crate-shared helper

    // 2. Generate CMDB via legacy path
    let legacy_cmdb: Value = analyze_<sensor_name>(project_root.to_str().unwrap())
        .await
        .expect("legacy sensor failed");

    // 3. Generate CMDB via broker path
    let host = boot_test_host_with(<SensorName>Broker::new(...)).await;
    let _outcome = host.dispatch("<sensor-name>", "<sensor-name>/run-sensor",
                                  serde_json::Map::new()).await.unwrap();
    let broker_cmdb_path = project_root.join(".claude/<sensor-name>-cmdb.json");
    let broker_cmdb: Value = serde_json::from_str(
        &std::fs::read_to_string(&broker_cmdb_path).expect("broker didn't write CMDB")
    ).unwrap();

    // 4. Assert byte-identity (with timestamp tolerance)
    assert_cmdb_byte_identical!(legacy_cmdb, broker_cmdb);
}

fn setup_fixture_project(root: &std::path::Path) {
    // Per-sensor fixture setup. Most sensors need:
    //  - .claude/ dir
    //  - a git repo (init + commit)
    //  - sensor-specific fixtures (e.g., secret-refs needs SECRETS.md)
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    std::process::Command::new("git").arg("init").current_dir(root)
        .output().expect("git init");
    // ... per-sensor additions
}
```

### Shared `assert_cmdb_byte_identical!` macro

```rust
// crates/neurogrim-sensory/tests/common/mod.rs
#[macro_export]
macro_rules! assert_cmdb_byte_identical {
    ($legacy:expr, $broker:expr) => {{
        let legacy = $legacy.clone();
        let broker = $broker.clone();

        // Strip timestamp fields (tolerable within ±5s)
        let mut legacy_stripped = legacy.clone();
        let mut broker_stripped = broker.clone();
        strip_timestamp_fields(&mut legacy_stripped);
        strip_timestamp_fields(&mut broker_stripped);

        // Assert structural equality
        assert_eq!(
            legacy_stripped, broker_stripped,
            "CMDB output differs between legacy sensor and broker path\nLEGACY:\n{}\nBROKER:\n{}",
            serde_json::to_string_pretty(&legacy).unwrap(),
            serde_json::to_string_pretty(&broker).unwrap(),
        );

        // Independently check timestamp fields are within tolerance
        assert_timestamps_within_tolerance(&legacy, &broker, std::time::Duration::from_secs(5));
    }};
}

fn strip_timestamp_fields(v: &mut serde_json::Value) {
    // Recursively remove `updated_at`, `meta.updated_at`, `discovered_at` fields
    // Detailed impl in common/mod.rs
}

fn assert_timestamps_within_tolerance(
    legacy: &serde_json::Value,
    broker: &serde_json::Value,
    tolerance: std::time::Duration,
) {
    // Parse timestamps from each; assert within tolerance
}
```

### A.2.4 migration template

For each of the 25 remaining sensors:
1. Create `neurogrim-sensory/src/brokers/<sensor_name>_broker.rs` (broker wrapper)
2. Clone the test template above → `neurogrim-sensory/tests/byte_identity_<sensor_name>.rs`
3. Customize `setup_fixture_project` for the sensor's fixture requirements
4. Run `cargo test byte_identity_<sensor_name>` → must pass
5. Register the broker as a factory in NeuroGrim's broker host config
6. Commit with message format: `feat(sensory): broker migration — <sensor_name>`

### Special-case sensors (per ultra-pass U4)

- **`coherence`**: reads OTHER sensors' CMDBs. Fixture must include 2+ pre-written
  CMDBs from sibling sensors. Test takes ~6h instead of 3h. Document specifically.
- **`agent-behavior`, `docker-topology`, `git-health`** (fallible sensors): test must
  also cover the error path (intentionally break the fixture; assert broker returns
  the same `Err` shape the legacy path does).

**Gate 6 outcome:** Test template + macro defined. Per-sensor migration is mechanical
(clone template, customize fixture, verify byte-identity). Coherence + fallible sensors
get special-case notes.

---

## Summary — all 6 gates closed

| Gate | Question | Status |
|---|---|---|
| 1 (U1) | V1 WorkspaceBroker concrete sketch | ✅ Sketch + 14 pipelines + trait shape defined |
| 2 (U2) | CmdbMaterializer path resolution | ✅ 3-step chain: registry override → broker default → None |
| 3 (U3) | Sensory Queue enforcer V1 scope | ✅ Rate limit + schema validation in V1; redaction in V2 |
| 4 (U9) | Pilot sensor validation criteria | ✅ 9-item checklist; bulk migration gated behind pilot pass |
| 5 (U6) | cereGrim public/private boundary | ✅ Two-stream artifact discipline; grep test enforced |
| 6 (U10) | Sensor migration byte-identity test | ✅ Template + macro defined; per-sensor mechanical |

## What unblocks now

With all 6 gates closed, the implementation phases may proceed in plan order:

- **A.0** (substrate primitives) — extension registry, workspace trait derived from
  Gate 1 sketch, CmdbMaterializer per Gate 2, default-cluster prep
- **A.1** (V1 workspace broker) — implement the Gate 1 sketch as the canonical
  `WorkspaceBrokerV1`; absorb LocalAwarenessBroker
- **A.2** (sensory broker pattern + bulk migration) — Gate 4 pilot first, then Gate 6
  template for the bulk; Gate 3-scoped enforcer
- **A.3** (cereGrim design validation) — Gate 5 two-stream artifacts
- **B** (default-cluster lift) — straightforward
- **D** (Frame stack in real use) — straightforward
- **C** (A2A as broker dispatch) — separate scoping pass per plan

Realistic effort estimate: **~7-9 weeks single-developer, ~9-11 weeks calendar.**
