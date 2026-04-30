import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  FileText,
  HardHat,
  Info,
  Lock,
  Network,
  Plus,
  Save,
  ShieldAlert,
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
 * S15-C-4 v2: Registry editor with curated forms per section.
 *
 * v1 (shipped) ships domain weights as the single editable section
 * + ETag-based conflict detection. v2 (this) extends with curated
 * editors for the v4.x load-bearing sections:
 *
 * - **Weights** — slider per declared domain (v1 carryover).
 * - **Autonomy** — per-action_type level dropdown (auto/notify/
 *   approve/blocked); safety invariants list display. The v4.x
 *   reframe makes this load-bearing: operators set policy here,
 *   not in vim.
 * - **Hats** — per-hat domain_multipliers table + description +
 *   add/remove hat.
 * - **Federation** — children CRUD: display_name, a2a_endpoint,
 *   weight, enabled. Wraps v3.5 `federation rewire` flow as a CLI
 *   pointer (full button-driven rewire is a v3 follow-on).
 *
 * **Single Save semantics:** the whole registry is one editable
 * document. Sub-tabs are projections; edits accumulate into a
 * shared draft; one Save button validates + PUTs the whole thing.
 * This keeps the ETag flow simple (one read, one write) and means
 * `BrainRegistry::validate()` runs against the full draft.
 *
 * **Deferred to v3:** schemars-derived JSON Schema endpoint +
 * generic form generator (handles arbitrary registry sections);
 * 3-way merge UI on conflict (current behavior: reload-on-conflict);
 * domain definitions / `_todo_<name>` editors; interactive rewire
 * button (currently a CLI pointer).
 */
