import { createFileRoute } from '@tanstack/react-router'

import { type DocsLanguage, i18n } from '@/lib/i18n'
import { languageAlternates, SITE } from '@/lib/site'
import { source } from '@/lib/source'

const xmlEscapes: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' }
const xmlSpecial = /[&<>"']/g

function escapeXml(value: string): string {
  return value.replace(xmlSpecial, (char) => xmlEscapes[char])
}

/** A page's entry in one language, with the hreflang alternates of the page's head. */
function urlEntry(url: (lang: DocsLanguage) => string, lang: DocsLanguage): string {
  const links = languageAlternates(url).map(
    ({ hrefLang, href }) => `<xhtml:link rel="alternate" hreflang="${hrefLang}" href="${escapeXml(href)}"/>`
  )
  return `<url><loc>${escapeXml(url(lang))}</loc>${links.join('')}</url>`
}

function sitemap(): Response {
  const urls = i18n.languages.map((lang) => urlEntry((code) => `${SITE}/${code}`, lang))
  for (const lang of i18n.languages) {
    for (const page of source.getPages(lang)) {
      // The page's URL in another language swaps the language prefix, as in its head.
      urls.push(urlEntry((code) => `${SITE}/${code}${page.url.slice(lang.length + 1)}`, lang))
    }
  }
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls.join('\n')}
</urlset>
`
  return new Response(xml, { headers: { 'Content-Type': 'application/xml; charset=utf-8' } })
}

export const Route = createFileRoute('/sitemap.xml')({
  server: {
    handlers: { GET: sitemap, HEAD: sitemap }
  }
})
