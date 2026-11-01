//! `gui-config.yaml`: settings of `lirtup-gui` only (the scaler never reads
//! it). Keeps the 1.x keys and adds the ones of the new design (6.6).

use serde::{Deserialize, Serialize};

use crate::{ConfigError, Mapping};

/// GUI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Follow `LANG` / `LC_MESSAGES` (default).
    #[default]
    System,
    /// Español.
    Es,
    /// English.
    En,
}

/// Light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the desktop (default).
    #[default]
    System,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// Aspect of the window tiles in Inicio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TileAspect {
    /// From the windows themselves (default).
    #[default]
    #[serde(rename = "auto")]
    Auto,
    /// 4:3.
    #[serde(rename = "4:3")]
    FourThree,
    /// 16:9.
    #[serde(rename = "16:9")]
    SixteenNine,
}

/// The GUI settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiConfig {
    // New in 2.0 (Ajustes ▸ General)
    /// Language.
    pub language: Language,
    /// Theme.
    pub theme: Theme,
    /// Seed color of the Material 3 scheme (`#2F5F9E` by default).
    pub seed_color: String,
    /// Close to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Start hidden in the tray.
    pub start_hidden: bool,
    /// Keep `lirtup serve` running (daemon) after the GUI quits.
    pub keep_running_after_exit: bool,
    /// Milliseconds between window list refreshes (Inicio).
    pub auto_refresh_ms: u32,
    /// Milliseconds between thumbnail refreshes.
    pub tile_preview_interval_ms: u32,
    /// Tile aspect.
    pub tile_aspect: TileAspect,

    // Kept from 1.x (Ajustes ▸ Estilo)
    /// Zoom percent, 50 to 250.
    pub zoom: u32,
    /// Font scale percent, 80 to 150.
    pub font_scale: u32,
    /// Font family (empty = the bundled Google Sans Flex).
    pub font_family: String,
    /// Columns of the window grid, 1 to 8 (0 = automatic).
    pub tile_columns: u32,

    /// 1.x keys with no use in 2.0 (`palette`, `palette_preset`,
    /// `profiles_width`, `settings_width`, `tile_aspect_ratio`, ...) and
    /// unknown keys, kept for saving.
    #[serde(flatten)]
    pub extra: Mapping,
}

impl Default for GuiConfig {
    fn default() -> Self {
        GuiConfig {
            language: Language::System,
            theme: Theme::System,
            seed_color: "#2F5F9E".into(),
            close_to_tray: true,
            start_hidden: false,
            keep_running_after_exit: false,
            auto_refresh_ms: 1000,
            tile_preview_interval_ms: 500,
            tile_aspect: TileAspect::Auto,
            zoom: 100,
            font_scale: 100,
            font_family: String::new(),
            tile_columns: 0,
            extra: Mapping::new(),
        }
    }
}

impl GuiConfig {
    /// Read [`crate::paths::gui_config_path`] (or `path`). Never fails on bad
    /// values: they fall back to the defaults and numbers are clamped to the
    /// 1.x bounds (`_OVERRIDE_BOUNDS`). Only I/O on an existing file errors.
    pub fn load(path: Option<&std::path::Path>) -> Result<GuiConfig, ConfigError> {
        let default_path = crate::paths::gui_config_path();
        let path = path.unwrap_or(&default_path);
        let map = match crate::util::read_mapping(path) {
            Ok(Some(map)) => map,
            Ok(None) => return Ok(GuiConfig::default()),
            Err(e @ ConfigError::Io { .. }) => return Err(e),
            Err(e) => {
                tracing::warn!("{e}; using the default GUI settings");
                return Ok(GuiConfig::default());
            }
        };
        let (mut config, warnings) = crate::util::tolerant::<GuiConfig>(&map, "gui.");
        for w in warnings {
            tracing::warn!("{}: {}", w.key, w.message);
        }
        config.clamp();
        Ok(config)
    }

    /// Write (temporary file + rename), keeping unknown keys.
    pub fn save(&self, path: Option<&std::path::Path>) -> Result<(), ConfigError> {
        let default_path = crate::paths::gui_config_path();
        let path = path.unwrap_or(&default_path);
        let map = crate::util::to_mapping(self);
        crate::util::write_atomic(path, &crate::util::to_yaml(&map, path)?)
    }

    /// Clamp the numbers to the 1.x bounds (`_OVERRIDE_BOUNDS`);
    /// `tile_columns` 0 (automatic) is allowed.
    pub fn clamp(&mut self) {
        let clamp = |v: &mut u32, lo: u32, hi: u32, name: &str| {
            let c = (*v).clamp(lo, hi);
            if c != *v {
                tracing::warn!("gui.{name}: {} is out of {lo} to {hi}, using {c}", *v);
                *v = c;
            }
        };
        clamp(&mut self.zoom, 50, 250, "zoom");
        clamp(&mut self.font_scale, 80, 150, "font_scale");
        if self.tile_columns != 0 {
            clamp(&mut self.tile_columns, 1, 8, "tile_columns");
        }
        clamp(&mut self.auto_refresh_ms, 250, 30_000, "auto_refresh_ms");
        clamp(
            &mut self.tile_preview_interval_ms,
            16,
            1000,
            "tile_preview_interval_ms",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let c = GuiConfig::load(Some(&dir.path().join("gui-config.yaml"))).unwrap();
        assert_eq!(c, GuiConfig::default());
    }

    #[test]
    fn reads_1x_file_and_keeps_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui-config.yaml");
        std::fs::write(
            &path,
            "palette:\n  text: '#cccccc'\npalette_preset: Auto\nzoom: 400\nfont_scale: big\n\
             tile_columns: 0\ntheme: dark\nauto_refresh_ms: 10\n",
        )
        .unwrap();
        let c = GuiConfig::load(Some(&path)).unwrap();
        assert_eq!(c.zoom, 250);
        assert_eq!(c.font_scale, 100);
        assert_eq!(c.tile_columns, 0);
        assert_eq!(c.theme, Theme::Dark);
        assert_eq!(c.auto_refresh_ms, 250);
        assert!(c.extra.contains_key("palette"));
        assert!(c.extra.contains_key("palette_preset"));

        c.save(Some(&path)).unwrap();
        let back = GuiConfig::load(Some(&path)).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn broken_yaml_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui-config.yaml");
        std::fs::write(&path, "zoom: [\n").unwrap();
        assert_eq!(GuiConfig::load(Some(&path)).unwrap(), GuiConfig::default());
    }
}
