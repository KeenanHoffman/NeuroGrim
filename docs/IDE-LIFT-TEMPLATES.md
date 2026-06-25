# IDE Lift — Phase C Broker Templates

Reference templates for IDE-side Phase C sub-phases. These are NOT shipped
code — they're patterns the operator can copy into the IDE repo at
`D:\local-pc-operational-management\children\neurogrim-ide\src-tauri\src\`
after wiring the substrate dep (per `PHASE-PROGRESS.md` "IDE-side dep
prerequisite" section).

Each template shows the substrate-using shape; the operator translates
the existing IDE module into this shape using the strangler-fig pattern
(keep old code, add broker shim, switch call sites, delete old code one
release cycle later).

---

## C2 — `browser-kill-switch-broker`

Replaces `src/browser/kill_switch.rs`. The existing module reads
`LocalAwareness.facts["ide-browser-kill-switch-engaged"]` on every dispatch;
the broker version reads `GovernanceComposer::is_kill_switch_armed()` which
the substrate has owned since A1.

```rust
// src/brokers/browser_kill_switch.rs (NEW in IDE)
use neurogrim_brokers::{
    Broker, BrokerError, GovernanceComposer, LeafContext, LeafError, Overlay,
    Pipeline, Role, RoleSet, WorldEvent,
};
use neurogrim_brokers::governance::PreDispatchSubgate;
use neurogrim_brokers::pipeline::{AuditClass, EffectClass, Step, Tunability, Visibility};
use std::sync::Arc;

pub struct BrowserKillSwitchBroker {
    id: String,
    governance: Arc<GovernanceComposer>,
    overlay: Arc<Overlay<BrowserKillSwitchOverlay>>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BrowserKillSwitchOverlay {
    pub armed: bool,
}

impl BrowserKillSwitchBroker {
    pub fn new(id: impl Into<String>, governance: Arc<GovernanceComposer>) -> Self {
        let armed = governance.is_kill_switch_armed();
        Self {
            id: id.into(),
            governance,
            overlay: Arc::new(Overlay::new(BrowserKillSwitchOverlay { armed })),
        }
    }

    pub fn catalog(&self) -> Vec<Pipeline> {
        // Per A1: the canonical arm/disengage pipelines are framework-provided
        // (via GovernanceComposer::canonical_governance_pipelines). This
        // broker hosts ONLY the IDE-specific browser-kill-switch view; the
        // operator arms/disengages via the central governance pipelines.
        GovernanceComposer::canonical_governance_pipelines(&self.id)
    }
}

#[async_trait::async_trait]
impl Broker for BrowserKillSwitchBroker {
    fn id(&self) -> &str { &self.id }
    fn role_set(&self) -> RoleSet { RoleSet::single(Role::Sense) }

