//! Wire-format types for dashboard API endpoints.
//!
//! These are dashboard-specific DTOs — NOT the canonical
//! `neurogrim_core::AgentOutput` shape. Each endpoint owns its own
//! contract; the route handler converts the underlying core data
//! (e.g., `AgentOutput`, `BrainRegistry`) into one of these for
//! transmission.
//!
//! `ts-rs` generates TypeScript types from each `#[derive(TS)]`
//! struct at `cargo test` time. The output lives in
//! `neurogrim-dashboard/bindings/` and is committed to git so the
//! frontend's `tsc` typechecking + the published-crate distribution
//! both have access to up-to-date types.
//!
//! Generation command:
//! ```bash
//! cargo test -p neurogrim-dashboard --lib export_bindings
//! ```
//!
//! Drift gate: a CI step diffs `bindings/` against the latest
//! committed version; non-empty diff = fail. Plan reference:
//! v3.4 Phase 0.4 in `audit/dec-...`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Response body of `GET /api/health`. The simplest endpoint —
/// proves the wire-format pipeline works. Returns ok=true plus a
/// few diagnostic fields the frontend can use to detect server/
/// client version drift.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HealthResponse {
    pub ok: bool,
    pub registry_path: String,
    /// Server version (matches the dashboard binary's
    /// `CARGO_PKG_VERSION`). Frontends bundled at v3.4 connecting
    /// to a v3.5 server can warn the operator.
    pub version: String,
    /// v3.5.0 — true when the dashboard was started with
    /// `--allow-mutations`. The frontend reads this on first load
    /// to decide whether to render mutation-shaped UI (Start/Stop
    /// buttons, sensor refresh, etc.) so we avoid 403 round-trips
    /// for buttons the user can't use.
    pub mutations_allowed: bool,
}

/// Lightweight, prose-tuned summary of a Brain's current state,
/// powering the dashboard's Overview page.
///
/// Distinct from `neurogrim_core::AgentOutput`: this DTO is
/// dashboard-curated (top 3 recs, top 3 strongest signals,
/// federation peer count rather than full peer list) — the goal
/// is "what does a human want to see on first glance," not
/// "every spec'd field." The full AgentOutput is available via
/// `GET /api/agent` for consumers that need it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct OverviewResponse {
    /// Display name for the Brain (from `meta.description` first
    /// sentence; falls back to project_root basename).
    pub project_label: String,
    /// Filesystem path to the registry, surfaced for operator
    /// awareness (which Brain is being shown).
    pub project_root: String,
    /// Total declared domains.
    pub domain_count: u32,
    /// Subset that are weighted (weight > 0).
    pub weighted_count: u32,
    /// Subset that are advisory (weight == 0).
    pub advisory_count: u32,
    /// Unified score 0..=100. None when the Brain is all-advisory
    /// (in that case the score is structurally 0 / N/A).
    pub score: Option<u8>,
    /// Weighted-mean confidence 0..=100 across non-advisory
    /// domains. None paired with `score: None` for all-advisory.
    pub confidence: Option<u8>,
    /// Trajectory classification ("improving" | "degrading" |
    /// "stable" | "volatile" | "no-data"). Stringly-typed at the
    /// wire to keep the frontend simple; mapped to TrajectoryKind
    /// in TS.
    pub trajectory_class: String,
    /// Trajectory velocity (signed; positive = improving).
    pub trajectory_velocity: f64,
    /// Number of score-history samples observed.
    pub trajectory_samples: u32,
    /// Top recommendations (up to 3).
    pub top_recommendations: Vec<RecommendationDto>,
    /// Top strongest signals (up to 3 highest effective scores).
    pub strongest_signals: Vec<DomainSignalDto>,
    /// Count of declared federation peers.
    pub federation_peer_count: u32,
}

/// Per-domain summary used in `OverviewResponse.strongest_signals`
/// and the Domains-page table. The full per-domain detail (CMDB
/// findings, history) is fetched separately via
/// `GET /api/domains/:name`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainSignalDto {
    pub name: String,
    /// Humanized display (from registry.principle_map; falls back
    /// to the kebab-case name).
    pub display_name: String,
    pub effective_score: u8,
    pub confidence: u8,
    pub weight: f64,
}

