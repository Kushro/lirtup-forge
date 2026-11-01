//! The scaling pipeline of LirtUp Forge (`src/upscaler/pipeline/`).
//!
//! Modules are declared here once (by the reviewer), so the three round 2
//! fronts work in separate files and never edit this one:
//!
//! | Module | Front | Ports |
//! | --- | --- | --- |
//! | [`pipeline`] | P | `pipeline.py`, `session.py`, `controller.py`: the pipeline thread (capture → upload → chain → present), its commands and config swaps |
//! | [`tiles`] | P | `tile.py`, `utils.py`: tile processing on the GPU side |
//! | [`upscale`] | P | `upscale.py`: the `UpscalerManager` (models, double upscale, geometry) |
//! | [`presenter`] | P | `presenter.py`, `swapchain.py`: the effect chain and presentation |
//! | [`framegen`] | F | `native_framegen.py`: the frame generation thread, cycles, background restart |
//! | [`pacing`] | F | the pacing of generated frames |
//! | [`stats`] | S | `stats.py`: `StatsSample` collection, GPU warning, panel content |
//! | [`osd`] | S | `osd.py`: OSD messages and timing |
//! | [`latency`] | S | the latency CSV (`LatencyLog`) |
//! | [`menubar`] | S | `menubar.py`: Wine menu bar detection |
//! | [`bench`] | S | `bench.py`: «Medir en mi GPU» |
//!
//! The daemon is not here: it is `lirtup-core::daemon` (round 4, front Y).
//! Round 4 adds `toolbar` and `tokens` (front T) the same way.
//!
//! The reviewer writes the cross-module signatures (pipeline ↔ stats ↔ osd ↔
//! framegen) before round 2 starts, on top of the APIs round 1 delivers.

pub mod bench;
pub mod framegen;
pub mod latency;
pub mod menubar;
pub mod osd;
pub mod pacing;
pub mod pipeline;
pub mod presenter;
pub mod stats;
pub mod tiles;
pub mod upscale;
