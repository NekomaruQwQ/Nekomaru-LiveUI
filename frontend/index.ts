import { mount } from "svelte";

import "./debug";
import App from "./src/App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("Missing #app element");

// Replace the spinner placeholder before mounting so the loading state doesn't
// flash alongside the rendered app.
target.replaceChildren();

mount(App, { target });
