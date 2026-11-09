// Drives the launcher with J1's Wii Remote: it points with a hand cursor, the
// D-pad moves between buttons, A presses. A button drawn with a remote button's
// badge (an aria-hidden "A", "B"… inside it) answers to that button; otherwise
// B goes back and HOME returns to Accueil.
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "./tauri.js";
import { wiimotes } from "./wiimotes.svelte.js";
import { router } from "./router.svelte.js";
import { settings, settingsReady, setSettings } from "./settings.svelte.js";
import { untrack } from "svelte";

// Where older launchers kept the sensor bar's position, before it was a setting.
const BAR_KEY = "wp-sensor-bar";

const SMOOTHING = 0.45;
const REPEAT_DELAY = 380;
const REPEAT_EVERY = 110;

const FOCUSABLE = 'a[href], button:not([disabled]), select:not([disabled]), input:not([disabled]), [role="radio"]';
const DIRECTIONS = { "↑": [0, -1], "↓": [0, 1], "←": [-1, 0], "→": [1, 0] };

/** Hand cursor, in window pixels; `shown` once J1 points at the window. */
export const pointer = $state({ x: 0, y: 0, shown: false, pressed: false });

/** What J1's camera sees (camera pixels, 1024×768), for the Manettes screen;
 *  source: what moves the hand, "bar", "gyro", "tilt" or null. */
export const camera = $state({ dots: [], source: null });

/** Where the sensor bar sits: "above" or "below" the screen. */
export const barPosition = () => (settings.sensorBarTop ? "above" : "below");

/** The launcher's pointer (src-tauri/src/wiimote/pointer.rs) and the game both follow it. */
export const setBar = (bar) => setSettings({ sensorBarTop: bar === "above" });

settingsReady.then(() => {
  try {
    const saved = localStorage.getItem(BAR_KEY);
    if (saved === null) return;
    localStorage.removeItem(BAR_KEY);
    if (saved === "above" && !settings.sensorBarTop) setBar("above").catch(() => {});
  } catch {}
});

let target = null; // where the remote points, before smoothing
let focused = null;
let hovered = null;
let focusedByPointer = false;
let lastSpot = null; // centre of the last picked control, to pick up from after a screen change
let repeat = null;
let running = false;

function focus(el, byPointer = false) {
  focusedByPointer = byPointer;
  if (el) lastSpot = centre(el);
  if (focused === el) return;
  focused?.removeAttribute("data-wm-focus");
  focused = el;
  el?.setAttribute("data-wm-focus", "");
}

const visible = (el) => {
  if (el.closest('[aria-hidden="true"]')) return false;
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth;
};

const centre = (el) => {
  const r = el.getBoundingClientRect();
  return [r.left + r.width / 2, r.top + r.height / 2];
};

/** Control whose centre is closest to that spot. */
function nearest(all, [x, y]) {
  const dist = (el) => Math.hypot(centre(el)[0] - x, centre(el)[1] - y);
  return all.reduce((best, el) => (!best || dist(el) < dist(best) ? el : best), null);
}

/** Nearest control in that direction, favouring ones straight ahead. */
function step([dx, dy]) {
  if (dx && focused instanceof HTMLInputElement && focused.type === "range" && focused.isConnected) {
    if (dx > 0) focused.stepUp();
    else focused.stepDown();
    focused.dispatchEvent(new Event("input", { bubbles: true }));
    focused.dispatchEvent(new Event("change", { bubbles: true }));
    return;
  }
  const all = [...document.querySelectorAll(FOCUSABLE)].filter(visible);
  if (!focused?.isConnected) {
    focus(nearest(all, lastSpot ?? [0, 0]));
    return;
  }
  const [cx, cy] = centre(focused);
  let best = null;
  let bestScore = Infinity;
  for (const el of all) {
    if (el === focused) continue;
    const [x, y] = centre(el);
    const along = (x - cx) * dx + (y - cy) * dy;
    if (along <= 1) continue;
    const score = along + 2 * Math.abs((x - cx) * dy - (y - cy) * dx);
    if (score < bestScore) [best, bestScore] = [el, score];
  }
  if (best) {
    focus(best);
    best.scrollIntoView({ block: "nearest", inline: "nearest" });
  }
}

/** A visible control carrying that remote button's badge. */
function badged(button) {
  const all = [...document.querySelectorAll(FOCUSABLE)].filter(visible);
  return all.find((el) => [...el.children].some((c) => c.getAttribute("aria-hidden") === "true" && c.textContent.trim() === button));
}

