//! BB #2a `Overlay<T>` + BB #2b `WorkingState<W>`.
//!
//! The Overlay is the broker's read-only consumer-facing projection of its
//! working state. Per BROKER-CONTRACT.md §"The Overlay contract": atomic-swap
//! updates, versioned read, no-torn-read enforcement.
//!
//! ## Implementation (Wave 1)
//!
//! BROKER-SPEC-GAPS.md gap #5 resolution: `arc-swap` crate provides the
//! atomic-swap primitive. The broker (single writer) calls `swap()` to publish
//! a new Overlay; consumers (many readers) call `load()` to get a snapshot.
//! Consumers' loaded snapshots are reference-counted and survive subsequent
//! swaps — no torn reads possible.
//!
//! ## Working state vs Overlay (per spec)
//!
//! - **`Overlay<T>`** — read-only consumer-facing. Atomic-swap publish from
//!   broker; reference-counted snapshot read from consumer.
//! - **`WorkingState<W>`** — broker-private full read/write surface. Never
//!   exposed to consumers. Holds the broker's accumulator state (loaded
//!   catalog, workflow positions, skill-filter weight cache, rate-limit
//!   counters, etc.).
//!
//! Wave 1 implements both with arc-swap-backed Overlay + simple
//! Arc<Mutex<W>>-backed WorkingState.

use std::sync::Arc;

/// Read-only consumer-facing projection with atomic-swap updates.
/// Per BB #2a Overlay primitive.
pub struct Overlay<T> {
    inner: arc_swap::ArcSwap<T>,
}

impl<T> Overlay<T> {
    /// Create a new Overlay with the given initial state.
    pub fn new(initial: T) -> Self {
        Self {
            inner: arc_swap::ArcSwap::from_pointee(initial),
        }
    }

    /// Atomically swap the current Overlay state with `new`. Existing reader
    /// snapshots are unaffected; new readers see `new`.
    pub fn swap(&self, new: T) {
        self.inner.store(Arc::new(new));
    }

    /// Read a snapshot of the current Overlay state. The snapshot survives
    /// subsequent swaps (Arc reference-counted).
    pub fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }
}

/// A read guard over an Overlay snapshot. Wave 1 may extend this for
/// version-tracking + curation-budget telemetry per spec.
pub type OverlayReadGuard<T> = Arc<T>;

/// Broker-private full read/write state. Per BB #2b WorkingState.
/// Wave 1 wraps Arc<Mutex<W>> with broker-internal access only.
pub struct WorkingState<W> {
    inner: Arc<tokio::sync::Mutex<W>>,
}

impl<W> WorkingState<W> {
    pub fn new(initial: W) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(initial)),
        }
    }

    /// Acquire a write lock on the working state. Broker-internal only;
    /// consumers must not call this.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, W> {
        self.inner.lock().await
    }
}

/// Tier-2 derived/filtered projection over a base [`Overlay<T>`].
///
/// Per BROKER-CONTRACT.md Glossary, an `OverlayView` is the per-consumer
/// projection a multi-tenant broker hands out: each consumer sees a
/// different filtered view of the same base Overlay, computed by applying
/// the consumer-specific filter closure to the current snapshot.
///
/// The base Overlay is unmodified — multiple `OverlayView`s can share the
/// same base concurrently, each with its own filter. Filters run at
/// `load()` time (no caching); the snapshot semantics inherit from the
/// base (no torn reads).
///
/// ## Use cases
///
/// - **Multi-tenant broker per-consumer ACL** — a Topology Broker hands
///   each caller a different OverlayView whose filter retains only the
///   broker entries the caller is permitted to discover.
/// - **Per-clearance-level projection** — a sensitive-data broker exposes
///   an OverlayView per clearance tier whose filter elides or summarizes
///   high-sensitivity fields.
/// - **Per-role observability filters** — a Meta-vs-Primary lobe-specific
///   view of a Sense broker's overlay (Meta sees the full sense data;
///   Primary sees a token-budgeted summary).
///
/// The base `Overlay<T>` stays the single source of truth; the View is a
/// cheap derived projection. There is no inverse — Views are read-only.
pub struct OverlayView<T, U> {
    base: Arc<Overlay<T>>,
    filter: Box<dyn Fn(&T) -> U + Send + Sync>,
}

