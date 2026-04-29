import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SkillsPage } from "./SkillsPage";
import type { SkillsResponse } from "@bindings/SkillsResponse";
import type { SkillDto } from "@bindings/SkillDto";

const skill = (overrides: Partial<SkillDto> = {}): SkillDto => ({
  name: "rubber-duck",
  path: ".claude/skills/rubber-duck/SKILL.md",
  format: "plugin",
  description: "A Socratic listener for stuck moments.",
  last_invoked_at: "2026-04-27T12:00:00Z",
  invocation_count: 5,
  recent_invocation_count: 3,
  hygiene_status: "alive",
  ...overrides,
});

const resp = (overrides: Partial<SkillsResponse> = {}): SkillsResponse => ({
  skills: [],
  ledger_present: true,
  total_invocations: 0,
  alive_window_days: 30,
  ...overrides,
});

function mockFetch(payload: SkillsResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
  } as Response);
}

function renderWithQuery() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <SkillsPage />
    </QueryClientProvider>
  );
}

describe("SkillsPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/skills");
  });

  it("renders the header with skill count + window legend", async () => {
    mockFetch(
      resp({
        skills: [skill({ name: "alpha" }), skill({ name: "beta" })],
        total_invocations: 12,
      })
    );
    renderWithQuery();
    expect(await screen.findByText("Skills")).toBeInTheDocument();
    expect(
      screen.getByText(/2 declared.*12 invocations recorded.*alive = invoked in last 30d/i)
    ).toBeInTheDocument();
  });

  it("renders one row per skill", async () => {
    mockFetch(
      resp({
        skills: [
          skill({ name: "alpha", description: "Alpha skill" }),
          skill({ name: "beta", description: "Beta skill" }),
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("skill-row-alpha")).toBeInTheDocument();
    expect(screen.getByTestId("skill-row-beta")).toBeInTheDocument();
  });

  it("filter chips narrow the table by hygiene_status", async () => {
    mockFetch(
      resp({
        skills: [
          skill({ name: "live", hygiene_status: "alive" }),
          skill({
            name: "stale",
            hygiene_status: "dead",
            recent_invocation_count: 0,
          }),
          skill({
            name: "fresh",
            hygiene_status: "new",
            invocation_count: 0,
            recent_invocation_count: 0,
            last_invoked_at: null,
          }),
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("skill-row-live")).toBeInTheDocument();
    expect(screen.getByTestId("skill-row-stale")).toBeInTheDocument();
    expect(screen.getByTestId("skill-row-fresh")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("filter-alive"));
    expect(screen.getByTestId("skill-row-live")).toBeInTheDocument();
    expect(screen.queryByTestId("skill-row-stale")).not.toBeInTheDocument();
    expect(screen.queryByTestId("skill-row-fresh")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("filter-dead"));
    expect(screen.queryByTestId("skill-row-live")).not.toBeInTheDocument();
    expect(screen.getByTestId("skill-row-stale")).toBeInTheDocument();
  });

  it("search filters by name + description", async () => {
    mockFetch(
      resp({
        skills: [
          skill({ name: "alpha", description: "Helps with the foo task" }),
          skill({ name: "beta", description: "About the bar concept" }),
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("skill-row-alpha")).toBeInTheDocument();
    const search = screen.getByTestId("skills-search");
    fireEvent.change(search, { target: { value: "bar" } });
    expect(screen.queryByTestId("skill-row-alpha")).not.toBeInTheDocument();
    expect(screen.getByTestId("skill-row-beta")).toBeInTheDocument();
  });

  it("clicking a row expands the description detail", async () => {
    mockFetch(
      resp({
        skills: [
          skill({
            name: "alpha",
            description: "The alpha description.",
            path: ".claude/skills/alpha/SKILL.md",
          }),
        ],
      })
    );
    renderWithQuery();
    const row = await screen.findByTestId("skill-row-alpha");
    fireEvent.click(row);
    const detail = screen.getByTestId("skill-row-alpha-detail");
    expect(within(detail).getByText("The alpha description.")).toBeInTheDocument();
    expect(within(detail).getByText(".claude/skills/alpha/SKILL.md")).toBeInTheDocument();
  });

  it("clicking the same row again collapses the detail", async () => {
    mockFetch(resp({ skills: [skill({ name: "alpha" })] }));
    renderWithQuery();
    const row = await screen.findByTestId("skill-row-alpha");
    fireEvent.click(row);
    expect(screen.getByTestId("skill-row-alpha-detail")).toBeInTheDocument();
    fireEvent.click(row);
    expect(screen.queryByTestId("skill-row-alpha-detail")).not.toBeInTheDocument();
  });

  it("renders the no-ledger banner when ledger is missing", async () => {
    mockFetch(
      resp({
        ledger_present: false,
        skills: [skill({ name: "x", hygiene_status: "no-ledger" })],
      })
    );
    renderWithQuery();
    expect(
      await screen.findByText(/Invocation ledger not yet wired up/i)
    ).toBeInTheDocument();
    const row = screen.getByTestId("skill-row-x");
    expect(within(row).getByText("no-ledger")).toBeInTheDocument();
  });

  it("table sorts by invocation count when header clicked", async () => {
    mockFetch(
      resp({
        skills: [
          skill({ name: "low", invocation_count: 1 }),
          skill({ name: "high", invocation_count: 50 }),
          skill({ name: "mid", invocation_count: 10 }),
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("skill-row-low")).toBeInTheDocument();

    // Default: name asc — high, low, mid.
    const rowsByOrder = () =>
      screen
        .getAllByRole("row")
        .filter((r) => r.getAttribute("data-testid")?.startsWith("skill-row-") &&
                       !r.getAttribute("data-testid")?.endsWith("-detail"))
        .map((r) => r.getAttribute("data-testid"));
    expect(rowsByOrder()[0]).toBe("skill-row-high");

    // Click "Invocations" — defaults to desc for numeric col.
    fireEvent.click(screen.getByText("Invocations"));
    expect(rowsByOrder()[0]).toBe("skill-row-high"); // 50 first
    expect(rowsByOrder()[1]).toBe("skill-row-mid"); // 10 next
    expect(rowsByOrder()[2]).toBe("skill-row-low"); // 1 last
  });

  it("renders an error state on API failure", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ error: "broken" }),
    } as Response);
    renderWithQuery();
    expect(await screen.findByText(/Failed to load skills/i)).toBeInTheDocument();
  });

  it("renders '—' for never-invoked skills + alive count display", async () => {
    mockFetch(
      resp({
        skills: [
          skill({
            name: "never",
            invocation_count: 0,
            recent_invocation_count: 0,
            last_invoked_at: null,
            hygiene_status: "new",
          }),
          skill({
            name: "active",
            invocation_count: 7,
            recent_invocation_count: 3,
          }),
        ],
      })
    );
    renderWithQuery();
    const neverRow = await screen.findByTestId("skill-row-never");
    expect(within(neverRow).getAllByText("—").length).toBeGreaterThan(0);
    const activeRow = screen.getByTestId("skill-row-active");
    expect(within(activeRow).getByText("7")).toBeInTheDocument();
    expect(within(activeRow).getByText(/\(3 recent\)/)).toBeInTheDocument();
  });

  it("renders 'No skills match' when filter+search yield empty", async () => {
    mockFetch(
      resp({
        skills: [skill({ name: "alpha", hygiene_status: "alive" })],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("skill-row-alpha")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("filter-dead"));
    expect(screen.getByText(/No skills match/i)).toBeInTheDocument();
  });
});
