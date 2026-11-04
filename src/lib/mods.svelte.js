// Installed mods (src-tauri/src/mods.rs), shared by every screen.
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";
import { t, local } from "./i18n.svelte.js";

/**
 * list: [{ id, name, version, authors, description, games, size, enabled, icon, settings }]
 *   name and description: a Text (lib/i18n.svelte.js local()); icon: a data: URL or null; settings: how many it has
 * version: name (a Text) of the active version, or null
 * problems: what the check found in that version and the enabled mods, [{ level: "error" | "warning", message, packages: [id] }]
 */
export const mods = $state({ list: [], version: null, problems: [] });

const apply = (next) => {
  if (next) {
    mods.list = next.mods;
    mods.version = next.version;
    mods.problems = next.problems;
  }
  return next;
};

export const refreshMods = async () => {
  if (inTauri) apply(await invoke("mods_list"));
};

refreshMods();

/** Installs .wpmod files: also says { added: [name], refused: [message] }. */
export const importMods = async (paths) => apply(await invoke("mods_import", { paths }));

/** The same through the file picker; null when cancelled. */
export const pickMods = async () => apply(await invoke("mods_pick"));

export const setModEnabled = async (id, on) => apply(await invoke("mods_set_enabled", { id, on }));

export const removeMod = async (id) => apply(await invoke("mods_remove", { id }));

export const enabledMods = () => mods.list.filter((m) => m.enabled);

/** Where the game stands, for the screens that lead to it. */
export const modsSummary = () => {
  if (mods.problems.some((p) => p.level === "error")) return t("La version ou les mods activés empêchent le lancement : voir la Bibliothèque.");
  const names = [...(mods.version ? [local(mods.version)] : []), ...enabledMods().map((m) => local(m.name))];
  return names.length ? t("Wii Party se lancera avec : {names}.", { names: names.join(", ") }) : t("Wii Party se lancera sans mod.");
};
