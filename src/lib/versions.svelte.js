// Installed versions (src-tauri/src/versions.rs) and the theme the active one brings.
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";
import { refreshMods } from "./mods.svelte.js";

const COLORS_KEY = "wp-theme-colors";

/** list: [{ id, name, version, authors, description, games, size, active, shipped, icon, settings }]
 *  name and description: a Text (lib/i18n.svelte.js local()). */
export const versions = $state({ ready: !inTauri, list: [], error: null });

/** What screens draw from the theme. Its colors go straight to the CSS variables. */
export const theme = $state({ tagline: null, hero: [], home: [], library: [] });

let painted = [];

const paint = (colors) => {
  const style = document.documentElement.style;
  painted.forEach((name) => style.removeProperty(`--${name}`));
  painted = Object.keys(colors);
  Object.entries(colors).forEach(([name, value]) => style.setProperty(`--${name}`, value));
};

// The colors of last time, so that the launcher doesn't flash pink while it starts.
try {
  paint(JSON.parse(localStorage.getItem(COLORS_KEY)) ?? {});
} catch {}

const apply = (next) => {
  if (!next) return next;
  versions.list = next.versions;
  versions.error = next.error;
  versions.ready = true;
  const colors = next.theme?.colors ?? {};
  paint(colors);
  Object.assign(theme, { tagline: next.theme?.tagline ?? null, hero: next.theme?.hero ?? [], home: next.theme?.home ?? [], library: next.theme?.library ?? [] });
  try {
    localStorage.setItem(COLORS_KEY, JSON.stringify(colors));
  } catch {}
  // Whether the mods get along depends on the version.
  refreshMods();
  return next;
};

if (inTauri) {
  invoke("versions_state").then(apply, (e) => {
    versions.error = String(e);
    versions.ready = true;
  });
}

export const activeVersion = () => versions.list.find((v) => v.active);

/** `id`: null goes back to Wii Party as it came. */
export const activateVersion = async (id) => apply(await invoke("versions_activate", { id }));

/** Installs .wpver files and uses the last one: also says { added: [name], refused: [message] }. */
export const importVersions = async (paths) => apply(await invoke("versions_import", { paths }));

/** The same through the file picker; null when cancelled. */
export const pickVersion = async () => apply(await invoke("versions_pick"));

export const removeVersion = async (id) => apply(await invoke("versions_remove", { id }));
