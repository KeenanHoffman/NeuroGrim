import { useEffect } from "react";
import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useNavigate,
} from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import type { BrainsListResponse } from "@bindings/BrainsListResponse";
import { AppShell } from "@/components/layout/AppShell";
import { OverviewPage } from "@/components/overview/OverviewPage";
import { DomainsPage } from "@/components/domains/DomainsPage";
import { DomainDetailPage } from "@/components/domains/DomainDetailPage";
import { FederationPage } from "@/components/federation/FederationPage";
import { PublishGatesPage } from "@/components/publish-gates/PublishGatesPage";
import { SkillsPage } from "@/components/skills/SkillsPage";
import { BrainProvider } from "@/lib/useBrain";

/**
 * Path 2: typed multi-Brain route tree.
 *
 * Top-level layout: AppShell wraps every page. The index route at
 * `/` redirects to `/brains/<self_id>/`, where `<self_id>` is
 * fetched from `/api/brains` on first load. All pages live under
 * `/brains/$brainId/...` and resolve `brainId` via the URL params,
 * making every fetch + navigation brain-scoped.
 */

const rootRoute = createRootRoute({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
  notFoundComponent: NotFoundPage,
});

/** Index route — redirects to /brains/<self_id>/ as soon as we
 * know the host's id from /api/brains. Renders a minimal "Locating
 * default Brain…" placeholder during the in-flight fetch. */
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: IndexRedirect,
});

/** Layout route for `/brains/$brainId/...` — provides the brainId
 * to descendants via context and renders an Outlet for the
 * concrete page. */
const brainsLayoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/brains/$brainId",
  component: BrainsLayout,
});

const brainOverviewRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "/",
  component: OverviewPage,
});

const brainDomainsRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "domains",
  component: DomainsPage,
});

const brainDomainDetailRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "domains/$name",
  component: () => {
    const { name } = brainDomainDetailRoute.useParams();
    return <DomainDetailPage name={name} />;
  },
});

const brainFederationRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "federation",
  component: FederationPage,
});

const brainSkillsRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "skills",
  component: SkillsPage,
});

/** v4.0 S12-G-6 — manual-gate UI surface. Read-only view of the
 * brain's `publish-gates.yaml` joined with the
 * `publish-gate-ledger.jsonl` so operators can see "what's pending"
 * at a glance. */
const brainPublishGatesRoute = createRoute({
  getParentRoute: () => brainsLayoutRoute,
  path: "publish-gates",
  component: PublishGatesPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  brainsLayoutRoute.addChildren([
    brainOverviewRoute,
    brainDomainsRoute,
    brainDomainDetailRoute,
    brainFederationRoute,
    brainSkillsRoute,
    brainPublishGatesRoute,
  ]),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

/** Pulls the brainId path param, validates it, and provides it
 * via context to every page rendered under `/brains/$brainId`. */
function BrainsLayout() {
  const { brainId } = brainsLayoutRoute.useParams();
  return (
    <BrainProvider brainId={brainId}>
      <Outlet />
    </BrainProvider>
  );
}

/** Redirects `/` to `/brains/<self_id>/` once the brain list loads.
 * Renders a small placeholder during the in-flight fetch so the
 * page is never blank. */
function IndexRedirect() {
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery({
    queryKey: ["brains"],
    queryFn: async () => {
      const res = await fetch("/api/brains");
      if (!res.ok) throw new Error(`/api/brains returned ${res.status}`);
      return (await res.json()) as BrainsListResponse;
    },
    staleTime: 5 * 60_000,
  });
  useEffect(() => {
    if (data?.self_id) {
      navigate({
        to: "/brains/$brainId",
        params: { brainId: data.self_id },
        replace: true,
      });
    }
  }, [data, navigate]);
  if (isLoading) {
    return (
      <div className="text-sm text-muted-foreground py-16 text-center">
        Locating default Brain…
      </div>
    );
  }
  if (error || !data) {
    return (
      <div className="text-center py-16">
        <h2 className="text-2xl font-semibold text-destructive">
          Could not load Brain list
        </h2>
        <p className="mt-2 text-sm text-muted-foreground">
          {(error as Error)?.message ?? "Unknown error"}
        </p>
      </div>
    );
  }
  return null;
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
