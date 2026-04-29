import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
  type AnyRouter,
} from "@tanstack/react-router";
import type { ReactElement } from "react";

/**
 * Wrap a component under test in a tiny TanStack Router instance so
 * the `useNavigate` / `Link` hooks resolve.
 *
 * Why this exists: the app's full route tree pulls in every page
 * (and each page's queries / lazy resources). For component-level
 * tests we want a minimal router that only knows about a `/` index
 * mounting the component under test, plus a few stub paths so calls
 * like `navigate({ to: "/domains" })` don't blow up the router with
 * "no such route".
 *
 * Usage:
 * ```tsx
 * const router = makeTestRouter(<DomainsPage />);
 * render(<RouterProvider router={router} />);
 * ```
 */
export function makeTestRouter(
  ui: ReactElement,
  initialPath: string = "/"
): AnyRouter {
  const rootRoute = createRootRoute({
    component: () => <Outlet />,
  });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    component: () => ui,
  });
  // Stub routes the component under test might navigate to. These
  // just render a marker the test can assert on (or that React
  // renders silently after navigation).
  const domainsRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/domains",
    component: () => <div data-testid="route-/domains" />,
  });
  const domainDetailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/domains/$name",
    component: () => {
      const params = domainDetailRoute.useParams();
      return <div data-testid={`route-/domains/${params.name}`} />;
    },
  });

  const routeTree = rootRoute.addChildren([
    indexRoute,
    domainsRoute,
    domainDetailRoute,
  ]);

  return createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  });
}

export { RouterProvider };
