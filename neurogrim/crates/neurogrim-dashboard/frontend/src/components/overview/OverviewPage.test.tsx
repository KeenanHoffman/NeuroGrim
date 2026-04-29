import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { OverviewPage } from "./OverviewPage";
import { BrainProvider } from "@/lib/useBrain";
import { HatProvider } from "@/lib/useHat";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { DashboardLayoutResponse } from "@bindings/DashboardLayoutResponse";

const overview = (over: Partial<OverviewResponse> = {}): OverviewResponse => ({
  project_label: "Test Brain",
  project_root: "/tmp/test",
  domain_count: 3,
  weighted_count: 1,
  advisory_count: 2,
  score: 78,
  confidence: 90,
  trajectory_class: "stable",
  trajectory_velocity: 0,
  trajectory_samples: 5,
  top_recommendations: [],
  strongest_signals: [
    { name: "test-health", display_name: "Test", effective_score: 85, confidence: 95, weight: 0.4 },
  ],
  federation_peer_count: 0,
  ...over,
});

const layout = (
  widgets: DashboardLayoutResponse["widgets"],
  isDefault = false
): DashboardLayoutResponse => ({
  schema_version: "1",
  brain_id: "test",
  is_default: isDefault,
  widgets,
});

function mockFetch(routeMap: Record<string, unknown>) {
  global.fetch = vi.fn((input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    for (const [pattern, payload] of Object.entries(routeMap)) {
      if (url.includes(pattern)) {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: async () => payload,
        } as Response);
      }
    }
    return Promise.resolve({
      ok: false,
      status: 404,
      json: async () => ({ error: "no route" }),
    } as Response);
  }) as typeof fetch;
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <OverviewPage />
    </BrainProvider>
  );
  return render(
    <QueryClientProvider client={qc}>
      <HatProvider>
        <RouterProvider router={router} />
      </HatProvider>
    </QueryClientProvider>
  );
}

describe("OverviewPage layout dispatch", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the identity widget when the layout includes it", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout([
        {
          id: "i1",
          widget_type: "identity",
          size: "full",
          title: null,
          config: {},
        },
      ]),
    });
    renderPage();
    expect(await screen.findByText("Test Brain")).toBeInTheDocument();
  });

  it("renders the score-gauge widget", async () => {
    mockFetch({
      "/overview": overview({ score: 42 }),
      "/dashboard-layout": layout([
        {
          id: "g",
          widget_type: "score-gauge",
          size: "third",
          title: null,
          config: {},
        },
      ]),
    });
    renderPage();
    // Gauge renders the score number.
    expect(await screen.findByText("42")).toBeInTheDocument();
  });

  it("renders an UnknownWidget placeholder for unrecognized types", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout([
        {
          id: "u",
          widget_type: "future-widget-from-tomorrow",
          size: "full",
          title: null,
          config: {},
        },
      ]),
    });
    renderPage();
    expect(
      await screen.findByText(/Unknown widget type: future-widget-from-tomorrow/i)
    ).toBeInTheDocument();
  });

  it("shows the default-layout banner when is_default is true", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout(
        [
          {
            id: "i1",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
        ],
        true
      ),
    });
    renderPage();
    expect(
      await screen.findByTestId("default-layout-banner")
    ).toBeInTheDocument();
  });

  it("hides the banner when is_default is false (custom layout)", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout(
        [
          {
            id: "i1",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
        ],
        false
      ),
    });
    renderPage();
    await screen.findByText("Test Brain");
    expect(
      screen.queryByTestId("default-layout-banner")
    ).not.toBeInTheDocument();
  });

  it("warns when domain-card is missing the required config.domain", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout([
        {
          id: "bad",
          widget_type: "domain-card",
          size: "third",
          title: null,
          config: {}, // ← missing `domain`
        },
      ]),
    });
    renderPage();
    expect(
      await screen.findByText(/domain-card requires config\.domain/i)
    ).toBeInTheDocument();
  });

  it("default-layout banner shows a Customize button that enters edit mode", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout(
        [
          {
            id: "i",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
        ],
        true
      ),
    });
    renderPage();
    const banner = await screen.findByTestId("default-layout-banner");
    const customize = banner.querySelector(
      "[data-testid='customize-from-banner']"
    ) as HTMLButtonElement;
    expect(customize).toBeTruthy();
    fireEvent.click(customize);
    // Banner is hidden in edit mode; toolbar appears.
    expect(screen.queryByTestId("default-layout-banner")).not.toBeInTheDocument();
    expect(screen.getByTestId("edit-mode-on")).toBeInTheDocument();
  });

  it("entering edit mode shows per-widget controls (move + remove + size)", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout(
        [
          {
            id: "w1",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
          {
            id: "w2",
            widget_type: "score-gauge",
            size: "third",
            title: null,
            config: {},
          },
        ],
        false
      ),
    });
    renderPage();
    fireEvent.click(await screen.findByTestId("enter-edit-mode"));
    // Two sets of controls, one per widget.
    expect(screen.getByTestId("widget-edit-w1")).toBeInTheDocument();
    expect(screen.getByTestId("widget-edit-w2")).toBeInTheDocument();
    expect(screen.getByTestId("remove-w1")).toBeInTheDocument();
    expect(screen.getByTestId("remove-w2")).toBeInTheDocument();
    // Up/down arrow disable states: first widget can't move up,
    // last widget can't move down.
    expect(screen.getByTestId("move-up-w1")).toBeDisabled();
    expect(screen.getByTestId("move-down-w2")).toBeDisabled();
    // The middle/end pair: first widget CAN move down, second CAN move up.
    expect(screen.getByTestId("move-down-w1")).not.toBeDisabled();
    expect(screen.getByTestId("move-up-w2")).not.toBeDisabled();
  });

  it("removing a widget in edit mode drops it from the rendered grid", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout(
        [
          {
            id: "stay",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
          {
            id: "go",
            widget_type: "score-gauge",
            size: "third",
            title: null,
            config: {},
          },
        ],
        false
      ),
    });
    const { container } = renderPage();
    fireEvent.click(await screen.findByTestId("enter-edit-mode"));
    fireEvent.click(screen.getByTestId("remove-go"));
    // After remove, only the "stay" widget remains in the grid.
    const remaining = container.querySelectorAll("[data-widget-id]");
    expect(remaining).toHaveLength(1);
    expect(remaining[0].getAttribute("data-widget-id")).toBe("stay");
  });

  it("renders multiple widgets in layout order", async () => {
    mockFetch({
      "/overview": overview(),
      "/dashboard-layout": layout([
        {
          id: "id",
          widget_type: "identity",
          size: "full",
          title: null,
          config: {},
        },
        {
          id: "g",
          widget_type: "score-gauge",
          size: "third",
          title: null,
          config: {},
        },
      ]),
    });
    const { container } = renderPage();
    await screen.findByText("Test Brain");
    // Two widget wrappers, in the documented order.
    const widgets = container.querySelectorAll("[data-widget-id]");
    expect(widgets).toHaveLength(2);
    expect(widgets[0].getAttribute("data-widget-id")).toBe("id");
    expect(widgets[1].getAttribute("data-widget-id")).toBe("g");
  });
});
