//! `typosquat_proximity` sensor — Phase 2.
//!
//! Levenshtein distance ≤ 1 to a top-1000 popular package on the
//! same registry. Flags packages that are one edit away from a
//! popular name AND not themselves in the top-1000.
//!
//! Static asset: top-1000 lists are compiled into the binary from
//! `data/typosquat-popular-<ecosystem>.txt`. Refreshed quarterly via
//! `scripts/refresh-typosquat-list.sh`. Lists are small (~10kB
//! each) so compile-in cost is negligible.

use crate::supply_chain_sca::Package;
use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
use serde_json::json;

use super::scoring::{VigilanceFinding, VigilanceKind};

/// Top-1000 popular crates from crates.io, refreshed quarterly.
/// Source: crates.io database dump, sorted by total download count.
const POPULAR_CRATES_IO: &str = include_str!("../../data/typosquat-popular-cratesio.txt");

/// Top-1000 popular packages on PyPI.
/// Source: TopPyPI dataset.
const POPULAR_PYPI: &str = include_str!("../../data/typosquat-popular-pypi.txt");

/// Top-1000 popular packages on npm.
/// Source: nodejs-public-rankings.
const POPULAR_NPM: &str = include_str!("../../data/typosquat-popular-npm.txt");

pub fn scan(packages: &[Package]) -> Vec<VigilanceFinding> {
    let mut findings = Vec::new();

    // Pre-parse the popular lists once per ecosystem.
    let popular_by_ecosystem: std::collections::HashMap<&'static str, Vec<&str>> = [
        (ECOSYSTEM_CRATES_IO, parse_list(POPULAR_CRATES_IO)),
        (ECOSYSTEM_PYPI, parse_list(POPULAR_PYPI)),
        (ECOSYSTEM_NPM, parse_list(POPULAR_NPM)),
    ]
    .into_iter()
    .collect();

    // Dedup by (ecosystem, name).
    let mut seen: std::collections::HashSet<(String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.ecosystem)) {
            continue;
        }
        let Some(popular) = popular_by_ecosystem.get(pkg.ecosystem) else {
            continue;
        };

        // Skip if THIS package is itself a top-1000 entry — that's
        // the canonical name, not a typosquat.
        let lowered = pkg.name.to_ascii_lowercase();
        if popular.iter().any(|p| p.eq_ignore_ascii_case(&lowered)) {
            continue;
        }

        // Find the closest popular name within Levenshtein ≤ 1.
        if let Some((target, _distance)) = closest_within_one(&lowered, popular) {
            findings.push(VigilanceFinding {
                kind: VigilanceKind::TyposquatProximity,
                package: pkg.clone(),
                summary: format!(
                    "name is one edit from popular package '{}' (ecosystem: {})",
                    target, pkg.ecosystem
                ),
                evidence: Some(json!({
                    "candidate_typosquat_target": target,
                    "levenshtein_distance": 1,
                })),
                confidence: 0.7,
            });
        }
    }

    findings
}

fn parse_list(raw: &str) -> Vec<&str> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Return the closest popular name within Levenshtein distance 1,
/// or `None` if no popular name is within that bound.
fn closest_within_one<'a>(name: &str, popular: &[&'a str]) -> Option<(&'a str, usize)> {
    let name_lower = name.to_ascii_lowercase();
    for &candidate in popular {
        if let Some(d) = compute_levenshtein_le(name_lower.as_bytes(), candidate.as_bytes(), 1) {
            if d == 1 {
                return Some((candidate, d));
            }
        }
    }
    None
}

/// Levenshtein distance, early-aborting if it would exceed `max`.
/// Returns `Some(d)` if d ≤ max, `None` if d > max.
///
/// Hand-rolled to avoid an extra crate dependency.

fn compute_levenshtein_le(a: &[u8], b: &[u8], max: usize) -> Option<usize> {
    // Quick reject: if the length difference exceeds max, distance > max.
    if a.len().abs_diff(b.len()) > max {
        return None;
    }

    // Standard DP table, but we early-abort when no row entry is ≤ max.
    let n = a.len();
    let m = b.len();
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];

    for i in 1..=n {
        curr[0] = i;
        let mut row_min = curr[0];
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
            if curr[j] < row_min {
                row_min = curr[j];
            }
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    let d = prev[m];
    if d <= max {
        Some(d)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_basic() {
        assert_eq!(compute_levenshtein_le(b"kitten", b"kitten", 5), Some(0));
        assert_eq!(compute_levenshtein_le(b"kitten", b"sitten", 5), Some(1));
        assert_eq!(compute_levenshtein_le(b"kitten", b"sittin", 5), Some(2));
        // Early abort: max=1 should reject distance-3.
        assert_eq!(compute_levenshtein_le(b"kitten", b"foobar", 1), None);
    }

    #[test]
    fn levenshtein_length_diff_quick_reject() {
        // Very different lengths → quick reject without DP.
        assert_eq!(compute_levenshtein_le(b"a", b"abcdef", 1), None);
    }

    #[test]
    fn closest_within_one_finds_typosquat() {
        let popular = vec!["serde", "tokio", "reqwest"];
        let result = closest_within_one("serdee", &popular);
        assert_eq!(result, Some(("serde", 1)));
    }

    #[test]
    fn closest_within_one_skips_self() {
        // "serde" vs popular list including "serde" — distance 0,
        // not 1, so this returns None.
        let popular = vec!["serde", "tokio"];
        let result = closest_within_one("serde", &popular);
        assert!(result.is_none()); // self has distance 0, we only flag on distance 1
    }

    #[test]
    fn closest_within_one_no_match() {
        let popular = vec!["serde", "tokio", "reqwest"];
        let result = closest_within_one("totally-unrelated", &popular);
        assert!(result.is_none());
    }

    #[test]
    fn case_insensitive_match() {
        let popular = vec!["serde", "tokio"];
        let result = closest_within_one("SERDEE", &popular);
        assert_eq!(result, Some(("serde", 1)));
    }
}
