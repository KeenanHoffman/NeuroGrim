# IDE Cluster Manifest — register all 19 brokers (C9 wire-up complete)

This is the cluster.toml + per-broker manifest set the IDE operator
drops into `<ide-project-root>/.claude/brain/broker/` after C9 wire-up
ships to make the BrokerHost actually register every broker on boot.

Until this manifest set lands in a real IDE project, the brokers exist
in code but the BrokerHost boots with zero brokers (broker-using
commands return "BrokerHost not initialized" for everything).

---

## `cluster.toml`

```toml
[cluster]
id = "neurogrim-ide-cluster"
name = "NeuroGrim IDE Broker Cluster"
brokers_dir = "./"

# C9a (mechanical batch)
[cluster.brokers.ide-state-sense]
manifest_path = "ide-state-sense.toml"
[cluster.brokers.window]
manifest_path = "window.toml"
[cluster.brokers.browser-dom-read]
manifest_path = "browser-dom-read.toml"
[cluster.brokers.browser-overlay]
manifest_path = "browser-overlay.toml"

# C9b (browser nav + screenshot bespoke)
[cluster.brokers.browser-nav]
manifest_path = "browser-nav.toml"
[cluster.brokers.browser-screenshot]
manifest_path = "browser-screenshot.toml"

# C9c (high-risk DOM + eval + intercept)
[cluster.brokers.browser-dom-write]
manifest_path = "browser-dom-write.toml"
[cluster.brokers.browser-eval]
manifest_path = "browser-eval.toml"
[cluster.brokers.browser-intercept]
manifest_path = "browser-intercept.toml"

# C9d (layout)
[cluster.brokers.layout]
manifest_path = "layout.toml"

# C9e (consolidated small brokers)
[cluster.brokers.agent]
manifest_path = "agent.toml"
[cluster.brokers.file]
manifest_path = "file.toml"
[cluster.brokers.ui-cards]
manifest_path = "ui-cards.toml"
[cluster.brokers.ui-state]
manifest_path = "ui-state.toml"
[cluster.brokers.tool-promotion]
manifest_path = "tool-promotion.toml"
[cluster.brokers.chrome]
manifest_path = "chrome.toml"
[cluster.brokers.tour]
manifest_path = "tour.toml"
[cluster.brokers.checkpoint]
manifest_path = "checkpoint.toml"
[cluster.brokers.federation]
manifest_path = "federation.toml"

[cluster.materializer]
composition_order = [
    # Operator surface (read-only Sense + small):
    "overlay-ide-state-sense",
    "overlay-federation",
    # Agent-facing surfaces:
    "awareness-routing-agent",
    "awareness-routing-file",
    "awareness-routing-ui-cards",
    "awareness-routing-ui-state",
    "awareness-routing-tool-promotion",
    "awareness-routing-window",
    "awareness-routing-layout",
    "awareness-routing-checkpoint",
    "awareness-routing-tour",
    "awareness-routing-chrome",
    # Browser surface:
    "awareness-routing-browser-nav",
    "awareness-routing-browser-screenshot",
    "awareness-routing-browser-dom-read",
    "awareness-routing-browser-dom-write",
    "awareness-routing-browser-eval",
    "awareness-routing-browser-intercept",
    # AuditOnly browser overlay (not enumerated in awareness routing
    # per A14; segment will be empty).
    "awareness-routing-browser-overlay",
]
output_path = "current-projection.md"
segments_dir = "segments"
context_budget_chars = 65536  # IDE-scale: 19 brokers × ~3KB avg = ~57KB

# A13/A12 — operator-declared Frame defaults.
[cluster.frame]
[cluster.frame.values]
stakes = "medium"
posture = "operator-active"
latency_budget_ms = 1500
```

## Per-broker manifest template

Each `<broker-id>.toml` follows the same shape (substitute `<broker-id>`
+ `<Role enum>` per broker):

```toml
[broker]
id = "<broker-id>"
name = "<Display Name>"
roles = ["<role-as-kebab-case>"]
cold_store_path = "<broker-id>-cold/"
catalog_path = "<broker-id>-catalog.yaml"
```

Per-broker role assignments (from C9.0 classification):

| Broker | Role |
|---|---|
| ide-state-sense | sense |
| window | embodiment |
| browser-dom-read | sense |
| browser-overlay | embodiment |
| browser-nav | embodiment |
| browser-screenshot | embodiment |
| browser-dom-write | embodiment |
| browser-eval | embodiment |
| browser-intercept | embodiment |
| layout | embodiment |
| agent | innate-ability |
| file | embodiment |
| ui-cards | embodiment |
| ui-state | embodiment |
| tool-promotion | innate-ability |
| chrome | embodiment |
| tour | innate-ability |
| checkpoint | innate-ability |
| federation | sense |

