// Programs shipped inside the launcher (src-tauri/src/tools.rs).
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";

/** @returns {Promise<Array<{id, name, version, installedVersion, path}>>} empty outside the app */
export const toolsStatus = () => (inTauri ? invoke("tools_status") : Promise.resolve([]));

export const dolphinOf = (tools) => tools?.find((t) => t.id === "dolphin" && t.installedVersion) ?? null;
