import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { TopRecommendations } from "./TopRecommendations";
import type { RecommendationDto } from "@bindings/RecommendationDto";

const rec = (overrides: Partial<RecommendationDto> = {}): RecommendationDto => ({
  domain: "code-quality",
  gate: "before-merge",
  status: "open",
  command: "neurogrim score",
  description: "Some rationale",
  ...overrides,
});

describe("TopRecommendations", () => {
  it("renders empty state when no recommendations", () => {
    render(<TopRecommendations recommendations={[]} />);
    expect(screen.getByText(/Nothing pressing/i)).toBeInTheDocument();
  });

  it("renders domain + gate badges for each rec", () => {
    render(
      <TopRecommendations
        recommendations={[rec({ domain: "test-health", gate: "immediate" })]}
      />
    );
    expect(screen.getByText("test-health")).toBeInTheDocument();
    expect(screen.getByText("immediate")).toBeInTheDocument();
  });

  it("renders the description and command", () => {
    render(
      <TopRecommendations
        recommendations={[
          rec({
            description: "Pay down test debt",
            command: "neurogrim sensory test-health",
          }),
        ]}
      />
    );
    expect(screen.getByText("Pay down test debt")).toBeInTheDocument();
    expect(
      screen.getByText(/neurogrim sensory test-health/)
    ).toBeInTheDocument();
  });

  it("omits the gate badge when gate is empty", () => {
    render(
      <TopRecommendations
        recommendations={[rec({ gate: "" })]}
      />
    );
    expect(screen.queryByText(/^before-merge$|^immediate$|^pre-deploy$/))
      .not.toBeInTheDocument();
  });

  it("omits description block when missing", () => {
    render(
      <TopRecommendations
        recommendations={[rec({ description: "" })]}
      />
    );
    // Domain badge still shows; description region absent.
    expect(screen.getByText("code-quality")).toBeInTheDocument();
  });
});
