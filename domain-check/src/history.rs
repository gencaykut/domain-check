use domain_check_lib::{CheckMethod, DomainResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_HISTORY_FILE: &str = ".domain-check-history.jsonl";
pub const UNKNOWN_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum HistoryStatus {
    Available,
    Taken,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryRecord {
    domain: String,
    status: HistoryStatus,
    method: CheckMethod,
    timestamp: u64,
}

#[derive(Debug, Default)]
pub struct HistorySelection {
    pub cached: Vec<DomainResult>,
    pub pending: Vec<String>,
    pub duplicate_count: usize,
}

#[derive(Debug)]
pub struct HistoryStore {
    path: PathBuf,
    records: HashMap<String, HistoryRecord>,
}

impl HistoryStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.into();
        let mut records = HashMap::new();
        match fs::File::open(&path) {
            Ok(file) => {
                for line in BufReader::new(file).lines() {
                    let Ok(line) = line else { continue };
                    let Ok(record) = serde_json::from_str::<HistoryRecord>(&line) else {
                        continue;
                    };
                    records.insert(normalize_domain(&record.domain), record);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(Self { path, records })
    }

    pub fn select(&self, domains: impl IntoIterator<Item = String>) -> HistorySelection {
        self.select_at(domains, now_timestamp())
    }

    pub fn reusable_domains(&self) -> HashSet<String> {
        let now = now_timestamp();
        self.records
            .iter()
            .filter(|(_, record)| record_is_reusable(record, now))
            .map(|(domain, _)| domain.clone())
            .collect()
    }

    fn select_at(&self, domains: impl IntoIterator<Item = String>, now: u64) -> HistorySelection {
        let mut selection = HistorySelection::default();
        let mut seen = HashSet::new();
        for domain in domains {
            let normalized = normalize_domain(&domain);
            if normalized.is_empty() || !seen.insert(normalized.clone()) {
                selection.duplicate_count += 1;
                continue;
            }
            match self.records.get(&normalized) {
                Some(record) if record_is_reusable(record, now) => {
                    selection.cached.push(record_to_result(record));
                }
                _ => selection.pending.push(normalized),
            }
        }
        selection
    }

    pub fn append_results(
        &mut self,
        results: &[DomainResult],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if results.is_empty() {
            return Ok(());
        }
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let timestamp = now_timestamp();
        for result in results {
            let record = HistoryRecord {
                domain: normalize_domain(&result.domain),
                status: status_from_result(result),
                method: result.method_used.clone(),
                timestamp,
            };
            let mut line = serde_json::to_vec(&record)?;
            line.push(b'\n');
            // One append write keeps each JSONL record intact across concurrent processes.
            file.write_all(&line)?;
            self.records.insert(record.domain.clone(), record);
        }
        file.flush()?;
        Ok(())
    }

    pub fn clear(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

pub fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn record_is_reusable(record: &HistoryRecord, now: u64) -> bool {
    record.status != HistoryStatus::Unknown
        || now.saturating_sub(record.timestamp) < UNKNOWN_TTL_SECS
}

fn status_from_result(result: &DomainResult) -> HistoryStatus {
    match result.available {
        Some(true) => HistoryStatus::Available,
        Some(false) => HistoryStatus::Taken,
        None => HistoryStatus::Unknown,
    }
}

fn record_to_result(record: &HistoryRecord) -> DomainResult {
    DomainResult {
        domain: record.domain.clone(),
        available: match record.status {
            HistoryStatus::Available => Some(true),
            HistoryStatus::Taken => Some(false),
            HistoryStatus::Unknown => None,
        },
        info: None,
        check_duration: None,
        method_used: record.method.clone(),
        error_message: (record.status == HistoryStatus::Unknown)
            .then(|| "reused from recent history; retry is allowed after 24 hours".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn result(domain: &str, available: Option<bool>) -> DomainResult {
        DomainResult {
            domain: domain.to_string(),
            available,
            info: None,
            check_duration: None,
            method_used: CheckMethod::Whois,
            error_message: None,
        }
    }

    #[test]
    fn history_persists_and_reuses_terminal_results() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let mut store = HistoryStore::load(&path).unwrap();
        store
            .append_results(&[result("Example.COM", Some(false))])
            .unwrap();

        let loaded = HistoryStore::load(&path).unwrap();
        let selected = loaded.select(vec!["example.com".to_string()]);
        assert!(selected.pending.is_empty());
        assert_eq!(selected.cached.len(), 1);
        assert_eq!(selected.cached[0].available, Some(false));
    }

    #[test]
    fn unknown_results_expire_after_twenty_four_hours() {
        let store = HistoryStore {
            path: PathBuf::new(),
            records: HashMap::from([(
                "retry.com".to_string(),
                HistoryRecord {
                    domain: "retry.com".to_string(),
                    status: HistoryStatus::Unknown,
                    method: CheckMethod::Unknown,
                    timestamp: 100,
                },
            )]),
        };
        assert_eq!(
            store
                .select_at(vec!["retry.com".to_string()], 100 + UNKNOWN_TTL_SECS - 1)
                .cached
                .len(),
            1
        );
        assert_eq!(
            store
                .select_at(vec!["retry.com".to_string()], 100 + UNKNOWN_TTL_SECS)
                .pending,
            vec!["retry.com"]
        );
    }

    #[test]
    fn duplicate_inputs_are_removed_within_one_run() {
        let store = HistoryStore {
            path: PathBuf::new(),
            records: HashMap::new(),
        };
        let selected = store.select(vec![
            "One.com".to_string(),
            "one.com.".to_string(),
            "two.com".to_string(),
        ]);
        assert_eq!(selected.pending, vec!["one.com", "two.com"]);
        assert_eq!(selected.duplicate_count, 1);
    }
}
