//! Built-in [`crate::scoring_source::ScoringSource`] impls + factories
//! that live in `neurogrim-core` (V5-MOD-1 Phase 2, 2026-05-02).
//!
//! Two source types ship from core:
//!
//! - **`cmdb`** — reads a JSON CMDB file under the project root.
//!   Honors `path`, `score_field`, `updated_at_field`, optional
//!   `confidence` envelope field. Strips UTF-8 BOM (PowerShell
//!   writes BOM by default with `-Encoding UTF8`). See [`cmdb`].
//!
//! - **`function`** — no-op. The "function" source type marks
//!   domains whose scoring is implementation-specific and handled
//!   elsewhere in the pipeline (not via a `CmdbData` envelope).
//!   The factory exists so the source-type is *known* to the
//!   registry; calling its `load()` returns `None` (the caller
//!   then falls through to its own scoring path). See [`function`].
//!
//! The third built-in source type (`a2a` — fractal composition
//! via peer A2A invocation) lives in `neurogrim-ecosystem`
//! (`A2aSource` + `A2aSourceFactory`), where `invoke_child`
//! already lives. Keeping it there avoids forcing
//! `neurogrim-core` to depend on `neurogrim-a2a` (which depends
//! on `neurogrim-core` — would create a cycle). The consuming
//! binary registers the A2A factory at startup via
//! `registry.register(Box::new(neurogrim_ecosystem::scoring_source::A2aSourceFactory))`.

pub mod cmdb;
pub mod function;
