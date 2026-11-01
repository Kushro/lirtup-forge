//! Memory and semaphores shared by fd with lsfg (`vk_external.cpp`).

use std::os::fd::OwnedFd;

use crate::{Result, Texture};

/// A texture in a dedicated, exportable allocation.
#[derive(Clone)]
pub struct ExternalTexture {
    _private: std::sync::Arc<()>,
}

impl ExternalTexture {
    /// The texture, usable like any other.
    pub fn texture(&self) -> &Texture {
        todo!("front G")
    }

    /// Export a new opaque fd of its memory (`vkGetMemoryFdKHR`). The caller
    /// owns the fd; lsfg takes ownership when it imports it.
    pub fn export_fd(&self) -> Result<OwnedFd> {
        todo!("front G")
    }
}

/// An exportable binary semaphore.
#[derive(Clone)]
pub struct ExternalSemaphore {
    _private: std::sync::Arc<()>,
}

impl ExternalSemaphore {
    /// Export a new opaque fd (`vkGetSemaphoreFdKHR`).
    pub fn export_fd(&self) -> Result<OwnedFd> {
        todo!("front G")
    }

    /// Raw handle.
    pub fn raw(&self) -> ash::vk::Semaphore {
        todo!("front G")
    }
}
