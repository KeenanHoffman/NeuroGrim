//! A.0.1 integration test — BrokerExtensionRegistry round-trip.
//!
//! Verifies that operator-authored TOML extension configs are discovered
//! on disk, applied to the appropriate Extensible broker at host boot,
//! and that schema-version mismatches fail loudly.
//!
//! Test scenarios:
//! 1. Empty extensions dir → host boots normally; no extensions applied.
//! 2. Extension targeting an Extensible broker → broker receives the config
//!    via apply_extension, can downcast + store it.
//! 3. Extension targeting a NON-Extensible broker → host logs warn but
//!    boots successfully (extensions are opt-in convenience).
//! 4. Schema version mismatch → host boot fails with clear error.

use async_trait::async_trait;
use neurogrim_brokers::{
    Broker, BrokerError, BrokerFactoryRegistry, BrokerHost, BrokerHostConfig,
    ExtensionConfig, ExtensionError, Extensible, Pipeline, Role, RoleSet, WorldEvent,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ===== Test fixture broker: implements Extensible + records what's applied =====

#[derive(Debug, Clone, serde::Deserialize)]
struct TestExtensionData {
    label: String,
    value: i64,
}

#[derive(Default)]
#[allow(dead_code)] // some fields read only in specific tests
struct AppliedRecord {
    label: String,
    value: i64,
    schema_version: String,
    source_filename: String,
}

struct TestExtensibleBroker {
    id: String,
    applied: Arc<Mutex<Vec<AppliedRecord>>>,
}

#[async_trait]
impl Broker for TestExtensibleBroker {
    fn id(&self) -> &str {
        &self.id
    }
    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::Sense)
    }
    async fn read_overlay(&self) -> serde_json::Value {
        let applied = self.applied.lock().unwrap();
        serde_json::json!({
            "applied_count": applied.len(),
            "labels": applied.iter().map(|r| r.label.clone()).collect::<Vec<_>>(),
        })
    }
    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        Ok(())
    }
    async fn execute_leaf(
        &self,
        _name: &str,
        _ctx: neurogrim_brokers::LeafContext,
    ) -> Result<serde_json::Value, neurogrim_brokers::LeafError> {
        Ok(serde_json::Value::Null)
    }
    fn as_extensible(&self) -> Option<&dyn Extensible> {
        Some(self)
    }
}

#[async_trait]
impl Extensible for TestExtensibleBroker {
    fn extension_schema_version(&self) -> &str {
        "1"
    }
    async fn apply_extension(&self, config: &ExtensionConfig) -> Result<(), ExtensionError> {
        let data: TestExtensionData = config.deserialize_section("test_data")?;
        let filename = config
            .source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        self.applied.lock().unwrap().push(AppliedRecord {
            label: data.label,
            value: data.value,
            schema_version: config.schema_version.clone(),
            source_filename: filename,
        });
        Ok(())
    }
}

// ===== Test fixture broker: NOT Extensible (uses default as_extensible) =====

struct TestPlainBroker {
    id: String,
}

#[async_trait]
impl Broker for TestPlainBroker {
    fn id(&self) -> &str {
        &self.id
    }
    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::Sense)
    }
    async fn read_overlay(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        Ok(())
    }
    async fn execute_leaf(
        &self,
        _name: &str,
        _ctx: neurogrim_brokers::LeafContext,
    ) -> Result<serde_json::Value, neurogrim_brokers::LeafError> {
        Ok(serde_json::Value::Null)
    }
    // Note: as_extensible() is NOT overridden — returns None.
}

// ===== Test cluster manifest helpers =====

