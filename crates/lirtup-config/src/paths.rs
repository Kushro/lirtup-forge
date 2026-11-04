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

/// Environment variable that moves the built-in MagpieFX effects
/// (`lirtup_magpiefx::catalog::EFFECTS_DIR_ENV`); the seed sits next to them.
const EFFECTS_DIR_ENV: &str = "LIRTUP_EFFECTS_DIR";

/// Where an installed build keeps the seed (next to
/// `/usr/share/lirtup/effects`).
const SYSTEM_MAGPIEFX_SEED_DIR: &str = "/usr/share/lirtup/magpiefx-seed";

/// Read-only folder of precompiled MagpieFX SPIR-V shipped with the
/// package (`magpiefx-seed`), looked up after [`magpiefx_cache_dir`]:
/// `$LIRTUP_MAGPIEFX_SEED_DIR`, else `magpiefx-seed` next to
/// `$LIRTUP_EFFECTS_DIR`, else `/usr/share/lirtup/magpiefx-seed`. The folder
/// may not exist (a missing seed never hits). `None` is kept for callers
/// that want no seed; this function always has one.
pub fn magpiefx_seed_dir() -> Option<PathBuf> {
    Some(seed_dir_from(
        std::env::var_os(MAGPIEFX_SEED_DIR_ENV),
        std::env::var_os(EFFECTS_DIR_ENV),
    ))
}

/// [`magpiefx_seed_dir`] from the two variables (empty = unset). Mirrors
/// `lirtup_magpiefx::catalog::builtin_dir` without its debug-build source
/// tree fallback: a source tree has no seed.
fn seed_dir_from(seed: Option<std::ffi::OsString>, effects: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(seed) = seed.filter(|v| !v.is_empty()) {
        return PathBuf::from(seed);
    }
    if let Some(effects) = effects.filter(|v| !v.is_empty()) {
        let effects = PathBuf::from(effects);
        return match effects.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join("magpiefx-seed"),
            _ => effects.join("..").join("magpiefx-seed"),
        };
    }
    PathBuf::from(SYSTEM_MAGPIEFX_SEED_DIR)
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::*;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    #[test]
    fn seed_dir_follows_the_variables() {
        // its own variable wins
        assert_eq!(
            seed_dir_from(os("/opt/seed"), os("/x/share/lirtup/effects")),
            Path::new("/opt/seed")
        );
        // else next to the effects folder (the AppImage, a tarball)
        assert_eq!(
            seed_dir_from(None, os("/tmp/.mount_abc/usr/share/lirtup/effects")),
            Path::new("/tmp/.mount_abc/usr/share/lirtup/magpiefx-seed")
        );
        assert_eq!(
            seed_dir_from(os(""), os("/usr/local/share/lirtup/effects/")),
            Path::new("/usr/local/share/lirtup/magpiefx-seed")
        );
        // a relative effects folder without a parent
        assert_eq!(
            seed_dir_from(None, os("effects")),
            Path::new("effects/../magpiefx-seed")
        );
        // else the system folder
        assert_eq!(
            seed_dir_from(None, None),
            Path::new("/usr/share/lirtup/magpiefx-seed")
        );
        assert_eq!(
            seed_dir_from(os(""), os("")),
            Path::new("/usr/share/lirtup/magpiefx-seed")
        );
    }
}
