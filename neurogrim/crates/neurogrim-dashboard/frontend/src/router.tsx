import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useNavigate,
} from "@tanstack/react-router";
import { AppShell } from "@/components/layout/AppShell";
import { OverviewPage } from "@/components/overview/OverviewPage";
import { DomainsPage } from "@/components/domains/DomainsPage";
import { DomainDetailPage } from "@/components/domains/DomainDetailPage";
import { FederationPage } from "@/components/federation/FederationPage";
import { SkillsPage } from "@/components/skills/SkillsPage";

/**
 * Phase 1.5: typed route tree built with TanStack Router. Replaces
 * the in-house `useRoute` hook. The motivation isn't current pain —
 * the in-house hook was a clean 30 LOC — it's that any future page
 * we add (settings, ledger viewer, governance proposals, calibration
 * tools) will benefit from typed `<Link to="...">` links and the
 * route-level loader hooks the v3.5+ live-update plan needs.
 */

const rootRoute = createRootRoute({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
  notFoundComponent: NotFoundPage,
});

const overviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: OverviewPage,
});

const domainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains",
  component: DomainsPage,
});

const domainDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains/$name",
  component: () => {
    // Bridge from typed route params → existing prop-based component.
    // Keeping the prop-based shape lets vitest tests render
    // DomainDetailPage directly without a router wrapper.
    const params = domainDetailRoute.useParams();
    return <DomainDetailPage name={params.name} />;
  },
});

const federationRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/federation",
  component: FederationPage,
});

const skillsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skills",
  component: SkillsPage,
});

const routeTree = rootRoute.addChildren([
  overviewRoute,
  domainsRoute,
  domainDetailRoute,
  federationRoute,
  skillsRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function NotFoundPage() {
  const navigate = useNavigate();
  return (
    <div className="text-center py-16">
      <h2 className="text-2xl font-semibold">Page not found</h2>
      <p className="mt-2 text-sm text-muted-foreground">
        That route doesn't exist in the v3.4 dashboard.
      </p>
      <button
        onClick={() => navigate({ to: "/" })}
        className="mt-4 text-sm text-primary underline-offset-4 hover:underline"
      >
        Back to Overview
      </button>
    </div>
  );
}
