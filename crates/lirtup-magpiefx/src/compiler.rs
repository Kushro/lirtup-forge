//! HLSL → SPIR-V by subprocess, with a disk cache (`compiler.py`).
//!
//! - `dxc` (preferred) gets the `-fvk-*-shift` options of
//!   `scripts/hlsl-to-spv.fish`, `-HV 2018` (Magpie effects target fxc) and,
//!   for FP16, shader model 6.2 with `-enable-16bit-types`;
//! - `glslangValidator` is the fallback the three
//!   `Anime4K_Denoise_Bilateral_*` effects need (dxc rejects their
//!   intrinsic-sized arrays); FP16 adds `--hlsl-enable-16bit-types`.
//!
//! `UPSCALER_MAGPIEFX_COMPILER` (`dxc`, `glslang` or `auto`, the default)
//! picks the backends, `DXC` and `GLSLANG` override the binaries. The cache
//! key covers the source, the `.hlsli`/`.h` files next to the effect (their
//! names and contents, not the folder's path), the compiler kind and its
//! command line, and the swap rewrite. It does not cover the compiler's
//! version, so the precompiled seed (`magpiefx-seed`, see
//! [`SpirvCache::seed`]) hits whatever dxc or glslang is installed, or none.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use xxhash_rust::xxh64::Xxh64;

use crate::{MagpieError, Result};

/// What to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileRequest<'a> {
    /// Effect name (for errors).
    pub effect: &'a str,
    /// Pass, 1-based (for errors).
    pub pass: u32,
    /// HLSL source.
    pub source: &'a str,
    /// Entry point.
    pub entry: &'a str,
    /// Folder for `#include` (the effect's folder).
    pub include_dir: Option<&'a Path>,
    /// Use 16-bit types.
    pub fp16: bool,
}

/// An HLSL compiler.
pub trait HlslCompiler: Send + Sync {
    /// `dxc` or `glslangValidator`.
    fn name(&self) -> &str;

    /// `--version` output, for Diagnostics.
    fn version(&self) -> Option<String>;

    /// Compile to SPIR-V (same arguments as 1.x `_command`).
    fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>>;

    /// Run only the preprocessor (macros and includes expanded), for the
    /// red/blue swap rewrite. The default says it is not supported.
    fn preprocess(&self, request: &CompileRequest<'_>) -> Result<String> {
        Err(MagpieError::Compile {
            effect: request.effect.to_owned(),
            pass: request.pass,
            compiler: self.name().to_owned(),
            log: "preprocessing is not supported".to_owned(),
        })
    }

    /// What identifies the compiler and its flags in the cache key (the
    /// command line without the file names nor the include folder's path):
    /// [`kind_cache_key`] of [`HlslCompiler::name`].
    fn cache_key(&self, request: &CompileRequest<'_>) -> String {
        kind_cache_key(self.name(), request)
    }
}

/// Stands for the effect's folder in cache keys: the `-I` path changes
/// between installs (`/usr/share/lirtup/effects`, the AppImage's
/// `/tmp/.mount_*`, a source tree) while the include files, which the key
/// covers by contents, stay the same.
const INCLUDE_MARKER: &str = "<effect-dir>";

/// The compiler kinds the cache knows without an instance, in the default
/// preference order (the names of [`Dxc`] and [`Glslang`]).
const KNOWN_KINDS: [&str; 2] = ["dxc", "glslangValidator"];

/// The cache key of a compiler kind's command line for `request`, computed
/// without the compiler (it may not be installed: seed lookups). `dxc` and
/// `glslangValidator` get their real arguments with the include folder
/// replaced by a marker; any other name gets `<name> fp16=<bool>`.
pub fn kind_cache_key(name: &str, request: &CompileRequest<'_>) -> String {
    let include = request.include_dir.map(|_| Path::new(INCLUDE_MARKER));
    match name {
        "dxc" => format!(
            "dxc {}",
            Dxc::args("SRC", "OUT", request.entry, include, request.fp16).join(" ")
        ),
        "glslangValidator" => format!(
            "glslang {}",
            Glslang::args("SRC", "OUT", request.entry, include, request.fp16).join(" ")
        ),
        other => format!("{other} fp16={}", request.fp16),
    }
}

/// `dxc` (from `$DXC` or `PATH`).
pub struct Dxc {
    /// Binary.
    pub path: PathBuf,
}

