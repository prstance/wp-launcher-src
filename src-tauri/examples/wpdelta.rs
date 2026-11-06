//! The deltas of a package (src/delta.rs), for scripts/make-wpmod.py:
//!
//!   cargo run --release --example wpdelta -- jobs.json
//!
//! jobs.json: [{"source": "…", "result": "…", "out": "…", "lz": true}, …]. Prints
//! one line per delta: its size against the result's.
//!
//!   cargo run --release --example wpdelta -- --apply <unpacked package> <extracted disc>
//!
//! makes the package's files from the disc as the launcher does before a launch (rebuild.rs), to try
//! it out of the launcher.

#[path = "../src/delta.rs"]
mod delta;
#[path = "../src/lz.rs"]
mod lz;

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Job {
    source: PathBuf,
    result: PathBuf,
    out: PathBuf,
    lz: bool,
}

fn apply(package: &std::path::Path, disc: &std::path::Path) {
    let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(package.join("mod.json")).or_else(|_| std::fs::read_to_string(package.join("version.json"))).unwrap()).unwrap();
    for p in m["patches"].as_array().unwrap().iter().filter(|p| p["type"] == "delta") {
        for f in p["files"].as_array().unwrap() {
            let (from, file) = (f["disc"].as_str().unwrap(), f["file"].as_str().unwrap());
            let source = match from {
                "main.dol" => disc.join("sys/main.dol"),
                d => disc.join("files").join(d.trim_start_matches('/')),
            };
            let made = delta::apply(&std::fs::read(package.join(f["delta"].as_str().unwrap())).unwrap(), &std::fs::read(&source).unwrap())
                .unwrap_or_else(|e| panic!("{file}: {e:?}"));
            let out = package.join(file);
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            std::fs::write(&out, made).unwrap();
            println!("{file}");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--apply") {
        return apply(args[1].as_ref(), args[2].as_ref());
    }
    let path = args.first().expect("usage: wpdelta jobs.json");
    let jobs: Vec<Job> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for job in jobs {
        let (source, result) = (std::fs::read(&job.source).unwrap(), std::fs::read(&job.result).unwrap());
        let made = delta::make(&source, &result, job.lz).unwrap_or_else(|e| panic!("{}: {e}", job.result.display()));
        // (checked, as the launcher will make it)
        let again = delta::apply(&made, &source).unwrap_or_else(|e| panic!("{}: {e:?}", job.result.display()));
        if job.lz {
            assert_eq!(lz::decompress(&again).unwrap(), lz::decompress(&result).unwrap(), "{}", job.result.display());
        } else {
            assert!(again == result, "{}", job.result.display());
        }
        if let Some(dir) = job.out.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&job.out, &made).unwrap();
        println!("{} {} {}", made.len(), result.len(), job.result.display());
    }
}