/// Recommendation summary used in `OverviewResponse.top_recommendations`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct RecommendationDto {
    pub domain: String,
    pub gate: String,
    pub status: String,
    pub command: String,
    /// Short human-readable rationale (from
    /// `Recommendation::description`); may be empty if the source
    /// recommendation didn't carry one.
    pub description: String,
}

// =================================================================
// Phase 1.2 — Domains page + detail
// =================================================================

/// Response body of `GET /api/domains` — the sortable list view that
/// powers the Domains page table.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainsListResponse {
    pub domains: Vec<DomainListItemDto>,
}

/// One row in the domains table. Includes everything sortable from
/// the page UI (name, weight, scores, confidence, trajectory).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainListItemDto {
    pub name: String,
    pub display_name: String,
    pub weight: f64,
    pub raw_score: u8,
    pub effective_score: u8,
    pub confidence: u8,
    /// Trajectory classification at the per-domain level. One of
    /// "improving" | "degrading" | "stable" | "volatile" | "no-data".
    pub trajectory_class: String,
    pub trajectory_velocity: f64,
    pub trajectory_samples: u32,
    /// CMDB `meta.updated_at` (ISO 8601 UTC). `None` when the CMDB
    /// is missing on disk (the domain registers but no sensor has
    /// produced a CMDB yet — the v3.2 stub-domain pattern).
    pub last_updated: Option<String>,
}

/// Response body of `GET /api/domains/:name` — the drill-in detail.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainDetailResponse {
    pub name: String,
    pub display_name: String,
    pub weight: f64,
    pub raw_score: u8,
    pub effective_score: u8,
    pub confidence: u8,
    pub trajectory_class: String,
    pub trajectory_velocity: f64,
    pub trajectory_samples: u32,
    /// Operator-supplied sensor authoring intent — the
    /// `_todo_<name>` placeholder on the domain definition (set
    /// via `domain new --sensor-intent`). Useful when the sensor
    /// hasn't been written yet. None when absent.
    pub sensor_intent: Option<String>,
    /// CMDB findings array (preserved verbatim from the on-disk
    /// envelope). Empty array when no CMDB exists or no findings
    /// were emitted.
    pub findings: Vec<FindingDto>,
    /// Score-history sparkline data. Each entry is one tick. May
    /// be empty (Brain just initialized) or sparse.
    pub history: Vec<HistoryPointDto>,
    /// Filesystem path to the CMDB JSON, displayed for operator
    /// awareness ("which file would I edit to refresh this").
    pub cmdb_path: String,
    /// CMDB `meta.updated_at` if present; None when no CMDB on disk.
    pub last_updated: Option<String>,
}

/// CMDB finding entry. Schema mirrors the canonical
/// `cmdb-envelope-v1.schema.json` finding shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct FindingDto {
    pub name: String,
    pub status: String,
    pub points: i32,
    /// Optional human-readable explanation. Sensors SHOULD include
    /// it on warnings/errors; for "pass" findings it's commonly
    /// absent.
    pub detail: Option<String>,
}

/// One point in a domain's score-history sparkline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HistoryPointDto {
    /// ISO 8601 UTC timestamp.
    pub scored_at: String,
    pub score: u8,
    pub confidence: u8,
}

// =================================================================
// Phase 1.3 — Federation page
// =================================================================

/// Response body of `GET /api/federation` — the Brain's view of its
/// federation: who it is, who its A2A peers are, and whether each
/// peer is currently reachable.
///
/// The dashboard renders this as a topology diagram (self in center,
/// peers around) plus a per-peer detail panel.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct FederationResponse {
    pub self_brain: SelfBrainDto,
    pub peers: Vec<PeerDto>,
    /// Schema version of the registry — surfaced so the page can
    /// warn if the operator's registry predates `read_only` support
    /// (anything before 2.1).
    pub registry_schema_version: String,
}

