import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useRoute, matchDomainDetail } from "./useRoute";

describe("useRoute", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/");
  });

  it("returns the current pathname", () => {
    window.history.replaceState({}, "", "/domains");
    const { result } = renderHook(() => useRoute());
    expect(result.current.pathname).toBe("/domains");
  });

  it("navigate() updates pathname + browser URL", () => {
    const { result } = renderHook(() => useRoute());
    expect(result.current.pathname).toBe("/");
    act(() => {
      result.current.navigate("/domains/test-health");
    });
    expect(result.current.pathname).toBe("/domains/test-health");
    expect(window.location.pathname).toBe("/domains/test-health");
  });

  it("popstate updates pathname (browser back/forward)", () => {
    const { result } = renderHook(() => useRoute());
    act(() => {
      result.current.navigate("/domains");
    });
    act(() => {
      window.history.replaceState({}, "", "/");
      window.dispatchEvent(new PopStateEvent("popstate"));
    });
    expect(result.current.pathname).toBe("/");
  });

  it("navigate() is a no-op when target equals current pathname", () => {
    window.history.replaceState({}, "", "/domains");
    const { result } = renderHook(() => useRoute());
    const before = window.history.length;
    act(() => {
      result.current.navigate("/domains");
    });
    // pushState would increment history.length; no-op leaves it.
    expect(window.history.length).toBe(before);
  });
});

describe("matchDomainDetail", () => {
  it("matches /domains/<name> and returns the name", () => {
    expect(matchDomainDetail("/domains/test-health")).toEqual({
      name: "test-health",
    });
  });

  it("decodes percent-encoded names", () => {
    expect(matchDomainDetail("/domains/with%20space")).toEqual({
      name: "with space",
    });
  });

  it("returns null for non-detail paths", () => {
    expect(matchDomainDetail("/")).toBeNull();
    expect(matchDomainDetail("/domains")).toBeNull();
    expect(matchDomainDetail("/domains/foo/bar")).toBeNull();
    expect(matchDomainDetail("/other")).toBeNull();
  });
});
