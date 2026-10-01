import { type DocsLanguage, i18n } from './i18n'

/**
 * The canonical docs host, for page metadata and crawler-facing files rather than whichever host served a request.
 * /en/ redirects to /en, so page URLs carry no trailing slash.
 */
export const SITE = 'https://docs.serverbee.app'

export const docsSiteName: Record<DocsLanguage, string> = { en: 'ServerBee Docs', zh: 'ServerBee 文档' }

export const ogLocale: Record<DocsLanguage, string> = { en: 'en_US', zh: 'zh_CN' }

/** The hreflang of each language: the Chinese pages are written in Simplified Chinese. */
export const hrefLangs: Record<DocsLanguage, string> = { en: 'en', zh: 'zh-Hans' }

/**
 * A page's canonical link and its hreflang alternates, from its absolute URL in each language. Every page exists in
 * every language (scripts/check-contracts.ts), and x-default is the default language, where / redirects.
 */
export function languageLinks(url: (lang: DocsLanguage) => string, current: DocsLanguage) {
  return [
    { rel: 'canonical', href: url(current) },
    ...i18n.languages.map((lang) => ({ rel: 'alternate', hrefLang: hrefLangs[lang], href: url(lang) })),
    { rel: 'alternate', hrefLang: 'x-default', href: url(i18n.defaultLanguage) }
  ]
}
