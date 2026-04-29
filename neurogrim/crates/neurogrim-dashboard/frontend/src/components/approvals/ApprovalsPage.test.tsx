import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApprovalsPage } from "./ApprovalsPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { ApprovalsPageResponse } from "@bindings/ApprovalsPageResponse";
import type { ApprovalRequestView } from "@bindings/ApprovalRequestView";
import type { ApprovalResolutionView } from "@bindings/ApprovalResolutionView";

const req = (
  overrides: Partial<ApprovalRequestView> = {},
): ApprovalRequestView => ({
  action_id: "abc-123",
  tool: "queue_publish",
  action_type: "mutate-state",
  requested_at: "2026-04-29T18:00:00Z",
  ...overrides,
});

const res = (
  overrides: Partial<ApprovalResolutionView> = {},
): ApprovalResolutionView => ({
  action_id: "old-action",
  decision: "approve",
  operator: "alice",
  decided_at: "2026-04-29T17:00:00Z",
  ...overrides,
});

const resp = (
  overrides: Partial<ApprovalsPageResponse> = {},
): ApprovalsPageResponse => ({
  pending: [],
  recent_resolutions: [],
  ...overrides,
});

function mockFetch(payload: ApprovalsPageResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  } as Response);
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <ApprovalsPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("ApprovalsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/approvals");
  });

  it("renders empty state when no pending and no resolutions", async () => {
    mockFetch(resp());
    renderPage();
    expect(await screen.findByTestId("approvals-empty")).toBeInTheDocument();
    expect(
      screen.getByText(/no approvals pending or recent/i),
    ).toBeInTheDocument();
  });

  it("renders pending table with Approve/Deny buttons per row", async () => {
    mockFetch(
      resp({
        pending: [
          req({ action_id: "a-1", tool: "queue_publish" }),
          req({ action_id: "a-2", tool: "domain_new" }),
        ],
      }),
    );
    renderPage();
    expect(
      await screen.findByTestId("approvals-pending-table"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("approval-row-a-1")).toBeInTheDocument();
    expect(screen.getByTestId("approval-row-a-2")).toBeInTheDocument();
    expect(screen.getByTestId("approve-button-a-1")).toBeInTheDocument();
    expect(screen.getByTestId("deny-button-a-2")).toBeInTheDocument();
  });

  it("renders resolutions table with decision badges", async () => {
    mockFetch(
      resp({
        recent_resolutions: [
          res({ action_id: "old-1", decision: "approve" }),
          res({ action_id: "old-2", decision: "deny" }),
        ],
      }),
    );
    renderPage();
    expect(
      await screen.findByTestId("approvals-resolutions-table"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("decision-approve")).toBeInTheDocument();
    expect(screen.getByTestId("decision-deny")).toBeInTheDocument();
  });

  it("Approve button hits the resolve endpoint with decision=approve", async () => {
    let postedBody: string | undefined;
    let postedUrl: string | undefined;
    global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (init?.method === "POST") {
        postedUrl = url;
        postedBody = init.body as string;
        return {
          ok: true,
          status: 200,
          json: async () => ({
            action_id: "a-1",
            decision: "approve",
            operator: "test",
            decided_at: "2026-04-29T18:00:00Z",
          }),
          text: async () => "",
        } as Response;
      }
      // GET for the page load
      return {
        ok: true,
        status: 200,
        json: async () => resp({ pending: [req({ action_id: "a-1" })] }),
        text: async () => "",
      } as Response;
    });
    renderPage();
    const btn = await screen.findByTestId("approve-button-a-1");
    fireEvent.click(btn);
    await waitFor(() => expect(postedUrl).toBeDefined());
    expect(postedUrl).toContain("/approvals/a-1/resolve");
    expect(postedBody).toContain("\"decision\":\"approve\"");
  });

  it("Deny button hits the resolve endpoint with decision=deny", async () => {
    let postedBody: string | undefined;
    global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (init?.method === "POST") {
        postedBody = init.body as string;
        return {
          ok: true,
          status: 200,
          json: async () => ({}),
          text: async () => "",
        } as Response;
      }
      return {
        ok: true,
        status: 200,
        json: async () => resp({ pending: [req({ action_id: "a-x" })] }),
        text: async () => "",
      } as Response;
    });
    renderPage();
    fireEvent.click(await screen.findByTestId("deny-button-a-x"));
    await waitFor(() => expect(postedBody).toBeDefined());
    expect(postedBody).toContain("\"decision\":\"deny\"");
  });

  it("renders error state when initial fetch fails", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({}),
      text: async () => "",
    } as Response);
    renderPage();
    expect(
      await screen.findByText(/failed to load approvals/i),
    ).toBeInTheDocument();
  });
});
