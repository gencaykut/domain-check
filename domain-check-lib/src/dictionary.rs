//! Optional compact dictionary-derived model for generated-name quality.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

const MAX_INDEXED_LENGTH: usize = 15;
const BIGRAM_COUNT: usize = 26 * 26;
const TRIGRAM_COUNT: usize = 26 * 26 * 26;

#[derive(Debug)]
pub(crate) struct DictionaryModel {
    bigrams: Box<[u32; BIGRAM_COUNT]>,
    trigrams: Box<[u32]>,
    exact: [Vec<u64>; MAX_INDEXED_LENGTH + 1],
    deletions: [Vec<u64>; MAX_INDEXED_LENGTH + 1],
    stats: DictionaryModelStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictionaryModelStats {
    pub words: usize,
    pub exact_hashes: usize,
    pub deletion_hashes: usize,
    pub estimated_bytes: usize,
    pub load_millis: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DictionarySignal {
    pub ngram_score: u8,
    pub exact_word: bool,
    pub one_edit_neighbor: bool,
}

static MODEL: OnceLock<Option<DictionaryModel>> = OnceLock::new();

pub(crate) fn optional_dictionary_model() -> Option<&'static DictionaryModel> {
    MODEL
        .get_or_init(|| dictionary_path().and_then(|path| DictionaryModel::load(path).ok()))
        .as_ref()
}

/// Statistics for the optional dictionary model, loading it once when available.
pub fn dictionary_model_stats() -> Option<DictionaryModelStats> {
    optional_dictionary_model().map(DictionaryModel::stats)
}

fn dictionary_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DOMAIN_CHECK_WORDS_FILE") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let local = PathBuf::from("words_alpha.txt");
    local.is_file().then_some(local)
}

impl DictionaryModel {
    pub(crate) fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let started = Instant::now();
        let mut bigrams = Box::new([0u32; BIGRAM_COUNT]);
        let mut trigrams = vec![0u32; TRIGRAM_COUNT].into_boxed_slice();
        let mut exact: [Vec<u64>; MAX_INDEXED_LENGTH + 1] = std::array::from_fn(|_| Vec::new());
        let mut deletions: [Vec<u64>; MAX_INDEXED_LENGTH + 1] = std::array::from_fn(|_| Vec::new());
        let mut words = 0usize;

