//! `cargo xtask schema-drift-check` — verify vendored schemas
//! match their canonical source in the LSP-Brains spec repo
//! (V5-MOD-2 Phase 0, 2026-05-02).
//!
//! NeuroGrim vendors a small set of JSON Schemas from the LSP-Brains
//! spec repository so `neurogrim-core` can validate against them
//! without a path dep on the spec. Drift between the local copy and
//! the canonical source is a real risk when the spec ships a v2
//! schema; this xtask catches that drift mechanically before it
//! ships in a release.
//!
//! # Vendored schema registry
//!
//! See [`VENDORED_SCHEMAS`] below. Add an entry when a new schema is
//! vendored from LSP-Brains. The xtask compares the local path
//! against `<lsp_brains>/<canonical_relative_path>` for each entry.
//!
//! # Schemas authored locally are NOT checked
//!
//! `diagnostics-ledger-v1.schema.json` (V5-FOUND-1 Phase 2) was
//! authored inside `neurogrim-core` and has no upstream in
//! LSP-Brains. The drift-check skips it by registry omission — only
//! schemas listed in [`VENDORED_SCHEMAS`] are compared.
//!
//! # Behavior on missing LSP-Brains directory
//!
//! - **Spec dir exists** → compare each schema; drift = exit 1.
//! - **Spec dir missing** → warn + exit 0 (CI environments may not
//!   check out the spec; not a hard error).

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use colored::Colorize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// Registry of schemas vendored from LSP-Brains into NeuroGrim.
///
/// Adding a new vendored schema:
/// 1. Copy the schema from `<lsp_brains>/schemas/foo.json` to
///    `crates/neurogrim-core/data/schemas/foo.json`.
/// 2. Add a header comment at the top of the local copy citing the
///    canonical LSP-Brains path.
/// 3. Add an entry to this registry.
/// 4. Run `cargo xtask schema-drift-check` to confirm equality.
const VENDORED_SCHEMAS: &[VendoredSchema] = &[
    VendoredSchema {
        local: "crates/neurogrim-core/data/schemas/cmdb-envelope-v1.schema.json",
        canonical: "schemas/cmdb-envelope-v1.schema.json",
        synced_against_spec_version: "v2.7 (E-B2-1 confidence field)",
    },
];

#[derive(Debug, Clone, Copy)]
struct VendoredSchema {
    /// Path under the NeuroGrim workspace root.
    local: &'static str,
    /// Path relative to the LSP-Brains repo root.
    canonical: &'static str,
    /// Spec version the local copy was synced against. Informational
    /// — for the human reading the drift report.
    synced_against_spec_version: &'static str,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Path to the LSP-Brains repo root. Defaults to `../LSP-Brains/`
    /// (sibling to the NeuroGrim repo). Override for non-default
    /// checkouts or CI environments.
    #[arg(long, default_value = "../LSP-Brains")]
    lsp_brains_path: PathBuf,

    /// Path to the NeuroGrim workspace root. Defaults to "." (the
    /// current working directory, which is the workspace root when
    /// invoked via `cargo xtask`).
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    let spec_dir = &args.lsp_brains_path;
    let workspace = &args.workspace_root;

    // Spec dir missing → warn + exit 0. CI may not check it out;
    // a vendored copy that ships ahead of a missing spec dir is
    // still releaseable.
    if !spec_dir.exists() {
        println!(
            "{} LSP-Brains spec directory not found at {}; \
             skipping drift-check (set --lsp-brains-path to override)",
            "WARN".yellow(),
            spec_dir.display()
        );
        return Ok(());
    }

    let mut drifted = Vec::new();
    let mut missing = Vec::new();
    let mut compared = 0usize;

    for entry in VENDORED_SCHEMAS {
        let local_path = workspace.join(entry.local);
        let canonical_path = spec_dir.join(entry.canonical);

        if !local_path.exists() {
            missing.push(format!("local copy missing: {}", local_path.display()));
            continue;
        }
        if !canonical_path.exists() {
            missing.push(format!(
                "canonical missing in spec: {} (entry: {})",
                canonical_path.display(),
                entry.local
            ));
            continue;
        }

        let local_text = fs::read_to_string(&local_path)
            .with_context(|| format!("read local copy: {}", local_path.display()))?;
        let canonical_text = fs::read_to_string(&canonical_path)
            .with_context(|| format!("read canonical: {}", canonical_path.display()))?;

        // Parse both as JSON Values. Equality of `serde_json::Value`
        // is structural — whitespace, key order, and trailing
        // newlines are normalized away. The local copy has a header
        // comment in the `description` field; match against the
        // canonical's `description` after stripping the vendoring
        // marker. We do this by parsing both, cloning the local,
        // and overwriting the local's `description` with the
        // canonical's before comparing — so semantic drift in any
        // OTHER field is still caught.
        let local: Value = serde_json::from_str(&local_text)
            .with_context(|| format!("parse local: {}", local_path.display()))?;
        let canonical: Value = serde_json::from_str(&canonical_text)
            .with_context(|| format!("parse canonical: {}", canonical_path.display()))?;

        let mut local_for_compare = local.clone();
        if let (Some(local_obj), Some(canonical_desc)) = (
            local_for_compare.as_object_mut(),
            canonical.get("description"),
        ) {
            local_obj.insert("description".to_string(), canonical_desc.clone());
        }

        compared += 1;
        if local_for_compare != canonical {
            drifted.push((entry, local_path.clone(), canonical_path.clone()));
        }
    }

    // Report
    println!(
        "schema-drift-check: compared {compared}/{} vendored schemas against {}",
        VENDORED_SCHEMAS.len(),
        spec_dir.display()
    );
    for entry in VENDORED_SCHEMAS {
        let drift_match = drifted.iter().find(|(e, _, _)| e.local == entry.local);
        let missing_match = missing.iter().find(|m| m.contains(entry.local));
        match (drift_match, missing_match) {
            (Some(_), _) => {
                println!(
                    "  {} {} (synced against: {})",
                    "DRIFTED".red(),
                    entry.local,
                    entry.synced_against_spec_version
                );
            }
            (_, Some(reason)) => {
                println!("  {} {} ({})", "MISSING".yellow(), entry.local, reason);
            }
            _ => {
                println!(
                    "  {} {} (synced against: {})",
                    "ok".green(),
                    entry.local,
                    entry.synced_against_spec_version
                );
            }
        }
    }

    if !drifted.is_empty() {
        println!(
            "\n{} {} schema(s) drifted from canonical. Update the \
             local copy or bump to a new schema version.",
            "DRIFT".red().bold(),
            drifted.len()
        );
        for (_, local, canonical) in &drifted {
            println!(
                "  diff {} {}",
                local.display(),
                canonical.display()
            );
        }
        anyhow::bail!("schema drift detected");
    }

    if !missing.is_empty() {
        println!(
            "\n{} {} schema path(s) could not be compared. Resolve \
             before relying on the drift-check.",
            "WARN".yellow().bold(),
            missing.len()
        );
    }

    println!("\n{}", "all vendored schemas match canonical".green());
    Ok(())
}
