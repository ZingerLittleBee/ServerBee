import type { Node } from 'fumadocs-core/page-tree'
import type { InferPageType } from 'fumadocs-core/source'

import { type DocsLanguage, i18n, isDocsLanguage } from './i18n'
import { SITE } from './site'
import { findPage, source } from './source'

type DocsPage = InferPageType<typeof source>

const languageNames: Record<DocsLanguage, string> = { en: 'English', zh: '简体中文' }

/** The llms.txt prose, in the file's language. With two languages, "other" is unambiguous. */
const indexCopy: Record<DocsLanguage, { about: string; full: string; other: string }> = {
  en: {
    about: 'Each link is a documentation page as Markdown. Without `.mdx`, the same link opens the HTML page.',
    full: 'Every page above in one file',
    other: 'This documentation in Simplified Chinese'
  },
  zh: {
    about: '每个链接都是一篇文档的 Markdown 版本，去掉末尾的 `.mdx` 即为对应的 HTML 页面。',
    full: '以上全部页面合并为一个文件',
    other: '本文档的英文版'
  }
}

/** A page's Markdown export: its URL with `.mdx` appended, so the index is `/en/docs.mdx`. */
export function getPageMarkdownUrl(page: DocsPage): string {
  return `${page.url}.mdx`
}

/**
 * fumadocs-mdx 14.x wraps the Markdown serializer's handlers without their `peek`, so a bold span after plain text
 * comes out as `&#x2A;*text**`. fumadocs-core fixed the wrapper later (`Object.assign(wrapped, handler)`), but no 14.x
 * release of fumadocs-mdx bundles that fix.
 */
function restoreAttentionMarkers(markdown: string): string {
  return markdown.replaceAll('&#x2A;', '*')
}

/** A page as Markdown, with its URL so that a reader of llms-full.txt can cite and open it. */
export async function getLLMText(page: DocsPage): Promise<string> {
  const processed = restoreAttentionMarkers(await page.data.getText('processed')).trim()
  const description = page.data.description ? `> ${page.data.description}\n\n` : ''
  return `# ${page.data.title}\n\n${description}URL: ${SITE}${page.url}\n\n${processed}\n`
}

/** The llms.txt or llms-full.txt of a language. The default language's sit at the root, where they have always been. */
function llmsPath(lang: DocsLanguage, file: 'llms.txt' | 'llms-full.txt'): string {
  return lang === i18n.defaultLanguage ? `/${file}` : `/${lang}/${file}`
}

const plainText = { 'Content-Type': 'text/plain; charset=utf-8' }

export function textResponse(body: string): Response {
  return new Response(body, { headers: plainText })
}

/** A server route that throws notFound() answers 200 with a JSON body, so the export routes return this instead. */
export function notFoundText(): Response {
  return new Response('Not found\n', { status: 404, headers: plainText })
}

/** The Markdown export of one page, or a 404 for an unknown language or page. */
export async function markdownResponse(lang: string, slugs: string[]): Promise<Response> {
  const page = isDocsLanguage(lang) ? findPage(slugs, lang) : undefined
  if (!page) {
    return notFoundText()
  }
  // text/markdown has no default charset (RFC 7763), so without one browsers decode the export as windows-1252.
  return new Response(await getLLMText(page), { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } })
}

function escapeLinkText(text: string): string {
  return text.replace(/([[\]])/g, '\\$1')
}

/** An llms.txt (https://llmstxt.org): one H1, a summary, a note, and an H2 section of Markdown links per sidebar group. */
export function llmsIndex(lang: DocsLanguage): string {
  const lines = ['# ServerBee', '']
  const summary = source.getPage([], lang)?.data.description
  if (summary) {
    lines.push(`> ${summary}`, '')
  }
  // The reference parser (llms_txt) only reads the summary when something follows it before the first H2.
  lines.push(indexCopy[lang].about)
  const link = (page: DocsPage) => {
    const description = page.data.description ? `: ${page.data.description}` : ''
    return `- [${escapeLinkText(page.data.title)}](${SITE}${getPageMarkdownUrl(page)})${description}`
  }
  // llms.txt readers only collect links under an H2, so pages listed before the first sidebar group get one too.
  let inSection = false
  const section = (name: string) => {
    lines.push('', `## ${name}`, '')
    inSection = true
  }
  const visit = (node: Node) => {
    if (node.type === 'separator') {
      section(String(node.name))
    } else if (node.type === 'page') {
      const page = source.getNodePage(node, lang)
      if (page) {
        if (!inSection) {
          section(page.data.title)
        }
        lines.push(link(page))
      }
    } else {
      section(String(node.name))
      if (node.index) {
        visit(node.index)
      }
      node.children.forEach(visit)
    }
  }
  source.getPageTree(lang).children.forEach(visit)
  lines.push(
    '',
    '## Optional',
    '',
    `- [llms-full.txt](${SITE}${llmsPath(lang, 'llms-full.txt')}): ${indexCopy[lang].full}`
  )
  for (const other of i18n.languages) {
    if (other !== lang) {
      lines.push(`- [${languageNames[other]}](${SITE}${llmsPath(other, 'llms.txt')}): ${indexCopy[lang].other}`)
    }
  }
  return `${lines.join('\n').replace(/\n{3,}/g, '\n\n')}\n`
}

/** Pages in navigation order, then any page the navigation does not list. */
function orderedPages(lang: DocsLanguage): DocsPage[] {
  const pages: DocsPage[] = []
  const visit = (node: Node) => {
    if (node.type === 'page') {
      const page = source.getNodePage(node, lang)
      if (page) {
        pages.push(page)
      }
    } else if (node.type === 'folder') {
      if (node.index) {
        visit(node.index)
      }
      node.children.forEach(visit)
    }
  }
  source.getPageTree(lang).children.forEach(visit)
  const listed = new Set(pages.map((page) => page.url))
  return [...pages, ...source.getPages(lang).filter((page) => !listed.has(page.url))]
}

/** Every page of a language as Markdown, in navigation order. */
export async function llmsFull(lang: DocsLanguage): Promise<string> {
  const texts = await Promise.all(orderedPages(lang).map(getLLMText))
  return texts.join('\n')
}
