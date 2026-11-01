/**
 * @file vk_external.cpp
 * @brief Sharing images and semaphores with another Vulkan device by fd.
 *
 * The native frame generation engine (lsfg, see src/upscaler/lsfg) creates
 * its own VkDevice on the same GPU. Images and semaphores are shared with it
 * as opaque file descriptors (VK_KHR_external_memory_fd and
 * VK_KHR_external_semaphore_fd), so frames never leave video memory:
 *
 *   - `Device.create_texture2d_external` creates a texture whose memory can
 *     be exported, and `Resource.export_memory_fd` returns an fd for it.
 *   - `Device.create_external_semaphore` creates an exportable binary
 *     semaphore, and `Device.export_semaphore_fd` returns an fd for it.
 *     Opaque fds have reference semantics: the other device signals or
 *     waits on the same semaphore, every frame, from a fresh fd.
 *   - `Device.copy_texture_signal` copies a finished frame into a shared
 *     texture and signals a semaphore once the copy is done.
 *   - `Device.wait_semaphore` blocks until the other device signals a
 *     semaphore (GIL released).
 *
 * The other device must import the memory with the same size and memory
 * type, which lsfg does (first DEVICE_LOCAL type, dedicated allocation).
 *
 * Two Python threads use the device's queue (the pipeline and the frame
 * generation presenter), and Vulkan requires one at a time: like the rest
 * of this module, every submit, present and wait idle holds the device's
 * `queue_mutex` (vk_QueueLock). The GIL is not enough, since presenting
 * releases it while it waits for the display.
 */

#include "vk_device.h"
#include "vk_resource.h"
#include "vk_utils.h"
#include <algorithm>
#include <cstring>

// =============================================================================
//  Helpers
// =============================================================================

static bool require_external(vk_Device *dev) {
  if (!dev->supports_external_fd) {
    PyErr_SetString(PyExc_RuntimeError,
                    "The device does not support sharing memory and "
                    "semaphores by fd (VK_KHR_external_memory_fd, "
                    "VK_KHR_external_semaphore_fd)");
    return false;
  }
  return true;
}

/** First DEVICE_LOCAL memory type allowed by *type_bits* (as lsfg does). */
static uint32_t device_local_type(const VkPhysicalDeviceMemoryProperties &props,
                                  uint32_t type_bits) {
  for (uint32_t i = 0; i < props.memoryTypeCount; i++) {
    if ((type_bits & (1u << i)) && (props.memoryTypes[i].propertyFlags &
                                    VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT))
      return i;
  }
  return UINT32_MAX;
}

static VkSemaphore semaphore_from_handle(unsigned long long handle) {
  return reinterpret_cast<VkSemaphore>(static_cast<uintptr_t>(handle));
}

/** Wait for the previous copy_texture_signal (its command buffer is reused). */
static bool wait_pending_copy(vk_Device *dev) {
  if (!dev->ext_pending)
    return true;
  VkResult res;
  Py_BEGIN_ALLOW_THREADS;
  res = vkWaitForFences(dev->device, 1, &dev->ext_fence, VK_TRUE, UINT64_MAX);
  Py_END_ALLOW_THREADS;
  if (res != VK_SUCCESS) {
    PyErr_Format(PyExc_RuntimeError, "Waiting for a frame copy failed (%d)",
                 res);
    return false;
  }
  vkResetFences(dev->device, 1, &dev->ext_fence);
  dev->ext_pending = false;
  return true;
}

// =============================================================================
//  Device methods
// =============================================================================

/**
 * Create a 2D texture whose memory can be exported as an opaque fd.
 *
 * Same usage and layout (GENERAL) as `create_texture2d`, with a dedicated
 * allocation, which some drivers require for exported images.
 *
 * Args: width, height, format.
 */
