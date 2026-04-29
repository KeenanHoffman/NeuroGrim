<!-- topic: dashboard-layouts — bundled in neurogrim-cli v3.4 -->
# Dashboard layouts — authoring guide for agents

The v3.4 dashboard's Overview page is composed from a per-Brain
widget layout. This topic is the **authoring reference** —
read it before composing or editing a layout for a Brain you're
working on. For the dashboard's overall surface (the five pages,
SSE updates, theme, etc.), see `neurogrim explain ui`.

## When you need this

An agent typically reaches for layout authoring in one of three
moments:

1. **Bootstrapping a new Brain** — `neurogrim init` produced a
   working registry with the v3.4 default layout. The default
   is posture-aware (gauge for weighted, child-cards for an
   all-advisory federation parent), but it doesn't know what
   *this particular* project cares most about. Author a custom
   layout to highlight the operator's actual priorities.
2. **Reframing the homepage** — the operator says "my dashboard
   shows N/A and that's accurate but unhelpful." The fix is
   usually a layout that surfaces the substantive signals
   (child scores, specific domains, framing notes) instead of
   leaning on the unified-score gauge.
3. **Capturing methodology choices** — the layout is part of
   each Brain's checked-in state. Adopters cloning a template
   inherit the layout, so authoring once produces reusable
   ergonomics.

## File location + shape

`<brain>/.claude/brain/dashboard-layout.json`. JSON object with
`schema_version`, `widgets`, optional `is_default` (always
`false` for a hand-authored layout — the server overrides this
field on read regardless).

```json
{
  "schema_version": "1",
  "widgets": [
    { "id": "ident",  "widget_type": "identity",          "size": "full",  "config": {} },
    { "id": "intro",  "widget_type": "markdown-note",     "size": "full",
      "title": "What this Brain measures",
      "config": { "content": "Brief framing for operators..." } },
    { "id": "gauge",  "widget_type": "score-gauge",       "size": "third", "config": {} },
    { "id": "strong", "widget_type": "strongest-signals", "size": "third", "config": { "count": 5 } },
    { "id": "recs",   "widget_type": "top-recommendations","size": "third", "config": { "count": 5 } }
  ]
}
```

Each widget has:

- `id` — stable string. Used as the React key + as a target by
  edit-mode operations.
- `widget_type` — one of the registered types (catalog below).
  Unknown types render an `[unknown widget]` placeholder rather
  than blanking the page; forward-compatible.
- `size` — `full | half | third | quarter`. Maps to 12/12, 6/12,
  4/12, 3/12 of the row width. Widgets autoflow left-to-right;
  rows wrap at width 1.0.
- `title` — optional override for the widget's header. `null`
  uses the widget's default title.
- `config` — widget-specific. Each type's required fields are
  documented below. Pass `{}` for widgets that take no config.

## Widget catalog (v3.4)

| Widget type | Reads | Required config | Notes |
|-------------|-------|-----------------|-------|
| `identity` | overview | — | Brain identity card: project label, root, domain count + weighted/advisory split, federation peer count. Always lead with this widget. |
| `score-gauge` | overview | — | Radial gauge + trajectory badge. Renders "N/A · observe-only" honestly when the Brain is all-advisory. Best at `third` width. |
| `strongest-signals` | overview | — | Top N domains by effective_score. Optional `count` (default 3). |
| `top-recommendations` | overview | — | Top N gates / calls to action. Optional `count` (default 3). |
| `domain-card` | self-fetches /domains/:name | `domain` (string) | Single-domain stat card: score, weight, confidence, trajectory. Click drills into the domain detail page. Self-fetches so a slow A2A pull doesn't stall the layout. |
| `markdown-note` | none | `content` (string) | Free-text card. Inline `**bold**`, `*italic*`, `` `code` `` rendered safely (HTML-escaped first; no XSS surface). Use for framing notes, posture explanations, or short directives. |

## Size guidance

- **`full`** — identity, framing notes, section headers (a
  `markdown-note` with just a title can divide a layout into
  visual sections).
- **`half`** — strong/weak signal callouts when you want them
  prominent; pairs of related cards.
- **`third`** — the canonical scan-it-quickly row. The gauge,
  strongest signals, and top recommendations as `third × 3`
  fit one row at typical viewport widths.
