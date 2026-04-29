import { describe, it, expect } from "vitest";
import { widgetSpanClass } from "./useDashboardLayout";

describe("widgetSpanClass", () => {
  it("maps full → 12-column span", () => {
    expect(widgetSpanClass("full")).toContain("col-span-12");
  });

  it("maps half → 6-column span", () => {
    expect(widgetSpanClass("half")).toContain("col-span-6");
  });

  it("maps third → 4-column span", () => {
    expect(widgetSpanClass("third")).toContain("col-span-4");
  });

  it("maps quarter → 3-column span", () => {
    expect(widgetSpanClass("quarter")).toContain("col-span-3");
  });

  it("falls back to full-width for unknown sizes (forward compat)", () => {
    // A bundle running against a future server that emits a
    // size we don't recognize ("eighth"?) should render
    // legibly rather than zero-width.
    expect(widgetSpanClass("eighth")).toContain("col-span-12");
    expect(widgetSpanClass("")).toContain("col-span-12");
  });
});
