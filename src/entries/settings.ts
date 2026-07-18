import "../lib/boot";
import { mount } from "svelte";
import Settings from "../settings/Settings.svelte";

const app = mount(Settings, { target: document.getElementById("app")! });
export default app;
