// Fetches the programs the launcher ships with (src-tauri/tools.json) into
// src-tauri/tools/<id>/ for this platform; Tauri bundles that folder as a
// resource. Runs before `tauri dev` and `tauri build`, and does nothing when the
// right versions are already there.
//
//   node scripts/fetch-tools.mjs            dev: unpinned (sha256 null) archives are allowed
//   node scripts/fetch-tools.mjs --strict   build: every archive must have its sha256
//   WP_TOOL_DOLPHIN=path/to/archive …       use a local archive instead (e.g. a CI artifact)
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "../src-tauri");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "tools.json"), "utf8"));
const os = { linux: "linux", win32: "windows", darwin: "macos" }[process.platform];
const strict = process.argv.includes("--strict");
const toolsDir = path.join(root, "tools");
const cacheDir = path.join(root, ".tools-cache");

const readJson = (file) => {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
};

const sha256 = (file) => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

async function download(name, { url, sha256: expected }) {
  if (!expected && strict) throw new Error(`${name} : pas de sha256 dans tools.json pour ${url}`);
  const file = path.join(cacheDir, path.basename(new URL(url).pathname));
  if (!(fs.existsSync(file) && expected && sha256(file) === expected)) {
    console.log(`${name} : téléchargement de ${url}`);
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${name} : ${url} → HTTP ${res.status}`);
    fs.mkdirSync(cacheDir, { recursive: true });
    fs.writeFileSync(file, Buffer.from(await res.arrayBuffer()));
  }
  const actual = sha256(file);
  if (!expected) console.warn(`${name} : sha256 à reporter dans tools.json : ${actual}`);
  else if (actual !== expected) throw new Error(`${name} : sha256 ${actual}, attendu ${expected}`);
  return file;
}

function extract(archive, dest) {
  fs.mkdirSync(dest, { recursive: true });
  if (archive.endsWith(".zip") && process.platform !== "win32") execFileSync("unzip", ["-q", archive, "-d", dest]);
  else execFileSync("tar", ["-xf", archive, "-C", dest]);
  // Links (an AppImage's desktop entry and icon…) become plain files or go,
  // so the folder copies anywhere as a resource.
  for (const entry of fs.readdirSync(dest, { recursive: true, withFileTypes: true })) {
    const link = path.join(entry.parentPath, entry.name);
    if (!entry.isSymbolicLink()) continue;
    const target = fs.existsSync(link) ? fs.realpathSync(link) : null;
    fs.rmSync(link);
    if (target && fs.statSync(target).isFile()) fs.copyFileSync(target, link);
  }
}

let failed = false;
for (const [id, tool] of Object.entries(manifest)) {
  const source = tool[os];
  if (!source) continue;
  const dir = path.join(toolsDir, id);
  const local = process.env[`WP_TOOL_${id.toUpperCase()}`];
  const version = local ? `${tool.version}+local` : tool.version;
  const marker = readJson(path.join(dir, "installed.json"));
  if (marker?.version === version && fs.existsSync(path.join(dir, source.bin))) continue;

  try {
    const archive = local ? path.resolve(local) : await download(tool.name, source);
    const staging = `${dir}.staging`;
    fs.rmSync(staging, { recursive: true, force: true });
    extract(archive, staging);
    if (!fs.existsSync(path.join(staging, source.bin))) throw new Error(`${tool.name} : ${source.bin} absent de l'archive`);
    fs.writeFileSync(path.join(staging, "installed.json"), JSON.stringify({ version, bin: source.bin }, null, 2));
    fs.rmSync(dir, { recursive: true, force: true });
    fs.renameSync(staging, dir);
    console.log(`${tool.name} ${version} prêt dans src-tauri/tools/${id}`);
  } catch (e) {
    // In development, an older copy is better than no app at all.
    if (marker && !strict) console.warn(`${e.message}\n${tool.name} : on garde la version ${marker.version}`);
    else {
      console.error(e.message);
      failed = true;
    }
  }
}
process.exit(failed ? 1 : 0);