        for line in BufReader::new(File::open(path)?).lines() {
            let line = line?;
            let word = line.trim().as_bytes();
            if word.len() < 2
                || word.len() > MAX_INDEXED_LENGTH
                || !word.iter().all(u8::is_ascii_lowercase)
            {
                continue;
            }
            words += 1;
            for gram in word.windows(2) {
                bigrams[bigram_index(gram)] = bigrams[bigram_index(gram)].saturating_add(1);
            }
            for gram in word.windows(3) {
                trigrams[trigram_index(gram)] = trigrams[trigram_index(gram)].saturating_add(1);
            }
            exact[word.len()].push(stable_hash(word));
            if word.len() >= 3 {
                for removed in 0..word.len() {
                    deletions[word.len()].push(hash_without(word, removed));
                }
            }
        }
        for hashes in exact.iter_mut().chain(deletions.iter_mut()) {
            hashes.sort_unstable();
            hashes.dedup();
        }
        let exact_hashes = exact.iter().map(Vec::len).sum::<usize>();
        let deletion_hashes = deletions.iter().map(Vec::len).sum::<usize>();
        let estimated_bytes = BIGRAM_COUNT * size_of::<u32>()
            + TRIGRAM_COUNT * size_of::<u32>()
            + (exact_hashes + deletion_hashes) * size_of::<u64>();
        let stats = DictionaryModelStats {
            words,
            exact_hashes,
            deletion_hashes,
            estimated_bytes,
            load_millis: started.elapsed().as_millis(),
        };
        Ok(Self {
            bigrams,
            trigrams,
            exact,
            deletions,
            stats,
        })
    }

    pub(crate) fn stats(&self) -> DictionaryModelStats {
        self.stats
    }

    pub(crate) fn signal(&self, name: &str) -> DictionarySignal {
        let bytes = name.as_bytes();
        if bytes.len() > MAX_INDEXED_LENGTH || !bytes.iter().all(u8::is_ascii_lowercase) {
            return DictionarySignal {
                ngram_score: 0,
                exact_word: false,
                one_edit_neighbor: false,
            };
        }
        let bigram_score = average_log_score(
            bytes
                .windows(2)
                .map(|gram| self.bigrams[bigram_index(gram)]),
            16.0,
        );
        let trigram_score = average_log_score(
            bytes
                .windows(3)
                .map(|gram| self.trigrams[trigram_index(gram)]),
            13.0,
        );
        let ngram_score = (bigram_score + trigram_score).clamp(0, 20) as u8;
        let hash = stable_hash(bytes);
        let exact_word = self.exact[bytes.len()].binary_search(&hash).is_ok();
        let one_edit_neighbor = !exact_word && self.has_one_edit_neighbor(bytes, hash);
        DictionarySignal {
            ngram_score,
            exact_word,
            one_edit_neighbor,
        }
    }

    fn has_one_edit_neighbor(&self, word: &[u8], hash: u64) -> bool {
        // A longer dictionary word can delete one letter to become this candidate.
        if word.len() < MAX_INDEXED_LENGTH
            && self.deletions[word.len() + 1].binary_search(&hash).is_ok()
        {
            return true;
        }
        for removed in 0..word.len() {
            let deletion = hash_without(word, removed);
            // Candidate deletion equals a shorter word, or shares a deletion signature with a
            // same-length word (one substitution/transposition-like difference).
            if self.exact[word.len().saturating_sub(1)]
                .binary_search(&deletion)
                .is_ok()
                || self.deletions[word.len()].binary_search(&deletion).is_ok()
            {
                return true;
            }
        }
        false
    }
}

fn average_log_score(values: impl Iterator<Item = u32>, denominator: f32) -> i16 {
    let logs = values
        .map(|count| (count as f32 + 1.0).log2())
        .collect::<Vec<_>>();
    if logs.is_empty() {
        return 0;
    }
    let average = logs.iter().sum::<f32>() / logs.len() as f32;
    (average / denominator * 10.0).round() as i16
}

fn bigram_index(gram: &[u8]) -> usize {
    usize::from(gram[0] - b'a') * 26 + usize::from(gram[1] - b'a')
}

fn trigram_index(gram: &[u8]) -> usize {
    (usize::from(gram[0] - b'a') * 26 + usize::from(gram[1] - b'a')) * 26
        + usize::from(gram[2] - b'a')
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hash_without(bytes: &[u8], removed: usize) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for (index, byte) in bytes.iter().enumerate() {
        if index != removed {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture() -> (tempfile::NamedTempFile, DictionaryModel) {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "planet\nplaner\nplans\nbrand\nbrands\nclean\nclear").unwrap();
        let model = DictionaryModel::load(file.path()).unwrap();
        (file, model)
    }

    #[test]
    fn compact_model_detects_exact_and_one_edit_neighbors() {
        let (_file, model) = fixture();
        assert!(model.signal("planet").exact_word);
        assert!(model.signal("planat").one_edit_neighbor);
        assert!(model.signal("brandsx").one_edit_neighbor);
        assert!(!model.signal("zuviko").exact_word);
    }

    #[test]
    fn model_builds_frequency_tables_and_reports_compact_size() {
        let (_file, model) = fixture();
        assert!(model.signal("planet").ngram_score > model.signal("qxzqxx").ngram_score);
        let stats = model.stats();
        assert_eq!(stats.words, 7);
        assert!(stats.estimated_bytes < 100_000);
    }
}
