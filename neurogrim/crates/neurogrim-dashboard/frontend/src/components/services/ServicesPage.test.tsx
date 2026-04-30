import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ServicesPage } from "./ServicesPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { ServicesListResponse } from "@bindings/ServicesListResponse";
import type { ServiceSnapshot } from "@bindings/ServiceSnapshot";
import type { PeerLogResponse } from "@bindings/PeerLogResponse";

const svc = (overrides: Partial<ServiceSnapshot> = {}): ServiceSnapshot => ({
  peer_name: "neurogrim",
  pid: 12345,
  port: 8421,
  started_at: new Date().toISOString(),
  log_path: ".claude/brain/logs/neurogrim.log",
  ...overrides,
});

function mockFetch(payload: ServicesListResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  } as Response);
}

/**
 * Mock fetch that handles BOTH the services-list and the
 * per-peer log endpoint. Returns the right payload based on URL
 * substring; tests pass partial responses for each.
 */
function mockServicesAndLogFetch(opts: {
  services: ServicesListResponse;
  log: PeerLogResponse;
}) {
  global.fetch = vi.fn().mockImplementation(async (url: RequestInfo | URL) => {
    const u = typeof url === "string" ? url : url.toString();
    if (u.includes("/peers/") && u.includes("/log")) {
      return {
        ok: true,
        status: 200,
        json: async () => opts.log,
        text: async () => JSON.stringify(opts.log),
      } as Response;
    }
    return {
      ok: true,
      status: 200,
      json: async () => opts.services,
      text: async () => JSON.stringify(opts.services),
    } as Response;
  });
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <ServicesPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("ServicesPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/services");
  });

  it("renders empty state when no services tracked", async () => {
    mockFetch({ services: [] });
    renderPage();
    expect(await screen.findByTestId("services-empty")).toBeInTheDocument();
    expect(
      screen.getByText(/no services tracked/i),
    ).toBeInTheDocument();
  });

  it("renders services table with one row per peer", async () => {
    mockFetch({
      services: [svc({ peer_name: "neurogrim" }), svc({ peer_name: "lsp-brains", port: 8422 })],
    });
    renderPage();
    expect(await screen.findByTestId("services-table")).toBeInTheDocument();
    expect(screen.getByTestId("service-row-neurogrim")).toBeInTheDocument();
    expect(screen.getByTestId("service-row-lsp-brains")).toBeInTheDocument();
  });

  it("displays pid + port + log_path in the row", async () => {
    mockFetch({
      services: [svc({ peer_name: "alpha", pid: 9999, port: 17345, log_path: "/tmp/alpha.log" })],
    });
    renderPage();
    expect(await screen.findByTestId("service-row-alpha")).toBeInTheDocument();
    expect(screen.getByText("9999")).toBeInTheDocument();
    expect(screen.getByText("17345")).toBeInTheDocument();
    expect(screen.getByText("/tmp/alpha.log")).toBeInTheDocument();
  });

  it("renders error state when fetch fails", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({}),
      text: async () => "",
    } as Response);
    renderPage();
    expect(
      await screen.findByText(/failed to load services/i),
    ).toBeInTheDocument();
  });

  // ── Per-peer log tail viewer (S15-C-2 expansion) ──────────────

  it("each row has a View log button that opens the peer log modal", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/proj/.claude/brain/logs/alpha.log",
        present: true,
        total_size_bytes: 42n,
        truncated: false,
        lines: ["boot complete", "ready on port 8421"],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    // Modal opens with the log content.
    expect(
      await screen.findByTestId("peer-log-modal-alpha"),
    ).toBeInTheDocument();
    const content = await screen.findByTestId("peer-log-content");
    expect(content.textContent).toContain("boot complete");
    expect(content.textContent).toContain("ready on port 8421");
  });

  it("close button dismisses the peer log modal", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/x",
        present: true,
        total_size_bytes: 0n,
        truncated: false,
        lines: ["x"],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    await screen.findByTestId("peer-log-backdrop");
    fireEvent.click(screen.getByTestId("peer-log-close"));
    expect(screen.queryByTestId("peer-log-backdrop")).toBeNull();
  });

  it("ESC dismisses the peer log modal", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/x",
        present: true,
        total_size_bytes: 0n,
        truncated: false,
        lines: ["x"],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    await screen.findByTestId("peer-log-backdrop");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTestId("peer-log-backdrop")).toBeNull();
  });

  it("backdrop click dismisses the peer log modal", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/x",
        present: true,
        total_size_bytes: 0n,
        truncated: false,
        lines: ["x"],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    const backdrop = await screen.findByTestId("peer-log-backdrop");
    fireEvent.click(backdrop);
    expect(screen.queryByTestId("peer-log-backdrop")).toBeNull();
  });

  it("renders absent state when log file is missing", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/proj/.claude/brain/logs/alpha.log",
        present: false,
        total_size_bytes: null,
        truncated: false,
        lines: [],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    expect(
      await screen.findByTestId("peer-log-absent"),
    ).toBeInTheDocument();
  });

  it("shows truncation hint when log is larger than tail window", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/proj/.claude/brain/logs/alpha.log",
        present: true,
        total_size_bytes: 12n * 1024n * 1024n, // 12 MB
        truncated: true,
        lines: ["recent line"],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    // Wait for the data fetch to land so the truncation hint
    // (which renders alongside the log_path inside the data
    // check) is visible.
    await screen.findByTestId("peer-log-content");
    expect(
      screen.getByText(/showing last/i),
    ).toBeInTheDocument();
  });

  it("renders empty-file state when log exists but has zero bytes", async () => {
    mockServicesAndLogFetch({
      services: { services: [svc({ peer_name: "alpha" })] },
      log: {
        log_path: "/x",
        present: true,
        total_size_bytes: 0n,
        truncated: false,
        lines: [],
      },
    });
    renderPage();
    await screen.findByTestId("service-row-alpha");
    fireEvent.click(screen.getByTestId("service-view-log-alpha"));
    expect(
      await screen.findByTestId("peer-log-empty"),
    ).toBeInTheDocument();
  });
});