PyObject *vk_Device_create_texture2d_external(vk_Device *self, PyObject *args) {
  uint32_t width, height;
  int format;
  if (!PyArg_ParseTuple(args, "IIi", &width, &height, &format))
    return nullptr;
  if (width == 0 || height == 0) {
    PyErr_SetString(vk_Texture2DError, "Invalid dimensions");
    return nullptr;
  }
  if (vk_format_map.find(format) == vk_format_map.end()) {
    PyErr_Format(vk_Texture2DError, "Invalid pixel format %d", format);
    return nullptr;
  }
  vk_Device *dev = vk_Device_get_initialized(self);
  if (!dev || !require_external(dev))
    return nullptr;

  VkFormat vk_format = vk_format_map[format].first;
  uint32_t bpp = vk_format_map[format].second;

  vk_Resource *res = PyObject_New(vk_Resource, &vk_Resource_Type);
  if (!res)
    return PyErr_NoMemory();
  VK_CLEAR_OBJECT(res);
  res->py_device = dev;
  Py_INCREF(dev);
  res->image_extent = {width, height, 1};
  res->slices = 1;
  res->row_pitch = width * bpp;
  res->size = res->row_pitch * height;
  res->format = vk_format;
  res->heap_type = 0;

  VkExternalMemoryImageCreateInfo external_info = {
      VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO};
  external_info.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;

  VkImageCreateInfo info = {VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
  info.pNext = &external_info;
  info.imageType = VK_IMAGE_TYPE_2D;
  info.format = vk_format;
  info.extent = {width, height, 1};
  info.mipLevels = 1;
  info.arrayLayers = 1;
  info.samples = VK_SAMPLE_COUNT_1_BIT;
  info.tiling = VK_IMAGE_TILING_OPTIMAL;
  info.usage = VK_IMAGE_USAGE_TRANSFER_SRC_BIT |
               VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_SAMPLED_BIT |
               VK_IMAGE_USAGE_STORAGE_BIT;
  info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
  info.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
  if (vkCreateImage(dev->device, &info, nullptr, &res->image) != VK_SUCCESS) {
    res->image = VK_NULL_HANDLE;
    Py_DECREF(res);
    PyErr_SetString(vk_Texture2DError, "Failed to create exportable image");
    return nullptr;
  }

  VkMemoryRequirements req;
  vkGetImageMemoryRequirements(dev->device, res->image, &req);
  res->heap_size = req.size;
  uint32_t type_index = device_local_type(dev->mem_props, req.memoryTypeBits);
  if (type_index == UINT32_MAX) {
    Py_DECREF(res);
    PyErr_SetString(vk_Texture2DError, "No device-local memory type");
    return nullptr;
  }

  VkMemoryDedicatedAllocateInfo dedicated = {
      VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO};
  dedicated.image = res->image;
  VkExportMemoryAllocateInfo export_info = {
      VK_STRUCTURE_TYPE_EXPORT_MEMORY_ALLOCATE_INFO};
  export_info.pNext = &dedicated;
  export_info.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
  VkMemoryAllocateInfo alloc = {VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  alloc.pNext = &export_info;
  alloc.allocationSize = req.size;
  alloc.memoryTypeIndex = type_index;
  VkResult vres = vkAllocateMemory(dev->device, &alloc, nullptr, &res->memory);
  if (vres != VK_SUCCESS) {
    res->memory = VK_NULL_HANDLE;
    Py_DECREF(res);
    PyErr_Format(vk_Texture2DError,
                 "Failed to allocate exportable memory (error %d)", vres);
    return nullptr;
  }
  VK_CHECK_OR_RETURN_NULL(
      vkBindImageMemory(dev->device, res->image, res->memory, 0),
      vk_Texture2DError, "Failed to bind image memory");

  VkImageViewCreateInfo vinfo = {VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO};
  vinfo.image = res->image;
  vinfo.viewType = VK_IMAGE_VIEW_TYPE_2D;
  vinfo.format = vk_format;
  vinfo.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
  vinfo.subresourceRange.levelCount = 1;
  vinfo.subresourceRange.layerCount = 1;
  VK_CHECK_OR_RETURN_NULL(
      vkCreateImageView(dev->device, &vinfo, nullptr, &res->image_view),
      vk_Texture2DError, "Failed to create image view");

  if (!vk_execute_one_time_commands(dev, [&](VkCommandBuffer cmd) {
        vk_cmd_transition_for_compute(cmd, res->image, 0, 1);
      })) {
    Py_DECREF(res);
    return nullptr;
  }

  res->descriptor_image_info.imageView = res->image_view;
  res->descriptor_image_info.imageLayout = VK_IMAGE_LAYOUT_GENERAL;
  return reinterpret_cast<PyObject *>(res);
}

/** Create an exportable binary semaphore. Returns its handle as an int. */
PyObject *vk_Device_create_external_semaphore(vk_Device *self,
                                              PyObject * /*ignored*/) {
  vk_Device *dev = vk_Device_get_initialized(self);
  if (!dev || !require_external(dev))
    return nullptr;
  VkExportSemaphoreCreateInfo export_info = {
      VK_STRUCTURE_TYPE_EXPORT_SEMAPHORE_CREATE_INFO};
  export_info.handleTypes = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_FD_BIT;
  VkSemaphoreCreateInfo info = {VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO};
  info.pNext = &export_info;
  VkSemaphore semaphore;
  VK_CHECK_OR_RETURN_NULL(
      vkCreateSemaphore(dev->device, &info, nullptr, &semaphore),
      PyExc_RuntimeError, "Failed to create exportable semaphore");
  return PyLong_FromUnsignedLongLong(
      static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(semaphore)));
}

/** Destroy a semaphore made by create_external_semaphore (waits idle). */
PyObject *vk_Device_destroy_semaphore(vk_Device *self, PyObject *args) {
  unsigned long long handle;
  if (!PyArg_ParseTuple(args, "K", &handle))
    return nullptr;
  if (self->device && handle) {
    {
      vk_QueueLock lock(self);
      vkQueueWaitIdle(self->queue);
    }
    vkDestroySemaphore(self->device, semaphore_from_handle(handle), nullptr);
  }
  Py_RETURN_NONE;
}

/** A new opaque fd for a semaphore (the receiver takes ownership). */
PyObject *vk_Device_export_semaphore_fd(vk_Device *self, PyObject *args) {
  unsigned long long handle;
  if (!PyArg_ParseTuple(args, "K", &handle))
    return nullptr;
  if (!require_external(self))
    return nullptr;
  VkSemaphoreGetFdInfoKHR info = {VK_STRUCTURE_TYPE_SEMAPHORE_GET_FD_INFO_KHR};
  info.semaphore = semaphore_from_handle(handle);
  info.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_FD_BIT;
  int fd = -1;
  VK_CHECK_OR_RETURN_NULL(self->get_semaphore_fd(self->device, &info, &fd),
                          PyExc_RuntimeError, "Failed to export semaphore fd");
  return PyLong_FromLong(fd);
}

/**
 * Copy *src* into *dst* (both GENERAL, same size) after all work already
 * submitted, then signal *semaphore*. Does not wait for the copy; the next
 * call does, since it reuses the command buffer.
 *
 * Args: src (Resource), dst (Resource), semaphore (int handle).
 */
PyObject *vk_Device_copy_texture_signal(vk_Device *self, PyObject *args) {
  PyObject *py_src, *py_dst;
  unsigned long long handle;
  if (!PyArg_ParseTuple(args, "O!O!K", &vk_Resource_Type, &py_src,
                        &vk_Resource_Type, &py_dst, &handle))
    return nullptr;
  vk_Resource *src = reinterpret_cast<vk_Resource *>(py_src);
  vk_Resource *dst = reinterpret_cast<vk_Resource *>(py_dst);
  if (!src->image || !dst->image) {
    PyErr_SetString(PyExc_TypeError, "src and dst must be images");
    return nullptr;
  }
  if (!require_external(self) || !wait_pending_copy(self))
    return nullptr;

  VkCommandBuffer cmd = self->ext_cmd;
  vkResetCommandBuffer(cmd, 0);
  VkCommandBufferBeginInfo begin = {VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
  begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
  vkBeginCommandBuffer(cmd, &begin);

  // Everything written before (the effects chain) is visible to the copy
  VkMemoryBarrier before = {VK_STRUCTURE_TYPE_MEMORY_BARRIER};
  before.srcAccessMask = VK_ACCESS_MEMORY_WRITE_BIT;
  before.dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT |
                         VK_ACCESS_TRANSFER_WRITE_BIT;
  vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                       VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 1, &before, 0,
                       nullptr, 0, nullptr);

  VkImageCopy region = {};
  region.srcSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
  region.srcSubresource.layerCount = 1;
  region.dstSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
  region.dstSubresource.layerCount = 1;
  region.extent = {
      std::min(src->image_extent.width, dst->image_extent.width),
      std::min(src->image_extent.height, dst->image_extent.height), 1};
  vkCmdCopyImage(cmd, src->image, VK_IMAGE_LAYOUT_GENERAL, dst->image,
                 VK_IMAGE_LAYOUT_GENERAL, 1, &region);

  // The copy is finished before the semaphore signals
  VkMemoryBarrier after = {VK_STRUCTURE_TYPE_MEMORY_BARRIER};
  after.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
  after.dstAccessMask = VK_ACCESS_MEMORY_READ_BIT;
  vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT,
                       VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, 0, 1, &after, 0,
                       nullptr, 0, nullptr);
  vkEndCommandBuffer(cmd);

  VkSemaphore semaphore = semaphore_from_handle(handle);
  VkSubmitInfo submit = {VK_STRUCTURE_TYPE_SUBMIT_INFO};
  submit.commandBufferCount = 1;
  submit.pCommandBuffers = &cmd;
  submit.signalSemaphoreCount = 1;
  submit.pSignalSemaphores = &semaphore;
  VkResult res;
  {
    vk_QueueLock lock(self);
    res = vkQueueSubmit(self->queue, 1, &submit, self->ext_fence);
  }
  VK_CHECK_OR_RETURN_NULL(res, PyExc_RuntimeError,
                          "Failed to submit the frame copy");
  self->ext_pending = true;
  Py_RETURN_NONE;
}