fn write_cluster_fixtures(
    tmp: &TempDir,
    broker_decls: &[(&str, &str)], // (broker_id, broker_type)
) -> std::path::PathBuf {
    let cluster_path = tmp.path().join("cluster.toml");
    let mut cluster_toml = format!(
        r#"
[cluster]
id = "extension-test"
name = "Extension Round-Trip Test"
brokers_dir = "./"

[cluster.materializer]
composition_order = []
output_path = "{output}"
segments_dir = "{segments}"
context_budget_chars = 16384
"#,
        output = tmp
            .path()
            .join("current-projection.md")
            .display()
            .to_string()
            .replace('\\', "/"),
        segments = tmp
            .path()
            .join("segments")
            .display()
            .to_string()
            .replace('\\', "/"),
    );

    for (broker_id, _broker_type) in broker_decls {
        cluster_toml.push_str(&format!(
            "\n[cluster.brokers.{}]\nmanifest_path = \"{}.toml\"\n",
            broker_id, broker_id
        ));
    }
    std::fs::write(&cluster_path, &cluster_toml).unwrap();

    for (broker_id, broker_type) in broker_decls {
        std::fs::write(
            tmp.path().join(format!("{}.toml", broker_id)),
            format!(
                r#"
[broker]
id = "{}"
name = "{}"
roles = ["sense"]
cold_store_path = "./cold/"
catalog_path = "./catalog.yaml"
broker_type = "{}"
"#,
                broker_id, broker_id, broker_type
            ),
        )
        .unwrap();
    }
    cluster_path
}

fn write_extension_config(
    tmp: &TempDir,
    broker_id: &str,
    filename: &str,
    schema_version: &str,
    label: &str,
    value: i64,
) {
    let ext_dir = tmp.path().join("extensions").join(broker_id);
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(
        ext_dir.join(filename),
        format!(
            r#"
[extension]
schema_version = "{}"
authored_by = "test-fixture"

[test_data]
label = "{}"
value = {}
"#,
            schema_version, label, value
        ),
    )
    .unwrap();
}

fn build_factories(
    extensible_handles: HashMap<String, Arc<Mutex<Vec<AppliedRecord>>>>,
) -> BrokerFactoryRegistry {
    let mut reg = BrokerFactoryRegistry::new();
    // Capture handles in the closure so each constructed broker shares state
    // with the test fixture that wants to assert on it.
    let handles_for_extensible = extensible_handles;
    reg.register(
        "test-extensible",
        Arc::new(move |broker_id: &str, _gov, _root| {
            let applied = handles_for_extensible
                .get(broker_id)
                .cloned()
                .unwrap_or_else(|| Arc::new(Mutex::new(Vec::new())));
            let broker = TestExtensibleBroker {
                id: broker_id.to_string(),
                applied,
            };
            Ok((Arc::new(broker) as Arc<dyn Broker>, vec![]))
        }),
    );
    reg.register(
        "test-plain",
        Arc::new(|broker_id: &str, _gov, _root| {
            let broker = TestPlainBroker {
                id: broker_id.to_string(),
            };
            Ok((Arc::new(broker) as Arc<dyn Broker>, vec![]))
        }),
    );
    reg
}

// ===== Tests =====

