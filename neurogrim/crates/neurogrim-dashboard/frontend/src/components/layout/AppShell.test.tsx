import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AppShell } from "./AppShell";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
} from "@tanstack/react-router";

function renderShell(initialPath: string = "/") {
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
  const domainsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/domains",
    component: () => <div data-testid="page-domains">Domains</div>,
  });
  const domainDetailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/domains/$name",
    component: () => <div data-testid="page-domain-detail">Detail</div>,
  });
  const federationRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/federation",
    component: () => <div data-testid="page-federation">Federation</div>,
  });
  const skillsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/skills",
    component: () => <div data-testid="page-skills">Skills</div>,
  });
  const tree = rootRoute.addChildren([
    indexRoute,
    domainsRoute,
    domainDetailRoute,
    federationRoute,
    skillsRoute,
  ]);
  const router = createRouter({
    routeTree: tree,
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  });
  return {
    router,
    ...render(<RouterProvider router={router} />),
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
    renderShell("/");
    expect(await screen.findByTestId("nav-overview")).toBeInTheDocument();
    expect(screen.getByTestId("nav-domains")).toBeInTheDocument();
    expect(screen.getByTestId("nav-federation")).toBeInTheDocument();
    expect(screen.getByTestId("nav-skills")).toBeInTheDocument();
  });

  it("highlights the current section in the sidebar", async () => {
    const { router } = renderShell("/domains");
    await screen.findByTestId("nav-domains");
    // bg-secondary class is part of the active styling.
    expect(screen.getByTestId("nav-domains").className).toMatch(/bg-secondary/);
    expect(screen.getByTestId("nav-overview").className).not.toMatch(/bg-secondary/);
    expect(router.state.location.pathname).toBe("/domains");
  });

  it("treats /domains/<name> as 'Domains' active", async () => {
    renderShell("/domains/test-health");
    await screen.findByTestId("nav-domains");
    expect(screen.getByTestId("nav-domains").className).toMatch(/bg-secondary/);
  });

  it("clicking a nav link navigates to that route", async () => {
    const { router } = renderShell("/");
    fireEvent.click(await screen.findByTestId("nav-skills"));
    await screen.findByTestId("page-skills");
    expect(router.state.location.pathname).toBe("/skills");
  });

  it("theme toggle flips the html `dark` class", async () => {
    document.documentElement.classList.add("dark");
    renderShell("/");
    const toggle = await screen.findByTestId("theme-toggle");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    fireEvent.click(toggle);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(window.localStorage.getItem("neurogrim:theme")).toBe("light");
    fireEvent.click(toggle);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("mobile menu button opens and closes the overlay sidebar", async () => {
    renderShell("/");
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
    renderShell("/");
    fireEvent.click(await screen.findByTestId("mobile-menu-button"));
    const overlay = screen.getByTestId("mobile-sidebar-overlay");
    fireEvent.click(overlay);
    expect(screen.queryByTestId("mobile-sidebar-overlay")).not.toBeInTheDocument();
  });
});
