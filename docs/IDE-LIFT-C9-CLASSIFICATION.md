# C9.0 — IdeAction Variant Classification

> **Plan gate** (§C9 + ultra-pass U10): do not start C9 bulk migration
> without a published classification of all IdeAction variants. This is
> that document.

**Source:** `D:\local-pc-operational-management\children\neurogrim-ide\src-tauri\src\ipc\ide_action.rs`
(64 variants as of 2026-06-25 grep).

**Classification axes per variant:**
1. **Target broker** — which broker hosts this verb after C9 ships
2. **Visibility** — Surfaced (agent-facing) / Internal (broker-internal) / AuditOnly (audit-traced but not agent-listed)
3. **Leaf-op signature** — pure-data / needs-AppHandle / needs-WebView2 / needs-IDE-state (panes/sessions/registry)

**Pattern rule used:** group variants by their dispatcher dispatch_* helper +
target subsystem. Where a variant straddles two brokers (rare), the table
flags it with a "→ split" note for the migration author to handle bespoke.

---

## Group 1: agent verbs (target broker: `agent-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `SendPrompt` | Surfaced | needs-AppHandle + IDE-state | Routes to claude session manager |
| `ResolvePermission` | Surfaced | needs-AppHandle | Permission-card resolution; pairs with C6 permission-tokens broker |

## Group 2: file + document (target broker: `file-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `OpenFile` | Surfaced | needs-AppHandle | Opens in IDE editor pane |
| `ShowDocument` | Surfaced | needs-AppHandle | Read-only viewer surface |

## Group 3: LocalAwareness (target broker: `local-awareness-broker` — already lifted at B1)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `SetLocalAwarenessFact` | Surfaced | pure-data | Maps to `local-awareness-broker/set-fact` |

## Group 4: UI cards (target broker: `ui-cards-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `EmitCustomCard` | Surfaced | needs-AppHandle | Emits a card to the IDE UI |

## Group 5: UI state — focus + posture (target broker: `ui-state-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `SwitchActiveTab` | Surfaced | needs-AppHandle + IDE-state | Tab switcher |
| `SetFocusedPane` | Surfaced | needs-AppHandle | Focus manager |
| `SwitchPosture` | Surfaced | needs-AppHandle | Posture (hat) switcher |
| `SwitchViewMode` | Surfaced | needs-AppHandle | View mode (split/single/etc) |

## Group 6: Tool promotion (target broker: `tool-promotion-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `PromoteTool` | Surfaced | needs-AppHandle | Tool promotion ledger |
| `RestorePromotedTool` | Surfaced | needs-AppHandle | Tool restoration |

## Group 7: Sense projections (target broker: `ide-state-sense-broker` — read-only)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `SnapshotLayout` | Internal | needs-IDE-state | Layout snapshot read |
| `SnapshotPanes` | Internal | needs-IDE-state | Pane snapshot read |
| `GetFocusedCell` | Internal | needs-IDE-state | Focus reader |
| `GetPosture` | Internal | needs-IDE-state | Posture reader |
| `GetDockState` | Internal | needs-IDE-state | Dock reader |
| `GetActiveTool` | Internal | needs-IDE-state | Active tool reader |

## Group 8: Chrome state (target broker: `chrome-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `ToggleChromeCollapsed` | Surfaced | needs-AppHandle | Chrome collapse toggle |
| `SetChromeCellCollapsed` | Surfaced | needs-AppHandle | Per-cell chrome state |
| `SetEvidenceTab` | Surfaced | needs-AppHandle | Evidence tab toggle |
| `UnpinAllInPane` | Surfaced | needs-IDE-state | Pin-management op |

## Group 9: Tour scripting (target broker: `tour-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `RunTourScript` | Surfaced | needs-AppHandle + IDE-state | Tour replay |

## Group 10: Layout (target broker: `layout-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `PlacePane` | Surfaced | needs-AppHandle | Layout place |
| `SpanPane` | Surfaced | needs-AppHandle | Layout span |
| `MovePane` | Surfaced | needs-AppHandle | Layout move |
| `ClearPane` | Surfaced | needs-AppHandle | Layout clear |
| `SwapPanes` | Surfaced | needs-AppHandle | Layout swap |
| `SetTrackWeight` | Surfaced | needs-AppHandle | Track weight |
| `SetPaneContent` | Surfaced | needs-AppHandle + IDE-state | Pane content assignment |
| `FocusPane` | Surfaced | needs-AppHandle | Focus delegate |
| `IsolatePane` | Surfaced | needs-AppHandle | Isolation toggle |
| `MaximizePane` | Surfaced | needs-AppHandle | Max toggle |
| `FloatPane` | Surfaced | needs-AppHandle | Float window |
| `UnfloatPane` | Surfaced | needs-AppHandle | Unfloat |

