import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AppShell } from "./AppShell";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import { HatProvider } from "@/lib/useHat";
import { ToastProvider } from "@/components/ui/toast";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
} from "@tanstack/react-router";

function renderShell(initialPath: string = "/brains/test-brain") {
  // The AppShell uses Tanstack `<Link>` + `useLocation`, so we need
  // a real router around it (not just the makeTestRouter helper —
  // we want the AppShell to render as the root, not as a child page).
  const rootRoute = createRootRoute({
    component: () => (
      <AppShell>
        <Outlet />
      </AppShell>
    ),
  });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    component: () => <div data-testid="page-root">Index</div>,
  });
  // Path 2: routes nested under /brains/$brainId. AppShell reads
  // brainId via useParams and toggles nav visibility/active state on
  // it. The test routes mirror the production structure.
  const brainOverviewRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId",
    component: () => <div data-testid="page-overview">Overview</div>,
  });
  const brainDomainsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId/domains",
    component: () => <div data-testid="page-domains">Domains</div>,
  });
  const brainDomainDetailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId/domains/$name",
    component: () => <div data-testid="page-domain-detail">Detail</div>,
  });
  const brainFederationRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId/federation",
    component: () => <div data-testid="page-federation">Federation</div>,
  });
  const brainSkillsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/brains/$brainId/skills",
    component: () => <div data-testid="page-skills">Skills</div>,
  });
  const tree = rootRoute.addChildren([
    indexRoute,
    brainOverviewRoute,
    brainDomainsRoute,
    brainDomainDetailRoute,
    brainFederationRoute,
    brainSkillsRoute,
  ]);
  const router = createRouter({
    routeTree: tree,
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  });
  // AppShell now uses TanStack Query (via useDashboardEvents); a
  // provider is required even though no actual queries fire in this
  // test.
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return {
    router,
    ...render(
      <QueryClientProvider client={qc}>
        <ToastProvider>
          <HatProvider>
            <RouterProvider router={router} />
          </HatProvider>
        </ToastProvider>
      </QueryClientProvider>
    ),
  };
}

