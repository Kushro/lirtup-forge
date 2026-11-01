//! Frame generation for LirtUp Forge (`framegen.py`, the lsfg half of
//! `native_framegen.py`).
//!
//! Two engines, chosen by `frame_gen_engine`:
//!
//! - **Layer** ([`layer`]): the lsfg-vk Vulkan layer. [`layer::prepare`]
//!   writes this process's `lsfg-vk.toml` and sets the environment; it must
//!   run before `lirtup_gpu::Instance::new`. Changing the multiplier means
//!   `exec` of `/proc/self/exe` ([`layer::restart_self`]).
//! - **Native** ([`native`]): lsfg built in, through `lsfg-sys`, with images
//!   and semaphores shared by fd. This crate wraps the C API safely; the
//!   presentation thread, cycles and pacing are `lirtup-pipeline::framegen`
//!   and `::pacing` (front F, round 2).
//!
//! [`dll`] finds `lsfg-vk.dll` / `Lossless.dll` in Steam; [`AutoMultiplier`]
//! picks the multiplier from the game's frame rate; [`FrameGenState`] is what
//! the GUI shows.
//!
//! This crate does not depend on `lirtup-gpu`: fds and `VkFormat` numbers
//! cross the boundary as plain values.

// Round 0 stubs: the bodies are `todo!()`. Front C removes this.
#![allow(unused_variables)]

pub mod dll;
pub mod error;
pub mod layer;
pub mod multiplier;
pub mod native;
pub mod status;

pub use error::{FgError, Result};
pub use multiplier::AutoMultiplier;
pub use status::FrameGenState;
