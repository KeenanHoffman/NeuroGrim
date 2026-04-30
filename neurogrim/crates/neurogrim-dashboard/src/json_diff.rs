//! Keypath-level JSON diffing for the `_neurogrim/config-changes`
//! queue (S15-C-7 v2).
//!
//! Operator edits to the registry / dashboard layout / custom pages
//! emit on the bus so agents observe. v1 of the payload had only a
//! free-text `summary`; v2 adds a `diff: [{path, op, before, after}]`
//! list so subscribers can react surgically without re-fetching the
//! whole document.
//!
//! ## Path format
//!
//! Object keys are joined with `.`; array indices are bracketed:
//! `config.autonomy.action_types.edit-code.default_level`,
//! `config.children.python-starter.weight`, `widgets[2].size`.
//! Top-level scalar replacements (rare in our domain — the root is
//! always an object) use the `"$"` marker.
//!
//! ## Op semantics (RFC 6902 alignment)
//!
//! - **`add`**: path didn't exist in `before`, exists in `after`.
//!   `before` is omitted from the entry.
//! - **`remove`**: path existed in `before`, gone in `after`.
//!   `after` is omitted.
//! - **`replace`**: path exists in both, value changed (or types
//!   differ). Both `before` and `after` are populated.
//!
//! ## Truncation
//!
//! Diff output is capped at [`MAX_CHANGES`] entries to keep queue
//! payloads bounded. Callers that exceed the cap get the first
//! `MAX_CHANGES` deterministic-ordered changes (BTree-sorted keys,
//! ascending array indices); the cap is generous enough that
//! realistic operator edits always fit.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Maximum number of keypath changes emitted per diff. A typical
/// registry edit touches 1-3 paths; a layout change adds/removes a
/// handful of widgets. 100 is comfortably above realistic bounds
/// while small enough that a pathological all-fields-edit doesn't
/// blow the queue payload.
pub const MAX_CHANGES: usize = 100;

/// Single keypath-level change. Mirrors the payload field shape
/// agents subscribed to `_neurogrim/config-changes` will see.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct KeypathChange {
    /// Dotted path with array brackets, e.g. `config.autonomy.action_types.edit-code.default_level`
    /// or `widgets[2].size`. Empty path becomes `"$"` for top-level
    /// scalar replacements (defensive — our domain's roots are
    /// always objects).
    pub path: String,
    pub op: ChangeOp,
    /// Prior value (None for `add`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(type = "unknown")]
    pub before: Option<Value>,
    /// New value (None for `remove`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(type = "unknown")]
    pub after: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Add,
    Remove,
    Replace,
}

/// Compute the keypath-level diff between two JSON values.
/// Returns at most [`MAX_CHANGES`] entries; semantically equal
/// inputs produce an empty vec.
pub fn diff(before: &Value, after: &Value) -> Vec<KeypathChange> {
    let mut changes = Vec::new();
    let mut prefix = String::new();
    diff_recursive(&mut prefix, before, after, &mut changes);
    if changes.len() > MAX_CHANGES {
        changes.truncate(MAX_CHANGES);
    }
    changes
}

