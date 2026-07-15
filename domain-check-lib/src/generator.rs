//! Deterministic, network-free generation of pronounceable domain candidates.

use crate::{score_domain, InvestmentScore};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// Sonorants and other brand-friendly consonants are intentionally represented more often than
// hard stops. The generator is still deterministic; this only changes which parts of the search
// space receive more coverage in a finite run.
const CONSONANTS: &[u8] = b"lrvsmnctpldrvnsgkfbhjwz";
const VOWELS: &[u8] = b"aeiou";
const PATTERNS: &[&str] = &["CVCVC", "CVCVCV", "CVCCVC", "CVCVCVC"];
const LENGTH_CONSTRAINED_PATTERNS: &[&str] = &[
    "CVCVC",
    "VCVCV",
    "CVVCV",
    "CVCVV",
    "CVCCV",
    "CCVCV", // 5
    "CVCVCV",
    "CVCCVC",
    "VCVCVC",
    "CVCVVC",
    "CVVCVC",
    "CCVCVC", // 6
    "CVCVCVC",
    "VCVCVCV",
    "CVCVCVV",
    "CVVCVCV",
    "CVCCVCV", // 7
    "CVCVCVCV",
    "VCVCVCVC",
    "CVVCVCVC",
    "CVCVCCVC", // 8
    "CVCVCVCVC",
    "VCVCVCVCV",
    "CVVCVCVCV", // 9
    "CVCVCVCVCV",
    "VCVCVCVCVC",
    "CVVCVCVCVC", // 10
];
const SAFE_SYLLABLES: &[&str] = &[
    "al", "ara", "avi", "bel", "bo", "ca", "dari", "do", "ela", "eva", "ivo", "lari", "le", "lo",
    "luma", "mari", "mi", "navi", "ne", "nora", "nova", "ora", "ravi", "re", "ria", "sani", "se",
    "sol", "tavi", "te", "uma", "va", "vela", "vero", "vi", "via", "za", "zen",
];
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
const ARTIFICIAL_ONSETS: &[&str] = &["bn", "dl", "fn", "hn", "kn", "mn", "rr", "ss", "tn"];
const NATURAL_CONSONANT_ONSETS: &[&str] = &[
    "bl", "br", "ch", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "ph", "pl", "pr", "sc", "sh", "sk",
    "sl", "sm", "sn", "sp", "st", "sw", "th", "tr", "tw", "wh", "wr",
];
const DENSE_FAMILY_FRAGMENTS: &[&str] = &["ara", "lar", "nor", "rav", "ria", "vel"];

/// Settings for deterministic premium candidate generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateGenerationConfig {
    pub count: usize,
    pub top: usize,
    pub tld: String,
    /// Inclusive second-level label length bounds. Premium generation supports 5..=10.
    pub min_length: usize,
    pub max_length: usize,
    /// Minimum generator-specific linguistic quality accepted for selection.
    pub min_generation_quality: u8,
    /// Maximum selected candidates assigned to the same phonetic family.
    pub max_per_family: usize,
}

/// Optional second-level label filters for premium generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateGenerationFilters {
    pub contains: Option<String>,
    pub starts_with: Option<String>,
    pub ends_with: Option<String>,
    pub excluded_domains: HashSet<String>,
}

/// A generated domain and its explainable local investment score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredCandidate {
    pub domain: String,
    pub scoring: InvestmentScore,
    pub generation_quality: GenerationQualityScore,
    /// Deterministic key for the phonetic family used during diverse selection.
    pub family_key: String,
    #[serde(skip)]
    pattern_index: usize,
}

/// Generator-specific linguistic quality. This deliberately does not alter `InvestmentScore` or
/// the public `score_domain` API used for manually supplied domains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationQualityScore {
    pub total_score: u8,
    pub phonotactic_score: u8,
    pub boundary_score: u8,
    pub rhythm_score: u8,
    pub naturalness_score: u8,
    pub penalty: u8,
    pub reasons: Vec<String>,
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
    generate_premium_candidates_with_filters(config, &CandidateGenerationFilters::default())
}

/// Generate premium candidates while applying deterministic second-level label filters.
pub fn generate_premium_candidates_with_filters(
    config: &CandidateGenerationConfig,
    filters: &CandidateGenerationFilters,
) -> Vec<ScoredCandidate> {
    generate_premium_candidates_with_filters_and_stats(config, filters).0
}

