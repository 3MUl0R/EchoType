import "./app.css";
import Overlay from "$lib/components/Overlay.svelte";
import { mount } from "svelte";

const app = mount(Overlay, {
  target: document.getElementById("app")!,
});

export default app;
