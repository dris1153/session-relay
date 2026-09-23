import { setLanguage, t, useLanguage, type Language } from "../lib/i18n";
import { api } from "../lib/tauri-commands";

const LANGUAGES: Language[] = ["vi", "en"];

export function LanguageSwitch() {
  const current = useLanguage();
  const choose = (language: Language) => {
    setLanguage(language);
    api.saveSettings({ language }).catch(() => {});
  };
  return (
    <div role="group" aria-label={t("common.language")} className="fixed right-6 top-6 flex gap-1 text-caption font-medium uppercase tracking-wide">
      {LANGUAGES.map((language) => (
        <button
          key={language}
          type="button"
          aria-pressed={current === language}
          onClick={() => choose(language)}
          className={`rounded-control px-2 py-1 transition-colors ${current === language ? "bg-soft-stone text-carbon-ink" : "text-pebble hover:text-graphite"}`}
        >
          {language}
        </button>
      ))}
    </div>
  );
}
