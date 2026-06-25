//! A8 — System-pressure pre-dispatch subgate + `SystemFactsProvider` trait.
//!
//! Lifts the IDE's `browser/admission.rs` pressure-aware admission control
//! into substrate. The pattern: a `SystemFactsProvider` reads memory / CPU
//! / disk pressure; a `SystemPressureSubgate` reads from the provider and
//! refuses dispatches when pressure exceeds operator-declared thresholds.
//!
//! ## Why a pluggable provider
//!
//! Different deployments need different sources: the IDE uses `sysinfo`
//! (heavyweight Rust crate, ~70 transitive deps); cereGrim's terminal-host
//! might use raw `/proc/meminfo` on Linux; tests inject a static fixture.
//! The substrate stays dep-light by providing the trait + a stub default;
//! operators register a real provider during host construction.
//!
//! ## Pressure tiers
//!
//! - `Healthy` — full speed; no restrictions.
//! - `Watch` — operator may want to throttle spawn-heavy pipelines.
//! - `Critical` — only essential governance pipelines should proceed.
//! - `Refuse` — refuse all non-essential dispatches.
//!
//! The subgate accepts a `min_tier` field: any pipeline gets refused if the
//! current tier is worse than `min_tier`. Operators wire stricter tiers for
//! pipelines that shouldn't fire under load (e.g., browser spawns get
//! `min_tier = Watch` so they refuse at Critical/Refuse).

use crate::governance::{GovernanceRefusal, PreDispatchSubgate};
use crate::pipeline::Pipeline;
use std::sync::Arc;

/// Pressure tiers for `SystemPressureSubgate`. Ordered: Healthy < Watch <
/// Critical < Refuse (worst tier last).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PressureTier {
    Healthy,
    Watch,
    Critical,
    Refuse,
}

impl PressureTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Watch => "watch",
            Self::Critical => "critical",
            Self::Refuse => "refuse",
        }
    }
}

/// A snapshot of system pressure facts. Returned by `SystemFactsProvider`;
/// consumed by `SystemPressureSubgate`.
#[derive(Debug, Clone)]
pub struct SystemFacts {
    pub free_ram_mb: u64,
    pub cpu_load_pct: u8,
    pub tier: PressureTier,
}

impl SystemFacts {
    pub fn healthy() -> Self {
        Self {
            free_ram_mb: u64::MAX,
            cpu_load_pct: 0,
            tier: PressureTier::Healthy,
        }
    }
}

/// Pluggable system-facts source. Operators register a real implementation
/// during host construction; substrate ships a `HealthyDefault` stub.
pub trait SystemFactsProvider: Send + Sync {
    fn read(&self) -> SystemFacts;
}

/// Default provider that always reports healthy — useful for tests and for
/// deployments that don't yet have a real provider wired up. Production
/// deployments should register a real provider (sysinfo, /proc/meminfo, etc.).
pub struct HealthyDefault;

impl SystemFactsProvider for HealthyDefault {
    fn read(&self) -> SystemFacts {
        SystemFacts::healthy()
    }
}

/// Pre-dispatch subgate that refuses dispatches when system pressure is
/// worse than `min_tier`. Pluggable provider lets the substrate stay dep-
/// light; operators wire a real provider.
pub struct SystemPressureSubgate {
    name: String,
    min_tier: PressureTier,
    provider: Arc<dyn SystemFactsProvider>,
}

impl SystemPressureSubgate {
    pub fn new(
        name: impl Into<String>,
        min_tier: PressureTier,
        provider: Arc<dyn SystemFactsProvider>,
    ) -> Self {
        Self {
            name: name.into(),
            min_tier,
            provider,
        }
    }
}

impl PreDispatchSubgate for SystemPressureSubgate {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, _pipeline: &Pipeline) -> Result<(), GovernanceRefusal> {
        let facts = self.provider.read();
        if facts.tier > self.min_tier {
            return Err(GovernanceRefusal::Subgate {
                name: self.name.clone(),
                reason: format!(
                    "system pressure `{}` exceeds min_tier `{}` (free_ram={}MB, cpu_load={}%)",
                    facts.tier.as_str(),
                    self.min_tier.as_str(),
                    facts.free_ram_mb,
                    facts.cpu_load_pct
                ),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};
    use std::sync::Mutex;

    struct StubProvider {
        tier: Mutex<PressureTier>,
    }
    impl StubProvider {
        fn new(tier: PressureTier) -> Self {
            Self {
                tier: Mutex::new(tier),
            }
        }
        fn set(&self, tier: PressureTier) {
            *self.tier.lock().unwrap() = tier;
        }
    }
    impl SystemFactsProvider for StubProvider {
        fn read(&self) -> SystemFacts {
            SystemFacts {
                free_ram_mb: 1024,
                cpu_load_pct: 10,
                tier: *self.tier.lock().unwrap(),
            }
        }
    }

    fn make_pipeline() -> Pipeline {
        Pipeline {
            id: "t/x".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::WorldEffect,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        }
    }

    #[test]
    fn pressure_tier_ordering() {
        assert!(PressureTier::Healthy < PressureTier::Watch);
        assert!(PressureTier::Watch < PressureTier::Critical);
        assert!(PressureTier::Critical < PressureTier::Refuse);
    }

    #[test]
    fn healthy_default_provider_always_passes() {
        let subgate = SystemPressureSubgate::new(
            "test-pressure",
            PressureTier::Healthy,
            Arc::new(HealthyDefault),
        );
        subgate.check(&make_pipeline()).unwrap();
    }

    #[test]
    fn subgate_passes_when_tier_at_or_below_min_tier() {
        let provider = Arc::new(StubProvider::new(PressureTier::Watch));
        let subgate = SystemPressureSubgate::new(
            "test-pressure",
            PressureTier::Watch,
            provider.clone(),
        );
        subgate.check(&make_pipeline()).unwrap();
        provider.set(PressureTier::Healthy);
        subgate.check(&make_pipeline()).unwrap();
    }

    #[test]
    fn subgate_refuses_when_tier_exceeds_min_tier() {
        let provider = Arc::new(StubProvider::new(PressureTier::Critical));
        let subgate = SystemPressureSubgate::new(
            "test-pressure",
            PressureTier::Watch,
            provider.clone(),
        );
        let err = subgate.check(&make_pipeline()).unwrap_err();
        match err {
            GovernanceRefusal::Subgate { name, reason } => {
                assert_eq!(name, "test-pressure");
                assert!(reason.contains("system pressure"));
                assert!(reason.contains("critical"));
            }
            other => panic!("expected Subgate refusal, got {:?}", other),
        }
        // Recovery: provider switches to Healthy → subgate allows again.
        provider.set(PressureTier::Healthy);
        subgate.check(&make_pipeline()).unwrap();
    }
}
