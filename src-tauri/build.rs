use std::fmt::Write;
use std::path::PathBuf;

fn main() {
    // The mods' core (core/), built into the binary: as bundled resources, its PowerPC ELF objects
    // make linuxdeploy crash while it makes the AppImage (src/mods.rs: core_dir).
    let core = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("core");
    println!("cargo:rerun-if-changed={}", core.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&core).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
    files.sort();
    let mut out = String::from("pub static CORE_FILES: &[(&str, &[u8])] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        writeln!(out, "    ({:?}, include_bytes!({:?})),", f.file_name().unwrap().to_string_lossy(), f.display().to_string()).unwrap();
    }
    out.push_str("];\n");
    std::fs::write(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("core_files.rs"), out).unwrap();
    tauri_build::build()
}
