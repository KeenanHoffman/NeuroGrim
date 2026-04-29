import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { ReactNode } from "react";
import { HatProvider, useHat, hatToQuery, DEFAULT_HAT } from "./useHat";

function wrapper({ children }: { children: ReactNode }) {
  return <HatProvider>{children}</HatProvider>;
}

describe("useHat", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("defaults to DEFAULT_HAT when no preference stored", () => {
    const { result } = renderHook(() => useHat(), { wrapper });
    expect(result.current.hat).toBe(DEFAULT_HAT);
  });

  it("reads previously stored hat from localStorage", () => {
    window.localStorage.setItem("neurogrim:hat", "engineer");
    const { result } = renderHook(() => useHat(), { wrapper });
    expect(result.current.hat).toBe("engineer");
  });

  it("setHat updates state and persists to localStorage", () => {
    const { result } = renderHook(() => useHat(), { wrapper });
    act(() => result.current.setHat("reviewer"));
    expect(result.current.hat).toBe("reviewer");
    expect(window.localStorage.getItem("neurogrim:hat")).toBe("reviewer");
  });

  it("throws when used without a HatProvider", () => {
    // renderHook without wrapper — useHat should throw.
    expect(() => renderHook(() => useHat())).toThrow(/HatProvider/);
  });
});

describe("hatToQuery", () => {
  it("returns null for default hat", () => {
    expect(hatToQuery(DEFAULT_HAT)).toBeNull();
    expect(hatToQuery("default")).toBeNull();
    expect(hatToQuery("")).toBeNull();
  });

  it("returns the hat id for explicit hats", () => {
    expect(hatToQuery("engineer")).toBe("engineer");
    expect(hatToQuery("reviewer")).toBe("reviewer");
  });
});
