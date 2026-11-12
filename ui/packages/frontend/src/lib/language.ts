import {
  getLocale,
  isLocale,
  type Locale,
  locales,
  setLocale,
} from "@/paraglide/runtime.js";

// Endonyms: each language is named in its own language.
const localeNames: Record<Locale, string> = { en: "English" };

export const languageOptions = locales.map((locale) => ({
  value: locale,
  label: localeNames[locale],
}));

export const currentLanguage = (): string => getLocale();

// Reloads the page: Paraglide messages are read at render time.
export function changeLanguage(value: string): void {
  if (isLocale(value) && value !== getLocale()) setLocale(value);
}
