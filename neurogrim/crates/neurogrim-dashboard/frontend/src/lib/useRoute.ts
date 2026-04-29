import { useEffect, useState, useCallback } from "react";

/**
 * Minimal in-house URL routing — `window.location.pathname` + the
 * History API + `popstate` listener. Phase 1.2 needs to navigate
 * between Overview, Domains, and Domain detail pages without a full
 * router; Phase 1.5 swaps this out for `@tanstack/react-router`.
 *
 * Drop-in replacement plan: rename `navigate` callsites to
 * `router.navigate`, replace `pathname` consumers with TanStack's
 * route matcher, delete this file. ~15 minutes of Phase 1.5 work.
 *
 * Why not just use `useState`? We want browser back/forward to
 * work naturally — popstate is the difference between "URL is the
 * source of truth" and "useState is the source of truth, URL is
 * decorative."
 */
export function useRoute() {
  const [pathname, setPathname] = useState<string>(
    () => window.location.pathname
  );

  useEffect(() => {
    const onPopState = () => setPathname(window.location.pathname);
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  const navigate = useCallback((to: string) => {
    if (to === window.location.pathname) return;
    window.history.pushState({}, "", to);
    setPathname(to);
  }, []);

  return { pathname, navigate };
}

/**
 * Shape-tested URL parsers for v3.4's known routes. Co-located so
 * the route matrix is in one place; Phase 1.5's TanStack migration
 * replaces these with the typed router definitions.
 */
export function matchDomainDetail(pathname: string): { name: string } | null {
  const m = pathname.match(/^\/domains\/([^/]+)$/);
  if (!m) return null;
  return { name: decodeURIComponent(m[1]) };
}
