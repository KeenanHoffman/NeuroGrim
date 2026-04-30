import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";

// `isCurrentPageHttps` reads window.location.protocol, which jsdom
// makes hard to override at runtime. Mock the module so v3 banner
// tests can drive the on-HTTPS / on-HTTP branches deterministically.
// The pure-helper tests in useTlsStatus.test.ts cover the real
// window.location code path.
vi.mock("./useTlsStatus", async (importOriginal) => {
  const orig = await importOriginal<typeof import("./useTlsStatus")>();
  return {
    ...orig,
    isCurrentPageHttps: vi.fn(() => false),
  };
});

// SecretsPage must be imported AFTER vi.mock so it picks up the
// mocked binding. vitest hoists `vi.mock` so the order doesn't
// strictly matter, but the convention is clearer this way.
import { SecretsPage } from "./SecretsPage";
import { isCurrentPageHttps } from "./useTlsStatus";

// In the modal, jsdom's `window.confirm` is undefined; stub it to a
// truthy value so the delete-row tests can fire.
beforeEach(() => {
  vi.stubGlobal("confirm", () => true);
});

function mockFetch(handlers: {
  list?: () => Record<string, unknown>;
  set?: (body: Record<string, unknown>) => Record<string, unknown>;
  setError?: () => Response;
  del?: () => Record<string, unknown>;
  tlsStatus?: () => Record<string, unknown>;
}) {
  global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    if (url === "/api/tls-status") {
      return ok(
        handlers.tlsStatus?.() ?? {
          https_available: false,
        },
      );
    }
    if (url.endsWith("/secrets") && (!init || init.method !== "POST" && init.method !== "DELETE")) {
      return ok(handlers.list?.() ?? { brain_id: "test", manifest_path: "/x", manifest_present: false, items: [] });
    }
    if (init?.method === "POST") {
      if (handlers.setError) {
        return handlers.setError();
      }
      if (handlers.set) {
        const body = JSON.parse(init.body as string);
        return ok(handlers.set(body));
      }
    }
    if (init?.method === "DELETE" && handlers.del) {
      return ok(handlers.del());
    }
    return ok({});
  });
}

function ok(payload: unknown): Response {
  return {
    ok: true,
    status: 200,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  } as Response;
}

function err(status: number, payload: unknown): Response {
  return {
    ok: false,
    status,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  } as Response;
}

function renderPage() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <SecretsPage />
    </BrainProvider>,
  );
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

