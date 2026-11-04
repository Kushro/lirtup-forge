//! Writes the precompiled MagpieFX seed: the SPIR-V of every built-in
//! effect, for every swap combination a session can ask for, with the file
//! names the runtime cache uses (`SpirvCache`). No GPU needed.
//!
//! ```text
//! cargo run --release -p lirtup-magpiefx --bin magpiefx-seed -- --out DIR [--jobs N] [--fp16 both|on|off] [--effects a,b] [--effects-dir DIR]
//! ```
//!
//! It compiles through the runtime's own API into a cache whose folder is
//! `--out` (and that has no seed), so the files get exactly the names the
//! runtime looks up, and variants with the same key are written once. The
//! keys do not depend on the effects folder's path nor on the compiler's
//! version: the seed hits for `/usr/share/lirtup/effects`, the AppImage's
//! mount and any dxc/glslang, or none.
//!
//! dxc compiles first and glslang is the fallback (the three
//! `Anime4K_Denoise_Bilateral_*` effects need it). Exits with 1 if any
//! variant fails, with 2 on a usage error.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use lirtup_magpiefx::catalog::{Catalog, EFFECTS_DIR_ENV};
use lirtup_magpiefx::compiler::{SpirvCache, detect_compilers};
use lirtup_magpiefx::runtime::{CompileOptions, compile_effect_with, compile_swap_rb};

const USAGE: &str = "\
usage: magpiefx-seed --out DIR [options]

Writes the precompiled MagpieFX seed (SPIR-V of the built-in effects).

options:
  --out DIR          where the .spv files go (created if missing; existing
                     files are kept)
  --jobs N           parallel compilations (default: the number of CPUs)
  --fp16 MODE        both (default): FP32, plus FP16 for the effects that
                     declare it; on: FP16 only (FP32 where not declared);
                     off: FP32 only
  --effects A,B      only these effects (catalog names), default all
  --effects-dir DIR  the built-in effects (default: $LIRTUP_EFFECTS_DIR,
                     else this crate's effects/)
  -h, --help         this help

environment: DXC, GLSLANG, UPSCALER_MAGPIEFX_COMPILER (as the runtime)";

/// The `(swap_input, swap_output)` combinations a session can request: the
/// first effect of a chain reads BGRA, the last one stores BGRA, a single
/// effect does both, the ones in between neither.
const SWAPS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

/// Which precisions to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fp16 {
    Both,
    On,
    Off,
}

impl Fp16 {
    fn parse(s: &str) -> Option<Fp16> {
        match s {
            "both" => Some(Fp16::Both),
            "on" => Some(Fp16::On),
            "off" => Some(Fp16::Off),
            _ => None,
        }
    }

    /// The `CompileOptions::fp16` values to compile with. FP16 on an effect
    /// without `//!CAPABILITY FP16` compiles FP32 (same keys, so a hit).
    fn variants(self) -> &'static [bool] {
        match self {
            Fp16::Both => &[false, true],
            Fp16::On => &[true],
            Fp16::Off => &[false],
        }
    }
}

/// The command line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    out: PathBuf,
    jobs: usize,
    fp16: Fp16,
    effects: Option<Vec<String>>,
    effects_dir: PathBuf,
}

/// Parse the arguments after the program name. `Ok(None)`: help asked.
fn parse_args(
    args: impl IntoIterator<Item = String>,
    default_effects_dir: PathBuf,
    default_jobs: usize,
) -> Result<Option<Args>, String> {
    let mut out: Option<PathBuf> = None;
    let mut jobs = default_jobs.max(1);
    let mut fp16 = Fp16::Both;
    let mut effects: Option<Vec<String>> = None;
    let mut effects_dir = default_effects_dir;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_owned(), Some(v.to_owned())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| args.next())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(None),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--jobs" => {
                let v = value("--jobs")?;
                jobs = v
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("--jobs: {v:?} is not a positive number"))?;
            }
            "--fp16" => {
                let v = value("--fp16")?;
                fp16 = Fp16::parse(&v).ok_or_else(|| format!("--fp16: {v:?} is not both, on or off"))?;
            }
            "--effects" => {
                let v = value("--effects")?;
                let names: Vec<String> = v
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                if names.is_empty() {
                    return Err("--effects needs at least one name".to_owned());
                }
                effects = Some(names);
            }
            "--effects-dir" => effects_dir = PathBuf::from(value("--effects-dir")?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let out = out.ok_or_else(|| "--out DIR is required".to_owned())?;
    Ok(Some(Args {
        out,
        jobs,
        fp16,
        effects,
        effects_dir,
    }))
}

