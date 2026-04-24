use std::{env, fs, path::PathBuf};

use crate::actions::LaunchTarget;
use crate::index::{EntryKind, LauncherEntry};

pub fn load_default_browser_bookmarks() -> Vec<LauncherEntry> {
    let Some(path) = chromium_bookmarks_path() else {
        return Vec::new();
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };

    let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    if let Some(roots) = root.get("roots").and_then(serde_json::Value::as_object) {
        for (_, subtree) in roots {
            walk(subtree, &mut entries);
        }
    }
    entries
}

fn walk(node: &serde_json::Value, out: &mut Vec<LauncherEntry>) {
    match node.get("type").and_then(serde_json::Value::as_str) {
        Some("url") => {
            let Some(url) = node.get("url").and_then(serde_json::Value::as_str) else {
                return;
            };
            if url.is_empty() {
                return;
            }
            let name = node
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(url);
            let display_name = if name.is_empty() {
                url.to_string()
            } else {
                name.to_string()
            };
            out.push(LauncherEntry {
                name: display_name,
                kind: EntryKind::Bookmark,
                target: LaunchTarget::new(url),
            });
        }
        Some("folder") => {
            if let Some(children) = node.get("children").and_then(serde_json::Value::as_array) {
                for child in children {
                    walk(child, out);
                }
            }
        }
        _ => {}
    }
}

fn chromium_bookmarks_path() -> Option<PathBuf> {
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let candidates = [
        local.join(r"Google\Chrome\User Data\Default\Bookmarks"),
        local.join(r"Microsoft\Edge\User Data\Default\Bookmarks"),
        local.join(r"BraveSoftware\Brave-Browser\User Data\Default\Bookmarks"),
        local.join(r"Vivaldi\User Data\Default\Bookmarks"),
        local.join(r"Chromium\User Data\Default\Bookmarks"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}
