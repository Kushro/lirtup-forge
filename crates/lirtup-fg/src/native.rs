//! Safe wrapper over `lsfg-sys` (the built-in engine).
//!
//! lsfg has global state, so [`LsfgEngine`] is a process-wide singleton:
//! [`LsfgEngine::initialize`] fails while another one is alive. Contexts
//! hold an `Arc` of the engine; dropping a context deletes it in lsfg, and
//! lsfg is finalized when the last `Arc` goes. All calls are serialized inside (one mutex), so the
//! engine is `Send + Sync`.
//!
//! File descriptors: lsfg takes the fds it is given when a call succeeds.
//! When a call fails after its argument checks, some fds may already belong
//! to the driver, so they are leaked instead of closed (see `lsfg_c.h`).

use std::ffi::{CStr, CString, c_char, c_int};
use std::os::fd::{AsRawFd, IntoRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use lirtup_config::Config;

use crate::{FgError, Result};

/// Whether an [`LsfgEngine`] is alive in this process.
static ENGINE_ALIVE: AtomicBool = AtomicBool::new(false);

/// What a DLL offers (`lsfg_probe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Probe {
    /// FP16 and FP32 shaders.
    Fp16,
    /// FP32 only.
    Fp32Only,
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| FgError::Invalid(format!("{}: path with a NUL byte", path.display())))
}

/// `lsfg_last_error()` of this thread.
fn last_error() -> String {
    // SAFETY: lsfg_last_error returns a NUL-terminated string owned by lsfg
    // (thread-local), valid until the next lsfg call on this thread; it is
    // copied right away.
    unsafe {
        let ptr = lsfg_sys::lsfg_last_error();
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

fn lsfg_error(code: c_int) -> FgError {
    FgError::Lsfg {
        code,
        message: last_error(),
    }
}

/// Whether a failed call stopped before importing any fd.
fn failed_before_import(code: c_int) -> bool {
    matches!(code, lsfg_sys::LSFG_ERR_ARGUMENT | lsfg_sys::LSFG_ERR_STATE)
}

/// Check a `Lossless.dll` without touching Vulkan.
pub fn probe(dll: &Path) -> Result<Probe> {
    let path = c_path(dll)?;
    let mut err = [0 as c_char; 512];
    // SAFETY: `path` is a valid C string and `err` a writable buffer of the
    // given length; lsfg_probe may be called from any thread at any time.
    let rc = unsafe { lsfg_sys::lsfg_probe(path.as_ptr(), err.as_mut_ptr(), err.len()) };
    match rc {
        lsfg_sys::LSFG_PROBE_FP16 => Ok(Probe::Fp16),
        lsfg_sys::LSFG_PROBE_FP32_ONLY => Ok(Probe::Fp32Only),
        code => {
            // SAFETY: lsfg_probe NUL-terminates `err` (zeroed before the call).
            let message = unsafe { CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            Err(FgError::Lsfg { code, message })
        }
    }
}

/// Options of [`LsfgEngine::initialize`] (see `lsfg_initialize` in
/// `lsfg_c.h`).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    /// `Lossless.dll`.
    pub dll: PathBuf,
    /// PCI vendor of the GPU (`lirtup_gpu::AdapterInfo::vendor_id`).
    pub vendor_id: u32,
    /// PCI device of the GPU.
    pub device_id: u32,
    /// Generated frames per real frame (multiplier − 1), 1 to 4.
    pub generated: u32,
    /// Flow scale, 0.25 to 1.0.
    pub flow_scale: f32,
    /// Lighter model.
    pub performance: bool,
    /// HDR input.
    pub hdr: bool,
    /// Use FP16 shaders if available.
    pub fp16: bool,
}

impl EngineOptions {
    /// The options for `generated` frames from the frame generation fields
    /// of `config` (flow scale, performance mode) and the GPU lirtup runs on.
    /// `fp16` is whether that GPU supports FP16 shaders.
    pub fn from_config(
        config: &Config,
        dll: PathBuf,
        vendor_id: u32,
        device_id: u32,
        generated: u32,
        fp16: bool,
    ) -> EngineOptions {
        EngineOptions {
            dll,
            vendor_id,
            device_id,
            generated,
            flow_scale: config.frame_gen_flow_scale.clamp(0.25, 1.0),
            performance: config.frame_gen_performance,
            hdr: false,
            fp16,
        }
    }
}

/// The initialized lsfg engine.
pub struct LsfgEngine {
    options: EngineOptions,
    fp16: bool,
    /// Serializes every call into lsfg.
    lock: Mutex<()>,
}

impl std::fmt::Debug for LsfgEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LsfgEngine")
            .field("options", &self.options)
            .field("fp16", &self.fp16)
            .finish()
    }
}

