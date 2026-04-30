<!-- topic: command-post — bundled in neurogrim-cli v3.5 -->
# Command post — operator-driven dashboard

The Command Post is v4.3's reframe — the dashboard becomes the
**primary editing surface** rather than a viewer. Operators don't
touch JSON files for routine work; they use forms, tables, and
curated views. Edits emit on the bus so agents observe.

This topic covers v4.3's foundation stories. As of v4.3 session 4,
the registry editor (C-4 v1 weights + v2 autonomy/hats/federation),
custom pages (C-6 v1 CRUD + v2 widget gallery integration),
edit-via-bus integration (C-7 v1), and inline help (C-8 v3) are
shipped. Mobile-responsive breakpoints (C-9) and the schemars-driven
generic form generator (C-4 v3) are the remaining deferred polish.

## What's in v4.3 v1 (this stage)

Three new built-in pages joined the existing six (Overview,
Domains, Federation, Skills, Publish gates, Approvals):

- **Services** (`/brains/:id/services`) — per-peer process list:
  peer_name, pid, port, uptime, log path. Read-only fleet
  telemetry. Refreshes every 5 seconds. Start/stop actions stay on
  the Federation page.

- **Logs** (`/brains/:id/logs`) — filterable timeline aggregating
  publish-gate runs + autonomy approvals into a single view.
  Filter chips per source. Refreshes every 30 seconds. v1 ships
  publish-gates + approvals sources; future stories add invocation-
  ledger, score-history, services.jsonl, and `_neurogrim/notifications`.

- **Settings** (`/brains/:id/settings`) — five tabs:
  - **Registry** (C-4 v1+v2) — sub-tabbed curated editors for
    domain weights, autonomy action_types, hats with multipliers,
    and federation children. Single ETag-protected Save round-trips
    the whole registry; `BrainRegistry::validate()` runs server-side.
  - **Custom pages** (C-6 v1+v2) — add/delete custom named pages.
    The page itself opens at `/brains/:id/p/:pageName` with full
    edit-mode support: Customize → add widgets via the v3.4 catalog
    → resize / reorder / per-widget config → Save. PUTs to
    `/api/brains/:id/dashboard-pages/:name/layout` (gated by
    `--allow-mutations`).
  - **Culture** — read-only viewer (culture is a contract, not
    a setting).
  - **Queue config** — read-only viewer (editor lands with the
    queue backend selector).
  - **Publish gates** — pointer to the dedicated `/publish-gates`
    page.

## What's deferred

| Story | Why deferred |
|---|---|
| **C-4 v3** schemars-driven generic form generator | Useful for adopters declaring custom registry sections we don't ship curated forms for. Curated forms (v2) cover the in-tree sections operators actually edit; the generator is the long tail. |
| **C-4 v3** 3-way merge UI on ETag conflict | v1 ships ETag detection + reload-on-conflict banner. The merge UI is value-add when concurrent operator edits become common — today's single-operator usage rarely hits it. |
| **C-4 v3** domain definitions / `_todo_<name>` editor | Less frequently-edited surface; benefits from text-editor review more than form fields. |
| **C-6 v3** custom-page polish | v2 ships the widget gallery integration (operators compose pages through the UI). v3 follow-ons: page rename, icon picker, per-page title (vs. the kebab-case id), folder grouping at the 8-page limit. |
| **C-9** mobile-responsive breakpoints | Audit each page at 375px viewport; collapse sidebar at <768px. Best paired with operator visual review. |

<!-- anchor: multi-page-schema -->
## Multi-page schema (v2)

The new `dashboard-pages.json` shape supersedes v3.4's single
`dashboard-layout.json`:

```json
{
  "schema_version": "2",
  "brain_id": "alpha",
  "pages": {
    "overview": [...widgets...],
    "custom-pc-state": [...widgets...]
  },
  "page_order": ["overview", "services", "logs", "settings", "custom-pc-state"]
}
```

**Backward compat**: when only the v3.4 file exists,
`read_dashboard_pages` synthesizes a v2 view with the old layout
under `pages.overview`. Operators don't lose their work when the
adopter Brain upgrades binary versions.

