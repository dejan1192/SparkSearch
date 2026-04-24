use crate::index::LauncherEntry;
use crate::{history::RunHistory, index::EntryKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub entry: LauncherEntry,
    pub score: i32,
}

pub fn search(
    entries: &[LauncherEntry],
    query: &str,
    limit: usize,
    history: &RunHistory,
) -> Vec<SearchResult> {
    let query = query.trim();

    let mut results: Vec<_> = entries
        .iter()
        .filter_map(|entry| {
            if query.is_empty() {
                return Some(SearchResult {
                    entry: entry.clone(),
                    score: 0,
                });
            }

            score_entry(entry, query, history).map(|score| SearchResult {
                entry: entry.clone(),
                score,
            })
        })
        .collect();

    results.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            a.entry
                .name
                .to_lowercase()
                .cmp(&b.entry.name.to_lowercase())
        })
    });
    results.truncate(limit);
    results
}

fn score_entry(entry: &LauncherEntry, query: &str, history: &RunHistory) -> Option<i32> {
    let name_score = fuzzy_score(&entry.name, query);
    let path_score = fuzzy_score(&entry.target.file, query).map(|score| score - 20);

    let base = name_score.into_iter().chain(path_score).max()?;
    let history_boost = history.score_boost(entry);
    let cold_start_boost = if history.has_record(entry) {
        0
    } else {
        executable_bonus(entry)
    };

    Some(base + history_boost + cold_start_boost)
}

fn executable_bonus(entry: &LauncherEntry) -> i32 {
    let extension = entry
        .target
        .file
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let is_executable = matches!(extension.as_deref(), Some("exe" | "cmd" | "bat"));

    match (entry.kind, is_executable) {
        (EntryKind::PathExecutable, _) => 90,
        (EntryKind::BuiltIn, _) => 70,
        (EntryKind::Bookmark, _) => 0,
        (_, true) => 60,
        (EntryKind::StartMenu, _) => 25,
    }
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i32> {
    let candidate_lower = candidate.to_lowercase();
    let query_lower = query.to_lowercase();

    if candidate_lower.contains(&query_lower) {
        let index = candidate_lower.find(&query_lower).unwrap_or(0) as i32;
        return Some(200 - index);
    }

    let mut score = 0;
    let mut last_match: Option<usize> = None;
    let mut search_from = 0;

    for query_char in query_lower.chars() {
        let remaining = &candidate_lower[search_from..];
        let Some(relative_index) = remaining.find(query_char) else {
            return None;
        };

        let absolute_index = search_from + relative_index;
        score += 20;

        if Some(absolute_index.saturating_sub(1)) == last_match {
            score += 15;
        }

        if absolute_index == 0
            || candidate_lower
                .as_bytes()
                .get(absolute_index.saturating_sub(1))
                .is_some_and(|byte| matches!(byte, b' ' | b'-' | b'_' | b'\\' | b'/'))
        {
            score += 10;
        }

        last_match = Some(absolute_index);
        search_from = absolute_index + query_char.len_utf8();
    }

    Some(score)
}
