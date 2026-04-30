import { useState, type ReactNode } from "react";
import { Link, useLocation, useParams } from "@tanstack/react-router";
import {
  Brain,
  KeyRound,
  LayoutDashboard,
  Layers,
  Network,
  BookOpen,
  GitMerge,
  ScrollText,
  Server,
  Settings,
  ShieldCheck,
  Moon,
  Sun,
  Menu,
  X,
} from "lucide-react";
import { useTheme } from "@/lib/useTheme";
import { useDashboardEvents, type ConnectionStatus } from "@/lib/useDashboardEvents";
import { HatPicker } from "@/components/layout/HatPicker";
import { BrainSelector } from "@/components/layout/BrainSelector";

interface NavItem {
  /** Literal route path the Link component accepts. TanStack
   *  Router's typed routes require a static `to` string. */
  to:
    | "/brains/$brainId"
    | "/brains/$brainId/domains"
    | "/brains/$brainId/federation"
    | "/brains/$brainId/skills"
    | "/brains/$brainId/publish-gates"
    | "/brains/$brainId/approvals"
    | "/brains/$brainId/services"
    | "/brains/$brainId/logs"
    | "/brains/$brainId/secrets"
    | "/brains/$brainId/settings";
  /** Path suffix used by `isActive` to compare against the
   *  currently-rendered pathname (`""` for the brain root). */
  suffix: string;
  label: string;
  icon: ReactNode;
}

const NAV: NavItem[] = [
  {
    to: "/brains/$brainId",
    suffix: "",
    label: "Overview",
    icon: <LayoutDashboard className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/domains",
    suffix: "/domains",
    label: "Domains",
    icon: <Layers className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/federation",
    suffix: "/federation",
    label: "Federation",
    icon: <Network className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/skills",
    suffix: "/skills",
    label: "Skills",
    icon: <BookOpen className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/publish-gates",
    suffix: "/publish-gates",
    label: "Publish gates",
    icon: <GitMerge className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/approvals",
    suffix: "/approvals",
    label: "Approvals",
    icon: <ShieldCheck className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/services",
    suffix: "/services",
    label: "Services",
    icon: <Server className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/logs",
    suffix: "/logs",
    label: "Logs",
    icon: <ScrollText className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/secrets",
    suffix: "/secrets",
    label: "Secrets",
    icon: <KeyRound className="h-4 w-4" />,
  },
  {
    to: "/brains/$brainId/settings",
    suffix: "/settings",
    label: "Settings",
    icon: <Settings className="h-4 w-4" />,
  },
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
  // Subscribe at the shell level so the connection lives for the
  // whole session — pages mount/unmount, but the SSE socket persists.
  const liveStatus = useDashboardEvents();

  // Read brainId from URL params when we're inside `/brains/$brainId/`;
  // strict: false because the index route `/` has no brainId. The
  // sidebar Brain selector handles that case (renders "no brain
  // selected" briefly while the index redirect is in flight).
  const params = useParams({ strict: false }) as { brainId?: string };
  const brainId = params.brainId;

  const isActive = (item: NavItem): boolean => {
    if (!brainId) return false;
    const target = `/brains/${brainId}${item.suffix}`;
    if (item.suffix === "") {
      // Overview is active for the bare brain root and anything under
      // /domains/<name> (the detail page) — keep Domains active for
      // those, but Overview only for the literal root.
      return pathname === target || pathname === `${target}/`;
    }
    return pathname === target || pathname.startsWith(`${target}/`);
  };

  return (
    <div className="min-h-screen bg-background text-foreground flex">
      {/* Desktop sidebar */}
      <aside className="hidden md:flex md:flex-col md:w-60 md:border-r md:border-border md:sticky md:top-0 md:h-screen">
        <SidebarContent
          isActive={isActive}
          onItemClick={() => setMobileOpen(false)}
          liveStatus={liveStatus}
          brainId={brainId}
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
              liveStatus={liveStatus}
              brainId={brainId}
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
  liveStatus,
  brainId,
}: {
  isActive: (n: NavItem) => boolean;
  onItemClick: () => void;
  liveStatus: ConnectionStatus;
  brainId: string | undefined;
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

      <div className="px-3 pb-3 space-y-2">
        <BrainSelector />
        {brainId && <HatPicker />}
      </div>

      <nav className="flex-1 px-2 space-y-1" aria-label="Primary">
        {brainId &&
          NAV.map((item) => (
            <Link
              key={item.suffix}
              to={item.to}
              params={{ brainId }}
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

      <div className="border-t border-border px-3 py-3 flex items-center justify-between gap-2">
        <LiveIndicator status={liveStatus} />
        <span className="text-xs text-muted-foreground font-mono">v3.4 Phase 2.1</span>
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

function LiveIndicator({ status }: { status: ConnectionStatus }) {
  const dot =
    status === "live"
      ? "bg-emerald-400"
      : status === "connecting"
        ? "bg-amber-400 animate-pulse"
        : status === "offline"
          ? "bg-red-400"
          : "bg-muted-foreground/40";
  const label = {
    live: "live",
    connecting: "...",
    offline: "offline",
    disabled: "static",
  }[status];
  const tooltip = {
    live: "Connected to /api/events — pages refresh as the Brain changes.",
    connecting: "Connecting to /api/events…",
    offline: "Disconnected from /api/events. Pages fall back to polling.",
    disabled:
      "File watcher not available — live updates disabled. Pages refresh on load only.",
  }[status];
  return (
    <span
      className="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
      title={tooltip}
      data-testid="live-indicator"
      data-status={status}
    >
      <span className={`h-2 w-2 rounded-full ${dot}`} />
      {label}
    </span>
  );
}
