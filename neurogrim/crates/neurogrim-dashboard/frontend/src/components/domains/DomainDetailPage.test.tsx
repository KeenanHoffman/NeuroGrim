import { render, screen, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { DomainDetailPage } from "./DomainDetailPage";
import type { DomainDetailResponse } from "@bindings/DomainDetailResponse";
import { makeTestRouter, RouterProvider } from "@/test/router-helper";
import { HatProvider } from "@/lib/useHat";
import { BrainProvider } from "@/lib/useBrain";

const detail = (
  overrides: Partial<DomainDetailResponse> = {}
): DomainDetailResponse => ({
  name: "test-health",
  display_name: "Test Health",
  weight: 0.35,
  raw_score: 80,
  effective_score: 78,
  confidence: 95,
  trajectory_class: "stable",
  trajectory_velocity: 0,
  trajectory_samples: 5,
  sensor_intent: null,
  findings: [],
  history: [],
  cmdb_path: "/path/to/.claude/test-health-cmdb.json",
  last_updated: "2026-04-25T10:00:00Z",
  ...overrides,
});

function mockFetch(payload: DomainDetailResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
  } as Response);
}

function renderWithQuery(name = "test-health") {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const router = makeTestRouter(
    <BrainProvider brainId="test-brain">
      <DomainDetailPage name={name} />
    </BrainProvider>
  );
  return render(
    <QueryClientProvider client={qc}>
      <HatProvider>
        <RouterProvider router={router} />
      </HatProvider>
    </QueryClientProvider>
  );
}

describe("DomainDetailPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/domains/test-health");
  });

  it("renders the header with name + display name", async () => {
    mockFetch(detail());
    renderWithQuery();
    expect(await screen.findByText("Test Health")).toBeInTheDocument();
    expect(screen.getByText("test-health")).toBeInTheDocument();
  });

  it("shows 'advisory' for weight-0 domains, weight badge for weighted", async () => {
    mockFetch(detail({ weight: 0 }));
    const { unmount } = renderWithQuery();
    expect(await screen.findByText("advisory")).toBeInTheDocument();
    unmount();

    mockFetch(detail({ weight: 0.4 }));
    renderWithQuery();
    expect(await screen.findByText(/weight 0.40/)).toBeInTheDocument();
  });

  it("renders the four stat tiles", async () => {
    mockFetch(detail({ effective_score: 80, raw_score: 90, confidence: 75 }));
    renderWithQuery();
    expect(await screen.findByText("Effective")).toBeInTheDocument();
    expect(screen.getByText("80")).toBeInTheDocument();
    expect(screen.getByText("Raw")).toBeInTheDocument();
    expect(screen.getByText("90")).toBeInTheDocument();
    expect(screen.getByText("Confidence")).toBeInTheDocument();
    expect(screen.getByText("75%")).toBeInTheDocument();
    expect(screen.getByText("Trajectory")).toBeInTheDocument();
  });

  it("shows the sensor authoring intent when present", async () => {
    mockFetch(
      detail({
        sensor_intent: "Sensor will report uncovered modules from cargo-tarpaulin output.",
      })
    );
    renderWithQuery();
    expect(
      await screen.findByText(/uncovered modules from cargo-tarpaulin/)
    ).toBeInTheDocument();
    expect(screen.getByText(/Sensor authoring intent/i)).toBeInTheDocument();
  });

  it("hides the sensor-intent block when null", async () => {
    mockFetch(detail({ sensor_intent: null }));
    renderWithQuery();
    expect(await screen.findByText("Test Health")).toBeInTheDocument();
    expect(screen.queryByText(/Sensor authoring intent/i)).not.toBeInTheDocument();
  });

  it("renders the empty-state for findings when none present", async () => {
    mockFetch(detail({ findings: [] }));
    renderWithQuery();
    expect(
      await screen.findByText(/No findings/i)
    ).toBeInTheDocument();
  });

  it("renders one row per finding with status badge + points", async () => {
    mockFetch(
      detail({
        findings: [
          {
            name: "uncovered_modules",
            status: "warn",
            points: -3,
            detail: "tests/util.rs has 0 coverage",
          },
          {
            name: "coverage_target",
            status: "pass",
            points: 0,
            detail: null,
          },
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByText("uncovered_modules")).toBeInTheDocument();
    expect(screen.getByText("coverage_target")).toBeInTheDocument();
    expect(screen.getByText("warn")).toBeInTheDocument();
    expect(screen.getByText("pass")).toBeInTheDocument();
    expect(screen.getByText("-3")).toBeInTheDocument();
    expect(screen.getByText("0")).toBeInTheDocument();
  });

  it("renders empty-state for history when empty", async () => {
    mockFetch(detail({ history: [] }));
    renderWithQuery();
    expect(await screen.findByText(/No history yet/i)).toBeInTheDocument();
    expect(
      screen.getByText("neurogrim score", { exact: false })
    ).toBeInTheDocument();
  });

  it("renders an error state with friendly message when API 404s", async () => {
    mockFetch(detail(), 404);
    renderWithQuery("nonexistent");
    expect(
      await screen.findByText(/Failed to load domain/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Domain 'nonexistent' not found/i)
    ).toBeInTheDocument();
  });

  it("color-codes positive vs negative finding points", async () => {
    mockFetch(
      detail({
        findings: [
          { name: "good", status: "pass", points: 5, detail: null },
          { name: "bad", status: "warn", points: -2, detail: null },
          { name: "info", status: "info", points: 0, detail: null },
        ],
      })
    );
    renderWithQuery();
    expect(await screen.findByText("good")).toBeInTheDocument();
    expect(screen.getByText("+5").className).toMatch(/emerald/);
    expect(screen.getByText("-2").className).toMatch(/red/);
    expect(screen.getByText("0").className).toMatch(/muted-foreground/);
  });

  it("CMDB metadata block shows the path + last_updated", async () => {
    mockFetch(
      detail({
        cmdb_path: "/p/.claude/x-cmdb.json",
        last_updated: "2026-04-25T10:00:00Z",
      })
    );
    renderWithQuery();
    expect(
      await screen.findByText(/\/p\/\.claude\/x-cmdb\.json/)
    ).toBeInTheDocument();
    expect(screen.getByText("2026-04-25T10:00:00Z")).toBeInTheDocument();
  });
});
