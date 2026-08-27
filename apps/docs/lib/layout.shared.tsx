import { zhCN } from '@fumadocs/language/zh-cn';
import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { uiTranslations } from 'fumadocs-ui/i18n';
import { i18n, type Language } from './i18n';
import { appName, gitConfig, releaseUrl } from './shared';

export const translations = i18n.translations().extend(uiTranslations()).preset('zh', zhCN());

const navLabels: Record<Language, { docs: string; blog: string; download: string }> = {
  zh: { docs: '文档', blog: '开发者博客', download: '下载' },
  en: { docs: 'Docs', blog: 'Blog', download: 'Download' },
};

export function baseOptions(lang: Language): BaseLayoutProps {
  const labels = navLabels[lang];
  const prefix = lang === i18n.defaultLanguage ? '' : `/${lang}`;

  return {
    nav: {
      title: appName,
    },
    links: [
      {
        text: labels.docs,
        url: `${prefix}/docs`,
      },
      {
        text: labels.blog,
        url: `${prefix}/blog`,
      },
      {
        text: labels.download,
        url: releaseUrl,
        external: true,
      },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
