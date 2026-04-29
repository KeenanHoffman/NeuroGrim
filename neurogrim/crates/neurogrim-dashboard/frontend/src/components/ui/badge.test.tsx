import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Badge } from "./badge";

describe("Badge", () => {
  it("renders children text", () => {
    render(<Badge>hello</Badge>);
    expect(screen.getByText("hello")).toBeInTheDocument();
  });

  it("applies the default variant when none specified", () => {
    render(<Badge data-testid="b">x</Badge>);
    const el = screen.getByTestId("b");
    // Default variant uses the primary color tokens.
    expect(el.className).toMatch(/bg-primary/);
  });

  it.each([
    ["success", /bg-emerald/],
    ["warning", /bg-amber/],
    ["danger", /bg-red/],
    ["secondary", /bg-secondary/],
    ["destructive", /bg-destructive/],
    ["outline", /text-foreground/],
  ] as const)(
    "applies the %s variant's color tokens",
    (variant, expectedClassRe) => {
      render(<Badge variant={variant} data-testid="b">x</Badge>);
      expect(screen.getByTestId("b").className).toMatch(expectedClassRe);
    }
  );

  it("merges custom className with variant classes", () => {
    render(
      <Badge className="my-custom" data-testid="b">x</Badge>
    );
    expect(screen.getByTestId("b").className).toMatch(/my-custom/);
  });
});
