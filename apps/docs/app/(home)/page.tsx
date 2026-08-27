import Link from 'next/link';
import {
  ArrowRight,
  BookOpen,
  Download,
  Settings2,
  Wrench,
} from 'lucide-react';
import { DesktopDemo } from '@/components/hero/DesktopDemo';
import { blogSource, formatBlogDate, sortBlogPages } from '@/lib/blog';
import { appDescription, releaseUrl } from '@/lib/shared';

const docs = [
  {
    icon: BookOpen,
    title: '快速开始',
    description: '完成安装、初始化和第一次模型调用。',
    href: '/docs',
  },
  {
    icon: Settings2,
    title: '模型配置',
    description: '配置协议、服务地址、凭据和生成参数。',
    href: '/docs/model-configuration',
  },
  {
    icon: Wrench,
    title: '故障排查',
    description: '解决证书、连接和模型测试问题。',
    href: '/docs/troubleshooting',
  },
];

export default function HomePage() {
  const posts = sortBlogPages(blogSource.getPages()).slice(0, 3);

  return (
    <main className="flex flex-1 flex-col">
      <section className="border-b px-4 pb-16 pt-20 sm:px-6 sm:pb-24 sm:pt-28">
        <div className="mx-auto max-w-6xl">
          <div className="mx-auto max-w-3xl text-center">
            <p className="font-mono text-sm font-medium text-fd-primary">开源 · 本地运行 · 自由接入</p>
            <h1 className="mt-5 text-4xl font-bold tracking-tight sm:text-6xl">
              {appDescription}
            </h1>
            <p className="mx-auto mt-6 max-w-2xl text-lg leading-8 text-fd-muted-foreground">
              在本机运行自己的模型网关，接入 OpenAI、Anthropic 等兼容服务，继续使用 Cursor Agent 的工具调用、Skills 和 MCP。
            </p>
            <div className="mt-10 flex flex-wrap justify-center gap-3">
              <Link
                href="/docs"
                className="inline-flex items-center gap-2 rounded-lg bg-fd-primary px-5 py-3 font-medium text-fd-primary-foreground transition-opacity hover:opacity-90"
              >
                阅读文档
                <ArrowRight className="size-4" />
              </Link>
              <a
                href={releaseUrl}
                className="inline-flex items-center gap-2 rounded-lg border bg-fd-card px-5 py-3 font-medium transition-colors hover:bg-fd-accent"
              >
                <Download className="size-4" />
                下载最新版
              </a>
            </div>
          </div>
          <DesktopDemo />
        </div>
      </section>

      <section className="border-b px-6 py-16 sm:py-20">
        <div className="mx-auto max-w-5xl">
          <div className="flex items-end justify-between gap-6">
            <div>
              <p className="font-mono text-sm font-medium text-fd-primary">DOCUMENTATION</p>
              <h2 className="mt-3 text-3xl font-bold tracking-tight">文档</h2>
            </div>
            <Link href="/docs" className="hidden items-center gap-2 text-sm font-medium sm:flex">
              查看全部
              <ArrowRight className="size-4" />
            </Link>
          </div>

          <div className="mt-10 grid gap-4 md:grid-cols-3">
            {docs.map(({ icon: Icon, title, description, href }) => (
              <Link
                key={href}
                href={href}
                className="group rounded-xl border bg-fd-card p-6 transition-colors hover:bg-fd-accent"
              >
                <Icon className="mb-5 size-5 text-fd-primary" />
                <h3 className="flex items-center justify-between font-semibold">
                  {title}
                  <ArrowRight className="size-4 transition-transform group-hover:translate-x-1" />
                </h3>
                <p className="mt-2 text-sm leading-6 text-fd-muted-foreground">{description}</p>
              </Link>
            ))}
          </div>
        </div>
      </section>

      <section className="px-6 py-16 sm:py-20">
        <div className="mx-auto max-w-5xl">
          <div className="flex items-end justify-between gap-6">
            <div>
              <p className="font-mono text-sm font-medium text-fd-primary">DEVELOPER BLOG</p>
              <h2 className="mt-3 text-3xl font-bold tracking-tight">开发者博客</h2>
            </div>
            <Link href="/blog" className="flex items-center gap-2 text-sm font-medium">
              查看全部
              <ArrowRight className="size-4" />
            </Link>
          </div>

          <div className="mt-10 divide-y border-y">
            {posts.map((post) => (
              <Link
                key={post.url}
                href={post.url}
                className="group grid gap-3 py-6 transition-colors hover:text-fd-primary sm:grid-cols-[10rem_1fr_auto] sm:items-center"
              >
                <time className="text-sm text-fd-muted-foreground">{formatBlogDate(post.path)}</time>
                <div>
                  <h3 className="font-semibold">{post.data.title}</h3>
                  <p className="mt-1 text-sm text-fd-muted-foreground">{post.data.description}</p>
                </div>
                <ArrowRight className="hidden size-4 transition-transform group-hover:translate-x-1 sm:block" />
              </Link>
            ))}
          </div>
        </div>
      </section>
    </main>
  );
}
