// Launcher settings (src-tauri/src/settings.rs) and the current ISO, shared by every screen.
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";

export const settings = $state({
  loaded: false,
  player: null, // { name, mii }: the account, made on the welcome screen
  iso: null, // { path, name, gameId, country }
  recent: [],
  alwaysUseIso: false,
  verifyIso: true,
  windowMode: 0, // 0 full screen, 1 borderless full screen, 2 windowed
  gameLanguage: 3,
  resolution: 3,
  aspect: 0,
  vsync: true,
  showFps: false,
  volume: 100,
  minimizeInGame: true,
  sensorBarTop: false, // the sensor bar on top of the screen, not below it
  discordPresence: true,
  language: "auto", // "fr", "en", "es" or "auto" (lib/i18n.svelte.js)
  autostart: false,
  dev: false, // the launcher in development: its Debug page (screens/Debug.svelte)
});

const apply = (next) => {
  if (next) Object.assign(settings, next, { loaded: true });
};

export const settingsReady = inTauri ? invoke("settings_get").then(apply) : Promise.resolve();

/** Changes some settings; the screen follows at once, and goes back if saving fails. */
export async function setSettings(patch) {
  const before = Object.fromEntries(Object.keys(patch).map((k) => [k, settings[k]]));
  Object.assign(settings, patch);
  if (!inTauri) return;
  try {
    apply(await invoke("settings_set", { patch }));
  } catch (e) {
    Object.assign(settings, before);
    throw e;
  }
}

/** Opens the file picker. Throws a message when the file isn't Wii Party. */
export const pickIso = async () => apply(await invoke("iso_pick"));

/** Uses that file. Throws a message when it isn't Wii Party. */
export const chooseIso = async (path) => apply(await invoke("iso_choose", { path }));
