//! BB #22 — Hot-Store Materializer.
//!
//! Writes per-broker Overlay state to
//! `.claude/brain/broker/segments/overlay-<broker_id>.md`. Composed by
//! Materializer Composer (BB #22a) into `current-projection.md`.

use crate::broker::Broker;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotStoreMatError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),
}

pub struct HotStoreMaterializer {
    broker_id: String,
    segments_dir: PathBuf,
}

impl HotStoreMaterializer {
    pub fn new(broker_id: String, segments_dir: PathBuf) -> Self {
        Self {
            broker_id,
            segments_dir,
        }
    }

    /// Segment file path for this broker's overlay.
    pub fn segment_path(&self) -> PathBuf {
        self.segments_dir
            .join(format!("overlay-{}.md", self.broker_id))
    }

    /// Materialize the broker's current Overlay to its segment file.
    /// Markdown output: heading + fenced JSON block + curated key summary.
    pub async fn materialize(&self, broker: Arc<dyn Broker>) -> Result<(), HotStoreMatError> {
        let overlay = broker.read_overlay().await;
        let pretty = serde_json::to_string_pretty(&overlay)?;

        let body = format!(
            "## `{}` — Overlay state\n\n```json\n{}\n```\n",
            self.broker_id, pretty
        );

        std::fs::create_dir_all(&self.segments_dir)?;
        std::fs::write(self.segment_path(), body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerError, Role, RoleSet, WorldEvent};
    use crate::pipeline::Pipeline;
    use crate::runner::{LeafContext, LeafError};
    use async_trait::async_trait;
    use tempfile::TempDir;

    struct FakeBroker;

    #[async_trait]
    impl Broker for FakeBroker {
        fn id(&self) -> &str {
            "fake"
        }
        fn role_set(&self) -> RoleSet {
            RoleSet::single(Role::Sense)
        }
        async fn read_overlay(&self) -> serde_json::Value {
            serde_json::json!({"counter": 42, "items": [1, 2, 3]})
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
            _: &str,
            _: LeafContext,
        ) -> Result<serde_json::Value, LeafError> {
            Ok(serde_json::Value::Null)
        }
    }

    #[tokio::test]
    async fn materializes_overlay_to_segment_file() {
        let tmp = TempDir::new().unwrap();
        let mat = HotStoreMaterializer::new("fake".to_string(), tmp.path().to_path_buf());
        let broker = Arc::new(FakeBroker);
        mat.materialize(broker).await.unwrap();
        let contents = std::fs::read_to_string(mat.segment_path()).unwrap();
        assert!(contents.contains("`fake` — Overlay state"));
        assert!(contents.contains("\"counter\": 42"));
    }
}
