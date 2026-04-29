/**
 * S12-G-5 smoke spec — Federation page renders without throwing.
 *
 * Why this spec exists (per S12 epic): the v3.5 polish cycle hit a
 * React #310 crash in the federation page that unit tests didn't
 * catch because the failure was a real-DOM render error, not a
 * component-logic bug. This spec is the canary for that class of
 * regression — visit the federation page, confirm the topology
 * marker renders, confirm no uncaught browser errors.
 *
 * What this catches:
 * - React render-time exceptions in FederationPage / Topology.
 * - Schema drift between the /api/federation response and the
 *   frontend's typed bindings (`@bindings/...`).
 * - Network-layer errors (e.g. dashboard server returns 500 on
 *   the federation endpoint).
 */
import { test, expect } from "@playwright/test";

test("federation page renders topology without errors", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(`console.error: ${msg.text()}`);
    }
  });

  // Land on the host brain's overview, then navigate to its
  // federation page via the in-app link. Going through the click
  // flow exercises the same client-side router code adopters hit.
  await page.goto("/");
  await page.getByRole("link", { name: /federation/i }).click();

  // URL settled on the federation route.
  await expect(page).toHaveURL(/\/federation\/?$/);

  // The topology marker renders. This is the data-testid on the
  // federation page's main container — present whether the brain
  // has 0 peers or N peers (empty-state still renders the wrapper).
  await expect(page.getByTestId("federation-topology")).toBeVisible();

  // No uncaught errors. This is the assertion that would have caught
  // the v3.5 React #310 crash.
  expect(
    consoleErrors,
    `unexpected browser errors on federation page:\n${consoleErrors.join("\n")}`,
  ).toEqual([]);
});