/// "Self" — what the Brain says about itself when describing the
/// federation. Mirrors the small subset of identity the operator
/// usually wants to confirm at a glance.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SelfBrainDto {
    /// Display label for this Brain (registry.meta.description first
    /// sentence; falls back to project_root basename).
    pub label: String,
    /// Filesystem path to the project root.
    pub project_root: String,
    /// Server version (CARGO_PKG_VERSION).
    pub version: String,
}

/// One declared A2A / subprocess peer in the registry's
/// `config.children` block, augmented with a freshness probe for A2A
/// peers (best-effort, capped timeout).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PeerDto {
    /// kebab-case peer id (key under `config.children`).
    pub name: String,
    pub display_name: String,
    /// Either "a2a" or "subprocess" — selects how the parent reaches
    /// this peer. Subprocess peers are NOT probed; their status is
    /// always `unprobed`.
    pub transport: String,
    /// A2A endpoint URL when transport == "a2a"; `None` otherwise.
    pub a2a_endpoint: Option<String>,
    /// Filesystem path to the child Brain (subprocess transport) or
    /// the relative path declared alongside the A2A endpoint. May be
    /// absent for purely-remote peers.
    pub brain_path: Option<String>,
    pub weight: f64,
    /// Read-only sibling per LSP-Brains v2.1+ (does not influence the
    /// parent's score; observation only).
    pub read_only: bool,
    pub enabled: bool,
    /// Freshness probe outcome. `unprobed` for disabled or non-A2A
    /// peers; `alive` / `unreachable` for A2A peers we tried to reach.
    pub status: PeerStatusDto,
    /// When status == "alive", the relevant fields from the peer's
    /// Agent Card. None on probe failure or non-A2A transports.
    pub agent_card: Option<AgentCardExcerptDto>,
}

/// Status taxonomy for federation peers. Stringly-typed at the wire
/// (matches the rest of this module's stringly-typed enums) and
/// re-narrowed in TS via a discriminated union.
///
/// The two-stage probe (Phase 2.6.1) splits the old catch-all
/// `unreachable` into three more specific outcomes so the dashboard
/// can tell the operator *why* a peer isn't responding:
///
/// - **alive** — Agent Card fetched successfully.
/// - **not-running** — TCP connection refused. The clearest signal
///   that the A2A daemon isn't listening on the declared port.
/// - **unhealthy** — TCP connected but the Agent Card fetch failed
///   or timed out. The process is running but not serving the
///   well-known endpoint cleanly.
/// - **unreachable** — Network-level failure (DNS, no route, etc.)
///   or a TCP-connect timeout that wasn't a refusal. Catch-all for
///   anything that's not one of the above.
/// - **unprobed** — subprocess transport (we don't probe those).
/// - **disabled** — `enabled: false` in the registry.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PeerStatusDto {
    /// One of "alive" | "not-running" | "unhealthy" | "unreachable"
    /// | "unprobed" | "disabled".
    pub kind: String,
    /// Operator-facing detail (error message, "subprocess transport",
    /// etc.). Empty when the status is self-explanatory.
    pub message: String,
}

/// Subset of the peer's Agent Card that's interesting to show in the
/// dashboard. The full card is intentionally not surfaced — operators
/// who need it can run `neurogrim a2a-discover <url>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct AgentCardExcerptDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub interface_version: String,
    pub schema_version: String,
    /// "http+sse" | "json-rpc" — peer's declared transport protocol.
    pub transport_protocol: String,
    /// Topology role from the Agent Card (`project` | `ecosystem` |
    /// `local` | `external`). None when the peer didn't declare one.
    pub topology_role: Option<String>,
    /// Topology parent id when declared (typical for child Brains).
    pub topology_parent_id: Option<String>,
}

// =================================================================
// Phase 1.4 — Skills page
// =================================================================

