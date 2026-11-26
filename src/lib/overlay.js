// What Dolphin's window shows while the game loads, and its notifications (src-tauri/src/overlay.rs):
// the launcher's colours as they are drawn now (light or dark, a version's), and texts.
import { activeVersion } from "./versions.svelte.js";
import { enabledMods } from "./mods.svelte.js";
import { PLAYERS } from "./wiimotes.svelte.js";
import { t, local } from "./i18n.svelte.js";

const COLOURS = {
  bg: "var(--bg)",
  card: "var(--card)",
  text: "var(--text)",
  muted: "var(--muted)",
  accent: "var(--accent)",
  accentSoft: "var(--accent-soft)",
  accentDeep: "var(--accent-deep)",
  label: "var(--label)",
  tint: "var(--tint)",
  line: "var(--line)",
  decor: "var(--decor)",
  gradTop: "var(--grad-top)",
  gradBottom: "var(--grad-bottom)",
  shadow: "rgb(var(--shadow))",
  dangerBg: "var(--danger-bg)",
  danger: "var(--danger)",
};

/** Each colour as "#rrggbb", whatever CSS made it from (color-mix, a version's theme). */
function colours() {
  const probe = document.createElement("span");
  document.body.append(probe);
  const ctx = Object.assign(document.createElement("canvas"), { width: 1, height: 1 }).getContext("2d", { willReadFrequently: true });
  const hex = (css) => {
    probe.style.color = css;
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "#000";
    ctx.fillStyle = getComputedStyle(probe).color;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
  };
  const look = Object.fromEntries(Object.entries(COLOURS).map(([key, css]) => [key, hex(css)]));
  probe.remove();
  return { ...look, players: PLAYERS.map((p) => p.color) };
}

/** The loading screen's title and line under it, and the launcher's look. */
export function overlay(title, subtitle) {
  return {
    look: colours(),
    title,
    subtitle,
    starting: t("Démarrage du jeu…"),
    shaders: t("Préparation des graphismes : {n} %"),
  };
}

/** Wii Party: the version's name, and the mods it starts with. */
export function gameOverlay() {
  const version = activeVersion();
  const mods = enabledMods().map((m) => local(m.name));
  const subtitle = mods.length ? t("Avec {names}", { names: mods.join(", ") }) : version ? "" : t("Le jeu d’origine, sans mod");
  return overlay(local(version?.name) ?? "Wii Party", subtitle);
}
