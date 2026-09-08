//! Persisted app config: per-context theme assignment and a couple of
//! preferences. Lives at `$XDG_CONFIG_HOME/luxury-tui/config.yaml` (or the
//! platform equivalent via the `directories` crate).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::Palette;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// context name -> palette name
    #[serde(default)]
    pub context_palettes: HashMap<String, String>,
    /// Explicit kubeconfig path override (defaults to KUBECONFIG / ~/.kube/config).
    #[serde(default)]
    pub kubeconfig_path: Option<PathBuf>,
    #[serde(default)]
    pub log_line_buffer: Option<usize>,
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "luxury-tui", "luxury-tui")
            .map(|dirs| dirs.config_dir().join("config.yaml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_yaml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    pub fn palette_for(&self, context: &str) -> Palette {
        self.context_palettes
            .get(context)
            .and_then(|name| Palette::from_name(name))
            .unwrap_or_else(|| Palette::for_context(context))
    }

    pub fn set_palette(&mut self, context: &str, palette: Palette) {
        self.context_palettes
            .insert(context.to_string(), palette.name().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_palette_choice_overrides_the_deterministic_default() {
        let mut config = Config::default();
        let default = config.palette_for("staging");
        let other = Palette::ALL.into_iter().find(|p| *p != default).unwrap();

        config.set_palette("staging", other);

        assert_eq!(config.palette_for("staging"), other);
        assert_eq!(
            config.palette_for("some-other-context"),
            Palette::for_context("some-other-context")
        );
    }

    #[test]
    fn round_trips_through_yaml() {
        let mut config = Config::default();
        config.set_palette("prod", Palette::Volcano);
        let yaml = serde_yaml::to_string(&config).unwrap();
        let reloaded: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(reloaded.palette_for("prod"), Palette::Volcano);
    }
}