- **`quarter`** — best for 4 same-kind cards in one row (e.g.,
  4 child-Brain `domain-card`s in a federation parent's layout).

## Common patterns

### Pattern 1 — Weighted Brain (single project)

```
identity (full)
score-gauge (third) | strongest-signals (third) | top-recommendations (third)
[optional] weighted-domain cards (third × 3) showing the domains feeding the score
```

This is the v3.4 posture-aware default for any Brain with
weighted domains. Add the bottom row of weighted-domain cards
when you want operators to see *what's feeding the score* as
prominently as the score itself — useful for adopters
calibrating which advisory domains to promote next.

### Pattern 2 — All-advisory federation parent (e.g., ecosystem)

```
identity (full)
markdown-note (full): "Observe-only posture — N/A is correct"
domain-card child-A (quarter) | child-B (quarter) | child-C (quarter) | child-D (quarter)
strongest-signals (half) | top-recommendations (half)
```

Lifts each `child-*` domain to a first-class card. Solves the
"all-advisory N/A is honest but unhelpful" problem by surfacing
the children's substantive scores on the parent's homepage.
Click any child card to drill into that child's full dashboard
via the multi-Brain navigation.

### Pattern 3 — Spec-quality Brain (e.g., LSP-Brains)

```
identity (full)
markdown-note (full): explain "this Brain scores a spec, not a codebase"
domain-card (third) × 6: spec-completeness, schema-validity, link-integrity,
                        glossary-freshness, diagram-sync, rfc-2119-compliance
strongest-signals (half) | top-recommendations (half)
```

The 6-card grid surfaces what spec quality means in this Brain's
specific framing. The framing note is critical context for an
operator opening the dashboard cold.

### Pattern 4 — Pilot adopter (e.g., job-hunt)

```
identity (full)
markdown-note (full): pilot context + goal-spec pointer
two highest-touch domains (half | half)
six work-signal domains (third × 6, two rows)
strongest-signals (half) | top-recommendations (half)
```

Lead with the operationally-critical domains, then the medium-
touch signals. The framing note doubles as documentation for
anyone evaluating the methodology against this real-world use.

## Edit mode workflow

The dashboard ships an in-page editor — operators don't have to
hand-write JSON unless they want to:

1. **Customize button** appears on every Overview page (in the
   default-layout banner when applicable, or via the toolbar
   for custom layouts).
2. **Per-widget controls** — ↑ ↓ reorder, size dropdown, ✕ remove.
   Plus inline config editors for `domain-card` (domain id text
   input) and `markdown-note` (title + content textarea).
3. **Add widget** — picker dropdown + `+ Add` button. Inserts
   at the end with reasonable defaults; operator fills in
   widget-specific config.
4. **Save** — PUT to `/api/brains/:id/dashboard-layout`. Writes
   atomically (temp file + rename) so concurrent readers never
   see a half-written file. Server fires a `LayoutChanged` SSE
   event; all open dashboards (this brain + any peer dashboards)
   pick up the change within ~250ms.
5. **Reset to default** — DELETEs the file. The synthesized
   posture-aware default takes over.

For agent ergonomics: an operator asking "make the homepage
focus on X" can be served either by editing the JSON file
directly (precise, version-controlled) or by walking them
through the edit-mode UI.

## Tips for agents authoring layouts

- **Always lead with `identity`** as the first widget. It's the
  "what is this Brain" card; orienting context for any reader.
- **Prefer one `markdown-note` framing card** near the top when
  the Brain's posture is non-obvious (all-advisory, observe-only,
  pilot, spec-vs-codebase). One paragraph; uses **bold** for
  the headline phrase.
- **Don't stuff every domain on the homepage.** Pick the
  operationally-critical ones; trust the Domains page for the
  full list. A reasonable upper bound is 6-8 domain-cards on
  Overview before the page gets noisy.
- **Match `quarter` × 4** when you have exactly four same-kind
  items (children, supply-chain layers, etc.) so they fit one
  row cleanly.
- **The `is_default` field is server-managed.** Setting it in
  the JSON file does nothing — the server overrides on read.
  Don't waste tokens on it.
- **The widget `id` only needs to be stable within the layout.**
  Use semantic ids (`score-gauge`, `child-neurogrim`) for
  readability; auto-generated ids are fine if you're scripting
  layouts.

## Cross-references

- `neurogrim explain ui` — the dashboard's overall surface
  (the five pages, multi-Brain navigation, hat lens, theme)
- `neurogrim explain federation` — fractal composition + A2A
  peers, which underpins the child-card patterns
- `neurogrim explain domain` — what a domain is (so layouts
  reference real things)
- README "The dashboard (v3.4)" — quickstart for operators
