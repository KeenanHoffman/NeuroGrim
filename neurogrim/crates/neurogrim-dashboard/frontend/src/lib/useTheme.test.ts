import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useTheme } from "./useTheme";

describe("useTheme", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  afterEach(() => {
    document.documentElement.classList.remove("dark");
  });

  it("defaults to dark when no preference stored and OS is dark", () => {
    const { result } = renderHook(() => useTheme());
    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("reads previously stored preference from localStorage", () => {
    window.localStorage.setItem("neurogrim:theme", "light");
    const { result } = renderHook(() => useTheme());
    expect(result.current.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("toggleTheme flips and persists", () => {
    const { result } = renderHook(() => useTheme());
    expect(result.current.theme).toBe("dark");
    act(() => result.current.toggleTheme());
    expect(result.current.theme).toBe("light");
    expect(window.localStorage.getItem("neurogrim:theme")).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    act(() => result.current.toggleTheme());
    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("setTheme overrides explicitly", () => {
    const { result } = renderHook(() => useTheme());
    act(() => result.current.setTheme("light"));
    expect(result.current.theme).toBe("light");
    expect(window.localStorage.getItem("neurogrim:theme")).toBe("light");
  });
});
