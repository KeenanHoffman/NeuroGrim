import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/**
 * The currently-selected hat lens. `"default"` is the un-hatted
 * sentinel — the rest of the app collapses it to `null` before
 * passing it to `?hat=` so the backend's resolved_hat() returns
 * `None` (matching the CLI's no-hat behavior).
 */
export type HatId = string;
export const DEFAULT_HAT: HatId = "default";
const STORAGE_KEY = "neurogrim:hat";

interface HatContextValue {
  hat: HatId;
  setHat: (h: HatId) => void;
}

const HatContext = createContext<HatContextValue | null>(null);

export function HatProvider({ children }: { children: ReactNode }) {
  const [hat, setHatState] = useState<HatId>(() => readInitialHat());

  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, hat);
    } catch {
      // localStorage may be disabled (private mode); the in-memory
      // value still applies for the session.
    }
  }, [hat]);

  const setHat = useCallback((h: HatId) => {
    setHatState(h);
  }, []);

  const value = useMemo<HatContextValue>(() => ({ hat, setHat }), [hat, setHat]);

  return <HatContext.Provider value={value}>{children}</HatContext.Provider>;
}

/**
 * Read the currently-selected hat. Throws when used outside a
 * `HatProvider` — that's a wiring bug, not a runtime path the
 * dashboard should keep going on.
 */
export function useHat(): HatContextValue {
  const ctx = useContext(HatContext);
  if (!ctx) {
    throw new Error("useHat must be used within a HatProvider");
  }
  return ctx;
}

/**
 * Convenience: return the hat as the value the backend wants in
 * `?hat=` — `null` for the default lens, the hat id otherwise. Use
 * this in queryFn/querystring construction.
 */
export function hatToQuery(hat: HatId): string | null {
  if (!hat || hat === DEFAULT_HAT) return null;
  return hat;
}

function readInitialHat(): HatId {
  if (typeof window === "undefined") return DEFAULT_HAT;
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored && stored.length > 0) return stored;
  } catch {
    // ignored
  }
  return DEFAULT_HAT;
}
