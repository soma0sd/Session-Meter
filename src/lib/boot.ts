// Side-effect boot shared by every window entry: load fonts + base styles and
// apply a best-effort initial theme (refined later from settings via IPC).
import "../styles/theme.css";
import "./fonts";
import { applyTheme, watchSystemTheme } from "./theme";

applyTheme("system");
// Follow live OS light/dark changes while the theme setting is "system".
watchSystemTheme();
