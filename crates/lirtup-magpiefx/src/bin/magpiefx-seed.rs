//! Writes the precompiled MagpieFX seed: the SPIR-V of every built-in
//! effect, for every swap combination a session can ask for, with the file
//! names the runtime cache uses (`SpirvCache`). No GPU needed.
//!
//! ```text
//! cargo run --release -p lirtup-magpiefx --bin magpiefx-seed -- --out DIR [--jobs N] [--fp16 both|on|off] [--effects a,b]
//! ```
//!
//! Exits with 1 if any effect fails.

// TODO(magpiefx): block 4c of the live-tuning contract (evolve
// examples/compile_all.rs).
fn main() {
    eprintln!("magpiefx-seed: not implemented yet");
    std::process::exit(1);
}
