use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct ShortcutConfig {
    commands: HashMap<String, ShortcutCommand>,
}

#[derive(Clone, Debug)]
pub struct LoadedShortcutConfig {
    pub config: ShortcutConfig,
    pub status: String,
}

#[derive(Clone, Debug, Default)]
struct PartialShortcutCommand {
    target: Option<String>,
    search_target: Option<String>,
}

#[derive(Clone, Debug)]
struct ShortcutCommand {
    target: String,
    search_target: Option<String>,
}

impl ShortcutConfig {
    pub fn load_or_create() -> LoadedShortcutConfig {
        let path = shortcut_config_path();
        let mut status = format!("Shortcuts: {}", path.display());

        if !path.exists() {
            match write_default_config(&path) {
                Ok(()) => {}
                Err(error) => {
                    status = format!("Using default shortcuts; could not write config: {error}");
                    return LoadedShortcutConfig {
                        config: Self::defaults(),
                        status,
                    };
                }
            }
        }

        match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| Self::from_str(&contents))
        {
            Ok(config) => LoadedShortcutConfig { config, status },
            Err(error) => LoadedShortcutConfig {
                config: Self::defaults(),
                status: format!("Using default shortcuts; config error: {error}"),
            },
        }
    }

    pub fn defaults() -> Self {
        Self::from_str(DEFAULT_SHORTCUTS).expect("default shortcut config should be valid")
    }

    pub fn from_str(contents: &str) -> Result<Self, String> {
        let mut partials = HashMap::<String, PartialShortcutCommand>::new();

        for (line_index, raw_line) in contents.lines().enumerate() {
            let line_number = line_index + 1;
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {line_number}: expected `key = \"value\"`"))?;
            let key = key.trim();
            let value = parse_value(value.trim())
                .ok_or_else(|| format!("line {line_number}: expected a quoted value"))?;

            let (shortcut, is_search_target) = key
                .strip_suffix(".search")
                .map(|shortcut| (shortcut, true))
                .unwrap_or((key, false));
            validate_shortcut(shortcut).map_err(|error| format!("line {line_number}: {error}"))?;

            let entry = partials.entry(shortcut.to_string()).or_default();
            if is_search_target {
                entry.search_target = Some(value);
            } else {
                entry.target = Some(value);
            }
        }

        let mut commands = HashMap::new();
        for (shortcut, partial) in partials {
            let target = partial
                .target
                .ok_or_else(|| format!("{shortcut} is missing a base URL"))?;
            commands.insert(
                shortcut,
                ShortcutCommand {
                    target,
                    search_target: partial.search_target,
                },
            );
        }

        Ok(Self { commands })
    }

    pub fn resolve(&self, command: &str) -> Option<String> {
        let mut parts = command.trim().splitn(2, char::is_whitespace);
        let shortcut = parts.next().unwrap_or_default();
        let query = parts.next().unwrap_or_default().trim();
        let command = self.commands.get(shortcut)?;

        if query.is_empty() {
            return Some(command.target.clone());
        }

        let search_target = command.search_target.as_ref()?;
        Some(apply_query(search_target, query))
    }

    pub fn shortcut_prefix<'a>(&self, query: &'a str) -> Option<&'a str> {
        let (shortcut, _) = query.split_once(char::is_whitespace)?;

        if self.commands.contains_key(shortcut) {
            Some(shortcut)
        } else {
            None
        }
    }

    pub fn target_for_shortcut(&self, shortcut: &str) -> Option<&str> {
        self.commands
            .get(shortcut)
            .map(|command| command.target.as_str())
    }
}

fn shortcut_config_path() -> PathBuf {
    let local_path = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("shortcuts.conf");

    if local_path.exists() {
        return local_path;
    }

    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Spark Run").join("shortcuts.conf"))
        .unwrap_or(local_path)
}

fn write_default_config(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(path, DEFAULT_SHORTCUTS).map_err(|error| error.to_string())
}

fn parse_value(value: &str) -> Option<String> {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return Some(value[1..value.len() - 1].to_string());
    }

    None
}

fn validate_shortcut(shortcut: &str) -> Result<(), String> {
    if !shortcut.starts_with('!') || shortcut.len() == 1 {
        return Err("shortcut must start with `!`".to_string());
    }

    if shortcut.chars().any(char::is_whitespace) {
        return Err("shortcut cannot contain whitespace".to_string());
    }

    Ok(())
}

fn apply_query(template: &str, query: &str) -> String {
    let query = url_encode(query);

    if template.contains("{query}") {
        template.replace("{query}", &query)
    } else {
        format!("{template}{query}")
    }
}

fn url_encode(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

const DEFAULT_SHORTCUTS: &str = include_str!("../shortcuts.conf");

#[cfg(test)]
mod tests {
    use super::ShortcutConfig;

    #[test]
    fn resolves_bare_shortcuts() {
        let config = ShortcutConfig::defaults();

        assert_eq!(config.resolve("!g").as_deref(), Some("https://google.com/"));
        assert_eq!(
            config.resolve("!gpt").as_deref(),
            Some("https://chatgpt.com/")
        );
    }

    #[test]
    fn detects_configured_shortcut_prefixes() {
        let config = ShortcutConfig::defaults();

        assert_eq!(config.shortcut_prefix("!git rust"), Some("!git"));
        assert_eq!(config.shortcut_prefix("!unknown rust"), None);
        assert_eq!(config.shortcut_prefix("!git"), None);
    }

    #[test]
    fn resolves_search_shortcuts() {
        let config = ShortcutConfig::defaults();

        assert_eq!(
            config.resolve("!git owner/repo issue").as_deref(),
            Some("https://github.com/search?q=owner%2Frepo+issue")
        );
        assert_eq!(
            config.resolve("!gpt explain rust lifetimes").as_deref(),
            Some("https://chatgpt.com/?q=explain+rust+lifetimes")
        );
    }

    #[test]
    fn resolves_custom_shortcuts() {
        let config = ShortcutConfig::from_str(
            r#"
            !x = "https://example.com/"
            !x.search = "https://example.com/search?q={query}"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.resolve("!x rust launcher").as_deref(),
            Some("https://example.com/search?q=rust+launcher")
        );
    }

    #[test]
    fn non_searchable_shortcut_with_query_is_not_resolved() {
        let config = ShortcutConfig::defaults();

        assert_eq!(config.resolve("!claude explain rust"), None);
    }
}
