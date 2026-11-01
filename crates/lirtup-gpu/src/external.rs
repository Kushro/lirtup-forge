//! Memory and semaphores shared by fd with lsfg (`vk_external.cpp`).

use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;

use ash::vk;

use crate::error::vk_err;
use crate::{Device, GpuError, Result, Texture};

/// A texture in a dedicated, exportable allocation.
#[derive(Clone)]
pub struct ExternalTexture {
    texture: Texture,
}

impl ExternalTexture {
    pub(crate) fn new(texture: Texture) -> ExternalTexture {
        ExternalTexture { texture }
    }

    /// The texture, usable like any other.
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Export a new opaque fd of its memory (`vkGetMemoryFdKHR`). The caller
    /// owns the fd; lsfg takes ownership when it imports it.
    pub fn export_fd(&self) -> Result<OwnedFd> {
        let device = self.texture.device();
        let memory = self
            .texture
            .dedicated_memory()
            .ok_or_else(|| GpuError::Invalid("texture has no exportable memory".into()))?;
        let info = vk::MemoryGetFdInfoKHR::default()
            .memory(memory)
            .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        // SAFETY: the memory was allocated exportable as an opaque fd.
        let fd =
            unsafe { device.memory_fd_fn()?.get_memory_fd(&info) }.map_err(vk_err("vkGetMemoryFdKHR"))?;
        // SAFETY: vkGetMemoryFdKHR returns a new fd that the caller owns.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

struct SemaphoreInner {
    device: Arc<Device>,
    raw: vk::Semaphore,
}

/// An exportable binary semaphore.
#[derive(Clone)]
pub struct ExternalSemaphore {
    inner: Arc<SemaphoreInner>,
}

impl ExternalSemaphore {
    pub(crate) fn create(device: &Arc<Device>) -> Result<ExternalSemaphore> {
        let mut export = vk::ExportSemaphoreCreateInfo::default()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
        let info = vk::SemaphoreCreateInfo::default().push_next(&mut export);
        // SAFETY: the device has VK_KHR_external_semaphore_fd (checked by the
        // caller); the chain points to a local alive here.
        let raw =
            unsafe { device.raw().create_semaphore(&info, None) }.map_err(vk_err("vkCreateSemaphore"))?;
        Ok(ExternalSemaphore {
            inner: Arc::new(SemaphoreInner {
                device: device.clone(),
                raw,
            }),
        })
    }

    /// Export a new opaque fd (`vkGetSemaphoreFdKHR`).
    pub fn export_fd(&self) -> Result<OwnedFd> {
        let info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(self.inner.raw)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
        // SAFETY: the semaphore was created exportable as an opaque fd.
        let fd = unsafe { self.inner.device.semaphore_fd_fn()?.get_semaphore_fd(&info) }
            .map_err(vk_err("vkGetSemaphoreFdKHR"))?;
        // SAFETY: vkGetSemaphoreFdKHR returns a new fd that the caller owns.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// Raw handle.
    pub fn raw(&self) -> vk::Semaphore {
        self.inner.raw
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.inner.device
    }
}

impl Drop for SemaphoreInner {
    fn drop(&mut self) {
        // As 1.x `destroy_semaphore`: the queue may still wait on or signal
        // it (presents do not keep it alive), so idle the queue first.
        if let Err(e) = self.device.wait_idle() {
            tracing::warn!("waiting before destroying a semaphore: {e}");
        }
        // SAFETY: the queue is idle and this is the last handle.
        unsafe { self.device.raw().destroy_semaphore(self.raw, None) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync + Clone>() {}

    #[test]
    fn shared_objects_are_send_sync() {
        assert_send_sync::<ExternalTexture>();
        assert_send_sync::<ExternalSemaphore>();
    }
}
