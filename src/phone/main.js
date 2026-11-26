// The page a phone opens to be a Wii Remote (src-tauri/src/wiimote/phone.rs serves it).
import { mount } from "svelte";
import "@fontsource/m-plus-rounded-1c/latin-500.css";
import "@fontsource/m-plus-rounded-1c/latin-700.css";
import "@fontsource/m-plus-rounded-1c/latin-800.css";
import "../app.css";
import Phone from "./Phone.svelte";

// The phone's own colours, light or dark.
const dark = matchMedia("(prefers-color-scheme: dark)");
const scheme = () => {
  document.documentElement.dataset.scheme = dark.matches ? "dark" : "light";
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", dark.matches ? "#141118" : "#efeeef");
};
scheme();
dark.addEventListener("change", scheme);
document.documentElement.lang = (navigator.language ?? "fr").slice(0, 2);
// iOS zooms on a pinch whatever the viewport says.
document.addEventListener("gesturestart", (e) => e.preventDefault());

export default mount(Phone, { target: document.getElementById("app") });
