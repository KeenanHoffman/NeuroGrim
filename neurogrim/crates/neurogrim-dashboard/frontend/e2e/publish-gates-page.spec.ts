/**
 * S12-G-6 smoke spec — Publish-gates page renders cleanly.
 *
 * What this catches:
 * - The new route at `/brains/$brainId/publish-gates` resolves
 *   without 404.
 * - The page renders one of three valid states (empty,
 *   schema-corrupt, populated) — we use the AppShell's
 *   `data-testid="publish-gates-page"` wrapper as the deterministic
 *   anchor.
 * - The new "Publish gates" nav link is reachable from any other
 *   page via the AppShell sidebar.
 *
 * What this deliberately does NOT do:
 * - Assert specific gate IDs (those depend on whether NeuroGrim's
 *   own brain has a manifest; the page handles both states).
 * - Click ack buttons — there are no ack buttons in v1 (read-only
 *   page; ack happens via CLI).
 */
import { test, expect } from "@playwright/test";

test("publish-gates page renders cleanly via nav link", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(`console.error: ${msg.text()}`);
    }
  });

  // Land on the host brain's overview, then click the new nav link.
  // Going through the click flow exercises the AppShell nav typing
  // (the "Publish gates" route was added to the typed `to` union).
  await page.goto("/");
  await page.getByRole("link", { name: /publish gates/i }).click();

  // URL settled on the publish-gates route.
  await expect(page).toHaveURL(/\/publish-gates\/?$/);

  // Page wrapper rendered. The wrapper is present in all three
  // render branches (empty / malformed / populated).
  await expect(page.getByTestId("publish-gates-page")).toBeVisible();

  // Page has the expected H1.
  await expect(
    page.getByRole("heading", { name: /publish gates/i, level: 1 }),
  ).toBeVisible();

  // No uncaught errors.
  expect(
    consoleErrors,
    `unexpected browser errors on publish-gates page:\n${consoleErrors.join("\n")}`,
  ).toEqual([]);
});