/// Generate filtered candidates and report how many otherwise valid names history excluded.
pub fn generate_premium_candidates_with_filters_and_stats(
    config: &CandidateGenerationConfig,
    filters: &CandidateGenerationFilters,
) -> (Vec<ScoredCandidate>, usize) {
    if config.count == 0
        || config.top == 0
        || config.min_length < 5
        || config.max_length > 10
        || config.min_length > config.max_length
        || config.min_generation_quality > 100
        || config.max_per_family == 0
    {
        return (Vec::new(), 0);
    }
    let Some(tld) = normalize_tld(&config.tld) else {
        return (Vec::new(), 0);
    };
    let Some(filters) = normalize_filters(filters, config.max_length) else {
        return (Vec::new(), 0);
    };

    let constrained = config.min_length != 5 || config.max_length != 10;
    let patterns: Vec<&str> = if constrained {
        LENGTH_CONSTRAINED_PATTERNS
            .iter()
            .copied()
            .filter(|pattern| (config.min_length..=config.max_length).contains(&pattern.len()))
            .collect()
    } else {
        PATTERNS.to_vec()
    };
    let include_syllables =
        (config.min_length..=config.max_length).any(|length| (5..=10).contains(&length));
    let generation_families = patterns.len() + usize::from(include_syllables);
    if generation_families == 0 {
        return (Vec::new(), 0);
    }

    let mut candidates = Vec::with_capacity(config.count);
    let mut seen = HashSet::with_capacity(config.count);
    let mut sequence = 0usize;
    let mut raw_candidate_count = 0usize;
    let mut excluded_count = 0usize;
    let max_attempts = config.count.saturating_mul(30).max(100);

    while raw_candidate_count < config.count && sequence < max_attempts {
        let pattern_index = sequence % generation_families;
        let ordinal = sequence / generation_families;
        let raw_name = if pattern_index < patterns.len() {
            let pattern = patterns[pattern_index];
            let space = pattern_space(pattern);
            // Odd constants spread adjacent requests across the pattern space without randomness.
            let permuted = ordinal.wrapping_mul(104_729).wrapping_add(7_919) % space;
            render_pattern(pattern, permuted)
        } else {
            render_syllables(ordinal)
        };
        sequence += 1;
        let Some(name) = apply_filters(raw_name, &filters, sequence) else {
            continue;
        };

        if !(config.min_length..=config.max_length).contains(&name.len())
            || !matches_filters(&name, &filters)
            || !is_premium_name(&name)
            || is_reserved_collision(&name)
            || !seen.insert(name.clone())
        {
            continue;
        }
        let domain = format!("{name}.{tld}");
        if filters.excluded_domains.contains(&domain) {
            excluded_count += 1;
            continue;
        }
        raw_candidate_count += 1;
        let scoring = score_domain(&domain);
        let generation_quality = generation_quality_score(&name);
        if generation_quality.total_score < config.min_generation_quality {
            continue;
        }
        candidates.push(ScoredCandidate {
            scoring,
            generation_quality,
            family_key: family_key(&name),
            domain,
            pattern_index,
        });
    }

    candidates.sort_by(|left, right| {
        right
            .generation_quality
            .total_score
            .cmp(&left.generation_quality.total_score)
            .then_with(|| right.scoring.total_score.cmp(&left.scoring.total_score))
            .then_with(|| left.domain.cmp(&right.domain))
    });
    (
        select_diverse(
            candidates,
            config.top.min(config.count),
            generation_families,
            &filters,
            config.max_per_family,
        ),
        excluded_count,
    )
}

fn normalize_filters(
    filters: &CandidateGenerationFilters,
    max_length: usize,
) -> Option<CandidateGenerationFilters> {
    let normalize = |value: &Option<String>| -> Option<Option<String>> {
        match value {
            None => Some(None),
            Some(value)
                if !value.is_empty()
                    && value.len() <= max_length
                    && value.bytes().all(|byte| byte.is_ascii_alphabetic()) =>
            {
                Some(Some(value.to_ascii_lowercase()))
            }
            Some(_) => None,
        }
    };
    Some(CandidateGenerationFilters {
        contains: normalize(&filters.contains)?,
        starts_with: normalize(&filters.starts_with)?,
        ends_with: normalize(&filters.ends_with)?,
        excluded_domains: filters
            .excluded_domains
            .iter()
            .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase())
            .collect(),
    })
}

