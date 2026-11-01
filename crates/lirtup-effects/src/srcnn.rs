//! SRCNN with the CuNNy models (`srcnn/`): 2x per run.
//!
//! Full frame: every pass reads and writes textures at the input size (the
//! names of `model.json`), the last one writes the 2x output; all passes run
//! with the same `ceil(w / 8) x ceil(h / 8)` groups (the last one writes 2x2
//! pixels per thread).
//!
//! Tiles ([`Srcnn::new_tiled`]): the input is a texture array with one
//! expanded tile (`tile_size + 2 * margin`) per layer; the intermediate
//! passes work on arrays of the same size and the last pass writes the
//! interior of each tile straight into a full-size 2D output, reading the
//! residual (`input` of the last pass) from a full-frame texture. Each tile is
//! one push constant block (`TileParams` of the `_tile` shaders, 1.x
//! `TileProcessor`).

use std::collections::BTreeMap;
use std::sync::Arc;

use lirtup_config::Model;
use lirtup_gpu::{CommandRecorder, ComputePipeline, Device, Dispatch, Filter, Format, Sampler, Texture};
use serde::Deserialize;

use crate::util::{self, Bindings, CbWriter};
use crate::{EffectError, Pass, Result};

/// Tile mode: the input is a texture array with one tile per layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileLayout {
    /// Tile interior size.
    pub tile_size: u32,
    /// Context margin around each tile.
    pub margin: u32,
    /// Layers (maximum tiles per frame).
    pub layers: u32,
}

/// Options of a SRCNN pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SrcnnOptions {
    /// Model; must not be [`Model::None`].
    pub model: Model,
    /// Tiles instead of the full frame.
    pub tiles: Option<TileLayout>,
    /// Use the FP16 variants if the device has `shaderFloat16`.
    pub fp16: bool,
}

/// Where one tile of a tiled run goes in the full output (1.x `TileSpec`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TilePlacement {
    /// Top-left of the output rectangle, in output pixels.
    pub dst_x: u32,
    /// Top-left of the output rectangle, in output pixels.
    pub dst_y: u32,
    /// Width written (clipped at the right edge).
    pub width: u32,
    /// Height written (clipped at the bottom edge).
    pub height: u32,
}

impl TilePlacement {
    /// The placement of tile `(tx, ty)` for a `scale`x run (`TileSpec.from_raw`).
    pub fn from_grid(tx: u32, ty: u32, tile_size: u32, scale: u32, full_out: (u32, u32)) -> TilePlacement {
        let dst_x = tx * tile_size * scale;
        let dst_y = ty * tile_size * scale;
        TilePlacement {
            dst_x,
            dst_y,
            width: (tile_size * scale).min(full_out.0.saturating_sub(dst_x)),
            height: (tile_size * scale).min(full_out.1.saturating_sub(dst_y)),
        }
    }
}

/// `model.json`.
#[derive(Debug, Deserialize)]
struct ModelJson {
    #[serde(default)]
    name: String,
    #[serde(default = "default_scale")]
    scale: u32,
    #[serde(default = "default_depth")]
    depth: String,
    passes: usize,
    #[allow(dead_code)]
    num_textures: usize,
    srv_uav: Vec<(Vec<String>, Vec<String>)>,
    samplers: Vec<Vec<String>>,
}

fn default_scale() -> u32 {
    2
}

fn default_depth() -> String {
    "rgba8".to_owned()
}

impl ModelJson {
    fn load(model: Model, json: &str) -> Result<ModelJson> {
        let asset = |message: String| EffectError::Asset { name: format!("{model:?}/model.json"), message };
        let cfg: ModelJson = serde_json::from_str(json).map_err(|e| asset(e.to_string()))?;
        if cfg.srv_uav.len() != cfg.passes {
            return Err(asset(format!("srv_uav must have {} entries", cfg.passes)));
        }
        if cfg.samplers.len() != cfg.passes {
            return Err(asset(format!("samplers must have {} entries", cfg.passes)));
        }
        if cfg.scale != 2 {
            return Err(asset(format!("unsupported scale {}", cfg.scale)));
        }
        Ok(cfg)
    }

