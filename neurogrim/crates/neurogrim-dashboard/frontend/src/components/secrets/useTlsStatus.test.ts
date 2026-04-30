/**
 * Tests for the pure helpers in `useTlsStatus.ts` (S14-S-4.5 v3).
 *
 * The `useTlsStatus()` React hook itself is exercised in
 * `SecretsPage.test.tsx`; this file covers the pure functions so
 * the contract is pinned independently of React rendering.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  clearPinnedFingerprint,
  compareFingerprint,
  fingerprintStorageKey,
  httpsUrlForCurrentPage,
  isCurrentPageHttps,
  pinFingerprint,
  readPinnedFingerprint,
} from "./useTlsStatus";

describe("fingerprintStorageKey", () => {
  it("scopes the key per host", () => {
    expect(fingerprintStorageKey("127.0.0.1")).toBe(
      "neurogrim:tls-fingerprint:127.0.0.1",
    );
    expect(fingerprintStorageKey("dashboard.local")).toBe(
      "neurogrim:tls-fingerprint:dashboard.local",
    );
  });
});

describe("pin / read / clear cycle", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("read returns null when nothing pinned", () => {
    expect(readPinnedFingerprint("127.0.0.1")).toBeNull();
  });

  it("pin then read returns the pinned value", () => {
    pinFingerprint("127.0.0.1", "deadbeef".repeat(8));
    expect(readPinnedFingerprint("127.0.0.1")).toBe("deadbeef".repeat(8));
  });

  it("pin overwrites a previously-pinned value", () => {
    pinFingerprint("127.0.0.1", "old");
    pinFingerprint("127.0.0.1", "new");
    expect(readPinnedFingerprint("127.0.0.1")).toBe("new");
  });

  it("clear removes the pin", () => {
    pinFingerprint("127.0.0.1", "x");
    clearPinnedFingerprint("127.0.0.1");
    expect(readPinnedFingerprint("127.0.0.1")).toBeNull();
  });

  it("scopes per host — pinning one doesn't leak to another", () => {
    pinFingerprint("alpha", "fp-a");
    pinFingerprint("beta", "fp-b");
    expect(readPinnedFingerprint("alpha")).toBe("fp-a");
    expect(readPinnedFingerprint("beta")).toBe("fp-b");
  });

  it("read survives localStorage exceptions silently", () => {
    const original = Storage.prototype.getItem;
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readPinnedFingerprint("127.0.0.1")).toBeNull();
    Storage.prototype.getItem = original;
  });
});

describe("compareFingerprint", () => {
  it("first-visit when no pin yet", () => {
    expect(compareFingerprint("aaaa", null)).toEqual({
      kind: "first-visit",
      fingerprint: "aaaa",
    });
  });

  it("match when pin equals server value", () => {
    expect(compareFingerprint("aaaa", "aaaa")).toEqual({ kind: "match" });
  });

  it("mismatch when pin differs from server", () => {
    expect(compareFingerprint("server-fp", "pinned-fp")).toEqual({
      kind: "mismatch",
      pinned: "pinned-fp",
      current: "server-fp",
    });
  });

  it("no-server-fingerprint when server returns null", () => {
    expect(compareFingerprint(null, "any")).toEqual({
      kind: "no-server-fingerprint",
    });
  });

  it("no-server-fingerprint when server returns undefined", () => {
    expect(compareFingerprint(undefined, "any")).toEqual({
      kind: "no-server-fingerprint",
    });
  });
});

describe("httpsUrlForCurrentPage", () => {
  beforeEach(() => {
    // jsdom defaults to localhost:3000 unless we override per-test.
    Object.defineProperty(window, "location", {
      writable: true,
      value: new URL("http://127.0.0.1:8420/brains/test/secrets?foo=1#hash"),
    });
  });

  it("preserves path + hash + search, swaps protocol + port", () => {
    const url = httpsUrlForCurrentPage(8421);
    const parsed = new URL(url);
    expect(parsed.protocol).toBe("https:");
    expect(parsed.port).toBe("8421");
    expect(parsed.pathname).toBe("/brains/test/secrets");
    expect(parsed.search).toBe("?foo=1");
    expect(parsed.hash).toBe("#hash");
  });

  it("preserves the hostname", () => {
    const url = httpsUrlForCurrentPage(8421);
    expect(new URL(url).hostname).toBe("127.0.0.1");
  });
});

describe("isCurrentPageHttps", () => {
  it("returns true on HTTPS URLs", () => {
    Object.defineProperty(window, "location", {
      writable: true,
      value: new URL("https://localhost:8421/x"),
    });
    expect(isCurrentPageHttps()).toBe(true);
  });

  it("returns false on HTTP URLs", () => {
    Object.defineProperty(window, "location", {
      writable: true,
      value: new URL("http://localhost:8420/x"),
    });
    expect(isCurrentPageHttps()).toBe(false);
  });
});
