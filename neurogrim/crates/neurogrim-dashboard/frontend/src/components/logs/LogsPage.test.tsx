import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LogsPage } from "./LogsPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";

function mockFetch(map: Record<string, unknown>) {
  global.fetch = vi.fn().mockImplementation(async (url: string) => {
    for (const [pattern, payload] of Object.entries(map)) {
      if (url.includes(pattern)) {
        return {
          ok: true,
          status: 200,
          json: async () => payload,
          text: async () => JSON.stringify(payload),
        } as Response;
      }
    }
    return {
      ok: false,
      status: 404,
      json: async () => ({}),
      text: async () => "",
    } as Response;
  });
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <LogsPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("LogsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/logs");
  });

  it("renders empty state when no events", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": {
        ledger_path: "/x",
        present: false,
        total_entries: 0,
        entries: [],
      },
      notifications: {
        topic: "_neurogrim/notifications",
        messages: [],
        next_offset: 0,
      },
    });
    renderPage();
    expect(await screen.findByTestId("logs-empty")).toBeInTheDocument();
  });

  it("aggregates publish-gate ledger entries into the timeline", async () => {
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "r1",
            gate_id: "tests-pass",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: "2026-04-29T18:00:01Z",
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: { pending: [], recent_resolutions: [] },
    });
    renderPage();
    expect(await screen.findByTestId("logs-timeline")).toBeInTheDocument();
    expect(screen.getByText("tests-pass")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-passed")).toBeInTheDocument();
  });

  it("aggregates approvals into the timeline", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: {
        pending: [
          {
            action_id: "a-1",
            tool: "queue_publish",
            action_type: "mutate-state",
            requested_at: "2026-04-29T19:00:00Z",
          },
        ],
        recent_resolutions: [
          {
            action_id: "a-old",
            decision: "approve",
            operator: "alice",
            decided_at: "2026-04-29T18:00:00Z",
          },
        ],
      },
    });
    renderPage();
    expect(await screen.findByTestId("logs-timeline")).toBeInTheDocument();
    expect(screen.getByText("a-1")).toBeInTheDocument();
    expect(screen.getByText("a-old")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-pending")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-approve")).toBeInTheDocument();
  });

  it("filter chips narrow the timeline by source", async () => {
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "r1",
            gate_id: "tests-pass",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: "2026-04-29T18:00:01Z",
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: {
        pending: [],
        recent_resolutions: [
          {
            action_id: "a-1",
            decision: "approve",
            operator: "alice",
            decided_at: "2026-04-29T17:00:00Z",
          },
        ],
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    // Both visible initially.
    expect(screen.getByText("tests-pass")).toBeInTheDocument();
    expect(screen.getByText("a-1")).toBeInTheDocument();
    // Click "Approvals" chip → only approvals remain.
    fireEvent.click(screen.getByTestId("filter-approvals"));
    expect(screen.queryByText("tests-pass")).not.toBeInTheDocument();
    expect(screen.getByText("a-1")).toBeInTheDocument();
    // Click "Publish gates" chip → only gates.
    fireEvent.click(screen.getByTestId("filter-publish-gates"));
    expect(screen.getByText("tests-pass")).toBeInTheDocument();
    expect(screen.queryByText("a-1")).not.toBeInTheDocument();
  });

  // ── S15-C-2 v2: invocation-ledger source ─────────────────────

  it("aggregates invocation-ledger entries into the timeline", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": {
        ledger_path: "/x/invocation-ledger.jsonl",
        present: true,
        total_entries: 2,
        entries: [
          {
            ts: "2026-04-29T19:00:00Z",
            entry_type: "skill",
            name: "plan-critic",
            session_id: "s1abc123",
            invocation_id: "i1",
          },
          {
            ts: "2026-04-29T18:00:00Z",
            entry_type: "skill",
            name: "hats",
            session_id: "s2def456",
            invocation_id: "i2",
          },
        ],
      },
      notifications: {
        topic: "_neurogrim/notifications",
        messages: [],
        next_offset: 0,
      },
    });
    renderPage();
    expect(await screen.findByTestId("logs-timeline")).toBeInTheDocument();
    expect(screen.getByText("plan-critic")).toBeInTheDocument();
    expect(screen.getByText("hats")).toBeInTheDocument();
    // Two "invoked" outcome badges (one per row).
    expect(screen.getAllByTestId("outcome-invoked")).toHaveLength(2);
    // Session ids are truncated for display.
    expect(screen.getByText(/^s1abc123/)).toBeInTheDocument();
  });

  it("renders (no name) subject when invocation entry lacks name field", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": {
        ledger_path: "/x",
        present: true,
        total_entries: 1,
        entries: [
          {
            ts: "2026-04-29T18:00:00Z",
            entry_type: "skill",
            name: null,
            session_id: null,
            invocation_id: null,
          },
        ],
      },
      notifications: { topic: "_neurogrim/notifications", messages: [], next_offset: 0 },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    expect(screen.getByText("(no name)")).toBeInTheDocument();
  });

  // ── S15-C-2 v2: notifications source ─────────────────────────

  it("aggregates notifications into the timeline using payload.kind", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      notifications: {
        topic: "_neurogrim/notifications",
        messages: [
          {
            id: "m1",
            topic: "_neurogrim/notifications",
            payload: { kind: "agent-action-completed", severity: "info" },
            produced_at: "2026-04-29T20:00:00Z",
            priority: "normal",
          },
          {
            id: "m2",
            topic: "_neurogrim/notifications",
            payload: { event: "disk-space-low", level: "warn", detail: "C: 99%" },
            produced_at: "2026-04-29T19:30:00Z",
            priority: "normal",
          },
        ],
        next_offset: 2,
      },
    });
    renderPage();
    expect(await screen.findByTestId("logs-timeline")).toBeInTheDocument();
    expect(screen.getByText("agent-action-completed")).toBeInTheDocument();
    expect(screen.getByText("disk-space-low")).toBeInTheDocument();
  });

  it("falls back to (see payload) when notification has no recognizable subject", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      notifications: {
        topic: "_neurogrim/notifications",
        messages: [
          {
            id: "weird",
            topic: "_neurogrim/notifications",
            payload: { foo: 1, bar: [1, 2, 3] },
            produced_at: "2026-04-29T20:00:00Z",
            priority: "normal",
          },
        ],
        next_offset: 1,
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    expect(screen.getByText("(see payload)")).toBeInTheDocument();
  });

  it("survives notifications endpoint failure with empty fallback", async () => {
    // notifications endpoint returns 404 (topic file doesn't exist
    // for fresh brains); the page should still render the other
    // sources rather than blanking the whole timeline.
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "r1",
            gate_id: "tests-pass",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: "2026-04-29T18:00:01Z",
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      // No "notifications" key → falls through to 404 → fetchNotifications returns the empty fallback.
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    expect(screen.getByText("tests-pass")).toBeInTheDocument();
  });

  // ── S15-C-2 v2: filter chip counts ───────────────────────────

  it("renders filter chip counts per source", async () => {
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "r1",
            gate_id: "g1",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: null,
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: {
        pending: [],
        recent_resolutions: [
          {
            action_id: "a-1",
            decision: "approve",
            operator: "alice",
            decided_at: "2026-04-29T17:00:00Z",
          },
          {
            action_id: "a-2",
            decision: "deny",
            operator: "bob",
            decided_at: "2026-04-29T16:00:00Z",
          },
        ],
      },
      "invocation-ledger": {
        ledger_path: "/x",
        present: true,
        total_entries: 3,
        entries: [
          { ts: "2026-04-29T15:00:00Z", entry_type: "skill", name: "a", session_id: null, invocation_id: null },
          { ts: "2026-04-29T14:00:00Z", entry_type: "skill", name: "b", session_id: null, invocation_id: null },
          { ts: "2026-04-29T13:00:00Z", entry_type: "skill", name: "c", session_id: null, invocation_id: null },
        ],
      },
      notifications: {
        topic: "_neurogrim/notifications",
        messages: [
          {
            id: "n1",
            topic: "_neurogrim/notifications",
            payload: { kind: "x" },
            produced_at: "2026-04-29T12:00:00Z",
            priority: "normal",
          },
        ],
        next_offset: 1,
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    // Counts shown next to each chip label.
    const all = screen.getByTestId("filter-all");
    expect(all.textContent).toContain("(7)"); // 1+2+3+1
    const gates = screen.getByTestId("filter-publish-gates");
    expect(gates.textContent).toContain("(1)");
    const approvals = screen.getByTestId("filter-approvals");
    expect(approvals.textContent).toContain("(2)");
    const invocations = screen.getByTestId("filter-invocations");
    expect(invocations.textContent).toContain("(3)");
    const notifications = screen.getByTestId("filter-notifications");
    expect(notifications.textContent).toContain("(1)");
  });

  // ── S15-C-3 expansion: score-history source ────────────────────

  it("aggregates score-history entries with delta-encoded outcomes", async () => {
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      notifications: { topic: "_neurogrim/notifications", messages: [], next_offset: 0 },
      "score-history": {
        history_path: "/x/score-history.json",
        present: true,
        total_entries: 4,
        entries: [
          // Newest-first; backend already sorted.
          { scored_at: "2026-04-29T13:00:00Z", score: 80, delta: 2 },
          { scored_at: "2026-04-29T12:00:00Z", score: 78, delta: 0 },
          { scored_at: "2026-04-29T11:00:00Z", score: 78, delta: -2 },
          { scored_at: "2026-04-29T10:00:00Z", score: 80, delta: null },
        ],
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    // Subjects: each row shows "score N (±Δ)".
    expect(screen.getByText("score 80 (+2)")).toBeInTheDocument();
    expect(screen.getByText("score 78 (±0)")).toBeInTheDocument();
    expect(screen.getByText("score 78 (-2)")).toBeInTheDocument();
    // First-ever snapshot has no delta — surface the raw score.
    expect(screen.getByText("score 80")).toBeInTheDocument();
    // One outcome badge per direction.
    expect(screen.getByTestId("outcome-improved")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-stable")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-declined")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-first")).toBeInTheDocument();
  });

  it("renders N/A subject when score-history snapshot has null score", async () => {
    // All-advisory Brains record `score: null`; the timeline should
    // still surface those snapshots without crashing or rendering
    // "score null".
    mockFetch({
      "publish-gates": { manifest_present: false, manifest_error: null, gates: [], recent_ledger: [] },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      notifications: { topic: "_neurogrim/notifications", messages: [], next_offset: 0 },
      "score-history": {
        history_path: "/x/score-history.json",
        present: true,
        total_entries: 1,
        entries: [
          { scored_at: "2026-04-29T10:00:00Z", score: null, delta: null },
        ],
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    expect(screen.getByText("N/A")).toBeInTheDocument();
    expect(screen.getByTestId("outcome-first")).toBeInTheDocument();
  });

  it("score-history filter chip narrows the timeline to score events only", async () => {
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "r1",
            gate_id: "tests-pass",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: null,
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: { pending: [], recent_resolutions: [] },
      "invocation-ledger": { ledger_path: "/x", present: false, total_entries: 0, entries: [] },
      notifications: { topic: "_neurogrim/notifications", messages: [], next_offset: 0 },
      "score-history": {
        history_path: "/x/score-history.json",
        present: true,
        total_entries: 1,
        entries: [
          { scored_at: "2026-04-29T17:00:00Z", score: 80, delta: 2 },
        ],
      },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    // Both visible initially.
    expect(screen.getByText("tests-pass")).toBeInTheDocument();
    expect(screen.getByText("score 80 (+2)")).toBeInTheDocument();
    // Click the score-history chip → only score events remain.
    fireEvent.click(screen.getByTestId("filter-score-history"));
    expect(screen.queryByText("tests-pass")).not.toBeInTheDocument();
    expect(screen.getByText("score 80 (+2)")).toBeInTheDocument();
  });

  it("sorts entries newest first", async () => {
    mockFetch({
      "publish-gates": {
        manifest_present: true,
        manifest_error: null,
        gates: [],
        recent_ledger: [
          {
            run_id: "older",
            gate_id: "older-gate",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-28T18:00:00Z",
            completed_at: null,
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
          {
            run_id: "newer",
            gate_id: "newer-gate",
            gate_type: "automated",
            mode: "full",
            started_at: "2026-04-29T18:00:00Z",
            completed_at: null,
            status: "passed",
            blocking: true,
            operator: null,
            exit_code: 0,
            error_detail: null,
          },
        ],
      },
      approvals: { pending: [], recent_resolutions: [] },
    });
    renderPage();
    await screen.findByTestId("logs-timeline");
    const rows = screen.getAllByText(/-gate$/);
    // The first matched row (top of table) is "newer-gate" — newest first.
    expect(rows[0].textContent).toBe("newer-gate");
    expect(rows[1].textContent).toBe("older-gate");
  });
});
