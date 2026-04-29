import { createContext, useContext, type ReactNode } from "react";
import { useParams } from "@tanstack/react-router";

/**
 * The id of the Brain currently being viewed.
 *
 * Routes are `/brains/$brainId/...`. The brainId can be read in two
 * ways:
 *
 * 1. **`useParams`** (production path) — components anywhere in the
 *    Router's tree can call this hook directly. Works for sidebar
 *    components like `<BrainSelector>` and `<HatPicker>` that sit
 *    *outside* the page's `<Outlet />` but inside the Router.
 *
 * 2. **`<BrainProvider>` context** (test path) — when a component
 *    is rendered without a parameterized route (e.g., page-level
 *    component tests using `makeTestRouter`), wrap it in
 *    `<BrainProvider>` to seed the brainId. Production code does
 *    NOT need to wrap with the provider; pages just call
 *    `useBrainId()` and it falls through to `useParams`.
 *
 * The hook prefers the explicit context value (so tests can pin
 * brainId regardless of route), then falls back to the URL param.
 * Throws when neither is available — that's a wiring bug.
 */
const BrainContext = createContext<string | null>(null);

export function BrainProvider({
  brainId,
  children,
}: {
  brainId: string;
  children: ReactNode;
}) {
  return <BrainContext.Provider value={brainId}>{children}</BrainContext.Provider>;
}

export function useBrainId(): string {
  // Both hooks always run — Rules of Hooks. We then pick the
  // first non-null source. `strict: false` on useParams is required
  // because sidebar components are outside the Route that owns
  // the `:brainId` segment, and would otherwise warn about
  // resolving from an ambiguous match.
  const ctx = useContext(BrainContext);
  const params = useParams({ strict: false }) as { brainId?: string };
  const id = ctx ?? params.brainId;
  if (!id) {
    throw new Error(
      "useBrainId must be called within a <BrainProvider> or under a `/brains/$brainId/...` route"
    );
  }
  return id;
}

/**
 * Construct the per-brain API endpoint URL. Centralizes the
 * `/api/brains/<id>/<rest>` shape so a future refactor to a
 * different prefix only touches one file.
 */
export function brainApi(brainId: string, rest: string): string {
  const trimmed = rest.startsWith("/") ? rest.slice(1) : rest;
  return `/api/brains/${encodeURIComponent(brainId)}/${trimmed}`;
}