impl LsfgEngine {
    /// Initialize lsfg (one engine per process).
    pub fn initialize(options: &EngineOptions) -> Result<Arc<LsfgEngine>> {
        if !(1..=4).contains(&options.generated) {
            return Err(FgError::Invalid(format!(
                "generated frames must be 1 to 4, got {}",
                options.generated
            )));
        }
        let dll = c_path(&options.dll)?;
        if ENGINE_ALIVE.swap(true, Ordering::AcqRel) {
            return Err(FgError::Lsfg {
                code: lsfg_sys::LSFG_ERR_STATE,
                message: "an lsfg engine is already running in this process".into(),
            });
        }
        // SAFETY: `dll` is a valid C string; ENGINE_ALIVE guarantees no other
        // engine (and so no other lsfg call) exists while this one runs.
        let rc = unsafe {
            lsfg_sys::lsfg_initialize(
                dll.as_ptr(),
                options.vendor_id,
                options.device_id,
                options.generated,
                options.flow_scale.clamp(0.25, 1.0),
                c_int::from(options.performance),
                c_int::from(options.hdr),
                c_int::from(options.fp16),
            )
        };
        if rc < 0 {
            let err = lsfg_error(rc);
            ENGINE_ALIVE.store(false, Ordering::Release);
            return Err(err);
        }
        let fp16 = rc & lsfg_sys::LSFG_INIT_FP16 != 0;
        tracing::info!(
            "lsfg initialized: x{} ({}, {})",
            options.generated + 1,
            if options.performance { "3.1P" } else { "3.1" },
            if fp16 { "FP16" } else { "FP32" }
        );
        Ok(Arc::new(LsfgEngine {
            options: options.clone(),
            fp16,
            lock: Mutex::new(()),
        }))
    }

    /// Whether FP16 shaders are in use.
    pub fn fp16(&self) -> bool {
        self.fp16
    }

    /// The options it was created with.
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Create a context: two input image fds, `generated` output image fds,
    /// size and `VkFormat` (as a number). lsfg takes the fds.
    pub fn create_context(
        self: &Arc<Self>,
        inputs: [OwnedFd; 2],
        outputs: Vec<OwnedFd>,
        width: u32,
        height: u32,
        vk_format: u32,
    ) -> Result<LsfgContext> {
        if outputs.len() != self.options.generated as usize {
            return Err(FgError::Invalid(format!(
                "{} output images for {} generated frames",
                outputs.len(),
                self.options.generated
            )));
        }
        let outs: Vec<c_int> = outputs.iter().map(AsRawFd::as_raw_fd).collect();
        let rc = {
            let _guard = self.guard();
            // SAFETY: the fds are open (owned here), `outs` holds `outs.len()`
            // of them, and the engine mutex serializes lsfg calls.
            unsafe {
                lsfg_sys::lsfg_create_context(
                    inputs[0].as_raw_fd(),
                    inputs[1].as_raw_fd(),
                    outs.as_ptr(),
                    outs.len(),
                    width,
                    height,
                    vk_format,
                )
            }
        };
        if rc < 0 {
            let err = lsfg_error(rc);
            if !failed_before_import(rc) {
                release_fds(inputs.into_iter().chain(outputs));
            }
            return Err(err);
        }
        release_fds(inputs.into_iter().chain(outputs));
        Ok(LsfgContext {
            engine: Arc::clone(self),
            id: rc,
        })
    }
}