## Group 11: Window management (target broker: `window-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `WindowSwitch` | Surfaced | needs-AppHandle | Window switch |
| `WindowOpen` | Surfaced | needs-AppHandle | Window open |
| `WindowRename` | Surfaced | needs-AppHandle | Window rename |
| `WindowClose` | Surfaced | needs-AppHandle | Window close |

## Group 12: Checkpoint + Restore (target broker: `checkpoint-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `Checkpoint` | Surfaced | needs-AppHandle + IDE-state | Snapshot creation |
| `Restore` | Surfaced | needs-AppHandle + IDE-state | Snapshot restore |

## Group 13: Federation / peers (target broker: `federation-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `PeerPing` | Internal | pure-data | Liveness probe |

## Group 14: Browser DOM read (target broker: `browser-dom-read-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserQuerySelector` | Surfaced | needs-WebView2 | DOM query |
| `BrowserSnapshotDom` | Surfaced | needs-WebView2 | DOM snapshot |
| `BrowserGetConsoleLogs` | Surfaced | needs-WebView2 | Console reader |
| `BrowserGetNetworkSummary` | Surfaced | needs-WebView2 | Network reader |
| `BrowserGetUrl` | Surfaced | needs-WebView2 | URL reader |

## Group 15: Browser navigation (target broker: `browser-nav-broker`; uses A7 RateLimitSubgate)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserNavigate` | Surfaced | needs-WebView2 | navigation quota bucket |
| `BrowserBack` | Surfaced | needs-WebView2 | navigation quota bucket |
| `BrowserForward` | Surfaced | needs-WebView2 | navigation quota bucket |
| `BrowserReload` | Surfaced | needs-WebView2 | navigation quota bucket |
| `BrowserSetViewport` | Surfaced | needs-WebView2 | configuration; not quota-counted |
| `BrowserSetThrottling` | Surfaced | needs-WebView2 | configuration |

## Group 16: Browser screenshots (target broker: `browser-screenshot-broker`; uses A7 screenshot bucket)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserScreenshotPage` | Surfaced | needs-WebView2 | screenshot quota bucket |
| `BrowserScreenshotRegion` | Surfaced | needs-WebView2 | screenshot quota bucket |
| `BrowserScreenshotElement` | Surfaced | needs-WebView2 | screenshot quota bucket |

## Group 17: Browser eval (target broker: `browser-eval-broker`; uses A7 eval bucket)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserEvalWithResult` | Surfaced | needs-WebView2 | eval quota bucket; high-risk; needs C5 capability check |

## Group 18: Browser DOM write (target broker: `browser-dom-write-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserClick` | Surfaced | needs-WebView2 | High-risk; capability-gated |
| `BrowserType` | Surfaced | needs-WebView2 | High-risk; capability-gated |
| `BrowserFocus` | Surfaced | needs-WebView2 | DOM focus |
| `BrowserSetAttribute` | Surfaced | needs-WebView2 | DOM mutation |
| `BrowserDispatchEvent` | Surfaced | needs-WebView2 | DOM event dispatch |

## Group 19: Browser network intercept (target broker: `browser-intercept-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserInterceptObserve` | Surfaced | needs-WebView2 | Read-only observation |
| `BrowserInterceptBlock` | Surfaced | needs-WebView2 | Block rule; capability-gated |
| `BrowserInterceptModify` | Surfaced | needs-WebView2 | Modify rule; capability-gated |

## Group 20: Browser overlay annotations (target broker: `browser-overlay-broker`; AuditOnly per A14)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserHighlight` | **AuditOnly** | needs-WebView2 | Visual overlay; not agent-listed |
| `BrowserClearHighlights` | **AuditOnly** | needs-WebView2 | |
| `BrowserAnnotate` | **AuditOnly** | needs-WebView2 | |
| `BrowserClearAnnotations` | **AuditOnly** | needs-WebView2 | |

## Group 21: Browser scroll (target broker: `browser-nav-broker`)

| Variant | Visibility | Leaf-op signature | Notes |
|---|---|---|---|
| `BrowserScrollTo` | Surfaced | needs-WebView2 | Scroll positioning |

---

## Summary by target broker