    fn intermediate_format(&self) -> Result<Format> {
        match self.depth.as_str() {
            "rgba8" => Ok(Format::R8G8B8A8Unorm),
            "rgba16" => Ok(Format::R16G16B16A16Float),
            other => Err(EffectError::Asset {
                name: format!("{}/model.json", self.name),
                message: format!("unsupported depth '{other}' (rgba8 or rgba16)"),
            }),
        }
    }

    /// Every texture name written, except `output`, in first-use order
    /// (`collect_uav_names`).
    fn intermediate_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (_, uavs) in &self.srv_uav {
            for name in uavs {
                if name != "output" && !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }
}

/// Size of the `TileParams` push constant block (8 `uint`).
const TILE_PUSH_SIZE: u32 = 32;

/// A CuNNy model bound to an input: output is twice the input size (or, in
/// tile mode, twice each tile without its margin).
pub struct Srcnn {
    label: String,
    pipelines: Vec<ComputePipeline>,
    groups: [u32; 3],
    output: Texture,
    tiles: Option<TileState>,
    _intermediates: Vec<Texture>,
}

struct TileState {
    margin: u32,
    full_out: (u32, u32),
    layers: u32,
    placements: Vec<TilePlacement>,
    active: u32,
}

impl Srcnn {
    /// Load the model and create its passes.
    ///
    /// Tile mode needs the residual and the full output size, which these
    /// options do not carry: with `tiles: Some(..)` use [`Srcnn::new_tiled`].
    pub fn new(device: &Arc<Device>, input: &Texture, options: &SrcnnOptions) -> Result<Srcnn> {
        if options.tiles.is_some() {
            return Err(EffectError::Unsupported("tiled SRCNN is created with Srcnn::new_tiled".to_owned()));
        }
        let (assets, cfg) = load(options.model)?;
        let format = cfg.intermediate_format()?;
        let (w, h) = (input.width(), input.height());
        let label = model_label(options.model);

        let mut named: BTreeMap<String, Texture> = BTreeMap::new();
        for name in cfg.intermediate_names() {
            let tex = util::texture(device, w, h, format, &format!("{label} {name}"))?;
            named.insert(name, tex);
        }
        let output = util::texture(device, w * 2, h * 2, Format::R8G8B8A8Unorm, &format!("{label} output"))?;
        named.insert("output".to_owned(), output.clone());
        named.insert("input".to_owned(), input.clone());

        let samplers = Samplers::new(device)?;
        let mut pipelines = Vec::with_capacity(cfg.passes);
        for pass in 0..cfg.passes {
            let last = pass + 1 == cfg.passes;
            let out = if last { (w * 2, h * 2) } else { (w, h) };
            let cb = util::constant_buffer_with(device, cb_words((w, h), out), &format!("{label} cb{}", pass + 1))?;
            pipelines.push(build_pass(
                device,
                &cfg,
                pass,
                assets.passes[pass],
                &cb,
                &named,
                &samplers,
                0,
                &format!("{label} pass {}", pass + 1),
            )?);
        }

        Ok(Srcnn {
            label,
            pipelines,
            groups: util::groups((w, h), (8, 8)),
            output,
            tiles: None,
            _intermediates: named.into_values().collect(),
        })
    }

