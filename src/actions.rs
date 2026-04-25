use std::path::Path;

use crate::shortcuts::ShortcutConfig;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Normal,
    Elevated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchTarget {
    pub file: String,
    pub params: String,
    pub directory: Option<String>,
}

impl LaunchTarget {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            params: String::new(),
            directory: None,
        }
    }
}

pub fn terminal_command_display(command: &str) -> Option<&str> {
    let command = terminal_command_text(command)?;

    if command.trim().is_empty() {
        None
    } else {
        Some(command.trim())
    }
}

pub fn launch(target: &LaunchTarget, mode: LaunchMode) -> Result<(), String> {
    if target.file.trim().is_empty() {
        return Err("Nothing to launch yet.".to_string());
    }

    let operation = match mode {
        LaunchMode::Normal => "open",
        LaunchMode::Elevated => "runas",
    };

    let op = wide_null(operation);
    let file = wide_null(target.file.trim());
    let params = wide_null(target.params.trim());
    let directory = target
        .directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(wide_null);

    let params_ptr = if target.params.trim().is_empty() {
        std::ptr::null()
    } else {
        params.as_ptr()
    };

    let directory_ptr = directory
        .as_ref()
        .map(|value| value.as_ptr())
        .unwrap_or(std::ptr::null());

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            params_ptr,
            directory_ptr,
            SW_SHOWNORMAL,
        )
    };

    if result as isize <= 32 {
        let code = result as isize;
        return Err(format!(
            "Windows could not launch `{}`. ShellExecuteW returned code {}.",
            target.file, code
        ));
    }

    Ok(())
}

pub fn target_from_path(path: &Path) -> LaunchTarget {
    let directory = path
        .parent()
        .map(|parent| parent.to_string_lossy().to_string());

    LaunchTarget {
        file: path.to_string_lossy().to_string(),
        params: String::new(),
        directory,
    }
}

pub fn target_from_raw_command(command: &str, shortcuts: &ShortcutConfig) -> LaunchTarget {
    let command = command.trim();

    if let Some(command) = terminal_command_text(command) {
        return terminal_command_target(command);
    }

    if let Some(target) = shortcuts.resolve(command) {
        return LaunchTarget::new(target);
    }

    if let Some(stripped) = command.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let file = stripped[..end_quote].to_string();
            let params = stripped[end_quote + 1..].trim().to_string();
            return LaunchTarget {
                file,
                params,
                directory: None,
            };
        }
    }

    let mut parts = command.splitn(2, char::is_whitespace);
    let file = parts.next().unwrap_or_default().to_string();
    let params = parts.next().unwrap_or_default().trim().to_string();

    LaunchTarget {
        file,
        params,
        directory: None,
    }
}

fn terminal_command_text(command: &str) -> Option<&str> {
    let command = command.trim();
    let rest = command.strip_prefix('>')?;

    if rest.is_empty() {
        return Some("");
    }

    if rest.starts_with(char::is_whitespace) {
        return Some(rest.trim_start());
    }

    None
}

fn terminal_command_target(command: &str) -> LaunchTarget {
    if command.trim().is_empty() {
        return LaunchTarget::new("");
    }

    LaunchTarget {
        file: "cmd.exe".to_string(),
        params: format!("/K {}", command.trim()),
        directory: None,
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::target_from_raw_command;
    use crate::shortcuts::ShortcutConfig;

    fn shortcuts() -> ShortcutConfig {
        ShortcutConfig::defaults()
    }

    #[test]
    fn google_shortcut_builds_search_url() {
        let target = target_from_raw_command("!g rust egui launcher", &shortcuts());

        assert_eq!(
            target.file,
            "https://www.google.com/search?q=rust+egui+launcher"
        );
        assert!(target.params.is_empty());
    }

    #[test]
    fn duckduckgo_shortcut_builds_search_url() {
        let target = target_from_raw_command("!d privacy search", &shortcuts());

        assert_eq!(target.file, "https://duckduckgo.com/?q=privacy+search");
        assert!(target.params.is_empty());
    }

    #[test]
    fn github_shortcut_builds_search_url() {
        let target = target_from_raw_command("!git owner/repo issue", &shortcuts());

        assert_eq!(
            target.file,
            "https://github.com/search?q=owner%2Frepo+issue"
        );
        assert!(target.params.is_empty());
    }

    #[test]
    fn youtube_shortcut_builds_search_url() {
        let target = target_from_raw_command("!y lo-fi beats", &shortcuts());

        assert_eq!(
            target.file,
            "https://www.youtube.com/results?search_query=lo-fi+beats"
        );
        assert!(target.params.is_empty());
    }

    #[test]
    fn chatgpt_shortcut_builds_prompt_url() {
        let target = target_from_raw_command("!gpt explain rust lifetimes", &shortcuts());

        assert_eq!(target.file, "https://chatgpt.com/?q=explain+rust+lifetimes");
        assert!(target.params.is_empty());
    }

    #[test]
    fn bare_search_shortcuts_open_homepages() {
        for (shortcut, url) in [
            ("!g", "https://google.com/"),
            ("!d", "https://duckduckgo.com/"),
            ("!git", "https://github.com/"),
            ("!y", "https://www.youtube.com/"),
        ] {
            let target = target_from_raw_command(shortcut, &shortcuts());

            assert_eq!(target.file, url);
            assert!(target.params.is_empty());
        }
    }

    #[test]
    fn bare_ai_shortcuts_open_homepages() {
        for (shortcut, url) in [
            ("!gpt", "https://chatgpt.com/"),
            ("!claude", "https://claude.ai/"),
            ("!kimi", "https://www.kimi.com/"),
        ] {
            let target = target_from_raw_command(shortcut, &shortcuts());

            assert_eq!(target.file, url);
            assert!(target.params.is_empty());
        }
    }

    #[test]
    fn non_searchable_shortcut_with_extra_text_falls_back_to_raw_command() {
        let target = target_from_raw_command("!claude explain rust lifetimes", &shortcuts());

        assert_eq!(target.file, "!claude");
        assert_eq!(target.params, "explain rust lifetimes");
    }

    #[test]
    fn terminal_prefix_runs_command_in_terminal() {
        let target = target_from_raw_command("> cargo test", &shortcuts());

        assert_eq!(target.file, "cmd.exe");
        assert_eq!(target.params, "/K cargo test");
        assert!(target.directory.is_none());
    }

    #[test]
    fn terminal_prefix_without_command_waits_for_input() {
        let target = target_from_raw_command(">", &shortcuts());

        assert!(target.file.is_empty());
        assert!(target.params.is_empty());
    }
}
