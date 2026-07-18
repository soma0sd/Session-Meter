// Theme handling for the frontend windows. The chosen theme (light/dark/system)
// comes from settings; "system" follows the OS. Rust also mirrors this for the tray icon.
import { writable } from "svelte/store";

export type Theme = "light" | "dark" | "system";

export const theme = writable<Theme>("system");

function prefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

export function resolveDark(t: Theme): boolean {
  return t === "dark" || (t === "system" && prefersDark());
}

// The theme setting last applied to this window, so watchSystemTheme knows whether an
// OS light/dark flip should re-render.
let currentTheme: Theme = "system";

export function applyTheme(t: Theme): void {
  currentTheme = t;
  document.documentElement.setAttribute(
    "data-theme",
    resolveDark(t) ? "dark" : "light",
  );
}

// Re-apply the theme when the OS light/dark preference flips while the setting is
// "system". Called once per window from boot.ts; the listener lives for the window.
let systemWatchBound = false;
export function watchSystemTheme(): void {
  if (systemWatchBound || typeof window === "undefined") return;
  systemWatchBound = true;
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      if (currentTheme === "system") applyTheme("system");
    });
}