    /// Tile mode. `input` is the array of expanded tiles (one per layer, its
    /// width is the feature size the shaders see), `residual` the full-frame
    /// texture the last pass adds (the 1x frame for 2x, the first stage's
    /// output for 4x), and `output_size` the full output. `layout.margin` is
    /// the margin in the feature space of this stage (doubled for the second
    /// stage of 4x, as 1.x `_shader_margin`).
    pub fn new_tiled(
        device: &Arc<Device>,
        input: &Texture,
        residual: &Texture,
        output_size: (u32, u32),
        layout: &TileLayout,
        options: &SrcnnOptions,
    ) -> Result<Srcnn> {
        let (assets, cfg) = load(options.model)?;
        let format = cfg.intermediate_format()?;
        let feat = input.width();
        let layers = layout.layers.max(1);
        let label = format!("{} tiles", model_label(options.model));

        let mut named: BTreeMap<String, Texture> = BTreeMap::new();
        for name in cfg.intermediate_names() {
            let tex = util::texture_array(device, feat, feat, layers, true, format, &format!("{label} {name}"))?;
            named.insert(name, tex);
        }
        named.insert("input".to_owned(), input.clone());
        let output = util::texture(device, output_size.0, output_size.1, Format::R8G8B8A8Unorm, &format!("{label} output"))?;

        let samplers = Samplers::new(device)?;
        let mut pipelines = Vec::with_capacity(cfg.passes);
        for pass in 0..cfg.passes {
            let last = pass + 1 == cfg.passes;
            let pass_label = format!("{label} pass {}", pass + 1);
            if last {
                // The final pass writes into the full output and reads the
                // residual as `input` (1.x `_finalize_pipeline`).
                let cb = util::constant_buffer_with(device, cb_words((feat, feat), output_size), &pass_label)?;
                let mut last_named = named.clone();
                last_named.insert("input".to_owned(), residual.clone());
                last_named.insert("output".to_owned(), output.clone());
                pipelines.push(build_pass(
                    device,
                    &cfg,
                    pass,
                    assets.tile_passes[pass],
                    &cb,
                    &last_named,
                    &samplers,
                    TILE_PUSH_SIZE,
                    &pass_label,
                )?);
            } else {
                let cb = util::constant_buffer_with(device, cb_words((feat, feat), (feat, feat)), &pass_label)?;
                pipelines.push(build_pass(
                    device,
                    &cfg,
                    pass,
                    assets.tile_passes[pass],
                    &cb,
                    &named,
                    &samplers,
                    TILE_PUSH_SIZE,
                    &pass_label,
                )?);
            }
        }

        Ok(Srcnn {
            label,
            pipelines,
            groups: util::groups((feat, feat), (8, 8)),
            output,
            tiles: Some(TileState { margin: layout.margin, full_out: output_size, layers, placements: Vec::new(), active: 0 }),
            _intermediates: named.into_values().collect(),
        })
    }

    /// Number of dispatches per frame.
    pub fn pass_count(&self) -> usize {
        match &self.tiles {
            None => self.pipelines.len(),
            Some(t) => self.pipelines.len() * t.active as usize,
        }
    }

    /// In tile mode, how many layers to run this frame (the rest are skipped).
    pub fn set_active_tiles(&mut self, count: u32) {
        if let Some(t) = &mut self.tiles {
            t.active = count.min(t.layers).min(t.placements.len() as u32);
        }
    }

    /// In tile mode, where the tile of each layer goes (layer `i` is
    /// `tiles[i]`); also makes all of them active. At most `layers` are kept.
    pub fn set_tiles(&mut self, tiles: &[TilePlacement]) {
        if let Some(t) = &mut self.tiles {
            t.placements = tiles.iter().take(t.layers as usize).copied().collect();
            t.active = t.placements.len() as u32;
        }
    }
}

impl Pass for Srcnn {
    fn label(&self) -> &str {
        &self.label
    }

    fn output(&self) -> &Texture {
        &self.output
    }

