import { render, screen, within, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { DomainsPage } from "./DomainsPage";
import type { DomainListItemDto } from "@bindings/DomainListItemDto";
import type { DomainsListResponse } from "@bindings/DomainsListResponse";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import { HatProvider } from "@/lib/useHat";
import { BrainProvider } from "@/lib/useBrain";

const dom = (overrides: Partial<DomainListItemDto> = {}): DomainListItemDto => ({
  name: "test-health",
  display_name: "Test Health",
  weight: 0.35,
  raw_score: 80,
  effective_score: 78,
  confidence: 95,
  trajectory_class: "stable",
  trajectory_velocity: 0,
  trajectory_samples: 5,
  last_updated: "2026-04-25T10:00:00Z",
  ...overrides,
});

function mockFetch(payload: DomainsListResponse) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: true,
    json: async () => payload,
  } as Response);
}

function renderWithQuery() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <DomainsPage />
    </BrainProvider>
  );
  return {
    router,
    ...render(
      <QueryClientProvider client={qc}>
        <HatProvider>
          <RouterProvider router={router} />
        </HatProvider>
      </QueryClientProvider>
    ),
  };
}

describe("DomainsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/");
  });

  it("renders one row per domain after data loads", async () => {
    mockFetch({
      domains: [
        dom({ name: "a", display_name: "Alpha" }),
        dom({ name: "b", display_name: "Beta" }),
      ],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Beta")).toBeInTheDocument();
  });

  it("shows the 'advisory' badge for weight-0 rows", async () => {
    mockFetch({
      domains: [dom({ name: "a", display_name: "Alpha", weight: 0 })],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();
    const row = screen.getByTestId("row-a");
    expect(within(row).getByText("advisory")).toBeInTheDocument();
  });

  it("shows the numeric weight for weighted rows", async () => {
    mockFetch({
      domains: [dom({ name: "a", display_name: "Alpha", weight: 0.35 })],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();
    const row = screen.getByTestId("row-a");
    expect(within(row).getByText("0.35")).toBeInTheDocument();
  });

  it("clicking a row pushes /domains/:name", async () => {
    mockFetch({
      domains: [dom({ name: "test-health", display_name: "Test Health" })],
    });
    const { router } = renderWithQuery();
    const row = await screen.findByTestId("row-test-health");
    fireEvent.click(row);
    // Wait for the router to settle on the new location. The
    // navigation target now includes the brain-id prefix.
    await screen.findByTestId("route-/brains/test-brain/domains/test-health");
    expect(router.state.location.pathname).toBe(
      "/brains/test-brain/domains/test-health"
    );
  });

  it("colors high effective scores green, mid amber, low red", async () => {
    mockFetch({
      domains: [
        dom({ name: "hi", display_name: "Hi", effective_score: 90 }),
        dom({ name: "mid", display_name: "Mid", effective_score: 60 }),
        dom({ name: "lo", display_name: "Lo", effective_score: 30 }),
      ],
    });
    renderWithQuery();
    expect(await screen.findByText("Hi")).toBeInTheDocument();
    expect(within(screen.getByTestId("row-hi")).getByText("90").className).toMatch(/emerald/);
    expect(within(screen.getByTestId("row-mid")).getByText("60").className).toMatch(/amber/);
    expect(within(screen.getByTestId("row-lo")).getByText("30").className).toMatch(/red/);
  });

  it("clicking a column header sorts by that column", async () => {
    mockFetch({
      domains: [
        dom({ name: "z", display_name: "Zebra", effective_score: 30 }),
        dom({ name: "a", display_name: "Alpha", effective_score: 90 }),
      ],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();

    // Default sort: name asc — Alpha before Zebra.
    const rows = () =>
      screen.getAllByRole("row").filter((r) => r.getAttribute("data-testid")?.startsWith("row-"));
    expect(rows()[0]).toHaveAttribute("data-testid", "row-a");

    // Click "Effective" — re-sorts ascending by effective_score (30, 90).
    fireEvent.click(screen.getByText("Effective"));
    expect(rows()[0]).toHaveAttribute("data-testid", "row-z"); // 30 first

    // Click again — flip to descending (90 first).
    fireEvent.click(screen.getByText("Effective"));
    expect(rows()[0]).toHaveAttribute("data-testid", "row-a"); // 90 first
  });

  it("shows a friendly 'just now' / '1 day ago' relative for recent timestamps", async () => {
    const now = new Date();
    const yesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString();
    mockFetch({
      domains: [dom({ name: "a", display_name: "Alpha", last_updated: yesterday })],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText(/1 day ago/)).toBeInTheDocument();
  });

  it("renders '—' when last_updated is null", async () => {
    mockFetch({
      domains: [dom({ name: "a", display_name: "Alpha", last_updated: null })],
    });
    renderWithQuery();
    expect(await screen.findByText("Alpha")).toBeInTheDocument();
    expect(within(screen.getByTestId("row-a")).getByText("—")).toBeInTheDocument();
  });
});