/// `glslangValidator` (from `$GLSLANG` or `PATH`).
pub struct Glslang {
    /// Binary.
    pub path: PathBuf,
}

impl Dxc {
    /// Arguments after the binary (`_command("dxc", ...)` of 1.x).
    pub fn args(src: &str, out: &str, entry: &str, include_dir: Option<&Path>, fp16: bool) -> Vec<String> {
        let mut args: Vec<String> = [
            "-spirv",
            src,
            "-Fo",
            out,
            "-T",
            if fp16 { "cs_6_2" } else { "cs_6_0" },
            "-E",
            entry,
            // Magpie effects are written for fxc (HLSL 2018 semantics)
            "-HV",
            "2018",
            "-fvk-auto-shift-bindings",
            "-fvk-t-shift",
            "1024",
            "0",
            "-fvk-u-shift",
            "2048",
            "0",
            "-fvk-s-shift",
            "3072",
            "0",
            "-fvk-use-dx-layout",
            "-Wno-conversion",
        ]
        .map(str::to_owned)
        .to_vec();
        if fp16 {
            args.push("-enable-16bit-types".to_owned());
        }
        if let Some(dir) = include_dir {
            args.insert(0, format!("-I{}", dir.display()));
        }
        args
    }
}

impl Glslang {
    /// Arguments after the binary (`_command("glslang", ...)` of 1.x).
    pub fn args(src: &str, out: &str, entry: &str, include_dir: Option<&Path>, fp16: bool) -> Vec<String> {
        let mut args: Vec<String> = [
            "-V",
            "-D",
            "-S",
            "comp",
            "-e",
            entry,
            "--target-env",
            "vulkan1.1",
            "--hlsl-offsets",
            "--shift-texture-binding",
            "1024",
            "--shift-uav-binding",
            "2048",
            "--shift-image-binding",
            "2048",
            "--shift-sampler-binding",
            "3072",
            "-o",
            out,
            src,
        ]
        .map(str::to_owned)
        .to_vec();
        if fp16 {
            let at = args.len() - 3;
            args.insert(at, "--hlsl-enable-16bit-types".to_owned());
        }
        if let Some(dir) = include_dir {
            args.insert(0, format!("-I{}", dir.display()));
        }
        args
    }
}

