// The phone as a Wii Remote (src-tauri/src/wiimote/phone.rs): its motion and the buttons on
// its screen go to the launcher, whose emulated remote answers with its LEDs, rumble and sound.

const G = 9.80665;
const RAD = Math.PI / 180;

/** Core button bits, as the remote reports them (protocol.rs). */
export const B = { up: 0x0800, down: 0x0400, left: 0x0100, right: 0x0200, a: 0x0008, b: 0x0004, plus: 0x1000, home: 0x0080, minus: 0x0010, one: 0x0002, two: 0x0001 };

/**
 * status: "idle" (not started) | "connecting" | "connected" | "lost" (the launcher doesn't answer) | "bye"
 * bye: why the launcher let the phone go: "removed", "full", "stopped"
 * motion: null until known, then whether the browser gives the phone's motion
 * turn: how far the screen turned the page (°), which the remote turns back
 */
export const remote = $state({ status: "idle", bye: null, leds: 0, rumble: false, buttons: 0, motion: null, talking: false, turn: 0 });

/** The pairing token, from the QR code's address (kept for a bookmark without it). */
const token = (() => {
  const fromAddress = location.hash.slice(1);
  try {
    if (/^[0-9a-f]{32}$/.test(fromAddress)) localStorage.setItem("wp-token", fromAddress);
    return /^[0-9a-f]{32}$/.test(fromAddress) ? fromAddress : localStorage.getItem("wp-token");
  } catch {
    return fromAddress || null;
  }
})();

let ws = null;
let retry = null;
let failures = 0;
let started = false;
let audio = null;
let playAt = 0;
let talkTimer = null;
let tellBattery = null;
let lastSent = 0;

// The remote's axes: x to its left, y to its back, z out of its face. The phone's screen is the
// face and its top edge the front, which points at the TV: x = −X, y = −Y, z = Z of the phone's.
let accel = [0, 0, 1];
let gyro = [0, 0, 0];
const packet = new Float32Array(7);

// Browsers disagree on the motion they give. Chrome on Android puts rotationRate's three turns in
// another order than the spec's (alpha about Z, beta about X, gamma about Y), and some count the
// accelerometer the other way round. Both are checked against the orientation the browser also
// gives, whose turns and gravity every browser agrees on. Where it can, the Gyroscope sensor
// (Chrome) gives the turns about x, y and z, no guessing.
let up = null;
let accelVote = 0;
/** For alpha, beta and gamma: the phone's axis they turn about (0 X, 1 Y, 2 Z) and which way. */
const SPEC = [[2, 1], [0, 1], [1, 1]];
const MAP_KEY = "wp-rotation-rate";
let map = (() => {
  try {
    const saved = JSON.parse(localStorage.getItem(MAP_KEY));
    return Array.isArray(saved) && saved.length === 3 ? saved : SPEC;
  } catch {
    return SPEC;
  }
})();
/** How much each of alpha, beta, gamma went with turns about X, Y, Z, recently. */
const votes = [0, 1, 2].map(() => [0, 0, 0]);
let lastRates = null;
let turned = null;
let turnedAt = 0;
let gyroscope = null;

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The orientation as a quaternion [w, x, y, z]: Z by alpha, then X by beta, then Y by gamma. */
function quaternion(alpha, beta, gamma) {
  const half = (deg) => [Math.cos((deg * RAD) / 2), Math.sin((deg * RAD) / 2)];
  const [cz, sz] = half(alpha);
  const [cx, sx] = half(beta);
  const [cy, sy] = half(gamma);
  // qz · qx · qy
  return [cz * cx * cy - sz * sx * sy, cz * sx * cy - sz * cx * sy, cz * cx * sy + sz * sx * cy, sz * cx * cy + cz * sx * sy];
}

/** The turn from quaternion a to b, in the phone's frame (rad). */
function turnBetween(a, b) {
  // conj(a) · b
  let w = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  let x = a[0] * b[1] - a[1] * b[0] - a[2] * b[3] + a[3] * b[2];
  let y = a[0] * b[2] + a[1] * b[3] - a[2] * b[0] - a[3] * b[1];
  let z = a[0] * b[3] - a[1] * b[2] + a[2] * b[1] - a[3] * b[0];
  if (w < 0) [w, x, y, z] = [-w, -x, -y, -z];
  const s = Math.hypot(x, y, z);
  if (s < 1e-9) return [0, 0, 0];
  const angle = 2 * Math.atan2(s, w);
  return [(x / s) * angle, (y / s) * angle, (z / s) * angle];
}

