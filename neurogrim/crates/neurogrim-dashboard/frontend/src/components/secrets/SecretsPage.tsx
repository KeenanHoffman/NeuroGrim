import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Eye,
  EyeOff,
  KeyRound,
  Lock,
  RotateCw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import type { SecretsListResponse } from "@bindings/SecretsListResponse";
import type { SecretListItem } from "@bindings/SecretListItem";
import type { SetSecretResponse } from "@bindings/SetSecretResponse";
import type { DeleteSecretResponse } from "@bindings/DeleteSecretResponse";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button-ish";
import { Badge } from "@/components/ui/badge";
import { HelpIcon } from "@/components/help/HelpIcon";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * S14-S-6 v1: secrets-management page.
 *
 * Surfaces declared secrets from `<project>/.claude/secret-refs.yaml`
 * with their backend-stored status (present / missing). Operators
 * set + rotate values via a modal form; the value travels over the
 * HTTPS listener (S14-S-4.5 v2) when the operator has run
 * `tls-cert generate`. Otherwise it traverses HTTP on loopback —
 * still safe in practice for single-host deployments.
 *
 * **Critical UX invariant:** secret values are NEVER displayed back.
 * Operators can rotate or delete; the only way to read a stored
 * value is by exercising the application that consumes it.
 *
 * **v2 deferred:**
 *
 *   - "Test" button per-secret: validates a stored secret without
 *     exposing it (e.g., a no-op API call to verify auth). Needs
 *     adopter-defined test endpoints in `secret-refs.yaml`.
 *   - Client-side passphrase-derived encryption: TLS already
 *     protects the wire; the additional layer is meaningful for
 *     hostile-host threat models we don't currently in-scope.
 *   - Rotated-at history: today the OsNativeBackend tracks
 *     created_at + updated_at; v2 will show a per-secret rotation
 *     history.
 */
