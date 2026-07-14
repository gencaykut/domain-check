//! Deterministic, network-independent domain investment scoring.

use serde::{Deserialize, Serialize};

/// An explainable 0-100 investment score for a domain name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestmentScore {
    pub total_score: u8,
    pub length_score: u8,
    pub pronounceability_score: u8,
    pub spelling_score: u8,
    pub commercial_score: u8,
    pub risk_penalty: u8,
    pub reasons: Vec<String>,
}

/// Score a domain using fixed, explainable heuristics and no external services.
pub fn score_domain(domain: &str) -> InvestmentScore {
    let normalized = domain.trim().trim_end_matches('.').to_lowercase();
    let labels: Vec<&str> = normalized
        .split('.')
        .filter(|part| !part.is_empty())
        .collect();
    let name = match labels.len() {
        0 => "",
        1 => labels[0],
        len => labels[len - 2],
    };
    let tld = labels.last().copied().unwrap_or("");

    let chars: Vec<char> = name.chars().collect();
    let length = chars.len();
    let has_hyphen = name.contains('-');
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    let ascii_letters = chars.iter().filter(|c| c.is_ascii_alphabetic()).count();
    let has_non_ascii = chars
        .iter()
        .any(|c| !c.is_ascii_alphanumeric() && *c != '-');
    let vowel_count = chars.iter().filter(|c| is_vowel(**c)).count();
    let vowel_ratio = if ascii_letters == 0 {
        0.0
    } else {
        vowel_count as f64 / ascii_letters as f64
    };
    let longest_cluster = longest_consonant_cluster(name);
    let longest_repeat = longest_repeated_run(name);

    let mut reasons = Vec::new();

    let length_score = match length {
        5..=9 => {
            add_reason(&mut reasons, "ideal length");
            25
        }
        4 | 10 => 21,
        3 | 11 => 17,
        2 | 12 => 12,
        13..=15 => 7,
        _ => 2,
    };

    let pronounceability_score = if vowel_count == 0 || ascii_letters == 0 {
        add_reason(&mut reasons, "no vowels");
        0
    } else if (0.30..=0.60).contains(&vowel_ratio) && longest_cluster <= 2 {
        add_reason(&mut reasons, "balanced vowels");
        25
    } else if (0.22..=0.68).contains(&vowel_ratio) && longest_cluster <= 3 {
        18
    } else if longest_cluster <= 3 {
        11
    } else {
        add_reason(&mut reasons, "hard consonant cluster");
        3
    };

    let mut spelling_score = 25i16;
    if has_hyphen {
        spelling_score -= 10;
        add_reason(&mut reasons, "contains hyphen");
    }
    if has_digit {
        spelling_score -= 10;
        add_reason(&mut reasons, "contains digits");
    }
    if has_non_ascii {
        spelling_score -= 6;
        add_reason(&mut reasons, "non-ASCII spelling");
    }
    if longest_cluster >= 4 {
        spelling_score -= 8;
        add_reason(&mut reasons, "awkward spelling");
    }
    if longest_repeat >= 3 {
        spelling_score -= 7;
        add_reason(&mut reasons, "repeated letters");
    }
    let rare_letters = chars
        .iter()
        .filter(|c| matches!(c, 'q' | 'x' | 'z'))
        .count();
    spelling_score -= (rare_letters.min(3) * 4) as i16;
    if spelling_score >= 22 {
        add_reason(&mut reasons, "easy spelling");
    }
    let spelling_score = spelling_score.clamp(0, 25) as u8;

    let mut commercial_score = 0u8;
    if tld == "com" {
        commercial_score += 10;
        add_reason(&mut reasons, ".com bonus");
    }
    if (5..=10).contains(&length)
        && pronounceability_score >= 18
        && !has_hyphen
        && !has_digit
        && !has_non_ascii
    {
        commercial_score += 7;
        add_reason(&mut reasons, "brandable structure");
    }
    const PREFIXES: &[&str] = &["get", "go", "my", "try", "use", "pro", "neo"];
    const SUFFIXES: &[&str] = &[
        "ai", "app", "base", "fy", "hub", "io", "labs", "ly", "tech", "wise",
    ];
    if PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
        commercial_score += 4;
        add_reason(&mut reasons, "commercial prefix");
    }
    if SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
        commercial_score += 4;
        add_reason(&mut reasons, "commercial suffix");
    }
    commercial_score = commercial_score.min(25);

    let mut risk_penalty = 0u8;
    const NEGATIVE_PARTS: &[&str] = &[
        "abuse", "adult", "bad", "crime", "damn", "death", "fraud", "hate", "jail", "kill",
        "moron", "ponzi", "scam", "spam", "suck", "vermin",
    ];
    if NEGATIVE_PARTS.iter().any(|part| name.contains(part)) {
        risk_penalty += 22;
        add_reason(&mut reasons, "negative term");
    }
    if has_hyphen {
        risk_penalty += 6;
    }
    if has_digit {
        risk_penalty += 6;
    }
    if !(3..=15).contains(&length) {
        risk_penalty += 8;
        add_reason(&mut reasons, "difficult length");
    }
    if longest_cluster >= 4 {
        risk_penalty += 10;
    }
    if longest_repeat >= 3 {
        risk_penalty += 6;
    }
    if vowel_count == 0 && !name.is_empty() {
        risk_penalty += 10;
    }
    risk_penalty = risk_penalty.min(50);

    let subtotal = length_score as i16
        + pronounceability_score as i16
        + spelling_score as i16
        + commercial_score as i16;
    let total_score = (subtotal - risk_penalty as i16).clamp(0, 100) as u8;

    InvestmentScore {
        total_score,
        length_score,
        pronounceability_score,
        spelling_score,
        commercial_score,
        risk_penalty,
        reasons,
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

fn longest_consonant_cluster(name: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in name.chars() {
        if c.is_ascii_alphabetic() && !is_vowel(c) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn longest_repeated_run(name: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    let mut previous = None;
    for c in name.chars() {
        if previous == Some(c) {
            current += 1;
        } else {
            current = 1;
            previous = Some(c);
        }
        longest = longest.max(current);
    }
    longest
}

fn add_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|existing| existing == reason) {
        reasons.push(reason.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_brand_scores_higher_than_consonant_noise() {
        let brand = score_domain("cladine.com");
        let noise = score_domain("xqtrpz.com");
        assert!(brand.total_score >= 80);
        assert!(noise.total_score <= 35);
        assert!(brand.total_score > noise.total_score);
    }

    #[test]
    fn hyphen_and_digit_receive_penalties() {
        let clean = score_domain("novara.com");
        assert!(score_domain("no-vara.com").total_score < clean.total_score);
        assert!(score_domain("novara7.com").total_score < clean.total_score);
    }

    #[test]
    fn long_and_negative_names_receive_penalties() {
        assert!(score_domain("averyveryverylongbrand.com").risk_penalty > 0);
        assert!(score_domain("ponzihub.com").risk_penalty >= 22);
    }

    #[test]
    fn score_is_bounded_and_deterministic() {
        let inputs = ["", "A.COM", "sub.example.com", "xqtrpz.com", "çığ.com"];
        for input in inputs {
            let first = score_domain(input);
            let second = score_domain(input);
            assert_eq!(first, second);
            assert!(first.total_score <= 100);
        }
    }

    #[test]
    fn uppercase_and_subdomain_are_normalized() {
        assert_eq!(score_domain("CLADINE.COM"), score_domain("cladine.com"));
        assert_eq!(score_domain("www.cladine.com"), score_domain("cladine.com"));
    }
}
