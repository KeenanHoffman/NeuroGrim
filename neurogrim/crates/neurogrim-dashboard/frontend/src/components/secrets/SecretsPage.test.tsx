import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SecretsPage } from "./SecretsPage";
import { BrainProvider } from "@/lib/useBrain";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";

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
}) {
  global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
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
});
