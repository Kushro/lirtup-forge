//! The lsfg-vk layer: configuration before the Vulkan instance exists
//! (`framegen.prepare`).

use std::path::PathBuf;

use lirtup_config::Config;

use crate::{FrameGenState, Result};

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

/// The installed layer manifest (`implicit_layer.d/VkLayer_LS_frame_generation.json`),
/// if any.
pub fn find_layer() -> Option<PathBuf> {
    todo!("front C")
}

/// Whether the user set `LSFGVK_*` variables themselves (the GUI then shows
/// «Definida por variables LSFGVK_*» and leaves the section disabled).
pub fn external_env() -> bool {
    todo!("front C")
}

/// Configure the layer for this process from the frame generation fields of
/// `config`: write `lsfg-vk.toml`, set the environment (or
/// `DISABLE_LSFGVK=1` when off). Must run before
/// `lirtup_gpu::Instance::new`; does nothing useful for the native engine
/// except reporting its state.
pub fn prepare(config: &Config) -> Result<LayerSetup> {
    todo!("front C")
}

/// Replace this process with a fresh copy of itself (`exec` of
/// `/proc/self/exe` with the same arguments), so the layer reads the new
/// multiplier. Only returns on error.
pub fn restart_self() -> std::io::Error {
    todo!("front C")
}
