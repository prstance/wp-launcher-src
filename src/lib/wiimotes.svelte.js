// Wii Remotes the launcher is connected to (src-tauri/src/wiimote/), shared by every screen.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./tauri.js";
import { t } from "./i18n.svelte.js";

/**
 * bluetooth: "loading" | "ready" | "off" | "none" | "unsupported" | "error"
 * slots: 4 × (null | { addr, model, battery, extension, buttons })
 * system: remotes held by the computer's own Bluetooth, [{ addr, model }]
 * phones: null, or while phones can connect { url, qr (SVG path), qrSize, error }
 * A slot's model: "Wiimote", "Wiimote Plus" or "phone".
 */
export const wiimotes = $state({
  bluetooth: inTauri ? "loading" : "unsupported",
  message: null,
  scanning: false,
  paused: false,
  connecting: 0,
  slots: [null, null, null, null],
  system: [],
  phones: null,
});

if (inTauri) {
  listen("wiimotes://state", (e) => Object.assign(wiimotes, e.payload));
  invoke("wiimote_state").then((s) => Object.assign(wiimotes, s));
}

/** Whether a game can start. Where the launcher can't connect remotes yet, only Dolphin can tell. */
export const hasPlayer = () => wiimotes.bluetooth === "unsupported" || wiimotes.slots.some(Boolean);

export const scan = (on) => invoke("wiimote_scan", { on });
export const disconnect = (slot) => invoke("wiimote_disconnect", { slot });
export const identify = (slot) => invoke("wiimote_identify", { slot });
export const moveTo = (from, to) => invoke("wiimote_move", { from, to });
export const release = (addr) => invoke("wiimote_release", { addr });

/** A slot's model, for people. */
export const modelName = (model) => (model === "phone" ? t("Téléphone") : model);

/** Player colours, J1–J4: header, darker outline. */
export const PLAYERS = [
  { color: "#1283cb", deep: "#0b4f7c" },
  { color: "#d75d41", deep: "#8a2f1c" },
  { color: "#25a720", deep: "#16631a" },
  { color: "#d9b527", deep: "#6b5600" },
];
