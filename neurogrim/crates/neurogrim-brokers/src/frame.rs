//! A13 / BB #35 — Frame stack.
//!
//! A `Frame` is a key-value bag of operator-tunable values that travels
//! with every dispatch. Pipelines declare their `tunability` (Untunable /
//! OperatorOnly / OperatorConfirmed / Autonomous); the frame's values
//! steer per-dispatch behavior at the tier the operator allows.
//!
//! ## MVP scope (V0)
//!
//! - **`Frame`** — `HashMap<String, serde_json::Value>` of operator-declared
//!   defaults loaded from cluster manifest `[frame]` TOML section.
//! - **`LeafContext.frame: Frame`** — leaf-ops + preconditions read frame
//!   values during dispatch.
//! - **Precondition DSL substitution** — `{frame.<key>}` placeholders in
//!   precondition strings are replaced with the frame value before the DSL
//!   evaluates. Combines with the existing `{param_name}` substitution.
//! - **Frame inheritance**: sub-pipeline dispatches inherit the parent
//!   frame (no per-sub-pipeline override in V0; that lands when a real
//!   tuning-pipeline use case appears).
//! - **Untunable enforcement** is via Rust types: Frame is immutable after
//!   construction; there's no `Frame::set()` method, so a `Pipeline` with
//!   `Tunability::Untunable` can't be tuned at runtime. Operator-confirmed
//!   tuning becomes a separate API in S1-T (proposal ledger integration).
//!
//! ## Deferred to S1-T
//!
//! - Operator runtime-tuning UX (CLI + Brain UI). V0 = edit manifest, restart.
//! - Autonomous tuning + proposal ledger integration.
//! - Frame inheritance overrides (sub-pipeline declares "override
//!   parent.stakes for this dispatch").
//! - Untunable runtime enforcement (V0 relies on the absence of mutation
//!   API; S1-T may add a `Frame::propose_tune()` that respects tunability).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frame {
    /// Key-value pairs (operator-declared in cluster manifest). Use dotted
    /// names like `stakes`, `posture`, `risk_appetite`, `latency_budget_ms`.
    pub values: HashMap<String, serde_json::Value>,
}

impl Frame {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<serde_json::Value>,
    {
        let mut values = HashMap::new();
        for (k, v) in pairs {
            values.insert(k.into(), v.into());
        }
        Self { values }
    }

    /// Get a frame value by key. Returns `None` if not declared.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.values.get(key)
    }

    /// Get a frame value as string (handles strings + auto-stringifies
    /// numbers/bools so precondition DSL substitution is cheap).
    pub fn get_as_string(&self, key: &str) -> Option<String> {
        self.values.get(key).map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Substitute `{frame.<key>}` placeholders in `s` with frame values.
    /// Used by the precondition DSL.
    pub fn substitute_placeholders(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                // Try to parse {frame.KEY}
                let mut buf = String::new();
                let mut found_close = false;
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        chars.next();
                        found_close = true;
                        break;
                    }
                    buf.push(chars.next().unwrap());
                }
                if found_close {
                    if let Some(key) = buf.strip_prefix("frame.") {
                        if let Some(val) = self.get_as_string(key) {
                            out.push_str(&val);
                            continue;
                        }
                    }
                    // Not a frame placeholder — preserve as-is so the caller's
                    // {param_name} substitution still works downstream.
                    out.push('{');
                    out.push_str(&buf);
                    out.push('}');
                } else {
                    out.push('{');
                    out.push_str(&buf);
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_get_returns_declared_value() {
        let f = Frame::from_pairs(vec![
            ("stakes", serde_json::json!("high")),
            ("latency_budget_ms", serde_json::json!(500)),
        ]);
        assert_eq!(f.get("stakes").unwrap(), &serde_json::json!("high"));
        assert_eq!(f.get_as_string("stakes"), Some("high".to_string()));
        assert_eq!(f.get_as_string("latency_budget_ms"), Some("500".to_string()));
        assert!(f.get("nonexistent").is_none());
    }

    #[test]
    fn substitute_frame_placeholders_replaces_known_keys() {
        let f = Frame::from_pairs(vec![("stakes", serde_json::json!("high"))]);
        let out = f.substitute_placeholders("requires {frame.stakes} stakes");
        assert_eq!(out, "requires high stakes");
    }

    #[test]
    fn substitute_preserves_unknown_or_non_frame_placeholders() {
        let f = Frame::from_pairs(vec![("stakes", serde_json::json!("high"))]);
        // {frame.unknown} → not substituted (key missing), preserved literally
        let out = f.substitute_placeholders("{frame.unknown} {param_x}");
        assert_eq!(out, "{frame.unknown} {param_x}");
    }

    #[test]
    fn substitute_handles_mixed_placeholders_and_text() {
        let f = Frame::from_pairs(vec![
            ("stakes", serde_json::json!("medium")),
            ("budget", serde_json::json!(1000)),
        ]);
        let s = "Run with {frame.stakes} stakes; budget {frame.budget}; user-param {x}";
        assert_eq!(
            f.substitute_placeholders(s),
            "Run with medium stakes; budget 1000; user-param {x}"
        );
    }

    #[test]
    fn substitute_handles_unclosed_brace() {
        let f = Frame::from_pairs(vec![("stakes", serde_json::json!("high"))]);
        let out = f.substitute_placeholders("trailing {open");
        assert_eq!(out, "trailing {open");
    }
}
