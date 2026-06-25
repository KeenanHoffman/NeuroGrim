//! D3 / BB #20 — Skill Filter primitive (topic-filtered materialization).
//!
//! Pluggable `SegmentRanker` trait that the Materializer Composer can
//! consult to pick the top-K segments by relevance instead of including
//! all of them. The proper answer to materializer scale at very large
//! broker counts; A10's per-broker budget allocation is the substrate-
//! default fallback when no ranker is registered.
//!
//! ## V0 MVP scope
//!
//! - **`SegmentRanker`** trait: takes a list of `(name, body)` segments +
//!   a `MaterializerContext` (hat / posture / current task) and returns a
//!   ranked subset.
//! - **`NoOpRanker`** — substrate-shipped default that returns all segments
//!   in input order (no filtering, no ranking). Composer uses this when no
//!   ranker is registered.
//! - Operators / consuming projects (cereGrim, IDE) ship their own ranker
//!   implementations that consult their hat system or session context.
//!
//! The Composer's integration with the ranker is intentionally OPTIONAL:
//! existing compose() behavior is unchanged; consumers opt in by passing
//! `Some(Arc<dyn SegmentRanker>)` to a new builder method
//! (`MaterializerComposer::with_ranker()`). The plan defers full BB #20
//! integration to S2-T; this V0 lands the primitive so the IDE has a
//! place to wire its hat-aware classifier when ready.

use std::sync::Arc;

/// Context the ranker considers when choosing which segments to include.
/// V0 keeps it minimal; S1-T adds hat / posture / session / risk_appetite.
#[derive(Debug, Clone, Default)]
pub struct RankerContext {
    /// Operator-declared current hat (architect / debugger / pm / ...) per
    /// the hat-system convention. Empty if not set.
    pub current_hat: String,
    /// Free-form posture hint (e.g., "exploratory", "tight-feedback-loop").
    pub posture: String,
    /// Optional current-task description (the agent's working context).
    pub current_task: String,
}

/// A segment that may be included in the projection. The ranker decides
/// which subset to keep + in what order.
#[derive(Debug, Clone)]
pub struct CandidateSegment {
    pub name: String,
    pub body: String,
}

/// Pluggable segment-ranking trait. Operators / consuming projects ship
/// implementations that consult their session context to pick top-K.
pub trait SegmentRanker: Send + Sync {
    fn rank<'a>(
        &self,
        segments: &'a [CandidateSegment],
        ctx: &RankerContext,
    ) -> Vec<&'a CandidateSegment>;
}

/// Substrate-shipped default ranker: returns all segments in input order.
/// Equivalent to "no ranking applied"; lets the Materializer Composer
/// call into a ranker uniformly without conditional logic.
pub struct NoOpRanker;

impl SegmentRanker for NoOpRanker {
    fn rank<'a>(
        &self,
        segments: &'a [CandidateSegment],
        _ctx: &RankerContext,
    ) -> Vec<&'a CandidateSegment> {
        segments.iter().collect()
    }
}

/// Type alias for the optional ranker the Composer may consult.
pub type SharedRanker = Arc<dyn SegmentRanker>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_ranker_returns_all_segments_in_order() {
        let segments = vec![
            CandidateSegment {
                name: "a".to_string(),
                body: "alpha".to_string(),
            },
            CandidateSegment {
                name: "b".to_string(),
                body: "beta".to_string(),
            },
            CandidateSegment {
                name: "c".to_string(),
                body: "gamma".to_string(),
            },
        ];
        let ranked = NoOpRanker.rank(&segments, &RankerContext::default());
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].name, "a");
        assert_eq!(ranked[1].name, "b");
        assert_eq!(ranked[2].name, "c");
    }

    /// Sanity check: a custom ranker can filter + reorder.
    #[test]
    fn custom_ranker_can_filter_and_reorder() {
        struct OnlyEvenIndexRanker;
        impl SegmentRanker for OnlyEvenIndexRanker {
            fn rank<'a>(
                &self,
                segments: &'a [CandidateSegment],
                _ctx: &RankerContext,
            ) -> Vec<&'a CandidateSegment> {
                segments
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % 2 == 0)
                    .map(|(_, s)| s)
                    .rev()
                    .collect()
            }
        }
        let segments: Vec<_> = (0..4)
            .map(|i| CandidateSegment {
                name: format!("seg-{}", i),
                body: format!("body-{}", i),
            })
            .collect();
        let ranked = OnlyEvenIndexRanker.rank(&segments, &RankerContext::default());
        assert_eq!(ranked.len(), 2);
        // Reversed even-index: seg-2 first, then seg-0
        assert_eq!(ranked[0].name, "seg-2");
        assert_eq!(ranked[1].name, "seg-0");
    }

    #[test]
    fn ranker_context_carries_hat_and_posture() {
        let ctx = RankerContext {
            current_hat: "architect".to_string(),
            posture: "exploratory".to_string(),
            current_task: "design broker integration".to_string(),
        };
        struct AssertingRanker;
        impl SegmentRanker for AssertingRanker {
            fn rank<'a>(
                &self,
                segments: &'a [CandidateSegment],
                ctx: &RankerContext,
            ) -> Vec<&'a CandidateSegment> {
                assert_eq!(ctx.current_hat, "architect");
                assert_eq!(ctx.posture, "exploratory");
                segments.iter().collect()
            }
        }
        AssertingRanker.rank(&[], &ctx);
    }
}