/// `$LIRTUP_EFFECTS_DIR`, else this crate's `effects/` (a release build's
/// `catalog::builtin_dir()` would be `/usr/share/lirtup/effects`).
fn default_effects_dir() -> PathBuf {
    std::env::var_os(EFFECTS_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/effects")))
}

/// Number and total size of the `.spv` files in `dir`.
fn spv_files(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "spv"))
        .fold((0, 0), |(n, bytes), e| {
            (n + 1, bytes + e.metadata().map_or(0, |m| m.len()))
        })
}

/// `"ff"`, `"tf"`, ... for the logs.
fn swap_label(swap_input: bool, swap_output: bool) -> String {
    let c = |b: bool| if b { 't' } else { 'f' };
    format!("{}{}", c(swap_input), c(swap_output))
}

fn main() {
    let default_jobs = std::thread::available_parallelism().map_or(4, |n| n.get());
    let args = match parse_args(std::env::args().skip(1), default_effects_dir(), default_jobs) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Err(e) => {
            eprintln!("magpiefx-seed: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    std::process::exit(run(&args));
}

/// Generate the seed; returns the exit code.
fn run(args: &Args) -> i32 {
    if !args.effects_dir.is_dir() {
        eprintln!(
            "magpiefx-seed: no effects folder at {} (--effects-dir)",
            args.effects_dir.display()
        );
        return 1;
    }
    let compilers = detect_compilers();
    if compilers.is_empty() {
        eprintln!("magpiefx-seed: no HLSL compiler found (install dxc and glslang, or set DXC/GLSLANG)");
        return 1;
    }
    let names: Vec<&str> = compilers.iter().map(|c| c.name()).collect();
    println!("compilers: {}", names.join(", "));
    if !names.contains(&"glslangValidator") {
        println!("warning: no glslangValidator; the Anime4K_Denoise_Bilateral effects will fail");
    }

    let catalog = Catalog::scan_dirs(&args.effects_dir, None);
    let entries: Vec<_> = match &args.effects {
        None => catalog.entries.iter().collect(),
        Some(wanted) => {
            let mut found = Vec::new();
            for name in wanted {
                match catalog.find(name) {
                    Some(e) => found.push(e),
                    None => {
                        eprintln!(
                            "magpiefx-seed: no effect {name:?} in {}",
                            args.effects_dir.display()
                        );
                        return 2;
                    }
                }
            }
            found
        }
    };
    if entries.is_empty() {
        eprintln!("magpiefx-seed: no effects in {}", args.effects_dir.display());
        return 1;
    }

    if let Err(e) = std::fs::create_dir_all(&args.out) {
        eprintln!("magpiefx-seed: cannot create {}: {e}", args.out.display());
        return 1;
    }
    let (files_before, _) = spv_files(&args.out);
    // No seed: everything comes from the compilers (or from --out itself).
    let cache = SpirvCache::with_seed(args.out.clone(), None);
    println!(
        "{} effects from {}, fp16 {:?}, {} jobs, into {}",
        entries.len(),
        args.effects_dir.display(),
        args.fp16,
        args.jobs,
        args.out.display()
    );

    let start = Instant::now();
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let variants_ok = AtomicUsize::new(0);
    let fp16_effects = AtomicUsize::new(0);

    // The separate swap pass, for the chains that cannot fold the swap.
    match compile_swap_rb(&compilers, &cache) {
        Ok(_) => {
            variants_ok.fetch_add(1, Ordering::Relaxed);
            println!("ok   SwapRB");
        }
        Err(e) => {
            println!("FAIL SwapRB: {e}");
            failures.lock().unwrap().push("SwapRB".to_owned());
        }
    }

    // One job per (effect, precision); its swap combinations run in order
    // (they share the unswapped fallback).
    let jobs: Vec<(&lirtup_magpiefx::CatalogEntry, bool)> = entries
        .iter()
        .flat_map(|e| args.fp16.variants().iter().map(move |fp16| (*e, *fp16)))
        .collect();
    let next = Mutex::new(jobs.iter());
    std::thread::scope(|scope| {
        for _ in 0..args.jobs.min(jobs.len()).max(1) {
            scope.spawn(|| {
                loop {
                    let Some(&(entry, fp16)) = next.lock().unwrap().next() else {
                        break;
                    };
                    let mut ok = 0;
                    let mut used_fp16 = false;
                    for (swap_input, swap_output) in SWAPS {
                        let options = CompileOptions {
                            fp16,
                            swap_input,
                            swap_output,
                        };
                        match compile_effect_with(entry, &compilers, &cache, &options) {
                            Ok(effect) => {
                                ok += 1;
                                used_fp16 |= effect.fp16;
                            }
                            Err(e) => {
                                let what = format!(
                                    "{} ({}, swap {})",
                                    entry.name,
                                    if fp16 { "fp16" } else { "fp32" },
                                    swap_label(swap_input, swap_output)
                                );
                                println!("FAIL {what}: {e}");
                                failures.lock().unwrap().push(what);
                            }
                        }
                    }
                    variants_ok.fetch_add(ok, Ordering::Relaxed);
                    if fp16 && used_fp16 {
                        fp16_effects.fetch_add(1, Ordering::Relaxed);
                    }
                    if ok == SWAPS.len() {
                        println!(
                            "ok   {}{}",
                            entry.name,
                            match (fp16, used_fp16) {
                                (true, true) => " (fp16)",
                                (true, false) => " (fp16 requested, fp32)",
                                _ => "",
                            }
                        );
                    }
                }
            });
        }
    });

    let failures = failures.into_inner().unwrap();
    let (files_after, bytes) = spv_files(&args.out);
    let attempted = jobs.len() * SWAPS.len() + 1;
    println!();
    println!(
        "{} of {} variants compiled ({} effects, {} in FP16) in {:.1} s",
        variants_ok.load(Ordering::Relaxed),
        attempted,
        entries.len(),
        fp16_effects.load(Ordering::Relaxed),
        start.elapsed().as_secs_f64()
    );
    println!(
        "{}: {} .spv files ({} new), {:.1} MiB",
        args.out.display(),
        files_after,
        files_after.saturating_sub(files_before),
        bytes as f64 / (1024.0 * 1024.0)
    );
    if failures.is_empty() {
        0
    } else {
        println!("failed ({}): {}", failures.len(), failures.join(", "));
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, String> {
        parse_args(args.iter().map(|s| (*s).to_owned()), PathBuf::from("/fx"), 8)
    }

    #[test]
    fn defaults() {
        let a = parse(&["--out", "seed"]).unwrap().unwrap();
        assert_eq!(
            a,
            Args {
                out: PathBuf::from("seed"),
                jobs: 8,
                fp16: Fp16::Both,
                effects: None,
                effects_dir: PathBuf::from("/fx"),
            }
        );
    }

    #[test]
    fn every_option() {
        let a = parse(&[
            "--out=o",
            "--jobs",
            "3",
            "--fp16",
            "off",
            "--effects",
            "CAS/CAS, Anime4K/Anime4K_Upscale_S,",
            "--effects-dir",
            "/e",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(a.out, PathBuf::from("o"));
        assert_eq!(a.jobs, 3);
        assert_eq!(a.fp16, Fp16::Off);
        assert_eq!(
            a.effects,
            Some(vec!["CAS/CAS".to_owned(), "Anime4K/Anime4K_Upscale_S".to_owned()])
        );
        assert_eq!(a.effects_dir, PathBuf::from("/e"));
    }

    #[test]
    fn errors_and_help() {
        assert_eq!(parse(&["--help"]), Ok(None));
        assert!(parse(&[]).unwrap_err().contains("--out"));
        assert!(parse(&["--out"]).is_err());
        assert!(parse(&["--out", "o", "--jobs", "0"]).is_err());
        assert!(parse(&["--out", "o", "--fp16", "maybe"]).is_err());
        assert!(parse(&["--out", "o", "--effects", " , "]).is_err());
        assert!(parse(&["--out", "o", "--bogus"]).is_err());
    }

    #[test]
    fn fp16_variants() {
        assert_eq!(Fp16::Both.variants(), [false, true]);
        assert_eq!(Fp16::On.variants(), [true]);
        assert_eq!(Fp16::Off.variants(), [false]);
    }

    #[test]
    fn swaps_cover_every_combination_once() {
        let mut all = SWAPS.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 4);
        assert_eq!(swap_label(true, false), "tf");
    }

    #[test]
    fn default_effects_dir_is_the_crate_folder_or_the_variable() {
        // only meaningful without the variable; the folder exists in the repo
        if std::env::var_os(EFFECTS_DIR_ENV).is_none() {
            assert!(default_effects_dir().join("FSR").is_dir());
        }
    }
}