export function SecretsPage() {
  const brainId = useBrainId();
  const [editing, setEditing] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["secrets-list", brainId],
    queryFn: () => fetchSecretsList(brainId),
    refetchInterval: 30_000,
  });

  return (
    <div className="space-y-6 p-6" data-testid="secrets-page">
      <header>
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <KeyRound className="h-6 w-6" />
          Secrets
          <HelpIcon topic="secrets" />
        </h1>
        <p className="text-sm text-muted-foreground mt-1">
          Declared in <code className="text-xs">.claude/secret-refs.yaml</code>;
          values stored in the OS-native keyring (Windows Credential
          Manager / macOS Keychain / Linux libsecret).{" "}
          <strong>Values are never displayed back</strong> — operators
          can set / rotate / delete only.
        </p>
      </header>

      {isLoading && (
        <Card>
          <CardContent className="text-sm text-muted-foreground">
            Loading…
          </CardContent>
        </Card>
      )}

      {error && (
        <Card data-testid="secrets-error">
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Failed to load secrets
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {error instanceof Error ? error.message : "unknown error"}
          </CardContent>
        </Card>
      )}

      {data && !data.manifest_present && (
        <Card data-testid="secrets-no-manifest">
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <CircleSlash className="h-5 w-5 text-muted-foreground" />
              No secret-refs.yaml
            </CardTitle>
            <CardDescription>
              Author{" "}
              <code className="text-xs">{data.manifest_path}</code> to
              declare which secrets this Brain uses. The manifest is
              the source of truth; this page can only manage values
              for secrets that are declared there.
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {data && data.manifest_present && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <Lock className="h-5 w-5" />
              Declared secrets ({data.items.length})
            </CardTitle>
            <CardDescription>
              <code className="text-xs">{data.manifest_path}</code>
            </CardDescription>
          </CardHeader>
          <CardContent>
            {data.items.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                Manifest declares no secrets.
              </p>
            ) : (
              <Table data-testid="secrets-table">
                <TableHeader>
                  <TableRow>
                    <TableHead>ID</TableHead>
                    <TableHead>Description</TableHead>
                    <TableHead>Provider</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Updated</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {data.items.map((item) => (
                    <SecretRow
                      key={item.id}
                      item={item}
                      onEdit={() => setEditing(item.id)}
                    />
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      )}

      {editing && (
        <SetSecretModal
          brainId={brainId}
          secretId={editing}
          isPresent={
            data?.items.find((i) => i.id === editing)?.present ?? false
          }
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}

function SecretRow({
  item,
  onEdit,
}: {
  item: SecretListItem;
  onEdit: () => void;
}) {
  const brainId = useBrainId();
  const qc = useQueryClient();
  const remove = useMutation({
    mutationFn: async () => {
      const url = `${brainApi(brainId, "secrets")}/${encodeURIComponent(item.id)}`;
      const res = await fetch(url, { method: "DELETE" });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as { error?: string };
        throw new Error(body.error ?? `${url} returned ${res.status}`);
      }
      return (await res.json()) as DeleteSecretResponse;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["secrets-list", brainId] });
    },
  });

  return (
    <TableRow data-testid={`secret-row-${item.id}`}>
      <TableCell className="font-mono text-sm">{item.id}</TableCell>
      <TableCell className="text-xs text-muted-foreground max-w-xs">
        {item.description ?? "—"}
      </TableCell>
      <TableCell>
        <Badge variant="outline" className="text-xs">
          {item.provider ?? "—"}
        </Badge>
      </TableCell>
      <TableCell>
        {item.present ? (
          <Badge
            variant="default"
            className="gap-1"
            data-testid={`secret-status-${item.id}`}
          >
            <CheckCircle2 className="h-3 w-3" />
            present
          </Badge>
        ) : (
          <Badge
            variant="outline"
            className="gap-1"
            data-testid={`secret-status-${item.id}`}
          >
            <CircleSlash className="h-3 w-3" />
            missing
          </Badge>
        )}
      </TableCell>
      <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
        {item.updated_at ? formatTime(item.updated_at) : "—"}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-2">
          <Button
            size="sm"
            variant="default"
            onClick={onEdit}
            data-testid={`secret-edit-${item.id}`}
          >
            {item.present ? (
              <>
                <RotateCw className="h-3.5 w-3.5 mr-1" />
                Rotate
              </>
            ) : (
              <>
                <Save className="h-3.5 w-3.5 mr-1" />
                Set value
              </>
            )}
          </Button>
          {item.present && (
            <Button
              size="sm"
              variant="destructive"
              onClick={() => {
                if (
                  window.confirm(
                    `Delete the stored value for "${item.id}"?\n\nThe manifest entry stays; only the backend value is removed.`,
                  )
                ) {
                  remove.mutate();
                }
              }}
              disabled={remove.isPending}
              data-testid={`secret-delete-${item.id}`}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </TableCell>
    </TableRow>
  );
}

function SetSecretModal({
  brainId,
  secretId,
  isPresent,
  onClose,
}: {
  brainId: string;
  secretId: string;
  isPresent: boolean;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [value, setValue] = useState("");
  const [reveal, setReveal] = useState(false);

  const save = useMutation({
    mutationFn: async () => {
      const url = `${brainApi(brainId, "secrets")}/${encodeURIComponent(secretId)}`;
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ value }),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as { error?: string };
        throw new Error(body.error ?? `${url} returned ${res.status}`);
      }
      return (await res.json()) as SetSecretResponse;
    },
    onSuccess: () => {
      // Clear the local value as soon as the request lands so the
      // browser DOM doesn't keep plaintext around longer than
      // necessary. The state update is synchronous on the next
      // tick.
      setValue("");
      qc.invalidateQueries({ queryKey: ["secrets-list", brainId] });
      onClose();
    },
  });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      data-testid="secret-modal-backdrop"
      onClick={onClose}
    >
      <div
        className="bg-background border rounded-lg shadow-lg max-w-lg w-full m-4 flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        data-testid={`secret-modal-${secretId}`}
      >
        <header className="flex items-center justify-between p-4 border-b">
          <div>
            <h2 className="text-lg font-bold flex items-center gap-2">
              {isPresent ? (
                <RotateCw className="h-5 w-5" />
              ) : (
                <Save className="h-5 w-5" />
              )}
              {isPresent ? "Rotate" : "Set"} <code className="text-sm">{secretId}</code>
            </h2>
            <p className="text-xs text-muted-foreground mt-1">
              The value is sent over the dashboard's HTTPS listener
              (S14-S-4.5 v2) when configured. Server writes to the
              OS keyring + zeroes the request payload.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 hover:bg-muted rounded"
            aria-label="Close"
            data-testid="secret-modal-close"
          >
            <X className="h-4 w-4" />
          </button>
        </header>
        <div className="p-4 space-y-3">
          <label className="block text-sm font-medium">
            Value
            <div className="relative mt-1">
              <input
                type={reveal ? "text" : "password"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                autoFocus
                autoComplete="new-password"
                spellCheck={false}
                className="w-full px-2 py-1.5 pr-10 text-sm border rounded font-mono"
                data-testid="secret-value-input"
                placeholder="paste secret value here"
              />
              <button
                type="button"
                onClick={() => setReveal((r) => !r)}
                className="absolute inset-y-0 right-0 flex items-center px-2 text-muted-foreground hover:text-foreground"
                aria-label={reveal ? "Hide value" : "Reveal value"}
                data-testid="secret-reveal-toggle"
              >
                {reveal ? (
                  <EyeOff className="h-4 w-4" />
                ) : (
                  <Eye className="h-4 w-4" />
                )}
              </button>
            </div>
          </label>
          <p className="text-xs text-muted-foreground">
            <strong>Reminder:</strong> the dashboard never displays
            stored values back. After Save, the only way to inspect
            this secret is by exercising the application that
            consumes it.
          </p>
          {save.isError && (
            <div
              className="text-sm text-destructive flex items-start gap-2"
              data-testid="secret-save-error"
            >
              <AlertTriangle className="h-4 w-4 mt-0.5" />
              <span>
                {save.error instanceof Error
                  ? save.error.message
                  : "save failed"}
              </span>
            </div>
          )}
        </div>
        <footer className="flex justify-end gap-2 p-4 border-t bg-muted/30">
          <Button
            size="sm"
            variant="outline"
            onClick={onClose}
            data-testid="secret-modal-cancel"
          >
            Cancel
          </Button>
          <Button
            size="sm"
            variant="default"
            onClick={() => save.mutate()}
            disabled={!value || save.isPending}
            data-testid="secret-modal-save"
          >
            <Save className="h-3.5 w-3.5 mr-1" />
            {save.isPending ? "Saving…" : "Save"}
          </Button>
        </footer>
      </div>
    </div>
  );
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

async function fetchSecretsList(brainId: string): Promise<SecretsListResponse> {
  const url = brainApi(brainId, "secrets");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as SecretsListResponse;
}
