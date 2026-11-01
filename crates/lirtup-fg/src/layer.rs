//! The lsfg-vk layer: configuration before the Vulkan instance exists
//! (`framegen.prepare`).
//!
//! The layer picks its profile once, when it loads, so everything here runs
//! before the first Vulkan call of the process. It is configured through a
//! file of our own (`LSFGVK_CONFIG`), so the user's
//! `~/.config/lsfg-vk/conf.toml` is never touched. When frame generation is
//! off (or the native engine runs), `DISABLE_LSFGVK=1` keeps the user's own
//! lsfg-vk profiles from matching this process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use lirtup_config::{Config, FrameGenEngine, FrameGenMultiplier};

use crate::dll::{DllKind, find_dll};
use crate::multiplier::AUTO_START_MULTIPLIER;
use crate::{FrameGenState, Result};

/// File name of the layer manifest.
pub const LAYER_MANIFEST: &str = "VkLayer_LSFGVK_frame_generation.json";

/// Variables that mean the user drives lsfg-vk by hand.
const USER_ENV: [&str; 3] = ["LSFGVK_CONFIG", "LSFGVK_ENV", "DISABLE_LSFGVK"];
/// Marks the variables above as ours (a restarted process inherits them).
const MANAGED_MARK: &str = "LIRTUP_FORGE_FRAMEGEN";

/// What [`prepare`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerSetup {
    /// Resulting state.
    pub state: FrameGenState,
    /// The multiplier the layer runs at (auto starts at ×2 with the layer).
    pub multiplier: u8,
    /// The `lsfg-vk.toml` written, if any.
    pub toml: Option<PathBuf>,
}

/// The installed layer manifest (`implicit_layer.d/VkLayer_LSFGVK_frame_generation.json`),
/// if any.
pub fn find_layer() -> Option<PathBuf> {
    layer_dirs()
        .into_iter()
        .map(|d| d.join(LAYER_MANIFEST))
        .find(|m| m.is_file())
}

/// `vulkan/implicit_layer.d` under `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`,
/// then `/etc/vulkan/implicit_layer.d` (where the loader looks).
fn layer_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![crate::dll::data_home()];
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    let mut out: Vec<PathBuf> = dirs
        .into_iter()
        .map(|d| d.join("vulkan/implicit_layer.d"))
        .collect();
    out.push(PathBuf::from("/etc/vulkan/implicit_layer.d"));
    out
}

/// Whether the user set `LSFGVK_*` variables themselves (the GUI then shows
/// «Definida por variables LSFGVK_*» and leaves the section disabled).
pub fn external_env() -> bool {
    std::env::var_os(MANAGED_MARK).is_none() && USER_ENV.iter().any(|v| std::env::var_os(v).is_some())
}

