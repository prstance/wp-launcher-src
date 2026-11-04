// Window chrome for the frameless launcher window. Outside Tauri (plain
// `npm run dev` in a browser) these are no-ops.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "./tauri.js";

const win = inTauri ? getCurrentWindow() : null;

export const minimize = () => win?.minimize();
export const toggleMaximize = () => win?.toggleMaximize();
export const close = () => win?.close();