    fn record(&self, recorder: &mut CommandRecorder) -> Result<()> {
        match &self.tiles {
            None => {
                for pipeline in &self.pipelines {
                    recorder.dispatch(&Dispatch {
                        pipeline,
                        groups: self.groups,
                        push: &[],
                        timestamp: Some(&self.label),
                    });
                }
            }
            Some(t) => {
                for (layer, placement) in t.placements.iter().take(t.active as usize).enumerate() {
                    let push = tile_push(placement, t.full_out, layer as u32, t.margin);
                    for pipeline in &self.pipelines {
                        recorder.dispatch(&Dispatch {
                            pipeline,
                            groups: self.groups,
                            push: &push,
                            timestamp: Some(&self.label),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn model_label(model: Model) -> String {
    format!("srcnn {}", model.as_str())
}

fn load(model: Model) -> Result<(crate::assets::CunnyAssets, ModelJson)> {
    let assets = crate::assets::cunny_model(model)
        .ok_or_else(|| EffectError::Unsupported("SRCNN needs a model (not 'none')".to_owned()))?;
    let cfg = ModelJson::load(model, assets.json)?;
    if assets.passes.len() != cfg.passes || assets.tile_passes.len() != cfg.passes {
        return Err(EffectError::Asset {
            name: format!("{model:?}"),
            message: format!("model.json says {} passes, {} shaders embedded", cfg.passes, assets.passes.len()),
        });
    }
    Ok((assets, cfg))
}

/// The SRCNN constant buffer (`IIIIffff`).
fn cb_words(input: (u32, u32), output: (u32, u32)) -> CbWriter {
    CbWriter::new()
        .u32(input.0)
        .u32(input.1)
        .u32(output.0)
        .u32(output.1)
        .f32(1.0 / input.0 as f32)
        .f32(1.0 / input.1 as f32)
        .f32(1.0 / output.0 as f32)
        .f32(1.0 / output.1 as f32)
}

/// `TileParams` (`_make_push_bytes`).
fn tile_push(p: &TilePlacement, full_out: (u32, u32), layer: u32, margin: u32) -> [u8; 32] {
    let words = [p.dst_x, p.dst_y, p.width, p.height, full_out.0, full_out.1, layer, margin];
    let mut out = [0u8; 32];
    for (chunk, w) in out.chunks_exact_mut(4).zip(words) {
        chunk.copy_from_slice(&w.to_le_bytes());
    }
    out
}

struct Samplers {
    point: Sampler,
    linear: Sampler,
}

impl Samplers {
    fn new(device: &Arc<Device>) -> Result<Samplers> {
        Ok(Samplers { point: util::sampler(device, Filter::Point)?, linear: util::sampler(device, Filter::Linear)? })
    }

    fn get(&self, kind: &str) -> Result<&Sampler> {
        match kind {
            "point" => Ok(&self.point),
            "linear" => Ok(&self.linear),
            other => Err(EffectError::Asset { name: "model.json".to_owned(), message: format!("unknown sampler '{other}'") }),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_pass(
    device: &Arc<Device>,
    cfg: &ModelJson,
    pass: usize,
    spirv: &[u8],
    cb: &lirtup_gpu::Buffer,
    named: &BTreeMap<String, Texture>,
    samplers: &Samplers,
    push_size: u32,
    label: &str,
) -> Result<ComputePipeline> {
    let lookup = |name: &String| {
        named.get(name).ok_or_else(|| EffectError::Asset {
            name: format!("{}/model.json", cfg.name),
            message: format!("pass {}: unknown texture '{name}'", pass + 1),
        })
    };
    let (srv_names, uav_names) = &cfg.srv_uav[pass];
    let srv = srv_names.iter().map(lookup).collect::<Result<Vec<_>>>()?;
    let uav = uav_names.iter().map(lookup).collect::<Result<Vec<_>>>()?;
    let smp = cfg.samplers[pass].iter().map(|s| samplers.get(s)).collect::<Result<Vec<_>>>()?;
    util::compute(
        device,
        spirv,
        "main",
        &Bindings { cb: Some(cb), srv: &srv, uav: &uav, samplers: &smp, push_size },
        label,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_json_parses() {
        for model in Model::ALL.into_iter().filter(|m| *m != Model::None) {
            let (assets, cfg) = load(model).unwrap();
            assert_eq!(assets.passes.len(), cfg.passes);
            cfg.intermediate_format().unwrap();
            let names = cfg.intermediate_names();
            assert!(!names.is_empty());
            assert!(!names.contains(&"output".to_owned()));
            // the last pass writes only the output
            assert_eq!(cfg.srv_uav.last().unwrap().1, vec!["output".to_owned()]);
        }
    }

    #[test]
    fn tile_placement_clips_at_edges() {
        let p = TilePlacement::from_grid(3, 0, 64, 2, (400, 300));
        assert_eq!(p, TilePlacement { dst_x: 384, dst_y: 0, width: 16, height: 128 });
        let push = tile_push(&p, (400, 300), 5, 8);
        assert_eq!(&push[0..4], &384u32.to_le_bytes());
        assert_eq!(&push[24..28], &5u32.to_le_bytes());
        assert_eq!(&push[28..32], &8u32.to_le_bytes());
    }
}