/// Configure the layer for this process from the frame generation fields of
/// `config`: write `lsfg-vk.toml`, set the environment (or
/// `DISABLE_LSFGVK=1` when off). Must run before
/// `lirtup_gpu::Instance::new`; does nothing useful for the native engine
/// except reporting its state.
///
/// It changes the process environment, so it must run while no other thread
/// reads or writes it (at startup, before the runtime and the pipeline
/// threads exist).
pub fn prepare(config: &Config) -> Result<LayerSetup> {
    let engine = config.frame_gen_engine;
    let auto = config.frame_gen_multiplier == FrameGenMultiplier::Auto;
    let multiplier = match config.frame_gen_multiplier {
        FrameGenMultiplier::Auto => {
            if engine == FrameGenEngine::Layer {
                tracing::warn!(
                    "the automatic frame generation multiplier needs the built-in engine; \
                     lsfg-vk runs at x{AUTO_START_MULTIPLIER}"
                );
            }
            AUTO_START_MULTIPLIER
        }
        FrameGenMultiplier::Fixed(n) => n.clamp(1, 5),
    };

    // Variables we set in a previous run of this process (restart_self) are
    // not the user's.
    if std::env::var_os(MANAGED_MARK).is_some() {
        remove_env(MANAGED_MARK);
        for var in USER_ENV {
            remove_env(var);
        }
    }
    if USER_ENV.iter().any(|v| std::env::var_os(v).is_some()) {
        let state = if std::env::var_os("DISABLE_LSFGVK").is_some_and(|v| v == "1") {
            FrameGenState::Off
        } else {
            FrameGenState::External
        };
        tracing::info!("lsfg-vk is configured by the environment, leaving it alone");
        return Ok(LayerSetup {
            state,
            multiplier,
            toml: None,
        });
    }

    let hint = config.frame_gen_dll.as_deref();
    let (state, toml) = if multiplier <= 1 {
        (FrameGenState::Off, None)
    } else if engine == FrameGenEngine::Native {
        // The layer must not generate frames on top of ours.
        match find_dll(DllKind::Lossless, hint) {
            None => {
                tracing::warn!(
                    "native frame generation needs Lossless.dll (Lossless Scaling on Steam), which was not found"
                );
                (FrameGenState::NoDll, None)
            }
            Some(dll) => {
                if auto {
                    tracing::info!(
                        "frame generation with the native engine, automatic ({})",
                        dll.display()
                    );
                } else {
                    tracing::info!(
                        "frame generation x{multiplier} with the native engine ({})",
                        dll.display()
                    );
                }
                (FrameGenState::Active, None)
            }
        }
    } else if find_layer().is_none() {
        tracing::warn!("frame generation needs lsfg-vk, which is not installed");
        (FrameGenState::NoLayer, None)
    } else if let Some(dll) = find_dll(DllKind::LsfgVk, hint) {
        let path = lirtup_config::paths::lsfg_toml_path();
        write_toml(
            &path,
            &dll,
            multiplier,
            config.frame_gen_flow_scale,
            config.frame_gen_performance,
        )?;
        set_env("LSFGVK_CONFIG", path.as_os_str());
        set_env(MANAGED_MARK, "1".as_ref());
        tracing::info!("frame generation x{multiplier} through lsfg-vk");
        return Ok(LayerSetup {
            state: FrameGenState::Active,
            multiplier,
            toml: Some(path),
        });
    } else {
        tracing::warn!(
            "frame generation needs lsfg-vk.dll (the 'lsfg-vk' branch of Lossless Scaling on Steam), \
             which was not found"
        );
        (FrameGenState::NoDll, None)
    };
    set_env("DISABLE_LSFGVK", "1".as_ref());
    set_env(MANAGED_MARK, "1".as_ref());
    Ok(LayerSetup {
        state,
        multiplier,
        toml,
    })
}

/// The contents of our `lsfg-vk.toml` (lsfg-vk 2.x format, one profile for
/// this executable).
pub fn toml_contents(
    dll: &Path,
    process: &str,
    multiplier: u8,
    flow_scale: f32,
    performance: bool,
) -> String {
    format!(
        "# Written by LirtUp Forge at startup; edits are overwritten.\n\
         version = 2\n\
         \n\
         [global]\n\
         allow_fp16 = true\n\
         dll = {dll}\n\
         \n\
         [[profile]]\n\
         name = \"LirtUp Forge\"\n\
         active_in = {process}\n\
         multiplier = {multiplier}\n\
         flow_scale = {flow_scale:.2}\n\
         performance_mode = {performance}\n\
         pacing_mode = \"vsync\"\n",
        dll = toml_str(&dll.to_string_lossy()),
        process = toml_str(process),
    )
}

fn write_toml(path: &Path, dll: &Path, multiplier: u8, flow_scale: f32, performance: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        toml_contents(dll, &process_name(), multiplier, flow_scale, performance),
    )?;
    Ok(())
}

/// A TOML basic string.
fn toml_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The executable name lsfg-vk matches profiles against (`/proc/self/exe`).
fn process_name() -> String {
    std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "lirtup".into())
}

#[allow(unsafe_code)]
fn set_env(key: &str, value: &std::ffi::OsStr) {
    // SAFETY: prepare() runs at startup, before other threads exist (its
    // documented contract), so nothing reads the environment concurrently.
    unsafe { std::env::set_var(key, value) };
}

