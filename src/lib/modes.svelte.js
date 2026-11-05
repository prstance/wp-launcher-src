// The game's modes on the home screen (src-tauri/src/modes.rs): their names come
// from the player's ISO, or from the active version, as do the pictures of a
// version. The launcher has its own screenshots for most of them (assets/modes),
// shown instead of the disc's small thumbnails, and without an ISO.
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";
import { t } from "./i18n.svelte.js";
import { theme } from "./versions.svelte.js";

/** list: [{ id, name, group: "party" | "pair" | "house" | "version", image, stock }] */
export const modes = $state({ list: [], loading: false });

const SHOTS = Object.fromEntries(
  Object.entries(import.meta.glob("../assets/modes/*.jpg", { eager: true, import: "default" })).map(([path, url]) => [path.split("/").pop().slice(0, -4), url]),
);

/** Those the launcher has a screenshot of, in the main menu's order, with their name in the game
 *  (a function: in the launcher's language of the moment). */
const OWN = [
  ["board-game-island", () => t("Ile aux défis"), "party"],
  ["globe-trot", () => t("Globe-trotter"), "party"],
  ["spin-off", () => t("Roue de la chance"), "party"],
  ["bingo", () => t("Bingo"), "party"],
  ["friend-connection", () => t("Test d'amitié"), "pair"],
  ["balance-boat", () => t("Oh du bateau!"), "pair"],
  ["match-up", () => t("Paires de Mii!"), "pair"],
  ["animal-tracker", () => t("Cris en folie"), "house"],
  ["hide-n-hunt", () => t("Cache-cache"), "house"],
  ["time-bomb", () => t("Tic-tac-boum!"), "house"],
  ["buddy-quiz", () => t("Quiz entre amis"), "house"],
];

let asked = null;

/** Asks the backend again when `key` (what they depend on: language, ISO, version) changed. */
export function loadModes(key, lang) {
  if (!inTauri || asked === key) return;
  asked = key;
  modes.loading = true;
  invoke("modes_get", { lang }).then(
    (list) => {
      if (asked === key) Object.assign(modes, { list, loading: false });
    },
    (e) => {
      console.warn("modes:", e);
      if (asked === key) Object.assign(modes, { list: [], loading: false });
    },
  );
}

/** What the home screen shows. Until the ISO gives the game's modes (or without
 *  one), the launcher's screenshots follow the version's pictures, unless its theme
 *  shows only those. */
export function shownModes() {
  const list = modes.list.map((m) => (m.stock && SHOTS[m.id] ? { ...m, image: SHOTS[m.id] } : m));
  if (theme.modesOnly || list.some((m) => m.group !== "version")) return list;
  return [...list, ...OWN.filter(([id]) => !list.some((m) => m.id === id)).map(([id, name, group]) => ({ id, name: name(), group, image: SHOTS[id], stock: true }))];
}
