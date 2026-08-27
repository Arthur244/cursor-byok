import { zhCN } from '@fumadocs/language/zh-cn';
import { defineTranslations } from 'fumadocs-core/i18n';
import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { uiTranslations } from 'fumadocs-ui/i18n';
import { appName, gitConfig, releaseUrl } from './shared';

export const translations = defineTranslations().extend(uiTranslations()).preset(zhCN());

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: appName,
    },
    links: [
      {
        text: '文档',
        url: '/docs',
      },
      {
        text: '开发者博客',
        url: '/blog',
      },
      {
        text: '下载',
        url: releaseUrl,
        external: true,
      },
    ],
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
