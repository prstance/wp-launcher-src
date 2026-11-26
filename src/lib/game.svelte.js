// The running game (src-tauri/src/game.rs), shared by every screen.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./tauri.js";
import { gameOverlay } from "./overlay.js";

/** phase: "idle" | "verifying" | "problems" | "preparing" | "running" | "error"
 *  title: what Dolphin runs, "game" or "miis" (the Mii Channel). */
export const game = $state({ phase: "idle", message: null, title: "game" });

if (inTauri) {
  listen("game://state", (e) => Object.assign(game, e.payload));
  invoke("game_state").then((s) => Object.assign(game, s));
}

/** `force` goes on despite problems found in the ISO. */
export const launch = (force = false) => invoke("game_launch", { force, look: gameOverlay() });
export const stop = () => invoke("game_stop");
export const dismiss = () => invoke("game_dismiss");