describe("SecretsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/brains/test-brain/secrets");
  });

  it("renders 'no manifest' state when manifest is absent", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/proj/.claude/secret-refs.yaml",
        manifest_present: false,
        items: [],
      }),
    });
    renderPage();
    expect(await screen.findByTestId("secrets-no-manifest")).toBeInTheDocument();
  });

  it("renders the secrets table when the manifest declares entries", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/proj/.claude/secret-refs.yaml",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: "GitHub PAT",
            provider: "env",
            rotation_days: 90,
            present: true,
            updated_at: "2026-04-29T12:00:00Z",
            backend: "os-native",
          },
          {
            id: "anthropic-key",
            description: "Anthropic API key",
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    expect(screen.getByText("github-pat")).toBeInTheDocument();
    expect(screen.getByText("anthropic-key")).toBeInTheDocument();
    expect(screen.getByTestId("secret-status-github-pat").textContent).toContain(
      "present",
    );
    expect(screen.getByTestId("secret-status-anthropic-key").textContent).toContain(
      "missing",
    );
  });

  it("opens the modal when 'Set value' is clicked for an absent secret", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: null,
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    fireEvent.click(screen.getByTestId("secret-edit-github-pat"));
    expect(
      await screen.findByTestId("secret-modal-github-pat"),
    ).toBeInTheDocument();
    // The save button inside the modal carries text "Save" (not
    // "Saving…" until the request lands). Its presence proves the
    // modal opened correctly.
    expect(screen.getByTestId("secret-modal-save")).toBeInTheDocument();
  });

  it("posts the value when Save is clicked + invalidates the list", async () => {
    let savedBody: Record<string, unknown> | null = null;
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: null,
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
      set: (body) => {
        savedBody = body;
        return {
          brain_id: "test-brain",
          secret_id: "github-pat",
          updated_at: "2026-04-29T12:34:56Z",
        };
      },
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    fireEvent.click(screen.getByTestId("secret-edit-github-pat"));
    const input = (await screen.findByTestId(
      "secret-value-input",
    )) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "ghp_secret_value" } });
    fireEvent.click(screen.getByTestId("secret-modal-save"));
    // Wait for the modal to close after success.
    await new Promise((r) => setTimeout(r, 0));
    expect(savedBody).toEqual({ value: "ghp_secret_value" });
  });

  it("shows save error inline + leaves modal open", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: null,
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
      setError: () => err(403, { error: "mutations are disabled" }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    fireEvent.click(screen.getByTestId("secret-edit-github-pat"));
    const input = (await screen.findByTestId(
      "secret-value-input",
    )) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "anything" } });
    fireEvent.click(screen.getByTestId("secret-modal-save"));
    expect(await screen.findByTestId("secret-save-error")).toBeInTheDocument();
    expect(
      screen.getByTestId("secret-modal-github-pat"),
    ).toBeInTheDocument();
  });

  it("uses a password input by default + reveal toggles to text", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: null,
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    fireEvent.click(screen.getByTestId("secret-edit-github-pat"));
    const input = (await screen.findByTestId(
      "secret-value-input",
    )) as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.click(screen.getByTestId("secret-reveal-toggle"));
    expect(input.type).toBe("text");
  });

  it("shows 'Rotate' label when secret is already present", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "github-pat",
            description: null,
            provider: null,
            rotation_days: null,
            present: true,
            updated_at: "2026-04-29T12:00:00Z",
            backend: "os-native",
          },
        ],
      }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    const button = screen.getByTestId("secret-edit-github-pat");
    expect(button.textContent).toContain("Rotate");
  });

  it("shows delete button only when the secret is present", async () => {
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "with-value",
            description: null,
            provider: null,
            rotation_days: null,
            present: true,
            updated_at: "2026-04-29T12:00:00Z",
            backend: "os-native",
          },
          {
            id: "no-value",
            description: null,
            provider: null,
            rotation_days: null,
            present: false,
            updated_at: null,
            backend: null,
          },
        ],
      }),
    });
    renderPage();
    await screen.findByTestId("secrets-table");
    expect(screen.getByTestId("secret-delete-with-value")).toBeInTheDocument();
    expect(screen.queryByTestId("secret-delete-no-value")).toBeNull();
  });

  it("never shows the secret value back from the list endpoint", async () => {
    // Even if the server were to wrongly include a `value` field,
    // the UI must not render it. This test asserts the contract
    // by inspecting all rendered text.
    mockFetch({
      list: () => ({
        brain_id: "test-brain",
        manifest_path: "/x",
        manifest_present: true,
        items: [
          {
            id: "secret",
            description: "describe",
            provider: "env",
            rotation_days: null,
            present: true,
            updated_at: "2026-04-29T12:00:00Z",
            backend: "os-native",
            // Hostile injected field — UI should ignore it.
            value: "DO_NOT_RENDER_ME",
          } as unknown as object,
        ],
      }),
    });
    const { container } = renderPage();
    await screen.findByTestId("secrets-table");
    expect(container.textContent).not.toContain("DO_NOT_RENDER_ME");
  });

  // ── S14-S-4.5 v3: TLS banner ──────────────────────────────────

  describe("TLS banner (v3)", () => {
    beforeEach(() => {
      window.localStorage.clear();
    });

    /**
     * Force the on-HTTPS branch of TlsBanner. Uses the
     * module-mocked `isCurrentPageHttps` (see `vi.mock` at the top
     * of the file). The pure-helper tests in useTlsStatus.test.ts
     * cover the real window.location code path.
     */
    function stubLocation(opts: { isHttps: boolean }) {
      vi.mocked(isCurrentPageHttps).mockReturnValue(opts.isHttps);
    }

    it("shows 'switch to HTTPS' banner when on HTTP and HTTPS available", async () => {
      stubLocation({ isHttps: false });
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({
          https_available: true,
          https_port: 8421,
          fingerprint_sha256: "ab".repeat(32),
        }),
      });
      renderPage();
      expect(
        await screen.findByTestId("tls-banner-switch"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("tls-banner-switch-button"),
      ).toBeInTheDocument();
      // The fingerprint surfaces so operators can compare to the
      // browser's accept-cert prompt.
      expect(screen.getByText(/expected fingerprint:/i)).toBeInTheDocument();
    });

    it("shows 'no TLS configured' banner when HTTPS is absent", async () => {
      stubLocation({ isHttps: false });
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({ https_available: false }),
      });
      renderPage();
      expect(
        await screen.findByTestId("tls-banner-no-tls"),
      ).toBeInTheDocument();
      expect(
        screen.getByText(/tls-cert generate/i),
      ).toBeInTheDocument();
    });

    it("shows first-visit banner on HTTPS with no pin yet", async () => {
      stubLocation({ isHttps: true });
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({
          https_available: true,
          https_port: 8421,
          fingerprint_sha256: "cafef00d".repeat(8),
        }),
      });
      renderPage();
      expect(
        await screen.findByTestId("tls-banner-first-visit"),
      ).toBeInTheDocument();
      // Fingerprint surfaces so the operator can compare to
      // what their browser's "View certificate" dialog shows.
      expect(
        screen.getByText("cafef00d".repeat(8)),
      ).toBeInTheDocument();
      // Trust button is present (no auto-pin until clicked).
      expect(
        screen.getByTestId("tls-banner-first-visit-trust"),
      ).toBeInTheDocument();
      // localStorage stays empty until Trust is clicked.
      expect(
        window.localStorage.getItem("neurogrim:tls-fingerprint:localhost"),
      ).toBeNull();
    });

    it("pins fingerprint when Trust is clicked", async () => {
      stubLocation({ isHttps: true });
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({
          https_available: true,
          https_port: 8421,
          fingerprint_sha256: "cafef00d".repeat(8),
        }),
      });
      renderPage();
      await screen.findByTestId("tls-banner-first-visit-trust");
      fireEvent.click(screen.getByTestId("tls-banner-first-visit-trust"));
      expect(
        window.localStorage.getItem("neurogrim:tls-fingerprint:localhost"),
      ).toBe("cafef00d".repeat(8));
      // After pinning, the first-visit banner disappears (silent
      // steady state since the pin matches the server fingerprint).
      expect(
        screen.queryByTestId("tls-banner-first-visit"),
      ).toBeNull();
    });

    it("renders silently (no banner) when fingerprint matches pin", async () => {
      stubLocation({ isHttps: true });
      window.localStorage.setItem(
        "neurogrim:tls-fingerprint:localhost",
        "ab".repeat(32),
      );
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({
          https_available: true,
          https_port: 8421,
          fingerprint_sha256: "ab".repeat(32),
        }),
      });
      renderPage();
      // Wait for the page shell so async fetches resolve.
      await screen.findByTestId("secrets-page");
      // None of the banner test ids should appear.
      expect(screen.queryByTestId("tls-banner-switch")).toBeNull();
      expect(screen.queryByTestId("tls-banner-no-tls")).toBeNull();
      expect(screen.queryByTestId("tls-banner-first-visit")).toBeNull();
      expect(screen.queryByTestId("tls-banner-mismatch")).toBeNull();
    });

    it("warns on fingerprint mismatch + offers clear-pin", async () => {
      stubLocation({ isHttps: true });
      window.localStorage.setItem(
        "neurogrim:tls-fingerprint:localhost",
        "old-pin",
      );
      mockFetch({
        list: () => ({
          brain_id: "test-brain",
          manifest_path: "/x",
          manifest_present: true,
          items: [],
        }),
        tlsStatus: () => ({
          https_available: true,
          https_port: 8421,
          fingerprint_sha256: "new-fingerprint",
        }),
      });
      renderPage();
      expect(
        await screen.findByTestId("tls-banner-mismatch"),
      ).toBeInTheDocument();
      // Both old + new values surface so operators can compare.
      expect(screen.getByText(/old-pin/)).toBeInTheDocument();
      expect(screen.getByText(/new-fingerprint/)).toBeInTheDocument();
      // Clear pin removes the stored value.
      fireEvent.click(screen.getByTestId("tls-banner-mismatch-clear"));
      expect(
        window.localStorage.getItem("neurogrim:tls-fingerprint:localhost"),
      ).toBeNull();
    });
  });
});
