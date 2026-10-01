import { defineI18n } from 'fumadocs-core/i18n'

export const i18n = defineI18n({
  languages: ['en', 'zh'],
  defaultLanguage: 'en',
  parser: 'dir'
})

export type DocsLanguage = (typeof i18n.languages)[number]

export function isDocsLanguage(lang: string): lang is DocsLanguage {
  return (i18n.languages as string[]).includes(lang)
}

/** The language of a URL path: its first segment, or the default language when that is not one. */
export function pathLanguage(pathname: string): DocsLanguage {
  const segment = pathname.split('/').filter(Boolean)[0] ?? ''
  return isDocsLanguage(segment) ? segment : i18n.defaultLanguage
}
