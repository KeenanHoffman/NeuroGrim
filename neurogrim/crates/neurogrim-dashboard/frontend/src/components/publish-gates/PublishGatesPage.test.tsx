import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { PublishGatesPage } from "./PublishGatesPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { PublishGatesPageResponse } from "@bindings/PublishGatesPageResponse";
import type { PublishGateView } from "@bindings/PublishGateView";
import type { PublishGateLedgerView } from "@bindings/PublishGateLedgerView";

/**
 * S12-G-6 vitest — covers the four render branches of the
 * PublishGatesPage: empty state, malformed-manifest banner,
 * gate-table state, and recent-ledger transitions across the status
 * vocabulary (passed / failed / pending / timed_out / deferred /
 * error / no_runs).
 */

const gate = (overrides: Partial<PublishGateView> = {}): PublishGateView => ({
  id: "tests-pass",
  gate_type: "automated",
  description: "All tests green",
  blocking: true,
  timeout_seconds: 120,
  current_status: "passed",
  last_run_at: "2026-04-29T12:00:00Z",
  last_run_id: "run-1",
  operator: null,
  ...overrides,
});

const ledgerEntry = (
  overrides: Partial<PublishGateLedgerView> = {},
): PublishGateLedgerView => ({
  run_id: "run-1",
  gate_id: "tests-pass",
  gate_type: "automated",
  mode: "full",
  started_at: "2026-04-29T12:00:00Z",
  completed_at: "2026-04-29T12:00:01Z",
  status: "passed",
  blocking: true,
  operator: null,
  exit_code: 0,
  error_detail: null,
  ...overrides,
});

const resp = (
  overrides: Partial<PublishGatesPageResponse> = {},
): PublishGatesPageResponse => ({
  manifest_present: true,
  manifest_error: null,
  gates: [],
  recent_ledger: [],
  ...overrides,
});

function mockFetch(payload: PublishGatesPageResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
  } as Response);
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <PublishGatesPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("PublishGatesPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/publish-gates");
  });

  it("renders empty state when no manifest present", async () => {
    mockFetch(resp({ manifest_present: false }));
    renderPage();
    expect(
      await screen.findByTestId("publish-gates-empty"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/no publish gates declared/i),
    ).toBeInTheDocument();
    // Empty state surfaces the explainer hint.
    expect(
      screen.getByText(/neurogrim explain publish-gates/i),
    ).toBeInTheDocument();
  });

  it("renders malformed-manifest banner with the parser error", async () => {
    mockFetch(
      resp({
        manifest_present: true,
        manifest_error:
          "publish-gates manifest failed schema validation (1 issue):\n  - /gates/0/id: pattern mismatch",
      }),
    );
    renderPage();
    expect(
      await screen.findByText(/publish-gates\.yaml is malformed/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/pattern mismatch/i)).toBeInTheDocument();
    expect(screen.getByText(/neurogrim doctor/i)).toBeInTheDocument();
  });

  it("renders gate table with gate rows from the manifest", async () => {
    mockFetch(
      resp({
        gates: [
          gate({ id: "tests-pass", current_status: "passed" }),
          gate({
            id: "review-dashboard",
            gate_type: "manual",
            description: "Operator verifies dashboard",
            current_status: "pending",
            operator: null,
          }),
        ],
      }),
    );
    renderPage();
    expect(await screen.findByTestId("publish-gates-table")).toBeInTheDocument();
    expect(screen.getByTestId("gate-row-tests-pass")).toBeInTheDocument();
    expect(
      screen.getByTestId("gate-row-review-dashboard"),
    ).toBeInTheDocument();
  });

  it("displays the right status badge for each row in the status vocabulary", async () => {
    mockFetch(
      resp({
        gates: [
          gate({ id: "g-passed", current_status: "passed" }),
          gate({ id: "g-failed", current_status: "failed" }),
          gate({ id: "g-pending", current_status: "pending" }),
          gate({ id: "g-timed_out", current_status: "timed_out" }),
          gate({ id: "g-deferred", current_status: "deferred" }),
          gate({ id: "g-error", current_status: "error" }),
          gate({ id: "g-no_runs", current_status: "no_runs" }),
        ],
      }),
    );
    renderPage();
    await screen.findByTestId("publish-gates-table");
    expect(
      screen.getAllByTestId("status-badge-passed").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-failed").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-pending").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-timed_out").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-deferred").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-error").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByTestId("status-badge-no_runs").length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("renders recent-ledger timeline when entries are present", async () => {
    mockFetch(
      resp({
        gates: [gate()],
        recent_ledger: [
          ledgerEntry({ run_id: "run-2", status: "passed" }),
          ledgerEntry({
            run_id: "run-1",
            status: "pending",
            gate_id: "review-dashboard",
            gate_type: "manual",
            started_at: "2026-04-28T10:00:00Z",
          }),
        ],
      }),
    );
    renderPage();
    expect(
      await screen.findByTestId("publish-gates-ledger"),
    ).toBeInTheDocument();
    expect(screen.getByText(/recent activity/i)).toBeInTheDocument();
  });

  it("shows operator handle for ack'd manual gates in both surfaces", async () => {
    mockFetch(
      resp({
        gates: [
          gate({
            id: "review-dashboard",
            gate_type: "manual",
            current_status: "passed",
            operator: "alice",
          }),
        ],
        recent_ledger: [
          ledgerEntry({
            gate_id: "review-dashboard",
            gate_type: "manual",
            mode: "ack",
            status: "passed",
            operator: "alice",
          }),
        ],
      }),
    );
    renderPage();
    await screen.findByTestId("publish-gates-table");
    // Operator handle visible in both the gate row and the ledger row
    // — we expect 2 occurrences of "alice".
    const occurrences = screen.getAllByText(/alice/);
    expect(occurrences.length).toBeGreaterThanOrEqual(2);
  });

  it("renders error state when fetch fails", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({}),
    } as Response);
    renderPage();
    expect(
      await screen.findByText(/failed to load publish-gates/i),
    ).toBeInTheDocument();
  });
});
