use std::{
    env, fs,
    path::{Path, PathBuf},
};

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentPreset {
    Mist,
    Blue,
    Teal,
    Violet,
}

#[derive(Clone, Copy, Debug)]
pub struct AccentStyle {
    pub selection_fill: egui::Color32,
    pub selection_stroke: egui::Color32,
    pub selection_bar: egui::Color32,
}

#[derive(Clone, Debug)]
pub struct ThemeConfig {
    accent: AccentPreset,
}

#[derive(Clone, Debug)]
pub struct LoadedThemeConfig {
    pub theme: ThemeConfig,
    pub status: String,
}

impl ThemeConfig {
    pub fn load_or_create() -> LoadedThemeConfig {
        let path = theme_config_path();
        let mut status = format!("Theme: {}", path.display());

        if !path.exists() {
            if let Err(error) = write_default_config(&path) {
                status = format!("Using default theme; could not write config: {error}");
                return LoadedThemeConfig {
                    theme: Self::defaults(),
                    status,
                };
            }
        }

        match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| Self::from_str(&contents))
        {
            Ok(theme) => LoadedThemeConfig { theme, status },
            Err(error) => LoadedThemeConfig {
                theme: Self::defaults(),
                status: format!("Using default theme; config error: {error}"),
            },
        }
    }

    pub fn defaults() -> Self {
        Self {
            accent: AccentPreset::Mist,
        }
    }

    pub fn from_str(contents: &str) -> Result<Self, String> {
        let mut accent = None;

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

            match key {
                "accent" => {
                    accent = Some(AccentPreset::parse(&value).ok_or_else(|| {
                        format!("line {line_number}: unknown accent preset `{value}`")
                    })?);
                }
                _ => return Err(format!("line {line_number}: unknown setting `{key}`")),
            }
        }

        Ok(Self {
            accent: accent.unwrap_or(AccentPreset::Mist),
        })
    }

    pub fn accent(&self) -> AccentPreset {
        self.accent
    }

    pub fn set_accent(&mut self, accent: AccentPreset) -> Result<(), String> {
        self.accent = accent;
        self.save()
    }

    pub fn accent_style(&self) -> AccentStyle {
        match self.accent {
            AccentPreset::Mist => AccentStyle {
                selection_fill: egui::Color32::from_hex("#bbbdc7").unwrap(),
                selection_stroke: egui::Color32::from_hex("#2e3036").unwrap(),
                selection_bar: egui::Color32::from_rgb(214, 221, 232),
            },
            AccentPreset::Blue => AccentStyle {
                selection_fill: egui::Color32::from_rgba_unmultiplied(88, 124, 178, 78),
                selection_stroke: egui::Color32::from_rgba_unmultiplied(200, 220, 248, 120),
                selection_bar: egui::Color32::from_rgb(112, 170, 255),
            },
            AccentPreset::Teal => AccentStyle {
                selection_fill: egui::Color32::from_rgba_unmultiplied(82, 144, 150, 78),
                selection_stroke: egui::Color32::from_rgba_unmultiplied(198, 238, 236, 120),
                selection_bar: egui::Color32::from_rgb(118, 214, 214),
            },
            AccentPreset::Violet => AccentStyle {
                selection_fill: egui::Color32::from_rgba_unmultiplied(120, 110, 168, 82),
                selection_stroke: egui::Color32::from_rgba_unmultiplied(224, 214, 248, 128),
                selection_bar: egui::Color32::from_rgb(184, 164, 255),
            },
        }
    }

    pub fn presets() -> &'static [AccentPreset] {
        &[
            AccentPreset::Mist,
            AccentPreset::Blue,
            AccentPreset::Teal,
            AccentPreset::Violet,
        ]
    }

    fn save(&self) -> Result<(), String> {
        let path = theme_config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, self.to_config_string()).map_err(|error| error.to_string())
    }

    fn to_config_string(&self) -> String {
        format!(
            "# Spark Run appearance settings.\naccent = \"{}\"\n",
            self.accent.as_key()
        )
    }
}

impl AccentPreset {
    pub fn label(self) -> &'static str {
        match self {
            AccentPreset::Mist => "Mist",
            AccentPreset::Blue => "Blue",
            AccentPreset::Teal => "Teal",
            AccentPreset::Violet => "Violet",
        }
    }

    pub fn swatch(self) -> egui::Color32 {
        match self {
            AccentPreset::Mist => egui::Color32::from_rgb(205, 211, 220),
            AccentPreset::Blue => egui::Color32::from_rgb(114, 156, 224),
            AccentPreset::Teal => egui::Color32::from_rgb(92, 184, 182),
            AccentPreset::Violet => egui::Color32::from_rgb(168, 148, 226),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mist" => Some(Self::Mist),
            "blue" => Some(Self::Blue),
            "teal" => Some(Self::Teal),
            "violet" => Some(Self::Violet),
            _ => None,
        }
    }

    fn as_key(self) -> &'static str {
        match self {
            AccentPreset::Mist => "mist",
            AccentPreset::Blue => "blue",
            AccentPreset::Teal => "teal",
            AccentPreset::Violet => "violet",
        }
    }
}

fn parse_value(value: &str) -> Option<String> {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return Some(value[1..value.len() - 1].to_string());
    }

    None
}

fn write_default_config(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(path, ThemeConfig::defaults().to_config_string()).map_err(|error| error.to_string())
}

fn theme_config_path() -> PathBuf {
    let local_path = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("theme.conf");

    if local_path.exists() {
        return local_path;
    }

    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Spark Run").join("theme.conf"))
        .unwrap_or(local_path)
}
