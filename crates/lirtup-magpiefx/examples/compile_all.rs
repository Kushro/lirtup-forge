//! Compiles every built-in MagpieFX effect to SPIR-V with the compilers found
//! on this machine (round 1 acceptance check of 5.8). No GPU needed.
//!
//! ```text
//! cargo run --release -p lirtup-magpiefx --example compile_all [-- --fp16] [-- --cache DIR]
//! ```
//!
//! Without `--cache` it compiles into a fresh temporary folder, so nothing
//! comes from an earlier run. Exits with 1 if any effect fails.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use lirtup_magpiefx::Catalog;
use lirtup_magpiefx::compiler::{SpirvCache, detect_compilers};
use lirtup_magpiefx::runtime::compile_effect;

fn main() {
    let mut fp16 = false;
    let mut cache_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fp16" => fp16 = true,
            "--cache" => cache_dir = args.next().map(PathBuf::from),
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let tmp = tempfile::tempdir().expect("temporary folder");
    let cache = SpirvCache::new(cache_dir.unwrap_or_else(|| tmp.path().to_owned()));

    let compilers = detect_compilers();
    if compilers.is_empty() {
        eprintln!("no HLSL compiler found (dxc or glslang)");
        std::process::exit(1);
    }
    let names: Vec<_> = compilers.iter().map(|c| c.name().to_owned()).collect();
    println!("compilers: {}", names.join(", "));

    let catalog = Catalog::scan(None);
    println!("{} effects, fp16 {fp16}", catalog.entries.len());

    let start = Instant::now();
    let failures = Mutex::new(Vec::new());
    let next = Mutex::new(catalog.entries.iter());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some(entry) = next.lock().unwrap().next() else {
                        break;
                    };
                    match compile_effect(entry, &compilers, &cache, fp16) {
                        Ok(effect) => println!(
                            "ok   {} ({} passes{})",
                            entry.name,
                            effect.spirv.len(),
                            if effect.fp16 { ", fp16" } else { "" }
                        ),
                        Err(err) => {
                            println!("FAIL {}: {err}", entry.name);
                            failures.lock().unwrap().push(entry.name.clone());
                        }
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    println!(
        "{} of {} compiled in {:.1} s",
        catalog.entries.len() - failures.len(),
        catalog.entries.len(),
        start.elapsed().as_secs_f64()
    );
    if !failures.is_empty() {
        println!("failed: {}", failures.join(", "));
        std::process::exit(1);
    }
}
