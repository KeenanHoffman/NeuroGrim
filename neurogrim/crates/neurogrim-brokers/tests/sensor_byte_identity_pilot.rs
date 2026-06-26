//! A.2.2 — Pilot sensor byte-identity test (per Gate 4).
//!
//! The Gate 4 acceptance contract: the pilot sensor (`secret-refs`)
//! produces a CMDB envelope via the broker path that is **byte-identical**
//! (with timestamp tolerance) to the same sensor's envelope via the
//! legacy direct-call path. Per Gate 6, this is the template every
//! sensor migration in A.2.4 follows.
//!
//! If this test fails for `secret-refs`, the SensorBackedBroker pattern
//! is broken and A.2.4 bulk migration MUST NOT start.

use neurogrim_brokers::{LeafContext, ParamMap, SensorBackedBroker};
use neurogrim_core::sensor::SensorFactory;
use neurogrim_sensory::sensor_impls::SecretRefsSensorFactory;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper: deep-clone a JSON value with all timestamp-shaped string
/// fields replaced by a sentinel. Lets us compare envelopes for
/// structural equality without timestamp drift causing false failures.
fn strip_timestamps(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                // Strip well-known timestamp keys
                if k == "updated_at" || k == "discovered_at" || k == "ts" {
                    out.insert(k.clone(), serde_json::Value::String("<TS>".into()));
                } else {
                    out.insert(k.clone(), strip_timestamps(val));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(strip_timestamps).collect())
        }
        other => other.clone(),
    }
}

#[tokio::test]
async fn secret_refs_broker_envelope_byte_identical_to_legacy() {
    // Set up a minimal fixture project root. The secret-refs sensor
    // tolerates a missing secrets manifest — its envelope ships with
    // score:0 + a finding noting the absence. Either way, we just need
    // the two paths to produce the same shape.
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path();

    // Path 1: legacy direct-call.
    let legacy_envelope =
        neurogrim_sensory::secret_refs::analyze_secret_refs(project_root.to_str().unwrap()).await;

    // Path 2: broker dispatch.
    let factory: Arc<dyn SensorFactory> = Arc::new(SecretRefsSensorFactory);
    let broker = SensorBackedBroker::new(factory, project_root.to_path_buf());
    let ctx = LeafContext {
        broker_id: broker.id().to_string(),
        pipeline_id: "secret-refs/run-sensor".to_string(),
        params: ParamMap::new(),
        overlay_snapshot: serde_json::Value::Null,
        frame: neurogrim_brokers::Frame::default(),
    };
    // run-sensor; broker stores the envelope in its overlay
    use neurogrim_brokers::Broker;
    broker
        .execute_leaf("run_sensor", ctx)
        .await
        .expect("broker run_sensor should succeed");
    let broker_envelope = broker.read_overlay().await;

    // Byte-identity assertion (with timestamp tolerance).
    let legacy_stripped = strip_timestamps(&legacy_envelope);
    let broker_stripped = strip_timestamps(&broker_envelope);
    assert_eq!(
        legacy_stripped, broker_stripped,
        "secret-refs envelope MUST be byte-identical between legacy + broker paths\n\
         LEGACY: {}\n\
         BROKER: {}",
        serde_json::to_string_pretty(&legacy_envelope).unwrap(),
        serde_json::to_string_pretty(&broker_envelope).unwrap(),
    );
}

/// Verify the broker's CMDB path matches the canonical legacy path
/// `.claude/secret-refs-cmdb.json`. (External CMDB consumers — scoring
/// engine, dashboard, MCP, A2A — depend on this exact path.)
#[test]
fn secret_refs_broker_cmdb_path_matches_canonical() {
    use neurogrim_brokers::Broker;
    let tmp = TempDir::new().unwrap();
    let factory: Arc<dyn SensorFactory> = Arc::new(SecretRefsSensorFactory);
    let broker = SensorBackedBroker::new(factory, tmp.path().to_path_buf());
    let path = broker.cmdb_path().expect("sensor broker must declare cmdb_path");
    assert_eq!(
        path.to_string_lossy(),
        ".claude/secret-refs-cmdb.json",
        "broker cmdb_path MUST equal the legacy sensor's canonical path"
    );
}

/// Verify the broker's role-set is exactly Sense (sensors are observers,
/// not actors).
#[test]
fn secret_refs_broker_role_set_is_sense_only() {
    use neurogrim_brokers::{Broker, Role};
    let tmp = TempDir::new().unwrap();
    let factory: Arc<dyn SensorFactory> = Arc::new(SecretRefsSensorFactory);
    let broker = SensorBackedBroker::new(factory, tmp.path().to_path_buf());
    assert!(broker.role_set().contains(&Role::Sense));
    assert!(!broker.role_set().contains(&Role::InnateAbility));
    assert!(!broker.role_set().contains(&Role::Embodiment));
}