fn diff_recursive(
    prefix: &mut String,
    before: &Value,
    after: &Value,
    changes: &mut Vec<KeypathChange>,
) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            // Sorted union of keys → deterministic output ordering.
            let mut keys: std::collections::BTreeSet<&String> =
                std::collections::BTreeSet::new();
            keys.extend(b.keys());
            keys.extend(a.keys());
            for key in keys {
                let saved_len = prefix.len();
                if !prefix.is_empty() {
                    prefix.push('.');
                }
                prefix.push_str(key);
                match (b.get(key), a.get(key)) {
                    (Some(bv), Some(av)) => diff_recursive(prefix, bv, av, changes),
                    (Some(bv), None) => changes.push(KeypathChange {
                        path: path_or_root(prefix),
                        op: ChangeOp::Remove,
                        before: Some(bv.clone()),
                        after: None,
                    }),
                    (None, Some(av)) => changes.push(KeypathChange {
                        path: path_or_root(prefix),
                        op: ChangeOp::Add,
                        before: None,
                        after: Some(av.clone()),
                    }),
                    (None, None) => {} // unreachable but defensive
                }
                prefix.truncate(saved_len);
            }
        }
        (Value::Array(b), Value::Array(a)) => {
            // Index-based diff. Reorderings show up as a series of
            // replaces — good enough for our domain (widgets are
            // positional, weights are keyed by name not index).
            let max_len = b.len().max(a.len());
            for i in 0..max_len {
                let saved_len = prefix.len();
                prefix.push_str(&format!("[{i}]"));
                match (b.get(i), a.get(i)) {
                    (Some(bv), Some(av)) => diff_recursive(prefix, bv, av, changes),
                    (Some(bv), None) => changes.push(KeypathChange {
                        path: path_or_root(prefix),
                        op: ChangeOp::Remove,
                        before: Some(bv.clone()),
                        after: None,
                    }),
                    (None, Some(av)) => changes.push(KeypathChange {
                        path: path_or_root(prefix),
                        op: ChangeOp::Add,
                        before: None,
                        after: Some(av.clone()),
                    }),
                    (None, None) => {}
                }
                prefix.truncate(saved_len);
            }
        }
        // Types differ or scalars at this level — emit replace.
        _ => changes.push(KeypathChange {
            path: path_or_root(prefix),
            op: ChangeOp::Replace,
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn path_or_root(prefix: &str) -> String {
    if prefix.is_empty() {
        "$".to_string()
    } else {
        prefix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn equal_values_produce_no_changes() {
        let v = json!({"a": 1, "b": [2, 3]});
        assert_eq!(diff(&v, &v), Vec::new());
    }

    #[test]
    fn empty_objects_produce_no_changes() {
        assert_eq!(diff(&json!({}), &json!({})), Vec::new());
    }

    #[test]
    fn scalar_replace_at_known_path() {
        let before = json!({"a": "old"});
        let after = json!({"a": "new"});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "a");
        assert_eq!(changes[0].op, ChangeOp::Replace);
        assert_eq!(changes[0].before, Some(json!("old")));
        assert_eq!(changes[0].after, Some(json!("new")));
    }

    #[test]
    fn nested_replace_uses_dotted_path() {
        let before = json!({
            "config": {
                "autonomy": {
                    "action_types": {
                        "edit-code": { "default_level": "approve" }
                    }
                }
            }
        });
        let after = json!({
            "config": {
                "autonomy": {
                    "action_types": {
                        "edit-code": { "default_level": "auto" }
                    }
                }
            }
        });
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].path,
            "config.autonomy.action_types.edit-code.default_level"
        );
        assert_eq!(changes[0].op, ChangeOp::Replace);
    }

    #[test]
    fn key_added_emits_add_op() {
        let before = json!({"a": 1});
        let after = json!({"a": 1, "b": 2});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "b");
        assert_eq!(changes[0].op, ChangeOp::Add);
        assert_eq!(changes[0].before, None);
        assert_eq!(changes[0].after, Some(json!(2)));
    }

    #[test]
    fn key_removed_emits_remove_op() {
        let before = json!({"a": 1, "b": 2});
        let after = json!({"a": 1});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "b");
        assert_eq!(changes[0].op, ChangeOp::Remove);
        assert_eq!(changes[0].before, Some(json!(2)));
        assert_eq!(changes[0].after, None);
    }

    #[test]
    fn array_index_replace_uses_brackets() {
        let before = json!({"widgets": [{"id": "a"}, {"id": "b"}]});
        let after = json!({"widgets": [{"id": "a"}, {"id": "B"}]});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "widgets[1].id");
        assert_eq!(changes[0].op, ChangeOp::Replace);
    }

    #[test]
    fn array_extension_emits_add_for_new_indices() {
        let before = json!([1, 2]);
        let after = json!([1, 2, 3]);
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "[2]");
        assert_eq!(changes[0].op, ChangeOp::Add);
        assert_eq!(changes[0].after, Some(json!(3)));
    }

    #[test]
    fn array_truncation_emits_remove_for_dropped_indices() {
        let before = json!([1, 2, 3]);
        let after = json!([1, 2]);
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "[2]");
        assert_eq!(changes[0].op, ChangeOp::Remove);
    }

    #[test]
    fn type_mismatch_emits_replace_with_both_values() {
        let before = json!({"x": 5});
        let after = json!({"x": "five"});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ChangeOp::Replace);
        assert_eq!(changes[0].before, Some(json!(5)));
        assert_eq!(changes[0].after, Some(json!("five")));
    }

    #[test]
    fn multiple_independent_changes_emitted_in_sorted_order() {
        let before = json!({
            "z": "old-z",
            "a": "old-a",
            "m": "old-m",
        });
        let after = json!({
            "z": "new-z",
            "a": "new-a",
            "m": "new-m",
        });
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 3);
        // BTree iteration order: a < m < z.
        assert_eq!(changes[0].path, "a");
        assert_eq!(changes[1].path, "m");
        assert_eq!(changes[2].path, "z");
    }

    #[test]
    fn registry_weight_change_is_a_single_path() {
        // Realistic registry edit: operator drags one weight slider.
        let before = json!({
            "config": {
                "domain_weights": {
                    "test-health": 0.4,
                    "code-quality": 0.6
                }
            }
        });
        let after = json!({
            "config": {
                "domain_weights": {
                    "test-health": 0.5,
                    "code-quality": 0.5
                }
            }
        });
        let changes = diff(&before, &after);
        // Both weights changed → two entries.
        assert_eq!(changes.len(), 2);
        let paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
        assert!(paths.contains(&"config.domain_weights.code-quality"));
        assert!(paths.contains(&"config.domain_weights.test-health"));
    }

    #[test]
    fn truncation_caps_at_max_changes() {
        // Build a synthetic case with > MAX_CHANGES distinct changes.
        let mut b_obj = serde_json::Map::new();
        let mut a_obj = serde_json::Map::new();
        for i in 0..(MAX_CHANGES + 50) {
            b_obj.insert(format!("k{i:04}"), json!(format!("old-{i}")));
            a_obj.insert(format!("k{i:04}"), json!(format!("new-{i}")));
        }
        let changes = diff(&Value::Object(b_obj), &Value::Object(a_obj));
        assert_eq!(changes.len(), MAX_CHANGES);
    }

    #[test]
    fn root_scalar_replace_uses_dollar_path() {
        let before = json!(5);
        let after = json!("five");
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "$");
    }

    #[test]
    fn null_to_value_is_a_replace_not_an_add() {
        // JSON null is a real value, distinct from key-absent.
        // Changing a value from null → x should be a replace at the
        // existing key.
        let before = json!({"x": null});
        let after = json!({"x": 1});
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "x");
        assert_eq!(changes[0].op, ChangeOp::Replace);
        assert_eq!(changes[0].before, Some(Value::Null));
    }
}
