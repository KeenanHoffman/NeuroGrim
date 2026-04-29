import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  FileText,
  Info,
  Lock,
  Plus,
  Save,
  Sliders,
  Trash2,
} from "lucide-react";
import type { ConfigFileResponse } from "@bindings/ConfigFileResponse";
import type { CustomPageMutationResponse } from "@bindings/CustomPageMutationResponse";
import type { DashboardPagesConfig } from "@bindings/DashboardPagesConfig";
import type { RegistryResponse } from "@bindings/RegistryResponse";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button-ish";
import { HelpIcon } from "@/components/help/HelpIcon";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * S15-C-5: built-in Settings page (read-only viewers).
 *
 * v1 surfaces three configuration files with read-only viewers:
 *
 * - **Culture** (`<root>/.claude/culture.yaml`) — values + version.
 *   Culture is a CONTRACT, not a setting; the viewer is permanently
 *   read-only with an inline pointer to `neurogrim explain culture`.
 * - **Queue config** (`<root>/.claude/brain/queue-config.yaml`) —
 *   per-topic backend + retention. Read-only in v1; the editor lands
 *   with S13-B-3 (SQLite backend) when adopters can actually
 *   choose between backends.
 * - **Publish gates** (`<root>/.claude/brain/publish-gates.yaml`) —
 *   linked from the existing Publish gates page rather than re-
 *   rendered here. Avoids duplication.
 *
 * **Editors deferred:** the registry editor (S15-C-4, the 8-day
 * load-bearing one) plus per-config editors for secret-refs.yaml
 * (depends on S14-S-6 passphrase entry flow) and queue-config.yaml
 * (depends on S13-B-3) ship in session 2 + later.
 */
export function SettingsPage() {
  // Default to Culture tab — read-only, low-risk, sets the right
  // tone for "read before edit". Registry editor (the load-bearing
  // C-4 surface) is one click away.
  const [tab, setTab] = useState<SettingsTab>("culture");

  return (
    <div className="space-y-6 p-6" data-testid="settings-page">
      <header>
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Read-only viewers for the Brain's configuration files. Editors
          for the registry, secret-refs, and queue-config land in
          subsequent stages — until then, edit the YAML directly and
          the dashboard picks up changes via SSE.
        </p>
      </header>

      <div className="flex flex-wrap gap-2" data-testid="settings-tabs">
        <TabButton
          label="Registry"
          active={tab === "registry"}
          onClick={() => setTab("registry")}
          testid="tab-registry"
        />
        <TabButton
          label="Custom pages"
          active={tab === "custom-pages"}
          onClick={() => setTab("custom-pages")}
          testid="tab-custom-pages"
        />
        <TabButton
          label="Culture"
          active={tab === "culture"}
          onClick={() => setTab("culture")}
          testid="tab-culture"
        />
        <TabButton
          label="Queue config"
          active={tab === "queue-config"}
          onClick={() => setTab("queue-config")}
          testid="tab-queue-config"
        />
        <TabButton
          label="Publish gates"
          active={tab === "publish-gates"}
          onClick={() => setTab("publish-gates")}
          testid="tab-publish-gates"
        />
      </div>

      {tab === "registry" && <RegistryTab />}
      {tab === "custom-pages" && <CustomPagesTab />}
      {tab === "culture" && <CultureTab />}
      {tab === "queue-config" && <QueueConfigTab />}
      {tab === "publish-gates" && <PublishGatesTab />}
    </div>
  );
}

type SettingsTab =
  | "registry"
  | "custom-pages"
  | "culture"
  | "queue-config"
  | "publish-gates";

function TabButton({
  label,
  active,
  onClick,
  testid,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  testid: string;
}) {
  return (
    <Button
      size="sm"
      variant={active ? "default" : "outline"}
      onClick={onClick}
      data-testid={testid}
    >
      {label}
    </Button>
  );
}

