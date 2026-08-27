import { RootProvider } from 'fumadocs-ui/provider/next';
import { i18nProvider } from 'fumadocs-ui/i18n';
import type { Metadata } from 'next';
import { translations } from '@/lib/layout.shared';
import './global.css';

const siteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? 'https://docs.leokun.cn';

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: 'cursor-byok 文档',
    template: '%s | cursor-byok',
  },
  description: 'cursor-byok 的安装、模型配置与故障排查指南。',
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <RootProvider i18n={i18nProvider(translations)}>{children}</RootProvider>
      </body>
    </html>
  );
}
