import { createFileRoute, notFound, useParams } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import browserCollections from 'collections/browser'
import { useFumadocsLoader } from 'fumadocs-core/source/client'
import { DocsLayout } from 'fumadocs-ui/layouts/docs'
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  MarkdownCopyButton,
  ViewOptionsPopover
} from 'fumadocs-ui/layouts/docs/page'
import { type ComponentProps, type ComponentType, Suspense } from 'react'

import { useMDXComponents } from '@/components/mdx'
import { type DocsLanguage, i18n, isDocsLanguage, pathLanguage } from '@/lib/i18n'
import { baseOptions, gitConfig } from '@/lib/layout.shared'
import { getPageMarkdownUrl } from '@/lib/llms'
import { docsSiteName, languageLinks, notFoundHead, ogLocale, SITE, shareImageAlt } from '@/lib/site'
import { findPage, source } from '@/lib/source'

function getDocsContentPath(lang: string, slugs: string[]): string {
  return `${lang}/${slugs.length > 0 ? slugs.join('/') : 'index'}.mdx`
}

export const Route = createFileRoute('/$lang/docs/$')({
  component: Page,
  loader: async ({ params }) => {
    // The docs index has an empty splat, which splits into [''] rather than [].
    const slugs = params._splat?.split('/').filter(Boolean) ?? []
    const path = getDocsContentPath(params.lang, slugs)
    const preloadResult = clientLoader.preload(path).then(
      () => null,
      (error: unknown) => error
    )
    const [data, preloadError] = await Promise.all([
      serverLoader({ data: { slugs, lang: params.lang } }),
      preloadResult
    ])
    if (preloadError) {
      throw preloadError
    }
    return data
  },
  // After loader, which TanStack Router reads first to type loaderData.
  head: ({ loaderData, match }) => {
    if (loaderData) {
      return docsHead(loaderData)
    }
    return match.status === 'notFound' ? notFoundHead(pathLanguage(match.pathname)) : {}
  }
})

interface DocsHeadData {
  description?: string
  lang: DocsLanguage
  markdownUrl: string
  title: string
  url: string
}

function docsHead({ description, lang, markdownUrl, title, url }: DocsHeadData) {
  // The page's URL in another language swaps the language prefix.
  const inLanguage = (other: string) => `${SITE}/${other}${url.slice(lang.length + 1)}`
  const image = `${SITE}/og/landing-${lang}.png`
  const describe = description ? [description] : []
  return {
    meta: [
      { title: `${title} | ${docsSiteName[lang]}` },
      ...describe.map((content) => ({ name: 'description', content })),
      { property: 'og:type', content: 'article' },
      { property: 'og:site_name', content: 'ServerBee' },
      { property: 'og:url', content: `${SITE}${url}` },
      { property: 'og:title', content: title },
      ...describe.map((content) => ({ property: 'og:description', content })),
      { property: 'og:locale', content: ogLocale[lang] },
      ...i18n.languages
        .filter((code) => code !== lang)
        .map((code) => ({ property: 'og:locale:alternate', content: ogLocale[code] })),
      { property: 'og:image', content: image },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      { property: 'og:image:type', content: 'image/png' },
      { property: 'og:image:alt', content: shareImageAlt[lang] },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: title },
      ...describe.map((content) => ({ name: 'twitter:description', content })),
      { name: 'twitter:image', content: image },
      { name: 'twitter:image:alt', content: shareImageAlt[lang] }
    ],
    links: [...languageLinks(inLanguage, lang), { rel: 'alternate', type: 'text/markdown', href: markdownUrl }]
  }
}

const serverLoader = createServerFn({
  method: 'GET'
})
  .inputValidator((data: { slugs: string[]; lang: string }) => data)
  .handler(async ({ data: { slugs, lang } }) => {
    if (!isDocsLanguage(lang)) {
      throw notFound()
    }
    const page = findPage(slugs, lang)
    if (!page) {
      throw notFound()
    }

    const pageTree = source.getPageTree(lang)

    return {
      path: getDocsContentPath(lang, page.slugs),
      lang,
      url: page.url,
      title: page.data.title,
      description: page.data.description,
      markdownUrl: getPageMarkdownUrl(page),
      pageTree: await source.serializePageTree(pageTree)
    }
  })

const clientLoader = browserCollections.docs.createClientLoader({
  component(
    { toc, frontmatter, default: MDX },
    {
      markdownUrl,
      path
    }: {
      markdownUrl: string
      path: string
    }
  ) {
    return <DocsClientPage Content={MDX} frontmatter={frontmatter} markdownUrl={markdownUrl} path={path} toc={toc} />
  }
})

interface DocsClientPageProps {
  Content: ComponentType<{ components: ReturnType<typeof useMDXComponents> }>
  frontmatter: {
    description?: string
    title: string
  }
  markdownUrl: string
  path: string
  toc: ComponentProps<typeof DocsPage>['toc']
}

function DocsClientPage({ Content, frontmatter, markdownUrl, path, toc }: DocsClientPageProps) {
  const components = useMDXComponents()

  return (
    <DocsPage toc={toc}>
      <DocsTitle>{frontmatter.title}</DocsTitle>
      <DocsDescription>{frontmatter.description}</DocsDescription>
      <div className="-mt-4 flex flex-row items-center gap-2 border-b pb-6">
        <MarkdownCopyButton markdownUrl={markdownUrl} />
        <ViewOptionsPopover
          githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/apps/docs/content/docs/${path}`}
          markdownUrl={markdownUrl}
        />
      </div>
      <DocsBody>
        <Content components={components} />
      </DocsBody>
    </DocsPage>
  )
}

function Page() {
  const { path, pageTree, markdownUrl, lang } = useFumadocsLoader(Route.useLoaderData())
  const { lang: routeLang } = useParams({ from: '/$lang/docs/$' })
  const currentLang = lang ?? routeLang

  return (
    <DocsLayout {...baseOptions(currentLang)} tree={pageTree}>
      <Suspense>{clientLoader.useContent(path, { markdownUrl, path })}</Suspense>
    </DocsLayout>
  )
}
