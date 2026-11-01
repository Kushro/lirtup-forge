//! The `//!` directive reader (`parser.py`).
//!
//! Follows Magpie's `EffectCompiler.cpp` (MagpieFX version 4) so that the
//! effects shipped with Magpie load unmodified. Only reads the source: code
//! generation is [`crate::codegen`], execution [`crate::runtime`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{MagpieError, Result};

/// The only MagpieFX version supported.
pub const MAGPIE_FX_VERSION: u32 = 4;

/// Kind of a `//!PARAMETER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// `float`.
    Float,
    /// `int`.
    Int,
}

impl ParamKind {
    /// HLSL spelling.
    pub fn hlsl(self) -> &'static str {
        match self {
            ParamKind::Float => "float",
            ParamKind::Int => "int",
        }
    }
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

    /// The OUTPUT texture.
    pub fn output(&self) -> &EffectTexture {
        &self.textures[1]
    }
}

/// Strip `//` and `/* */` comments, keeping `//!` directives and line
/// numbers. Line comments keep their newline; block comments are removed
/// whole, as in Magpie (an unterminated one runs to the end).
pub fn remove_comments(source: &str) -> String {
    strip_comments(source).0
}

/// [`remove_comments`] plus whether a block comment was left open.
fn strip_comments(source: &str) -> (String, bool) {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    loop {
        let Some(i) = rest.find('/') else {
            out.push_str(rest);
            return (out, false);
        };
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("//") && !tail.starts_with("//!") {
            match tail.find('\n') {
                Some(j) => rest = &tail[j..], // keep the newline
                None => return (out, false),
            }
        } else if tail.starts_with("/*") {
            match tail[2..].find("*/") {
                Some(j) => rest = &tail[2 + j + 2..],
                None => return (out, true),
            }
        } else {
            out.push('/');
            rest = &tail[1..];
        }
    }
}

static DIRECTIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*//![ \t]*([A-Za-z_][A-Za-z0-9_]*)[ \t]*(.*?)[ \t]*$").unwrap());
static DECL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)^\s*(\w+)\s+(\w+)\s*;\s*$").unwrap());
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

const BLOCK_KINDS: [&str; 5] = ["PARAMETER", "TEXTURE", "SAMPLER", "COMMON", "PASS"];

/// `(KEY, value)` of a directive line.
fn directive(line: &str) -> Option<(String, String)> {
    let caps = DIRECTIVE_RE.captures(line)?;
    Some((caps[1].to_uppercase(), caps[2].to_owned()))
}

/// Split the comment-free source into `(kind, lines)` blocks; the first one
/// is the header.
fn split_blocks(source: &str) -> Vec<(String, Vec<&str>)> {
    let mut blocks: Vec<(String, Vec<&str>)> = vec![("HEADER".to_owned(), Vec::new())];
    for line in source.split('\n') {
        match directive(line) {
            Some((key, _)) if BLOCK_KINDS.contains(&key.as_str()) => blocks.push((key, vec![line])),
            _ => blocks.last_mut().expect("header block").1.push(line),
        }
    }
    blocks
}

/// The leading run of `//!` lines (blank lines allowed between them) and the
/// code after it.
fn take_directives(lines: &[&str]) -> (Vec<(String, String)>, String) {
    let mut directives = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let Some(d) = directive(line) else { break };
        directives.push(d);
        i += 1;
    }
    (directives, lines[i..].join("\n"))
}

/// Builds `MagpieError::Parse` for one effect.
struct Ctx<'a> {
    effect: &'a str,
}

impl Ctx<'_> {
    fn err(&self, message: impl Into<String>) -> MagpieError {
        MagpieError::Parse { effect: self.effect.to_owned(), message: message.into() }
    }

    fn number(&self, text: &str, kind: ParamKind, what: &str) -> Result<f64> {
        let text = text.trim();
        let parsed = match kind {
            ParamKind::Int => text.parse::<i64>().ok().map(|v| v as f64),
            ParamKind::Float => text.trim_end_matches(['f', 'F']).parse::<f64>().ok(),
        };
        parsed.ok_or_else(|| self.err(format!("{what}: invalid {} value '{text}'", kind.hlsl())))
    }

    fn ints(&self, value: &str, what: &str) -> Result<Vec<i64>> {
        value.split(',').map(|s| s.trim().parse::<i64>().map_err(|_| self.err(what.to_owned()))).collect()
    }
}

