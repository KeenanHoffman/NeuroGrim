import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ServicesPage } from "./ServicesPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { ServicesListResponse } from "@bindings/ServicesListResponse";
import type { ServiceSnapshot } from "@bindings/ServiceSnapshot";

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
});
