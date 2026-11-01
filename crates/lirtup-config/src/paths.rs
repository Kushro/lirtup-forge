//! Locations of the configuration files (same folder as 1.x).

use std::path::PathBuf;

/// Folder name under `$XDG_CONFIG_HOME`, kept from 1.x.
pub const APP_DIR: &str = "linux-rt-upscaler";

/// `~/.config/linux-rt-upscaler` (or `$XDG_CONFIG_HOME/linux-rt-upscaler`).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_DIR)
}

/// `config.yaml` in [`config_dir`].
pub fn config_path() -> PathBuf {
    config_dir().join("config.yaml")
}

/// `gui-config.yaml` in [`config_dir`].
pub fn gui_config_path() -> PathBuf {
    config_dir().join("gui-config.yaml")
}

/// `modes/` in [`config_dir`].
pub fn modes_dir() -> PathBuf {
    config_dir().join("modes")
}

/// `effects/` in [`config_dir`]: user MagpieFX effects.
pub fn user_effects_dir() -> PathBuf {
    config_dir().join("effects")
}

/// Cache folder of compiled MagpieFX SPIR-V (same folder as 1.x:
/// `$XDG_CACHE_HOME/linux-rt-upscaler/magpiefx`).
pub fn magpiefx_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join(APP_DIR)
        .join("magpiefx")
}

/// Where the process writes its own `lsfg-vk.toml`.
pub fn lsfg_toml_path() -> PathBuf {
    config_dir().join("lsfg-vk.toml")
}

/// `$XDG_PICTURES_DIR/Screenshots`, the 1.x default.
pub fn default_screenshot_dir() -> PathBuf {
    dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Screenshots")
}

/// Expand a leading `~` to the home folder.
pub fn expand_tilde(path: &std::path::Path) -> PathBuf {
    match (path.strip_prefix("~"), dirs::home_dir()) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    }
}