impl<T, U> OverlayView<T, U>
where
    T: Send + Sync + 'static,
    U: Send + Sync + 'static,
{
    /// Construct a new View over `base` that applies `filter` on each
    /// `load()`. The filter must be deterministic per-snapshot (same `&T`
    /// → same `U`) so callers can reason about View stability across
    /// reads; the substrate does not enforce this beyond the type signature.
    pub fn new(
        base: Arc<Overlay<T>>,
        filter: impl Fn(&T) -> U + Send + Sync + 'static,
    ) -> Self {
        Self {
            base,
            filter: Box::new(filter),
        }
    }

    /// Load the current Overlay snapshot + apply the filter. Returns the
    /// projected value `U` by-value (Views are read-only; callers receive
    /// owned data, not a reference into the base).
    pub fn load(&self) -> U {
        let snap = self.base.load();
        (self.filter)(&snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::Arc;

    #[test]
    fn overlay_swap_publishes_new_state() {
        let o = Overlay::new(vec![1, 2, 3]);
        let snap1 = o.load();
        o.swap(vec![4, 5, 6]);
        let snap2 = o.load();
        assert_eq!(*snap1, vec![1, 2, 3]); // snap1 unaffected
        assert_eq!(*snap2, vec![4, 5, 6]); // snap2 sees new state
    }

    proptest! {
        /// Property: under arbitrary concurrent reads + writes, every read
        /// returns ONE of the published versions (no torn reads).
        #[test]
        fn overlay_no_torn_reads_under_concurrent_access(
            values in proptest::collection::vec(0i32..1000, 1..50)
        ) {
            let o = Arc::new(Overlay::new(vec![0i32]));
            let writer_handles: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let o = o.clone();
                    std::thread::spawn(move || {
                        o.swap(vec![v; i + 1]);
                    })
                })
                .collect();

            let reader_handles: Vec<_> = (0..20)
                .map(|_| {
                    let o = o.clone();
                    std::thread::spawn(move || {
                        for _ in 0..50 {
                            let snap = o.load();
                            // No-torn-read invariant: every element of the
                            // snapshot equals the first element (since each
                            // write publishes a vector where all elements
                            // are identical). If snap is torn (mixed from
                            // two writes), this would fail.
                            if !snap.is_empty() {
                                let first = snap[0];
                                prop_assert!(snap.iter().all(|&x| x == first),
                                    "torn read detected: {:?}", *snap);
                            }
                        }
                        Ok(())
                    })
                })
                .collect();

            for h in writer_handles {
                h.join().unwrap();
            }
            for h in reader_handles {
                h.join().unwrap().unwrap();
            }
        }
    }

    #[test]
    fn overlay_view_applies_filter_to_snapshot() {
        let base = Arc::new(Overlay::new(vec![1u32, 2, 3, 4, 5]));
        let view: OverlayView<Vec<u32>, Vec<u32>> = OverlayView::new(
            base.clone(),
            |snap| snap.iter().filter(|&&x| x % 2 == 0).copied().collect(),
        );
        assert_eq!(view.load(), vec![2, 4]);
        // Mutating base reflects in subsequent loads
        base.swap(vec![10, 11, 12, 13]);
        assert_eq!(view.load(), vec![10, 12]);
    }

    #[test]
    fn overlay_view_multiple_views_share_base() {
        let base = Arc::new(Overlay::new((vec!["a", "b", "c"], 42u32)));
        let names_view: OverlayView<(Vec<&'static str>, u32), Vec<&'static str>> =
            OverlayView::new(base.clone(), |snap| snap.0.clone());
        let count_view: OverlayView<(Vec<&'static str>, u32), u32> =
            OverlayView::new(base.clone(), |snap| snap.1);
        assert_eq!(names_view.load(), vec!["a", "b", "c"]);
        assert_eq!(count_view.load(), 42);
    }

    #[test]
    fn overlay_view_filter_runs_each_load() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let base = Arc::new(Overlay::new(0u32));
        let calls = Arc::new(AtomicU32::new(0));
        let calls_for_filter = calls.clone();
        let view: OverlayView<u32, u32> = OverlayView::new(base, move |&n| {
            calls_for_filter.fetch_add(1, Ordering::SeqCst);
            n * 2
        });
        let _ = view.load();
        let _ = view.load();
        let _ = view.load();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "filter must run on every load (no caching)"
        );
    }

    #[tokio::test]
    async fn working_state_lock_serializes_writes() {
        let ws = Arc::new(WorkingState::new(0i32));
        let tasks: Vec<_> = (0..100)
            .map(|_| {
                let ws = ws.clone();
                tokio::spawn(async move {
                    let mut guard = ws.lock().await;
                    *guard += 1;
                })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }
        // If the Mutex was broken, increments would race + total < 100.
        assert_eq!(*ws.lock().await, 100);
    }
}
