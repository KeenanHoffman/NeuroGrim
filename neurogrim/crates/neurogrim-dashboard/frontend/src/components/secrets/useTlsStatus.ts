import { useQuery } from "@tanstack/react-query";
import type { TlsStatusResponse } from "@bindings/TlsStatusResponse";

/**
 * S14-S-4.5 v3 — fetch the dashboard's TLS status.
 *
 * Returns: whether HTTPS is bound + the cert fingerprint to pin.
 * Polled every 60s so a `tls-cert rotate` while the page is open
 * surfaces in the banner without a manual reload.
 */
export function useTlsStatus() {
  return useQuery({
    queryKey: ["tls-status"],
    queryFn: async (): Promise<TlsStatusResponse> => {
      const res = await fetch("/api/tls-status");
      if (!res.ok) throw new Error(`/api/tls-status returned ${res.status}`);
      return (await res.json()) as TlsStatusResponse;
    },
    refetchInterval: 60_000,
    staleTime: 30_000,
  });
}

/**
 * localStorage key for the TOFU-pinned cert fingerprint.
 * Per-host scope: a single browser visiting multiple dashboards
 * pins each host independently.
 */
export function fingerprintStorageKey(host: string): string {
  return `neurogrim:tls-fingerprint:${host}`;
}

/**
 * Read the previously-pinned fingerprint for the current host.
 * Returns null when no fingerprint has been pinned yet (first
 * visit) or when localStorage is unavailable (e.g., privacy
 * modes that block it).
 */
export function readPinnedFingerprint(host: string): string | null {
  try {
    return window.localStorage.getItem(fingerprintStorageKey(host));
  } catch {
    return null;
  }
}

/**
 * Pin a fingerprint for the current host. No-op when localStorage
 * is unavailable (the page still functions; subsequent visits just
 * can't TOFU-verify).
 */
export function pinFingerprint(host: string, fingerprint: string): void {
  try {
    window.localStorage.setItem(fingerprintStorageKey(host), fingerprint);
  } catch {
    // localStorage unavailable — silently degrade.
  }
}

/**
 * Clear the pinned fingerprint. Useful after a deliberate cert
 * rotation when the operator knows the new fingerprint should
 * replace the old.
 */
export function clearPinnedFingerprint(host: string): void {
  try {
    window.localStorage.removeItem(fingerprintStorageKey(host));
  } catch {
    // ignore
  }
}

export type FingerprintCheck =
  | { kind: "match" }
  | { kind: "first-visit"; fingerprint: string }
  | { kind: "mismatch"; pinned: string; current: string }
  | { kind: "no-server-fingerprint" };

/**
 * TOFU comparison logic. The server's fingerprint is the
 * authoritative current value (read from disk on each request);
 * the localStorage pin is what we trusted on first visit.
 *
 * - **first-visit**: no pin yet — caller should pin + show a
 *   one-time "trusted on first use" hint.
 * - **match**: pin matches the server's fingerprint — silent.
 * - **mismatch**: pin and current differ — surface a warning.
 *   Either the operator rotated the cert (run
 *   `neurogrim secrets tls-cert rotate` then accept the new one
 *   in the browser; clear the pin via `clearPinnedFingerprint`)
 *   OR an attacker swapped the cert (rare on loopback but
 *   possible if the host is compromised).
 * - **no-server-fingerprint**: the server has no cert file —
 *   shouldn't happen if HTTPS is bound, but defensive.
 */
export function compareFingerprint(
  serverFingerprint: string | null | undefined,
  pinnedFingerprint: string | null,
): FingerprintCheck {
  if (!serverFingerprint) {
    return { kind: "no-server-fingerprint" };
  }
  if (!pinnedFingerprint) {
    return { kind: "first-visit", fingerprint: serverFingerprint };
  }
  if (pinnedFingerprint === serverFingerprint) {
    return { kind: "match" };
  }
  return {
    kind: "mismatch",
    pinned: pinnedFingerprint,
    current: serverFingerprint,
  };
}

/**
 * Compute the HTTPS URL that corresponds to the current page,
 * given the HTTPS port from `tls-status`. Preserves path + hash
 * + search so the banner click lands on the same Secrets page
 * over HTTPS.
 */
export function httpsUrlForCurrentPage(httpsPort: number): string {
  // Use the current host (without port) + the HTTPS port. We
  // explicitly DON'T trust window.location.host because it
  // includes the wrong port. window.location.hostname gives the
  // pure hostname.
  const url = new URL(window.location.href);
  url.protocol = "https:";
  url.port = String(httpsPort);
  return url.toString();
}

/**
 * Is the current page already loaded over HTTPS? Used to decide
 * whether to show the "switch to HTTPS" banner at all.
 */
export function isCurrentPageHttps(): boolean {
  try {
    return window.location.protocol === "https:";
  } catch {
    return false;
  }
}