fn resolve_header(ctx: &Ctx, desc: &mut EffectDesc, lines: &[&str]) -> Result<()> {
    let (directives, code) = take_directives(lines);
    if directives.first().map(|(k, v)| (k.as_str(), v.as_str())) != Some(("MAGPIE", "EFFECT")) {
        return Err(ctx.err("Missing '//!MAGPIE EFFECT' header"));
    }
    let mut version_seen = false;
    for (key, value) in &directives[1..] {
        match key.as_str() {
            "VERSION" => {
                if value.trim() != MAGPIE_FX_VERSION.to_string() {
                    return Err(ctx.err(format!(
                        "Unsupported MagpieFX version '{value}' (only {MAGPIE_FX_VERSION} is supported)"
                    )));
                }
                version_seen = true;
            }
            "SORT_NAME" => desc.sort_name = value.clone(),
            "USE" => {
                for flag in value.split(',').map(|f| f.trim().to_uppercase()) {
                    match flag.as_str() {
                        "MULADD" => desc.use_muladd = true,
                        "_DYNAMIC" => desc.use_dynamic = true,
                        "" => {}
                        _ => desc.warnings.push(format!("Unknown USE flag '{flag}'")),
                    }
                }
            }
            "CAPABILITY" => {
                for flag in value.split(',').map(|f| f.trim().to_uppercase()) {
                    match flag.as_str() {
                        "FP16" => desc.supports_fp16 = true,
                        "" => {}
                        _ => desc.warnings.push(format!("Unknown CAPABILITY flag '{flag}'")),
                    }
                }
            }
            _ => desc.warnings.push(format!("Unknown header directive '{key}'")),
        }
    }
    if !version_seen {
        return Err(ctx.err("Missing '//!VERSION' directive"));
    }
    // The header may only contain an #include line
    let code = code.trim();
    if !code.is_empty() {
        if code.starts_with("#include") && !code.contains('\n') {
            desc.header_code = code.to_owned();
        } else {
            return Err(ctx.err("Unexpected code in the header block (only #include is allowed)"));
        }
    }
    Ok(())
}

fn resolve_parameter(ctx: &Ctx, desc: &mut EffectDesc, lines: &[&str], idx: usize) -> Result<()> {
    let (directives, code) = take_directives(lines);
    let mut opts: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in &directives[1..] {
        if opts.contains_key(key) {
            return Err(ctx.err(format!("Parameter #{idx}: duplicated '{key}'")));
        }
        if ["DEFAULT", "LABEL", "MIN", "MAX", "STEP"].contains(&key.as_str()) {
            opts.insert(key.clone(), value.clone());
        } else {
            desc.warnings.push(format!("Parameter #{idx}: unknown directive '{key}'"));
        }
    }
    for req in ["DEFAULT", "MIN", "MAX", "STEP"] {
        if !opts.contains_key(req) {
            return Err(ctx.err(format!("Parameter #{idx}: missing '//!{req}'")));
        }
    }
    let decl = DECL_RE.captures(&code).filter(|c| &c[1] == "float" || &c[1] == "int").ok_or_else(|| {
        ctx.err(format!("Parameter #{idx}: expected 'float name;' or 'int name;', got '{}'", code.trim()))
    })?;
    let kind = if &decl[1] == "int" { ParamKind::Int } else { ParamKind::Float };
    let name = decl[2].to_owned();
    let what = format!("Parameter '{name}'");
    let p = EffectParameter {
        default: ctx.number(&opts["DEFAULT"], kind, &what)?,
        min: ctx.number(&opts["MIN"], kind, &what)?,
        max: ctx.number(&opts["MAX"], kind, &what)?,
        step: ctx.number(&opts["STEP"], kind, &what)?,
        label: opts.get("LABEL").cloned().unwrap_or_default(),
        name,
        kind,
    };
    if !(p.min <= p.default && p.default <= p.max) {
        return Err(ctx.err(format!("{what}: default outside [min, max]")));
    }
    desc.params.push(p);
    Ok(())
}

