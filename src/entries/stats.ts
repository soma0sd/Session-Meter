import "../lib/boot";
import { mount } from "svelte";
import Stats from "../stats/Stats.svelte";

const app = mount(Stats, { target: document.getElementById("app")! });
export default app;