describe("AppShell", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  afterEach(() => {
    document.documentElement.classList.remove("dark");
  });

  // makeTestRouter is exported from the test helper and re-used by
  // page tests; smoke-check it here so any future shape change is
  // surfaced as a single failure rather than dozens.
  it("re-exports the test router helper", () => {
    expect(typeof makeTestRouter).toBe("function");
  });

  it("renders all four primary nav links", async () => {
    renderShell();
    expect(await screen.findByTestId("nav-overview")).toBeInTheDocument();
    expect(screen.getByTestId("nav-domains")).toBeInTheDocument();
    expect(screen.getByTestId("nav-federation")).toBeInTheDocument();
    expect(screen.getByTestId("nav-skills")).toBeInTheDocument();
  });

  it("highlights the current section in the sidebar", async () => {
    const { router } = renderShell("/brains/test-brain/domains");
    await screen.findByTestId("nav-domains");
    // bg-secondary class is part of the active styling.
    expect(screen.getByTestId("nav-domains").className).toMatch(/bg-secondary/);
    expect(screen.getByTestId("nav-overview").className).not.toMatch(/bg-secondary/);
    expect(router.state.location.pathname).toBe("/brains/test-brain/domains");
  });

  it("treats /domains/<name> as 'Domains' active", async () => {
    renderShell("/brains/test-brain/domains/test-health");
    await screen.findByTestId("nav-domains");
    expect(screen.getByTestId("nav-domains").className).toMatch(/bg-secondary/);
  });

  it("clicking a nav link navigates to that route", async () => {
    const { router } = renderShell();
    fireEvent.click(await screen.findByTestId("nav-skills"));
    await screen.findByTestId("page-skills");
    expect(router.state.location.pathname).toBe("/brains/test-brain/skills");
  });

  it("theme toggle flips the html `dark` class", async () => {
    document.documentElement.classList.add("dark");
    renderShell();
    const toggle = await screen.findByTestId("theme-toggle");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    fireEvent.click(toggle);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(window.localStorage.getItem("neurogrim:theme")).toBe("light");
    fireEvent.click(toggle);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("mobile menu button opens and closes the overlay sidebar", async () => {
    renderShell();
    const menuButton = await screen.findByTestId("mobile-menu-button");
    expect(screen.queryByTestId("mobile-sidebar-overlay")).not.toBeInTheDocument();
    fireEvent.click(menuButton);
    const overlay = screen.getByTestId("mobile-sidebar-overlay");
    // Both the desktop and mobile sidebars render the SidebarContent
    // (desktop is hidden at runtime via `hidden md:flex` but jsdom
    // ignores media queries, so React-tree-wise both exist). Scope
    // the close-button query to the overlay.
    const closeBtn = within(overlay).getByTestId("sidebar-close");
    fireEvent.click(closeBtn);
    expect(screen.queryByTestId("mobile-sidebar-overlay")).not.toBeInTheDocument();
  });

  it("clicking the overlay backdrop closes the sidebar", async () => {
    renderShell();
    fireEvent.click(await screen.findByTestId("mobile-menu-button"));
    const overlay = screen.getByTestId("mobile-sidebar-overlay");
    fireEvent.click(overlay);
    expect(screen.queryByTestId("mobile-sidebar-overlay")).not.toBeInTheDocument();
  });
});

// ── Pure dispatcher: SSE event → toast call ─────────────────────────────

import { vi } from "vitest";
import { dispatchToastForEvent } from "./AppShell";
import type { DashboardEvent } from "@/lib/useDashboardEvents";

describe("dispatchToastForEvent (toast trigger policy)", () => {
  it("service_failed dispatches an error toast with peer + reason", () => {
    const addToast = vi.fn();
    const event: DashboardEvent = {
      kind: "service_failed",
      peer_name: "alpha",
      reason: "port-conflict: port 8421 already bound",
    };
    dispatchToastForEvent(event, addToast);
    expect(addToast).toHaveBeenCalledTimes(1);
    expect(addToast).toHaveBeenCalledWith(
      "error",
      'Peer "alpha" failed',
      "port-conflict: port 8421 already bound",
    );
  });

  it("service_started does NOT toast (operator-caused / visible on Federation)", () => {
    const addToast = vi.fn();
    const event: DashboardEvent = {
      kind: "service_started",
      peer_name: "alpha",
      pid: 1234,
      port: 8421,
    };
    dispatchToastForEvent(event, addToast);
    expect(addToast).not.toHaveBeenCalled();
  });

  it("service_stopped does NOT toast (operator-caused)", () => {
    const addToast = vi.fn();
    const event: DashboardEvent = {
      kind: "service_stopped",
      peer_name: "alpha",
      pid: 1234,
    };
    dispatchToastForEvent(event, addToast);
    expect(addToast).not.toHaveBeenCalled();
  });

  it("registry_changed does NOT toast (operator caused, would be noise)", () => {
    const addToast = vi.fn();
    dispatchToastForEvent({ kind: "registry_changed" }, addToast);
    expect(addToast).not.toHaveBeenCalled();
  });

  it("score_changed does NOT toast (too high frequency)", () => {
    const addToast = vi.fn();
    dispatchToastForEvent(
      { kind: "score_changed", domain: "test-health" },
      addToast,
    );
    expect(addToast).not.toHaveBeenCalled();
  });

  it("notification_published does NOT toast (v2 work)", () => {
    const addToast = vi.fn();
    dispatchToastForEvent({ kind: "notification_published" }, addToast);
    expect(addToast).not.toHaveBeenCalled();
  });

  it("approval_resolved does NOT toast (operator just resolved)", () => {
    const addToast = vi.fn();
    dispatchToastForEvent({ kind: "approval_resolved" }, addToast);
    expect(addToast).not.toHaveBeenCalled();
  });
});