fn resolve_texture(ctx: &Ctx, desc: &mut EffectDesc, lines: &[&str], idx: usize) -> Result<()> {
    let (directives, code) = take_directives(lines);
    let mut tex = EffectTexture { format: "R8G8B8A8_UNORM".to_owned(), ..EffectTexture::default() };
    let mut seen = BTreeSet::new();
    for (key, value) in &directives[1..] {
        if !seen.insert(key.clone()) {
            return Err(ctx.err(format!("Texture #{idx}: duplicated '{key}'")));
        }
        match key.as_str() {
            "SOURCE" => tex.source = value.clone(),
            "FORMAT" => {
                if crate::formats::lookup(value).is_none() {
                    return Err(ctx.err(format!("Texture #{idx}: unknown format '{value}'")));
                }
                tex.format = value.clone();
            }
            "WIDTH" => tex.width_expr = WS_RE.replace_all(value, "").into_owned(),
            "HEIGHT" => tex.height_expr = WS_RE.replace_all(value, "").into_owned(),
            _ => desc.warnings.push(format!("Texture #{idx}: unknown directive '{key}'")),
        }
    }
    if seen.contains("WIDTH") != seen.contains("HEIGHT") {
        return Err(ctx.err(format!("Texture #{idx}: WIDTH and HEIGHT must come in pairs")));
    }
    if seen.contains("SOURCE") && seen.contains("WIDTH") {
        return Err(ctx.err(format!("Texture #{idx}: SOURCE excludes WIDTH/HEIGHT")));
    }
    let decl = DECL_RE
        .captures(&code)
        .filter(|c| &c[1] == "Texture2D")
        .ok_or_else(|| ctx.err(format!("Texture #{idx}: expected 'Texture2D name;', got '{}'", code.trim())))?;
    let name = decl[2].to_owned();

    if name == "INPUT" {
        if !seen.is_empty() {
            return Err(ctx.err("INPUT texture cannot have options"));
        }
        return Ok(());
    }
    if name == "OUTPUT" {
        if seen.contains("SOURCE") || seen.contains("FORMAT") {
            return Err(ctx.err("OUTPUT texture only accepts WIDTH/HEIGHT"));
        }
        desc.textures[1].width_expr = tex.width_expr;
        desc.textures[1].height_expr = tex.height_expr;
        return Ok(());
    }
    if !seen.contains("FORMAT") {
        if !seen.contains("SOURCE") {
            return Err(ctx.err(format!("Texture '{name}': missing '//!FORMAT'")));
        }
        // Taken from the file when the texture is loaded (Magpie's UNKNOWN)
        tex.format = String::new();
    }
    tex.name = name;
    desc.textures.push(tex);
    Ok(())
}

fn resolve_sampler(ctx: &Ctx, desc: &mut EffectDesc, lines: &[&str], idx: usize) -> Result<()> {
    let (directives, code) = take_directives(lines);
    let mut sam = EffectSampler { name: String::new(), filter: "POINT".to_owned(), address: "CLAMP".to_owned() };
    let mut seen = BTreeSet::new();
    for (key, value) in &directives[1..] {
        if !seen.insert(key.clone()) {
            return Err(ctx.err(format!("Sampler #{idx}: duplicated '{key}'")));
        }
        let value = value.to_uppercase();
        match key.as_str() {
            "FILTER" => {
                if value != "POINT" && value != "LINEAR" {
                    return Err(ctx.err(format!("Sampler #{idx}: invalid FILTER '{value}'")));
                }
                sam.filter = value;
            }
            "ADDRESS" => {
                if value != "CLAMP" && value != "WRAP" {
                    return Err(ctx.err(format!("Sampler #{idx}: invalid ADDRESS '{value}'")));
                }
                sam.address = value;
            }
            _ => desc.warnings.push(format!("Sampler #{idx}: unknown directive '{key}'")),
        }
    }
    if !seen.contains("FILTER") {
        return Err(ctx.err(format!("Sampler #{idx}: missing '//!FILTER'")));
    }
    let decl = DECL_RE
        .captures(&code)
        .filter(|c| &c[1] == "SamplerState")
        .ok_or_else(|| ctx.err(format!("Sampler #{idx}: expected 'SamplerState name;', got '{}'", code.trim())))?;
    sam.name = decl[2].to_owned();
    desc.samplers.push(sam);
    Ok(())
}

