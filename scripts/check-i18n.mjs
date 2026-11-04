// Lists the texts given to t() and plural() (and to tr! in the backend) that a
// locale doesn't translate, and the translations nothing uses any more.
//   node scripts/check-i18n.mjs
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const files = (dir) => readdirSync(dir).flatMap((f) => (statSync(join(dir, f)).isDirectory() ? files(join(dir, f)) : [join(dir, f)]));
const sources = files("src").filter((f) => /\.(svelte|js)$/.test(f) && !f.includes("locales"));

// t('…') / t("…") and the two French forms of plural(n, '…', '…').
const STRING = String.raw`(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"|\x60((?:[^\x60\\$]|\\.)*)\x60)`;
const CALL = new RegExp(String.raw`\b(?:t\(\s*${STRING}|plural\([^,]+,\s*${STRING}\s*,\s*${STRING})`, "g");
const unescape = (s) => s.replace(/\\(.)/g, "$1");

const used = new Map();
for (const file of sources) {
  for (const m of readFileSync(file, "utf8").matchAll(CALL)) {
    for (const text of m.slice(1).filter((s) => s !== undefined)) used.set(unescape(text), file);
  }
}

let missing = 0;
for (const code of ["en", "es"]) {
  const dictionary = (await import(`../src/locales/${code}.js`)).default;
  for (const [text, file] of used) {
    if (!(text in dictionary)) {
      missing++;
      console.log(`${code}: manque « ${text} » (${file})`);
    }
  }
  for (const text of Object.keys(dictionary)) if (!used.has(text)) console.log(`${code}: inutilisé « ${text} »`);
}
// The backend: tr!("…") against the (French, English, Spanish) of src-tauri/src/i18n.rs.
const rust = files("src-tauri/src").filter((f) => f.endsWith(".rs"));
const RUST_STRING = String.raw`"((?:[^"\\]|\\.)*)"`;
const table = readFileSync("src-tauri/src/i18n.rs", "utf8").split("const TEXTS")[1].split("\n];")[0];
const translated = new Set([...table.matchAll(new RegExp(String.raw`^\s*\(${RUST_STRING},`, "gm"))].map((m) => unescape(m[1])));
const told = new Map();
for (const file of rust) {
  const code = readFileSync(file, "utf8").replace(/^\s*\/\/.*$/gm, "");
  for (const m of code.matchAll(new RegExp(String.raw`\btr!\(\s*${RUST_STRING}`, "g"))) told.set(unescape(m[1]), file);
}
for (const [text, file] of told) {
  if (!translated.has(text)) {
    missing++;
    console.log(`rust: manque « ${text} » (${file})`);
  }
}
for (const text of translated) if (!told.has(text)) console.log(`rust: inutilisé « ${text} »`);

console.log(`${used.size + told.size} textes, ${missing} traductions manquantes`);
process.exit(missing ? 1 : 0);
