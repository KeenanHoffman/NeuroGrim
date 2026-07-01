---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# DESIGN-SQL.md — Proposed shape for SQL-DSP v0

> *Pre-prototype design. Subject to change as tasks expose what
> actually matters. Do not implement until task specs in `tasks/`
> are written and at least one is hand-traced through this design
> to validate it.*

## Protocol skeleton

JSON-RPC 2.0, same shape as LSP. Methods grouped by intent:

### Discovery — "what's here?"

```ts
// Lists all schemas in the database (e.g. "main", "temp" for SQLite;
// multiple for Postgres).
db/listSchemas() → SchemaInfo[]

// Lists all tables/views in a schema, with row count + size estimates.
db/listObjects(schema: string) → ObjectInfo[]

// Returns the structural shape of a table or view: columns (name, type,
// nullable, default), primary key, foreign keys, indexes, triggers.
// Optionally includes sample-value summaries for each column (top-N values,
// null rate, distinct count) — gated by `with_stats: bool`.
db/describe(schema: string, object: string, with_stats?: bool)
  → ObjectShape
```

### Navigation — "where does this go?"

```ts
// Foreign-key resolution: given a row's value for a FK column, return the
// referenced row from the parent table. The DSP equivalent of LSP's
// `textDocument/definition`.
db/resolveRef(
  schema: string,
  table: string,
  column: string,
  value: JsonValue,
) → RowRef | null

// Reverse FK: given a primary-key value, find every row that points at it.
// LSP's `textDocument/references`. Bounded by `limit` to keep responses
// manageable on hot rows.
db/findRefs(
  schema: string,
  table: string,
  pk: JsonValue,
  limit: number = 100,
) → RowRef[]
```

### Inline metadata — "what is this thing?"

```ts
// Returns rich, contextualized information about a column: type, nullability,
// FK target if any, sample distribution, comment/docs, indexes that touch
// it. The DSP equivalent of LSP's `textDocument/hover`.
db/hover(schema: string, table: string, column: string)
  → ColumnHover

// Same for a row — given a PK value, return the row plus inline
// resolution of foreign keys (e.g. instead of just `user_id: 42`, also
// returns `user_id_resolved: { name: "Alice", ... }`).
db/inspectRow(
  schema: string,
  table: string,
  pk: JsonValue,
  resolve_depth: number = 1,
) → RowInspection
```

### Quality / safety — "is this query OK?"

```ts
// Static-analyze a SQL string against the live schema before executing.
// Returns diagnostics: missing tables, missing columns, type mismatches,
// implicit cross-joins (Cartesian products), missing indexes that would
// matter. LSP's `textDocument/publishDiagnostics`.
db/diagnoseQuery(query: string) → Diagnostic[]

// Returns the query plan as a *structured tree*, not a text blob.
// Includes per-node cost estimates, suspected index usage, expected row
// counts. Lets the agent reason about performance structurally.
db/explainQuery(query: string) → PlanNode
```

### Escape hatch — "I'll just write SQL"

```ts
// The agent can always drop to raw SQL. The DSP doesn't try to replace
// SQL — it tries to add semantic primitives ON TOP. Returns rows as
// typed `JsonValue[][]` plus column metadata.
db/execute(query: string, parameters?: JsonValue[]) → ExecuteResult
```

## What we explicitly chose NOT to include in v0

Documenting these so future-us doesn't accidentally drift in:

### No fake file system

`db/listObjects` returns structured `ObjectInfo`, not a directory listing.
`db/inspectRow` returns a typed `RowInspection`, not a file blob. We
*never* return paths or pretend rows are files. Every method takes typed
identifiers (`schema`, `table`, `pk`).

### No `db/listRows` / "show me the data"

It's tempting to add a method that paginates rows from a table. We didn't
because it's a thin wrapper around `db/execute("SELECT * FROM t LIMIT n")`
that adds nothing semantic. If agents want bulk row iteration, they should
write SQL — that's exactly what SQL is good at.

### No completion methods

