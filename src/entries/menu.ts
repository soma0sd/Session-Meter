import "../lib/boot";
import { mount } from "svelte";
import Menu from "../menu/Menu.svelte";

const app = mount(Menu, { target: document.getElementById("app")! });
export default app;
