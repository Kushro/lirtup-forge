//! MagpieFX: Magpie's effect format (`//!` directives over HLSL), loaded at
//! run time (`src/upscaler/magpiefx/`).
//!
//! Text in, SPIR-V and GPU passes out:
//!
//! 1. [`parser::parse_effect`] reads the directives into an [`EffectDesc`];
//! 2. [`codegen::generate_pass_source`] writes one HLSL compute shader per
//!    pass;
//! 3. a [`compiler::HlslCompiler`] (subprocess `dxc`, or `glslangValidator`)
//!    turns it into SPIR-V, through the [`compiler::SpirvCache`];
//! 4. [`runtime::MagpieEffect`] binds it all on the GPU and implements
//!    `lirtup_effects::Pass`.
//!
//! [`catalog`] lists the built-in effects (`src/upscaler/magpiefx/effects`,
//! 117 today) and the user's (`~/.config/linux-rt-upscaler/effects`), with the
//! `verified` flag from `magpiefx-report.md`. [`spec`] parses the
//! `magpie_effects:` items.

// Round 0 stubs: the bodies are `todo!()`. Front E removes this.
#![allow(unused_variables)]

pub mod catalog;
pub mod codegen;
pub mod compiler;
pub mod dds;
pub mod error;
pub mod expr;
pub mod formats;
pub mod parser;
pub mod runtime;
pub mod spec;
pub mod swizzle;

pub use catalog::{Catalog, CatalogEntry, EffectOrigin};
pub use error::{MagpieError, Result};
pub use parser::{EffectDesc, EffectParameter, EffectPass, EffectSampler, EffectTexture, ParamKind};
pub use runtime::{CompiledEffect, MagpieEffect};
pub use spec::EffectSpec;
