import "../lib/boot";
import { mount } from "svelte";
import Style from "../style/Style.svelte";

const app = mount(Style, { target: document.getElementById("app")! });
export default app;
