import { OverviewPage } from "@/components/overview/OverviewPage";
import { DomainsPage } from "@/components/domains/DomainsPage";
import { DomainDetailPage } from "@/components/domains/DomainDetailPage";
import { FederationPage } from "@/components/federation/FederationPage";
import { matchDomainDetail, useRoute } from "@/lib/useRoute";

/**
 * Phase 1.3: route between Overview / Domains / Domain detail /
 * Federation via the in-house `useRoute` hook. Phase 1.5 swaps this
 * match statement for TanStack Router with typed routes.
 */
export default function App() {
  const { pathname, navigate } = useRoute();

  const page = (() => {
    if (pathname === "/" || pathname === "") return <OverviewPage />;
    if (pathname === "/domains") return <DomainsPage />;
    if (pathname === "/federation") return <FederationPage />;
    const detail = matchDomainDetail(pathname);
    if (detail) return <DomainDetailPage name={detail.name} />;
    return <NotFound />;
  })();

  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="border-b border-border">
        <div className="container max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-baseline gap-6">
            <button
              onClick={() => navigate("/")}
              className="text-xl font-semibold hover:text-foreground/80 transition-colors"
            >
              NeuroGrim Dashboard
            </button>
            <nav className="hidden md:flex items-center gap-4 text-sm">
              <NavLink
                href="/"
                active={pathname === "/" || pathname === ""}
                onClick={() => navigate("/")}
              >
                Overview
              </NavLink>
              <NavLink
                href="/domains"
                active={pathname === "/domains" || matchDomainDetail(pathname) !== null}
                onClick={() => navigate("/domains")}
              >
                Domains
              </NavLink>
              <NavLink
                href="/federation"
                active={pathname === "/federation"}
                onClick={() => navigate("/federation")}
              >
                Federation
              </NavLink>
            </nav>
          </div>
          <span className="text-xs text-muted-foreground">v3.4 Phase 1.3</span>
        </div>
      </header>
      <main className="container max-w-7xl mx-auto px-6 py-8">{page}</main>
    </div>
  );
}

function NavLink({
  href,
  active,
  onClick,
  children,
}: {
  href: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        onClick();
      }}
      className={
        active
          ? "text-foreground font-medium"
          : "text-muted-foreground hover:text-foreground transition-colors"
      }
    >
      {children}
    </a>
  );
}

function NotFound() {
  const { navigate } = useRoute();
  return (
    <div className="text-center py-16">
      <h2 className="text-2xl font-semibold">Page not found</h2>
      <p className="mt-2 text-sm text-muted-foreground">
        That route doesn't exist in the v3.4 dashboard.
      </p>
      <button
        onClick={() => navigate("/")}
        className="mt-4 text-sm text-primary underline-offset-4 hover:underline"
      >
        Back to Overview
      </button>
    </div>
  );
}
