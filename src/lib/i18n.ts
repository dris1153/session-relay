import { useSyncExternalStore } from "react";
import en from "../locales/en.json";
import vi from "../locales/vi.json";

export type Language = "vi" | "en";

const dictionaries: Record<Language, Record<string, string>> = { vi, en };
const listeners = new Set<() => void>();
let current: Language = navigator.language.toLowerCase().startsWith("vi") ? "vi" : "en";
document.documentElement.lang = current;

export function setLanguage(language: Language) {
  if (!(language in dictionaries)) return;
  current = language;
  document.documentElement.lang = language;
  listeners.forEach((notify) => notify());
}

/** Re-renders the caller when the language changes. */
export function useLanguage(): Language {
  return useSyncExternalStore(
    (notify) => {
      listeners.add(notify);
      return () => listeners.delete(notify);
    },
    () => current,
  );
}

/** Looks up `key` in the active language (falls back to English), filling `{name}` params. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const text = dictionaries[current][key] ?? dictionaries.en[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`));
}

/** Localized message for a command error code. */
export function errorText(code: string): string {
  const key = `errors.${code}`;
  return t(key) === key ? t("errors.unknown", { code }) : t(key);
}
