import Link from 'next/link';
import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
import { blogSource, formatBlogDate, sortBlogPages } from '@/lib/blog';

export const metadata: Metadata = {
  title: '开发者博客',
  description: 'cursor-byok 的架构决策、协议实现与开发进展。',
};

export default function BlogPage() {
  const posts = sortBlogPages(blogSource.getPages());

  return (
    <main className="mx-auto w-full max-w-5xl px-6 py-16 sm:py-24">
      <div className="max-w-2xl">
        <p className="font-mono text-sm font-medium text-fd-primary">DEVELOPER BLOG</p>
        <h1 className="mt-4 text-4xl font-bold tracking-tight">开发者博客</h1>
        <p className="mt-4 text-lg leading-8 text-fd-muted-foreground">
          记录 cursor-byok 的架构决策、协议实现和开发进展。
        </p>
      </div>

      <div className="mt-12 divide-y border-y">
        {posts.map((post) => (
          <Link
            key={post.url}
            href={post.url}
            className="group grid gap-3 py-7 transition-colors hover:text-fd-primary sm:grid-cols-[10rem_1fr_auto] sm:items-center"
          >
            <time className="text-sm text-fd-muted-foreground">{formatBlogDate(post.path)}</time>
            <div>
              <h2 className="font-semibold">{post.data.title}</h2>
              <p className="mt-1 text-sm leading-6 text-fd-muted-foreground">
                {post.data.description}
              </p>
            </div>
            <ArrowRight className="hidden size-4 transition-transform group-hover:translate-x-1 sm:block" />
          </Link>
        ))}
      </div>
    </main>
  );
}
