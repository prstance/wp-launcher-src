// The player's Miis (src-tauri/src/mii): the library, the game's own Miis, the Mii Channel.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./tauri.js";
import { settings } from "./settings.svelte.js";
import { wiimotes, hasPlayer, scan } from "./wiimotes.svelte.js";
import { t } from "./i18n.svelte.js";

/** list: [{ id, name, creator, female, color, birthday, head }]. head: a PNG data URL,
 *  null until the face parts are taken from the ISO. faces: they are. */
export const miis = $state({ ready: !inTauri, list: [], faces: false, max: 100, suggestedName: "" });

/** The game's own Miis, to start from; loaded when a screen asks. id: their index. */
export const discMiis = $state({ list: [], loading: false, error: null });

const apply = (next) => {
  if (next) Object.assign(miis, { list: next.miis, faces: next.faces, max: next.max, suggestedName: next.suggestedName, ready: true });
  return next;
};

export const refreshMiis = () => (inTauri ? invoke("miis_state").then(apply) : Promise.resolve());

/** Takes them from the ISO the first time, which takes a few seconds. */
export async function loadDiscMiis() {
  if (!inTauri || discMiis.loading) return;
  discMiis.loading = true;
  discMiis.error = null;
  try {
    discMiis.list = await invoke("miis_disc");
  } catch (e) {
    discMiis.error = String(e);
  }
  discMiis.loading = false;
}

if (inTauri) {
  refreshMiis();
  // The heads come once the ISO is read, and the Mii Channel brings new Miis.
  listen("miis://changed", () => {
    refreshMiis();
    if (discMiis.list.length || discMiis.error) loadDiscMiis();
  });
}

/** The player's Mii, if they have one. */
export const myMii = () => miis.list.find((m) => m.id === settings.player?.mii) ?? null;

/** Copies one of the game's Miis into the library under `name`; its id. */
export async function adoptMii(index, name) {
  const done = await invoke("miis_adopt", { index, name });
  apply(done.state);
  return done.id;
}

/** { added: [id], refused: [message] } */
export async function importMiis(paths) {
  const done = await invoke("miis_import", { paths });
  apply(done.state);
  return done;
}

/** The same through the file picker; null when cancelled. */
export async function pickMiis() {
  const done = await invoke("miis_pick");
  if (done) apply(done.state);
  return done;
}

export const renameMii = async (id, name) => apply(await invoke("miis_rename", { id, name }));

export async function removeMii(id) {
  apply(await invoke("miis_remove", { id }));
  // The backend lets go of it in the settings too.
  if (settings.player?.mii === id) settings.player = { ...settings.player, mii: null };
}

/** Saves it as a .mii file; false when cancelled. */
export const exportMii = (id) => invoke("miis_export", { id });

/** Dolphin opens the Mii Channel; game.title is "miis" while it runs. */
export const openMiiChannel = () => invoke("miis_channel");

/** The Mii Channel button, which first connects a remote when there is none: the
 *  channel is played by pointing. { label, hint, enabled, run, waiting }: waiting
 *  while it can't open the channel yet. */
export function channelButton() {
  const open = { label: t("Créer dans la Chaîne Mii"), enabled: true, run: openMiiChannel, waiting: false };
  if (!settings.iso) return { ...open, hint: t("Avec ton ISO de Wii Party"), enabled: false, waiting: true };
  if (hasPlayer()) return { ...open, hint: t("Dans Dolphin, depuis ton disque") };
  const connect = { label: t("Connecter une Wiimote"), enabled: false, waiting: true };
  if (wiimotes.connecting) return { ...connect, hint: t("Connexion…") };
  if (wiimotes.scanning) return { ...connect, hint: t("Recherche : appuie sur 1 et 2"), enabled: true, run: () => scan(false) };
  const blocked = {
    loading: t("Bluetooth…"),
    off: t("Bluetooth désactivé : active-le sur l’ordinateur"),
    none: t("Aucun adaptateur Bluetooth trouvé"),
    error: wiimotes.message ?? t("Erreur Bluetooth"),
  }[wiimotes.bluetooth];
  if (blocked) return { ...connect, hint: blocked };
  return { ...connect, hint: t("Indispensable pour la Chaîne Mii"), enabled: true, run: () => scan(true) };
}
