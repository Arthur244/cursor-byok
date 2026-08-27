import Link from 'next/link';
import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { ArrowLeft } from 'lucide-react';
import { InlineTOC } from 'fumadocs-ui/components/inline-toc';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import { getMDXComponents } from '@/components/mdx';
import { blogSource, formatBlogDate } from '@/lib/blog';

export default async function BlogPostPage(props: PageProps<'/blog/[slug]'>) {
  const { slug } = await props.params;
  const page = blogSource.getPage([slug]);
  if (!page) notFound();

  const MDX = page.data.body;

  return (
    <main className="mx-auto w-full max-w-3xl px-6 py-12 sm:py-20">
      <article>
        <Link
          href="/blog"
          className="mb-10 inline-flex items-center gap-2 text-sm text-fd-muted-foreground transition-colors hover:text-fd-foreground"
        >
          <ArrowLeft className="size-4" />
          返回开发者博客
        </Link>

        <header className="border-b pb-8">
          <time className="text-sm text-fd-muted-foreground">{formatBlogDate(page.path)}</time>
          <h1 className="mt-4 text-3xl font-bold tracking-tight sm:text-4xl">{page.data.title}</h1>
          <p className="mt-4 text-lg leading-8 text-fd-muted-foreground">{page.data.description}</p>
          <p className="mt-5 text-sm font-medium">cursor-byok 开发团队</p>
        </header>

        <div className="prose mt-10 min-w-0">
          <InlineTOC items={page.data.toc} />
          <MDX
            components={getMDXComponents({
              a: createRelativeLink(blogSource, page),
            })}
          />
        </div>
      </article>
    </main>
  );
}

export function generateStaticParams() {
  return blogSource.getPages().map((page) => ({ slug: page.slugs[0] }));
}

export async function generateMetadata(props: PageProps<'/blog/[slug]'>): Promise<Metadata> {
  const { slug } = await props.params;
  const page = blogSource.getPage([slug]);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
  };
}
