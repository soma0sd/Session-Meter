// Shared per-window bootstrap: pull settings + effective locale from Rust, apply the
// theme, and set the UI language. Guarded so the pages still render in a plain browser
// preview (no Tauri backend), falling back to system theme + navigator language.
import { getSettings, getEffectiveLocale, type Settings } from "./ipc";
import { applyTheme, theme as themeStore } from "./theme";
import { matchLocale, setLocale } from "./i18n";
import { settings as settingsStore } from "./stores";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  // A browser preview may select any supported language with ?lang=.
  const override = matchLocale(new URLSearchParams(location.search).get("lang") ?? "");
  if (override) {
    setLocale(override);
    return appearance;
  }
  if (isTauri()) {
    void listen<string>("locale://changed", ({ payload }) => {
      setLocale(matchLocale(payload) ?? "en");
    }).then((unlisten) => {
      window.addEventListener("pagehide", unlisten, { once: true });
    }).catch(() => {});
  }
  try {
    setLocale(await getEffectiveLocale());
  } catch {
    setLocale(matchLocale(navigator.language) ?? "en");
  }
  return appearance;
}
