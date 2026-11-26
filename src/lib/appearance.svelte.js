// Light or dark launcher (settings.appearance: light by default, "system" follows the computer). The scheme is
// data-scheme on <html>, whose dark colours are in app.css; a copy in localStorage paints the
// right one before the settings arrive.
import { settings } from "./settings.svelte.js";

const KEY = "wp-appearance";
const system = matchMedia("(prefers-color-scheme: dark)");
const changed = new Set();

let current = (() => {
  try {
    return localStorage.getItem(KEY) ?? "light";
  } catch {
    return "light";
  }
})();

const dark = () => current === "dark" || (current === "system" && system.matches);

/** The launcher is drawn dark. */
export const isDark = () => document.documentElement.dataset.scheme === "dark";

/** `fn` runs each time the launcher goes light or dark. */
export const onSchemeChange = (fn) => changed.add(fn);

function paint() {
  const scheme = dark() ? "dark" : "light";
  if (document.documentElement.dataset.scheme === scheme) return;
  document.documentElement.dataset.scheme = scheme;
  changed.forEach((fn) => fn());
}

paint();
system.addEventListener("change", paint);

/** Keeps the launcher in the chosen appearance. */
export function followAppearance() {
  return $effect.root(() => {
    $effect(() => {
      if (!settings.loaded) return;
      current = settings.appearance;
      try {
        localStorage.setItem(KEY, current);
      } catch {}
      paint();
    });
  });
}