#[tokio::test]
async fn empty_extensions_dir_boots_cleanly() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp, &[("test-broker", "test-extensible")]);

    let mut handles = HashMap::new();
    let handle = Arc::new(Mutex::new(Vec::<AppliedRecord>::new()));
    handles.insert("test-broker".to_string(), handle.clone());

    let host = match BrokerHost::boot(
        &cluster_path,
        BrokerHostConfig {
            project_root: None,
            trust_budget_ceiling: 1000,
            broker_factories: build_factories(handles),
        },
    )
    .await
    {
        Ok(h) => h,
        Err(e) => panic!("boot should succeed with no extensions: {}", e),
    };

    assert_eq!(host.bootstrapped().len(), 1);
    assert_eq!(handle.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn extension_applied_to_extensible_broker() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp, &[("test-broker", "test-extensible")]);

    // Two extension configs targeting `test-broker`.
    write_extension_config(&tmp,"test-broker", "alpha.toml", "1", "first", 100);
    write_extension_config(&tmp,"test-broker", "beta.toml", "1", "second", 200);

    let mut handles = HashMap::new();
    let handle = Arc::new(Mutex::new(Vec::<AppliedRecord>::new()));
    handles.insert("test-broker".to_string(), handle.clone());

    if let Err(e) = BrokerHost::boot(
        &cluster_path,
        BrokerHostConfig {
            project_root: None,
            trust_budget_ceiling: 1000,
            broker_factories: build_factories(handles),
        },
    )
    .await
    {
        panic!("boot should succeed with valid extensions: {}", e);
    }

    let applied = handle.lock().unwrap();
    assert_eq!(applied.len(), 2, "both extensions should apply");
    // Deterministic order: alphabetical (alpha before beta).
    assert_eq!(applied[0].label, "first");
    assert_eq!(applied[0].value, 100);
    assert_eq!(applied[0].source_filename, "alpha.toml");
    assert_eq!(applied[1].label, "second");
    assert_eq!(applied[1].value, 200);
    assert_eq!(applied[1].source_filename, "beta.toml");
}

#[tokio::test]
async fn extension_targeting_non_extensible_broker_warns_but_boots() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp, &[("plain-broker", "test-plain")]);

    write_extension_config(&tmp,"plain-broker", "ignored.toml", "1", "ignored", 42);

    let handles = HashMap::new(); // plain broker has no handle

    let host = match BrokerHost::boot(
        &cluster_path,
        BrokerHostConfig {
            project_root: None,
            trust_budget_ceiling: 1000,
            broker_factories: build_factories(handles),
        },
    )
    .await
    {
        Ok(h) => h,
        Err(e) => panic!(
            "boot succeeds even when extensions target non-Extensible brokers: {}",
            e
        ),
    };
    assert_eq!(host.bootstrapped().len(), 1);
    // No applied state to assert — plain broker doesn't track. Boot success is the contract.
}

#[tokio::test]
async fn schema_version_mismatch_fails_boot_loudly() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp, &[("test-broker", "test-extensible")]);

    // Config declares schema_version "99"; broker supports "1".
    write_extension_config(&tmp,"test-broker", "wrong-version.toml", "99", "x", 1);

    let mut handles = HashMap::new();
    let handle = Arc::new(Mutex::new(Vec::<AppliedRecord>::new()));
    handles.insert("test-broker".to_string(), handle.clone());

    let err = BrokerHost::boot(
        &cluster_path,
        BrokerHostConfig {
            project_root: None,
            trust_budget_ceiling: 1000,
            broker_factories: build_factories(handles),
        },
    )
    .await
    .err().expect("boot should fail on schema-version mismatch");

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("schema version mismatch")
            || msg.contains("SchemaVersionMismatch"),
        "expected schema-version mismatch error, got: {}",
        msg
    );
    assert!(msg.contains("99"), "error should cite requested version: {}", msg);
    assert!(msg.contains("\"1\"") || msg.contains("`1`"), "error should cite supported version: {}", msg);
}

#[tokio::test]
async fn malformed_extension_toml_fails_boot_with_path_context() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp, &[("test-broker", "test-extensible")]);

    // Write a malformed TOML file under the broker's extension dir.
    let ext_dir = tmp.path().join("extensions").join("test-broker");
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(ext_dir.join("malformed.toml"), "this is not = valid toml [").unwrap();

    let mut handles = HashMap::new();
    handles.insert(
        "test-broker".to_string(),
        Arc::new(Mutex::new(Vec::<AppliedRecord>::new())),
    );

    let err = BrokerHost::boot(
        &cluster_path,
        BrokerHostConfig {
            project_root: None,
            trust_budget_ceiling: 1000,
            broker_factories: build_factories(handles),
        },
    )
    .await
    .err().expect("boot should fail on malformed extension TOML");

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("malformed.toml"),
        "error should cite the malformed file path, got: {}",
        msg
    );
}

