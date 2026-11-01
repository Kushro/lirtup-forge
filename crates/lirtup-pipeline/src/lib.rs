//! The scaling pipeline of LirtUp Forge (`src/upscaler/pipeline/`).
//!
//! Modules are declared here once (by the reviewer), so the round 2 fronts
//! work in separate files and never edit this one:
//!
//! | Module | Front | Ports |
//! | --- | --- | --- |
//! | [`error`] | reviewer | the error every module returns |
//! | [`pipeline`] | P | `pipeline.py`, `session.py`, `controller.py`: the session thread (capture → upload → chain → present), its commands and config swaps |
//! | [`tiles`] | P | `tile.py`, `utils.py`: tile processing on the GPU side |
//! | [`upscale`] | P | `upscale.py`: the `UpscalerManager` (models, double upscale, tiles or full frame) |
//! | [`presenter`] | P | `presenter.py`: the effect chain and presentation |
//! | [`surface`] | K | `swapchain.py` and the Qt overlay thread: the overlay window thread and the swapchain on it |
//! | [`framegen`] | F | `native_framegen.py`: the frame generation thread, cycles, background restart |
//! | [`pacing`] | F | the pacing of generated frames |
//! | [`stats`] | S | `stats.py`: `StatsSample` collection, GPU warning, the stats panel |
//! | [`osd`] | S | `osd.py`: OSD messages and timing |
//! | [`latency`] | S | the latency CSV (`LatencyLog`) |
//! | [`menubar`] | S | `menubar.py`: Wine menu bar detection |
//! | [`bench`] | S | `bench.py`: «Medir en mi GPU» |
//!
//! The daemon is not here: it is `lirtup-core::daemon` (round 4, front Y).
//! Round 4 adds `toolbar` and `tokens` (front T) the same way.
//!
//! # How the pieces talk (round 2 contract)
//!
//! ```text
//!  lirtup-core::Engine (K) ──SessionSetup──▶ pipeline::Session (P, its own thread)
//!        ▲   │ PipelineCommand                 │ owns: Capture, Upscaler, Presenter,
//!        │   ▼                                 │       Osd, StatsPanel, Stats, MenuBarDetector
//!   proto::Event ◀──────── events ─────────────┤
//!                                              ├─▶ surface::OverlayThread (K): OverlayWindow,
//!                                              │     InputForwarder; OverlayCommand ⇄ OverlayNotice
//!                                              └─▶ Presenter ──▶ SharedSurface (swapchain, K)
//!                                                     │              ▲
//!                                                     └─▶ FrameGen (F) ─ lsfg-present thread
//!  Present reports (real and generated) ──PresentSink──▶ Stats (S) ──▶ LatencyLog (S)
//! ```
//!
//! - Every module returns [`error::Result`]; the error knows its
//!   [`lirtup_proto::Stage`] so a failure becomes `Event::SessionError`.
//! - The session thread sends [`lirtup_proto::Event`]s on a
//!   `crossbeam_channel::Sender<Event>` it gets from the engine; nothing
//!   here knows about the socket.
//! - The swapchain is shared by the session thread and the frame generation
//!   present thread as a [`surface::SharedSurface`] (`Arc<Mutex<_>>`).
//! - Presents are reported to [`stats::Stats`] through a
//!   [`stats::PresentSink`] from either thread, with the capture time of the
//!   game frame they came from (latency).

pub mod bench;
pub mod error;
pub mod framegen;
pub mod latency;
pub mod menubar;
pub mod osd;
pub mod pacing;
pub mod pipeline;
pub mod presenter;
pub mod stats;
pub mod surface;
pub mod tiles;
pub mod upscale;

pub use error::{PipelineError, Result};
pub use pipeline::{PipelineCommand, Session, SessionSetup};
