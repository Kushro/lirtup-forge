//! Raw FFI to `include/lsfg_c.h`, the C API of the vendored lsfg-vk core.
//!
//! Declarations are written by hand (the API is seven functions; no
//! bindgen). The safe wrapper is `lirtup_fg::native`. Read the header for the
//! ownership and threading rules: on success lsfg owns the fds passed in;
//! after a failure the caller must not close them either (leak them).

#![allow(unsafe_code)]

use std::ffi::{c_char, c_float, c_int};

/// Success.
pub const LSFG_OK: c_int = 0;
/// Bad argument.
pub const LSFG_ERR_ARGUMENT: c_int = -1;
/// Not initialized, or initialized twice.
pub const LSFG_ERR_STATE: c_int = -2;
/// `Lossless.dll` missing or unreadable.
pub const LSFG_ERR_DLL: c_int = -3;
/// Shader resources missing in the DLL.
pub const LSFG_ERR_SHADERS: c_int = -4;
/// A Vulkan call inside lsfg failed.
pub const LSFG_ERR_VULKAN: c_int = -5;
/// Any other exception.
pub const LSFG_ERR_INTERNAL: c_int = -6;

/// `lsfg_probe`: FP16 set present.
pub const LSFG_PROBE_FP16: c_int = 0;
/// `lsfg_probe`: FP32 only.
pub const LSFG_PROBE_FP32_ONLY: c_int = 1;

/// `lsfg_initialize` result bit: FP16 shaders in use.
pub const LSFG_INIT_FP16: c_int = 1;

unsafe extern "C" {
    /// See `lsfg_c.h`.
    pub fn lsfg_probe(dll: *const c_char, err: *mut c_char, errlen: usize) -> c_int;

    /// See `lsfg_c.h`.
    pub fn lsfg_initialize(
        dll: *const c_char,
        vendor_id: u32,
        device_id: u32,
        generated: u32,
        flow_scale: c_float,
        performance: c_int,
        hdr: c_int,
        fp16: c_int,
    ) -> c_int;

    /// See `lsfg_c.h`.
    pub fn lsfg_create_context(
        in0: c_int,
        in1: c_int,
        outs: *const c_int,
        n: usize,
        width: u32,
        height: u32,
        vk_format: u32,
    ) -> c_int;

    /// See `lsfg_c.h`.
    pub fn lsfg_present(ctx: c_int, in_sem: c_int, out_sems: *const c_int, n: usize) -> c_int;

    /// See `lsfg_c.h`.
    pub fn lsfg_delete_context(ctx: c_int) -> c_int;

    /// See `lsfg_c.h`.
    pub fn lsfg_finalize();

    /// See `lsfg_c.h`.
    pub fn lsfg_last_error() -> *const c_char;
}
