//! BB #24 — Awareness Materializer.
//!
//! Writes per-broker pipeline catalog routing signals to
//! `.claude/brain/broker/segments/awareness-routing-<broker_id>.md`. Composed
//! by Materializer Composer (BB #22a) into `current-projection.md`.
//!
//! ## MVP scope (Wave 3 per ultra-pass U1 + U3)
//!
//! - **Per-pipeline parameter schema surfaced** (ultra-pass U1 closure) so
//!   the agent has the schema needed to call `dispatch_pipeline` with valid
//!   params. The single MCP tool surface gives no params hints; this segment
//!   is where the agent learns them.
//! - **Stub ranking policy** (ultra-pass U3): unranked; operator-declared
//!   order from the catalog. BB #20 Skill Filter lands properly when the
//!   second broker arrives in S1-T.
//! - **Currently-legal status surfaced** per BROKER-CONTRACT.md central
//!   invariant. Agent sees which pipelines are dispatchable RIGHT NOW.

use crate::broker::Broker;
use crate::pipeline::{Pipeline, Visibility};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AwarenessMatError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct AwarenessMaterializer {
    broker_id: String,
    segments_dir: PathBuf,
}

impl AwarenessMaterializer {
    pub fn new(broker_id: String, segments_dir: PathBuf) -> Self {
        Self {
            broker_id,
            segments_dir,
        }
    }

    pub fn segment_path(&self) -> PathBuf {
        self.segments_dir
            .join(format!("awareness-routing-{}.md", self.broker_id))
    }

    /// Materialize the broker's Surfaced pipelines as a routing-signal segment.
    /// Each pipeline gets: name, description, when_to_use, currently-legal,
    /// parameter schema (the U1 closure: agent needs this for the single
    /// dispatch_pipeline MCP tool).
    pub async fn materialize(&self, broker: Arc<dyn Broker>) -> Result<(), AwarenessMatError> {
        let surfaced = broker
            .legal_pipelines()
            .await
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect::<Vec<_>>();

        let mut body = format!(
            "## `{}` — Awareness routing ({} pipeline{})\n\n",
            self.broker_id,
            surfaced.len(),
            if surfaced.len() == 1 { "" } else { "s" }
        );

        if surfaced.is_empty() {
            body.push_str("_No Surfaced pipelines currently legal._\n");
        } else {
            for p in &surfaced {
                body.push_str(&render_pipeline_signal(p));
            }
        }

        std::fs::create_dir_all(&self.segments_dir)?;
        std::fs::write(self.segment_path(), body)?;
        Ok(())
    }
}

fn render_pipeline_signal(p: &Pipeline) -> String {
    let mut out = format!("### `{}`\n\n", p.id);
    if !p.description.is_empty() {
        out.push_str(&format!("**Description:** {}\n\n", p.description));
    }
    if !p.when_to_use.is_empty() {
        out.push_str(&format!("**When to use:** {}\n\n", p.when_to_use));
    }
    out.push_str("**Currently legal:** yes (preconditions met at projection time)\n\n");
    if !p.preconditions.is_empty() {
        out.push_str("**Preconditions:**\n");
        for pc in &p.preconditions {
            out.push_str(&format!("- `{}`\n", pc));
        }
        out.push('\n');
    }
    // U1 closure: surface the parameter schema so the agent has what it needs
    // to call the single dispatch_pipeline MCP tool with valid params.
    if !p.params.is_null() {
        out.push_str("**Parameters schema:**\n\n```json\n");
        out.push_str(&serde_json::to_string_pretty(&p.params).unwrap_or_default());
        out.push_str("\n```\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerError, Role, RoleSet, WorldEvent};
    use crate::pipeline::{AuditClass, EffectClass, Step, Tunability};
    use crate::runner::{LeafContext, LeafError};
    use async_trait::async_trait;
    use tempfile::TempDir;

    fn make_surfaced(id: &str) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::HotStoreUpdate,
            params: serde_json::json!({
                "type": "object",
                "properties": {
                    "work_unit_id": {"type": "string"}
                },
                "required": ["work_unit_id"]
            }),
            preconditions: vec!["work_unit_exists".to_string()],
            steps: vec![Step::Leaf {
                leaf_op: "claim".to_string(),
            }],
            description: "Claim the next ready work unit.".to_string(),
            when_to_use: "When you're ready to start the next backlog item."
                .to_string(),
        }
    }

    fn make_internal(id: &str) -> Pipeline {
        let mut p = make_surfaced(id);
        p.visibility = Visibility::Internal;
        p
    }

    struct BrokerWithPipelines(Vec<Pipeline>);

    #[async_trait]
    impl Broker for BrokerWithPipelines {
        fn id(&self) -> &str {
            "test"
        }
        fn role_set(&self) -> RoleSet {
            RoleSet::single(Role::InnateAbility)
        }
        async fn read_overlay(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn legal_pipelines(&self) -> Vec<Pipeline> {
            self.0.clone()
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
    async fn materializes_surfaced_pipelines_only() {
        let tmp = TempDir::new().unwrap();
        let mat = AwarenessMaterializer::new("test".to_string(), tmp.path().to_path_buf());
        let broker = Arc::new(BrokerWithPipelines(vec![
            make_surfaced("test/p1"),
            make_internal("test/p2-internal"),
            make_surfaced("test/p3"),
        ]));
        mat.materialize(broker).await.unwrap();
        let contents = std::fs::read_to_string(mat.segment_path()).unwrap();
        assert!(contents.contains("Awareness routing (2 pipelines)"));
        assert!(contents.contains("test/p1"));
        assert!(contents.contains("test/p3"));
        assert!(!contents.contains("test/p2-internal"));
    }

    #[tokio::test]
    async fn surfaces_param_schema_for_agent() {
        let tmp = TempDir::new().unwrap();
        let mat = AwarenessMaterializer::new("test".to_string(), tmp.path().to_path_buf());
        let broker = Arc::new(BrokerWithPipelines(vec![make_surfaced("test/p")]));
        mat.materialize(broker).await.unwrap();
        let contents = std::fs::read_to_string(mat.segment_path()).unwrap();
        // U1 closure: param schema must be in the segment
        assert!(contents.contains("Parameters schema:"));
        assert!(contents.contains("work_unit_id"));
    }
}