/// lsfg starting (or restarting with other options) on a thread of its own,
/// `lsfg-restart` (`_begin_restart` and `_restart_lsfg` of 1.x).
///
/// Initializing takes a while (lsfg creates its device and loads the
/// shaders from the DLL), and lsfg fixes the number of generated frames
/// then, so a new multiplier means finalizing and initializing again. Until
/// it is done, the pipeline presents its real frames alone.
pub struct EngineStart {
    options: EngineOptions,
    thread: Option<JoinHandle<()>>,
    result: Receiver<Result<Arc<LsfgEngine>>>,
}

impl std::fmt::Debug for EngineStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineStart")
            .field("options", &self.options)
            .finish()
    }
}

impl EngineStart {
    /// Finalize `previous` (when given) and initialize lsfg with `options`
    /// on a new thread.
    ///
    /// `previous` must be the last reference to its engine (every
    /// [`LsfgContext`] of it dropped): lsfg admits one engine per process.
    pub fn spawn(previous: Option<Arc<LsfgEngine>>, options: EngineOptions) -> Result<EngineStart> {
        if let Some(engine) = &previous
            && Arc::strong_count(engine) != 1
        {
            return Err(FgError::Invalid(
                "restarting lsfg while one of its contexts is alive".into(),
            ));
        }
        let (tx, result) = mpsc::sync_channel(1);
        let thread_options = options.clone();
        let thread = std::thread::Builder::new()
            .name("lsfg-restart".into())
            .spawn(move || {
                // Finalized here: it can take as long as initializing.
                drop(previous);
                let _ = tx.send(LsfgEngine::initialize(&thread_options));
            })?;
        Ok(EngineStart {
            options,
            thread: Some(thread),
            result,
        })
    }

    /// The options it initializes with.
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }

    /// The engine (or the error) once the thread is done; `None` while it
    /// still works. Call it until it returns `Some`.
    pub fn poll(&mut self) -> Option<Result<Arc<LsfgEngine>>> {
        match self.result.try_recv() {
            Ok(result) => {
                self.join();
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.join();
                Some(Err(FgError::Invalid("the lsfg-restart thread panicked".into())))
            }
        }
    }

    /// Block until the thread is done (lsfg cannot be finalized while it
    /// initializes).
    pub fn wait(mut self) -> Result<Arc<LsfgEngine>> {
        self.join();
        self.result
            .try_recv()
            .unwrap_or_else(|_| Err(FgError::Invalid("the lsfg-restart thread panicked".into())))
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for EngineStart {
    fn drop(&mut self) {
        // Wait for the thread: the engine it makes is finalized when its
        // result (still in the channel) is dropped, after this.
        self.join();
    }
}

/// Give up ownership without closing: lsfg (or the driver) owns them now.
fn release_fds(fds: impl IntoIterator<Item = OwnedFd>) {
    for fd in fds {
        let _ = fd.into_raw_fd();
    }
}

/// One lsfg context.
pub struct LsfgContext {
    engine: Arc<LsfgEngine>,
    id: c_int,
}

impl std::fmt::Debug for LsfgContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LsfgContext").field("id", &self.id).finish()
    }
}

impl LsfgContext {
    /// Generate the frames for the newest input: lsfg waits on `input_ready`
    /// and signals one of `outputs_ready` per generated frame. lsfg takes the
    /// fds.
    pub fn present(&mut self, input_ready: OwnedFd, outputs_ready: Vec<OwnedFd>) -> Result<()> {
        let generated = self.engine.options.generated as usize;
        if outputs_ready.len() != generated {
            return Err(FgError::Invalid(format!(
                "{} output semaphores for {generated} generated frames",
                outputs_ready.len()
            )));
        }
        let outs: Vec<c_int> = outputs_ready.iter().map(AsRawFd::as_raw_fd).collect();
        let rc = {
            let _guard = self.engine.guard();
            // SAFETY: the fds are open (owned here), `outs` holds `outs.len()`
            // of them, `id` is a live context, and the engine mutex
            // serializes lsfg calls.
            unsafe { lsfg_sys::lsfg_present(self.id, input_ready.as_raw_fd(), outs.as_ptr(), outs.len()) }
        };
        if rc < 0 {
            let err = lsfg_error(rc);
            if !failed_before_import(rc) {
                release_fds(std::iter::once(input_ready).chain(outputs_ready));
            }
            return Err(err);
        }
        release_fds(std::iter::once(input_ready).chain(outputs_ready));
        Ok(())
    }

