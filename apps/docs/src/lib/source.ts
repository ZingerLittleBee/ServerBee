import { docs } from 'collections/server'
import { loader } from 'fumadocs-core/source'
import { lucideIconsPlugin } from 'fumadocs-core/source/lucide-icons'

import { type DocsLanguage, i18n } from './i18n'

export const source = loader({
  i18n,
  source: docs.toFumadocsSource(),
  baseUrl: '/docs',
  plugins: [lucideIconsPlugin()]
})

/**
 * The page at the slugs of a URL, which the router has decoded. fumadocs decodes slugs that name no page, as Next.js
 * passes them still encoded, and decoding them again threw on a `%` (a 500 for /en/docs/x%25y) or named another page
 * (the quick start, for /en/docs/%2571uick-start.mdx). Encoded first, they come back from that decoding as they were.
 */
export function findPage(slugs: string[], lang: DocsLanguage) {
  return source.getPage(slugs.map(encodeURI), lang)
}
