import "../lib/boot";
import { mount } from "svelte";
import Widget from "../widget/Widget.svelte";

const app = mount(Widget, { target: document.getElementById("app")! });
export default app;
