import type { InferPageType } from 'fumadocs-core/source'

import { i18n } from './i18n'
import { source } from './source'

type DocsPage = InferPageType<typeof source>
type DocsLanguage = (typeof i18n.languages)[number]

export function isDocsLanguage(lang: string): lang is DocsLanguage {
  return (i18n.languages as string[]).includes(lang)
}

/** A page's Markdown export: its URL with `.mdx` appended, so the index is `/en/docs.mdx`. */
export function getPageMarkdownUrl(page: DocsPage): string {
  return `${page.url}.mdx`
}

export async function getLLMText(page: DocsPage): Promise<string> {
  const processed = await page.data.getText('processed')
  return `# ${page.data.title}\n\n${processed}`
}

const plainText = { 'Content-Type': 'text/plain; charset=utf-8' }

/** A server route that throws notFound() answers 200 with a JSON body, so the export routes return this instead. */
export function notFoundText(): Response {
  return new Response('Not found\n', { status: 404, headers: plainText })
}

/** The Markdown export of one page, or a 404 for an unknown language or page. */
export async function markdownResponse(lang: string, slugs: string[]): Promise<Response> {
  const page = isDocsLanguage(lang) ? source.getPage(slugs, lang) : undefined
  if (!page) {
    return notFoundText()
  }
  // text/markdown has no default charset (RFC 7763), so without one browsers decode the export as windows-1252.
  return new Response(await getLLMText(page), { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } })
}
