import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { PeerActions } from "./PeerActions";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { PeerDto } from "@bindings/PeerDto";

const alivePeer = (over: Partial<PeerDto> = {}): PeerDto => ({
  name: "neurogrim",
  display_name: "NeuroGrim",
  transport: "a2a",
  a2a_endpoint: "http://127.0.0.1:8421/a2a/v1/",
  brain_path: "NeuroGrim",
  weight: 1.0,
  read_only: false,
  enabled: true,
  status: { kind: "alive", message: "" },
  agent_card: null,
  ...over,
});

function mockFetch(routes: Record<string, unknown>, opts: { delayHealthMs?: number } = {}) {
  global.fetch = vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    for (const [pattern, payload] of Object.entries(routes)) {
      if (url.includes(pattern)) {
        // Simulate the (real-world) staggered resolution of /api/health
        // vs the federation peer query — this is the order that
        // surfaced the original Rules-of-Hooks crash.
        if (pattern === "/api/health" && opts.delayHealthMs) {
          await new Promise((r) => setTimeout(r, opts.delayHealthMs));
        }
        return {
          ok: true,
          status: 200,
          json: async () => payload,
        } as Response;
      }
    }
    return {
      ok: false,
      status: 404,
      json: async () => ({ error: "no route", code: "not-mocked" }),
    } as Response;
  }) as typeof fetch;
}

function renderActions(peer: PeerDto) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <PeerActions peer={peer} />
    </BrainProvider>
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}

describe("PeerActions", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  /**
   * REGRESSION GUARD for React #310 — "Rendered more hooks during
   * this render than during the previous render."
   *
   * The original PeerActions.tsx returned `null` BEFORE calling
   * useMutation when `useHealth().loading` was true. On the second
   * render (health resolved), it suddenly invoked useMutation for
   * the first time, breaking Rules of Hooks. This test simulates
   * that exact loading→resolved transition with a delay on
   * /api/health and asserts the component doesn't throw.
   */
  it("does not crash when health resolves after the first render (Rules of Hooks regression)", async () => {
    mockFetch(
      {
        "/api/health": {
          ok: true,
          registry_path: "/tmp/x",
          version: "3.5.0",
          mutations_allowed: true,
        },
      },
      { delayHealthMs: 50 }
    );

    // If the bug was present, React would throw inside the
    // second render and bubble to the test runner. Just rendering
    // is the assertion.
    renderActions(alivePeer());

    // Wait for the deferred /api/health to resolve and the
    // component to commit its real (non-null) tree.
    await waitFor(() => {
      expect(screen.getByTestId("peer-actions-neurogrim")).toBeInTheDocument();
    });
  });

  it("renders Start + Stop when mutations_allowed=true and peer is alive", async () => {
    mockFetch({
      "/api/health": {
        ok: true,
        registry_path: "/tmp/x",
        version: "3.5.0",
        mutations_allowed: true,
      },
    });
    renderActions(alivePeer());
    await waitFor(() => {
      expect(screen.getByTestId("peer-start-neurogrim")).toBeInTheDocument();
      expect(screen.getByTestId("peer-stop-neurogrim")).toBeInTheDocument();
    });
    // Peer is alive → Start is disabled, Stop is enabled.
    expect(screen.getByTestId("peer-start-neurogrim")).toBeDisabled();
    expect(screen.getByTestId("peer-stop-neurogrim")).not.toBeDisabled();
  });

  it("hides controls when mutations_allowed=false", async () => {
    mockFetch({
      "/api/health": {
        ok: true,
        registry_path: "/tmp/x",
        version: "3.5.0",
        mutations_allowed: false,
      },
    });
    const { container } = renderActions(alivePeer());
    // Wait a bit for health to resolve, then assert nothing rendered.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(container.querySelector("[data-testid='peer-actions-neurogrim']")).toBeNull();
  });

  it("hides controls when peer transport is not a2a", async () => {
    mockFetch({
      "/api/health": {
        ok: true,
        registry_path: "/tmp/x",
        version: "3.5.0",
        mutations_allowed: true,
      },
    });
    const { container } = renderActions(alivePeer({ transport: "subprocess" }));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(container.querySelector("[data-testid='peer-actions-neurogrim']")).toBeNull();
  });

  it("surfaces server error inline when start fails synchronously", async () => {
    mockFetch({
      "/api/health": {
        ok: true,
        registry_path: "/tmp/x",
        version: "3.5.0",
        mutations_allowed: true,
      },
    });
    // Override fetch for the start endpoint specifically.
    const originalFetch = global.fetch;
    global.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.includes("/peers/neurogrim/start")) {
        return {
          ok: false,
          status: 422,
          json: async () => ({
            error: "port 51234 is already bound by another process",
            code: "port-conflict",
          }),
        } as Response;
      }
      return originalFetch(input);
    }) as typeof fetch;

    // Start with a not-running peer so the Start button is enabled.
    renderActions(
      alivePeer({ status: { kind: "not-running", message: "" } })
    );
    await waitFor(() => {
      expect(screen.getByTestId("peer-start-neurogrim")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("peer-start-neurogrim"));
    await waitFor(() => {
      expect(
        screen.getByTestId("peer-actions-error-neurogrim")
      ).toHaveTextContent(/port 51234 is already bound/);
    });
  });
});