**Built-in pages**: Overview, Services, Logs, Settings, Approvals,
Publish gates, plus the existing v3.4 routes (Domains, Federation,
Skills). The frontend renders these regardless of what's in the
`pages` map. The map governs widget content (Overview's layout,
custom pages); built-ins have hardcoded React components.

In v1 of S15, the v2 schema is **defined + read-compatible** but
not yet wired into the existing `dashboard-layout` endpoints. Full
migration lands with C-6 (custom pages) when the dynamic-route
runtime derivation is actually exercised.

<!-- anchor: cli-parity -->
## CLI parity invariant

Per S15 epic refinement, **every UI mutation maps to a documented
CLI surface**. Each form on the Registry editor (C-4 v1+v2) has a
CLI equivalent — adopters who prefer text-editor edits can skip the
dashboard entirely; the file system stays the source of truth.

| UI form | CLI / file-edit equivalent |
|---|---|
| Domain weight slider | Edit `config.domain_weights.<name>` in `brain-registry.json` |
| Autonomy action_type level dropdown | Edit `config.autonomy.action_types.<name>.default_level` |
| Hat domain multiplier slider | Edit `config.hats.<hat>.domain_multipliers.<domain>` |
| Federation child CRUD | `neurogrim federation register --name <name> --path <path>` (add); edit `config.children.<name>` (modify); remove the entry (delete) |
| Federation rewire | `neurogrim federation rewire --child <name>` (CLI today; button-driven flow is C-4 v3) |
| Add domain | `neurogrim domain new <name>` (existing) |

## Inspectability discipline

The on-disk JSON/YAML remains the canonical authority. The
dashboard pulls from disk on each refresh; an operator's `vim`
edit shows up in the dashboard within 30 seconds via SSE-driven
refetch. Dashboard edits flow back to disk through the C-4 v1+v2
PUT path — atomic temp-file-rename with ETag-protected concurrency.

Adopters can `tail -f .claude/brain/dashboard-pages.json` (or
`.claude/brain-registry.json`) to watch their changes propagate,
the same way they can `tail -f` any other Brain artifact.

<!-- anchor: edit-via-bus -->
## Edit-via-bus design (C-7)

Every UI mutation emits on `_neurogrim/config-changes` with this
payload (v1 minimal — v2 will add keypath-level diffs):

```json
{
  "action_type": "registry_edit | layout_change | layout_reset | page_added | page_removed | approval_resolved",
  "operator": "<from $NEUROGRIM_OPERATOR>",
  "timestamp": "<RFC3339>",
  "brain_id": "<id>",
  "summary": "<one-line human-readable summary>"
}
```

Agents subscribing to that topic observe operator activity in
real-time — the substrate for "agent reacts when operator changes
the autonomy block" workflows. No polling; SSE-pushed via the
v4.1 bus.

## Conflict detection (C-4 v1)

Two operators edit the same registry section concurrently — UI A
loads version N, UI B loads version N, both Save. C-4 v1 ships
ETag-style versioning: the GET response carries a SHA-256 fingerprint
of the file bytes; PUT echoes it back. The server rejects with 409
Conflict + `code: "etag-conflict"` when the on-disk fingerprint
differs.

The v1 mitigation is a "Reload" button on the conflict banner —
the operator loses unsaved changes. The 3-way merge UI is a v3
follow-on; today's single-operator workflow rarely hits it
because `vim` users typically don't have concurrent UI sessions
to the same Brain.

## See also

- `neurogrim explain methodology` — the conceptual model
- `neurogrim explain queues` — v4.1 bus that edit-via-bus uses
- `neurogrim explain publish-gates` — v4.0 sibling pipeline
- `neurogrim explain secrets` — v4.2 secrets infrastructure (S14-S-6
  is the future Settings-tab editor for `secret-refs.yaml`)
- `roadmap/epics/S15-command-post-ui.md` — story-level plan
- `crates/neurogrim-dashboard/src/pages.rs` — multi-page schema +
  backward-compat read
- `crates/neurogrim-dashboard/frontend/src/components/services/` —
  Services page implementation
- `crates/neurogrim-dashboard/frontend/src/components/logs/` — Logs
  page
- `crates/neurogrim-dashboard/frontend/src/components/settings/` —
  Settings page