fn apply_filters(
    name: String,
    filters: &CandidateGenerationFilters,
    sequence: usize,
) -> Option<String> {
    if filters.contains.is_none() && filters.starts_with.is_none() && filters.ends_with.is_none() {
        return Some(name);
    }
    let mut bytes = name.into_bytes();
    if let Some(prefix) = &filters.starts_with {
        bytes
            .get_mut(..prefix.len())?
            .copy_from_slice(prefix.as_bytes());
    }
    if let Some(suffix) = &filters.ends_with {
        let start = bytes.len().checked_sub(suffix.len())?;
        for (offset, byte) in suffix.bytes().enumerate() {
            let index = start + offset;
            if filters
                .starts_with
                .as_ref()
                .is_some_and(|prefix| index < prefix.len() && bytes[index] != byte)
            {
                return None;
            }
            bytes[index] = byte;
        }
    }
    if let Some(required) = &filters.contains {
        let current = std::str::from_utf8(&bytes).ok()?;
        if !current.contains(required) {
            let max_start = bytes.len().checked_sub(required.len())?;
            let starts = (0..=max_start)
                .cycle()
                .skip(sequence % (max_start + 1))
                .take(max_start + 1);
            let mut replacement = None;
            for start in starts {
                let mut candidate = bytes.clone();
                candidate[start..start + required.len()].copy_from_slice(required.as_bytes());
                let candidate_text = std::str::from_utf8(&candidate).ok()?;
                if matches_filters(candidate_text, filters) {
                    replacement = Some(candidate);
                    break;
                }
            }
            bytes = replacement?;
        }
    }
    let value = String::from_utf8(bytes).ok()?;
    matches_filters(&value, filters).then_some(value)
}

fn matches_filters(name: &str, filters: &CandidateGenerationFilters) -> bool {
    filters
        .contains
        .as_ref()
        .is_none_or(|value| name.contains(value))
        && filters
            .starts_with
            .as_ref()
            .is_none_or(|value| name.starts_with(value))
        && filters
            .ends_with
            .as_ref()
            .is_none_or(|value| name.ends_with(value))
}

