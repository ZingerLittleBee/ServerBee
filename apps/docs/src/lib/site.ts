import { type DocsLanguage, i18n } from './i18n'

/**
 * The canonical docs host, for page metadata and crawler-facing files rather than whichever host served a request.
 * /en/ redirects to /en, so page URLs carry no trailing slash.
 */
export const SITE = 'https://docs.serverbee.app'

export const docsSiteName: Record<DocsLanguage, string> = { en: 'ServerBee Docs', zh: 'ServerBee 文档' }

/** The heading of the 404 page (src/components/not-found.tsx), which its title also gives. */
export const notFoundTitle: Record<DocsLanguage, string> = { en: 'Page not found', zh: '页面不存在' }

/** The head of a 404 page, titled like a docs page so the title says what the page is. */
export function notFoundHead(lang: DocsLanguage) {
  return { meta: [{ title: `${notFoundTitle[lang]} | ${docsSiteName[lang]}` }] }
}

export const ogLocale: Record<DocsLanguage, string> = { en: 'en_US', zh: 'zh_CN' }

/** Alt text of the share image the landing and docs pages declare, public/og/landing-{lang}.png. */
export const shareImageAlt: Record<DocsLanguage, string> = {
  en: 'ServerBee: Self-hosted VPS monitoring, down to every route. A radar shows each server’s latency to Shanghai.',
  zh: 'ServerBee：自托管的 VPS 监控，细到每一条线路。雷达图显示各服务器到上海的延迟。'
}

/** The hreflang of each language: the Chinese pages are written in Simplified Chinese. */
export const hrefLangs: Record<DocsLanguage, string> = { en: 'en', zh: 'zh-Hans' }

/**
 * A page's hreflang alternates, from its absolute URL in each language. Every page exists in every language
 * (scripts/check-contracts.ts), and x-default is the default language, where / redirects.
 */
export function languageAlternates(url: (lang: DocsLanguage) => string) {
  return [
    ...i18n.languages.map((lang) => ({ hrefLang: hrefLangs[lang], href: url(lang) })),
    { hrefLang: 'x-default', href: url(i18n.defaultLanguage) }
  ]
}

/** A page's canonical link and its hreflang alternates, from its absolute URL in each language. */
export function languageLinks(url: (lang: DocsLanguage) => string, current: DocsLanguage) {
  return [
    { rel: 'canonical', href: url(current) },
    ...languageAlternates(url).map((alternate) => ({ rel: 'alternate', ...alternate }))
  ]
}
