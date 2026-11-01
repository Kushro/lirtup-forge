/*
 * lsfg_c.h: the C API of the vendored lsfg-vk frame generation core, for
 * LirtUp Forge 2.0 (Rust, through the `lsfg-sys` crate).
 *
 * It replaces the CPython bridge of 1.x (src/upscaler/lsfg/lib/lsfg_module.cpp)
 * with the same calls: probe, initialize, create_context, present,
 * delete_context, finalize. The implementation (lsfg_c.cpp, front C) wraps
 * LSFG_3_1 / LSFG_3_1P and the shader loader of Enigmata Temple; it catches
 * every C++ exception at this boundary and turns it into a return code plus a
 * message for lsfg_last_error().
 *
 * Threads: lsfg keeps global state. Call initialize / finalize from one
 * thread; create_context / present / delete_context may come from another
 * (the frame generation thread of lirtup-pipeline) but never concurrently
 * with each other or with finalize. The caller serializes (Rust: a Mutex).
 *
 * Vulkan: lsfg creates its own VkInstance and VkDevice through volk (hidden
 * visibility, -Wl,-Bsymbolic), on the GPU given by vendor_id/device_id.
 * Images and semaphores are shared with the caller's device as opaque fds
 * (VK_KHR_external_memory_fd, VK_KHR_external_semaphore_fd).
 *
 * File descriptors: on success lsfg takes ownership of every fd passed in
 * (Vulkan import semantics); the caller must not close them. On failure the
 * fds that lsfg had not imported yet are still the caller's, but the caller
 * cannot tell which: some may already belong to the driver. Do not close any
 * of them after a failure (leaking a few fds on an error path is safe;
 * closing one the driver owns is not). Argument checks (LSFG_ERR_ARGUMENT,
 * LSFG_ERR_STATE) happen before any import.
 *
 * License: GPL-3.0-or-later (the workspace); lsfg-vk is MIT, the shader
 * loader GPL-3.0.
 */
#ifndef LSFG_C_H
#define LSFG_C_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Return codes. Non-negative values are results; negative values are errors
 * and lsfg_last_error() says more. */
#define LSFG_OK 0
#define LSFG_ERR_ARGUMENT (-1)       /* bad argument (generated out of 1..4, n == 0, ...) */
#define LSFG_ERR_STATE (-2)          /* not initialized, or initialized twice */
#define LSFG_ERR_DLL (-3)            /* Lossless.dll missing or unreadable */
#define LSFG_ERR_SHADERS (-4)        /* shader resources missing in the DLL */
#define LSFG_ERR_VULKAN (-5)         /* a Vulkan call inside lsfg failed */
#define LSFG_ERR_INTERNAL (-6)       /* any other exception */

/* lsfg_probe() results. */
#define LSFG_PROBE_FP16 0            /* shaders found, FP16 set included */
#define LSFG_PROBE_FP32_ONLY 1       /* shaders found, FP32 only */

/* lsfg_initialize() result bits. */
#define LSFG_INIT_FP16 1             /* FP16 shaders are in use */

/*
 * Check that `dll` (path to Lossless.dll) has the frame generation shaders
 * (RT_RCDATA 304..351 FP16, 353..400 FP32). Does not touch Vulkan; safe to
 * call at any time, from any thread.
 *
 * Returns LSFG_PROBE_FP16, LSFG_PROBE_FP32_ONLY, or a negative error; on error
 * a message is also written to `err` (NUL-terminated, truncated to `errlen`;
 * `err` may be NULL).
 */
int lsfg_probe(const char *dll, char *err, size_t errlen);

/*
 * Load the shaders and create lsfg's Vulkan device.
 *
 *   dll         path to Lossless.dll
 *   vendor_id   PCI vendor of the GPU to run on (VkPhysicalDeviceProperties)
 *   device_id   PCI device of the GPU to run on
 *   generated   frames generated per real frame: multiplier - 1, 1 to 4
 *   flow_scale  optical flow resolution as the user sets it, 0.25 to 1.0
 *               (lsfg receives the inverse, as Temple and 1.x do)
 *   performance non-zero: the lighter 3.1P model
 *   hdr         non-zero: HDR input (1.x always passes 0)
 *   fp16        non-zero: use FP16 shaders when the DLL has them
 *
 * Returns a non-negative bit set (LSFG_INIT_FP16) or a negative error.
 * Calling it twice without lsfg_finalize() is LSFG_ERR_STATE.
 */
int lsfg_initialize(const char *dll, uint32_t vendor_id, uint32_t device_id,
                    uint32_t generated, float flow_scale, int performance,
                    int hdr, int fp16);

/*
 * Create a context for one swapchain-sized stream.
 *
 *   in0, in1    memory fds of the two input images (lsfg alternates them)
 *   outs, n     memory fds of the n output images (n == generated)
 *   width, height, vk_format   size and VkFormat of all images
 *
 * Returns the context id (>= 0) or a negative error.
 */
int lsfg_create_context(int in0, int in1, const int *outs, size_t n,
                        uint32_t width, uint32_t height, uint32_t vk_format);

/*
 * Generate the frames between the previous input and the current one.
 *
 *   ctx          a context id
 *   in_sem       semaphore fd lsfg waits on (the new input is ready)
 *   out_sems, n  semaphore fds lsfg signals, one per generated frame
 *
 * Returns LSFG_OK or a negative error.
 */
int lsfg_present(int ctx, int in_sem, const int *out_sems, size_t n);

/* Destroy a context. Returns LSFG_OK or a negative error. */
int lsfg_delete_context(int ctx);

/* Destroy every context and lsfg's device. Safe to call when not
 * initialized. */
void lsfg_finalize(void);

/* Message of the last error on this thread (empty string if none). Valid
 * until the next lsfg_* call on the same thread. */
const char *lsfg_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* LSFG_C_H */
