import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { BrainIdentityCard } from "./BrainIdentityCard";
import type { OverviewResponse } from "@bindings/OverviewResponse";

const overview = (
  partial: Partial<OverviewResponse> = {}
): OverviewResponse => ({
  project_label: "Test Brain",
  project_root: "/path/to/brain",
  domain_count: 8,
  weighted_count: 3,
  advisory_count: 5,
  score: 78,
  confidence: 82,
  trajectory_class: "stable",
  trajectory_velocity: 0,
  trajectory_samples: 5,
  top_recommendations: [],
  strongest_signals: [],
  federation_peer_count: 0,
  ...partial,
});

describe("BrainIdentityCard", () => {
  it("shows the project label and root path", () => {
    render(<BrainIdentityCard overview={overview()} />);
    expect(screen.getByText("Test Brain")).toBeInTheDocument();
    expect(screen.getByText("/path/to/brain")).toBeInTheDocument();
  });

  it("shows weighted + advisory badges when both > 0", () => {
    render(
      <BrainIdentityCard
        overview={overview({ weighted_count: 3, advisory_count: 5 })}
      />
    );
    expect(screen.getByText("3 weighted")).toBeInTheDocument();
    expect(screen.getByText("5 advisory")).toBeInTheDocument();
  });

  it("hides weighted badge when none weighted (all-advisory Brain)", () => {
    render(
      <BrainIdentityCard
        overview={overview({ weighted_count: 0, advisory_count: 17 })}
      />
    );
    expect(screen.queryByText(/weighted$/)).not.toBeInTheDocument();
    expect(screen.getByText("17 advisory")).toBeInTheDocument();
  });

  it("uses singular 'domain' / 'peer' for count of 1", () => {
    render(
      <BrainIdentityCard
        overview={overview({
          domain_count: 1,
          weighted_count: 0,
          advisory_count: 1,
          federation_peer_count: 1,
        })}
      />
    );
    expect(screen.getByText("1 domain")).toBeInTheDocument();
    expect(screen.getByText("1 federation peer")).toBeInTheDocument();
  });

  it("hides federation badge when no peers", () => {
    render(
      <BrainIdentityCard overview={overview({ federation_peer_count: 0 })} />
    );
    expect(screen.queryByText(/federation peer/)).not.toBeInTheDocument();
  });
});
