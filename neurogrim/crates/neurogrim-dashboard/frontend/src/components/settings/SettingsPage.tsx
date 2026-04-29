import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CircleSlash,
  FileText,
  Info,
  Lock,
} from "lucide-react";
import type { ConfigFileResponse } from "@bindings/ConfigFileResponse";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button-ish";
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

      {tab === "culture" && <CultureTab />}
      {tab === "queue-config" && <QueueConfigTab />}
      {tab === "publish-gates" && <PublishGatesTab />}
    </div>
  );
}

type SettingsTab = "culture" | "queue-config" | "publish-gates";

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
