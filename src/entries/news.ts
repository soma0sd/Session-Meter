import "../lib/boot";
import { mount } from "svelte";
import News from "../news/News.svelte";

const app = mount(News, { target: document.getElementById("app")! });
export default app;
