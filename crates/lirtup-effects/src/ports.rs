//! The hand ports of the retired native effects engine: Anime4K_Upscale_S
//! (four passes, 2x), LumaSharpen and CRT_Easymode (`shaders/effects/
//! {anime4k,luma_sharpen,crt_easymode}/`). Only with the `ports` feature,
//! which only the tests turn on: the app runs these effects through
//! MagpieFX (fx-unify).
//!
//! Kept as references: `tests/parity.rs` compares them with the 1.x Python
//! ports, and `lirtup-magpiefx/tests/ports.rs` with the same effects run
//! through MagpieFX.
//!
//! TODO(effects): move here `CrtParams`, the `Anime4kS`, `LumaSharpen` and
//! `CrtEasymode` variants (as `PortEffect`) and their pass, out of
//! `effects.rs`; their `.spv` are embedded only with this feature.
