// Hash router: "#/Library" shows src/screens/Library.svelte; "#/ModDetail/<id>"
// shows ModDetail.svelte with router.param "<id>".

const screens = import.meta.glob("../screens/*.svelte", { eager: true, import: "default" });

export const routes = Object.fromEntries(
  Object.entries(screens).map(([file, component]) => [file.match(/([^/]+)\.svelte$/)[1], component]),
);

const current = () => {
  const [name, ...rest] = location.hash.replace(/^#\//, "").split("/");
  if (!(name in routes)) return { name: "Main", param: null };
  return { name, param: rest.length ? decodeURIComponent(rest.join("/")) : null };
};

const hash = $state(current());

window.addEventListener("hashchange", () => {
  Object.assign(hash, current());
});

export const router = {
  get name() {
    return hash.name;
  },
  get param() {
    return hash.param;
  },
  /** Changes with the screen and with what it shows. */
  get key() {
    return `${hash.name}/${hash.param ?? ""}`;
  },
};