/** Each of alpha, beta, gamma goes to the axis it moved with most, once that is clear. */
function decide() {
  const next = votes.map((row) => {
    const order = [0, 1, 2].sort((i, j) => Math.abs(row[j]) - Math.abs(row[i]));
    const [best, second] = [Math.abs(row[order[0]]), Math.abs(row[order[1]])];
    return best > 25 && best > 3 * second ? [order[0], Math.sign(row[order[0]])] : null;
  });
  if (next.some((m) => !m) || new Set(next.map(([axis]) => axis)).size !== 3) return;
  if (JSON.stringify(next) === JSON.stringify(map)) return;
  map = next;
  try {
    localStorage.setItem(MAP_KEY, JSON.stringify(map));
  } catch {}
}

function onOrientation(e) {
  if (e.beta == null || e.gamma == null) return;
  const [b, g] = [e.beta * RAD, e.gamma * RAD];
  // Up, in the phone's frame.
  up = [-Math.sin(g) * Math.cos(b), Math.sin(b), Math.cos(g) * Math.cos(b)];
  if (e.alpha == null || gyroscope) return;
  const q = quaternion(e.alpha, e.beta, e.gamma);
  const dt = (e.timeStamp - turnedAt) / 1000;
  if (turned && lastRates && dt > 0.005 && dt < 0.1) {
    const w = turnBetween(turned, q).map((v) => v / dt);
    const speed = Math.hypot(...w);
    // Turning, but not through a jump of the angles (rad/s).
    if (speed > 0.5 && speed < 12) {
      for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) votes[i][j] = votes[i][j] * 0.995 + lastRates[i] * RAD * w[j];
      decide();
    }
  }
  turned = q;
  turnedAt = e.timeStamp;
}

/** Turns about the phone's X, Y, Z (°/s) to the remote's. */
const toRemote = (g) => [-g[0], -g[1], g[2]];

function onMotion(e) {
  const a = e.accelerationIncludingGravity;
  if (a?.x == null) return;
  remote.motion = true;
  const v = [a.x / G, a.y / G, a.z / G];
  const g = Math.hypot(...v);
  if (up && g > 0.7 && g < 1.3) accelVote = clamp(accelVote + Math.sign(dot(v, up)), -30, 30);
  const s = accelVote < -6 ? -1 : 1;
  accel = [-v[0] * s, -v[1] * s, v[2] * s];
  const r = e.rotationRate;
  if (r?.alpha != null && !gyroscope) {
    lastRates = [r.alpha, r.beta, r.gamma];
    const phone = [0, 0, 0];
    map.forEach(([axis, sign], i) => (phone[axis] = lastRates[i] * sign));
    gyro = toRemote(phone);
  }
  send();
}

/** The Gyroscope sensor, where there is one: turns about x, y, z in rad/s. */
function startGyroscope() {
  if (typeof window.Gyroscope !== "function") return;
  try {
    const sensor = new window.Gyroscope({ frequency: 60, referenceFrame: "device" });
    sensor.addEventListener("reading", () => {
      gyroscope = sensor;
      gyro = toRemote([sensor.x, sensor.y, sensor.z].map((v) => v / RAD));
    });
    sensor.addEventListener("error", () => (gyroscope = null));
    sensor.start();
  } catch {}
}

function send() {
  if (ws?.readyState !== WebSocket.OPEN) return;
  packet[0] = remote.buttons;
  packet.set(accel, 1);
  packet.set(gyro, 4);
  ws.send(packet);
  lastSent = performance.now();
}

const sendJson = (message) => ws?.readyState === WebSocket.OPEN && ws.send(JSON.stringify(message));

/** The buttons held now: sent at once. */
export function press(buttons) {
  if (buttons === remote.buttons) return;
  // A little tap for each new press (Android), unless it rumbles.
  if (buttons & ~remote.buttons && !remote.rumble) navigator.vibrate?.(8);
  remote.buttons = buttons;
  send();
}

function rumble(on) {
  remote.rumble = on;
  navigator.vibrate?.(on ? 30000 : 0);
}

