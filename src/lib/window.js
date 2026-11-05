// Window chrome for the frameless launcher window. Outside Tauri (plain
// `npm run dev` in a browser) these are no-ops.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";

const win = inTauri ? getCurrentWindow() : null;

export const minimize = () => win?.minimize();
// The window keeps its 16:9 size (window.rs): Agrandir swaps between two sizes.
export const toggleMaximize = () => inTauri && invoke("window_toggle_size");
export const close = () => win?.close();
