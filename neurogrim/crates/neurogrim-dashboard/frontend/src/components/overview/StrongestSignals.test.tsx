import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { StrongestSignals } from "./StrongestSignals";
import type { DomainSignalDto } from "@bindings/DomainSignalDto";

const signal = (overrides: Partial<DomainSignalDto> = {}): DomainSignalDto => ({
  name: "test-domain",
  display_name: "Test Domain",
  effective_score: 80,
  confidence: 90,
  weight: 0,
  ...overrides,
});

describe("StrongestSignals", () => {
  it("renders the empty-state message when no signals", () => {
    render(<StrongestSignals signals={[]} />);
    expect(screen.getByText(/No domains scored yet/i)).toBeInTheDocument();
  });

  it("renders a row for each signal with display name + score", () => {
    const signals = [
      signal({ name: "a", display_name: "Alpha", effective_score: 95 }),
      signal({ name: "b", display_name: "Beta", effective_score: 70 }),
      signal({ name: "c", display_name: "Gamma", effective_score: 40 }),
    ];
    render(<StrongestSignals signals={signals} />);
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Beta")).toBeInTheDocument();
    expect(screen.getByText("Gamma")).toBeInTheDocument();
    expect(screen.getByText("95")).toBeInTheDocument();
    expect(screen.getByText("70")).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
  });

  it("colors high scores green, mid amber, low red", () => {
    const signals = [
      signal({ name: "hi", display_name: "Hi", effective_score: 90 }),
      signal({ name: "mid", display_name: "Mid", effective_score: 60 }),
      signal({ name: "lo", display_name: "Lo", effective_score: 30 }),
    ];
    render(<StrongestSignals signals={signals} />);
    expect(screen.getByText("90").className).toMatch(/emerald/);
    expect(screen.getByText("60").className).toMatch(/amber/);
    expect(screen.getByText("30").className).toMatch(/red/);
  });

  it("shows confidence percent for each signal", () => {
    const signals = [signal({ confidence: 75 })];
    render(<StrongestSignals signals={signals} />);
    expect(screen.getByText(/conf 75%/)).toBeInTheDocument();
  });
});
