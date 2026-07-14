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
    "fap", "rape", "scam", "sex", "shit", "spam", "suck", "vermin",
];

// Exact collisions are intentionally data-driven so the list can be expanded without changing
// ranking logic. It covers conspicuous brands plus strong dictionary words that should be reviewed
// rather than presented as newly generated premium inventory.
const RESERVED_EXACT_NAMES: &[&str] = &[
    "adobe",
    "airbnb",
    "alexa",
    "alibaba",
    "amazon",
    "android",
    "apple",
    "bitcoin",
    "booking",
    "canva",
    "chrome",
    "cisco",
    "coinbase",
    "discord",
    "dropbox",
    "facebook",
    "ferrari",
    "firefox",
    "getir",
    "github",
    "google",
    "instagram",
    "linkedin",
    "microsoft",
    "netflix",
    "notion",
    "nvidia",
    "paypal",
    "reddit",
    "samsung",
    "shopify",
    "slack",
    "spotify",
    "stripe",
    "telegram",
    "tesla",
    "tiktok",
    "toyota",
    "twitter",
    "uber",
    "whatsapp",
    "windows",
    "youtube",
    "academy",
    "capital",
    "credit",
    "finance",
    "health",
    "market",
    "premium",
    "software",
    "travel",
    "wallet",
];

const COMMERCIAL_PREFIXES: &[&str] = &["get", "go", "my", "try", "use", "pro", "neo"];
const COMMERCIAL_SUFFIXES: &[&str] = &[
    "ai", "app", "base", "fy", "hub", "io", "labs", "ly", "tech", "wise",
];

// Consonant transitions that are uncommon inside English-like coined names. This deliberately
// targets strong signals only; ordinary brandable clusters such as cl, st, tr, nd and nt survive.
const UNLIKELY_BIGRAMS: &[&str] = &[
    "bd", "bk", "bp", "bw", "cf", "cg", "cj", "cv", "cw", "dj", "dk", "dt", "dz", "fb", "fg", "fj",
    "fk", "fm", "fp", "fv", "fw", "gb", "gd", "gf", "gj", "gk", "gp", "gv", "gw", "hj", "hk", "hp",
    "hv", "hw", "jb", "jc", "jd", "jf", "jg", "jh", "jk", "jl", "jm", "jn", "jp", "jr", "js", "jt",
    "jv", "jw", "jz", "kg", "kp", "kv", "kw", "mg", "mv", "mz", "pb", "pd", "pf", "pg", "pk", "pm",
    "pv", "pw", "qb", "qc", "qd", "qf", "qg", "qh", "qj", "qk", "ql", "qm", "qn", "qp", "qr", "qs",
    "qt", "qv", "qw", "qx", "qz", "td", "tf", "tg", "tj", "tk", "tp", "tv", "tx", "vb", "vc", "vd",
    "vf", "vg", "vh", "vj", "vk", "vm", "vp", "vt", "vw", "vz", "wb", "wf", "wg", "wj", "wk", "wm",
    "wp", "wq", "wt", "wv", "wx", "wz", "xb", "xc", "xd", "xf", "xg", "xh", "xj", "xk", "xl", "xm",
    "xn", "xp", "xq", "xr", "xs", "xt", "xv", "xw", "xz", "zb", "zc", "zd", "zf", "zg", "zj", "zk",
    "zm", "zp", "zq", "zr", "zs", "zt", "zv", "zw", "zx",
];

const AWKWARD_ENDINGS: &[&str] = &["iw", "uw", "yy", "aeu", "eao", "ioa", "uoa"];
const AWKWARD_VOWEL_TRANSITIONS: &[&str] =
    &["aa", "ae", "ao", "ea", "ee", "ii", "iu", "oa", "uo", "uu"];
