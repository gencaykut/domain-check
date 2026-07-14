//! Deterministic, network-free generation of pronounceable domain candidates.

use crate::{score_domain, InvestmentScore};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const CONSONANTS: &[u8] = b"bcdfghjklmnprstvwz";
const VOWELS: &[u8] = b"aeiou";
const PATTERNS: &[&str] = &["CVCVC", "CVCVCV", "CVCCVC", "CVCVCVC"];
const SAFE_SYLLABLES: &[&str] = &[
    "al", "ara", "avi", "bel", "bo", "ca", "dari", "do", "ela", "fa", "gali", "ha", "ivo", "ka",
    "lari", "lo", "mari", "mi", "navi", "ne", "ora", "pa", "ravi", "re", "sani", "se", "tavi",
    "te", "uma", "va", "vero", "vi", "za", "zen",
];
const GENERATION_FAMILIES: usize = PATTERNS.len() + 1;
const NEGATIVE_PARTS: &[&str] = &[
    "abuse", "adult", "bitch", "cock", "crime", "cunt", "damn", "death", "dick", "drug", "fraud",
    "fuck", "hate", "idiot", "jail", "kill", "loser", "meth", "moron", "nazi", "ponzi", "porn",
    "rape", "scam", "sex", "shit", "spam", "suck", "vermin",
];

/// Settings for deterministic premium candidate generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateGenerationConfig {
    pub count: usize,
    pub top: usize,
    pub tld: String,
}

/// A generated domain and its explainable local investment score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredCandidate {
    pub domain: String,
    pub scoring: InvestmentScore,
    #[serde(skip)]
    pattern_index: usize,
}

/// Normalize a single TLD (`.COM` -> `com`) and reject unsafe values.
pub fn normalize_tld(tld: &str) -> Option<String> {
    let normalized = tld.trim().trim_start_matches('.').to_ascii_lowercase();
    if (2..=63).contains(&normalized.len())
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !normalized.starts_with('-')
        && !normalized.ends_with('-')
    {
        Some(normalized)
    } else {
        None
    }
}

/// Generate `count` valid raw candidates, score them locally, and return the best `top`.
///
/// The fixed pattern order and index permutation make identical configurations reproducible.
pub fn generate_premium_candidates(config: &CandidateGenerationConfig) -> Vec<ScoredCandidate> {
    if config.count == 0 || config.top == 0 {
        return Vec::new();
    }
    let Some(tld) = normalize_tld(&config.tld) else {
        return Vec::new();
    };

    let mut candidates = Vec::with_capacity(config.count);
    let mut seen = HashSet::with_capacity(config.count);
    let mut sequence = 0usize;
    let max_attempts = config.count.saturating_mul(30).max(100);

    while candidates.len() < config.count && sequence < max_attempts {
        let pattern_index = sequence % GENERATION_FAMILIES;
        let ordinal = sequence / GENERATION_FAMILIES;
        let name = if pattern_index < PATTERNS.len() {
            let pattern = PATTERNS[pattern_index];
            let space = pattern_space(pattern);
            // Odd constants spread adjacent requests across the pattern space without randomness.
            let permuted = ordinal.wrapping_mul(104_729).wrapping_add(7_919) % space;
            render_pattern(pattern, permuted)
        } else {
            render_syllables(ordinal)
        };
        sequence += 1;

        if !is_premium_name(&name) || !seen.insert(name.clone()) {
            continue;
        }
        let domain = format!("{name}.{tld}");
        candidates.push(ScoredCandidate {
            scoring: score_domain(&domain),
            domain,
            pattern_index,
        });
    }

    candidates.sort_by(|left, right| {
        right
            .scoring
            .total_score
            .cmp(&left.scoring.total_score)
            .then_with(|| left.domain.cmp(&right.domain))
    });
    select_diverse(candidates, config.top.min(config.count))
}

fn select_diverse(candidates: Vec<ScoredCandidate>, top: usize) -> Vec<ScoredCandidate> {
    let per_pattern_limit = top.div_ceil(GENERATION_FAMILIES).max(1);
    let mut selected = Vec::with_capacity(top);
    let mut selected_domains = HashSet::new();
    let mut pattern_counts = [0usize; GENERATION_FAMILIES];
    let mut family_counts: HashMap<String, usize> = HashMap::new();

    for candidate in &candidates {
        let family = family_signature(&candidate.domain);
        if pattern_counts[candidate.pattern_index] >= per_pattern_limit
            || family_counts.get(&family).copied().unwrap_or(0) >= 2
        {
            continue;
        }
        pattern_counts[candidate.pattern_index] += 1;
        *family_counts.entry(family).or_default() += 1;
        selected_domains.insert(candidate.domain.clone());
        selected.push(candidate.clone());
        if selected.len() == top {
            return selected;
        }
    }

    // Fill any remaining slots without diversity caps, preserving score order.
    for candidate in candidates {
        if selected_domains.insert(candidate.domain.clone()) {
            selected.push(candidate);
            if selected.len() == top {
                break;
            }
        }
    }
    selected
}

