# Secret Refs — Safe Secret Catalog for Agents

**When to use this skill:** You need an agent to work with secrets (generate
code that fetches a credential, audit which secrets a project uses, onboard a
new credential) without ever exposing the secret value in the model's context.
The `secret-refs` domain catalogs where each credential lives, what it's for,
and how to access it — the agent reads the reference pattern (the safe lookup),
never the value itself. Positive containment: if a secret isn't in the manifest,
the agent does not know it exists.

Role: reference · configuration
Trigger phrases: "secret", "credential", "API key", "secret manager",
"how do I access", "where is the secret", "secret-refs", "fetch credentials",
"secret catalog"
Methodology-step: skills

---

## The Safety Guarantee

```
Agent reads:    secret_catalog[].reference_pattern   ← safe code, no values
Agent never sees: the actual credential value         ← lives only in GCP/AWS/Vault/…
```

If a secret isn't in the manifest, the agent doesn't know it exists. The manifest is
**positive containment**: only what you document is reachable by the agent.

---

## Files

| File | Status | Contains |
|------|--------|---------|
| `.claude/secret-refs.yaml` | ✅ Safe to commit | References only — no values |
| `.claude/secret-refs-cmdb.json` | ✅ Safe to commit | Rendered reference patterns — no values |
| Secret manager (GCP/AWS/…) | ❌ Never in repo | Actual credential values |

---

## Manifest Schema (`.claude/secret-refs.yaml`)

```yaml
# Safe to commit — no values, only references.

default_provider: gcp        # Used when an entry omits `provider`

# Custom providers (optional — see Python SDK section below)
providers:
  my-vault:
    description: "Internal HashiCorp Vault"
    reference_template: |
      import hvac, os
      client = hvac.Client(url="{vault_url}", token=os.environ["VAULT_TOKEN"])
      {env_var} = client.secrets.kv.v2.read_secret_version(
          path="{secret_path}")["data"]["data"]["value"]
    access_pattern: "{mount}/{path}"

secrets:
  db-password:
    description: "PostgreSQL master password — production"
    env_var: DATABASE_PASSWORD
    provider: gcp
    secret_path: projects/my-project/secrets/DB_PASSWORD/versions/latest
    used_by:
      - api-service
      - migration-runner
    rotation_days: 90
    tags: [database, production]
    vault_url: ""               # optional; needed for azure/vault providers
```

### Secret entry fields

| Field | Required | Description |
|-------|----------|-------------|
| `description` | Recommended | What this secret is for |
| `env_var` | Yes | Environment variable name (e.g. `DATABASE_PASSWORD`) |
| `provider` | No | Provider name; defaults to `default_provider` |
| `secret_path` | Yes | Full path in the secret manager |
| `used_by` | Recommended | Services/components that consume this secret |
| `rotation_days` | Recommended | Rotation policy in days |
| `vault_url` | For azure/vault | Secret manager endpoint URL |
| `tags` | No | Free-form labels |

---

## Built-In Providers

All built-in reference templates use `{env_var}`, `{secret_path}`, and `{vault_url}` as substitution tokens.

### `gcp` — Google Cloud Secret Manager
```python
from google.cloud import secretmanager
client = secretmanager.SecretManagerServiceClient()
response = client.access_secret_version(request={"name": "{secret_path}"})
{env_var} = response.payload.data.decode("UTF-8")
```
`secret_path` shape: `projects/{project}/secrets/{name}/versions/latest`

### `aws` — AWS Secrets Manager
```python
import boto3
client = boto3.client("secretsmanager")
response = client.get_secret_value(SecretId="{secret_path}")
{env_var} = response["SecretString"]
```
`secret_path` shape: `prod/my-service/db-password`

### `azure` — Azure Key Vault
```python
from azure.keyvault.secrets import SecretClient
from azure.identity import DefaultAzureCredential
client = SecretClient(vault_url="{vault_url}", credential=DefaultAzureCredential())
{env_var} = client.get_secret("{secret_path}").value
```
`secret_path` shape: `db-password` (secret name only; vault URL is in `vault_url`)

### `vault` — HashiCorp Vault (KV v2)
```python
import hvac, os
client = hvac.Client(url="{vault_url}", token=os.environ["VAULT_TOKEN"])
{env_var} = client.secrets.kv.v2.read_secret_version(path="{secret_path}")["data"]["data"]["value"]
```
`secret_path` shape: `prod/db/password`

### `env` — Environment variable (local / CI)
```python
import os
{env_var} = os.environ["{env_var}"]  # Must be set externally; use a secret manager in production
```
Use for local dev or CI pipelines where secrets are injected as env vars.

---

## Scoring Model

```
+40  manifest exists with ≥1 secret entry
+20  all entries have descriptions
+20  all entries have rotation_days
+20  scan coverage: no undocumented env vars detected in .env* files
−10  per undocumented env var (detected in code but not in manifest)
clamp(0, 100)
```

| Score | Meaning |
|-------|---------|
| 0 | No manifest — agent has no reference knowledge |
| 40 | Manifest exists but incomplete |
| 60–79 | Good coverage, some metadata missing |
| 80–99 | Near-complete contract |
| 100 | All secrets documented, described, rotation-planned, full scan coverage |

