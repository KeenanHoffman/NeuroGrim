import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SettingsPage } from "./SettingsPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import type { ConfigFileResponse } from "@bindings/ConfigFileResponse";

function mockConfigFetch(map: Record<string, ConfigFileResponse>) {
  global.fetch = vi.fn().mockImplementation(async (url: string) => {
    for (const [name, payload] of Object.entries(map)) {
      if (url.includes(`/config-file/${name}`)) {
        return {
          ok: true,
          status: 200,
          json: async () => payload,
          text: async () => "",
        } as Response;
      }
    }
    // Registry GET returns a minimal valid response so the
    // RegistryTab renders without errors when other tabs are
    // exercised.
    if (url.includes("/registry") && !url.includes("registry/")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          brain_id: "test-brain",
          path: "/proj/.claude/brain-registry.json",
          etag: "abc",
          registry: { config: { domain_weights: {} } },
        }),
        text: async () => "",
      } as Response;
    }
    return {
      ok: true,
      status: 200,
      json: async () => ({
        name: "unknown",
        present: false,
        path: "/x",
        text: null,
        error: null,
      }),
      text: async () => "",
    } as Response;
  });
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <SettingsPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("SettingsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/settings");
  });

  it("renders culture tab by default", async () => {
    mockConfigFetch({
      "culture.yaml": {
        name: "culture.yaml",
        present: true,
        path: "/proj/.claude/culture.yaml",
        text: "schema_version: 1\nvalues:\n  - positivity\n",
        error: null,
      },
    });
    renderPage();
    expect(await screen.findByTestId("settings-culture-card")).toBeInTheDocument();
    expect(
      await screen.findByTestId("settings-culture-text"),
    ).toBeInTheDocument();
  });

  it("switches tabs when clicked", async () => {
    mockConfigFetch({
      "culture.yaml": {
        name: "culture.yaml",
        present: true,
        path: "/x",
        text: "values: []\n",
        error: null,
      },
      "queue-config.yaml": {
        name: "queue-config.yaml",
        present: false,
        path: "/y",
        text: null,
        error: null,
      },
    });
    renderPage();
    await screen.findByTestId("settings-culture-card");
    fireEvent.click(screen.getByTestId("tab-queue-config"));
    expect(
      await screen.findByTestId("settings-queue-config-card"),
    ).toBeInTheDocument();
    expect(
      await screen.findByTestId("settings-queue-config-absent"),
    ).toBeInTheDocument();
  });

  it("shows absent state for missing config files", async () => {
    mockConfigFetch({
      "culture.yaml": {
        name: "culture.yaml",
        present: false,
        path: "/proj/.claude/culture.yaml",
        text: null,
        error: null,
      },
    });
    renderPage();
    expect(
      await screen.findByTestId("settings-culture-absent"),
    ).toBeInTheDocument();
  });

  it("publish-gates tab points at the dedicated page", async () => {
    mockConfigFetch({});
    renderPage();
    // Wait for the page shell to render before clicking tabs.
    await screen.findByTestId("settings-page");
    fireEvent.click(screen.getByTestId("tab-publish-gates"));
    expect(
      await screen.findByTestId("settings-publish-gates-pointer"),
    ).toBeInTheDocument();
    // Link to the dedicated page is visible.
    expect(screen.getByText(/publish gates page/i)).toBeInTheDocument();
  });

  it("renders the file path for diagnostic", async () => {
    mockConfigFetch({
      "culture.yaml": {
        name: "culture.yaml",
        present: true,
        path: "C:\\proj\\.claude\\culture.yaml",
        text: "x: y",
        error: null,
      },
    });
    renderPage();
    expect(
      await screen.findByText(/C:\\proj\\.claude\\culture\.yaml/),
    ).toBeInTheDocument();
  });

  // S15-C-4 v1 — Registry editor tab tests.
  describe("Registry tab (C-4 v1)", () => {
    function mockRegistryFetch(
      registry: Record<string, unknown>,
      etag = "abc123",
    ) {
      global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes("/registry") && !url.includes("/config-file/")) {
          if (init?.method === "PUT") {
            const body = JSON.parse(init.body as string) as {
              expected_etag: string;
            };
            if (body.expected_etag !== etag) {
              return {
                ok: false,
                status: 409,
                json: async () => ({
                  error: "etag mismatch",
                  code: "etag-conflict",
                }),
                text: async () => "",
              } as Response;
            }
            return {
              ok: true,
              status: 200,
              json: async () => ({ ok: true, etag: "new-etag" }),
              text: async () => "",
            } as Response;
          }
          return {
            ok: true,
            status: 200,
            json: async () => ({
              brain_id: "test-brain",
              path: "/proj/.claude/brain-registry.json",
              etag,
              registry,
            }),
            text: async () => "",
          } as Response;
        }
        return {
          ok: true,
          status: 200,
          json: async () => ({
            name: "unknown",
            present: false,
            path: "/x",
            text: null,
            error: null,
          }),
          text: async () => "",
        } as Response;
      });
    }

    it("renders one slider per declared domain weight", async () => {
      mockRegistryFetch({
        config: {
          domain_weights: {
            "test-health": 0.5,
            "code-quality": 0.5,
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      expect(
        await screen.findByTestId("registry-domain-row-test-health"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("registry-domain-row-code-quality"),
      ).toBeInTheDocument();
    });

    it("shows weight sum + valid hint when sum is 1.0", async () => {
      mockRegistryFetch({
        config: { domain_weights: { a: 0.6, b: 0.4 } },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      const sum = await screen.findByTestId("registry-weight-sum");
      expect(sum.textContent).toContain("1.000");
      expect(sum.textContent?.toLowerCase()).toContain("valid");
    });

    it("disables Save button when sum is invalid", async () => {
      mockRegistryFetch({
        config: { domain_weights: { a: 0.6, b: 0.4 } },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      // Adjust slider for `a` to break the 1.0 sum.
      const slider = await screen.findByTestId("registry-slider-a");
      fireEvent.change(slider, { target: { value: "0.9" } });
      const saveBtn = screen.getByTestId(
        "registry-save-button",
      ) as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(true);
      expect(
        screen.getByText(/must be 1\.0/i),
      ).toBeInTheDocument();
    });

    it("renders empty-state message when no domain weights declared", async () => {
      mockRegistryFetch({ config: { domain_weights: {} } });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      expect(
        await screen.findByText(/no domain weights declared/i),
      ).toBeInTheDocument();
    });
  });
});