    /// The lsfg context id.
    pub fn id(&self) -> i32 {
        self.id
    }
}

impl Drop for LsfgContext {
    fn drop(&mut self) {
        let _guard = self.engine.guard();
        // SAFETY: `id` is a live context of this engine (deleted only here),
        // and the engine mutex serializes lsfg calls.
        let rc = unsafe { lsfg_sys::lsfg_delete_context(self.id) };
        if rc < 0 {
            tracing::warn!("lsfg_delete_context({}): {}", self.id, last_error());
        }
    }
}

impl Drop for LsfgEngine {
    fn drop(&mut self) {
        // SAFETY: the last Arc is gone, so no context (they hold an Arc) and
        // no other call into lsfg exists.
        unsafe { lsfg_sys::lsfg_finalize() };
        ENGINE_ALIVE.store(false, Ordering::Release);
        tracing::debug!("lsfg finalized");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_is_send_sync() {
        fn check<T: Send + Sync>() {}
        check::<LsfgEngine>();
        check::<Arc<LsfgEngine>>();
        fn send<T: Send>() {}
        send::<LsfgContext>();
    }

    #[test]
    fn options_from_config_clamp_the_flow_scale() {
        let config = Config {
            frame_gen_flow_scale: 0.1,
            frame_gen_performance: true,
            ..Config::default()
        };
        let options = EngineOptions::from_config(&config, "/x/Lossless.dll".into(), 0x1002, 0x7550, 3, true);
        assert_eq!(options.flow_scale, 0.25);
        assert!(options.performance);
        assert_eq!(options.generated, 3);
        assert!(!options.hdr);
    }

    #[test]
    fn a_failed_start_reports_its_error() {
        let options = EngineOptions {
            dll: "/nonexistent/Lossless.dll".into(),
            vendor_id: 0,
            device_id: 0,
            generated: 0,
            flow_scale: 1.0,
            performance: false,
            hdr: false,
            fp16: false,
        };
        let start = EngineStart::spawn(None, options).unwrap();
        assert!(matches!(start.wait(), Err(FgError::Invalid(_))));
    }

    #[test]
    fn probe_of_a_missing_dll_fails() {
        let err = probe(Path::new("/nonexistent/Lossless.dll")).unwrap_err();
        assert!(
            matches!(
                err,
                FgError::Lsfg {
                    code: lsfg_sys::LSFG_ERR_DLL,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn bad_generated_is_rejected_before_lsfg() {
        let options = EngineOptions {
            dll: "/nonexistent/Lossless.dll".into(),
            vendor_id: 0,
            device_id: 0,
            generated: 5,
            flow_scale: 1.0,
            performance: false,
            hdr: false,
            fp16: true,
        };
        assert!(matches!(
            LsfgEngine::initialize(&options),
            Err(FgError::Invalid(_))
        ));
        assert!(!ENGINE_ALIVE.load(Ordering::Acquire));
    }

    /// Criterion of 5.8: `lsfg_probe` finds `Lossless.dll` (in Steam, or at
    /// `LIRTUP_LOSSLESS_DLL`). Skipped when Lossless Scaling is not installed.
    #[test]
    fn probe_finds_lossless_dll() {
        let dll = std::env::var_os("LIRTUP_LOSSLESS_DLL")
            .map(PathBuf::from)
            .or_else(|| crate::dll::find_dll(crate::dll::DllKind::Lossless, None));
        let Some(dll) = dll else {
            eprintln!("Lossless.dll not found, skipped");
            return;
        };
        let probe = probe(&dll).unwrap_or_else(|e| panic!("{}: {e}", dll.display()));
        eprintln!("{}: {probe:?}", dll.display());
    }
}