LSP has `textDocument/completion`. The DSP analogue ("given a partial
query, what could you be writing?") is appealing but premature. Modern
agents can write SQL fluently from a fresh prompt; the value of completion
is contextual to a typing UX, which agents don't have. If the task specs
later show agents repeatedly mistyping column names, we add it.

### No write methods (INSERT / UPDATE / DELETE)

DSP v0 is **read-only**. Mutations are stateful in ways that need
transaction semantics, ack/audit ledgers, and approval gates that
NeuroGrim's existing autonomy framework already handles. Mixing those
into DSP would either duplicate that machinery or compromise it.
`db/execute` accepts mutating SQL but the protocol doesn't surface it
as a first-class concept. Maybe DSP-Write is a v2 question.

### No cross-database joins, federated queries, materialized views

If agents want them, they write the SQL.

## Type sketches

```ts
type SchemaInfo = {
  name: string;
  description?: string;       // from comment if present
  object_count: number;
};

type ObjectInfo = {
  schema: string;
  name: string;
  kind: 'table' | 'view' | 'materialized_view';
  approximate_row_count: number | null;  // null if not cheaply available
  size_bytes: number | null;
};

type ObjectShape = {
  schema: string;
  name: string;
  kind: ObjectKind;
  columns: ColumnShape[];
  primary_key: string[];
  foreign_keys: ForeignKeyShape[];
  indexes: IndexShape[];
  stats?: ObjectStats;        // present iff with_stats=true
};

type ColumnShape = {
  name: string;
  type: string;               // raw DB type string
  nullable: boolean;
  default?: string;
  comment?: string;
  // hover-friendly augmentations:
  is_primary_key: boolean;
  is_foreign_key: boolean;
  fk_target?: { schema: string; table: string; column: string };
  index_membership: string[];  // names of indexes touching this column
};

type ForeignKeyShape = {
  name: string;
  columns: string[];
  references: { schema: string; table: string; columns: string[] };
  on_delete: 'no_action' | 'cascade' | 'set_null' | 'restrict';
  on_update: 'no_action' | 'cascade' | 'set_null' | 'restrict';
};

type RowRef = {
  schema: string;
  table: string;
  pk: JsonValue;
  excerpt?: Record<string, JsonValue>;  // human-readable preview columns
};

type ColumnHover = {
  shape: ColumnShape;
  stats?: {
    distinct_count_approx?: number;
    null_count_approx?: number;
    top_values?: { value: JsonValue; count: number }[];
    min?: JsonValue;
    max?: JsonValue;
  };
  recent_writes?: { ts: string; rows_changed: number }[];  // if observable
};

type Diagnostic = {
  severity: 'error' | 'warning' | 'hint';
  message: string;
  range?: { start: number; end: number };  // byte offset into the query
  code?: string;                            // e.g. "missing_table"
};

type PlanNode = {
  op: string;                  // e.g. "scan", "index_seek", "join", "agg"
  estimated_rows: number;
  estimated_cost: number;
  details: Record<string, JsonValue>;
  children: PlanNode[];
};

type ExecuteResult = {
  columns: { name: string; type: string }[];
  rows: JsonValue[][];
  truncated: boolean;
  duration_ms: number;
};
```

## Open questions for the prototype

These don't need answers before writing tasks, but they need answers
before code:

1. **How does DSP identify rows for `resolveRef` / `inspectRow` when the PK
   is composite?** Pass an array of values? An object keyed by column?
2. **What's the response budget per method?** `findRefs` on a hot row
   could match millions. We default `limit: 100` but the protocol needs
   to surface that truncation honestly.
3. **Does `db/describe` cache?** Schema queries are expensive on Postgres
   (information_schema joins). Server-side caching with invalidation
   needs a story.
4. **Authentication / connection management.** LSP servers are launched
   per-workspace. DSP servers need credentials. Probably handled by
   NeuroGrim's existing `secret-refs` flow but the protocol needs to
   acknowledge it.
5. **Streaming for large `db/execute` results.** LSP has progress
   notifications. DSP probably needs the same for row sets that exceed
   a single response budget. Not v0.
