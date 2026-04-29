import { useState, type ReactNode } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import {
  Brain,
  LayoutDashboard,
  Layers,
  Network,
  BookOpen,
  Moon,
  Sun,
  Menu,
  X,
} from "lucide-react";
import { useTheme } from "@/lib/useTheme";

interface NavItem {
  to: string;
  label: string;
  icon: ReactNode;
  /** Subroutes that should also light up this nav item (e.g.
   *  /domains lights up for /domains/foo too). */
  matchPrefix?: string;
}

const NAV: NavItem[] = [
  { to: "/", label: "Overview", icon: <LayoutDashboard className="h-4 w-4" /> },
  {
    to: "/domains",
    label: "Domains",
    icon: <Layers className="h-4 w-4" />,
    matchPrefix: "/domains",
  },
  { to: "/federation", label: "Federation", icon: <Network className="h-4 w-4" /> },
  { to: "/skills", label: "Skills", icon: <BookOpen className="h-4 w-4" /> },
];

/**
 * App shell — sidebar nav on the left (>= md), hamburger overlay
 * on small screens. Wraps `<Outlet />` (page content) on the right.
 *
 * Used as the root-route component in `router.tsx`. Renders once
 * per app load; child routes mount inside the `<main>` slot.
 */
export function AppShell({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  const [mobileOpen, setMobileOpen] = useState(false);

  const isActive = (item: NavItem): boolean => {
    if (item.matchPrefix) {
      if (pathname === item.matchPrefix) return true;
      return pathname.startsWith(`${item.matchPrefix}/`);
    }
    return pathname === item.to;
  };

  return (
    <div className="min-h-screen bg-background text-foreground flex">
      {/* Desktop sidebar */}
      <aside className="hidden md:flex md:flex-col md:w-60 md:border-r md:border-border md:sticky md:top-0 md:h-screen">
        <SidebarContent
          isActive={isActive}
          onItemClick={() => setMobileOpen(false)}
        />
      </aside>

      {/* Mobile overlay sidebar */}
      {mobileOpen && (
        <div
          className="md:hidden fixed inset-0 z-40 bg-background/80 backdrop-blur-sm"
          onClick={() => setMobileOpen(false)}
          data-testid="mobile-sidebar-overlay"
        >
          <aside
            className="absolute left-0 top-0 bottom-0 w-60 bg-background border-r border-border flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <SidebarContent
              isActive={isActive}
              onItemClick={() => setMobileOpen(false)}
            />
          </aside>
        </div>
      )}

      <main className="flex-1 min-w-0 flex flex-col">
        {/* Mobile top bar with hamburger */}
        <div className="md:hidden border-b border-border px-4 py-3 flex items-center gap-3">
          <button
            onClick={() => setMobileOpen(true)}
            aria-label="Open menu"
            data-testid="mobile-menu-button"
            className="text-muted-foreground hover:text-foreground"
          >
            <Menu className="h-5 w-5" />
          </button>
          <span className="font-semibold">NeuroGrim</span>
        </div>
        <div className="flex-1 px-6 py-8 max-w-7xl w-full mx-auto">
          {children}
        </div>
      </main>
    </div>
  );
}

function SidebarContent({
  isActive,
  onItemClick,
}: {
  isActive: (n: NavItem) => boolean;
  onItemClick: () => void;
}) {
  const { theme, toggleTheme } = useTheme();

  return (
    <>
      <div className="px-4 py-5 flex items-center justify-between">
        <Link
          to="/"
          className="flex items-center gap-2 text-base font-semibold hover:text-foreground/80 transition-colors"
          onClick={onItemClick}
          data-testid="sidebar-brand"
        >
          <Brain className="h-5 w-5" />
          NeuroGrim
        </Link>
        <button
          onClick={onItemClick}
          aria-label="Close menu"
          className="md:hidden text-muted-foreground hover:text-foreground"
          data-testid="sidebar-close"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <nav className="flex-1 px-2 space-y-1" aria-label="Primary">
        {NAV.map((item) => (
          <Link
            key={item.to}
            to={item.to}
            onClick={onItemClick}
            data-testid={`nav-${item.label.toLowerCase()}`}
            className={
              isActive(item)
                ? "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium bg-secondary text-foreground"
                : "flex items-center gap-3 rounded-md px-3 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
            }
          >
            {item.icon}
            {item.label}
          </Link>
        ))}
      </nav>

      <div className="border-t border-border px-3 py-3 flex items-center justify-between">
        <span className="text-xs text-muted-foreground font-mono">v3.4 Phase 1.5</span>
        <button
          onClick={toggleTheme}
          aria-label={
            theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
          }
          data-testid="theme-toggle"
          className="text-muted-foreground hover:text-foreground transition-colors"
        >
          {theme === "dark" ? (
            <Sun className="h-4 w-4" />
          ) : (
            <Moon className="h-4 w-4" />
          )}
        </button>
      </div>
    </>
  );
}