/// Response body of `GET /api/skills` — inventory + hygiene of every
/// skill the Brain can route to under `.claude/skills/`.
///
/// The dashboard renders this as a filterable table grouped by
/// hygiene status (alive / dead / new), with click-to-expand for the
/// frontmatter description.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SkillsResponse {
    pub skills: Vec<SkillDto>,
    /// True when `.claude/brain/invocation-ledger.jsonl` exists. When
    /// false, all skills will have hygiene_status = "no-ledger" — the
    /// page surfaces a banner explaining that the PostToolUse hook
    /// hasn't been wired up yet.
    pub ledger_present: bool,
    /// Total skill-typed entries in the ledger (any age). Useful as a
    /// "ledger isn't empty, just has no recent activity" sanity signal.
    pub total_invocations: u32,
    /// Window (in days) used to classify alive/dead. Surfaced so the
    /// page can label the legend ("alive = invoked in last 30 days").
    pub alive_window_days: u32,
}

// =================================================================
// Path 2 — Multi-Brain navigation
// =================================================================

/// Response body of `GET /api/brains` — every Brain reachable from
/// the host registry, transitively walked through `config.children`.
///
/// The dashboard uses this to populate the AppShell's Brain
/// switcher and to validate the `:brain_id` URL path segment.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct BrainsListResponse {
    pub self_id: String,
    pub brains: Vec<BrainListItemDto>,
}

/// One Brain in the federation tree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct BrainListItemDto {
    /// kebab-case id (URL-safe). Derived from `meta.project` or
    /// project_root basename. Stable across server restarts.
    pub id: String,
    /// Human-readable name from `meta.project` (or the id when that
    /// field is absent).
    pub display_name: String,
    /// Filesystem path to the project root.
    pub project_root: String,
    /// id of the parent Brain in the federation tree, or `null` for
    /// the host.
    pub parent_id: Option<String>,
    /// 0 for the host, 1 for direct children, 2 for grandchildren.
    pub depth: u32,
}

// =================================================================
// Phase 2.2 — Hat lens
// =================================================================

/// Response body of `GET /api/hats` — every hat declared in the
/// registry, plus a synthetic "default" entry the picker uses to
/// surface the un-hatted lens.
///
/// The dashboard renders this as a dropdown in the AppShell. When
/// the user picks a hat, every score-aware query re-fetches with
/// `?hat=<name>` so the Brain output is filtered through that
/// hat's `domain_multipliers`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HatsResponse {
    pub hats: Vec<HatDto>,
}

/// One hat declaration. Mirrors registry's `config.hats.<name>`
/// minus the scoring internals (`domain_multipliers`, `suggest_when`)
/// — the dashboard only needs the picker-facing surface.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HatDto {
    /// Hat id (kebab-case, e.g. "engineer", "reviewer"). The
    /// synthetic "default" entry uses the literal string `"default"`.
    pub name: String,
    /// Human-readable description from the registry (or a built-in
    /// description for the synthetic default entry).
    pub description: String,
    /// True for the synthetic "default" entry — operators see it
    /// at the top of the picker as a way to clear the lens.
    pub is_default: bool,
}

