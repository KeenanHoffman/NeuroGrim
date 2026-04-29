import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./index.css";
import App from "./App";
import { HatProvider } from "@/lib/useHat";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Phase 2.1: SSE-driven invalidation does the freshness work;
      // staleTime can be generous so brief tab switches don't
      // refetch unnecessarily.
      staleTime: 5_000,
      retry: 1,
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <HatProvider>
        <App />
      </HatProvider>
    </QueryClientProvider>
  </StrictMode>
);
