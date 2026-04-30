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

  // S15-C-4 v2 — Curated form sub-tabs (autonomy / hats / federation).
  describe("Registry tab (C-4 v2)", () => {
    function mockRegistryFetch(
      registry: Record<string, unknown>,
      etag = "abc123",
    ) {
      const captured: { lastBody?: unknown } = {};
      global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.includes("/registry") && !url.includes("/config-file/")) {
          if (init?.method === "PUT") {
            captured.lastBody = JSON.parse(init.body as string);
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
      return captured;
    }

    it("registry tab shows the four sub-tab buttons", async () => {
      mockRegistryFetch({ config: { domain_weights: { a: 1.0 } } });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      expect(
        await screen.findByTestId("registry-subtab-weights"),
      ).toBeInTheDocument();
      expect(screen.getByTestId("registry-subtab-autonomy")).toBeInTheDocument();
      expect(screen.getByTestId("registry-subtab-hats")).toBeInTheDocument();
      expect(
        screen.getByTestId("registry-subtab-federation"),
      ).toBeInTheDocument();
    });

    it("autonomy sub-tab renders absent state when no autonomy block declared", async () => {
      mockRegistryFetch({ config: { domain_weights: { a: 1.0 } } });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-autonomy"),
      );
      expect(
        await screen.findByTestId("registry-autonomy-absent"),
      ).toBeInTheDocument();
    });

    it("autonomy sub-tab renders one row per action_type", async () => {
      mockRegistryFetch({
        config: {
          domain_weights: { a: 1.0 },
          autonomy: {
            levels: {
              auto: { requires_approval: false, description: "auto-runs" },
              approve: {
                requires_approval: true,
                description: "needs human ok",
              },
            },
            action_types: {
              "edit-code": {
                default_level: "approve",
                blast_radius: "medium",
                reversible: true,
              },
              "refresh-snapshot": {
                default_level: "auto",
                blast_radius: "low",
                reversible: true,
              },
            },
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-autonomy"),
      );
      expect(
        await screen.findByTestId("registry-autonomy-row-edit-code"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("registry-autonomy-row-refresh-snapshot"),
      ).toBeInTheDocument();
      // The select shows the current value.
      const select = screen.getByTestId(
        "registry-autonomy-level-edit-code",
      ) as HTMLSelectElement;
      expect(select.value).toBe("approve");
    });

    it("changing autonomy level enables Save and writes the new level on PUT", async () => {
      const captured = mockRegistryFetch({
        config: {
          domain_weights: { a: 1.0 },
          autonomy: {
            levels: {
              auto: { requires_approval: false, description: "" },
              notify: { requires_approval: false, description: "" },
              approve: { requires_approval: true, description: "" },
              blocked: { requires_approval: true, description: "" },
            },
            action_types: {
              "edit-code": { default_level: "approve" },
            },
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-autonomy"),
      );
      const select = (await screen.findByTestId(
        "registry-autonomy-level-edit-code",
      )) as HTMLSelectElement;
      fireEvent.change(select, { target: { value: "notify" } });
      const saveBtn = screen.getByTestId(
        "registry-save-button",
      ) as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
      fireEvent.click(saveBtn);
      // Wait for the PUT to be captured.
      await new Promise((r) => setTimeout(r, 50));
      const body = captured.lastBody as {
        registry: { config: { autonomy: { action_types: Record<string, { default_level: string }> } } };
      };
      expect(
        body.registry.config.autonomy.action_types["edit-code"].default_level,
      ).toBe("notify");
    });

    it("hats sub-tab renders one row per declared hat", async () => {
      mockRegistryFetch({
        config: {
          domain_weights: { a: 0.5, b: 0.5 },
          hats: {
            engineer: {
              description: "active dev",
              domain_multipliers: { a: 2.0 },
            },
            reviewer: {
              description: "review",
              domain_multipliers: { b: 1.5 },
            },
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(await screen.findByTestId("registry-subtab-hats"));
      expect(
        await screen.findByTestId("registry-hat-row-engineer"),
      ).toBeInTheDocument();
      expect(screen.getByTestId("registry-hat-row-reviewer")).toBeInTheDocument();
      // Multipliers slider visible for declared domains.
      expect(
        screen.getByTestId("registry-hat-engineer-slider-a"),
      ).toBeInTheDocument();
    });

    it("adding a hat enables Save with the new entry", async () => {
      const captured = mockRegistryFetch({
        config: {
          domain_weights: { a: 1.0 },
          hats: {},
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(await screen.findByTestId("registry-subtab-hats"));
      const nameInput = (await screen.findByTestId(
        "registry-hat-new-name",
      )) as HTMLInputElement;
      fireEvent.change(nameInput, { target: { value: "auditor" } });
      fireEvent.click(screen.getByTestId("registry-hat-add-button"));
      // The new row appears.
      expect(
        await screen.findByTestId("registry-hat-row-auditor"),
      ).toBeInTheDocument();
      // Save button is enabled.
      const saveBtn = screen.getByTestId(
        "registry-save-button",
      ) as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
      fireEvent.click(saveBtn);
      await new Promise((r) => setTimeout(r, 50));
      const body = captured.lastBody as {
        registry: { config: { hats: Record<string, unknown> } };
      };
      expect(body.registry.config.hats.auditor).toBeDefined();
    });

    it("federation sub-tab renders one row per declared child", async () => {
      mockRegistryFetch({
        config: {
          domain_weights: { a: 1.0 },
          children: {
            "python-starter": {
              display_name: "Python Starter",
              a2a_endpoint: "http://localhost:8423/a2a/v1/",
              weight: 1.0,
              enabled: true,
            },
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-federation"),
      );
      expect(
        await screen.findByTestId("registry-federation-row-python-starter"),
      ).toBeInTheDocument();
      const endpoint = screen.getByTestId(
        "registry-federation-endpoint-python-starter",
      ) as HTMLInputElement;
      expect(endpoint.value).toBe("http://localhost:8423/a2a/v1/");
    });

    it("removing a federation child enables Save and PUT excludes it", async () => {
      const captured = mockRegistryFetch({
        config: {
          domain_weights: { a: 1.0 },
          children: {
            "python-starter": {
              display_name: "Python Starter",
              a2a_endpoint: "http://localhost:8423/a2a/v1/",
              weight: 1.0,
              enabled: true,
            },
          },
        },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-federation"),
      );
      fireEvent.click(
        await screen.findByTestId("registry-federation-delete-python-starter"),
      );
      const saveBtn = screen.getByTestId(
        "registry-save-button",
      ) as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
      fireEvent.click(saveBtn);
      await new Promise((r) => setTimeout(r, 50));
      const body = captured.lastBody as {
        registry: { config: { children: Record<string, unknown> } };
      };
      expect(body.registry.config.children["python-starter"]).toBeUndefined();
    });

    it("federation add-child seeds a stub entry the operator fills in", async () => {
      mockRegistryFetch({
        config: { domain_weights: { a: 1.0 } },
      });
      renderPage();
      await screen.findByTestId("settings-page");
      fireEvent.click(screen.getByTestId("tab-registry"));
      fireEvent.click(
        await screen.findByTestId("registry-subtab-federation"),
      );
      const nameInput = (await screen.findByTestId(
        "registry-federation-new-name",
      )) as HTMLInputElement;
      fireEvent.change(nameInput, { target: { value: "child-one" } });
      fireEvent.click(screen.getByTestId("registry-federation-add-button"));
      expect(
        await screen.findByTestId("registry-federation-row-child-one"),
      ).toBeInTheDocument();
      // The endpoint field starts blank — operator fills it in.
      const endpoint = screen.getByTestId(
        "registry-federation-endpoint-child-one",
      ) as HTMLInputElement;
      expect(endpoint.value).toBe("");
    });
  });
});