    async fn read_overlay(&self) -> serde_json::Value {
        serde_json::to_value(&*self.overlay.load()).unwrap_or(serde_json::Value::Null)
    }
    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        self.catalog().into_iter().filter(|p| matches!(p.visibility, Visibility::Surfaced)).collect()
    }
    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        GovernanceComposer::canonical_governance_pipelines(&self.id)
    }
    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        let armed = self.governance.is_kill_switch_armed();
        self.overlay.swap(BrowserKillSwitchOverlay { armed });
        Ok(())
    }
    async fn execute_leaf(&self, name: &str, _: LeafContext) -> Result<serde_json::Value, LeafError> {
        match name {
            "arm_kill_switch" => { self.governance.arm_kill_switch(); Ok(serde_json::json!({"armed": true})) }
            "disengage_kill_switch" => { self.governance.disarm_kill_switch(); Ok(serde_json::json!({"armed": false})) }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}
```

**Call site migration:** wherever IDE code previously called
`kill_switch::is_engaged(local_awareness)`, switch to
`governance.is_kill_switch_armed()` (or read the broker's Overlay if you
need the same atomic snapshot the agent sees).

---

## C3 — `browser-quotas-broker` (uses A7 `RateLimitSubgate`)

Replaces `src/browser/quotas.rs`. The existing module owns its own
`(pane_id, bucket)` sliding-window tables; the broker version registers
substrate `RateLimitSubgate` instances during host construction.

```rust
// During IDE host setup in main.rs:
use neurogrim_brokers::{BrokerHost, RateLimitSubgate};
use std::sync::Arc;
use std::time::Duration;

let host = BrokerHost::boot(cluster_path, config).await?;

// Per-pane navigation quota (was hard-coded 30/min in browser/quotas.rs)
host.governance.register_pre_dispatch_subgate(Arc::new(
    RateLimitSubgate::new(
        "browser-navigate-quota",
        "navigate",
        Duration::from_secs(60),
        30,
        Box::new(|pipeline| {
            // Extract pane_id from pipeline params via the catalog convention
            // (every browser pipeline carries pane_id in its params shape).
            pipeline.id.clone() // V0: per-pipeline; refine with pane_id in C9
        }),
    ),
));
```

**Note on C9 dependency:** the `pane_id` scope key isn't available until
the IdeAction consolidation (C9) lifts pane_id into the standard pipeline
param shape. Until then, C3 ships with per-pipeline scoping (looser than
the per-(pane,bucket) quotas the IDE has today, but the agent sees the
quota refusal correctly).

---

## C4 — `browser-admission-broker` (uses A8 `SystemPressureSubgate`)

Replaces `src/browser/admission.rs`. The existing module polls
`sysinfo::System::available_memory()` directly; the broker version wires
a `SystemFactsProvider` that calls sysinfo + a `SystemPressureSubgate`.

```rust
use neurogrim_brokers::{PressureTier, SystemFacts, SystemFactsProvider, SystemPressureSubgate};
use std::sync::Arc;
use sysinfo::System;

struct SysinfoFactsProvider {
    system: std::sync::Mutex<System>,
}

impl SystemFactsProvider for SysinfoFactsProvider {
    fn read(&self) -> SystemFacts {
        let mut sys = self.system.lock().unwrap();
        sys.refresh_memory();
        let free_mb = sys.available_memory() / 1024 / 1024;
        let tier = if free_mb > 2000 { PressureTier::Healthy }
                   else if free_mb > 1000 { PressureTier::Watch }
                   else if free_mb > 500 { PressureTier::Critical }
                   else { PressureTier::Refuse };
        SystemFacts { free_ram_mb: free_mb, cpu_load_pct: 0, tier }
    }
}

// During host setup:
let provider = Arc::new(SysinfoFactsProvider { system: std::sync::Mutex::new(System::new()) });
host.governance.register_pre_dispatch_subgate(Arc::new(
    SystemPressureSubgate::new("browser-admission", PressureTier::Watch, provider),
));
```

---

## C5 — Capability + batch-approval (uses A9 `CapabilitySubgate`)

Replaces `src/capability/mod.rs` + `src/capability/batch_approval.rs`.
Implement `CapabilityRegistry` consulting the IDE's existing
`v9-enforcement.json` matrix + batch-approval registry. Use Frame stack
(A13) to carry `confirmation_run_id` across multi-tick batch-approval
flows (which need D2 Workflow Engine when the run spans dispatches).

---

## C6 — `agent-permission-tokens-broker`

Replaces `src/agent/permission_tokens.rs`. Implement as governance leaf-ops:
`mint_token` + `consume_token`. `LeafContext.params["auth_token"]` flows
through; the broker maintains the (session_id, request_id) -> TokenEntry
HashMap. 10-minute TTL preserved.

---

## C7 — `agent-self-awareness-broker`

Replaces `src/agent_self_awareness.rs`. Sense-role broker that projects
the scoring overlay; CMDB write happens via `on_dispatch_complete` callback
registered during host construction. Plan §C7 + V0-RETRO §C1 (Composer is
the agent-facing interface) — CMDB writes become a Sense broker concern.

---

## C8 — `browser-overlay-broker` (IDE-only; uses A14 `Visibility::AuditOnly`)

Wraps `src/browser/overlay.rs`. Pipelines like `addHighlight`,
`clearHighlights` get `Visibility::AuditOnly` (from A14) — routed through
the broker dispatch ceremony for unified audit but NOT enumerated in
`current-projection.md` awareness (agent isn't supposed to see DOM
annotation as a choice).

---

## C9 — `IdeAction` consolidation (THE BIG ONE)

40+ enum variants → Surfaced pipelines on the appropriate brokers. Build
a scaffolder first: `cargo-neurogrim broker-scaffold --from-enum IdeAction`
(per plan §C9; the scaffolder lives in `neurogrim-cli/src/commands/`).

Each variant maps to a tuple of:
- visibility: `Surfaced` / `Internal` / `AuditOnly`
- broker_home: which broker hosts it (governance / capability / browser-* / etc.)
- leaf-op signature: standard `(LeafContext) -> Result<Value, LeafError>` if
  the variant doesn't need Tauri AppHandle / WebView2 access; if it does,
  the broker downcasts on an `IdeContextExt` trait (the broker-side
  Send+Sync trait that IDE-specific brokers consume).

Plan §C9 estimate: 8-12 days realistic. Anchor this as its own session.

---

## C10 — Dead-code removal sweep

One release cycle after each Phase C sub-phase ships: grep for the old
module's call sites; if zero non-test callers, delete the old module.

---

## How to use these templates

1. Resolve the IDE-side dep prerequisite (see PHASE-PROGRESS.md).
2. Pick a sub-phase (start with C2 — most isolated, smallest blast radius).
3. Create `src/brokers/<broker_name>.rs` in the IDE repo using the template.
4. Wire `BrokerHost::boot` into `src/main.rs` (Phase B3).
5. Register the new broker with the host.
6. Migrate call sites from the old module to the broker host (strangler
   shims: old call site = 3-5 line wrapper calling `host.dispatch()`).
7. After one release cycle with zero non-test callers, delete the old module.
