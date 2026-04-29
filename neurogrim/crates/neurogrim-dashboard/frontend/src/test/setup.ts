/**
 * Vitest setup — runs once before any test in the suite. Imports
 * `@testing-library/jest-dom` so all tests get its custom matchers
 * (toBeInTheDocument, toHaveClass, toHaveTextContent, etc.) without
 * each file having to import them individually.
 *
 * `afterEach(cleanup)` unmounts any rendered components after each
 * test so DOM state doesn't leak between tests.
 */
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
});
