use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{actions::LaunchTarget, index::LauncherEntry};

#[derive(Clone, Debug, Default)]
pub struct RunHistory {
    records: HashMap<String, RunRecord>,
    path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LoadedRunHistory {
    pub history: RunHistory,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct RunRecord {
    count: u32,
    last_run: u64,
}

impl RunHistory {
    pub fn load() -> LoadedRunHistory {
        let path = history_path();
        let status = format!("History: {}", path.display());

        let records = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .map(|contents| parse_history(&contents))
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        LoadedRunHistory {
            history: Self {
                records,
                path: Some(path),
            },
            status,
        }
    }

    pub fn record(&mut self, target: &LaunchTarget) {
        if target.file.trim().is_empty() {
            return;
        }

        let key = target_key(target);
        let record = self.records.entry(key).or_default();
        record.count = record.count.saturating_add(1);
        record.last_run = unix_now();
        self.save();
    }

    pub fn score_boost(&self, entry: &LauncherEntry) -> i32 {
        let Some(record) = self.records.get(&target_key(&entry.target)) else {
            return 0;
        };

        let count_boost = record.count.min(100) as i32 * 500;
        let recency_boost = if record.last_run > 0 { 50 } else { 0 };
        10_000 + count_boost + recency_boost
    }

    pub fn has_record(&self, entry: &LauncherEntry) -> bool {
        self.records.contains_key(&target_key(&entry.target))
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut rows: Vec<_> = self.records.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));

        let contents = rows
            .into_iter()
            .map(|(key, record)| {
                format!(
                    "{}\t{}\t{}",
                    escape_field(key),
                    record.count,
                    record.last_run
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let _ = fs::write(path, contents);
    }
}

fn history_path() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Spark Run").join("run-history.tsv"))
        .unwrap_or_else(|| PathBuf::from("run-history.tsv"))
}

fn parse_history(contents: &str) -> HashMap<String, RunRecord> {
    let mut records = HashMap::new();

    for line in contents.lines() {
        let mut parts = line.split('\t');
        let Some(key) = parts.next().and_then(unescape_field) else {
            continue;
        };
        let count = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let last_run = parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);

        if !key.is_empty() && count > 0 {
            records.insert(key, RunRecord { count, last_run });
        }
    }

    records
}

fn target_key(target: &LaunchTarget) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        target.file.trim().to_lowercase(),
        target.params.trim().to_lowercase(),
        target
            .directory
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase()
    )
}

fn escape_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape_field(value: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next()? {
            '\\' => output.push('\\'),
            't' => output.push('\t'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            other => {
                output.push('\\');
                output.push(other);
            }
        }
    }

    Some(output)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _history_path_for_tests(path: &Path) -> PathBuf {
    path.join("run-history.tsv")
}

