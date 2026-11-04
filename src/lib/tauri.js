// True inside the desktop app, false when the UI runs in a plain browser (`npm run dev`).
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
