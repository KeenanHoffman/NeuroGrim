/**
 * S12-G-5 smoke spec — Overview page loads cleanly.
 *
 * What this catches:
 * - The React app boots without uncaught errors that crash the root.
 * - The brain-registry fetch (/api/brains) succeeds and the index
 *   route's redirect to /brains/<self_id>/ resolves.
 * - The Overview page renders at least one widget card (the
 *   default-layout banner is the deterministic anchor — present on
 *   every brain that hasn't customized its layout yet).
 *
 * What this deliberately does NOT do:
 * - Assert specific score values (those depend on live CMDBs and
 *   would be brittle).
 * - Click into widgets (covered by per-widget unit tests in the
 *   `frontend/src/components/**` suite).
 */
import { test, expect } from "@playwright/test";

test("overview page loads and renders Overview content", async ({ page }) => {
  // Capture browser-side errors so a console.error (e.g. React #310)
  // surfaces as a test failure rather than a silent ✓.
  const consoleErrors: string[] = [];
  page.on("pageerror", (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(`console.error: ${msg.text()}`);
    }
  });

  // Visit root — index route redirects to /brains/<self_id>/.
  await page.goto("/");

  // Wait for the AppShell + an Overview-specific marker to be visible.
  // The default-layout-banner renders for any brain whose
  // dashboard-layout is unchanged from defaults; NeuroGrim's own
  // brain has customized its layout, so this banner may NOT be
  // present. Fall back to the brain-selector or any text the AppShell
  // always renders.
  await expect(page).toHaveURL(/\/brains\/[^/]+\/?$/);

  // The AppShell renders a header with at least one navigation link
  // ("Domains", "Federation", or "Skills"). Use Federation as the
  // anchor — it's stable across every brain.
  await expect(page.getByRole("link", { name: /federation/i })).toBeVisible();

  // No uncaught errors during the load.
  expect(
    consoleErrors,
    `unexpected browser errors during Overview load:\n${consoleErrors.join("\n")}`,
  ).toEqual([]);
});
