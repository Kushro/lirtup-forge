//! HLSL → SPIR-V by subprocess, with a disk cache (`compiler.py`).

use std::path::{Path, PathBuf};

use crate::Result;

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

impl HlslCompiler for Dxc {
    fn name(&self) -> &str {
        "dxc"
    }

    fn version(&self) -> Option<String> {
        todo!("front E")
    }

    fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>> {
        todo!("front E")
    }
}

impl HlslCompiler for Glslang {
    fn name(&self) -> &str {
        "glslangValidator"
    }

    fn version(&self) -> Option<String> {
        todo!("front E")
    }

    fn compile(&self, request: &CompileRequest<'_>) -> Result<Vec<u8>> {
        todo!("front E")
    }
}

/// The compilers found, in preference order (`select_backends`: dxc first,
/// glslang as the fallback the 3 Denoise effects need).
pub fn detect_compilers() -> Vec<Box<dyn HlslCompiler>> {
    todo!("front E")
}

/// SPIR-V cache on disk, keyed by a digest of source, includes, compiler and
/// flags (`lirtup_config::paths::magpiefx_cache_dir`).
pub struct SpirvCache {
    /// Folder.
    pub dir: PathBuf,
}

impl SpirvCache {
    /// Cache in `dir`.
    pub fn new(dir: PathBuf) -> SpirvCache {
        SpirvCache { dir }
    }

    /// Compile with the first compiler that succeeds, reading and writing the
    /// cache.
    pub fn compile(
        &self,
        compilers: &[Box<dyn HlslCompiler>],
        request: &CompileRequest<'_>,
    ) -> Result<Vec<u8>> {
        todo!("front E")
    }
}
