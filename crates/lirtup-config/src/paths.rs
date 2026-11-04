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

/// Environment variable naming the precompiled MagpieFX seed folder (the
/// AppImage launcher sets it).
pub const MAGPIEFX_SEED_DIR_ENV: &str = "LIRTUP_MAGPIEFX_SEED_DIR";

/// Read-only folder of precompiled MagpieFX SPIR-V shipped with the
/// package (`magpiefx-seed`), looked up after [`magpiefx_cache_dir`]:
/// `$LIRTUP_MAGPIEFX_SEED_DIR`, else `magpiefx-seed` next to the built-in
/// effects folder (`/usr/share/lirtup/magpiefx-seed`). `None` = no seed.
// TODO(magpiefx): the fallback next to the effects folder (stub: the
// environment variable only).
pub fn magpiefx_seed_dir() -> Option<PathBuf> {
    std::env::var_os(MAGPIEFX_SEED_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
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