/// One row in the Skills table.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SkillDto {
    /// Skill id — kebab-case folder/file name. Matches the `name`
    /// field in invocation-ledger entries (Claude Code's per-skill
    /// surface name).
    pub name: String,
    /// Filesystem path relative to project root, e.g.
    /// `.claude/skills/rubber-duck/SKILL.md` or `.claude/skills/foo.md`.
    pub path: String,
    /// One of "plugin" (folder + SKILL.md) | "legacy" (single .md
    /// file directly under .claude/skills/).
    pub format: String,
    /// First-paragraph description. Pulled from YAML frontmatter
    /// (`description:`) for plugin skills, from the lead paragraph
    /// for legacy. May be empty if the file lacks both.
    pub description: String,
    /// Most-recent invocation timestamp from the ledger (RFC 3339).
    /// None when never invoked or no ledger.
    pub last_invoked_at: Option<String>,
    /// Total invocation count from the ledger (all time, not windowed).
    /// Sum of hard + soft.
    pub invocation_count: u32,
    /// Invocations in the alive_window. Used to drive the
    /// alive/dead/new classification and shown in the table for
    /// at-a-glance freshness. Sum of hard + soft.
    pub recent_invocation_count: u32,
    /// "Hard" invocations: explicit `Skill` tool calls (slash
    /// commands or `Skill(name=...)`). All-time.
    pub hard_invocations: u32,
    /// "Soft" invocations: agent reads of the SKILL.md file via
    /// the Read tool. All-time. Captures the more common usage
    /// pattern where an agent follows skill guidance without
    /// going through the Skill tool. Schema-1 ledger entries
    /// (pre-soft tracking) are counted as hard.
    pub soft_invocations: u32,
    /// Hard invocations within the alive_window.
    pub recent_hard_invocations: u32,
    /// Soft invocations within the alive_window.
    pub recent_soft_invocations: u32,
    /// One of "alive" | "dead" | "new" | "no-ledger".
    /// - alive: at least one invocation in the alive_window
    /// - dead: invocations exist but none in the alive_window
    /// - new: never invoked
    /// - no-ledger: ledger file is missing entirely (PostToolUse
    ///   hook hasn't been wired up)
    pub hygiene_status: String,
}

// ── Explain content for inline help (S15-C-8 v1) ─────────────────────────

/// Response body of `GET /api/explain/:topic` — the markdown text of
/// a bundled `neurogrim explain` topic, returned verbatim. Used by
/// the Settings page's `?` HelpIcon to render the relevant topic
/// in a modal.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ExplainTopicResponse {
    pub name: String,
    pub content: String,
}

// ── Custom pages CRUD (S15-C-6 v1) ───────────────────────────────────────

/// Request body of `POST /api/brains/:brain_id/dashboard-pages/:name`.
/// v1 is name-only — operator picks an icon + adds widgets later via
/// the Add Widget flow (deferred).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct CreateCustomPageRequest {
    /// Optional title shown in the sidebar; defaults to the page id.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
}

/// Response body of the `POST` + `DELETE` page endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct CustomPageMutationResponse {
    pub ok: bool,
    pub name: String,
}

// ── Registry editor (S15-C-4 v1) ─────────────────────────────────────────

/// Response body of `GET /api/brains/:brain_id/registry` — full
/// registry JSON + ETag fingerprint for conflict detection.
///
/// **v1 scope:** the response is the raw registry JSON (as
/// `serde_json::Value`) plus an ETag derived from the file's bytes.
/// Curated forms on the frontend extract specific sections; the
/// schemars-driven full form generator + 3-way merge UI are
/// deferred until adopters need them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct RegistryResponse {
    pub brain_id: String,
    pub path: String,
    /// SHA-256 of the file bytes at read-time, hex-encoded. The
    /// frontend echoes this back on PUT; the server rejects with
    /// 409 Conflict when the on-disk fingerprint differs (someone
    /// else edited the file in the interim). v1 conflict mitigation;
    /// 3-way merge UI is C-4 v2.
    pub etag: String,
    /// Parsed registry JSON. Operator's curated forms extract
    /// specific sections; the textarea-fallback editor renders the
    /// raw object.
    #[ts(type = "Record<string, unknown>")]
    pub registry: serde_json::Value,
}

/// Request body of `PUT /api/brains/:brain_id/registry`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct RegistryUpdateRequest {
    /// ETag the client received on the GET. Server rejects with
    /// 409 Conflict when the on-disk fingerprint differs.
    pub expected_etag: String,
    /// Replacement registry JSON. Server validates via
    /// `BrainRegistry::from_json` + `registry.validate()` before
    /// writing.
    #[ts(type = "Record<string, unknown>")]
    pub registry: serde_json::Value,
}

// ── Settings page config-file viewer (S15-C-5) ───────────────────────────