/// `--version` of a binary, first non-empty line.
fn version_of(path: &Path) -> Option<String> {
    let out = Command::new(path).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(if out.stdout.is_empty() {
        &out.stderr
    } else {
        &out.stdout
    })
    .into_owned();
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// File name made of the effect and pass (`safe` of 1.x).
fn safe_name(request: &CompileRequest<'_>) -> String {
    let name = format!("{}_Pass{}", request.effect, request.pass);
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Run a compiler on `request` in a temporary folder and return the output
/// file. `args` gets `(src, out)`.
fn run_compiler(
    compiler: &str,
    exe: &Path,
    request: &CompileRequest<'_>,
    args: impl Fn(&str, &str) -> Vec<String>,
) -> Result<Vec<u8>> {
    let fail = |log: String| MagpieError::Compile {
        effect: request.effect.to_owned(),
        pass: request.pass,
        compiler: compiler.to_owned(),
        log,
    };
    let tmp = tempfile::Builder::new().prefix("magpiefx-").tempdir()?;
    let safe = safe_name(request);
    let src = tmp.path().join(format!("{safe}.hlsl"));
    let out = tmp.path().join(format!("{safe}.spv"));
    std::fs::write(&src, request.source)?;
    let output = Command::new(exe)
        .args(args(&src.to_string_lossy(), &out.to_string_lossy()))
        .output()
        .map_err(|e| fail(format!("cannot run {}: {e}", exe.display())))?;
    if !output.status.success() || !out.is_file() {
        let mut log = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if log.is_empty() {
            log = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        }
        if let Some(sig) = signal_of(&output.status) {
            log = format!("{compiler} crashed (signal {sig})\n{log}");
        }
        return Err(fail(log));
    }
    Ok(std::fs::read(&out)?)
}

fn signal_of(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status
        .signal()
        .or_else(|| status.code().filter(|c| *c >= 128).map(|c| c - 128))
}

/// Run a preprocessor command and return the expanded text: from `out_file`
/// when given, else from stdout.
fn run_preprocessor(
    compiler: &str,
    exe: &Path,
    request: &CompileRequest<'_>,
    args: impl Fn(&str, &str) -> Vec<String>,
    from_file: bool,
) -> Result<String> {
    let fail = |log: String| MagpieError::Compile {
        effect: request.effect.to_owned(),
        pass: request.pass,
        compiler: compiler.to_owned(),
        log: format!("preprocessing failed:\n{log}"),
    };
    let tmp = tempfile::Builder::new().prefix("magpiefx-").tempdir()?;
    let safe = safe_name(request);
    let src = tmp.path().join(format!("{safe}.hlsl"));
    let out = tmp.path().join(format!("{safe}.hlsl.i"));
    std::fs::write(&src, request.source)?;
    let output = Command::new(exe)
        .args(args(&src.to_string_lossy(), &out.to_string_lossy()))
        .output()
        .map_err(|e| fail(format!("cannot run {}: {e}", exe.display())))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let log = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            stderr
        };
        return Err(fail(log));
    }
    if from_file {
        Ok(std::fs::read_to_string(&out)?)
    } else {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn with_include(mut args: Vec<String>, include_dir: Option<&Path>) -> Vec<String> {
    if let Some(dir) = include_dir {
        args.insert(0, format!("-I{}", dir.display()));
    }
    args
}

impl HlslCompiler for Dxc {
    fn name(&self) -> &str {
        "dxc"
    }

    fn version(&self) -> Option<String> {
        version_of(&self.path)
    }

    fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>> {
        run_compiler("dxc", &self.path, request, |src, out| {
            Dxc::args(src, out, request.entry, request.include_dir, request.fp16)
        })
    }

    fn preprocess(&self, request: &CompileRequest<'_>) -> Result<String> {
        run_preprocessor(
            "dxc",
            &self.path,
            request,
            |src, out| {
                with_include(
                    vec!["-P".into(), "-Fi".into(), out.into(), src.into()],
                    request.include_dir,
                )
            },
            true,
        )
    }
}

impl HlslCompiler for Glslang {
    fn name(&self) -> &str {
        "glslangValidator"
    }

    fn version(&self) -> Option<String> {
        version_of(&self.path)
    }

    fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>> {
        run_compiler("glslangValidator", &self.path, request, |src, out| {
            Glslang::args(src, out, request.entry, request.include_dir, request.fp16)
        })
    }

    fn preprocess(&self, request: &CompileRequest<'_>) -> Result<String> {
        run_preprocessor(
            "glslangValidator",
            &self.path,
            request,
            |src, _| {
                with_include(
                    vec!["-E".into(), "-D".into(), "-S".into(), "comp".into(), src.into()],
                    request.include_dir,
                )
            },
            false,
        )
    }
}

/// First executable called `name` in `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn find_dxc() -> Option<PathBuf> {
    std::env::var_os("DXC")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| which("dxc"))
}

fn find_glslang() -> Option<PathBuf> {
    std::env::var_os("GLSLANG")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| which("glslangValidator"))
        .or_else(|| which("glslang"))
}

/// The compilers found, in preference order (`select_backends`: dxc first,
/// glslang as the fallback the 3 Denoise effects need). Empty when there is
/// none (or the one asked for in `UPSCALER_MAGPIEFX_COMPILER` is missing).
pub fn detect_compilers() -> Vec<Box<dyn HlslCompiler>> {
    let wanted = std::env::var("UPSCALER_MAGPIEFX_COMPILER")
        .unwrap_or_default()
        .to_lowercase();
    let wanted = if wanted.is_empty() {
        "auto".to_owned()
    } else {
        wanted
    };
    let mut found: Vec<Box<dyn HlslCompiler>> = Vec::new();
    if wanted == "auto" || wanted == "dxc" {
        match find_dxc() {
            Some(path) => found.push(Box::new(Dxc { path })),
            None if wanted == "dxc" => {
                tracing::warn!("dxc requested but not found (install directx-shader-compiler or set DXC)")
            }
            None => {}
        }
    }
    if wanted == "auto" || wanted == "glslang" {
        match find_glslang() {
            Some(path) => found.push(Box::new(Glslang { path })),
            None if wanted == "glslang" => tracing::warn!("glslangValidator requested but not found"),
            None => {}
        }
    }
    found
}

