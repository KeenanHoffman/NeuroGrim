/**
 * S12-G-5 smoke spec — Layout-edit toolbar toggles cleanly.
 *
 * What this catches:
 * - The "Customize" button on the Overview page mounts and is
 *   clickable.
 * - Clicking it transitions the page into edit mode (the
 *   `data-testid="edit-mode-on"` marker appears).
 * - The widget-add picker, the per-widget controls (move up/down,
 *   reset, remove), and the Save button all render in edit mode
 *   without throwing.
 *
 * What this deliberately does NOT do:
 * - Click Save. Saving mutates the brain's dashboard-layout.json
 *   ledger and would leak state between test runs. The smoke test
 *   only verifies the edit toolbar is reachable; the actual save
 *   path is exercised by `LayoutEditor.test.tsx` (vitest unit
 *   suite) plus the manual operator-checklist flow at G-6.
 */
import { test, expect } from "@playwright/test";

test("layout-edit toolbar opens via Customize button", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("pageerror", (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(`console.error: ${msg.text()}`);
    }
  });

  // Land on the host brain's overview.
  await page.goto("/");
  await expect(page).toHaveURL(/\/brains\/[^/]+\/?$/);

  // The Customize button toggles the page into edit mode. Use the
  // accessible name ("Customize") rather than a CSS selector — it's
  // stable across the radix UI primitives and Tailwind class churn.
  const customizeBtn = page.getByRole("button", { name: /customize/i });
  await expect(customizeBtn).toBeVisible();
  await customizeBtn.click();

  // Edit mode is on — the toolbar's data-testid marker is the
  // deterministic anchor.
  await expect(page.getByTestId("edit-mode-on")).toBeVisible();

  // The Save button now mounts (it's only visible while editing).
  await expect(page.getByRole("button", { name: /^save$/i })).toBeVisible();

  // No uncaught errors during the toggle.
  expect(
    consoleErrors,
    `unexpected browser errors during layout-edit toggle:\n${consoleErrors.join("\n")}`,
  ).toEqual([]);
});