#[allow(unsafe_code)]
fn remove_env(key: &str) {
    // SAFETY: as in set_env.
    unsafe { std::env::remove_var(key) };
}

/// Replace this process with a fresh copy of itself (`exec` of
/// `/proc/self/exe` with the same arguments), so the layer reads the new
/// multiplier. Only returns on error.
pub fn restart_self() -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let mut args = std::env::args_os();
    let argv0 = args.next().unwrap_or_else(|| OsString::from("lirtup"));
    tracing::info!("restarting to apply the frame generation settings");
    std::process::Command::new("/proc/self/exe")
        .arg0(argv0)
        .args(args)
        .exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_matches_1x() {
        let text = toml_contents(
            Path::new("/games/Lossless Scaling/lsfg-vk.dll"),
            "lirtup",
            3,
            0.5,
            true,
        );
        assert_eq!(
            text,
            "# Written by LirtUp Forge at startup; edits are overwritten.\n\
             version = 2\n\
             \n\
             [global]\n\
             allow_fp16 = true\n\
             dll = \"/games/Lossless Scaling/lsfg-vk.dll\"\n\
             \n\
             [[profile]]\n\
             name = \"LirtUp Forge\"\n\
             active_in = \"lirtup\"\n\
             multiplier = 3\n\
             flow_scale = 0.50\n\
             performance_mode = true\n\
             pacing_mode = \"vsync\"\n"
        );
    }

    #[test]
    fn toml_strings_are_escaped() {
        assert_eq!(toml_str(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }

    #[test]
    fn layer_dirs_end_in_etc() {
        let dirs = layer_dirs();
        assert!(dirs.iter().all(|d| d.ends_with("implicit_layer.d")));
        assert_eq!(dirs.last().unwrap(), Path::new("/etc/vulkan/implicit_layer.d"));
    }

    /// One test for everything that touches the environment, so tests running
    /// in parallel threads never race on it.
    #[test]
    fn prepare_states() {
        for var in USER_ENV.iter().chain([&MANAGED_MARK]) {
            remove_env(var);
        }
        // Off: the layer is disabled for this process.
        let setup = prepare(&Config::default()).unwrap();
        assert_eq!(setup.state, FrameGenState::Off);
        assert_eq!(setup.multiplier, 1);
        assert_eq!(std::env::var("DISABLE_LSFGVK").as_deref(), Ok("1"));
        // Our own variables are not the user's (as after restart_self).
        assert!(!external_env());

        // Native engine, automatic, with the DLL given by its file.
        let tmp = tempfile::tempdir().unwrap();
        let native = Config {
            frame_gen_engine: FrameGenEngine::Native,
            frame_gen_multiplier: FrameGenMultiplier::Auto,
            frame_gen_dll: Some(tmp.path().join("Lossless.dll")),
            ..Config::default()
        };
        let dll = tmp.path().join("Lossless.dll");
        std::fs::write(&dll, b"MZ").unwrap();
        let setup = prepare(&native).unwrap();
        assert_eq!(setup.state, FrameGenState::Active);
        assert_eq!(setup.multiplier, AUTO_START_MULTIPLIER);
        assert!(setup.toml.is_none());
        assert_eq!(std::env::var("DISABLE_LSFGVK").as_deref(), Ok("1"));

        // The user's own variables win.
        for var in USER_ENV.iter().chain([&MANAGED_MARK]) {
            remove_env(var);
        }
        set_env("LSFGVK_ENV", "1".as_ref());
        assert!(external_env());
        let on = Config {
            frame_gen_multiplier: FrameGenMultiplier::Fixed(3),
            ..Config::default()
        };
        assert_eq!(prepare(&on).unwrap().state, FrameGenState::External);
        remove_env("LSFGVK_ENV");
        set_env("DISABLE_LSFGVK", "1".as_ref());
        assert_eq!(prepare(&on).unwrap().state, FrameGenState::Off);
        remove_env("DISABLE_LSFGVK");
    }
}