const UNCOMMON_DOUBLE_CONSONANTS: &[&str] = &[
    "bb", "cc", "dd", "ff", "gg", "hh", "jj", "kk", "mm", "pp", "qq", "vv", "ww", "xx", "zz",
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

        if !is_premium_name(&name) || is_reserved_collision(&name) || !seen.insert(name.clone()) {
            continue;
        }
        let domain = format!("{name}.{tld}");
        let mut scoring = score_domain(&domain);
        apply_generation_quality(&name, &mut scoring);
        candidates.push(ScoredCandidate {
            scoring,
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
    let mut prefix_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut suffix_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut initial_counts: HashMap<u8, usize> = HashMap::new();
    let affix_limit = top.div_ceil(100).clamp(2, 10);
    let initial_limit = top.div_ceil(15).max(2);

    for candidate in &candidates {
        let family = family_signature(&candidate.domain);
        let name = candidate.domain.split('.').next().unwrap_or_default();
        let prefix = matched_prefix(name);
        let suffix = matched_suffix(name);
        if pattern_counts[candidate.pattern_index] >= per_pattern_limit
            || family_counts.get(&family).copied().unwrap_or(0) >= 2
            || prefix
                .is_some_and(|value| prefix_counts.get(value).copied().unwrap_or(0) >= affix_limit)
            || suffix
                .is_some_and(|value| suffix_counts.get(value).copied().unwrap_or(0) >= affix_limit)
            || name.bytes().next().is_some_and(|initial| {
                initial_counts.get(&initial).copied().unwrap_or(0) >= initial_limit
            })
        {
            continue;
        }
        pattern_counts[candidate.pattern_index] += 1;
        *family_counts.entry(family).or_default() += 1;
        if let Some(value) = prefix {
            *prefix_counts.entry(value).or_default() += 1;
        }
        if let Some(value) = suffix {
            *suffix_counts.entry(value).or_default() += 1;
        }
        if let Some(initial) = name.bytes().next() {
            *initial_counts.entry(initial).or_default() += 1;
        }
        selected_domains.insert(candidate.domain.clone());
        selected.push(candidate.clone());
        if selected.len() == top {
            return selected;
        }
    }

    // Relax only the pattern balance when necessary. Lexical-family and affix caps remain hard so
    // the tail of a large list cannot undo the diversity guarantees applied above.
    for candidate in candidates {
        let family = family_signature(&candidate.domain);
        let name = candidate.domain.split('.').next().unwrap_or_default();
        let prefix = matched_prefix(name);
        let suffix = matched_suffix(name);
        if family_counts.get(&family).copied().unwrap_or(0) >= 2
            || prefix
                .is_some_and(|value| prefix_counts.get(value).copied().unwrap_or(0) >= affix_limit)
            || suffix
                .is_some_and(|value| suffix_counts.get(value).copied().unwrap_or(0) >= affix_limit)
            || name.bytes().next().is_some_and(|initial| {
                initial_counts.get(&initial).copied().unwrap_or(0) >= initial_limit
            })
        {
            continue;
        }
        if selected_domains.insert(candidate.domain.clone()) {
            *family_counts.entry(family).or_default() += 1;
            if let Some(value) = prefix {
                *prefix_counts.entry(value).or_default() += 1;
            }
            if let Some(value) = suffix {
                *suffix_counts.entry(value).or_default() += 1;
            }
            if let Some(initial) = name.bytes().next() {
                *initial_counts.entry(initial).or_default() += 1;
            }
            selected.push(candidate);
            if selected.len() == top {
                break;
            }
        }
    }
    selected
}

fn is_reserved_collision(name: &str) -> bool {
    RESERVED_EXACT_NAMES.contains(&name)
}

fn matched_prefix(name: &str) -> Option<&'static str> {
    COMMERCIAL_PREFIXES
        .iter()
        .copied()
        .find(|prefix| name.starts_with(prefix))
}

fn matched_suffix(name: &str) -> Option<&'static str> {
    COMMERCIAL_SUFFIXES
        .iter()
        .copied()
        .find(|suffix| name.ends_with(suffix))
}

