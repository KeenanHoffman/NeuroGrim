//! BB #22a — Materializer Composer.
//!
//! Concatenates per-broker materializer segment files in operator-declared
//! order into `current-projection.md`. Governance segment FIRST regardless of
//! operator order (R-O-3 closure, Untunable; per ultra-pass U2 transparency
//! requirement, BB #32 Telemetry projects composition-state line).
//!
//! ## Submodules
//!
//! - [`hot_store`] — BB #22: per-broker Overlay → segment file.
//! - [`awareness`] — BB #24: per-broker pipeline catalog → segment file with
//!   description + when_to_use + currently-legal status + per-pipeline
//!   parameter schema (per ultra-pass U1).

pub mod hot_store;
pub mod awareness;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MaterializerError {
    #[error("segment file write failed: {0}")]
    WriteFailed(#[from] std::io::Error),

    #[error("composition budget exceeded: {0}")]
    BudgetExceeded(String),
}

/// Materializer Composer — Wave 3 implements.
pub struct MaterializerComposer {
    _output_path: PathBuf,
    _segments_dir: PathBuf,
}

impl MaterializerComposer {
    pub fn new(output_path: PathBuf, segments_dir: PathBuf) -> Self {
        Self {
            _output_path: output_path,
            _segments_dir: segments_dir,
        }
    }

    /// Compose all segments into `current-projection.md`. Governance-first
    /// override (R-O-3 closure) applied: governance segment placed first
    /// regardless of operator's declared `materializer_composition_order`.
    /// Wave 3 implements.
    pub fn compose(&self, _order: &[String]) -> Result<(), MaterializerError> {
        Err(MaterializerError::WriteFailed(std::io::Error::other(
            "MaterializerComposer::compose not yet implemented (Wave 3)",
        )))
    }
}