/**
 * Block until *semaphore* is signalled by the other device (it is waited
 * on, so it can be signalled again).
 *
 * Args: semaphore (int handle), timeout_ns (int, default 1 s).
 * Returns: True when signalled, False on timeout.
 */
PyObject *vk_Device_wait_semaphore(vk_Device *self, PyObject *args) {
  unsigned long long handle;
  unsigned long long timeout = 1000000000ULL;
  if (!PyArg_ParseTuple(args, "K|K", &handle, &timeout))
    return nullptr;
  if (!require_external(self))
    return nullptr;

  VkFence fence = vk_create_fence(self);
  if (fence == VK_NULL_HANDLE)
    return nullptr;
  VkSemaphore semaphore = semaphore_from_handle(handle);
  VkPipelineStageFlags stage = VK_PIPELINE_STAGE_ALL_COMMANDS_BIT;
  VkSubmitInfo submit = {VK_STRUCTURE_TYPE_SUBMIT_INFO};
  submit.waitSemaphoreCount = 1;
  submit.pWaitSemaphores = &semaphore;
  submit.pWaitDstStageMask = &stage;
  VkResult res;
  {
    vk_QueueLock lock(self);
    res = vkQueueSubmit(self->queue, 1, &submit, fence);
  }
  if (res == VK_SUCCESS) {
    Py_BEGIN_ALLOW_THREADS;
    res = vkWaitForFences(self->device, 1, &fence, VK_TRUE, timeout);
    // The submission must finish before its fence is destroyed
    if (res == VK_TIMEOUT) {
      std::lock_guard<std::mutex> lock(self->queue_mutex);
      vkQueueWaitIdle(self->queue);
    }
    Py_END_ALLOW_THREADS;
  }
  vkDestroyFence(self->device, fence, nullptr);
  if (res == VK_TIMEOUT)
    Py_RETURN_FALSE;
  VK_CHECK_OR_RETURN_NULL(res, PyExc_RuntimeError,
                          "Failed to wait for a shared semaphore");
  Py_RETURN_TRUE;
}

// =============================================================================
//  Resource methods
// =============================================================================

/** A new opaque fd for the texture's memory (the receiver takes ownership). */
PyObject *vk_Resource_export_memory_fd(vk_Resource *self,
                                       PyObject * /*ignored*/) {
  vk_Device *dev = self->py_device;
  if (!dev || !self->memory || self->py_heap) {
    PyErr_SetString(PyExc_TypeError, "Resource has no exportable memory");
    return nullptr;
  }
  if (!require_external(dev))
    return nullptr;
  VkMemoryGetFdInfoKHR info = {VK_STRUCTURE_TYPE_MEMORY_GET_FD_INFO_KHR};
  info.memory = self->memory;
  info.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
  int fd = -1;
  VK_CHECK_OR_RETURN_NULL(dev->get_memory_fd(dev->device, &info, &fd),
                          PyExc_RuntimeError,
                          "Failed to export memory fd (was the texture made "
                          "with create_texture2d_external?)");
  return PyLong_FromLong(fd);
}
