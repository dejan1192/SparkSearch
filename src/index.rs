use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use crate::actions::{target_from_path, LaunchTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherEntry {
    pub name: String,
    pub kind: EntryKind,
    pub target: LaunchTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    StartMenu,
    PathExecutable,
    BuiltIn,
    Bookmark,
}

pub fn index_entries() -> Vec<LauncherEntry> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    for (name, command) in [
        ("Command Prompt", "cmd.exe"),
        ("PowerShell", "powershell.exe"),
        ("File Explorer", "explorer.exe"),
        ("Task Manager", "taskmgr.exe"),
        ("Registry Editor", "regedit.exe"),
        ("Calculator", "calc.exe"),
        ("Notepad", "notepad.exe"),
    ] {
        push_entry(
            &mut entries,
            &mut seen,
            LauncherEntry {
                name: name.to_string(),
                kind: EntryKind::BuiltIn,
                target: LaunchTarget::new(command),
            },
        );
    }

    for root in start_menu_roots() {
        visit_dir(&root, 0, &mut |path| {
            if is_launchable(path) {
                push_path_entry(&mut entries, &mut seen, path, EntryKind::StartMenu);
            }
        });
    }

    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            if let Ok(read_dir) = fs::read_dir(dir) {
                for item in read_dir.flatten() {
                    let path = item.path();
                    if is_path_executable(&path) {
                        push_path_entry(&mut entries, &mut seen, &path, EntryKind::PathExecutable);
                    }
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.target.file.cmp(&b.target.file))
    });

    entries
}

fn push_path_entry(
    entries: &mut Vec<LauncherEntry>,
    seen: &mut HashSet<String>,
    path: &Path,
    kind: EntryKind,
) {
    let Some(name) = display_name(path) else {
        return;
    };

    push_entry(
        entries,
        seen,
        LauncherEntry {
            name,
            kind,
            target: target_from_path(path),
        },
    );
}

fn push_entry(entries: &mut Vec<LauncherEntry>, seen: &mut HashSet<String>, entry: LauncherEntry) {
    let key = format!(
        "{}|{}",
        entry.name.to_lowercase(),
        entry.target.file.to_lowercase()
    );

    if seen.insert(key) {
        entries.push(entry);
    }
}

fn start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(program_data) = env::var_os("ProgramData") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    if let Some(app_data) = env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    roots
}

fn visit_dir(root: &Path, depth: usize, visitor: &mut impl FnMut(&Path)) {
    if depth > 5 {
        return;
    }

    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };

    for item in read_dir.flatten() {
        let path = item.path();
        if path.is_dir() {
            visit_dir(&path, depth + 1, visitor);
        } else {
            visitor(&path);
        }
    }
}

fn display_name(path: &Path) -> Option<String> {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().replace('_', " "))
}

fn is_launchable(path: &Path) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("lnk" | "exe" | "bat" | "cmd" | "ps1")
    )
}

fn is_path_executable(path: &Path) -> bool {
    matches!(extension(path).as_deref(), Some("exe" | "bat" | "cmd"))
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}
