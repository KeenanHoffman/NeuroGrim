import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode } from "react";
import React from "react";
import { useDashboardEvents } from "./useDashboardEvents";

/**
 * Minimal fake EventSource. Captures handlers and exposes
 * `emit*` methods for tests to drive the hook through its state
 * transitions deterministically.
 */
class FakeEventSource {
  static last: FakeEventSource | null = null;
  url: string;
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  readyState = 0;
  closeCount = 0;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.last = this;
  }
  close() {
    this.closeCount += 1;
    this.readyState = 2;
  }

  emitOpen() {
    this.readyState = 1;
    this.onopen?.();
  }
  emitMessage(data: string) {
    this.onmessage?.({ data });
  }
  emitError() {
    this.readyState = 2;
    this.onerror?.();
  }
}

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return React.createElement(
    QueryClientProvider,
    { client: qc },
    children
  );
}

describe("useDashboardEvents", () => {
  let originalES: unknown;

  beforeEach(() => {
    vi.useFakeTimers();
    FakeEventSource.last = null;
    originalES = (globalThis as { EventSource?: unknown }).EventSource;
    (globalThis as { EventSource?: unknown }).EventSource = FakeEventSource;
  });

  afterEach(() => {
    vi.useRealTimers();
    (globalThis as { EventSource?: unknown }).EventSource = originalES;
  });

  it("starts in connecting state and transitions to live on open", () => {
    const { result } = renderHook(() => useDashboardEvents(), { wrapper });
    expect(result.current).toBe("connecting");
    act(() => FakeEventSource.last!.emitOpen());
    expect(result.current).toBe("live");
  });

  it("transitions to offline on error, then schedules a reconnect", () => {
    const { result } = renderHook(() => useDashboardEvents(), { wrapper });
    act(() => FakeEventSource.last!.emitOpen());
    expect(result.current).toBe("live");
    const firstSource = FakeEventSource.last!;
    act(() => FakeEventSource.last!.emitError());
    expect(result.current).toBe("offline");
    expect(firstSource.closeCount).toBeGreaterThan(0);
    // The reconnect timer fires at 3s, opening a fresh EventSource.
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(FakeEventSource.last).not.toBe(firstSource);
  });

  it("flips to disabled on the backend's literal 'disabled' sentinel", () => {
    const { result } = renderHook(() => useDashboardEvents(), { wrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() => FakeEventSource.last!.emitMessage('"disabled"'));
    expect(result.current).toBe("disabled");
  });

  it("invalidates relevant queries on registry_changed", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(qc, "invalidateQueries");
    const customWrapper = ({ children }: { children: ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);
    renderHook(() => useDashboardEvents(), { wrapper: customWrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() => FakeEventSource.last!.emitMessage('"registry_changed"'));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["overview"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["domains"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["federation"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["skills"] });
  });

  it("invalidates score-related queries on score_changed", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(qc, "invalidateQueries");
    const customWrapper = ({ children }: { children: ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);
    renderHook(() => useDashboardEvents(), { wrapper: customWrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() =>
      FakeEventSource.last!.emitMessage(
        '{"score_changed":{"domain":"test-health"}}'
      )
    );
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["overview"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["domains"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["domain-detail"] });
  });

  it("invalidates skills query on skill_invoked", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(qc, "invalidateQueries");
    const customWrapper = ({ children }: { children: ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);
    renderHook(() => useDashboardEvents(), { wrapper: customWrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() => FakeEventSource.last!.emitMessage('"skill_invoked"'));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["skills"] });
  });

  it("invalidates dashboard-layout on layout_changed", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(qc, "invalidateQueries");
    const customWrapper = ({ children }: { children: ReactNode }) =>
      React.createElement(QueryClientProvider, { client: qc }, children);
    renderHook(() => useDashboardEvents(), { wrapper: customWrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() => FakeEventSource.last!.emitMessage('"layout_changed"'));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["dashboard-layout"] });
  });

  it("ignores malformed messages without crashing", () => {
    const { result } = renderHook(() => useDashboardEvents(), { wrapper });
    act(() => FakeEventSource.last!.emitOpen());
    act(() => FakeEventSource.last!.emitMessage("not-json"));
    act(() => FakeEventSource.last!.emitMessage('{"unknown":"variant"}'));
    expect(result.current).toBe("live");
  });

  it("closes the EventSource on unmount", () => {
    const { unmount } = renderHook(() => useDashboardEvents(), { wrapper });
    act(() => FakeEventSource.last!.emitOpen());
    const es = FakeEventSource.last!;
    unmount();
    expect(es.closeCount).toBeGreaterThan(0);
  });
});