fn select_diverse(
    candidates: Vec<ScoredCandidate>,
    top: usize,
    generation_families: usize,
    filters: &CandidateGenerationFilters,
    max_per_family: usize,
) -> Vec<ScoredCandidate> {
    let per_pattern_limit = top.div_ceil(generation_families).max(1);
    let mut selected = Vec::with_capacity(top);
    let mut selected_domains = HashSet::new();
    let mut pattern_counts = vec![0usize; generation_families];
    let mut families: Vec<(String, String, usize)> = Vec::new();
    let mut prefix_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut suffix_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut dense_fragment_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut initial_counts: HashMap<u8, usize> = HashMap::new();
    let affix_limit = top.div_ceil(100).clamp(2, 10);
    let family_limit = max_per_family;
    let start_bigram_limit = if filters.starts_with.is_some() {
        top
    } else {
        top.div_ceil(12).max(3)
    };
    let start_trigram_limit = if filters.starts_with.is_some() {
        top
    } else {
        top.div_ceil(25).max(2)
    };
    let mut start_bigram_counts: HashMap<String, usize> = HashMap::new();
    let mut start_trigram_counts: HashMap<String, usize> = HashMap::new();
    let dense_fragment_limit = top.div_ceil(30).max(2);
    let initial_limit = if filters.starts_with.is_some() {
        top
    } else {
        top.div_ceil(8).max(3)
    };

    for candidate in &candidates {
        let name = candidate.domain.split('.').next().unwrap_or_default();
        let family_index = matching_family_index(name, &families);
        let family_count = family_index.map_or(0, |index| families[index].2);
        let start_bigram = prefix_chars(name, 2);
        let start_trigram = prefix_chars(name, 3);
        let prefix = matched_prefix(name);
        let suffix = matched_suffix(name);
        let dense_fragments = dense_family_fragments(name);
        if pattern_counts[candidate.pattern_index] >= per_pattern_limit
            || family_count >= family_limit
            || start_bigram_counts.get(&start_bigram).copied().unwrap_or(0) >= start_bigram_limit
            || start_trigram_counts
                .get(&start_trigram)
                .copied()
                .unwrap_or(0)
                >= start_trigram_limit
            || dense_fragments.iter().any(|fragment| {
                dense_fragment_counts.get(fragment).copied().unwrap_or(0) >= dense_fragment_limit
            })
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
        let mut selected_candidate = candidate.clone();
        assign_family(&mut selected_candidate, name, family_index, &mut families);
        *start_bigram_counts.entry(start_bigram).or_default() += 1;
        *start_trigram_counts.entry(start_trigram).or_default() += 1;
        if let Some(value) = prefix {
            *prefix_counts.entry(value).or_default() += 1;
        }
        if let Some(value) = suffix {
            *suffix_counts.entry(value).or_default() += 1;
        }
        for fragment in dense_fragments {
            *dense_fragment_counts.entry(fragment).or_default() += 1;
        }
        if let Some(initial) = name.bytes().next() {
            *initial_counts.entry(initial).or_default() += 1;
        }
        selected_domains.insert(candidate.domain.clone());
        selected.push(selected_candidate);
        if selected.len() == top {
            sort_selected(&mut selected);
            return selected;
        }
    }

    // Relax only the pattern balance when necessary. Lexical-family and affix caps remain hard so
    // the tail of a large list cannot undo the diversity guarantees applied above.
    for candidate in candidates {
        let name = candidate
            .domain
            .split('.')
            .next()
            .unwrap_or_default()
            .to_string();
        let family_index = matching_family_index(&name, &families);
        let family_count = family_index.map_or(0, |index| families[index].2);
        let start_bigram = prefix_chars(&name, 2);
        let start_trigram = prefix_chars(&name, 3);
        let prefix = matched_prefix(&name);
        let suffix = matched_suffix(&name);
        let dense_fragments = dense_family_fragments(&name);
        if family_count >= family_limit
            || start_bigram_counts.get(&start_bigram).copied().unwrap_or(0) >= start_bigram_limit
            || start_trigram_counts
                .get(&start_trigram)
                .copied()
                .unwrap_or(0)
                >= start_trigram_limit
            || dense_fragments.iter().any(|fragment| {
                dense_fragment_counts.get(fragment).copied().unwrap_or(0) >= dense_fragment_limit
            })
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
            let mut candidate = candidate;
            assign_family(&mut candidate, &name, family_index, &mut families);
            *start_bigram_counts.entry(start_bigram).or_default() += 1;
            *start_trigram_counts.entry(start_trigram).or_default() += 1;
            if let Some(value) = prefix {
                *prefix_counts.entry(value).or_default() += 1;
            }
            if let Some(value) = suffix {
                *suffix_counts.entry(value).or_default() += 1;
            }
            for fragment in dense_fragments {
                *dense_fragment_counts.entry(fragment).or_default() += 1;
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
    sort_selected(&mut selected);
    selected
}

fn prefix_chars(name: &str, count: usize) -> String {
    name.chars().take(count).collect()
}

fn matching_family_index(name: &str, families: &[(String, String, usize)]) -> Option<usize> {
    families
        .iter()
        .position(|(_, representative, _)| same_phonetic_family(name, representative))
}

fn assign_family(
    candidate: &mut ScoredCandidate,
    name: &str,
    family_index: Option<usize>,
    families: &mut Vec<(String, String, usize)>,
) {
    if let Some(index) = family_index {
        families[index].2 += 1;
        candidate.family_key = families[index].0.clone();
    } else {
        let key = family_key(name);
        families.push((key.clone(), name.to_string(), 1));
        candidate.family_key = key;
    }
}

fn dense_family_fragments(name: &str) -> Vec<&'static str> {
    DENSE_FAMILY_FRAGMENTS
        .iter()
        .copied()
        .filter(|fragment| name.contains(fragment))
        .collect()
}

fn sort_selected(candidates: &mut [ScoredCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .generation_quality
            .total_score
            .cmp(&left.generation_quality.total_score)
            .then_with(|| right.scoring.total_score.cmp(&left.scoring.total_score))
            .then_with(|| left.domain.cmp(&right.domain))
    });
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

fn matched_dense_family_fragments(name: &str) -> Vec<&'static str> {
    DENSE_FAMILY_FRAGMENTS
        .iter()
        .copied()
        .filter(|fragment| name.contains(fragment))
        .collect()
}

const FLUENT_BIGRAMS: &[&str] = &[
    "al", "an", "ar", "av", "ca", "ce", "el", "en", "er", "ia", "io", "la", "le", "li", "lo", "lu",
    "ma", "mi", "na", "ne", "ni", "no", "or", "ra", "re", "ri", "ro", "sa", "se", "si", "so", "ta",
    "te", "ti", "va", "ve", "vi", "vo",
];
const FLUENT_TRIGRAMS: &[&str] = &[
    "ala", "ali", "ara", "ari", "ava", "avi", "ela", "eli", "iva", "lar", "lia", "lin", "lum",
    "mar", "nav", "nor", "nov", "ora", "ori", "rav", "ria", "rio", "sol", "val", "vel", "ver",
    "via", "vio",
];
const NATURAL_ONSETS: &[&str] = &[
    "al", "ar", "av", "el", "la", "le", "li", "lo", "lu", "ma", "mi", "na", "ne", "no", "ra", "re",
    "ri", "ro", "sa", "se", "si", "so", "va", "ve", "vi",
];
const NATURAL_ENDINGS: &[&str] = &[
    "a", "ia", "io", "na", "ne", "no", "ra", "re", "ria", "ro", "sa", "se", "ta", "va", "via", "vo",
];
const HARSH_ENDINGS: &[&str] = &[
    "beg", "bep", "fif", "fok", "hut", "muc", "woc", "wok", "wug", "coc", "gub", "kug", "puc",
    "tut", "eg", "ep", "if", "uc",
];
const SONORANTS: &[u8] = b"lrmnsvwy";
const HARD_CONSONANTS: &[u8] = b"bdgjkptfch";

