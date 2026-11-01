//! State of frame generation in this process (`FrameGenStatus` of
//! `framegen.py`). `lirtup-proto::FrameGenStatus` carries it to the GUI.

use serde::{Deserialize, Serialize};

/// What frame generation is doing, or why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameGenState {
    /// Off in the configuration.
    #[default]
    Off,
    /// Configured and running.
    Active,
    /// lsfg-vk layer not installed.
    NoLayer,
    /// The DLL was not found.
    NoDll,
    /// The user set `LSFGVK_*` themselves.
    External,
    /// The native engine is not available (no external fd support, or lsfg
    /// failed to initialize).
    NoNative,
}
