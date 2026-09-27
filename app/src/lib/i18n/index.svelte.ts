import { systemLocale } from "$lib/api";
import en, { type Key } from "./en";
import es from "./es";
import ja from "./ja";
import ko from "./ko";
import zhHans from "./zh-Hans";
import zhHant from "./zh-Hant";

export type { Key };

export const DICTS = { en, ja, ko, "zh-Hans": zhHans, "zh-Hant": zhHant, es };
export type Locale = keyof typeof DICTS;

export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  ja: "日本語",
  ko: "한국어",
  "zh-Hans": "简体中文",
  "zh-Hant": "繁體中文",
  es: "Español",
};

/** The supported locale for a language tag such as "ja-JP" or "zh-Hant-TW"; English when none fits. */
export function matchLocale(tag: string): Locale {
  const t = tag.toLowerCase();
  if (t.startsWith("zh")) return /hant|-tw|-hk|-mo/.test(t) ? "zh-Hant" : "zh-Hans";
  return (["ja", "ko", "es"] as const).find((l) => t.startsWith(l)) ?? "en";
}

function saved(): Locale | null {
  try {
    const v = localStorage.getItem("locale");
    return v && v in DICTS ? (v as Locale) : null;
  } catch {
    return null;
  }
}

class I18n {
  system = $state<Locale>("en");
  /** The language picked in Settings; null follows the system. */
  choice = $state<Locale | null>(saved());
  locale = $derived(this.choice ?? this.system);

  async init() {
    const system = "__TAURI_INTERNALS__" in window ? await systemLocale().catch(() => null) : null;
    this.system = matchLocale(system ?? navigator.language);
  }

  pick(choice: Locale | null) {
    this.choice = choice;
    try {
      if (choice) localStorage.setItem("locale", choice);
      else localStorage.removeItem("locale");
    } catch {
      return;
    }
  }
}

export const i18n = new I18n();

/** UI text for `key` in the current language, with {name} placeholders filled; `<key>.one` is used when n is 1. */
export function t(key: Key, params: Record<string, string | number> = {}): string {
  const dict: Record<string, string> = DICTS[i18n.locale];
  const base: Record<string, string> = DICTS.en;
  const text = (params.n === 1 && (dict[`${key}.one`] ?? base[`${key}.one`])) || dict[key] || base[key] || key;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? ""));
}