fn resolve_common(ctx: &Ctx, desc: &mut EffectDesc, lines: &[&str], idx: usize) -> Result<()> {
    let (directives, code) = take_directives(lines);
    if directives.len() > 1 {
        return Err(ctx.err(format!("Common #{idx}: COMMON blocks take no options")));
    }
    desc.commons.push(code);
    Ok(())
}

fn resolve_passes(ctx: &Ctx, desc: &mut EffectDesc, blocks: &[Vec<&str>]) -> Result<()> {
    let mut parsed: Vec<(u32, Vec<(String, String)>, String)> = Vec::new();
    for lines in blocks {
        let (mut directives, code) = take_directives(lines);
        let first = directives.remove(0);
        let number = first
            .1
            .trim()
            .parse::<u32>()
            .map_err(|_| ctx.err(format!("Invalid pass number '{}'", first.1)))?;
        parsed.push((number, directives, code));
    }
    parsed.sort_by_key(|p| p.0);
    if parsed.iter().enumerate().any(|(i, p)| p.0 as usize != i + 1) {
        return Err(ctx.err("PASS numbers must be consecutive starting at 1"));
    }

    let tex_index: BTreeMap<String, u32> =
        desc.textures.iter().enumerate().map(|(i, t)| (t.name.clone(), i as u32)).collect();
    let n_passes = parsed.len();

    for (i, (number, directives, code)) in parsed.into_iter().enumerate() {
        let mut p = EffectPass { index: number, num_threads: (1, 1, 1), code, ..EffectPass::default() };
        let mut seen = BTreeSet::new();
        let mut available = tex_index.clone();
        for (key, value) in &directives {
            if !seen.insert(key.clone()) {
                return Err(ctx.err(format!("Pass {number}: duplicated '{key}'")));
            }
            match key.as_str() {
                "IN" => {
                    for name in value.split(',').map(str::trim) {
                        match available.remove(name) {
                            Some(j) if j != 1 => p.inputs.push(j),
                            _ => return Err(ctx.err(format!("Pass {number}: invalid input '{name}'"))),
                        }
                    }
                }
                "OUT" => {
                    let names: Vec<&str> = value.split(',').map(str::trim).collect();
                    if i == n_passes - 1 {
                        if names != ["OUTPUT"] {
                            return Err(ctx.err(format!("Pass {number}: the last pass must write only OUTPUT")));
                        }
                        p.outputs.push(1);
                        continue;
                    }
                    if names.len() > 8 {
                        return Err(ctx.err(format!("Pass {number}: at most 8 outputs")));
                    }
                    for name in names {
                        match available.remove(name) {
                            Some(j) if j > 1 && desc.textures[j as usize].source.is_empty() => p.outputs.push(j),
                            _ => return Err(ctx.err(format!("Pass {number}: invalid output '{name}'"))),
                        }
                    }
                }
                "BLOCK_SIZE" => {
                    let nums = ctx.ints(value, &format!("Pass {number}: invalid BLOCK_SIZE"))?;
                    if !(1..=2).contains(&nums.len()) || nums.iter().any(|n| *n <= 0) {
                        return Err(ctx.err(format!("Pass {number}: invalid BLOCK_SIZE")));
                    }
                    p.block_size = (nums[0] as u32, *nums.last().unwrap() as u32);
                }
                "NUM_THREADS" => {
                    let mut nums = ctx.ints(value, &format!("Pass {number}: invalid NUM_THREADS"))?;
                    if !(1..=3).contains(&nums.len()) || nums.iter().any(|n| *n <= 0) {
                        return Err(ctx.err(format!("Pass {number}: invalid NUM_THREADS")));
                    }
                    nums.resize(3, 1);
                    p.num_threads = (nums[0] as u32, nums[1] as u32, nums[2] as u32);
                }
                "STYLE" => match value.as_str() {
                    "PS" => {
                        p.ps_style = true;
                        p.block_size = (16, 16);
                        p.num_threads = (64, 1, 1);
                    }
                    "CS" => {}
                    _ => return Err(ctx.err(format!("Pass {number}: invalid STYLE '{value}'"))),
                },
                "DESC" => p.desc = value.clone(),
                _ => desc.warnings.push(format!("Pass {number}: unknown directive '{key}'")),
            }
        }
        if !seen.contains("IN") || !seen.contains("OUT") {
            return Err(ctx.err(format!("Pass {number}: IN and OUT are required")));
        }
        let explicit = seen.contains("BLOCK_SIZE") || seen.contains("NUM_THREADS");
        if p.ps_style && explicit {
            return Err(ctx.err(format!("Pass {number}: PS style passes cannot set BLOCK_SIZE/NUM_THREADS")));
        }
        if !p.ps_style && !(seen.contains("BLOCK_SIZE") && seen.contains("NUM_THREADS")) {
            return Err(ctx.err(format!("Pass {number}: CS style passes need BLOCK_SIZE and NUM_THREADS")));
        }
        if p.desc.is_empty() {
            p.desc = format!("Pass {number}");
        }
        desc.passes.push(p);
    }
    Ok(())
}

