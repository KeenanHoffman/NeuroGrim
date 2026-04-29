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
});
