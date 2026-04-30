import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
  type AnyRouter,
} from "@tanstack/react-router";
import { CustomPageRenderer } from "./CustomPageRenderer";
import { BrainProvider } from "@/lib/useBrain";
import { HatProvider } from "@/lib/useHat";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { DashboardPagesConfig } from "@bindings/DashboardPagesConfig";
import type { WidgetSpec } from "@bindings/WidgetSpec";

/**
 * Custom router so `useParams({ strict: false })` resolves the
 * pageName param. The shared `makeTestRouter` helper doesn't ship
 * a `/brains/$brainId/p/$pageName` matcher.
 */
function makeCustomPageRouter(initialPath: string): AnyRouter {
  const rootRoute = createRootRoute({ component: () => <Outlet /> });
  const customPageRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId/p/$pageName",
    component: () => (
      <BrainProvider brainId="test-brain">
        <CustomPageRenderer />
      </BrainProvider>
    ),
  });
  return createRouter({
    routeTree: rootRoute.addChildren([customPageRoute]),
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  });
}

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
  strongest_signals: [],
  federation_peer_count: 0,
  ...over,
});

const pages = (
  pageMap: Record<string, WidgetSpec[]>,
): DashboardPagesConfig => ({
  schema_version: "2",
  brain_id: "test-brain",
  pages: pageMap,
  page_order: ["overview", ...Object.keys(pageMap)],
});

/**
 * Mock fetch with capture of every PUT (mutation) seen. Tests
 * assert that a save fired by checking `captured.puts` rather
 * than a "last method" field — the post-save invalidation triggers
 * follow-up GETs that would overwrite a single-slot field.
 */
function mockFetch(
  routeMap: Record<string, unknown>,
): { puts: Array<{ url: string; body: unknown }> } {
  const captured: { puts: Array<{ url: string; body: unknown }> } = {
    puts: [],
  };
  global.fetch = vi
    .fn()
    .mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const method = init?.method ?? "GET";
      if (method === "PUT") {
        let parsed: unknown = init?.body;
        if (typeof init?.body === "string") {
          try {
            parsed = JSON.parse(init.body);
          } catch {
            // fall through with the raw body
          }
        }
        captured.puts.push({ url, body: parsed });
        return {
          ok: true,
          status: 200,
          json: async () => ({ ok: true, name: "stub" }),
          text: async () => "",
        } as Response;
      }
      for (const [pattern, payload] of Object.entries(routeMap)) {
        if (url.includes(pattern)) {
          return {
            ok: true,
            status: 200,
            json: async () => payload,
            text: async () => "",
          } as Response;
        }
      }
      return {
        ok: false,
        status: 404,
        json: async () => ({ error: "no route" }),
        text: async () => "",
      } as Response;
    }) as typeof fetch;
  return captured;
}

function renderPage(pageName = "my-custom-page") {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeCustomPageRouter(`/brains/test-brain/p/${pageName}`);
  return render(
    <QueryClientProvider client={qc}>
      <HatProvider>
        <RouterProvider router={router} />
      </HatProvider>
    </QueryClientProvider>,
  );
}

describe("CustomPageRenderer (S15-C-6 v2)", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the empty-state card when the page exists but has no widgets", async () => {
    mockFetch({
      "/dashboard-pages": pages({ "my-custom-page": [] }),
      "/overview": overview(),
    });
    renderPage();
    expect(
      await screen.findByTestId("custom-page-empty"),
    ).toBeInTheDocument();
    // The page header surfaces the page name.
    expect(screen.getByText("my-custom-page")).toBeInTheDocument();
  });

  it("renders the page-not-found card when the page is undeclared", async () => {
    mockFetch({
      "/dashboard-pages": pages({ "other-page": [] }),
      "/overview": overview(),
    });
    renderPage("missing-page");
    expect(
      await screen.findByTestId("custom-page-not-found"),
    ).toBeInTheDocument();
  });

  it("renders declared widgets through WidgetDispatch", async () => {
    mockFetch({
      "/dashboard-pages": pages({
        "my-custom-page": [
          {
            id: "ident-1",
            widget_type: "identity",
            size: "full",
            title: null,
            config: {},
          },
        ],
      }),
      "/overview": overview(),
    });
    renderPage();
    // identity widget surfaces the project_label from the overview.
    expect(await screen.findByText("Test Brain")).toBeInTheDocument();
  });

  it("Customize button enters edit mode and reveals the toolbar", async () => {
    mockFetch({
      "/dashboard-pages": pages({ "my-custom-page": [] }),
      "/overview": overview(),
    });
    renderPage();
    // Wait for the page to settle (empty card visible) before
    // clicking — the toolbar is part of the rendered output once
    // the layout query finishes.
    await screen.findByTestId("custom-page-empty");
    fireEvent.click(screen.getByTestId("enter-edit-mode"));
    expect(
      await screen.findByTestId("edit-mode-on"),
    ).toBeInTheDocument();
    // Reset button is hidden on custom pages (no posture-aware
    // default to fall back to).
    expect(screen.queryByTestId("reset-layout")).toBeNull();
  });

  it("adding a widget then Save PUTs to the per-page endpoint", async () => {
    const captured = mockFetch({
      "/dashboard-pages": pages({ "my-custom-page": [] }),
      "/overview": overview(),
    });
    renderPage();
    await screen.findByTestId("custom-page-empty");
    fireEvent.click(screen.getByTestId("enter-edit-mode"));
    await screen.findByTestId("edit-mode-on");
    // Add a widget. Default picker is "domain-card" — server
    // doesn't validate config.domain at PUT time, so this is a
    // valid test stand-in even though the operator would
    // normally pick a domain before saving.
    fireEvent.click(screen.getByTestId("add-widget"));
    fireEvent.click(screen.getByTestId("save-layout"));
    // Wait for the PUT to land — captured.puts is append-only so
    // it survives the post-save GET refetch from the invalidation.
    await waitFor(() => expect(captured.puts.length).toBe(1));
    const put = captured.puts[0];
    expect(put.url).toContain(
      "/dashboard-pages/my-custom-page/layout",
    );
    const body = put.body as { widgets: WidgetSpec[] };
    expect(body.widgets).toHaveLength(1);
    expect(body.widgets[0].widget_type).toBe("domain-card");
  });

  it("removes a widget when ✕ is clicked in edit mode", async () => {
    mockFetch({
      "/dashboard-pages": pages({
        "my-custom-page": [
          {
            id: "w1",
            widget_type: "score-gauge",
            size: "third",
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
      }),
      "/overview": overview(),
    });
    renderPage();
    // Wait for the widgets to render.
    await screen.findByTestId("custom-page-widgets");
    fireEvent.click(screen.getByTestId("enter-edit-mode"));
    await screen.findByTestId("edit-mode-on");
    // Both widgets have edit controls at this point.
    expect(screen.getByTestId("widget-edit-w1")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("remove-w1"));
    // After removal, w1 is gone but w2 remains.
    expect(screen.queryByTestId("widget-edit-w1")).toBeNull();
    expect(screen.getByTestId("widget-edit-w2")).toBeInTheDocument();
  });
});
