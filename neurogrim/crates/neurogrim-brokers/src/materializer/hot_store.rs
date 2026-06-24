//! BB #22 — Hot-Store Materializer.
//!
//! Writes per-broker Overlay state to
//! `.claude/brain/broker/segments/overlay-<broker_id>.md`. Composed by
//! Materializer Composer (BB #22a) into `current-projection.md`.
//!
//! ## Wave 0 scaffold; Wave 3 implements.

use std::path::PathBuf;

pub struct HotStoreMaterializer {
    _broker_id: String,
    _segment_path: PathBuf,
}

impl HotStoreMaterializer {
    pub fn new(broker_id: String, segment_path: PathBuf) -> Self {
        Self {
            _broker_id: broker_id,
            _segment_path: segment_path,
        }
    }

    /// Materialize the broker's Overlay state to its segment file.
    /// Wave 3 implements.
    pub fn materialize(&self, _overlay_json: serde_json::Value) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "HotStoreMaterializer::materialize not yet implemented (Wave 3)"
        ))
    }
}
