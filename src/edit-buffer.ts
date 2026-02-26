import "./app.css";
import EditBuffer from "$lib/components/EditBuffer.svelte";
import { mount } from "svelte";

const app = mount(EditBuffer, {
  target: document.getElementById("app")!,
});

export default app;
