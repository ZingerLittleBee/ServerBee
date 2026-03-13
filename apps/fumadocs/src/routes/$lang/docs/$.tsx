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
import { Suspense } from 'react'

import { useMDXComponents } from '@/components/mdx'
import { baseOptions, gitConfig } from '@/lib/layout.shared'
import { localizePageTree, source } from '@/lib/source'

export const Route = createFileRoute('/$lang/docs/$')({
  component: Page,
  loader: async ({ params }) => {
    const slugs = params._splat?.split('/') ?? []
    const data = await serverLoader({ data: { slugs, lang: params.lang } })
    await clientLoader.preload(data.path)
    return data
  }
})

const serverLoader = createServerFn({
  method: 'GET'
})
  .inputValidator((data: { slugs: string[]; lang: string }) => data)
  .handler(async ({ data: { slugs, lang } }) => {
    const page = source.getPage(slugs, lang)
    if (!page) {
      throw notFound()
    }

    const pageTree = source.getPageTree(lang)
    const localizedTree = localizePageTree(pageTree, lang)

    return {
      slugs: page.slugs,
      path: page.path,
      lang,
      pageTree: await source.serializePageTree(localizedTree)
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
    // biome-ignore lint/correctness/useHookAtTopLevel: this method is a component function used by createClientLoader
    const components = useMDXComponents()
    return (
      <DocsPage toc={toc}>
        <DocsTitle>{frontmatter.title}</DocsTitle>
        <DocsDescription>{frontmatter.description}</DocsDescription>
        <div className="-mt-4 flex flex-row items-center gap-2 border-b pb-6">
          <MarkdownCopyButton markdownUrl={markdownUrl} />
          <ViewOptionsPopover
            githubUrl={`https://github.com/${gitConfig.user}/${gitConfig.repo}/blob/${gitConfig.branch}/apps/fumadocs/content/docs/${path}`}
            markdownUrl={markdownUrl}
          />
        </div>
        <DocsBody>
          <MDX components={components} />
        </DocsBody>
      </DocsPage>
    )
  }
})

function Page() {
  const { path, pageTree, slugs, lang } = useFumadocsLoader(Route.useLoaderData())
  const { lang: routeLang } = useParams({ from: '/$lang/docs/$' })
  const currentLang = lang ?? routeLang
  const markdownUrl = `/llms.mdx/docs/${slugs.join('/')}`

  return (
    <DocsLayout {...baseOptions(currentLang)} tree={pageTree}>
      <Suspense>{clientLoader.useContent(path, { markdownUrl, path })}</Suspense>
    </DocsLayout>
  )
}