fn apply_generation_quality(name: &str, score: &mut InvestmentScore) {
    let unlikely_count = UNLIKELY_BIGRAMS
        .iter()
        .filter(|bigram| name.contains(**bigram))
        .count();
    let mut penalty = (unlikely_count.min(2) * 9) as u8;

    if unlikely_count > 0 {
        add_generation_reason(score, "unlikely English letter transition");
    }
    if AWKWARD_ENDINGS.iter().any(|ending| name.ends_with(ending)) {
        penalty = penalty.saturating_add(8);
        add_generation_reason(score, "ambiguous ending");
    }
    if AWKWARD_VOWEL_TRANSITIONS
        .iter()
        .any(|transition| name.contains(transition))
    {
        penalty = penalty.saturating_add(8);
        add_generation_reason(score, "awkward vowel transition");
    }
    if UNCOMMON_DOUBLE_CONSONANTS
        .iter()
        .any(|pair| name.contains(pair))
    {
        penalty = penalty.saturating_add(8);
        add_generation_reason(score, "uncommon doubled consonant");
    }

    let weak_prefix_stem = matched_prefix(name)
        .map(|prefix| &name[prefix.len()..])
        .is_some_and(|stem| stem.len() < 3 || contains_unlikely_bigram(stem));
    let weak_suffix_stem = matched_suffix(name)
        .map(|suffix| &name[..name.len() - suffix.len()])
        .is_some_and(|stem| stem.len() < 3 || contains_unlikely_bigram(stem));
    if weak_prefix_stem || weak_suffix_stem {
        penalty = penalty.saturating_add(18);
        add_generation_reason(score, "weak or meaningless affix stem");
    }

    if matched_prefix(name).is_some() || matched_suffix(name).is_some() {
        // The general scorer exposes affixes as a small commercial signal. Generated candidates
        // can match those strings by chance, so ranking neutralizes that bonus until a human has
        // confirmed that the stem is meaningful.
        penalty = penalty.saturating_add(2);
        add_generation_reason(score, "generated affix requires semantic review");
    }
    if has_repeated_fragment(name) {
        penalty = penalty.saturating_add(14);
        add_generation_reason(score, "repetitive or meaningless syllable pattern");
    }

    score.risk_penalty = score.risk_penalty.saturating_add(penalty).min(50);
    score.total_score = score.total_score.saturating_sub(penalty);
}

fn has_repeated_fragment(name: &str) -> bool {
    let bytes = name.as_bytes();
    (2..=3).any(|width| {
        bytes.windows(width).enumerate().any(|(index, fragment)| {
            bytes[index + width..]
                .windows(width)
                .any(|other| other == fragment)
        })
    })
}

fn contains_unlikely_bigram(name: &str) -> bool {
    UNLIKELY_BIGRAMS.iter().any(|bigram| name.contains(*bigram))
}

fn add_generation_reason(score: &mut InvestmentScore, reason: &str) {
    if !score.reasons.iter().any(|existing| existing == reason) {
        score.reasons.push(reason.to_string());
    }
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

    #[test]
    fn reserved_brand_and_dictionary_collisions_are_filtered() {
        assert!(is_reserved_collision("getir"));
        assert!(is_reserved_collision("google"));
        assert!(is_reserved_collision("finance"));
        assert!(!is_reserved_collision("cladine"));

        let candidates = generate_premium_candidates(&config(30_000, 1_000, "com"));
        assert!(!candidates.iter().any(|item| item.domain == "getir.com"));
    }

    #[test]
    fn awkward_generated_names_receive_quality_penalties() {
        let score_for = |name: &str| {
            let mut score = score_domain(&format!("{name}.com"));
            apply_generation_quality(name, &mut score);
            score
        };

        assert!(score_for("gobdeh").total_score < score_for("cladine").total_score);
        assert!(score_for("getow").total_score < score_for("cladine").total_score);
        assert!(score_for("cikiw").total_score < score_for("cladine").total_score);
        assert!(score_for("alalal").total_score < score_for("cladine").total_score);
        assert!(score_for("caalbo").total_score < score_for("cladine").total_score);
        assert!(score_for("babbeg").total_score < score_for("cladine").total_score);
        assert!(!is_premium_name("faphub"));
    }

    #[test]
    fn top_list_enforces_affix_density_limits() {
        let top = 50;
        let candidates = generate_premium_candidates(&config(30_000, top, "com"));
        let get_count = candidates
            .iter()
            .filter(|item| item.domain.starts_with("get"))
            .count();
        let hub_count = candidates
            .iter()
            .filter(|item| item.domain.split('.').next().unwrap().ends_with("hub"))
            .count();

        assert_eq!(candidates.len(), top);
        assert!(get_count <= 2, "get count was {get_count}");
        assert!(hub_count <= 2, "hub count was {hub_count}");
    }

    #[test]
    fn quality_filter_preserves_clean_brandable_examples() {
        let mut clean = score_domain("cladine.com");
        let original = clean.clone();
        apply_generation_quality("cladine", &mut clean);
        assert_eq!(clean, original);
        assert!(clean.total_score >= 85);
    }
}