/// Response body of `GET /api/brains/:brain_id/config-file/:name` —
/// raw text + presence for the operator-facing read-only Settings
/// viewers. Hardcoded allowlist (culture.yaml, queue-config.yaml)
/// keeps the surface tight.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ConfigFileResponse {
    /// Logical name of the file (matches the URL path segment).
    pub name: String,
    /// True when the file exists on disk.
    pub present: bool,
    /// Resolved on-disk path (for operator diagnostics).
    pub path: String,
    /// File text when `present`. None on absent or read-error.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text: Option<String>,
    /// Error message when read failed despite the file existing.
    /// `present: false` + `error: None` means the file simply
    /// doesn't exist (the common case for adopters who haven't
    /// authored the manifest yet).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

// ── Approvals page (S13-B-6) ─────────────────────────────────────────────

/// Response body of `GET /api/brains/:brain_id/approvals` — pending
/// + recently-resolved autonomy approvals. Backs the new
/// `/brains/:id/approvals` page; agents calling mutation tools that
/// resolve to `Approve` autonomy land entries on
/// `_neurogrim/approvals` and operators resolve them here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ApprovalsPageResponse {
    /// Approval requests that haven't been resolved yet. Joined
    /// from `_neurogrim/approvals` minus anything in
    /// `_neurogrim/approval-resolutions`. Newest-first.
    pub pending: Vec<ApprovalRequestView>,
    /// Recently-resolved approvals. Newest-first; capped at 50.
    pub recent_resolutions: Vec<ApprovalResolutionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ApprovalRequestView {
    pub action_id: String,
    pub tool: String,
    pub action_type: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ApprovalResolutionView {
    pub action_id: String,
    /// `"approve"` | `"deny"` (others surfaced for forward-compat).
    pub decision: String,
    pub operator: Option<String>,
    pub decided_at: String,
}

/// Request body of `POST /api/brains/:brain_id/approvals/:action_id/resolve`.
/// Operator click flow: the dashboard server reads its own
/// `$NEUROGRIM_OPERATOR` env at startup and stamps it on the
/// resolution; buttons send only `{decision}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ResolveApprovalRequest {
    /// `"approve"` | `"deny"`.
    pub decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ResolveApprovalResponse {
    pub action_id: String,
    pub decision: String,
    pub operator: Option<String>,
    pub decided_at: String,
}

// ── Coordination bus (S13-B-2) ───────────────────────────────────────────

/// Request body of `POST /api/brains/:brain_id/queues/:topic`.
/// `payload` is required; `priority` and `expires_in_ms` are
/// optional. The bus generates `id` + `produced_at`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueuePublishRequest {
    pub payload: serde_json::Value,
    /// "low" | "normal" | "high". Defaults to normal.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    /// Time-to-live in milliseconds; the bus computes
    /// `expires_at = produced_at + expires_in_ms`. Default: never.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expires_in_ms: Option<u64>,
}

/// Response body of `POST /api/brains/:brain_id/queues/:topic` — the
/// freshly-produced message's identifiers. Body of the published
/// message is NOT echoed back; consumers fetch via the read endpoint
/// or subscribe to the SSE stream.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueuePublishResponse {
    pub id: String,
    pub topic: String,
    pub produced_at: String,
}

/// One message in the wire format the dashboard returns. Mirrors
/// `neurogrim_core::queue::QueueMessage` but uses string typed fields
/// so the TS-side schema is stable across uuid / chrono crate version
/// drift.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueueMessageDto {
    pub id: String,
    pub topic: String,
    pub payload: serde_json::Value,
    pub produced_at: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expires_at: Option<String>,
}

/// Response body of `GET /api/brains/:brain_id/queues/:topic` — a
/// page of messages with the `next_offset` cursor for resume.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueueReadResponse {
    pub topic: String,
    pub messages: Vec<QueueMessageDto>,
    /// Offset to pass as `since` on the next read to resume after
    /// the last message in this page. Equals `since + messages.len()`.
    pub next_offset: u64,
}

/// Response body of `GET /api/brains/:brain_id/queues` — every
/// topic with a JSONL file on disk, plus per-topic stats.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueuesListResponse {
    pub topics: Vec<QueueTopicStatsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct QueueTopicStatsDto {
    pub topic: String,
    pub message_count: u32,
    pub size_bytes: u64,
    /// `produced_at` of the oldest message, RFC3339. None when empty.
    pub oldest: Option<String>,
    /// `produced_at` of the newest message, RFC3339. None when empty.
    pub newest: Option<String>,
}

