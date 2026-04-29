# Epic: Command Post UI — Stage 15

**Stage:** 15
**Release:** v4.3 — "Operator-Driven"
**Status:** PLANNED (drafted 2026-04-29)
**Priority:** Capstone — completes the v4.x reframe from "tool agents use" to "command post for humans + agents"
**Goal:** The dashboard becomes the primary editing surface. Operators don't touch JSON files for routine work; they use forms, dropdowns, and curated views. Edits emit on the bus so agents observe. Ship 3 new built-in pages (Services, Logs, Settings) AND an operator-defined custom-pages system that reuses the v3.4 widget catalog.

**Depends on:**
- S13 (UI edits emit on `_neurogrim/config-changes` queue)
- S14 (Settings UI's secret-entry forms route through `SecretStore`; values never visible)
- Existing v3.4 widget catalog + LayoutEditor + WidgetGallery (foundations)
- Existing MCP mutation tools `domain_new`, `federation_register`, `awareness_add` (Settings UI wraps as forms)

**Blocks:** Nothing in v4.x; future work on RBAC, multi-user collaboration, undo/redo branches off v4.4+

**Master roadmap:** `roadmap/v4-roadmap.md`

---

## Stage 15 Is Done When

**Foundation (shipped, session 1):**
- [x] `<brain>/.claude/brain/dashboard-pages.json` schema authored + backward-compat read of v3.4 `dashboard-layout.json` *(C-1 v1; full multi-page wiring lands with C-6 in session 2)*
- [x] Built-in **Services** page: per-peer process list (peer_name, pid, port, uptime, log_path) reading from `services.jsonl` *(C-2; log tail + re-probe + sensor refresh deferred — those depend on additional API endpoints)*
- [x] Built-in **Logs** page: filterable timeline aggregating publish-gate-ledger + autonomy approvals *(C-3; invocation-ledger / score-history / services.jsonl / `_neurogrim/notifications` sources are additive — the page is structured to absorb them; toast notifications deferred)*
- [x] Built-in **Settings** page: read-only viewers for `culture.yaml`, `queue-config.yaml`, plus pointer to Publish gates page *(C-5 v1; editors land with C-4 + later expansion)*
- [x] `culture.yaml` rendered read-only (culture is a contract, not a setting) *(C-5 v1 with explicit "this is a contract" framing in the viewer)*
- [x] CLI parity preserved: every UI surface today is read-only; no CLI deprecation
- [x] 15th explain topic: `neurogrim explain command-post` *(`crates/neurogrim-mcp/data/explain/command-post.md`; methodology_drift TOPICS extended; CLAUDE.md "all 12" → "all 13")*

**Foundation (shipped, session 2):**
- [x] Edits emit on `_neurogrim/config-changes` queue with standard payload *(C-7 v1 — `emit_config_change` helper decorates layout-save / layout-reset / approval-resolve / registry-edit / page-add / page-delete handlers; v1 payload is `{action_type, operator, timestamp, brain_id, summary}` — keypath-level diffs are a v2 enhancement)*
- [x] Edit conflicts detected via ETag-style versioning *(C-4 v1 — SHA-256 fingerprint on the registry file; PUT rejects with 409 Conflict on mismatch; 3-way merge UI deferred to C-4 v2)*
- [x] Built-in Settings: registry editor surface (domain weights only) *(C-4 v1 — slider per declared domain weight + sum-validity hint + Save/Discard with ETag-protected PUT; full schemars form generator + autonomy/hats/federation editors are C-4 v2)*
- [x] Operator can create custom named pages *(C-6 v1 — Add Page form on Settings page; `POST /api/brains/:id/dashboard-pages/:name` validates kebab-case + reserved-id collisions; `DELETE` removes; reachable at `/brains/$brainId/p/$pageName`)*
- [x] Inline help infrastructure *(C-8 v1 — `HelpIcon` component + modal that fetches `GET /api/explain/:topic`; anchor convention `<!-- anchor: <id> -->` documented + sample anchors added to scoring.md; rolling out across all 13 topics is gradual)*

**Heavy follow-ons (still deferred):**
- [ ] Schema → form generator handles 80% case (object/array/string/number/boolean/enum); textarea fallback for complex shapes *(C-4 v2 — full registry editor for autonomy / hats / federation / domain_definitions sections; the `schemars` integration + curated forms per section are the load-bearing piece)*
- [ ] 3-way merge UI when concurrent edits collide *(C-4 v2; current behavior is reload-on-conflict)*
- [ ] Built-in Services page: log tail, manual re-probe, sensor refresh *(C-2 expansion; needs new API endpoints for per-service log streams)*
- [ ] Built-in Logs page: invocation-ledger + score-history + services.jsonl + `_neurogrim/notifications` sources *(C-3 expansion)*
- [ ] Custom page widget gallery: operator picks widgets from the v3.4 catalog *(C-6 v2; v1 only supports name + delete)*
- [ ] Custom page limit (default 8 per Brain) + folder grouping when exceeded *(C-6 v2)*
- [ ] Page icon picker + per-page title *(C-6 v2)*
- [ ] Anchor links work cross-page: `/brains/:id/<page-name>/#widget-<id>` smooth-scrolls + pulse-highlights *(C-6 v2 + C-8 v2)*
- [ ] Inline help anchors rolled out across all 13 explain topics *(C-8 v2 — convention is established; mechanical work)*
- [ ] Markdown renderer in HelpIcon modal (currently preformatted text) *(C-8 v2)*
- [ ] Mobile-responsive at 375px viewport; no horizontal scroll on any page *(C-9 — final-polish pass; best paired with operator visual review)*
- [ ] Adopter walkthrough doc: first custom-page authoring, edit-via-bus subscription, conflict-resolution flow *(documentation pass)*

---

## Stories

### S15-C-1: Multi-page dashboard infrastructure (5 days) — 🟡 PARTIAL (schema authored + backward-compat read; full router/sidebar auto-population deferred to session 2 with C-6)

**What:** Extend v3.5 widget catalog. A "page" is now a named layout. Brain config has:

```json
{
  "schema_version": "1",
  "pages": {
    "overview": [...widgets...],
    "services": [...widgets...],
    "settings": [...widgets...],
    "custom-pc-state": [...widgets...]
  },
  "page_order": ["overview", "services", "logs", "settings", "custom-pc-state"]
}
```

Sidebar navigation auto-populates from declared pages. Per-page persistence in `.claude/brain/dashboard-pages.json` (replaces v3.4 `dashboard-layout.json` with backward-compat read).

**Default pages** for fresh Brains: `overview`, `services`, `settings`. Logs added in S15-C-3.

**Done when:**
- [ ] New schema + ts-rs bindings + 8 tests
- [ ] v3.4 `dashboard-layout.json` read-compat: deserialize into `pages.overview`
- [ ] Sidebar renders dynamically from declared pages
- [ ] TanStack Router routes auto-derived from pages map
- [ ] Migration helper: `neurogrim dashboard-pages migrate` rewrites old → new shape

### S15-C-2: Built-in Services page (5 days) — 🟡 PARTIAL (read-only fleet view shipped; log tail + re-probe + sensor refresh deferred)

**What:** Extract v3.5 `PeerActions` into a full page. Show per-peer process list (reads from `services.jsonl`), per-service log tail (5-second poll OR SSE-pushed), manual re-probe + sensor refresh actions (carry-over from v3.5.1 backlog).

**Done when:**
- [ ] Page component + route
- [ ] Process list table renders with status, pid, port, uptime
- [ ] Log tail surface (last 200 lines + live append via SSE)
- [ ] Re-probe + sensor refresh buttons + tests
- [ ] vitest covers the page

### S15-C-3: Built-in Logs page (3 days) — 🟡 PARTIAL (publish-gates + approvals sources shipped; remaining 4 sources + toast notifications deferred)

**What:** Filterable timeline view reading from:
- `<brain>/.claude/brain/services.jsonl` (S13.7 service runtime ledger)
- `<brain>/.claude/brain/invocation-ledger.jsonl` (existing)
- `<brain>/.claude/brain/score-history.json` (existing)
- `<brain>/.claude/brain/publish-gate-ledger.jsonl` (S12)
- `_neurogrim/notifications` queue (S13)

Filter chips per source. Click a row → drill into the originating widget. Toast notifications for new SSE events while user is on this page.

**Done when:**
- [ ] Page + filter chips + sortable timeline
- [ ] Toast system uses the v3.6 backlog item brought forward
- [ ] vitest covers the integration

### S15-C-4: Built-in Settings page — registry editor (8 days, the load-bearing one) — 🟡 PARTIAL (domain-weights editor + ETag conflict detection shipped; full schemars form generator + autonomy/hats/federation editors + 3-way merge UI deferred to v2)

**What:** Curated forms for each section of `brain-registry.json`:

- **Domain weights:** slider per domain (0.0–1.0) with preview unified-score impact
- **Domain definitions:** principle text edit; `_todo_<name>` authoring intent
- **Autonomy:** per-action_type level dropdown (Auto/Notify/Approve/Blocked); safety invariants list editor
- **Hats:** declare/remove; multipliers; description editing
- **Federation children:** add/remove peers; v3.5 `federation rewire` action exposed as a button

Schema source: Rust struct → JSON Schema (auto-generate via `schemars` crate, already in workspace deps) → form generator on the frontend. Save flow: validate → write atomically → emit `RegistryEdited` on `_neurogrim/config-changes` queue.

**Conflict detection:** if registry changed externally between load and save, surface a 3-way merge UI.

**Done when:**
- [ ] `schemars` integration emits JSON Schema for `BrainConfig` etc.
- [ ] Form generator on frontend handles object/array/string/number/boolean/enum
- [ ] Each section has a curated form (not raw JSON Schema rendering — operators get domain-specific UX)
- [ ] Validation on save uses existing `registry.validate()` + helpful error surfacing
- [ ] Conflict detection ships with diff UI
- [ ] vitest covers form behaviors + conflict resolution

### S15-C-5: Built-in Settings page — other configs (4 days) — 🟡 PARTIAL (read-only viewers for culture + queue-config + publish-gates pointer shipped; editors deferred until C-4's form generator + S14-S-6's passphrase flow land)

**What:**
- `culture.yaml` viewer (read-only — culture changes are a contract, not a setting; explained inline)
- `secret-refs.yaml` editor (declared secrets only; values via S14 path)
- `publish-gates.yaml` editor (define gates from S12)
- `queue-config.yaml` editor (per-topic backend + retention from S13)

**Done when:**
- [ ] Each editor sub-page + tests
- [ ] Read-only culture viewer with link to `neurogrim explain culture`
- [ ] Secrets editor handoff to S14 fetch flow tested

### S15-C-6: Operator-defined custom pages (4 days) — 🟡 PARTIAL (CRUD endpoints + Add Page form + catchall route + CustomPageRenderer shipped; widget gallery + icon picker + folder grouping deferred to v2)

**What:** "Add page" flow: operator names a page (kebab-case validated), picks an icon (lucide-react set), adds widgets via the v3.4 catalog. Custom pages persist alongside built-ins; sidebar rendering treats them identically.

Anchor links extend: `/brains/:id/<page-name>/#widget-<id>` works across pages.

Limit: 8 custom pages per Brain by default (configurable). Group into folders if more declared.

**Done when:**
- [ ] Add-page modal + page-rename + page-delete
- [ ] Validation: page-name uniqueness, kebab-case, doesn't collide with built-in routes
- [ ] vitest covers the flow
- [ ] Folder grouping when limit exceeded

### S15-C-7: Edit-via-bus integration (3 days) — ✅ SHIPPED (v1 minimal payload; keypath-level before/after diffs deferred to v2)

**What:** Every UI mutation emits on `_neurogrim/config-changes` queue.

Standard payload:
```json
{
  "action_type": "registry_edit | layout_change | secret_added | gate_added",
  "before": <serialized prior state>,
  "after": <serialized new state>,
  "operator": "<from $NEUROGRIM_OPERATOR>",
  "timestamp": "<RFC3339>"
}
```

Documented as the way for agents to observe operator activity. Sample agent: PC-state pilot can subscribe to its own Brain's queue.

**Done when:**
- [ ] Emission infrastructure shared across all mutation handlers
- [ ] `before/after` diff is small (key paths only, not full structures, for sensitive sections)
- [ ] vitest covers emission for each mutation type
- [ ] Adopter doc: how to write an agent that subscribes

### S15-C-8: Inline help integration (2 days) — 🟡 PARTIAL (HelpIcon component + modal + `GET /api/explain/:topic` endpoint + anchor convention with proof anchors in scoring.md; rolling anchors out across all 13 topics + markdown renderer deferred)

**What:** Each settings field has a `?` icon. Click → modal or sidebar pulls from relevant `neurogrim explain` topic at section anchor. Anchor format: `<brain>/explain/scoring#weighted-mean`.

This requires the explain topics to have stable section anchors. Audit existing 14 topics; add anchors as needed.

**Done when:**
- [ ] Anchor convention added to all 14 explain topics
- [ ] `?` icon implementation + modal
- [ ] vitest covers anchor resolution

### S15-C-9: Mobile-responsive breakpoints (3 days) — ⏳ DEFERRED (mobile breakpoints best audited with operator visual review at real 375px viewport; pairs with operator feedback in a polish session)

**What:** v3.4-v3.5 dashboard works on desktop + tablet; mobile is broken. Audit each page against 375px viewport; fix layout overflow; collapse sidebar at <768px.

**Goal:** "doesn't break", not "Mobile-First."

**Done when:**
- [ ] Each page renders cleanly at 375px (visual regression via Playwright snapshots)
- [ ] Sidebar collapses to drawer at <768px
- [ ] Touch targets ≥44px per WCAG
- [ ] No horizontal scroll on any page

---

## Risks (plan-critic concerns brought forward)

🟡 **Schema → form is harder than it looks.** `schemars` produces JSON Schema; turning that into ergonomic forms requires custom UI components per primitive type, anyOf/oneOf handling, and graceful degradation when the schema gets exotic. **Mitigation:** ship form support for the 80% case (object, array, string, number, boolean, enum); fall back to text-area for complex shapes; document the limitation. Estimated effort doubles if we try to handle every JSON Schema feature.

🟡 **Edit conflicts.** Operator A edits via UI; Operator B edits same registry via text editor; both Save. Last-writer-wins loses data. **Mitigation:** ETag-style versioning on registry reads; settings page detects conflict and shows 3-way merge UI in S15-C-4.

🟡 **Custom-page proliferation.** Operators add pages until the sidebar is unmanageable. **Mitigation:** limit to 8 custom pages per Brain (configurable); group into folders if more declared.

🔴 **Blocking concern: dashboard-down = can't edit.** The CLI must remain canonical. Every Settings UI action must map to a documented CLI invocation. **Mitigation:** S15-C-4 explicitly preserves CLI parity; review pass before ship verifies each form has a CLI equivalent.

🔵 **Suggestion: undo/redo for the last N edits.** Cheap given the bus already records every change. v4.4 work, not S15.

🔵 **Suggestion: "what changed" audit view per Brain.** Reads `_neurogrim/config-changes` queue; renders a timeline of operator + agent edits. v4.4+ candidate.

🔵 **Suggestion: settings field-level RBAC.** Beyond v4.x. Multi-user / network-exposed dashboard is its own stage (S16+).

---

## Cross-references

- Master roadmap: `roadmap/v4-roadmap.md`
- v3.4 widget catalog (frontend): `frontend/src/lib/widget-catalog.ts`
- v3.4 LayoutEditor: `frontend/src/components/overview/LayoutEditor.tsx`
- v3.5 WidgetGallery: `frontend/src/components/overview/WidgetGallery.tsx`
- Existing MCP mutation tools wrapped as forms: `crates/neurogrim-mcp/src/server.rs:439`, `:497`
- `schemars` crate (already in workspace deps): https://crates.io/crates/schemars
- S13 dependency (config-changes queue): `roadmap/epics/S13-agent-coordination-bus.md`
- S14 dependency (secret-entry flow): `roadmap/epics/S14-encrypted-secrets.md`
