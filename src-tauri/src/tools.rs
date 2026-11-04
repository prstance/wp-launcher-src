//! Programs shipped inside the launcher (Dolphin, wit). scripts/fetch-tools.mjs
//! puts them in src-tauri/tools/<id>/ at build time, with an installed.json
//! marker; Tauri bundles that folder as a resource.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const MANIFEST: &str = include_str!("../tools.json");

#[derive(Deserialize)]
struct Entry {
    name: String,
    version: String,
}

#[derive(Deserialize)]
struct Marker {
    version: String,
    bin: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    id: String,
    name: String,
    /// What tools.json asks for.
    version: String,
    /// What is actually there (None: missing from this build).
    installed_version: Option<String>,
    path: Option<PathBuf>,
}

fn tools_dir(app: &AppHandle) -> Option<PathBuf> {
    // During development they run straight from src-tauri/tools.
    if cfg!(debug_assertions) {
        return Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools"));
    }
    app.path().resource_dir().ok().map(|dir| dir.join("tools"))
}

/// Executable of a shipped tool, if this build has it.
pub fn find(app: &AppHandle, id: &str) -> Option<(String, PathBuf)> {
    let dir = tools_dir(app)?.join(id);
    let marker: Marker = serde_json::from_slice(&std::fs::read(dir.join("installed.json")).ok()?).ok()?;
    let bin = dir.join(&marker.bin);
    bin.is_file().then_some((marker.version, bin))
}

/// Dolphin's command line tool, which reads disc images.
pub fn dolphin_tool(app: &AppHandle) -> Option<PathBuf> {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).ok()?;
    let relative = manifest["dolphin"][std::env::consts::OS]["tool"].as_str()?;
    let path = tools_dir(app)?.join("dolphin").join(relative);
    path.is_file().then_some(path)
}

#[tauri::command]
pub fn tools_status(app: AppHandle) -> Vec<Tool> {
    let manifest: BTreeMap<String, Entry> = serde_json::from_str(MANIFEST).expect("tools.json");
    manifest
        .into_iter()
        .map(|(id, entry)| {
            let found = find(&app, &id);
            Tool {
                installed_version: found.as_ref().map(|(v, _)| v.clone()),
                path: found.map(|(_, p)| p),
                id,
                name: entry.name,
                version: entry.version,
            }
        })
        .collect()
}