---

## Running the Tool

```bash
# Regenerate CMDB from manifest
neurogrim sensory secret-refs --project-root . > .claude/secret-refs-cmdb.json

# View the secret catalog (safe — no values)
cat .claude/secret-refs-cmdb.json | jq '.secret_catalog[] | {id, env_var, provider, reference_pattern}'

# Check for undocumented secrets
cat .claude/secret-refs-cmdb.json | jq '.undocumented_secrets'

# Full health with secret-refs row
neurogrim health
```

---

## How Agents Consume the Catalog

When an agent needs to generate code that accesses a secret, it reads
`secret_catalog` from the CMDB:

```python
# Agent reads from Brain context:
# secret_catalog = [{
#   "id": "db-password",
#   "env_var": "DATABASE_PASSWORD",
#   "provider": "gcp",
#   "reference_pattern": "from google.cloud import secretmanager\n..."
# }]

# Agent generates (using reference_pattern):
from google.cloud import secretmanager
client = secretmanager.SecretManagerServiceClient()
response = client.access_secret_version(
    request={"name": "projects/my-project/secrets/DB_PASSWORD/versions/latest"})
DATABASE_PASSWORD = response.payload.data.decode("UTF-8")
```

The agent never asked for the value. It used the reference to generate safe lookup code.

---

## Custom Providers via the Python SDK

Define a custom provider by subclassing `SecretProvider` and calling `register()`.
This writes the provider into `.claude/secret-refs.yaml` under `providers:`, where the
Rust sensory tool picks it up on the next run — no changes to Rust required.

```python
from lsp_brains import SecretProvider, SecretProviderSpec

class MyInternalVault(SecretProvider):
    spec = SecretProviderSpec(
        name="my-vault",
        description="Internal HashiCorp Vault with AppRole auth",
        reference_template=(
            "import hvac, os\n"
            "client = hvac.Client(url=\"{vault_url}\", token=os.environ[\"VAULT_TOKEN\"])\n"
            "{env_var} = client.secrets.kv.v2.read_secret_version(\n"
            "    path=\"{secret_path}\")[\"data\"][\"data\"][\"value\"]"
        ),
        access_pattern="{mount}/{path}",
    )

# Register once — writes to .claude/secret-refs.yaml
MyInternalVault.register(project_root=".")

# Also usable for offline rendering / testing:
rendered = MyInternalVault.render_reference({
    "env_var": "DB_PASSWORD",
    "secret_path": "prod/db/password",
    "vault_url": "https://vault.internal.company.com",
})
print(rendered)
```

After registering, add secret entries that reference `provider: my-vault`:
```yaml
secrets:
  internal-db:
    description: "Internal database password"
    env_var: INTERNAL_DB_PASSWORD
    provider: my-vault
    secret_path: prod/internal/db-password
    vault_url: https://vault.internal.company.com
    rotation_days: 30
```

### `SecretProviderSpec` fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique provider ID (kebab-case) |
| `description` | Yes | Human-readable description |
| `reference_template` | Yes | Python code template with `{env_var}`, `{secret_path}`, `{vault_url}` tokens |
| `access_pattern` | No | Documents the `secret_path` shape (e.g. `"{mount}/{path}"`) |

---

## Domain Variables

| Variable | Type | Description |
|----------|------|-------------|
| `secret-refs:has_manifest` | bool | Whether `.claude/secret-refs.yaml` exists |
| `secret-refs:secrets_documented` | number | Count of entries in manifest |
| `secret-refs:all_secrets_described` | bool | All entries have `description` |
| `secret-refs:undocumented_env_vars` | number | Env vars detected but not in manifest |

---

## Correlation Examples

```json
{
  "id": "secrets-without-rotation",
  "type": "dependency",
  "severity": "warning",
  "domains": ["secret-refs", "security-standards"],
  "description": "Secrets are catalogued but rotation policy is missing — compliance evidence is incomplete.",
  "condition_tree": {
    "and": [
      { ">":  ["secret-refs:secrets_documented",    0] },
      { "==": ["secret-refs:all_secrets_described", false] }
    ]
  }
},
{
  "id": "undocumented-secrets-in-ci",
  "type": "compound_risk",
  "severity": "critical",
  "domains": ["secret-refs", "deploy-readiness"],
  "description": "Undocumented env vars detected while deploy pipeline is active — agent cannot generate safe access code for all secrets.",
  "condition_tree": {
    "and": [
      { ">": ["secret-refs:undocumented_env_vars", 0] },
      { ">": ["deploy-readiness:score",           30] }
    ]
  }
}
```

---

## Domain Promotion Guide

`secret-refs` starts at advisory weight `0.0`. Promote when:
- All application secrets are in the manifest
- All entries have descriptions and rotation policies
- Scan coverage is consistently 100% (no undocumented env vars)

```json
"secret-refs": 0.05
```

At 5% weight, a score of 0 (no manifest) reduces unified health by ~5 points — enough to appear in recommendations.