/// Parse an effect.
pub fn parse_effect(source: &str, name: &str) -> Result<EffectDesc> {
    let ctx = Ctx { effect: name };
    let (source, unterminated) = strip_comments(&source.replace("\r\n", "\n"));
    if unterminated {
        return Err(ctx.err("Unterminated block comment"));
    }
    let mut desc = EffectDesc {
        name: name.to_owned(),
        textures: vec![
            EffectTexture {
                name: "INPUT".to_owned(),
                format: "R8G8B8A8_UNORM".to_owned(),
                width_expr: "INPUT_WIDTH".to_owned(),
                height_expr: "INPUT_HEIGHT".to_owned(),
                source: String::new(),
            },
            EffectTexture { name: "OUTPUT".to_owned(), format: "R8G8B8A8_UNORM".to_owned(), ..EffectTexture::default() },
        ],
        ..EffectDesc::default()
    };

    let blocks = split_blocks(&source);
    resolve_header(&ctx, &mut desc, &blocks[0].1)?;

    let mut counters: BTreeMap<&str, usize> = BTreeMap::new();
    let mut pass_blocks: Vec<Vec<&str>> = Vec::new();
    for (kind, lines) in &blocks[1..] {
        let idx = counters.entry(kind.as_str()).or_insert(0);
        *idx += 1;
        let idx = *idx;
        match kind.as_str() {
            "PARAMETER" => resolve_parameter(&ctx, &mut desc, lines, idx)?,
            "TEXTURE" => resolve_texture(&ctx, &mut desc, lines, idx)?,
            "SAMPLER" => resolve_sampler(&ctx, &mut desc, lines, idx)?,
            "COMMON" => resolve_common(&ctx, &mut desc, lines, idx)?,
            _ => pass_blocks.push(lines.clone()),
        }
    }
    if pass_blocks.is_empty() {
        return Err(ctx.err("Effect has no PASS block"));
    }

    let mut names: Vec<&str> = desc.params.iter().map(|p| p.name.as_str()).collect();
    names.extend(desc.textures.iter().map(|t| t.name.as_str()));
    names.extend(desc.samplers.iter().map(|s| s.name.as_str()));
    let mut once = BTreeSet::new();
    let dupes: BTreeSet<&str> = names.iter().copied().filter(|n| !once.insert(*n)).collect();
    if !dupes.is_empty() {
        let list: Vec<&str> = dupes.into_iter().collect();
        return Err(ctx.err(format!("Duplicated identifiers: {}", list.join(", "))));
    }

    resolve_passes(&ctx, &mut desc, &pass_blocks)?;
    Ok(desc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EFFECT: &str = "\
//!MAGPIE EFFECT
//!VERSION 4
//!SORT_NAME Test effect
//!USE MulAdd, _Dynamic
//!CAPABILITY FP16

//!PARAMETER
//!LABEL Strength
//!DEFAULT 0.5
//!MIN 0
//!MAX 1.0f
//!STEP 0.01
float strength;

//!PARAMETER
//!DEFAULT 2
//!MIN 1
//!MAX 4
//!STEP 1
int taps;

//!TEXTURE
Texture2D INPUT;

//!TEXTURE
//!WIDTH INPUT_WIDTH * 2
//!HEIGHT INPUT_HEIGHT * 2
Texture2D OUTPUT;

//!TEXTURE
//!FORMAT R16G16B16A16_FLOAT
Texture2D tex1;

//!SAMPLER
//!FILTER LINEAR
//!ADDRESS wrap
SamplerState sam;

//!COMMON
float twice(float x) { return x * 2; } // a comment

/* a block
   comment */
//!PASS 2
//!STYLE PS
//!IN INPUT, tex1
//!OUT OUTPUT
MF4 Pass2(float2 pos) { return INPUT.SampleLevel(sam, pos, 0) + tex1.SampleLevel(sam, pos, 0); }

//!PASS 1
//!DESC First
//!IN INPUT
//!OUT tex1
//!BLOCK_SIZE 8
//!NUM_THREADS 64
void Pass1(uint2 blockStart, uint3 tid) { }
";

    fn err(source: &str) -> String {
        match parse_effect(source, "t") {
            Err(MagpieError::Parse { message, .. }) => message,
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    /// A minimal effect with `extra` blocks before one PS pass.
    fn with(extra: &str) -> String {
        format!("//!MAGPIE EFFECT\n//!VERSION 4\n{extra}\n//!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT OUTPUT\nMF4 Pass1(float2 pos) {{ return 0; }}\n")
    }

    #[test]
    fn parses_a_full_effect() {
        let d = parse_effect(EFFECT, "Test").unwrap();
        assert_eq!(d.name, "Test");
        assert_eq!(d.sort_name, "Test effect");
        assert!(d.use_muladd && d.use_dynamic && d.supports_fp16);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);

        assert_eq!(d.params.len(), 2);
        let s = d.param("strength").unwrap();
        assert_eq!((s.kind, s.default, s.min, s.max, s.step), (ParamKind::Float, 0.5, 0.0, 1.0, 0.01));
        assert_eq!(s.label, "Strength");
        let t = d.param("taps").unwrap();
        assert_eq!((t.kind, t.default, t.label.as_str()), (ParamKind::Int, 2.0, ""));

        let names: Vec<_> = d.textures.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["INPUT", "OUTPUT", "tex1"]);
        assert_eq!(d.output().width_expr, "INPUT_WIDTH*2");
        assert_eq!(d.output().height_expr, "INPUT_HEIGHT*2");
        assert_eq!(d.textures[2].format, "R16G16B16A16_FLOAT");

        assert_eq!(d.samplers, vec![EffectSampler { name: "sam".into(), filter: "LINEAR".into(), address: "WRAP".into() }]);
        assert_eq!(d.commons.len(), 1);
        assert!(!d.commons[0].contains("a comment"));

        // passes are sorted by number
        assert_eq!(d.passes.len(), 2);
        let p1 = &d.passes[0];
        assert_eq!((p1.index, p1.desc.as_str()), (1, "First"));
        assert_eq!((p1.inputs.clone(), p1.outputs.clone()), (vec![0], vec![2]));
        assert_eq!((p1.block_size, p1.num_threads, p1.ps_style), ((8, 8), (64, 1, 1), false));
        let p2 = &d.passes[1];
        assert_eq!((p2.inputs.clone(), p2.outputs.clone()), (vec![0, 2], vec![1]));
        assert_eq!((p2.block_size, p2.num_threads, p2.ps_style), ((16, 16), (64, 1, 1), true));
        assert_eq!(p2.desc, "Pass 2");
        assert!(p2.code.contains("MF4 Pass2"));
    }

    #[test]
    fn comments_keep_lines_and_directives() {
        let src = "a // x\n//!PASS 1\nb /* y\nz */ c\n/ d";
        assert_eq!(remove_comments(src), "a \n//!PASS 1\nb  c\n/ d");
        assert_eq!(remove_comments("x /* open"), "x ");
        assert_eq!(remove_comments("x // end"), "x ");
    }

    #[test]
    fn crlf_and_header_include() {
        let src = with("#include \"common.hlsli\"").replace('\n', "\r\n");
        let d = parse_effect(&src, "t").unwrap();
        assert_eq!(d.header_code, "#include \"common.hlsli\"");
    }

    #[test]
    fn header_errors() {
        assert!(err("//!VERSION 4\n").contains("MAGPIE EFFECT"));
        assert!(err("//!MAGPIE EFFECT\n//!PASS 1\n").contains("VERSION"));
        assert!(err("//!MAGPIE EFFECT\n//!VERSION 3\n").contains("Unsupported MagpieFX version"));
        assert!(err("//!MAGPIE EFFECT\n//!VERSION 4\n").contains("no PASS"));
        assert!(err(&with("float x;")).contains("header block"));
        assert!(err("x /* open\n//!MAGPIE EFFECT").contains("Unterminated"));
    }

    #[test]
    fn unknown_directives_are_warnings() {
        let d = parse_effect(&with("//!FOO bar\n//!USE what\n//!CAPABILITY nope"), "t").unwrap();
        assert_eq!(d.warnings.len(), 3);
    }

    #[test]
    fn parameter_errors() {
        let p = |body: &str| with(&format!("//!PARAMETER\n{body}"));
        assert!(err(&p("//!MIN 0\n//!MAX 1\n//!STEP 1\nfloat a;")).contains("missing '//!DEFAULT'"));
        assert!(err(&p("//!DEFAULT 2\n//!MIN 0\n//!MAX 1\n//!STEP 1\nfloat a;")).contains("outside"));
        assert!(err(&p("//!DEFAULT 1.5\n//!MIN 0\n//!MAX 2\n//!STEP 1\nint a;")).contains("invalid int"));
        assert!(err(&p("//!DEFAULT 0\n//!DEFAULT 0\n//!MIN 0\n//!MAX 1\n//!STEP 1\nfloat a;")).contains("duplicated"));
        assert!(err(&p("//!DEFAULT 0\n//!MIN 0\n//!MAX 1\n//!STEP 1\nfloat2 a;")).contains("expected 'float name;'"));
    }

    #[test]
    fn texture_errors() {
        let t = |body: &str| with(&format!("//!TEXTURE\n{body}"));
        assert!(err(&t("//!FORMAT NOPE\nTexture2D a;")).contains("unknown format"));
        assert!(err(&t("Texture2D a;")).contains("missing '//!FORMAT'"));
        assert!(err(&t("//!FORMAT R8_UNORM\n//!WIDTH 1\nTexture2D a;")).contains("pairs"));
        assert!(err(&t("//!SOURCE a.dds\n//!WIDTH 1\n//!HEIGHT 1\nTexture2D a;")).contains("excludes"));
        assert!(err(&t("//!FORMAT R8_UNORM\nTexture2D INPUT;")).contains("INPUT texture"));
        assert!(err(&t("//!FORMAT R8_UNORM\nTexture2D OUTPUT;")).contains("OUTPUT texture"));
        // a SOURCE without FORMAT takes the file's format later
        let d = parse_effect(&t("//!SOURCE lut.dds\nTexture2D lut;"), "t").unwrap();
        assert_eq!((d.textures[2].format.as_str(), d.textures[2].source.as_str()), ("", "lut.dds"));
    }

    #[test]
    fn sampler_errors() {
        let s = |body: &str| with(&format!("//!SAMPLER\n{body}"));
        assert!(err(&s("SamplerState s;")).contains("missing '//!FILTER'"));
        assert!(err(&s("//!FILTER CUBIC\nSamplerState s;")).contains("invalid FILTER"));
        assert!(err(&s("//!FILTER POINT\n//!ADDRESS MIRROR\nSamplerState s;")).contains("invalid ADDRESS"));
        let d = parse_effect(&s("//!FILTER point\nSamplerState s;"), "t").unwrap();
        assert_eq!((d.samplers[0].filter.as_str(), d.samplers[0].address.as_str()), ("POINT", "CLAMP"));
    }

    #[test]
    fn duplicate_identifiers() {
        let src = with("//!TEXTURE\n//!FORMAT R8_UNORM\nTexture2D a;\n//!SAMPLER\n//!FILTER POINT\nSamplerState a;");
        assert!(err(&src).contains("Duplicated identifiers: a"));
    }

    #[test]
    fn pass_errors() {
        let head = "//!MAGPIE EFFECT\n//!VERSION 4\n//!TEXTURE\n//!FORMAT R8_UNORM\nTexture2D t;\n";
        let e = |passes: &str| err(&format!("{head}{passes}"));
        assert!(e("//!PASS 2\n//!STYLE PS\n//!IN INPUT\n//!OUT OUTPUT\n").contains("consecutive"));
        assert!(e("//!PASS x\n").contains("Invalid pass number"));
        assert!(e("//!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT t\n").contains("only OUTPUT"));
        assert!(e("//!PASS 1\n//!STYLE PS\n//!IN OUTPUT\n//!OUT OUTPUT\n").contains("invalid input"));
        assert!(e("//!PASS 1\n//!STYLE PS\n//!IN INPUT\n").contains("IN and OUT"));
        assert!(e("//!PASS 1\n//!STYLE XS\n//!IN INPUT\n//!OUT OUTPUT\n").contains("invalid STYLE"));
        assert!(e("//!PASS 1\n//!IN INPUT\n//!OUT OUTPUT\n").contains("need BLOCK_SIZE"));
        assert!(e("//!PASS 1\n//!STYLE PS\n//!BLOCK_SIZE 8\n//!IN INPUT\n//!OUT OUTPUT\n").contains("cannot set"));
        assert!(
            e("//!PASS 1\n//!STYLE PS\n//!IN INPUT\n//!OUT INPUT\n//!PASS 2\n//!STYLE PS\n//!IN t\n//!OUT OUTPUT\n")
                .contains("invalid output")
        );
        assert!(
            e("//!PASS 1\n//!BLOCK_SIZE 0\n//!NUM_THREADS 1\n//!IN INPUT\n//!OUT OUTPUT\n").contains("BLOCK_SIZE")
        );
        // the same texture cannot be read and written by one pass
        assert!(
            e("//!PASS 1\n//!STYLE PS\n//!IN t\n//!OUT t\n//!PASS 2\n//!STYLE PS\n//!IN t\n//!OUT OUTPUT\n")
                .contains("invalid output 't'")
        );
    }

    #[test]
    fn bundled_effects_parse() {
        let root = crate::catalog::builtin_dir();
        let mut count = 0;
        for entry in crate::catalog::Catalog::scan(None).entries {
            let source = std::fs::read_to_string(&entry.path).unwrap();
            let desc = parse_effect(source.trim_start_matches('\u{feff}'), &entry.name)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
            assert!(!desc.passes.is_empty());
            count += 1;
        }
        assert_eq!(count, 117, "effects under {}", root.display());
    }
}