fn generation_quality_score(name: &str) -> GenerationQualityScore {
    let bigrams: Vec<&str> = name
        .char_indices()
        .zip(name.char_indices().skip(2))
        .map(|((start, _), (end, _))| &name[start..end])
        .collect();
    let trigrams: Vec<&str> = name
        .char_indices()
        .zip(name.char_indices().skip(3))
        .map(|((start, _), (end, _))| &name[start..end])
        .collect();
    let fluent_bigrams = bigrams
        .iter()
        .filter(|part| FLUENT_BIGRAMS.contains(part))
        .count();
    let unlikely_bigrams = bigrams
        .iter()
        .filter(|part| UNLIKELY_BIGRAMS.contains(part))
        .count();
    let fluent_trigrams = trigrams
        .iter()
        .filter(|part| FLUENT_TRIGRAMS.contains(part))
        .count();

    let mut reasons = Vec::new();
    let mut penalty = (unlikely_bigrams * 7).min(21) as i16;
    let dense_family_hits = matched_dense_family_fragments(name).len();
    if dense_family_hits > 0 {
        penalty += (dense_family_hits * 4) as i16;
        reasons.push("common generated phonetic family".to_string());
    }
    let coverage = if bigrams.is_empty() {
        0
    } else {
        (fluent_bigrams * 20 / bigrams.len()) as i16
    };
    let phonotactic_score = (6 + coverage + (fluent_trigrams.min(6) * 2) as i16
        - (unlikely_bigrams * 5) as i16)
        .clamp(0, 40) as u8;
    if phonotactic_score >= 28 {
        reasons.push("fluent n-gram profile".to_string());
    } else if unlikely_bigrams > 0 {
        reasons.push("unlikely letter transitions".to_string());
    }

    let natural_onset = NATURAL_ONSETS.iter().any(|part| name.starts_with(part));
    let natural_ending = NATURAL_ENDINGS.iter().any(|part| name.ends_with(part));
    let harsh_ending = HARSH_ENDINGS.iter().any(|part| name.ends_with(part));
    let mut boundary_score = 8i16;
    if natural_onset {
        boundary_score += 5;
    }
    if natural_ending {
        boundary_score += 8;
    }
    if harsh_ending || AWKWARD_ENDINGS.iter().any(|ending| name.ends_with(ending)) {
        boundary_score -= 8;
        penalty += 10;
        reasons.push("harsh or artificial ending".to_string());
    } else if natural_onset && natural_ending {
        reasons.push("natural brand boundaries".to_string());
    }
    let boundary_score = boundary_score.clamp(0, 20) as u8;

    let letters = name.as_bytes();
    let vowel_count = letters.iter().filter(|byte| VOWELS.contains(byte)).count();
    let sonorant_count = letters
        .iter()
        .filter(|byte| SONORANTS.contains(byte))
        .count();
    let hard_count = letters
        .iter()
        .filter(|byte| HARD_CONSONANTS.contains(byte))
        .count();
    let vowel_ratio = vowel_count as f32 / letters.len().max(1) as f32;
    let mut rhythm_score = if (0.32..=0.58).contains(&vowel_ratio) {
        11i16
    } else {
        6
    };
    rhythm_score += (sonorant_count.min(4) * 2) as i16;
    if hard_count >= 3 && hard_count > sonorant_count + 1 {
        rhythm_score -= 7;
        penalty += 8;
        reasons.push("hard-stop dominated rhythm".to_string());
    }
    if has_artificial_cvc_loop(name) {
        rhythm_score -= 6;
        penalty += 9;
        reasons.push("artificial CVC cadence".to_string());
    }
    let rhythm_score = rhythm_score.clamp(0, 20) as u8;

    let mut naturalness_score = 12i16 + (fluent_trigrams.min(3) * 2) as i16;
    if has_repeated_fragment(name) {
        naturalness_score -= 15;
        penalty += 20;
        reasons.push("repeated syllable fragment".to_string());
    }
    if AWKWARD_VOWEL_TRANSITIONS
        .iter()
        .any(|transition| name.contains(transition))
        || UNCOMMON_DOUBLE_CONSONANTS
            .iter()
            .any(|pair| name.contains(pair))
    {
        naturalness_score -= 6;
        penalty += 7;
        reasons.push("low natural-language probability".to_string());
    }
    if matched_prefix(name).is_some() || matched_suffix(name).is_some() {
        naturalness_score -= 2;
        reasons.push("generated affix requires semantic review".to_string());
    }
    let naturalness_score = naturalness_score.clamp(0, 20) as u8;
    let penalty = penalty.clamp(0, 50) as u8;
    let total_score = (phonotactic_score as i16
        + boundary_score as i16
        + rhythm_score as i16
        + naturalness_score as i16
        - penalty as i16)
        .clamp(0, 100) as u8;

    GenerationQualityScore {
        total_score,
        phonotactic_score,
        boundary_score,
        rhythm_score,
        naturalness_score,
        penalty,
        reasons,
    }
}