/** The remote's speaker: samples at their own rate, on the phone's. */
function play(buffer) {
  if (!audio || buffer.byteLength <= 8) return;
  const head = new DataView(buffer);
  const rate = head.getUint32(0, true);
  const volume = head.getFloat32(4, true);
  const samples = new Int16Array(buffer, 8, (buffer.byteLength - 8) >> 1);
  if (!rate || !samples.length) return;
  const n = samples.length;
  const length = Math.max(1, Math.round((n * audio.sampleRate) / rate));
  const chunk = audio.createBuffer(1, length, audio.sampleRate);
  const out = chunk.getChannelData(0);
  for (let i = 0; i < length; i++) {
    const p = (i * rate) / audio.sampleRate;
    const k = Math.floor(p);
    const a = samples[Math.min(k, n - 1)];
    const b = samples[Math.min(k + 1, n - 1)];
    out[i] = ((a + (b - a) * (p - k)) / 32768) * volume;
  }
  // Queued after the last one; started again a little ahead when late or too far ahead.
  const now = audio.currentTime;
  if (playAt < now + 0.02 || playAt > now + 0.4) playAt = now + 0.06;
  const source = audio.createBufferSource();
  source.buffer = chunk;
  source.connect(audio.destination);
  source.start(playAt);
  playAt += chunk.duration;
  remote.talking = true;
  clearTimeout(talkTimer);
  talkTimer = setTimeout(() => (remote.talking = false), 160);
}

function control(message) {
  if ("leds" in message) remote.leds = message.leds;
  if ("rumble" in message) rumble(message.rumble);
  if ("bye" in message) remote.bye = message.bye;
}

function connect() {
  clearTimeout(retry);
  if (!token || ws) return;
  remote.bye = null;
  if (remote.status !== "lost") remote.status = "connecting";
  const socket = new WebSocket(`wss://${location.host}/remote?t=${token}`);
  socket.binaryType = "arraybuffer";
  ws = socket;
  socket.onopen = () => {
    failures = 0;
    remote.status = "connected";
    tellBattery?.();
    send();
  };
  socket.onmessage = (e) => (typeof e.data === "string" ? control(JSON.parse(e.data)) : play(e.data));
  socket.onclose = () => {
    ws = null;
    rumble(false);
    remote.leds = 0;
    // Let go on purpose: the player decides when to come back. Silent too long (hidden,
    // locked): back as soon as it can.
    if (remote.bye && remote.bye !== "silent") {
      remote.status = "bye";
      return;
    }
    failures++;
    remote.status = failures >= 3 ? "lost" : "connecting";
    if (document.visibilityState === "visible") retry = setTimeout(connect, 1500);
  };
}

export const reconnect = connect;

/** Lets the launcher go. */
export function leave() {
  remote.bye = "left";
  ws?.close();
}

async function keepAwake() {
  try {
    await navigator.wakeLock?.request("screen");
  } catch {}
}

/** The page's rotation, which the remote turns back: it stays fixed on the phone like the buttons of a real one. */
function followTurn() {
  const turn = () => {
    const angle = screen.orientation?.angle ?? window.orientation ?? 0;
    remote.turn = ((angle % 360) + 360) % 360;
  };
  turn();
  screen.orientation?.addEventListener?.("change", turn);
  addEventListener("orientationchange", turn);
  addEventListener("resize", turn);
}

/** Starts it all, from a tap: the browser only lets motion, sound and full screen start from one. */
export function start() {
  if (started) return;
  started = true;
  const Audio = window.AudioContext ?? window.webkitAudioContext;
  if (Audio) {
    audio = new Audio();
    audio.resume?.();
  }
  document.documentElement
    .requestFullscreen?.({ navigationUI: "hide" })
    .then(() => screen.orientation?.lock?.("portrait-primary"))
    .catch(() => {});
  // iOS asks once for the motion sensors.
  const asks = [window.DeviceMotionEvent, window.DeviceOrientationEvent].filter((E) => typeof E?.requestPermission === "function").map((E) => E.requestPermission());
  Promise.all(asks)
    .then((answers) => {
      if (answers.some((a) => a !== "granted")) remote.motion = false;
    })
    .catch(() => (remote.motion = false))
    .finally(() => {
      addEventListener("devicemotion", onMotion);
      addEventListener("deviceorientation", onOrientation);
      startGyroscope();
      setTimeout(() => (remote.motion ??= false), 2000);
    });
  navigator
    .getBattery?.()
    .then((battery) => {
      tellBattery = () => sendJson({ battery: Math.round(battery.level * 100) });
      battery.addEventListener("levelchange", tellBattery);
      tellBattery();
    })
    .catch(() => {});
  keepAwake();
  followTurn();
  connect();
  // Without motion (a computer's browser), the buttons still need to say they're alive.
  setInterval(() => performance.now() - lastSent > 200 && send(), 250);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "visible") return;
    keepAwake();
    audio?.resume?.();
    if (!ws && remote.status !== "bye") connect();
  });
}

export const hasToken = () => !!token;