fn pattern_space(pattern: &str) -> usize {
    pattern.chars().fold(1usize, |space, slot| {
        space.saturating_mul(if slot == 'V' {
            VOWELS.len()
        } else {
            CONSONANTS.len()
        })
    })
}

fn render_pattern(pattern: &str, mut index: usize) -> String {
    let mut rendered = Vec::with_capacity(pattern.len());
    for slot in pattern.chars().rev() {
        let alphabet = if slot == 'V' { VOWELS } else { CONSONANTS };
        rendered.push(alphabet[index % alphabet.len()]);
        index /= alphabet.len();
    }
    rendered.reverse();
    String::from_utf8(rendered).expect("generator alphabets are ASCII")
}

fn render_syllables(index: usize) -> String {
    let space = SAFE_SYLLABLES.len().pow(3);
    let mut permuted = index.wrapping_mul(2_053).wrapping_add(431) % space;
    let mut parts = [""; 3];
    for part in parts.iter_mut().rev() {
        *part = SAFE_SYLLABLES[permuted % SAFE_SYLLABLES.len()];
        permuted /= SAFE_SYLLABLES.len();
    }
    parts.concat()
}

fn is_premium_name(name: &str) -> bool {
    (5..=10).contains(&name.len())
        && name.bytes().all(|byte| byte.is_ascii_lowercase())
        && !NEGATIVE_PARTS.iter().any(|part| name.contains(part))
        && longest_consonant_cluster(name) < 4
        && longest_repeated_run(name) < 3
}

fn longest_consonant_cluster(name: &str) -> usize {
    let mut current = 0;
    let mut longest = 0;
    for byte in name.bytes() {
        if VOWELS.contains(&byte) {
            current = 0;
        } else {
            current += 1;
            longest = longest.max(current);
        }
    }
    longest
}

fn longest_repeated_run(name: &str) -> usize {
    let mut previous = None;
    let mut current = 0;
    let mut longest = 0;
    for byte in name.bytes() {
        if previous == Some(byte) {
            current += 1;
        } else {
            previous = Some(byte);
            current = 1;
        }
        longest = longest.max(current);
    }
    longest
}

fn family_signature(domain: &str) -> String {
    domain
        .split('.')
        .next()
        .unwrap_or_default()
        .bytes()
        .filter(|byte| !VOWELS.contains(byte))
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(count: usize, top: usize, tld: &str) -> CandidateGenerationConfig {
        CandidateGenerationConfig {
            count,
            top,
            tld: tld.to_string(),
        }
    }

    #[test]
    fn output_is_deterministic_and_respects_limits() {
        let first = generate_premium_candidates(&config(500, 40, "com"));
        let second = generate_premium_candidates(&config(500, 40, "com"));
        assert_eq!(first, second);
        assert_eq!(first.len(), 40);
    }

    #[test]
    fn generated_names_are_valid_and_unique() {
        let candidates = generate_premium_candidates(&config(2_000, 1_000, "com"));
        let domains: HashSet<_> = candidates.iter().map(|item| &item.domain).collect();
        assert_eq!(domains.len(), candidates.len());
        assert!(candidates.iter().all(|item| {
            let name = item.domain.split('.').next().unwrap();
            is_premium_name(name) && !name.contains(|c: char| c.is_ascii_digit() || c == '-')
        }));
    }

    #[test]
    fn tld_is_normalized() {
        let candidates = generate_premium_candidates(&config(20, 5, ".COM"));
        assert!(candidates.iter().all(|item| item.domain.ends_with(".com")));
        assert_eq!(normalize_tld("bad.tld"), None);
    }

    #[test]
    fn top_results_include_multiple_patterns() {
        let candidates = generate_premium_candidates(&config(1_000, 100, "com"));
        let patterns: HashSet<_> = candidates.iter().map(|item| item.pattern_index).collect();
        assert_eq!(patterns.len(), GENERATION_FAMILIES);

        let mut families = HashMap::new();
        for candidate in candidates {
            *families
                .entry(family_signature(&candidate.domain))
                .or_insert(0usize) += 1;
        }
        assert!(families.values().all(|count| *count <= 2));
    }
}
