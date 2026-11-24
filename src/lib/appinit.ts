// Shared per-window bootstrap: pull settings + effective locale from Rust, apply the
// theme, and set the UI language. Guarded so the pages still render in a plain browser
// preview (no Tauri backend), falling back to system theme + navigator language.
import { getSettings, getEffectiveLocale, type Settings } from "./ipc";
import { applyTheme, theme as themeStore } from "./theme";
import { setLocale, type Locale } from "./i18n";
import { settings as settingsStore } from "./stores";

export async function initWindow(): Promise<Settings | null> {
  const appearance = getSettings().then((s) => {
    settingsStore.set(s);
    themeStore.set(s.theme);
    applyTheme(s.theme);
    return s;
  }).catch(() => {
    applyTheme("system");
    return null;
  });
  // Dev/preview override: ?lang=ko or ?lang=en.
  const override = new URLSearchParams(location.search).get("lang");
  if (override === "ko" || override === "en") {
    setLocale(override);
    return appearance;
  }
  try {
    const loc = await getEffectiveLocale();
    setLocale(loc as Locale);
  } catch {
    const nav = (navigator.language || "en").toLowerCase();
    setLocale(nav.startsWith("ko") ? "ko" : "en");
  }
  return appearance;
}