fn has_artificial_cvc_loop(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 5 {
        return false;
    }
    let consonant_at = |index: usize| !VOWELS.contains(&bytes[index]);
    let alternating = (0..bytes.len()).all(|index| consonant_at(index) == (index % 2 == 0));
    let hard_onsets = (0..bytes.len())
        .step_by(2)
        .filter(|index| HARD_CONSONANTS.contains(&bytes[*index]))
        .count();
    alternating && hard_onsets >= 2
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
        && !has_artificial_onset(name)
        && longest_consonant_cluster(name) < 4
        && longest_repeated_run(name) < 3
}

fn has_artificial_onset(name: &str) -> bool {
    if ARTIFICIAL_ONSETS
        .iter()
        .any(|onset| name.starts_with(onset))
    {
        return true;
    }
    let bytes = name.as_bytes();
    if bytes.len() < 2 || VOWELS.contains(&bytes[0]) || VOWELS.contains(&bytes[1]) {
        return false;
    }
    !NATURAL_CONSONANT_ONSETS
        .iter()
        .any(|onset| name.starts_with(onset))
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

/// Stable, human-readable key for a generated phonetic family.
pub fn family_key(domain_or_name: &str) -> String {
    let name = domain_or_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let root = name.chars().take(3).collect::<String>();
    let skeleton = consonant_skeleton(&name);
    format!("{root}:{}", skeleton.chars().take(3).collect::<String>())
}

fn consonant_skeleton(name: &str) -> String {
    name.bytes()
        .filter(|byte| !VOWELS.contains(byte))
        .map(char::from)
        .collect()
}

fn normalized_vowels(name: &str) -> String {
    name.bytes()
        .map(|byte| {
            if VOWELS.contains(&byte) {
                'a'
            } else {
                char::from(byte)
            }
        })
        .collect()
}

fn trigram_overlap(left: &str, right: &str) -> usize {
    let left = left.as_bytes().windows(3).collect::<HashSet<_>>();
    right
        .as_bytes()
        .windows(3)
        .filter(|trigram| left.contains(trigram))
        .count()
}

fn bounded_edit_distance(left: &str, right: &str, limit: usize) -> usize {
    if left.len().abs_diff(right.len()) > limit {
        return limit + 1;
    }
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (row, left_byte) in left.bytes().enumerate() {
        let mut current = vec![row + 1];
        let mut row_min = row + 1;
        for (column, right_byte) in right.bytes().enumerate() {
            let value = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(previous[column] + usize::from(left_byte != right_byte));
            row_min = row_min.min(value);
            current.push(value);
        }
        if row_min > limit {
            return limit + 1;
        }
        previous = current;
    }
    previous[right.len()]
}

fn same_phonetic_family(left: &str, right: &str) -> bool {
    let left = left.split('.').next().unwrap_or_default();
    let right = right.split('.').next().unwrap_or_default();
    if left.chars().take(3).eq(right.chars().take(3)) {
        return true;
    }
    let left_skeleton = consonant_skeleton(left);
    let right_skeleton = consonant_skeleton(right);
    let normalized_left = normalized_vowels(left);
    let normalized_right = normalized_vowels(right);
    let skeleton_close = left_skeleton == right_skeleton;
    let root_close = bounded_edit_distance(&normalized_left, &normalized_right, 1) <= 1;
    let overlap = trigram_overlap(&normalized_left, &normalized_right);
    skeleton_close && root_close && overlap >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(count: usize, top: usize, tld: &str) -> CandidateGenerationConfig {
        CandidateGenerationConfig {
            count,
            top,
            tld: tld.to_string(),
            min_length: 5,
            max_length: 10,
            min_generation_quality: 0,
            max_per_family: 2,
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
        assert_eq!(patterns.len(), PATTERNS.len() + 1);

        let mut families = HashMap::new();
        for candidate in candidates {
            *families
                .entry(candidate.family_key.clone())
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
        let score_for = generation_quality_score;

        assert!(score_for("gobdeh").total_score < score_for("cladine").total_score);
        assert!(score_for("getow").total_score < score_for("cladine").total_score);
        assert!(score_for("cikiw").total_score < score_for("cladine").total_score);
        assert!(
            score_for("alalal").total_score < score_for("cladine").total_score,
            "alalal={} cladine={}",
            score_for("alalal").total_score,
            score_for("cladine").total_score
        );
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
        let clean = generation_quality_score("cladine");
        assert!(clean.total_score >= 50, "cladine={clean:?}");
    }

    #[test]
    fn fluent_examples_outrank_hard_synthetic_names() {
        let fluent = ["alarado", "alarava", "elaboca"];
        let weak = ["babeg", "dabep", "fabfif", "kabhut", "mabmuc", "nabfok"];
        let lowest_fluent = fluent
            .iter()
            .map(|name| generation_quality_score(name).total_score)
            .min()
            .unwrap();
        let highest_weak = weak
            .iter()
            .map(|name| generation_quality_score(name).total_score)
            .max()
            .unwrap();
        assert!(
            lowest_fluent > highest_weak,
            "fluent floor {lowest_fluent} did not beat weak ceiling {highest_weak}"
        );
    }

    #[test]
    fn top_results_have_a_granular_quality_distribution() {
        let candidates = generate_premium_candidates(&config(30_000, 50, "com"));
        let distinct: HashSet<_> = candidates
            .iter()
            .map(|candidate| candidate.generation_quality.total_score)
            .collect();
        assert!(
            distinct.len() >= 5,
            "only {} distinct scores",
            distinct.len()
        );
        assert!(candidates
            .windows(2)
            .all(|pair| pair[0].generation_quality.total_score
                >= pair[1].generation_quality.total_score));
    }

    #[test]
    fn minimum_generation_quality_filters_deterministically() {
        let mut strict = config(20_000, 40, "com");
        strict.min_length = 6;
        strict.max_length = 6;
        strict.min_generation_quality = 70;
        let first = generate_premium_candidates(&strict);
        let second = generate_premium_candidates(&strict);
        assert_eq!(first, second);
        assert_eq!(first.len(), 40);
        assert!(first
            .iter()
            .all(|candidate| candidate.generation_quality.total_score >= 70));
    }

    #[test]
    fn artificial_initial_clusters_are_rejected() {
        for name in [
            "dlaral", "bnavan", "tnoris", "fnavad", "knorap", "hnavik", "mnanuv", "rralun",
            "sselas", "wnoril", "kmarav",
        ] {
            assert!(!is_premium_name(name), "{name} should be rejected");
        }
        for name in ["bravio", "cladine", "travis"] {
            assert!(is_premium_name(name), "{name} should remain valid");
        }
    }

    #[test]
    fn related_nav_and_val_variants_share_families() {
        let nav = [
            "navane", "navare", "navere", "navale", "navali", "navias", "navion", "navior",
        ];
        let val = [
            "valaro", "valina", "valian", "valiar", "valene", "valere", "valore",
        ];
        for variants in [nav.as_slice(), val.as_slice()] {
            for variant in &variants[1..] {
                assert!(
                    same_phonetic_family(variants[0], variant),
                    "{} and {variant} should share a family",
                    variants[0]
                );
            }
        }
        assert!(!same_phonetic_family("navane", "zelori"));
    }

    #[test]
    fn configured_family_quota_is_hard_and_deterministic() {
        let mut strict = config(100_000, 100, "com");
        strict.min_length = 6;
        strict.max_length = 6;
        strict.min_generation_quality = 70;
        strict.max_per_family = 1;
        let first = generate_premium_candidates(&strict);
        let second = generate_premium_candidates(&strict);
        assert_eq!(first, second);
        let mut counts = HashMap::new();
        for candidate in first {
            *counts.entry(candidate.family_key).or_insert(0usize) += 1;
        }
        assert!(counts.values().all(|count| *count <= 1));
    }

    #[test]
    fn default_top_hundred_caps_families_and_starting_ngrams() {
        let mut strict = config(500_000, 100, "com");
        strict.min_length = 6;
        strict.max_length = 6;
        strict.min_generation_quality = 70;
        let candidates = generate_premium_candidates(&strict);
        assert_eq!(candidates.len(), 100);
        let mut families = HashMap::new();
        let mut bigrams = HashMap::new();
        let mut trigrams = HashMap::new();
        for candidate in candidates {
            let name = candidate.domain.split('.').next().unwrap();
            *families.entry(candidate.family_key).or_insert(0usize) += 1;
            *bigrams.entry(prefix_chars(name, 2)).or_insert(0usize) += 1;
            *trigrams.entry(prefix_chars(name, 3)).or_insert(0usize) += 1;
        }
        assert!(families.values().all(|count| *count <= 2));
        assert!(bigrams.values().all(|count| *count <= 9));
        assert!(trigrams.values().all(|count| *count <= 4));
    }

    #[test]
    fn dense_phonetic_fragments_are_capped_in_top_results() {
        let mut strict = config(100_000, 100, "com");
        strict.min_length = 6;
        strict.max_length = 6;
        strict.min_generation_quality = 70;
        let candidates = generate_premium_candidates(&strict);
        assert_eq!(candidates.len(), 100);
        for fragment in DENSE_FAMILY_FRAGMENTS {
            let count = candidates
                .iter()
                .filter(|candidate| {
                    candidate
                        .domain
                        .split('.')
                        .next()
                        .unwrap()
                        .contains(fragment)
                })
                .count();
            assert!(count <= 4, "fragment {fragment} appeared {count} times");
        }
    }

    #[test]
    fn exclusions_are_replaced_with_new_quality_candidates() {
        let mut strict = config(20_000, 20, "com");
        strict.min_length = 6;
        strict.max_length = 6;
        strict.min_generation_quality = 70;
        let first = generate_premium_candidates(&strict);
        let excluded_domains = first
            .iter()
            .map(|candidate| candidate.domain.clone())
            .collect::<HashSet<_>>();
        let (second, skipped) = generate_premium_candidates_with_filters_and_stats(
            &strict,
            &CandidateGenerationFilters {
                excluded_domains: excluded_domains.clone(),
                ..Default::default()
            },
        );
        assert_eq!(second.len(), 20);
        assert!(skipped > 0);
        assert!(second.iter().all(|candidate| {
            candidate.generation_quality.total_score >= 70
                && !excluded_domains.contains(&candidate.domain)
        }));
    }

    #[test]
    fn exact_length_only_returns_matching_labels_and_is_deterministic() {
        let mut exact = config(10_000, 500, "com");
        exact.min_length = 5;
        exact.max_length = 5;
        let first = generate_premium_candidates(&exact);
        let second = generate_premium_candidates(&exact);
        assert_eq!(first, second);
        assert_eq!(first.len(), 500);
        assert!(first
            .iter()
            .all(|candidate| candidate.domain.split('.').next().unwrap().len() == 5));
    }

    #[test]
    fn length_range_is_inclusive() {
        let mut ranged = config(5_000, 500, "com");
        ranged.min_length = 6;
        ranged.max_length = 7;
        let candidates = generate_premium_candidates(&ranged);
        assert_eq!(candidates.len(), 500);
        assert!(candidates.iter().all(|candidate| {
            (6..=7).contains(&candidate.domain.split('.').next().unwrap().len())
        }));
    }

    #[test]
    fn impossible_length_configuration_returns_no_candidates() {
        let mut invalid = config(100, 20, "com");
        invalid.min_length = 11;
        invalid.max_length = 5;
        assert!(generate_premium_candidates(&invalid).is_empty());
    }

    #[test]
    fn label_filters_apply_to_second_level_domain() {
        let config = config(30_000, 50, "com");
        let contains = generate_premium_candidates_with_filters(
            &config,
            &CandidateGenerationFilters {
                contains: Some("ar".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(contains.len(), 50);
        assert!(contains.iter().all(|candidate| candidate
            .domain
            .split('.')
            .next()
            .unwrap()
            .contains("ar")));

        let starts = generate_premium_candidates_with_filters(
            &config,
            &CandidateGenerationFilters {
                starts_with: Some("la".to_string()),
                ..Default::default()
            },
        );
        assert!(starts
            .iter()
            .all(|candidate| candidate.domain.starts_with("la")));

        let ends = generate_premium_candidates_with_filters(
            &config,
            &CandidateGenerationFilters {
                ends_with: Some("ra".to_string()),
                ..Default::default()
            },
        );
        assert!(ends.iter().all(|candidate| candidate
            .domain
            .split('.')
            .next()
            .unwrap()
            .ends_with("ra")));
    }

    #[test]
    fn historical_domains_are_excluded_and_replaced_deterministically() {
        let config = config(5_000, 20, "com");
        let baseline = generate_premium_candidates(&config);
        let excluded_domains = baseline
            .iter()
            .take(5)
            .map(|candidate| candidate.domain.clone())
            .collect();
        let filters = CandidateGenerationFilters {
            excluded_domains,
            ..Default::default()
        };
        let (first, skipped) =
            generate_premium_candidates_with_filters_and_stats(&config, &filters);
        let second = generate_premium_candidates_with_filters(&config, &filters);
        assert_eq!(first, second);
        assert_eq!(first.len(), 20);
        assert!(skipped >= 5);
        assert!(first
            .iter()
            .all(|candidate| !filters.excluded_domains.contains(&candidate.domain)));
    }
}