const STILL = window.matchMedia("(prefers-reduced-motion: reduce)");

function press(el) {
  // The hand doesn't trigger :active.
  if (!STILL.matches) el.animate([{ scale: 1 }, { scale: 0.94 }, { scale: 1 }], { duration: 160, easing: "ease-out" });
  if (el instanceof HTMLSelectElement) {
    el.selectedIndex = (el.selectedIndex + 1) % el.options.length;
    el.dispatchEvent(new Event("change", { bubbles: true }));
  } else if (el instanceof HTMLInputElement && !["checkbox", "radio", "button", "submit"].includes(el.type)) {
    el.focus();
  } else {
    el.click();
  }
}

function onButton({ button, down }) {
  if (button === "A") pointer.pressed = down;
  if (!down) {
    if (DIRECTIONS[button]) clearTimeout(repeat);
    return;
  }
  if (DIRECTIONS[button]) {
    clearTimeout(repeat);
    const again = (wait) => {
      step(DIRECTIONS[button]);
      repeat = setTimeout(() => again(REPEAT_EVERY), wait);
    };
    again(REPEAT_DELAY);
    return;
  }
  // A presses what the hand or the D-pad picked; any other button (and A when
  // nothing is picked) presses the control showing its badge.
  const picked = button === "A" && focused?.isConnected ? focused : badged(button);
  if (picked) press(picked);
  else if (button === "B") {
    if (location.hash && location.hash !== "#/Main") history.back();
  } else if (button === "HOME") location.hash = "#/Main";
}

const clamp = (v) => Math.min(Math.max(v, 0), 1);

/** seen.pos: where J1 points, -1 to 1 across the screen (beyond: off it), worked out
 *  in pointer.rs from the bar, the gyroscope or the tilt (seen.source). */
function onPointer(seen) {
  camera.dots = seen?.dots ?? [];
  camera.source = seen?.source ?? null;
  const pos = seen?.pos;
  if (!pos) {
    // Bar just out of view (aiming off-screen), or nothing to point with.
    target = null;
    pointer.shown = false;
    return;
  }
  const fx = (pos[0] + 1) / 2;
  const fy = (pos[1] + 1) / 2;
  // A little past the edges still shows the hand, pinned to the border.
  target = [clamp(fx) * innerWidth, clamp(fy) * innerHeight];
  if (!pointer.shown) [pointer.x, pointer.y] = target;
  pointer.shown = fx > -0.25 && fx < 1.25 && fy > -0.25 && fy < 1.25;
}

function frame() {
  if (!running) return;
  if (target && pointer.shown) {
    pointer.x += (target[0] - pointer.x) * SMOOTHING;
    pointer.y += (target[1] - pointer.y) * SMOOTHING;
    const el = document.elementFromPoint(pointer.x, pointer.y)?.closest(FOCUSABLE) ?? null;
    // Only a change of control under the hand moves the highlight, so the D-pad
    // still works while the hand rests somewhere.
    if (el !== hovered) {
      hovered = el;
      if (el) focus(el, true);
      else if (focusedByPointer) focus(null);
    }
  }
  requestAnimationFrame(frame);
}

/** Starts listening to J1; returns a function that stops it. */
export function startRemoteNav() {
  if (!inTauri) return () => {};
  const stops = [listen("wiimotes://button", (e) => onButton(e.payload)), listen("wiimotes://pointer", (e) => onPointer(e.payload))];
  const forget = () => {
    focus(null);
    hovered = null;
  };
  // New screen: keep the highlight where it was (the D-pad's, not the hand's,
  // which follows whatever is under it), once the screen has drawn.
  const keepSpot = () => {
    const byPointer = focusedByPointer;
    const spot = lastSpot;
    forget();
    lastSpot = spot;
    if (byPointer || !spot) return;
    requestAnimationFrame(() =>
      requestAnimationFrame(() => focus(nearest([...document.querySelectorAll(FOCUSABLE)].filter(visible), spot))),
    );
  };
  // J1 gone: hide the hand and stop any held D-pad.
  const stopWatch = $effect.root(() => {
    // Follows the screen shown, which a theme change switches too.
    $effect(() => {
      router.key;
      untrack(keepSpot);
    });
    $effect(() => {
      if (!wiimotes.slots[0]) {
        clearTimeout(repeat);
        onPointer(null);
        pointer.pressed = false;
      }
    });
  });
  running = true;
  requestAnimationFrame(frame);
  return () => {
    stops.forEach((p) => p.then((stop) => stop()));
    stopWatch();
    running = false;
    clearTimeout(repeat);
    forget();
  };
}
