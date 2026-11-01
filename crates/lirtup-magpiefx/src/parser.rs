//! The `//!` directive reader (`parser.py`).

use serde::{Deserialize, Serialize};

use crate::Result;

/// Kind of a `//!PARAMETER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// `float`.
    Float,
    /// `int`.
    Int,
}

/// A `//!PARAMETER` (the GUI generates one control per parameter).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectParameter {
    /// HLSL name.
    pub name: String,
    /// Kind.
    pub kind: ParamKind,
    /// `//!DEFAULT`.
    pub default: f64,
    /// `//!MIN`.
    pub min: f64,
    /// `//!MAX`.
    pub max: f64,
    /// `//!STEP`.
    pub step: f64,
    /// `//!LABEL` (empty = use the name).
    pub label: String,
}

/// A `//!TEXTURE` (index 0 is INPUT, 1 is OUTPUT).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectTexture {
    /// HLSL name.
    pub name: String,
    /// `//!FORMAT` (empty for a `//!SOURCE` without one).
    pub format: String,
    /// `//!WIDTH` expression.
    pub width_expr: String,
    /// `//!HEIGHT` expression.
    pub height_expr: String,
    /// `//!SOURCE` file, relative to the effect.
    pub source: String,
}

/// A `//!SAMPLER`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectSampler {
    /// HLSL name.
    pub name: String,
    /// `POINT` or `LINEAR`.
    pub filter: String,
    /// `CLAMP` or `WRAP`.
    pub address: String,
}

/// A `//!PASS`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectPass {
    /// 1-based index.
    pub index: u32,
    /// Texture indices read.
    pub inputs: Vec<u32>,
    /// Texture indices written.
    pub outputs: Vec<u32>,
    /// `//!BLOCK_SIZE`.
    pub block_size: (u32, u32),
    /// `//!NUM_THREADS`.
    pub num_threads: (u32, u32, u32),
    /// `//!STYLE PS`.
    pub ps_style: bool,
    /// `//!DESC`.
    pub desc: String,
    /// The HLSL body.
    pub code: String,
}

/// A parsed effect (`EffectDesc` in 1.x).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectDesc {
    /// Name (file stem).
    pub name: String,
    /// `//!SORT_NAME`.
    pub sort_name: String,
    /// `//!USE MulAdd`.
    pub use_muladd: bool,
    /// `//!USE Dynamic`.
    pub use_dynamic: bool,
    /// `//!CAPABILITY FP16`.
    pub supports_fp16: bool,
    /// Code of the header block (`#include`, ...).
    pub header_code: String,
    /// Parameters.
    pub params: Vec<EffectParameter>,
    /// Textures, `[INPUT, OUTPUT, ...]`.
    pub textures: Vec<EffectTexture>,
    /// Samplers.
    pub samplers: Vec<EffectSampler>,
    /// `//!COMMON` blocks.
    pub commons: Vec<String>,
    /// Passes.
    pub passes: Vec<EffectPass>,
    /// Non-fatal problems.
    pub warnings: Vec<String>,
}

impl EffectDesc {
    /// A parameter by name.
    pub fn param(&self, name: &str) -> Option<&EffectParameter> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// Strip `//` and `/* */` comments, keeping `//!` directives and line
/// numbers.
pub fn remove_comments(source: &str) -> String {
    todo!("front E")
}

/// Parse an effect.
pub fn parse_effect(source: &str, name: &str) -> Result<EffectDesc> {
    todo!("front E")
}
