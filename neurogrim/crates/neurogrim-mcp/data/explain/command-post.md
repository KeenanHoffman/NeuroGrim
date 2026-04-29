<!-- topic: command-post — bundled in neurogrim-cli v3.5 -->
# Command post — operator-driven dashboard

The Command Post is v4.3's reframe — the dashboard becomes the
**primary editing surface** rather than a viewer. Operators don't
touch JSON files for routine work; they use forms, tables, and
curated views. Edits emit on the bus so agents observe.

This topic covers v4.3's foundation stories. The registry editor
(C-4 — load-bearing), custom pages (C-6), edit-via-bus integration
(C-7), inline help (C-8), and mobile-responsive breakpoints (C-9)
land in subsequent stages.

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

- **Settings** (`/brains/:id/settings`) — read-only YAML viewers
  for `culture.yaml`, `queue-config.yaml`, plus a pointer to the
  Publish gates page. Editors land with C-4 + later expansion.

## What's deferred

| Story | Why deferred |
|---|---|
| **C-4** registry editor | 8-day load-bearing story. `schemars` form generator + 3-way merge UI for concurrent edits. Lands in session 2. |
| **C-6** operator-defined custom pages | Add-page modal + dynamic widget grid + sidebar uniformity with built-ins. Session 2. |
| **C-7** edit-via-bus integration | Every UI mutation emits on `_neurogrim/config-changes`. Pairs with C-4's editor surface. Session 2. |
| **C-8** inline help | `?` icons next to each settings field linking to relevant explain-topic anchors. Adds anchors to all 15 topics. Session 2. |
| **C-9** mobile-responsive breakpoints | Audit each page at 375px viewport; collapse sidebar at <768px. Session 2 polish. |

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
CLI command**. Today's read-only viewers don't introduce any new
surface. When C-4 lands, each form on the Settings page maps to a
CLI invocation:

| UI form | CLI equivalent |
|---|---|
| Domain weight slider | `domain weight` subcommand (future) |
| Add federation peer | `neurogrim federation register --name <name> --path <path>` |
| Edit autonomy action_type | `autonomy set` subcommand (future) |
| Add domain | `neurogrim domain new <name>` (existing) |

Adopters who prefer the CLI workflow can ignore the dashboard
entirely; the file system stays the source of truth.

## Inspectability discipline

The Settings page is the **read-only window** into config files.
The on-disk YAML remains the canonical authority. The dashboard
pulls from disk on each refresh; an operator's `vim` edit shows
up in the dashboard within 30 seconds (the page's refetch
interval). The reverse direction — dashboard edits flowing to disk
— lands with C-4's editor.

Adopters can `tail -f .claude/brain/dashboard-pages.json` to
watch their changes propagate, the same way they can `tail -f`
any other Brain artifact.

<!-- anchor: edit-via-bus -->
## Edit-via-bus design (C-7 preview)

Once C-4 + C-7 ship, every UI mutation will emit on
`_neurogrim/config-changes` with this payload:

```json
{
  "action_type": "registry_edit | layout_change | secret_added | gate_added",
  "before": <serialized prior state>,
  "after": <serialized new state>,
  "operator": "<from $NEUROGRIM_OPERATOR>",
  "timestamp": "<RFC3339>"
}
```

Agents subscribing to that topic observe operator activity in
real-time — the substrate for "agent reacts when operator changes
the autonomy block" workflows. No polling; SSE-pushed via the
v4.1 bus.

## Conflict detection (C-4 preview)

Two operators edit the same registry section concurrently — UI A
loads version N, UI B loads version N, both Save. Without
detection, last-writer-wins silently overwrites. C-4 ships
ETag-style versioning + a 3-way merge UI when the server detects
the file's checksum changed between read and save.

For v1 (today), the on-disk YAML is single-writer in practice;
operators using `vim` shouldn't have concurrent UI sessions to the
same Brain. The conflict UI lands when concurrent editing becomes
plausible (multi-operator deployments are an S16+ concern).

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
