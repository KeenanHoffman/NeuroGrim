//! Secret references sensory tool — safe credential catalog for agents.
//!
//! Reads `.claude/secret-refs.yaml` (the human-authored manifest) and produces
//! a CMDB that catalogs every secret the project uses — with its location in the
//! secret manager, its purpose, and a pre-rendered reference pattern — but never
//! the value itself.
//!
//! Agents read `secret_catalog` from the CMDB to generate safe secret-access code
//! without ever needing the actual credential.
//!
//! Provider system:
//!   Built-in: gcp, aws, azure, vault, env
//!   Custom:   defined under `providers:` in the manifest YAML
//!   Merge:    custom providers override built-ins by name

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ── Built-in provider reference templates ────────────────────────────────────
// {env_var}    → the environment variable name (e.g. DATABASE_PASSWORD)
// {secret_path} → the full path in the secret manager
// {vault_url}  → the Vault/Azure endpoint URL (for vault/azure providers)

const GCP_TEMPLATE: &str = concat!(
    "from google.cloud import secretmanager\n",
    "client = secretmanager.SecretManagerServiceClient()\n",
    "response = client.access_secret_version(request={{\"name\": \"{secret_path}\"}})\n",
    "{env_var} = response.payload.data.decode(\"UTF-8\")"
);

const AWS_TEMPLATE: &str = concat!(
    "import boto3\n",
    "client = boto3.client(\"secretsmanager\")\n",
    "response = client.get_secret_value(SecretId=\"{secret_path}\")\n",
    "{env_var} = response[\"SecretString\"]"
);

const AZURE_TEMPLATE: &str = concat!(
    "from azure.keyvault.secrets import SecretClient\n",
    "from azure.identity import DefaultAzureCredential\n",
    "client = SecretClient(vault_url=\"{vault_url}\", credential=DefaultAzureCredential())\n",
    "{env_var} = client.get_secret(\"{secret_path}\").value"
);

const VAULT_TEMPLATE: &str = concat!(
    "import hvac, os\n",
    "client = hvac.Client(url=\"{vault_url}\", token=os.environ[\"VAULT_TOKEN\"])\n",
    "{env_var} = client.secrets.kv.v2.read_secret_version(",
    "path=\"{secret_path}\")[\"data\"][\"data\"][\"value\"]"
);

const ENV_TEMPLATE: &str = concat!(
    "import os\n",
    "{env_var} = os.environ[\"{env_var}\"]",
    "  # Must be set in environment; use a secret manager in production"
);

// ── MCP server boilerplate ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SecretRefsServer {
    tool_router: ToolRouter<Self>,
}
impl SecretRefsServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSecretRefsParams {
    pub project_root: String,
}