| Target broker | Variant count | Substrate primitives in play |
|---|---|---|
| `agent-broker` | 2 | C6 (permission tokens for ResolvePermission) |
| `file-broker` | 2 | none |
| `local-awareness-broker` | 1 | B1 existing broker (just route) |
| `ui-cards-broker` | 1 | none |
| `ui-state-broker` | 4 | none |
| `tool-promotion-broker` | 2 | none |
| `ide-state-sense-broker` | 6 | Sense role |
| `chrome-broker` | 4 | none |
| `tour-broker` | 1 | none |
| `layout-broker` | 12 | none (high variant count; consider sub-brokers) |
| `window-broker` | 4 | none |
| `checkpoint-broker` | 2 | candidate D2 Workflow Engine consumer (multi-tick save flow) |
| `federation-broker` | 1 | none |
| `browser-dom-read-broker` | 5 | C5 capability gate |
| `browser-nav-broker` | 7 (6 nav + 1 scroll) | A7 navigation quota |
| `browser-screenshot-broker` | 3 | A7 screenshot quota |
| `browser-eval-broker` | 1 | A7 eval quota + C5 capability gate |
| `browser-dom-write-broker` | 5 | C5 capability gate (all high-risk) |
| `browser-intercept-broker` | 3 | C5 capability gate |
| `browser-overlay-broker` | 4 | A14 AuditOnly visibility |
| **Total** | **64** | |

---

## Mechanical vs bespoke split

Per ultra-pass U10: "30-40% of variants will need bespoke leaf-op shapes."

**Mechanical (suitable for `broker-scaffold`):**
- Group 7 (Sense readers, 6) — pure projections from IDE state
- Group 11 (window, 4) — well-defined Tauri window API
- Group 13 (PeerPing, 1)
- Group 14 (DOM read, 5)
- Group 15 nav configuration variants (Viewport, Throttling — 2)
- Group 18 simple DOM (Focus, SetAttribute — 2)
- Group 19 read-only intercept (Observe — 1)
- Group 20 overlay (4)
- **Mechanical total: ~25 (~39%)**

**Bespoke (need hand-authoring):**
- Group 1 (agent — claude session integration is complex)
- Group 5 (UI state — Solid signal handling)
- Group 10 (layout — 12 variants, all touch the grid layout system)
- Group 12 (checkpoint — Workflow Engine multi-tick wrap)
- Group 15 active nav variants (Navigate, Back, Forward, Reload, Scroll — Tauri webview API + quota)
- Group 16 (screenshots — xcap integration)
- Group 17 (eval — high-risk + capability gate + result extraction)
- Group 18 high-risk DOM (Click, Type, DispatchEvent — input simulation)
- Group 19 intercept rules (Block, Modify — CDP wiring)
- **Bespoke total: ~39 (~61%)**

The plan's 30-40% bespoke estimate is conservative; this audit suggests
closer to 60% bespoke. Adjust the C9 effort estimate from 8-12 days to
**12-18 days realistic** for full C9 ship.

---

## Recommended C9 execution order

1. **Sub-phase C9a — Mechanical batch** (~3-4 days): scaffold-author all
   25 mechanical variants in one pass. Each takes ~5-10 minutes via
   `neurogrim broker-scaffold` + paste. Validates the scaffolder works
   at scale + lets the remaining 39 bespoke variants ship against a
   working substrate that already routes ~40% of agent verbs.
2. **Sub-phase C9b — Browser nav + screenshot bespoke** (~3-4 days):
   the 8 variants that need WebView2 active API + quota integration.
   Highest-value because browser actions are the agent's main world-
   effect surface.
3. **Sub-phase C9c — High-risk DOM + eval + intercept** (~3-4 days):
   the 9 variants needing capability + risk gating.
4. **Sub-phase C9d — Layout broker** (~2-3 days): 12 layout variants;
   touches grid system; needs IDE-state-pane access.
5. **Sub-phase C9e — Agent + checkpoint + UI** (~2-3 days): the
   remaining bespoke variants tied to claude session / multi-tick
   workflows / Solid signals.

**Total: 13-18 days realistic across 5 sub-phases.** Operator picks any
sub-phase as the next-session anchor; each ships independently with
its own catalog updates + cluster manifest amendment.

---

## How to use this classification

For each Sub-phase C9a/b/c/d/e:
1. Pick the next batch from the table (e.g., all Group 7 variants for
   the first C9a iteration).
2. Run `neurogrim broker-scaffold` per variant (using the table's
   broker-id + visibility + audit-class + leaf-op naming convention).
3. Paste output into the target broker's `catalog()` + `execute_leaf`
   match arm.
4. Add per-batch broker to cluster.toml.
5. Update IDE call sites to dispatch via the new pipelines (or keep
   strangler-shim until ready to retire the legacy IdeAction path).
