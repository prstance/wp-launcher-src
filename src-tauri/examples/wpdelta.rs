//! The deltas of a package (src/delta.rs), for scripts/make-wpmod.py:
//!
//!   cargo run --release --example wpdelta -- jobs.json
//!
//! jobs.json: [{"source": "…", "result": "…", "out": "…", "lz": true}, …]. Prints
//! one line per delta: its size against the result's.

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

fn main() {
    let path = std::env::args().nth(1).expect("usage: wpdelta jobs.json");
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
