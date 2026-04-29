import { useEffect, useState } from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "neurogrim:theme";

/**
 * Tiny theme state hook. Persists to localStorage; toggles the
 * `dark` class on `<html>` (Tailwind's `darkMode: ["class"]`
 * strategy).
 *
 * Default is dark — matches the operator-tuned palette the rest
 * of the dashboard's components were authored against. Light mode
 * is best-effort; some components (the topology SVG, the
 * sparkline) lean on dark contrast and the user can flip back.
 */
export function useTheme(): {
  theme: Theme;
  setTheme: (t: Theme) => void;
  toggleTheme: () => void;
} {
  const [theme, setThemeState] = useState<Theme>(() => readInitialTheme());

  useEffect(() => {
    applyTheme(theme);
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // localStorage may be disabled (private mode, embedded browsers);
      // theme still applies for the session — don't crash the app.
    }
  }, [theme]);

  return {
    theme,
    setTheme: setThemeState,
    toggleTheme: () => setThemeState((t) => (t === "dark" ? "light" : "dark")),
  };
}

function readInitialTheme(): Theme {
  if (typeof window === "undefined") return "dark";
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === "dark" || stored === "light") return stored;
  } catch {
    // Same as above — proceed with default.
  }
  // Fall back to the user's OS preference. We only consult media-
  // query on first load; once the user has made a choice it is
  // honored on subsequent visits.
  if (
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: light)").matches
  ) {
    return "light";
  }
  return "dark";
}

function applyTheme(theme: Theme): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (theme === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}
