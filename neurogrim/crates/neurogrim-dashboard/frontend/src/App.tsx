import { RouterProvider } from "@tanstack/react-router";
import { router } from "@/router";

/**
 * Phase 1.5: top-level app is now just a `RouterProvider`. The
 * route tree, layout (AppShell), and all page wiring live in
 * `router.tsx`. Theme + query-client setup lives one level up in
 * `main.tsx` so they survive any future router-level remount.
 */
export default function App() {
  return <RouterProvider router={router} />;
}
