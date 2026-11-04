//! The hand ports of the retired native engine (`lirtup_effects::ports`,
//! feature `ports`) against the same effects run through MagpieFX: same
//! test image, same parameters (the table of `lirtup_config::legacy`),
//! FP32. Needs a Vulkan GPU and an HLSL compiler (or the precompiled seed),
//! so the GPU test is ignored by default:
//!
//! ```text
//! cargo test -p lirtup-magpiefx --test ports -- --ignored
//! ```
//!
//! TODO(effects): the GPU comparison, and a plain test that the parameter
//! names of `lirtup_config::legacy::translate_native` are the ports'.

#[test]
#[ignore = "needs a Vulkan GPU and an HLSL compiler or the seed"]
fn ports_match_magpiefx() {
    // TODO(effects): run each `PortEffect` and its MagpieFX twin and compare.
}