/// Bumped when the cache layout or the compile flags change. `rs2`: the
/// include folder's path left the key.
const CACHE_VERSION: &str = "rs2";

/// Makes the temporary file names of [`SpirvCache::store`] unique between
/// threads of one process (the seed generator writes in parallel).
static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

/// SPIR-V cache on disk, keyed by a digest of source, includes, compiler and
/// flags (`lirtup_config::paths::magpiefx_cache_dir`).
pub struct SpirvCache {
    /// Folder.
    pub dir: PathBuf,
    /// Read-only folder of precompiled SPIR-V with the same file names
    /// (`lirtup_config::paths::magpiefx_seed_dir`), looked up after `dir`;
    /// its hits are used as they are (not copied).
    pub seed: Option<PathBuf>,
}

/// A rewrite of the preprocessed source (the red/blue swap): its cache key
/// and the function, which returns `None` when it cannot rewrite the source.
pub struct Transform<'a> {
    /// Identifies the rewrite in the cache key.
    pub key: &'a str,
    /// The rewrite.
    pub apply: &'a dyn Fn(&str) -> Option<String>,
}

impl SpirvCache {
    /// Cache in `dir`, with the package's precompiled seed
    /// (`lirtup_config::paths::magpiefx_seed_dir`) as the read-only fallback.
    pub fn new(dir: PathBuf) -> SpirvCache {
        SpirvCache::with_seed(dir, lirtup_config::paths::magpiefx_seed_dir())
    }

    /// Cache in `dir` with an explicit seed folder (`None`: no seed). Does
    /// not read the environment.
    pub fn with_seed(dir: PathBuf, seed: Option<PathBuf>) -> SpirvCache {
        SpirvCache { dir, seed }
    }

    /// Compile with the first compiler that succeeds, reading and writing the
    /// cache.
    pub fn compile(
        &self,
        compilers: &[Box<dyn HlslCompiler>],
        request: &CompileRequest<'_>,
    ) -> Result<Vec<u8>> {
        Ok(self
            .compile_with(compilers, request, None)?
            .expect("no transform to decline"))
    }

    /// [`SpirvCache::compile`] with a rewrite of the preprocessed source.
    /// `Ok(None)` when the rewrite declined the source.
    ///
    /// Lookups come first and need no compiler: for each compiler kind (the
    /// given compilers in order, then the other known kinds), the cache
    /// folder and then the seed. Only when nothing hits does it compile;
    /// [`MagpieError::NoCompiler`] when there is nothing to compile with.
    pub fn compile_with(
        &self,
        compilers: &[Box<dyn HlslCompiler>],
        request: &CompileRequest<'_>,
        transform: Option<&Transform<'_>>,
    ) -> Result<Option<Vec<u8>>> {
        let transform_key = transform.map(|t| t.key);
        let includes = include_digest(request.include_dir);
        if let Some(spirv) = self.lookup(compilers, request, &includes, transform_key) {
            return Ok(Some(spirv));
        }
        if compilers.is_empty() {
            return Err(MagpieError::NoCompiler);
        }
        let mut errors: Vec<(String, String)> = Vec::new();
        for compiler in compilers {
            let name = entry_name(&compiler.cache_key(request), &includes, transform_key, request);
            let path = self.dir.join(name);
            let result = match transform {
                None => compiler.compile(request),
                Some(t) => {
                    let expanded = match compiler.preprocess(request) {
                        Ok(text) => text,
                        Err(e) => {
                            errors.push((compiler.name().to_owned(), compile_log(e)));
                            continue;
                        }
                    };
                    let Some(rewritten) = (t.apply)(&expanded) else {
                        return Ok(None);
                    };
                    compiler.compile(&CompileRequest {
                        source: &rewritten,
                        ..request.clone()
                    })
                }
            };
            match result {
                Ok(spirv) => {
                    if !errors.is_empty() {
                        tracing::warn!(
                            "{} pass {}: {} failed, compiled with {} instead",
                            request.effect,
                            request.pass,
                            errors[0].0,
                            compiler.name()
                        );
                    }
                    self.store(&path, &spirv);
                    return Ok(Some(spirv));
                }
                Err(e) => errors.push((compiler.name().to_owned(), compile_log(e))),
            }
        }
        let mut log: Vec<String> = Vec::new();
        if !compilers.iter().any(|c| c.name() == "glslangValidator") {
            // Some effects (the Anime4K_Denoise_Bilateral ones) only
            // compile with glslang; say so before dxc's diagnostics
            log.push(format!(
                "{} needs glslangValidator (dxc cannot compile it); install glslang to use this effect.",
                request.effect
            ));
        }
        log.extend(errors.iter().map(|(name, l)| format!("{name}:\n{l}")));
        Err(MagpieError::Compile {
            effect: request.effect.to_owned(),
            pass: request.pass,
            compiler: errors.iter().map(|e| e.0.as_str()).collect::<Vec<_>>().join(", "),
            log: log.join("\n"),
        })
    }