function RegistryTab() {
  const brainId = useBrainId();
  const qc = useQueryClient();
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["registry", brainId],
    queryFn: () => fetchRegistry(brainId),
    refetchInterval: 60_000,
  });

  // Whole-registry draft state. Reset to server values whenever the
  // registry refetches. We hold the entire registry (not just the
  // currently-edited slice) so cross-section edits accumulate
  // cleanly and a single Save round-trips the full document.
  const [draft, setDraft] = useState<RegistryDoc | null>(null);
  useEffect(() => {
    if (data) {
      setDraft(JSON.parse(JSON.stringify(data.registry)) as RegistryDoc);
    }
  }, [data]);

  const [subTab, setSubTab] = useState<RegistrySubTab>("weights");

  const save = useMutation({
    mutationFn: async () => {
      if (!data || !draft) throw new Error("no data loaded");
      const url = brainApi(brainId, "registry");
      const res = await fetch(url, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          expected_etag: data.etag,
          registry: draft,
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

  // Memoize derived flags so unrelated sub-tab edits don't churn
  // the Save button on every keystroke.
  const { hasChanges, sumValid } = useMemo(() => {
    if (!data || !draft) return { hasChanges: false, sumValid: true };
    const changes = JSON.stringify(data.registry) !== JSON.stringify(draft);
    const weights = draft.config?.domain_weights ?? {};
    const sum = Object.values(weights).reduce(
      (a: number, b) => a + (typeof b === "number" ? b : 0),
      0,
    );
    return { hasChanges: changes, sumValid: Math.abs(sum - 1.0) <= 0.01 };
  }, [data, draft]);

  if (isLoading) {
    return (
      <Card data-testid="settings-registry-card">
        <CardContent>Loading registry…</CardContent>
      </Card>
    );
  }
  if (error || !data || !draft) {
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

  return (
    <Card data-testid="settings-registry-card">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <Sliders className="h-5 w-5" />
          Registry editor
          <HelpIcon topic="scoring" anchor="domain-weights" />
        </CardTitle>
        <CardDescription>
          Curated forms for the v4.x load-bearing sections. Edits
          accumulate across sub-tabs and save as a single atomic
          PUT — `BrainRegistry::validate()` runs server-side. The
          schemars-driven generic form generator (for arbitrary
          sections) is a v3 follow-on.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="text-xs text-muted-foreground font-mono">{data.path}</div>
        <div className="flex flex-wrap gap-2" data-testid="registry-subtabs">
          <RegistrySubTabButton
            label="Weights"
            icon={<Sliders className="h-3.5 w-3.5" />}
            active={subTab === "weights"}
            onClick={() => setSubTab("weights")}
            testid="registry-subtab-weights"
          />
          <RegistrySubTabButton
            label="Autonomy"
            icon={<ShieldAlert className="h-3.5 w-3.5" />}
            active={subTab === "autonomy"}
            onClick={() => setSubTab("autonomy")}
            testid="registry-subtab-autonomy"
          />
          <RegistrySubTabButton
            label="Hats"
            icon={<HardHat className="h-3.5 w-3.5" />}
            active={subTab === "hats"}
            onClick={() => setSubTab("hats")}
            testid="registry-subtab-hats"
          />
          <RegistrySubTabButton
            label="Federation"
            icon={<Network className="h-3.5 w-3.5" />}
            active={subTab === "federation"}
            onClick={() => setSubTab("federation")}
            testid="registry-subtab-federation"
          />
        </div>

        {subTab === "weights" && (
          <RegistryWeightsEditor draft={draft} setDraft={setDraft} />
        )}
        {subTab === "autonomy" && (
          <RegistryAutonomyEditor draft={draft} setDraft={setDraft} />
        )}
        {subTab === "hats" && (
          <RegistryHatsEditor draft={draft} setDraft={setDraft} />
        )}
        {subTab === "federation" && (
          <RegistryFederationEditor draft={draft} setDraft={setDraft} />
        )}

        <div className="flex items-center justify-between border-t pt-3">
          <div
            className={`text-sm ${sumValid ? "text-emerald-600" : "text-destructive"}`}
            data-testid="registry-weight-sum"
          >
            weight sum: {weightSum(draft).toFixed(3)}{" "}
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

type RegistrySubTab = "weights" | "autonomy" | "hats" | "federation";

/**
 * Loose registry document shape. Curated editors operate on
 * specific paths under `config`; everything else is preserved
 * verbatim through the JSON.parse / JSON.stringify round-trip.
 */
type RegistryDoc = {
  config?: {
    domain_weights?: Record<string, number>;
    autonomy?: AutonomySection;
    hats?: Record<string, HatEntry>;
    children?: Record<string, FederationChildEntry>;
    [key: string]: unknown;
  };
  [key: string]: unknown;
};

type AutonomySection = {
  levels?: Record<string, AutonomyLevelEntry>;
  action_types?: Record<string, AutonomyActionTypeEntry>;
  safety_invariants?: SafetyInvariantEntry[];
  [key: string]: unknown;
};

type AutonomyLevelEntry = {
  requires_approval?: boolean;
  description?: string;
  [key: string]: unknown;
};

type AutonomyActionTypeEntry = {
  default_level?: string;
  blast_radius?: string;
  reversible?: boolean;
  [key: string]: unknown;
};

type SafetyInvariantEntry = {
  rule?: string;
  enforced_level?: string;
  description?: string;
  [key: string]: unknown;
};

type HatEntry = {
  description?: string;
  domain_multipliers?: Record<string, number>;
  [key: string]: unknown;
};

type FederationChildEntry = {
  display_name?: string;
  a2a_endpoint?: string;
  agent_card_url?: string;
  brain_path?: string;
  interface_version?: string;
  weight?: number;
  enabled?: boolean;
  depends_on?: string[];
  [key: string]: unknown;
};

function weightSum(draft: RegistryDoc): number {
  const weights = draft.config?.domain_weights ?? {};
  return Object.values(weights).reduce(
    (a: number, b) => a + (typeof b === "number" ? b : 0),
    0,
  );
}

function RegistrySubTabButton({
  label,
  icon,
  active,
  onClick,
  testid,
}: {
  label: string;
  icon: React.ReactNode;
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
      className="flex items-center gap-1.5"
    >
      {icon}
      {label}
    </Button>
  );
}

// ── Weights sub-editor (v1 carryover) ────────────────────────────────────

function RegistryWeightsEditor({
  draft,
  setDraft,
}: {
  draft: RegistryDoc;
  setDraft: (next: RegistryDoc) => void;
}) {
  const weights = draft.config?.domain_weights ?? {};
  const domainNames = Object.keys(weights).sort();

  function updateWeight(name: string, v: number) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.domain_weights = next.config.domain_weights ?? {};
    next.config.domain_weights[name] = v;
    setDraft(next);
  }

  return (
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
          value={weights[name] ?? 0}
          onChange={(v) => updateWeight(name, v)}
        />
      ))}
    </div>
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

// ── Autonomy sub-editor (v4.x reframe — load-bearing) ────────────────────

const AUTONOMY_LEVEL_ORDER = ["auto", "notify", "approve", "blocked"];

function RegistryAutonomyEditor({
  draft,
  setDraft,
}: {
  draft: RegistryDoc;
  setDraft: (next: RegistryDoc) => void;
}) {
  const autonomy = draft.config?.autonomy ?? {};
  const levels = autonomy.levels ?? {};
  const actionTypes = autonomy.action_types ?? {};
  const safetyInvariants = autonomy.safety_invariants ?? [];

  // Level order: declared `auto/notify/approve/blocked` first (in
  // canonical order), then anything else alphabetically. Operators
  // can declare more levels but the canonical 4 are universally
  // understood.
  const declaredLevelKeys = Object.keys(levels);
  const orderedLevels = [
    ...AUTONOMY_LEVEL_ORDER.filter((k) => declaredLevelKeys.includes(k)),
    ...declaredLevelKeys
      .filter((k) => !AUTONOMY_LEVEL_ORDER.includes(k))
      .sort(),
  ];
  const actionTypeNames = Object.keys(actionTypes).sort();

  function updateActionTypeLevel(actionType: string, level: string) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.autonomy = next.config.autonomy ?? {};
    next.config.autonomy.action_types = next.config.autonomy.action_types ?? {};
    next.config.autonomy.action_types[actionType] = {
      ...(next.config.autonomy.action_types[actionType] ?? {}),
      default_level: level,
    };
    setDraft(next);
  }

  if (declaredLevelKeys.length === 0 && actionTypeNames.length === 0) {
    return (
      <div
        className="text-sm text-muted-foreground py-4"
        data-testid="registry-autonomy-absent"
      >
        No autonomy block declared in the registry. Run{" "}
        <code className="text-xs">neurogrim doctor</code> for the recommended
        starter shape, or add an `autonomy:` section to the registry directly.
      </div>
    );
  }

  return (
    <div className="space-y-5" data-testid="registry-autonomy-editor">
      <div>
        <h3 className="text-sm font-semibold mb-2 flex items-center gap-1.5">
          Action types
          <HelpIcon topic="autonomy" anchor="action-types" />
        </h3>
        <p className="text-xs text-muted-foreground mb-3">
          Per-action_type default level. The MCP dispatch path
          consults this when an agent invokes a mutation tool.
        </p>
        <div className="space-y-2" data-testid="registry-autonomy-action-list">
          {actionTypeNames.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No action_types declared.
            </div>
          ) : (
            actionTypeNames.map((name) => {
              const entry = actionTypes[name] ?? {};
              return (
                <div
                  key={name}
                  className="flex items-center gap-3 py-1"
                  data-testid={`registry-autonomy-row-${name}`}
                >
                  <div className="font-mono text-sm w-48 truncate">{name}</div>
                  <select
                    value={entry.default_level ?? ""}
                    onChange={(e) =>
                      updateActionTypeLevel(name, e.target.value)
                    }
                    className="flex-1 max-w-xs px-2 py-1 text-sm border rounded bg-background"
                    data-testid={`registry-autonomy-level-${name}`}
                  >
                    {orderedLevels.map((lvl) => (
                      <option key={lvl} value={lvl}>
                        {lvl}
                      </option>
                    ))}
                  </select>
                  <div className="text-xs text-muted-foreground flex gap-2">
                    {entry.blast_radius && (
                      <span>blast: {entry.blast_radius}</span>
                    )}
                    {entry.reversible !== undefined && (
                      <span>{entry.reversible ? "reversible" : "irreversible"}</span>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      <div>
        <h3 className="text-sm font-semibold mb-2 flex items-center gap-1.5">
          Levels
          <HelpIcon topic="autonomy" anchor="levels" />
        </h3>
        <p className="text-xs text-muted-foreground mb-3">
          Read-only display of the declared autonomy levels and
          their `requires_approval` semantics. v3 adds a level
          editor; today add new levels via the registry file.
        </p>
        <div className="space-y-1" data-testid="registry-autonomy-level-list">
          {orderedLevels.map((key) => {
            const lvl = levels[key];
            return (
              <div
                key={key}
                className="flex items-center gap-3 text-sm py-1 border-b last:border-b-0"
                data-testid={`registry-autonomy-level-row-${key}`}
              >
                <div className="font-mono w-24">{key}</div>
                <div className="w-32 text-xs">
                  {lvl?.requires_approval ? "requires approval" : "auto-runs"}
                </div>
                <div className="text-xs text-muted-foreground flex-1 truncate">
                  {lvl?.description ?? ""}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div>
        <h3 className="text-sm font-semibold mb-2 flex items-center gap-1.5">
          Safety invariants
          <HelpIcon topic="autonomy" anchor="safety-invariants" />
        </h3>
        <p className="text-xs text-muted-foreground mb-3">
          Hard floors that override per-action level (e.g.
          destructive operations remain blocked regardless of
          confidence). Read-only in v2 — invariants are a contract
          surface and benefit from explicit text-editor review.
        </p>
        <div
          className="space-y-1"
          data-testid="registry-autonomy-invariants-list"
        >
          {safetyInvariants.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No safety invariants declared.
            </div>
          ) : (
            safetyInvariants.map((inv, i) => (
              <div
                key={i}
                className="text-sm py-1 border-b last:border-b-0"
                data-testid={`registry-autonomy-invariant-${i}`}
              >
                <div className="font-mono">
                  {inv.rule ?? "(unnamed)"}{" "}
                  <span className="text-xs text-muted-foreground ml-1">
                    → {inv.enforced_level ?? "unknown"}
                  </span>
                </div>
                {inv.description && (
                  <div className="text-xs text-muted-foreground">
                    {inv.description}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

// ── Hats sub-editor ──────────────────────────────────────────────────────

function RegistryHatsEditor({
  draft,
  setDraft,
}: {
  draft: RegistryDoc;
  setDraft: (next: RegistryDoc) => void;
}) {
  const hats = draft.config?.hats ?? {};
  const hatNames = Object.keys(hats).sort();
  const declaredDomains = Object.keys(draft.config?.domain_weights ?? {}).sort();
  const [newHatName, setNewHatName] = useState("");

  function updateHat(name: string, mut: (entry: HatEntry) => void) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.hats = next.config.hats ?? {};
    const entry = { ...(next.config.hats[name] ?? {}) };
    mut(entry);
    next.config.hats[name] = entry;
    setDraft(next);
  }

  function deleteHat(name: string) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.hats = next.config.hats ?? {};
    delete next.config.hats[name];
    setDraft(next);
  }

  function addHat(name: string) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.hats = next.config.hats ?? {};
    next.config.hats[name] = {
      description: "",
      domain_multipliers: {},
    };
    setDraft(next);
    setNewHatName("");
  }

  const trimmedNew = newHatName.trim();
  const newNameValid =
    trimmedNew.length > 0 &&
    /^[a-z][a-z0-9-]{0,40}$/.test(trimmedNew) &&
    !hats[trimmedNew];

  return (
    <div className="space-y-4" data-testid="registry-hats-editor">
      <p className="text-xs text-muted-foreground">
        Per-hat domain multipliers bias the unified score when an
        operator or agent narrates `--hat &lt;name&gt;`. Multipliers
        above 1.0 amplify; below 1.0 dampen. Hats can only narrow
        attention, not loosen the cultural floor (see{" "}
        <code className="text-xs">neurogrim explain hat</code>).
      </p>
      <div className="space-y-3" data-testid="registry-hats-list">
        {hatNames.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            No hats declared. Add one below to start biasing scores
            for a decision-making lens.
          </div>
        ) : (
          hatNames.map((name) => (
            <HatRow
              key={name}
              name={name}
              entry={hats[name] ?? {}}
              declaredDomains={declaredDomains}
              onUpdate={(mut) => updateHat(name, mut)}
              onDelete={() => deleteHat(name)}
            />
          ))
        )}
      </div>
      <div className="border-t pt-3 space-y-2">
        <label className="block text-sm font-medium">Add a hat</label>
        <div className="flex gap-2">
          <input
            type="text"
            placeholder="kebab-case-name"
            value={newHatName}
            onChange={(e) => setNewHatName(e.target.value)}
            className="flex-1 max-w-xs px-2 py-1 text-sm border rounded font-mono"
            data-testid="registry-hat-new-name"
          />
          <Button
            size="sm"
            variant="default"
            onClick={() => addHat(trimmedNew)}
            disabled={!newNameValid}
            data-testid="registry-hat-add-button"
          >
            <Plus className="h-3.5 w-3.5 mr-1" />
            Add hat
          </Button>
        </div>
        <div className="text-xs text-muted-foreground">
          Lowercase letters + digits + hyphens; must start with a letter.
        </div>
      </div>
    </div>
  );
}

function HatRow({
  name,
  entry,
  declaredDomains,
  onUpdate,
  onDelete,
}: {
  name: string;
  entry: HatEntry;
  declaredDomains: string[];
  onUpdate: (mut: (entry: HatEntry) => void) => void;
  onDelete: () => void;
}) {
  const multipliers = entry.domain_multipliers ?? {};
  return (
    <div
      className="border rounded p-3 space-y-3"
      data-testid={`registry-hat-row-${name}`}
    >
      <div className="flex items-center justify-between">
        <div className="font-mono text-sm font-medium">{name}</div>
        <Button
          size="sm"
          variant="destructive"
          onClick={onDelete}
          data-testid={`registry-hat-delete-${name}`}
        >
          <Trash2 className="h-3.5 w-3.5 mr-1" />
          Remove
        </Button>
      </div>
      <div>
        <label className="block text-xs text-muted-foreground mb-1">
          Description
        </label>
        <input
          type="text"
          value={entry.description ?? ""}
          onChange={(e) =>
            onUpdate((next) => {
              next.description = e.target.value;
            })
          }
          className="w-full px-2 py-1 text-sm border rounded"
          data-testid={`registry-hat-desc-${name}`}
        />
      </div>
      <div>
        <div className="text-xs text-muted-foreground mb-1">
          Domain multipliers (0–5)
        </div>
        <div className="space-y-1">
          {declaredDomains.length === 0 ? (
            <div className="text-xs text-muted-foreground italic">
              No domains declared in registry.
            </div>
          ) : (
            declaredDomains.map((dom) => {
              const mult = multipliers[dom] ?? 1.0;
              return (
                <div
                  key={dom}
                  className="flex items-center gap-2"
                  data-testid={`registry-hat-${name}-mult-${dom}`}
                >
                  <div className="font-mono text-xs w-44 truncate">{dom}</div>
                  <input
                    type="range"
                    min={0}
                    max={5}
                    step={0.1}
                    value={mult}
                    onChange={(e) =>
                      onUpdate((next) => {
                        next.domain_multipliers =
                          next.domain_multipliers ?? {};
                        next.domain_multipliers[dom] = parseFloat(
                          e.target.value,
                        );
                      })
                    }
                    className="flex-1"
                    data-testid={`registry-hat-${name}-slider-${dom}`}
                  />
                  <div className="text-xs text-muted-foreground w-12 text-right tabular-nums">
                    {mult.toFixed(1)}×
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}

// ── Federation children sub-editor ───────────────────────────────────────

function RegistryFederationEditor({
  draft,
  setDraft,
}: {
  draft: RegistryDoc;
  setDraft: (next: RegistryDoc) => void;
}) {
  const children = draft.config?.children ?? {};
  const childNames = Object.keys(children).sort();
  const [newChildName, setNewChildName] = useState("");

  function updateChild(name: string, mut: (e: FederationChildEntry) => void) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.children = next.config.children ?? {};
    const entry = { ...(next.config.children[name] ?? {}) };
    mut(entry);
    next.config.children[name] = entry;
    setDraft(next);
  }

  function deleteChild(name: string) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.children = next.config.children ?? {};
    delete next.config.children[name];
    setDraft(next);
  }

  function addChild(name: string) {
    const next = JSON.parse(JSON.stringify(draft)) as RegistryDoc;
    next.config = next.config ?? {};
    next.config.children = next.config.children ?? {};
    next.config.children[name] = {
      display_name: name,
      a2a_endpoint: "",
      interface_version: "1",
      weight: 1.0,
      enabled: true,
    };
    setDraft(next);
    setNewChildName("");
  }

  const trimmedNew = newChildName.trim();
  const newNameValid =
    trimmedNew.length > 0 &&
    /^[a-z][a-z0-9-]{0,40}$/.test(trimmedNew) &&
    !children[trimmedNew];

  return (
    <div className="space-y-4" data-testid="registry-federation-editor">
      <p className="text-xs text-muted-foreground">
        A2A peer Brains. Adopters use{" "}
        <code className="text-xs">neurogrim federation rewire --child &lt;name&gt;</code>{" "}
        to reconcile a child's persisted port with this registry; the
        button-driven flow ships in v3.
      </p>
      <div className="space-y-3" data-testid="registry-federation-list">
        {childNames.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            No children declared.
          </div>
        ) : (
          childNames.map((name) => (
            <FederationChildRow
              key={name}
              name={name}
              entry={children[name] ?? {}}
              onUpdate={(mut) => updateChild(name, mut)}
              onDelete={() => deleteChild(name)}
            />
          ))
        )}
      </div>
      <div className="border-t pt-3 space-y-2">
        <label className="block text-sm font-medium">Add a child</label>
        <div className="flex gap-2">
          <input
            type="text"
            placeholder="kebab-case-name"
            value={newChildName}
            onChange={(e) => setNewChildName(e.target.value)}
            className="flex-1 max-w-xs px-2 py-1 text-sm border rounded font-mono"
            data-testid="registry-federation-new-name"
          />
          <Button
            size="sm"
            variant="default"
            onClick={() => addChild(trimmedNew)}
            disabled={!newNameValid}
            data-testid="registry-federation-add-button"
          >
            <Plus className="h-3.5 w-3.5 mr-1" />
            Add child
          </Button>
        </div>
        <div className="text-xs text-muted-foreground">
          Adds a stub entry — set the a2a_endpoint to point at the
          child's running peer.
        </div>
      </div>
    </div>
  );
}

function FederationChildRow({
  name,
  entry,
  onUpdate,
  onDelete,
}: {
  name: string;
  entry: FederationChildEntry;
  onUpdate: (mut: (e: FederationChildEntry) => void) => void;
  onDelete: () => void;
}) {
  return (
    <div
      className="border rounded p-3 space-y-2"
      data-testid={`registry-federation-row-${name}`}
    >
      <div className="flex items-center justify-between">
        <div className="font-mono text-sm font-medium">{name}</div>
        <Button
          size="sm"
          variant="destructive"
          onClick={onDelete}
          data-testid={`registry-federation-delete-${name}`}
        >
          <Trash2 className="h-3.5 w-3.5 mr-1" />
          Remove
        </Button>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
        <div>
          <label className="block text-xs text-muted-foreground mb-1">
            Display name
          </label>
          <input
            type="text"
            value={entry.display_name ?? ""}
            onChange={(e) =>
              onUpdate((next) => {
                next.display_name = e.target.value;
              })
            }
            className="w-full px-2 py-1 text-sm border rounded"
            data-testid={`registry-federation-display-${name}`}
          />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">
            A2A endpoint
          </label>
          <input
            type="text"
            value={entry.a2a_endpoint ?? ""}
            onChange={(e) =>
              onUpdate((next) => {
                next.a2a_endpoint = e.target.value;
              })
            }
            placeholder="http://127.0.0.1:8421/a2a/v1/"
            className="w-full px-2 py-1 text-sm border rounded font-mono"
            data-testid={`registry-federation-endpoint-${name}`}
          />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">
            Weight (0–1)
          </label>
          <input
            type="number"
            min={0}
            max={1}
            step={0.05}
            value={entry.weight ?? 1.0}
            onChange={(e) =>
              onUpdate((next) => {
                next.weight = parseFloat(e.target.value);
              })
            }
            className="w-32 px-2 py-1 text-sm border rounded tabular-nums"
            data-testid={`registry-federation-weight-${name}`}
          />
        </div>
        <div className="flex items-end">
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={entry.enabled ?? true}
              onChange={(e) =>
                onUpdate((next) => {
                  next.enabled = e.target.checked;
                })
              }
              data-testid={`registry-federation-enabled-${name}`}
            />
            <span>Enabled</span>
          </label>
        </div>
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
          <HelpIcon topic="command-post" anchor="multi-page-schema" />
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
      helpTopic="culture"
      helpAnchor="five-values"
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
      helpTopic="queues"
      helpAnchor="reserved-namespace"
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
          <HelpIcon topic="publish-gates" anchor="gate-types" />
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
  helpTopic,
  helpAnchor,
  testidSuffix,
}: {
  configName: string;
  title: string;
  icon: React.ReactNode;
  description: React.ReactNode;
  learnMoreCommand: string;
  helpTopic?: string;
  helpAnchor?: string;
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
          {helpTopic && <HelpIcon topic={helpTopic} anchor={helpAnchor} />}
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