// ── Publish-gates page (S12-G-6) ─────────────────────────────────────────

/// Response body of `GET /api/brains/:brain_id/publish-gates` —
/// renders the manual-gate UI surface (`/brains/:id/publish-gates`).
///
/// Combines the manifest (gate declarations) with the ledger (run
/// history) so the page can render "current state per gate" + a
/// recent-activity timeline in one fetch.
///
/// Empty state: `manifest_present: false`, `gates: []`,
/// `recent_ledger: []`. Page renders an explainer pointing at
/// `neurogrim explain publish-gates`.
///
/// Schema-corrupt state: `manifest_present: true`, `manifest_error:
/// Some(<schema-validation message>)`. Page surfaces a banner and
/// suggests `neurogrim doctor`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PublishGatesPageResponse {
    /// True when `<brain>/.claude/brain/publish-gates.yaml` exists
    /// on disk (regardless of whether it parses).
    pub manifest_present: bool,
    /// Schema-validation or YAML-syntax error if the manifest is
    /// present but malformed. None when the manifest is absent or
    /// valid.
    pub manifest_error: Option<String>,
    /// One entry per gate in the manifest, joined with the most
    /// recent ledger entry for that gate. Order matches the
    /// manifest's declared order (preserves operator intent).
    pub gates: Vec<PublishGateView>,
    /// Most recent N ledger entries, newest first. Capped at 50 in
    /// v1 to keep the response payload tight; the page can offer a
    /// "load more" affordance later if N=50 proves too small.
    pub recent_ledger: Vec<PublishGateLedgerView>,
}

/// A gate as it appears in the dashboard's table — manifest fields
/// + the latest ledger entry's outcome (or `no_runs` for never-run
/// gates).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PublishGateView {
    pub id: String,
    pub gate_type: String,
    pub description: String,
    pub blocking: bool,
    pub timeout_seconds: Option<u32>,
    /// Latest ledger entry's status, or `"no_runs"` if no ledger
    /// entry exists yet for this gate. One of:
    /// `passed | failed | pending | timed_out | deferred | error | no_runs`.
    pub current_status: String,
    /// `started_at` of the latest ledger entry (RFC3339). None if
    /// no ledger entry.
    pub last_run_at: Option<String>,
    /// `run_id` of the latest ledger entry. Lets the page link
    /// related ledger rows together.
    pub last_run_id: Option<String>,
    /// Operator handle from the latest ledger entry, when present
    /// (manual-gate `ack` or future `--operator` flag on automated).
    pub operator: Option<String>,
}

/// A ledger entry as the dashboard surfaces it. Mirrors
/// `LedgerEntry` from `neurogrim-cli::commands::publish_gate` but
/// omits `stdout_truncated` / `stderr_truncated` (the page doesn't
/// render full output; operators inspect the JSONL directly).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PublishGateLedgerView {
    pub run_id: String,
    pub gate_id: String,
    pub gate_type: String,
    pub mode: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub blocking: bool,
    pub operator: Option<String>,
    pub exit_code: Option<i32>,
    pub error_detail: Option<String>,
}

#[cfg(test)]
mod tests {
    /// Compile-time-style check: all #[derive(TS)] types in this
    /// module re-export to `bindings/` on `cargo test`. The actual
    /// file production happens via the `TS::export_all` machinery
    /// invoked by ts-rs's test harness — declaring the test target
    /// here keeps the trigger discoverable from `cargo test -p
    /// neurogrim-dashboard --lib`.
    ///
    /// (No assertions: ts-rs's test-time generator does the work
    /// during `cargo test`; this test just gives operators a
    /// well-named place to look when they wonder "where's the
    /// bindings generation entry point.")
    #[test]
    fn export_bindings_marker() {
        // Intentionally empty.
    }
}
