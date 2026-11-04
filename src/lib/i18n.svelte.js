// The launcher's languages. Texts are written in French in the code and go
// through t(), which looks them up in src/locales/<lang>.js; a text missing
// there stays in French (scripts/check-i18n.mjs lists them).
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./tauri.js";
import { settings } from "./settings.svelte.js";
import { refreshMods } from "./mods.svelte.js";
import en from "../locales/en.js";
import es from "../locales/es.js";

/** [code, name in that language] */
export const LANGUAGES = [["fr", "Français"], ["en", "English"], ["es", "Español"]];

const DICTIONARIES = { en, es };

/** The system's language when the launcher has it, English otherwise. */
const SYSTEM = (() => {
  const wanted = (navigator.languages?.length ? navigator.languages : [navigator.language ?? ""]).map((l) => l.slice(0, 2).toLowerCase());
  return wanted.find((l) => LANGUAGES.some(([code]) => code === l)) ?? "en";
})();

/** "fr", "en" or "es": settings.language, or the system's for "auto". */
export const lang = () => (LANGUAGES.some(([code]) => code === settings.language) ? settings.language : SYSTEM);

const fill = (text, params) => (params ? text.replace(/\{(\w+)\}/g, (all, name) => (name in params ? String(params[name]) : all)) : text);

/** The French `text` in the launcher's language; {name} takes params.name. */
export function t(text, params) {
  return fill(DICTIONARIES[lang()]?.[text] ?? text, params);
}

/** `one` or `other` (both in French, as for t) depending on `count`, which also fills {n}. */
export function plural(count, one, other, params) {
  // French puts 0 with 1; English and Spanish with the others.
  const single = lang() === "fr" ? Math.abs(count) < 2 : count === 1;
  return t(single ? one : other, { n: count, ...params });
}

/** A text from a package: a string, or the same text per language ({ fr, en, … }). */
export function local(text) {
  if (text == null || typeof text === "string") return text;
  return text[lang()] ?? text.en ?? text.fr ?? Object.values(text)[0] ?? "";
}

/** Keeps the page's lang and the backend's messages (src-tauri/src/i18n.rs) in the launcher's language. */
export function followLanguage() {
  return $effect.root(() => {
    $effect(() => {
      const code = lang();
      document.documentElement.lang = code;
      // The mods' problems are written by the backend: ask for them again.
      if (inTauri) invoke("locale_set", { lang: code }).then(refreshMods, () => {});
    });
  });
}