---

## Activation procedure (operator)

1. `mkdir -p <ide-project-root>/.claude/brain/broker/`
2. Drop the cluster.toml above + 19 per-broker manifests in that directory.
3. Restart the IDE. `BrokerHost::boot` will read the manifest set + register
   all 19 brokers.
4. Verify via the new `list_brokers_via_host` Tauri command — it should
   return all 19 broker ids.
5. Check `.claude/brain/broker/current-projection.md` — should contain a
   governance segment + 18 broker awareness routing segments (1 broker
   excluded since browser-overlay is AuditOnly + has no Surfaced pipelines).

But wait: the BrokerHost::boot currently constructs every cluster-declared
broker as a `WorkBroker` (per Wave 5.5 default; see broker_serve.rs run()).
The IDE-side brokers are `IdeStateSenseBroker`, `WindowBroker`, etc. —
NOT `WorkBroker`. The boot code needs to switch on broker_type per
per-broker manifest field (or the IDE needs its own host bootstrap that
constructs each broker from its actual type).

That's a **substrate-side BrokerHost.boot() extension** to consume a
`broker_type` field in the per-broker manifest — see the C10 follow-up
section below.

---

## C10 dead-code sweep procedure (after frontend migration)

C10 ships in three sequenced sub-steps after frontend migration retires
the legacy IdeAction dispatcher:

### C10.1 — Frontend migration to broker events

Per Phase C strangler-fig discipline. For each IdeAction variant:

1. Frontend's IPC handler for `dispatch_ide_action(action)` becomes a
   thin shim that calls `invoke('dispatch_pipeline_via_host', {...})`
   with the broker/pipeline corresponding to that action variant.
2. Frontend `listen('broker:<broker-id>:<op>:request', ...)` handlers
   land per variant; their bodies are the same code that used to live
   in the IdeAction-dispatch handler.
3. After all 64 variants have both a request listener AND a shim
   routing call sites through dispatch_pipeline_via_host, the legacy
   `dispatch_ide_action` Tauri command can retire.

### C10.2 — Substrate-side BrokerHost.boot() extension

`BrokerHost::boot()` currently hardcodes `WorkBroker` construction. To
host the IDE-side brokers, it needs to:

1. Read `broker_type` field from each per-broker manifest TOML.
2. Switch on broker_type to construct the right concrete type.
3. The IDE-side brokers register their constructors via a per-broker
   factory pattern (or the IDE provides its own custom BrokerHost
   variant that overrides boot()).

Estimated: ~1 day substrate work (extension of host.rs + per-broker
manifest schema; IDE-side registration of factory functions).

### C10.3 — Legacy IdeAction dispatcher deletion

After C10.1 + C10.2 land:

1. Grep for non-test callers of `dispatch_ide_action`. Should be zero.
2. Delete `ipc/ide_action.rs` (1500+ lines).
3. Delete the per-variant adapter functions across `browser/`, `agent/`,
   etc. that the dispatcher routed to.
4. Run `cargo check --workspace` to confirm no compilation breakage.
5. Run the IDE end-to-end to confirm every agent verb still works,
   now exclusively through the broker substrate.

---

## What's actually shipped today vs what C10 requires

**Today (this session ships):**
- All 19 broker shapes + catalogs + V0 leaf-op wire-ups (Tauri-emit
  pattern for 63 leaf-ops; real impl for federation/peer_ping)
- BrokerHost wired into Tauri lifecycle (B3)
- All Phase C strangler shims (C2 kill-switch bridge; C3-C8 subgate
  registrations + trace emitters)
- This cluster manifest doc as the operator's activation procedure

**What C10 requires before "completion":**
1. Substrate `BrokerHost::boot()` extension to construct IDE-side broker
   types from `broker_type` manifest field (~1 day).
2. Operator drops the cluster + per-broker manifests + per-broker
   catalog.yaml stubs into a real IDE project's
   `.claude/brain/broker/` (~0.5 day).
3. Frontend handler migration per variant (~5-10 days; one variant
   class at a time using the strangler-shim).
4. Legacy IdeAction dispatcher deletion + cargo check + manual smoke
   test (~1 day).

**Total C10 effort: ~7-12 days realistic across multiple sessions.**

The substrate-side and IDE-side broker SHAPE work is COMPLETE. The
remaining work is frontend migration + substrate extension to actually
construct IDE-typed brokers + legacy code retirement. None of that fits
in a single chat session — frontend migration alone requires per-variant
testing in the live IDE which is fundamentally an operator-driven
workflow.