/**
 * S15-C-4 v1: Registry editor (domain-weights only).
 *
 * v1 surfaces a slider per declared domain weight. Operators can
 * adjust weights, save, and the dashboard's PUT endpoint validates
 * via `BrainRegistry::validate()` (which checks that weights sum to
 * 1.0 ± 0.01) before atomic-write + edit-via-bus emission.
 *
 * **Conflict detection:** the GET response carries a SHA-256 ETag
 * the client echoes back on PUT. When the on-disk file changed
 * between read and save, the server returns 409 Conflict and the
 * UI surfaces a "registry was edited externally" banner. The
 * operator reloads the page (loses their unsaved changes — v2 will
 * ship the 3-way merge UI).
 *
 * **Other registry sections** (autonomy, hats, federation children,
 * domain definitions) are deferred until the schemars-driven full
 * form generator lands. For those, operators continue to use vim;
 * the dashboard still picks up changes via SSE.
 */
function RegistryTab() {
  const brainId = useBrainId();
  const qc = useQueryClient();
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["registry", brainId],
    queryFn: () => fetchRegistry(brainId),
    refetchInterval: 60_000,
  });

  // Local edit state for sliders; reset to server values whenever
  // the registry refetches.
  const [edits, setEdits] = useState<Record<string, number>>({});
  useEffect(() => {
    if (data) {
      const config = data.registry as { config?: { domain_weights?: Record<string, number> } };
      const weights = config.config?.domain_weights ?? {};
      setEdits(weights);
    }
  }, [data]);

  const save = useMutation({
    mutationFn: async () => {
      if (!data) throw new Error("no data loaded");
      // Build the new registry by merging edits into a copy.
      const next = JSON.parse(JSON.stringify(data.registry)) as {
        config: { domain_weights: Record<string, number> };
      };
      next.config.domain_weights = edits;
      const url = brainApi(brainId, "registry");
      const res = await fetch(url, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          expected_etag: data.etag,
          registry: next,
        }),
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}) as Record<string, unknown>);
        const err = new Error(
          (body as { error?: string }).error ?? `${url} returned ${res.status}`,
        ) as Error & { code?: string };
        err.code = (body as { code?: string }).code;
        throw err;
      }
      return res.json();
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["registry", brainId] });
    },
  });

  if (isLoading) {
    return (
      <Card data-testid="settings-registry-card">
        <CardContent>Loading registry…</CardContent>
      </Card>
    );
  }
  if (error || !data) {
    return (
      <Card data-testid="settings-registry-card">
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <AlertTriangle className="h-5 w-5 text-destructive" />
            Failed to load registry
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          {error instanceof Error ? error.message : "unknown error"}
        </CardContent>
      </Card>
    );
  }

  const config = data.registry as {
    config?: { domain_weights?: Record<string, number> };
  };
  const weights = config.config?.domain_weights ?? {};
  const domainNames = Object.keys(weights).sort();

  // Compute weight sum for the validation hint (registry validation
  // requires sum = 1.0 ± 0.01).
  const sum = Object.values(edits).reduce((a, b) => a + b, 0);
  const sumValid = Math.abs(sum - 1.0) <= 0.01;
  const hasChanges = JSON.stringify(weights) !== JSON.stringify(edits);

  return (
    <Card data-testid="settings-registry-card">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <Sliders className="h-5 w-5" />
          Registry — domain weights
          <HelpIcon topic="scoring" anchor="domain-weights" />
        </CardTitle>
        <CardDescription>
          Slider per declared domain. Weights must sum to 1.0 ± 0.01 to
          pass `registry.validate()`. Other registry sections (autonomy,
          hats, federation, domain_definitions) are still edited via
          your text editor — the dashboard picks up changes via SSE.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="text-xs text-muted-foreground font-mono">{data.path}</div>
        <div className="space-y-3" data-testid="registry-domain-weight-list">
          {domainNames.length === 0 && (
            <div className="text-sm text-muted-foreground">
              No domain weights declared in the registry.
            </div>
          )}
          {domainNames.map((name) => (
            <DomainWeightRow
              key={name}
              name={name}
              value={edits[name] ?? 0}
              onChange={(v) =>
                setEdits((prev) => ({ ...prev, [name]: v }))
              }
            />
          ))}
        </div>
        <div className="flex items-center justify-between border-t pt-3">
          <div
            className={`text-sm ${sumValid ? "text-emerald-600" : "text-destructive"}`}
            data-testid="registry-weight-sum"
          >
            sum: {sum.toFixed(3)}{" "}
            {sumValid ? (
              <span className="inline-flex items-center gap-1">
                <CheckCircle2 className="h-3.5 w-3.5" />
                valid
              </span>
            ) : (
              <span className="inline-flex items-center gap-1">
                <AlertTriangle className="h-3.5 w-3.5" />
                must be 1.0 ± 0.01
              </span>
            )}
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => refetch()}
              disabled={!hasChanges || save.isPending}
              data-testid="registry-discard-button"
            >
              Discard
            </Button>
            <Button
              size="sm"
              variant="default"
              onClick={() => save.mutate()}
              disabled={!hasChanges || !sumValid || save.isPending}
              data-testid="registry-save-button"
            >
              <Save className="h-3.5 w-3.5 mr-1" />
              {save.isPending ? "Saving…" : "Save"}
            </Button>
          </div>
        </div>
        {save.isError && (
          <div
            className="text-sm text-destructive flex items-center gap-2"
            data-testid="registry-save-error"
          >
            <AlertTriangle className="h-4 w-4" />
            {save.error instanceof Error ? save.error.message : "save failed"}
            {(save.error as Error & { code?: string })?.code === "etag-conflict" && (
              <Button
                size="sm"
                variant="outline"
                className="ml-2"
                onClick={() => refetch()}
              >
                Reload
              </Button>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function DomainWeightRow({
  name,
  value,
  onChange,
}: {
  name: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <div
      className="flex items-center gap-3"
      data-testid={`registry-domain-row-${name}`}
    >
      <div className="font-mono text-sm w-48 truncate">{name}</div>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        className="flex-1"
        data-testid={`registry-slider-${name}`}
      />
      <div className="text-xs text-muted-foreground w-16 text-right tabular-nums">
        {value.toFixed(2)}
      </div>
    </div>
  );
}

async function fetchRegistry(brainId: string): Promise<RegistryResponse> {
  const url = brainApi(brainId, "registry");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as RegistryResponse;
}

/**
 * S15-C-6 v1: Custom pages CRUD tab.
 *
 * Lists existing custom pages (anything in `dashboard-pages.json`'s
 * `pages` map that isn't a built-in id). Add Page form takes a
 * kebab-case name; server validates + persists. Delete buttons per
 * row remove the page.
 *
 * **v1 scope:** name + delete only. Title, icon picker, widget
 * gallery integration, and folder grouping at the 8-page-limit
 * threshold ship in v2.
 */
function CustomPagesTab() {
  const brainId = useBrainId();
  const qc = useQueryClient();
  const [newName, setNewName] = useState("");
  const { data, error, isLoading } = useQuery({
    queryKey: ["dashboard-pages", brainId],
    queryFn: () => fetchDashboardPages(brainId),
    refetchInterval: 30_000,
  });

  const create = useMutation({
    mutationFn: async (name: string) => {
      const url = `${brainApi(brainId, "dashboard-pages")}/${encodeURIComponent(name)}`;
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as { error?: string };
        throw new Error(body.error ?? `${url} returned ${res.status}`);
      }
      return (await res.json()) as CustomPageMutationResponse;
    },
    onSuccess: () => {
      setNewName("");
      qc.invalidateQueries({ queryKey: ["dashboard-pages", brainId] });
    },
  });

  const remove = useMutation({
    mutationFn: async (name: string) => {
      const url = `${brainApi(brainId, "dashboard-pages")}/${encodeURIComponent(name)}`;
      const res = await fetch(url, { method: "DELETE" });
      if (!res.ok) {
        throw new Error(`${url} returned ${res.status}`);
      }
      return (await res.json()) as CustomPageMutationResponse;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dashboard-pages", brainId] });
    },
  });

  if (isLoading) {
    return (
      <Card data-testid="settings-custom-pages-card">
        <CardContent>Loading…</CardContent>
      </Card>
    );
  }
  if (error || !data) {
    return (
      <Card data-testid="settings-custom-pages-card">
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <AlertTriangle className="h-5 w-5 text-destructive" />
            Failed to load custom pages
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          {error instanceof Error ? error.message : "unknown error"}
        </CardContent>
      </Card>
    );
  }

  // Custom pages = pages that aren't built-in ids.
  const builtins = new Set([
    "overview",
    "services",
    "logs",
    "settings",
    "approvals",
    "publish-gates",
  ]);
  const customNames = Object.keys(data.pages)
    .filter((n) => !builtins.has(n))
    .sort();

  const trimmedName = newName.trim();
  const nameValid =
    trimmedName.length > 0 &&
    /^[a-z][a-z0-9-]{0,63}$/.test(trimmedName) &&
    !trimmedName.endsWith("-") &&
    !trimmedName.includes("--") &&
    !builtins.has(trimmedName) &&
    !data.pages[trimmedName];

  return (
    <Card data-testid="settings-custom-pages-card">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <FileText className="h-5 w-5" />
          Custom pages ({customNames.length})
        </CardTitle>
        <CardDescription>
          Operator-defined pages live alongside the built-ins. Reach
          them at <code className="text-xs">/brains/&lt;id&gt;/p/&lt;page-name&gt;</code>.
          v1 supports name + delete; widget gallery integration ships
          in v2.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2" data-testid="custom-pages-list">
          {customNames.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No custom pages declared yet.
            </div>
          ) : (
            customNames.map((name) => (
              <div
                key={name}
                className="flex items-center justify-between p-2 border rounded"
                data-testid={`custom-page-row-${name}`}
              >
                <div>
                  <div className="font-mono text-sm">{name}</div>
                  <div className="text-xs text-muted-foreground">
                    {data.pages[name]?.length ?? 0} widget(s)
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => remove.mutate(name)}
                  disabled={remove.isPending}
                  data-testid={`custom-page-delete-${name}`}
                >
                  <Trash2 className="h-3.5 w-3.5 mr-1" />
                  Delete
                </Button>
              </div>
            ))
          )}
        </div>
        <div className="border-t pt-4 space-y-2">
          <label className="block text-sm font-medium">
            Add a custom page
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              placeholder="kebab-case-name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              className="flex-1 px-2 py-1 text-sm border rounded font-mono"
              data-testid="custom-page-name-input"
            />
            <Button
              size="sm"
              variant="default"
              onClick={() => create.mutate(trimmedName)}
              disabled={!nameValid || create.isPending}
              data-testid="custom-page-create-button"
            >
              <Plus className="h-3.5 w-3.5 mr-1" />
              {create.isPending ? "Adding…" : "Add"}
            </Button>
          </div>
          <div className="text-xs text-muted-foreground">
            Lowercase letters + digits + hyphens; must start with a
            letter, max 64 chars, can't collide with built-in page
            ids.
          </div>
          {create.isError && (
            <div
              className="text-sm text-destructive flex items-center gap-2"
              data-testid="custom-page-create-error"
            >
              <AlertTriangle className="h-4 w-4" />
              {create.error instanceof Error
                ? create.error.message
                : "create failed"}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

async function fetchDashboardPages(
  brainId: string,
): Promise<DashboardPagesConfig> {
  const url = brainApi(brainId, "dashboard-pages");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as DashboardPagesConfig;
}

function CultureTab() {
  return (
    <ConfigViewer
      configName="culture.yaml"
      title="Culture"
      icon={<Lock className="h-5 w-5" />}
      description={
        <>
          Cultural values + invariants for this Brain (spec §14).{" "}
          <strong>Culture is a contract, not a setting</strong> — values
          can only tighten across the federation, never loosen. Edit
          the YAML directly when you need to change it; the
          coherence sensor verifies all four ecosystem copies stay
          byte-identical.
        </>
      }
      learnMoreCommand="neurogrim explain culture"
      testidSuffix="culture"
    />
  );
}

function QueueConfigTab() {
  return (
    <ConfigViewer
      configName="queue-config.yaml"
      title="Queue config"
      icon={<FileText className="h-5 w-5" />}
      description={
        <>
          Per-topic backend + retention configuration for the v4.1
          coordination bus. v1 ships JSONL-backed only; the SQLite
          opt-in (for <code className="text-xs">ack_required: true</code>{" "}
          consumer-group topics) lands in S13-B-3. Editor for this
          file ships when SQLite does.
        </>
      }
      learnMoreCommand="neurogrim explain queues"
      testidSuffix="queue-config"
    />
  );
}

function PublishGatesTab() {
  const brainId = useBrainId();
  return (
    <Card data-testid="settings-publish-gates-pointer">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <FileText className="h-5 w-5" />
          Publish gates
        </CardTitle>
        <CardDescription>
          The publish-gates manifest has its own dedicated page with
          gate-state visualization and the recent ledger timeline.
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm space-y-3">
        <p>
          Visit the{" "}
          <a
            href={`/brains/${encodeURIComponent(brainId)}/publish-gates`}
            className="underline text-primary"
          >
            Publish gates page
          </a>{" "}
          to see gate definitions, current state, and recent run
          history. Editor lands with S15-C-5 expansion in a later
          stage — until then, edit{" "}
          <code className="text-xs">
            .claude/brain/publish-gates.yaml
          </code>{" "}
          directly.
        </p>
        <p className="text-muted-foreground">
          See <code className="text-xs">neurogrim explain publish-gates</code>{" "}
          for the full schema.
        </p>
      </CardContent>
    </Card>
  );
}

function ConfigViewer({
  configName,
  title,
  icon,
  description,
  learnMoreCommand,
  testidSuffix,
}: {
  configName: string;
  title: string;
  icon: React.ReactNode;
  description: React.ReactNode;
  learnMoreCommand: string;
  testidSuffix: string;
}) {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["config-file", brainId, configName],
    queryFn: () => fetchConfigFile(brainId, configName),
    refetchInterval: 30_000,
  });

  return (
    <Card data-testid={`settings-${testidSuffix}-card`}>
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          {icon}
          {title}
        </CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {isLoading && (
          <div className="text-sm text-muted-foreground">Loading…</div>
        )}
        {error && (
          <div className="text-sm text-destructive flex items-center gap-2">
            <AlertTriangle className="h-4 w-4" />
            failed to load: {error instanceof Error ? error.message : "unknown"}
          </div>
        )}
        {data && (
          <>
            <div className="text-xs text-muted-foreground font-mono">
              {data.path}
            </div>
            {!data.present && !data.error && (
              <div
                className="text-sm text-muted-foreground flex items-center gap-2"
                data-testid={`settings-${testidSuffix}-absent`}
              >
                <CircleSlash className="h-4 w-4" />
                file not present — adopter hasn't authored it yet
              </div>
            )}
            {data.present && data.text && (
              <pre
                className="text-xs whitespace-pre-wrap bg-muted/30 p-3 rounded max-h-96 overflow-auto"
                data-testid={`settings-${testidSuffix}-text`}
              >
                {data.text}
              </pre>
            )}
            {data.error && (
              <div className="text-sm text-destructive flex items-center gap-2">
                <AlertTriangle className="h-4 w-4" />
                {data.error}
              </div>
            )}
          </>
        )}
        <div className="text-xs text-muted-foreground flex items-center gap-1">
          <Info className="h-3 w-3" />
          See <code className="text-xs ml-1">{learnMoreCommand}</code> for the conceptual reference.
        </div>
      </CardContent>
    </Card>
  );
}

async function fetchConfigFile(
  brainId: string,
  name: string,
): Promise<ConfigFileResponse> {
  const url = `${brainApi(brainId, "config-file")}/${encodeURIComponent(name)}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ConfigFileResponse;
}
