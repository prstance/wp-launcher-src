import { mount } from "svelte";
import "@fontsource/m-plus-rounded-1c/latin-500.css";
import "@fontsource/m-plus-rounded-1c/latin-700.css";
import "@fontsource/m-plus-rounded-1c/latin-800.css";
import "./app.css";
import App from "./App.svelte";

export default mount(App, { target: document.getElementById("app") });
