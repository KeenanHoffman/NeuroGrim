use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A health score in the range [0, 100].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score(u8);

impl Score {
    /// Create a score, clamping to [0, 100].
    pub fn new(value: i64) -> Self {
        Score(value.clamp(0, 100) as u8)
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn zero() -> Self {
        Score(0)
    }
}

impl From<Score> for u8 {
    fn from(s: Score) -> u8 {
        s.0
    }
}

impl From<Score> for f64 {
    fn from(s: Score) -> f64 {
        s.0 as f64
    }
}

/// Confidence level in the range [0, 100].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confidence(u8);

impl Confidence {
    pub fn new(value: f64) -> Self {
        Confidence(value.round().clamp(0.0, 100.0) as u8)
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn zero() -> Self {
        Confidence(0)
    }

    pub fn full() -> Self {
        Confidence(100)
    }
}

/// A domain weight in the range [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Weight(f64);

impl Weight {
    pub fn new(value: f64) -> Self {
        Weight(value.clamp(0.0, 1.0))
    }

    pub fn value(&self) -> f64 {
        self.0
    }

    pub fn is_advisory(&self) -> bool {
        self.0 == 0.0
    }
}

/// Computed scores for a single domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainScore {
    pub domain: String,
    pub raw_score: Score,
    pub confidence: Confidence,
    pub effective_score: Score,
    pub weight: Weight,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trajectory: Option<TrajectoryResult>,
}

/// The complete scorecard produced by the Brain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scorecard {
    pub unified_score: Score,
    pub domains: HashMap<String, DomainScore>,
    pub scored_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor_applied: Option<FloorApplication>,
}

/// Indicates a domain floor constraint was applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorApplication {
    pub domain: String,
    pub domain_score: u8,
    pub min_score: u8,
    pub unified_cap: u8,
    pub message: String,
}

/// Trajectory analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryResult {
    pub velocity: f64,
    pub acceleration: f64,
    pub classification: TrajectoryClassification,
    pub samples: usize,
}

/// Trajectory classification (evaluated in order per spec Section 7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrajectoryClassification {
    NoData,
    Volatile,
    Improving,
    Degrading,
    Stable,
}

/// Autonomy levels (ordered from least to most restrictive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutonomyLevel {
    Auto,
    Notify,
    Approve,
    Blocked,
}

/// Gate status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Clean,
    Dirty,
    NeedsRun,
    Stale,
}

/// Score history snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSnapshot {
    pub scored_at: DateTime<Utc>,
    pub score: u8,
    pub domains: HashMap<String, SnapshotDomain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDomain {
    pub score: u8,
    pub confidence: u8,
}

/// Scoring model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScoringModel {
    Multiplier,
    Floor,
}

impl Default for ScoringModel {
    fn default() -> Self {
        ScoringModel::Multiplier
    }
}

/// Score label for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreLabel {
    Green,
    Yellow,
    Red,
}

impl ScoreLabel {
    pub fn from_score(score: u8, yellow_threshold: u8, red_threshold: u8) -> Self {
        if score >= yellow_threshold {
            ScoreLabel::Green
        } else if score >= red_threshold {
            ScoreLabel::Yellow
        } else {
            ScoreLabel::Red
        }
    }
}