    /// The cached SPIR-V of `request`, from the cache folder or the seed,
    /// for any compiler kind (see [`SpirvCache::compile_with`]).
    fn lookup(
        &self,
        compilers: &[Box<dyn HlslCompiler>],
        request: &CompileRequest<'_>,
        includes: &str,
        transform: Option<&str>,
    ) -> Option<Vec<u8>> {
        let dirs: Vec<&Path> = std::iter::once(self.dir.as_path())
            .chain(self.seed.as_deref())
            .collect();
        for key in lookup_keys(compilers, request) {
            let name = entry_name(&key, includes, transform, request);
            for dir in &dirs {
                if let Ok(spirv) = std::fs::read(dir.join(&name))
                    && !spirv.is_empty()
                {
                    if *dir != self.dir.as_path() {
                        tracing::debug!(
                            "{} pass {}: precompiled SPIR-V from {}",
                            request.effect,
                            request.pass,
                            dir.display()
                        );
                    }
                    return Some(spirv);
                }
            }
        }
        None
    }

    /// Write atomically (temporary file and rename); failures only log.
    fn store(&self, path: &Path, spirv: &[u8]) {
        let tmp = path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            NEXT_TMP.fetch_add(1, Ordering::Relaxed)
        ));
        let result = std::fs::create_dir_all(&self.dir)
            .and_then(|()| std::fs::write(&tmp, spirv))
            .and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            tracing::debug!("cannot write MagpieFX cache {}: {e}", path.display());
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// The compiler keys to look up, in order: the given compilers', then the
/// known kinds that are not among them (a seed entry compiled with glslang
/// serves a machine that only has dxc, and the other way round). No
/// duplicates.
fn lookup_keys(compilers: &[Box<dyn HlslCompiler>], request: &CompileRequest<'_>) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let given = compilers.iter().map(|c| c.cache_key(request));
    let others = KNOWN_KINDS
        .iter()
        .filter(|kind| !compilers.iter().any(|c| c.name() == **kind))
        .map(|kind| kind_cache_key(kind, request));
    for key in given.chain(others) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// File name (in the cache folder and in the seed) of a request compiled
/// with the compiler whose key is `compiler_key`.
fn entry_name(
    compiler_key: &str,
    includes: &str,
    transform: Option<&str>,
    request: &CompileRequest<'_>,
) -> String {
    let parts: [&[u8]; 5] = [
        CACHE_VERSION.as_bytes(),
        compiler_key.as_bytes(),
        includes.as_bytes(),
        transform.unwrap_or("").as_bytes(),
        request.source.as_bytes(),
    ];
    format!("{}.spv", digest(&parts))
}

fn compile_log(e: MagpieError) -> String {
    match e {
        MagpieError::Compile { log, .. } => log,
        other => other.to_string(),
    }
}

/// 128-bit digest (two xxh64 with different seeds) of NUL-separated parts.
fn digest(parts: &[&[u8]]) -> String {
    let mut a = Xxh64::new(0);
    let mut b = Xxh64::new(0x9E37_79B9_7F4A_7C15);
    for part in parts {
        for h in [&mut a, &mut b] {
            h.update(part);
            h.update(b"\0");
        }
    }
    format!("{:016x}{:016x}", a.digest(), b.digest())
}

/// Digest of the `.hlsli` and `.h` files next to the effect (they may be
/// `#include`d).
fn include_digest(dir: Option<&Path>) -> String {
    let Some(entries) = dir.and_then(|d| std::fs::read_dir(d).ok()) else {
        return String::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "hlsli" || e == "h"))
        .collect();
    files.sort();
    let contents: Vec<(Vec<u8>, Vec<u8>)> = files
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().as_bytes().to_vec())
                .unwrap_or_default();
            (name, std::fs::read(p).unwrap_or_default())
        })
        .collect();
    let parts: Vec<&[u8]> = contents
        .iter()
        .flat_map(|(n, c)| [n.as_slice(), c.as_slice()])
        .collect();
    digest(&parts)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use super::*;

    type Answer = Box<dyn Fn(&CompileRequest<'_>) -> Result<Vec<u8>> + Send + Sync>;

    /// A compiler that records requests and answers from a closure.
    pub(crate) struct Fake {
        pub name: &'static str,
        pub calls: Mutex<Vec<(u32, bool)>>,
        pub answer: Answer,
    }

    impl Fake {
        pub(crate) fn new(
            name: &'static str,
            answer: impl Fn(&CompileRequest<'_>) -> Result<Vec<u8>> + Send + Sync + 'static,
        ) -> Fake {
            Fake {
                name,
                calls: Mutex::new(Vec::new()),
                answer: Box::new(answer),
            }
        }

        pub(crate) fn failure(request: &CompileRequest<'_>, log: &str) -> MagpieError {
            MagpieError::Compile {
                effect: request.effect.to_owned(),
                pass: request.pass,
                compiler: "fake".to_owned(),
                log: log.to_owned(),
            }
        }
    }

    impl HlslCompiler for Fake {
        fn name(&self) -> &str {
            self.name
        }

        fn version(&self) -> Option<String> {
            Some("fake 1.0".to_owned())
        }

        fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>> {
            self.calls.lock().unwrap().push((request.pass, request.fp16));
            (self.answer)(request)
        }

        fn preprocess(&self, request: &CompileRequest<'_>) -> Result<String> {
            Ok(request.source.to_owned())
        }
    }

    fn request(source: &str, fp16: bool) -> CompileRequest<'_> {
        CompileRequest {
            effect: "Test",
            pass: 1,
            source,
            entry: "__M",
            include_dir: None,
            fp16,
        }
    }

    #[test]
    fn dxc_flags() {
        let fp32 = Dxc::args("in.hlsl", "out.spv", "__M", None, false);
        let fp16 = Dxc::args("in.hlsl", "out.spv", "__M", None, true);
        let t = |a: &[String]| a[a.iter().position(|x| x == "-T").unwrap() + 1].clone();
        assert_eq!(t(&fp32), "cs_6_0");
        assert!(!fp32.contains(&"-enable-16bit-types".to_owned()));
        assert_eq!(t(&fp16), "cs_6_2");
        assert!(fp16.contains(&"-enable-16bit-types".to_owned()));
        assert!(fp32.windows(3).any(|w| w == ["-fvk-t-shift", "1024", "0"]));
        let inc = Dxc::args("in.hlsl", "out.spv", "__M", Some(Path::new("/inc")), false);
        assert_eq!(inc[0], "-I/inc");
    }

    #[test]
    fn glslang_flags() {
        let fp32 = Glslang::args("in.hlsl", "out.spv", "__M", None, false);
        let fp16 = Glslang::args("in.hlsl", "out.spv", "__M", Some(Path::new("/inc")), true);
        assert!(!fp32.contains(&"--hlsl-enable-16bit-types".to_owned()));
        assert!(fp16.contains(&"--hlsl-enable-16bit-types".to_owned()));
        // the flag goes before "-o out src"
        assert_eq!(fp16[fp16.len() - 3..], ["-o", "out.spv", "in.hlsl"]);
        assert_eq!(fp16[fp16.len() - 4], "--hlsl-enable-16bit-types");
        assert_eq!(fp16[0], "-I/inc");
    }

    #[test]
    fn cache_hits_skip_the_compiler() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::with_seed(dir.path().join("cache"), None);
        let fake: Vec<Box<dyn HlslCompiler>> = vec![Box::new(Fake::new("dxc", |_| Ok(b"SPIRV".to_vec())))];
        assert_eq!(cache.compile(&fake, &request("a", false)).unwrap(), b"SPIRV");
        assert_eq!(cache.compile(&fake, &request("a", false)).unwrap(), b"SPIRV");
        // a different source or flag is another entry
        cache.compile(&fake, &request("b", false)).unwrap();
        cache.compile(&fake, &request("a", true)).unwrap();
        assert_eq!(std::fs::read_dir(dir.path().join("cache")).unwrap().count(), 3);
    }

    #[test]
    fn falls_back_to_the_next_compiler() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::with_seed(dir.path().to_path_buf(), None);
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![
            Box::new(Fake::new("dxc", |r| {
                Err(Fake::failure(r, "variable length arrays"))
            })),
            Box::new(Fake::new("glslangValidator", |_| Ok(b"GLSLANG".to_vec()))),
        ];
        assert_eq!(
            cache.compile(&compilers, &request("x", false)).unwrap(),
            b"GLSLANG"
        );
    }

    #[test]
    fn errors_say_glslang_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::with_seed(dir.path().to_path_buf(), None);
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![Box::new(Fake::new("dxc", |r| {
            Err(Fake::failure(r, "variable length arrays"))
        }))];
        match cache.compile(&compilers, &request("x", false)) {
            Err(MagpieError::Compile { log, compiler, .. }) => {
                assert_eq!(compiler, "dxc");
                assert!(log.contains("needs glslangValidator"));
                assert!(log.contains("variable length arrays"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            cache.compile(&[], &request("x", false)),
            Err(MagpieError::NoCompiler)
        ));
    }

    #[test]
    fn transforms_rewrite_or_decline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpirvCache::with_seed(dir.path().to_path_buf(), None);
        let compilers: Vec<Box<dyn HlslCompiler>> =
            vec![Box::new(Fake::new("dxc", |r| Ok(r.source.as_bytes().to_vec())))];
        let upper = |s: &str| Some(s.to_uppercase());
        let t = Transform {
            key: "upper",
            apply: &upper,
        };
        assert_eq!(
            cache
                .compile_with(&compilers, &request("abc", false), Some(&t))
                .unwrap()
                .unwrap(),
            b"ABC"
        );
        let no = |_: &str| None;
        let t = Transform {
            key: "no",
            apply: &no,
        };
        assert_eq!(
            cache
                .compile_with(&compilers, &request("abc", false), Some(&t))
                .unwrap(),
            None
        );
    }

    /// A compiler named `name` that answers `answer` and counts its calls.
    fn counting(name: &'static str, answer: &'static [u8]) -> (Vec<Box<dyn HlslCompiler>>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let fake = Fake::new(name, move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(answer.to_vec())
        });
        let compilers: Vec<Box<dyn HlslCompiler>> = vec![Box::new(fake)];
        (compilers, calls)
    }

    fn with_include<'a>(source: &'a str, dir: &'a Path) -> CompileRequest<'a> {
        CompileRequest {
            include_dir: Some(dir),
            ..request(source, false)
        }
    }

    #[test]
    fn key_does_not_depend_on_the_include_path() {
        let root = tempfile::tempdir().unwrap();
        let (a, b) = (root.path().join("a"), root.path().join("b"));
        for dir in [&a, &b] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("common.hlsli"), "#define X 1").unwrap();
        }
        for name in KNOWN_KINDS {
            let key = kind_cache_key(name, &with_include("s", &a));
            assert!(key.contains("-I<effect-dir>"), "{key}");
            assert!(!key.contains(&*a.to_string_lossy()), "{key}");
            assert_eq!(key, kind_cache_key(name, &with_include("s", &b)));
        }

        let cache = SpirvCache::with_seed(root.path().join("cache"), None);
        let (dxc, calls) = counting("dxc", b"SPIRV");
        cache.compile(&dxc, &with_include("s", &a)).unwrap();
        // same source and include contents in another folder: a hit
        cache.compile(&dxc, &with_include("s", &b)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // the include contents still count
        std::fs::write(b.join("common.hlsli"), "#define X 2").unwrap();
        cache.compile(&dxc, &with_include("s", &b)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn dxc_and_glslang_keys_differ() {
        let r = request("s", false);
        assert_ne!(kind_cache_key("dxc", &r), kind_cache_key("glslangValidator", &r));
        assert_ne!(
            kind_cache_key("dxc", &r),
            kind_cache_key("dxc", &request("s", true))
        );
        assert_eq!(kind_cache_key("other", &r), "other fp16=false");
    }

    #[test]
    fn cache_version_is_bumped() {
        // rs1 keys had the include folder's path in them
        assert_eq!(CACHE_VERSION, "rs2");
    }

    /// A seed folder with `source` compiled by a compiler named `name`.
    fn seed_with(root: &Path, name: &'static str, source: &str) -> PathBuf {
        let seed = root.join("seed");
        let (compilers, _) = counting(name, b"SEED");
        SpirvCache::with_seed(seed.clone(), None)
            .compile(&compilers, &request(source, false))
            .unwrap();
        seed
    }

    #[test]
    fn seed_hits_without_any_compiler() {
        let root = tempfile::tempdir().unwrap();
        let seed = seed_with(root.path(), "dxc", "a");
        let user = root.path().join("user");
        let cache = SpirvCache::with_seed(user.clone(), Some(seed));
        assert_eq!(cache.compile(&[], &request("a", false)).unwrap(), b"SEED");
        // a compiler that is installed is not run either
        let (dxc, calls) = counting("dxc", b"NEW");
        assert_eq!(cache.compile(&dxc, &request("a", false)).unwrap(), b"SEED");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // seed hits are not copied
        assert!(!user.exists());
    }

    #[test]
    fn seed_entries_of_the_other_compiler_hit() {
        let root = tempfile::tempdir().unwrap();
        // the seed compiled it with glslang (a Bilateral effect), the user
        // only has dxc
        let seed = seed_with(root.path(), "glslangValidator", "a");
        let cache = SpirvCache::with_seed(root.path().join("user"), Some(seed));
        let (dxc, calls) = counting("dxc", b"NEW");
        assert_eq!(cache.compile(&dxc, &request("a", false)).unwrap(), b"SEED");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn user_cache_comes_before_the_seed() {
        let root = tempfile::tempdir().unwrap();
        let seed = seed_with(root.path(), "dxc", "a");
        let user = root.path().join("user");
        let (dxc, _) = counting("dxc", b"USER");
        SpirvCache::with_seed(user.clone(), None)
            .compile(&dxc, &request("a", false))
            .unwrap();
        let cache = SpirvCache::with_seed(user, Some(seed));
        assert_eq!(cache.compile(&[], &request("a", false)).unwrap(), b"USER");
    }

    #[test]
    fn seed_covers_transforms() {
        let root = tempfile::tempdir().unwrap();
        let seed = root.path().join("seed");
        let upper = |s: &str| Some(s.to_uppercase());
        let t = Transform {
            key: "upper",
            apply: &upper,
        };
        let compilers: Vec<Box<dyn HlslCompiler>> =
            vec![Box::new(Fake::new("dxc", |r| Ok(r.source.as_bytes().to_vec())))];
        SpirvCache::with_seed(seed.clone(), None)
            .compile_with(&compilers, &request("abc", false), Some(&t))
            .unwrap();
        let cache = SpirvCache::with_seed(root.path().join("user"), Some(seed));
        assert_eq!(
            cache
                .compile_with(&[], &request("abc", false), Some(&t))
                .unwrap()
                .unwrap(),
            b"ABC"
        );
        // another transform key is another entry
        let other = Transform {
            key: "other",
            apply: &upper,
        };
        assert!(matches!(
            cache.compile_with(&[], &request("abc", false), Some(&other)),
            Err(MagpieError::NoCompiler)
        ));
    }

    #[test]
    fn misses_when_neither_cache_nor_seed_has_it() {
        let root = tempfile::tempdir().unwrap();
        let seed = seed_with(root.path(), "dxc", "a");
        let user = root.path().join("user");
        let cache = SpirvCache::with_seed(user.clone(), Some(seed.clone()));
        assert!(matches!(
            cache.compile(&[], &request("b", false)),
            Err(MagpieError::NoCompiler)
        ));
        // compiled into the user cache, never into the seed
        let (dxc, calls) = counting("dxc", b"NEW");
        assert_eq!(cache.compile(&dxc, &request("b", false)).unwrap(), b"NEW");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read_dir(&user).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(&seed).unwrap().count(), 1);
    }

    #[test]
    fn a_missing_seed_folder_is_a_miss() {
        let root = tempfile::tempdir().unwrap();
        let cache = SpirvCache::with_seed(root.path().join("user"), Some(root.path().join("nope")));
        assert!(matches!(
            cache.compile(&[], &request("a", false)),
            Err(MagpieError::NoCompiler)
        ));
    }

    #[test]
    fn digests_differ() {
        assert_ne!(digest(&[b"a", b"b"]), digest(&[b"ab"]));
        assert_eq!(digest(&[b"a"]).len(), 32);
    }
}