#[tool_router]
impl SecretRefsServer {
    #[tool(
        description = "Check secret references manifest: reads .claude/secret-refs.yaml, \
        resolves reference patterns for each catalogued secret via built-in or custom providers, \
        and scans .env* files for undocumented credentials. Returns CMDB-envelope JSON with \
        secret_catalog (safe for agents — no values, only reference patterns)."
    )]
    async fn check_secret_refs(&self, Parameters(p): Parameters<CheckSecretRefsParams>) -> String {
        serde_json::to_string_pretty(&analyze_secret_refs(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for SecretRefsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Secret references sensory tool. Produces a safe catalog of secret references \
                (no values) that agents use to generate correct secret-access code."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Analysis ──────────────────────────────────────────────────────────────────

pub async fn analyze_secret_refs(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    // ── Step 1: Load manifest ────────────────────────────────────────────────
    let manifest_path = root.join(".claude/secret-refs.yaml");
    let manifest_str = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(s) => s,
        Err(_) => {
            // No manifest — score 0, let the agent know how to fix this
            findings.push(Finding {
                name: "manifest_missing".into(),
                status: "missing".into(),
                points: 0,
                detail: Some("Create .claude/secret-refs.yaml to catalog this project's secrets. See .claude/skills/secret-refs.md.".into()),
            });
            extras.push(("has_manifest", Value::Bool(false)));
            extras.push(("secrets_documented", Value::from(0u32)));
            extras.push(("undocumented_env_vars", Value::from(0u32)));
            extras.push(("all_secrets_described", Value::Bool(false)));
            extras.push(("all_secrets_have_rotation", Value::Bool(false)));
            extras.push(("env_vars_detected", Value::from(0u32)));
            extras.push(("secret_catalog", json!([])));
            extras.push(("undocumented_secrets", json!([])));
            return build_cmdb("check-secret-refs", 0, findings, Some(extras));
        }
    };

    let manifest: Value = match serde_yaml::from_str(&manifest_str) {
        Ok(v) => v,
        Err(e) => {
            findings.push(Finding {
                name: "manifest_parse_error".into(),
                status: "error".into(),
                points: 0,
                detail: Some(format!(".claude/secret-refs.yaml parse error: {e}")),
            });
            extras.push(("has_manifest", Value::Bool(true)));
            extras.push(("secrets_documented", Value::from(0u32)));
            extras.push(("undocumented_env_vars", Value::from(0u32)));
            extras.push(("all_secrets_described", Value::Bool(false)));
            extras.push(("all_secrets_have_rotation", Value::Bool(false)));
            extras.push(("env_vars_detected", Value::from(0u32)));
            extras.push(("secret_catalog", json!([])));
            extras.push(("undocumented_secrets", json!([])));
            return build_cmdb("check-secret-refs", 0, findings, Some(extras));
        }
    };

    // ── Step 2: Extract manifest sections ────────────────────────────────────
    let default_provider = manifest
        .get("default_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("env")
        .to_string();

    let secrets_map = manifest
        .get("secrets")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let custom_providers = manifest
        .get("providers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // ── Step 3: Build merged provider template map ───────────────────────────
    // Start with built-ins; custom providers (from manifest) override by name.
    let mut provider_templates: HashMap<String, String> = HashMap::from([
        ("gcp".into(), GCP_TEMPLATE.into()),
        ("aws".into(), AWS_TEMPLATE.into()),
        ("azure".into(), AZURE_TEMPLATE.into()),
        ("vault".into(), VAULT_TEMPLATE.into()),
        ("env".into(), ENV_TEMPLATE.into()),
    ]);
    for (name, spec) in &custom_providers {
        if let Some(tmpl) = spec.get("reference_template").and_then(|v| v.as_str()) {
            provider_templates.insert(name.clone(), tmpl.to_string());
        }
    }

    // ── Step 4: Scan .env* files for env var names ───────────────────────────
    let env_file_names = [
        ".env",
        ".env.example",
        ".env.local",
        ".env.template",
        ".env.sample",
    ];
    let mut detected_env_vars: HashSet<String> = HashSet::new();
    for fname in &env_file_names {
        let fpath = root.join(fname);
        if let Ok(content) = tokio::fs::read_to_string(&fpath).await {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some(eq_pos) = line.find('=') {
                    let key = line[..eq_pos].trim().to_string();
                    // Only collect names that look like env vars (uppercase, underscores, digits)
                    if key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                        && !key.is_empty()
                    {
                        detected_env_vars.insert(key);
                    }
                }
            }
        }
    }

    // ── Step 5: Process each secret entry ────────────────────────────────────
    let mut catalog: Vec<Value> = Vec::new();
    let mut documented_env_vars: HashSet<String> = HashSet::new();
    let mut all_described = true;
    let mut all_have_rotation = true;

    for (id, entry) in &secrets_map {
        let description = entry
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let env_var = entry.get("env_var").and_then(|v| v.as_str()).unwrap_or("");
        let provider = entry
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_provider)
            .to_string();
        let secret_path = entry
            .get("secret_path")
            .and_then(|v| v.as_str())
            .unwrap_or(env_var);
        let vault_url = entry
            .get("vault_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://vault.example.com");
        let rotation = entry.get("rotation_days");
        let used_by = entry.get("used_by").cloned().unwrap_or(json!([]));
        let tags = entry.get("tags").cloned().unwrap_or(json!([]));

        if description.is_empty() {
            all_described = false;
        }
        if rotation.is_none() {
            all_have_rotation = false;
        }
        if !env_var.is_empty() {
            documented_env_vars.insert(env_var.to_string());
        }

        // Render reference pattern
        let template = provider_templates
            .get(&provider)
            .cloned()
            .unwrap_or_else(|| ENV_TEMPLATE.to_string());

        let reference_pattern = template
            .replace("{env_var}", env_var)
            .replace("{secret_path}", secret_path)
            .replace("{vault_url}", vault_url);

        let mut catalog_entry = json!({
            "id":                id,
            "description":       description,
            "env_var":           env_var,
            "provider":          provider,
            "secret_path":       secret_path,
            "used_by":           used_by,
            "tags":              tags,
            "reference_pattern": reference_pattern,
        });

        if let Some(r) = rotation {
            catalog_entry["rotation_days"] = r.clone();
        }

        catalog.push(catalog_entry);
    }

    // ── Step 6: Compute coverage ──────────────────────────────────────────────
    let undocumented: Vec<String> = detected_env_vars
        .difference(&documented_env_vars)
        .cloned()
        .collect();

    let secrets_count = secrets_map.len() as u32;
    let env_detected = detected_env_vars.len() as u32;
    let undoc_count = undocumented.len() as u32;

    // ── Step 7: Score ─────────────────────────────────────────────────────────
    if secrets_count > 0 {
        score += 40; // manifest exists with ≥1 entry
    }
    if secrets_count > 0 && all_described {
        score += 20;
    }
    if secrets_count > 0 && all_have_rotation {
        score += 20;
    }
    // Coverage: full coverage = +20; deduct 10 per undocumented var
    if undoc_count == 0 {
        score += 20;
    } else {
        score -= (undoc_count as i32) * 10;
    }

    // ── Step 8: Findings ──────────────────────────────────────────────────────
    if secrets_count > 0 {
        findings.push(Finding {
            name: "secrets_catalogued".into(),
            status: "ok".into(),
            points: 40,
            detail: Some(format!("{secrets_count} secret(s) documented in manifest.")),
        });
    }
    if !all_described && secrets_count > 0 {
        findings.push(Finding {
            name: "missing_descriptions".into(),
            status: "warning".into(),
            points: 0,
            detail: Some(
                "Some secrets lack descriptions. Add a `description:` field to each entry.".into(),
            ),
        });
    }
    if !all_have_rotation && secrets_count > 0 {
        findings.push(Finding {
            name: "missing_rotation_policy".into(),
            status: "warning".into(),
            points: 0,
            detail: Some(
                "Some secrets have no `rotation_days` defined. Document your rotation policy."
                    .into(),
            ),
        });
    }
    for var in &undocumented {
        findings.push(Finding {
            name: format!("undocumented_{}", var.to_lowercase()),
            status: "warning".into(),
            points: -10,
            detail: Some(format!(
                "Env var '{var}' detected in .env* file but not in secret-refs.yaml manifest. Add an entry."
            )),
        });
    }

    // ── Step 9: Assemble extras ───────────────────────────────────────────────
    extras.push(("has_manifest", Value::Bool(true)));
    extras.push(("secrets_documented", Value::from(secrets_count)));
    extras.push((
        "all_secrets_described",
        Value::Bool(all_described && secrets_count > 0),
    ));
    extras.push((
        "all_secrets_have_rotation",
        Value::Bool(all_have_rotation && secrets_count > 0),
    ));
    extras.push(("env_vars_detected", Value::from(env_detected)));
    extras.push(("undocumented_env_vars", Value::from(undoc_count)));
    extras.push(("secret_catalog", Value::Array(catalog)));
    extras.push(("undocumented_secrets", json!(undocumented)));

    build_cmdb(
        "check-secret-refs",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
    )
}
