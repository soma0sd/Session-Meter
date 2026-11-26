// Shared UI translations for the desktop windows and the browser preview.
import { writable, derived, type Readable } from "svelte/store";
import en from "./locales/en.json";
import ko from "./locales/ko.json";
import ja from "./locales/ja.json";
import zhCN from "./locales/zh-CN.json";
import de from "./locales/de.json";
import es from "./locales/es.json";

export const localeNames = {
  ko: "한국어",
  en: "English",
  ja: "日本語",
  "zh-CN": "简体中文",
  de: "Deutsch",
  es: "Español",
};
export type Locale = keyof typeof localeNames;
export type Language = Locale | "auto";

export const localeTags: Record<Locale, string> = {
  ko: "ko-KR",
  en: "en-US",
  ja: "ja-JP",
  "zh-CN": "zh-CN",
  de: "de-DE",
  es: "es-ES",
};

const completeDicts = {
  en, ko, ja, "zh-CN": zhCN, de, es,
} satisfies Record<Locale, Record<keyof typeof en, string>>;
const dicts: Record<Locale, Record<string, string>> = completeDicts;

export function matchLocale(value: string): Locale | undefined {
  const [language, ...subtags] = value.trim().toLowerCase().split(/[-_]/);
  switch (language) {
    case "ko":
    case "en":
    case "ja":
    case "de":
    case "es":
      return language;
    case "zh":
      return subtags.some((tag) => ["hant", "tw", "hk", "mo"].includes(tag)) ? undefined : "zh-CN";
    default:
      return undefined;
  }
}

export const locale = writable<Locale>("en");

export function setLocale(value: Locale): void {
  locale.set(value);
  document.documentElement.lang = localeTags[value];
}

export type TFn = (key: string, vars?: Record<string, string | number>) => string;

export function translate(value: Locale, key: string, vars?: Record<string, string | number>): string {
  let text = dicts[value][key] ?? en[key as keyof typeof en] ?? key;
  if (vars) {
    for (const [name, substitution] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(substitution));
    }
  }
  return text;
}

export const t: Readable<TFn> = derived(locale, (value): TFn => {
  return (key, vars) => translate(value, key, vars);
});
