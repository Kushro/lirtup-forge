//! Command-line options of `lirtup run` (the 1.x `upscale`, `config/args.py`).
//!
//! The binary (`lirtup-cli`) flattens [`ConfigArgs`] into its `run`
//! subcommand; the options become one more layer for
//! [`crate::resolve::resolve`].

use std::path::PathBuf;

use clap::Args;

use crate::Mapping;

/// Options that pick the files and the profile.
#[derive(Debug, Clone, Default, Args)]
pub struct ConfigArgs {
    /// Config file (default: ~/.config/linux-rt-upscaler/config.yaml).
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Profile to apply (default: the first one whose match fits the window).
    #[arg(short = 'p', long = "profile", value_name = "NAME")]
    pub profile: Option<String>,

    /// Scaling mode to use instead of the profile's.
    #[arg(long = "mode", value_name = "ID")]
    pub mode: Option<String>,

    /// Option overrides. Front C replaces this with one flag per `Config`
    /// field, as `args.py` does (`--model 4x12`, `--no-tile-processing`, ...).
    #[command(flatten)]
    pub options: OptionArgs,
}

/// One flag per [`crate::Config`] field. Only the most used ones are declared
/// in round 0; front C adds the rest with the same names and help as 1.x.
#[derive(Debug, Clone, Default, Args)]
pub struct OptionArgs {
    /// SRCNN model (none, veryfast, faster, fast, 3x12, 4x12, 4x16, 4x24, 4x32, 8x32).
    #[arg(short = 'm', long = "model")]
    pub model: Option<String>,

    /// Output geometry (fit, stretch, cover, 150%, 1920x1080, ...).
    #[arg(short = 'g', long = "geometry")]
    pub output_geometry: Option<String>,

    /// Overlay mode (always-on-top, top-transparent, fullscreen, windowed).
    #[arg(long = "overlay-mode")]
    pub overlay_mode: Option<String>,

    /// Frame generation multiplier (1 to 5, or auto).
    #[arg(long = "frame-gen")]
    pub frame_gen_multiplier: Option<String>,

    /// Generic override, repeatable: `--set key=value` (YAML value).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub set: Vec<String>,
}

impl ConfigArgs {
    /// The options given, as a mapping layer with the YAML keys of
    /// [`crate::Config`]. Flags not given are absent (they must not override
    /// the file).
    pub fn overrides(&self) -> Result<Mapping, crate::ConfigError> {
        todo!("front C")
    }
}
