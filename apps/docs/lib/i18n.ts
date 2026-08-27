import { defineI18n } from 'fumadocs-core/i18n';

export const i18n = defineI18n({
  defaultLanguage: 'zh',
  languages: ['zh', 'en'],
  hideLocale: 'default-locale',
});

export type Language = (typeof i18n)['languages'][number];

export function isLanguage(value: string): value is Language {
  return i18n.languages.some((lang) => lang === value);
}

/** HTML `lang` attribute values. */
export const htmlLang: Record<Language, string> = {
  zh: 'zh-CN',
  en: 'en',
};

/** Locale ids understood by the desktop app / demo. */
export const demoLocale: Record<Language, string> = {
  zh: 'zh-CN',
  en: 'en-US',
};
