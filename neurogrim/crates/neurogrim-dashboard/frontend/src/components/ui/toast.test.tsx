import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ToastProvider, useToast } from "./toast";

/**
 * Test harness — wraps `useToast` in a Provider and exposes the
 * dispatch via a button so tests can fire toasts via `fireEvent`.
 * The button takes the next dispatch's params from a ref so each
 * test can stage a different toast without re-rendering.
 */
function HarnessButton({
  spec,
}: {
  spec: { severity: "info" | "success" | "warning" | "error"; title: string; body?: string };
}) {
  const { addToast } = useToast();
  return (
    <button
      data-testid="dispatch"
      onClick={() => addToast(spec.severity, spec.title, spec.body)}
    >
      dispatch
    </button>
  );
}

function renderHarness(
  spec: { severity: "info" | "success" | "warning" | "error"; title: string; body?: string },
) {
  return render(
    <ToastProvider>
      <HarnessButton spec={spec} />
    </ToastProvider>,
  );
}

describe("ToastProvider / useToast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("addToast renders a toast with title + severity", () => {
    renderHarness({
      severity: "error",
      title: 'Peer "alpha" failed',
      body: "port-conflict: port 8421 already bound",
    });
    fireEvent.click(screen.getByTestId("dispatch"));
    const toaster = screen.getByTestId("toaster");
    expect(toaster).toBeInTheDocument();
    expect(toaster.textContent).toContain('Peer "alpha" failed');
    expect(toaster.textContent).toContain("port-conflict");
    // Severity surfaces as a data attribute for styling + tests.
    const toast = toaster.querySelector("[data-severity]") as HTMLElement;
    expect(toast.getAttribute("data-severity")).toBe("error");
  });

  it("auto-dismisses the toast after 8 seconds", () => {
    renderHarness({ severity: "info", title: "Hello" });
    fireEvent.click(screen.getByTestId("dispatch"));
    expect(screen.queryByTestId("toaster")).toBeInTheDocument();
    // Step forward just before the dismiss boundary — still visible.
    act(() => {
      vi.advanceTimersByTime(7999);
    });
    expect(screen.queryByTestId("toaster")).toBeInTheDocument();
    // Cross the boundary — toast is gone, and so is the empty
    // toaster container (Toaster returns null when empty).
    act(() => {
      vi.advanceTimersByTime(2);
    });
    expect(screen.queryByTestId("toaster")).toBeNull();
  });

  it("clicking the toast dismisses it before auto-timeout", () => {
    renderHarness({ severity: "info", title: "click me" });
    fireEvent.click(screen.getByTestId("dispatch"));
    const toaster = screen.getByTestId("toaster");
    const toast = toaster.querySelector("[data-severity]") as HTMLElement;
    fireEvent.click(toast);
    expect(screen.queryByTestId("toaster")).toBeNull();
  });

  it("dismiss button stops propagation and dismisses without re-dismiss", () => {
    renderHarness({ severity: "info", title: "via x" });
    fireEvent.click(screen.getByTestId("dispatch"));
    const toaster = screen.getByTestId("toaster");
    const dismissBtn = toaster.querySelector(
      '[data-testid^="toast-dismiss-"]',
    ) as HTMLElement;
    expect(dismissBtn).toBeInTheDocument();
    fireEvent.click(dismissBtn);
    expect(screen.queryByTestId("toaster")).toBeNull();
  });

  it("ESC key on a focused toast dismisses it", () => {
    renderHarness({ severity: "info", title: "esc-me" });
    fireEvent.click(screen.getByTestId("dispatch"));
    const toaster = screen.getByTestId("toaster");
    const toast = toaster.querySelector("[data-severity]") as HTMLElement;
    fireEvent.keyDown(toast, { key: "Escape" });
    expect(screen.queryByTestId("toaster")).toBeNull();
  });

  it("caps the visible stack at 3, dropping the oldest", () => {
    function MultiButton() {
      const { addToast } = useToast();
      return (
        <button
          data-testid="dispatch-many"
          onClick={() => {
            addToast("info", "first");
            addToast("info", "second");
            addToast("info", "third");
            addToast("info", "fourth");
          }}
        >
          dispatch
        </button>
      );
    }
    render(
      <ToastProvider>
        <MultiButton />
      </ToastProvider>,
    );
    fireEvent.click(screen.getByTestId("dispatch-many"));
    const toaster = screen.getByTestId("toaster");
    // Oldest ("first") got dropped; the 3 newest survive.
    expect(toaster.textContent).not.toContain("first");
    expect(toaster.textContent).toContain("second");
    expect(toaster.textContent).toContain("third");
    expect(toaster.textContent).toContain("fourth");
    // Confirm exactly 3 toasts are rendered.
    expect(toaster.querySelectorAll("[data-severity]").length).toBe(3);
  });

  it("renders no DOM when no toasts are active", () => {
    render(
      <ToastProvider>
        <HarnessButton spec={{ severity: "info", title: "x" }} />
      </ToastProvider>,
    );
    // Toaster returns null when toasts.length === 0.
    expect(screen.queryByTestId("toaster")).toBeNull();
  });

  it("useToast() throws when called outside a provider", () => {
    function Naughty() {
      useToast();
      return null;
    }
    // React swallows the throw inside a render and surfaces it via
    // the test runner; capture via a console.error spy to keep the
    // test output clean.
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Naughty />)).toThrow(
      /useToast\(\) must be called within a <ToastProvider>/,
    );
    spy.mockRestore();
  });
});
