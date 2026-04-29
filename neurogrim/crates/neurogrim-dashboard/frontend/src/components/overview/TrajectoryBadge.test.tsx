import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { TrajectoryBadge } from "./TrajectoryBadge";

describe("TrajectoryBadge", () => {
  it.each([
    ["improving", "Improving"],
    ["degrading", "Degrading"],
    ["volatile", "Volatile"],
    ["stable", "Stable"],
  ])("renders the human label for '%s'", (cls, label) => {
    render(<TrajectoryBadge trajectoryClass={cls} velocity={0} samples={5} />);
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it.each([
    ["no-data"],
    ["unknown-future-classification"],
  ])(
    "treats '%s' as insufficient-data fallback",
    (cls) => {
      render(<TrajectoryBadge trajectoryClass={cls} velocity={0} samples={2} />);
      expect(screen.getByText("Insufficient data")).toBeInTheDocument();
    }
  );

  it("hides velocity readout when |velocity| < 0.05", () => {
    render(<TrajectoryBadge trajectoryClass="stable" velocity={0.02} samples={10} />);
    // The "10 samples" text is shown, but no /period suffix.
    expect(screen.getByText(/10 samples/)).toBeInTheDocument();
    expect(screen.queryByText(/period/)).not.toBeInTheDocument();
  });

  it("shows signed velocity when present", () => {
    render(<TrajectoryBadge trajectoryClass="improving" velocity={3.4} samples={8} />);
    expect(screen.getByText(/\+3\.4\/period/)).toBeInTheDocument();
  });

  it("shows negative velocity for degrading", () => {
    render(<TrajectoryBadge trajectoryClass="degrading" velocity={-2.1} samples={6} />);
    expect(screen.getByText(/-2\.1\/period/)).toBeInTheDocument();
  });

  it("uses the singular 'sample' for 1 sample", () => {
    render(<TrajectoryBadge trajectoryClass="stable" velocity={0} samples={1} />);
    expect(screen.getByText(/^1 sample$/)).toBeInTheDocument();
  });
});
