import { render, screen, fireEvent, within } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FederationPage } from "./FederationPage";
import type { FederationResponse } from "@bindings/FederationResponse";
import type { PeerDto } from "@bindings/PeerDto";

const peer = (overrides: Partial<PeerDto> = {}): PeerDto => ({
  name: "alpha",
  display_name: "Alpha",
  transport: "a2a",
  a2a_endpoint: "http://127.0.0.1:8421/a2a/v1/",
  brain_path: "../alpha",
  weight: 1.0,
  read_only: false,
  enabled: true,
  status: { kind: "alive", message: "" },
  agent_card: {
    id: "alpha-brain",
    name: "Alpha Brain",
    version: "3.3.0",
    interface_version: "1",
    schema_version: "1",
    transport_protocol: "http+sse",
    topology_role: "project",
    topology_parent_id: "ecosystem",
  },
  ...overrides,
});

const fed = (overrides: Partial<FederationResponse> = {}): FederationResponse => ({
  self_brain: {
    label: "Test Project",
    project_root: "/tmp/test",
    version: "3.4.0",
  },
  peers: [],
  registry_schema_version: "2.1",
  ...overrides,
});

function mockFetch(payload: FederationResponse, status = 200) {
  global.fetch = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
  } as Response);
}

function renderWithQuery() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <FederationPage />
    </QueryClientProvider>
  );
}

describe("FederationPage", () => {
  beforeEach(() => {
    window.history.replaceState({}, "", "/federation");
  });

  it("renders the self banner with label, root, and versions", async () => {
    mockFetch(fed());
    renderWithQuery();
    expect(await screen.findByText("Test Project")).toBeInTheDocument();
    expect(screen.getByText("/tmp/test")).toBeInTheDocument();
    expect(screen.getByText(/dashboard 3\.4\.0/)).toBeInTheDocument();
    expect(screen.getByText(/registry schema 2\.1/)).toBeInTheDocument();
  });

  it("shows the empty-state CTA when no peers are declared", async () => {
    mockFetch(fed({ peers: [] }));
    renderWithQuery();
    expect(await screen.findByText(/No federation peers/i)).toBeInTheDocument();
    expect(
      screen.getByText(/neurogrim federation register/i)
    ).toBeInTheDocument();
  });

  it("renders one row per peer with status badge", async () => {
    mockFetch(
      fed({
        peers: [
          peer({ name: "alpha", display_name: "Alpha" }),
          peer({
            name: "bravo",
            display_name: "Bravo",
            status: { kind: "unreachable", message: "timeout after 1500 ms" },
            agent_card: null,
          }),
        ],
      })
    );
    renderWithQuery();
    const aliveRow = await screen.findByTestId("peer-row-alpha");
    expect(within(aliveRow).getByText("Alpha")).toBeInTheDocument();
    expect(within(aliveRow).getByText("alive")).toBeInTheDocument();
    const deadRow = screen.getByTestId("peer-row-bravo");
    expect(within(deadRow).getByText("Bravo")).toBeInTheDocument();
    expect(within(deadRow).getByText("unreachable")).toBeInTheDocument();
  });

  it("renders the SVG topology with one node per peer + the self node", async () => {
    mockFetch(
      fed({
        peers: [peer({ name: "alpha" }), peer({ name: "bravo" })],
      })
    );
    renderWithQuery();
    expect(await screen.findByTestId("federation-topology")).toBeInTheDocument();
    expect(screen.getByTestId("topology-node-alpha")).toBeInTheDocument();
    expect(screen.getByTestId("topology-node-bravo")).toBeInTheDocument();
  });

  it("clicking a peer row reveals the Agent Card detail panel", async () => {
    mockFetch(fed({ peers: [peer({ name: "alpha", display_name: "Alpha" })] }));
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-alpha");
    fireEvent.click(row);
    // The Agent Card detail panel uniquely contains the peer's id
    // ("alpha-brain") — surface that as the selection-visible signal
    // rather than the "Agent Card" string (which also appears in the
    // peers-table card description).
    expect(screen.getByText("alpha-brain")).toBeInTheDocument();
    expect(screen.getByText("http+sse")).toBeInTheDocument();
    expect(screen.getByText("ecosystem")).toBeInTheDocument();
  });

  it("clicking the same row again collapses the detail panel", async () => {
    mockFetch(fed({ peers: [peer({ name: "alpha" })] }));
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-alpha");
    fireEvent.click(row);
    expect(screen.getByText("alpha-brain")).toBeInTheDocument();
    fireEvent.click(row);
    expect(screen.queryByText("alpha-brain")).not.toBeInTheDocument();
  });

  it("clicking a topology node selects the peer", async () => {
    mockFetch(fed({ peers: [peer({ name: "alpha" })] }));
    renderWithQuery();
    const node = await screen.findByTestId("topology-node-alpha");
    fireEvent.click(node);
    expect(screen.getByText("alpha-brain")).toBeInTheDocument();
  });

  it("shows 'read-only' posture when peer is read_only=true", async () => {
    mockFetch(
      fed({
        peers: [peer({ name: "sib", display_name: "Sibling", read_only: true })],
      })
    );
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-sib");
    expect(within(row).getByText("read-only")).toBeInTheDocument();
  });

  it("shows 'contributing' for non-read-only peers", async () => {
    mockFetch(fed({ peers: [peer({ read_only: false })] }));
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-alpha");
    expect(within(row).getByText("contributing")).toBeInTheDocument();
  });

  it("renders subprocess peers with 'unprobed' status", async () => {
    mockFetch(
      fed({
        peers: [
          peer({
            name: "sub",
            display_name: "Sub",
            transport: "subprocess",
            a2a_endpoint: null,
            status: { kind: "unprobed", message: "transport=subprocess (not probed)" },
            agent_card: null,
          }),
        ],
      })
    );
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-sub");
    expect(within(row).getByText("unprobed")).toBeInTheDocument();
    expect(within(row).getByText("subprocess")).toBeInTheDocument();
  });

  it("renders a friendly error state on API failure", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ error: "broken" }),
    } as Response);
    renderWithQuery();
    expect(
      await screen.findByText(/Failed to load federation/i)
    ).toBeInTheDocument();
  });

  it("'disabled' peers render with the disabled status badge", async () => {
    mockFetch(
      fed({
        peers: [
          peer({
            name: "off",
            display_name: "Off",
            enabled: false,
            status: {
              kind: "disabled",
              message: "peer marked enabled=false in registry",
            },
            agent_card: null,
          }),
        ],
      })
    );
    renderWithQuery();
    const row = await screen.findByTestId("peer-row-off");
    expect(within(row).getByText("disabled")).toBeInTheDocument();
  });
});
