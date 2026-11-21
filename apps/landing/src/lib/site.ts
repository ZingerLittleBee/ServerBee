/* Addresses and commands the landing page points at. Plain TypeScript, so scripts can import it without Vite. */

export type Lang = 'en' | 'zh'

export const LANGS: readonly Lang[] = ['en', 'zh']

/** The documentation site. It is deployed separately, so every link to it is absolute. */
export const DOCS_SITE = 'https://docs.serverbee.app'

const REPOSITORY = 'https://github.com/ZingerLittleBee/ServerBee'
const INSTALLER = 'curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh'

export const links = {
  github: REPOSITORY,
  releases: `${REPOSITORY}/releases`,
  railway: 'https://railway.com/deploy/serverbee-server',
  status: 'https://demo.serverbee.app/status'
} as const

export const installCommands = {
  docker: `${INSTALLER} | sudo sh -s -- server --method docker -y`,
  binary: `${INSTALLER} | sudo sh -s -- server --method binary -y`,
  agent: `${INSTALLER} | sudo bash -s -- agent --server-url 'https://panel.example.com' --enrollment-code '<enrollment-code>'`
} as const

export type FeatureKey = 'dash' | 'ios' | 'ipq' | 'network' | 'security'

/** Feature sections in page order: Chinese readers come for network quality, English readers for dashboards. */
export const featureOrder: Record<Lang, FeatureKey[]> = {
  en: ['dash', 'network', 'ipq', 'security', 'ios'],
  zh: ['network', 'ipq', 'security', 'dash', 'ios']
}

/** Documentation pages the landing links to. scripts/check-contracts.ts requires each one in both locales. */
export const docsPages = [
  'capabilities',
  'dashboards',
  'ip-quality',
  'mobile',
  'monitoring',
  'quick-start',
  'testing'
] as const
export type DocsPage = (typeof docsPages)[number]

/** A documentation page, optionally at a heading: `hash` is the heading's id, without the '#'. */
export function docsUrl(lang: Lang, page: DocsPage, hash?: string): string {
  return `${DOCS_SITE}/${lang}/docs/${page}${hash ? `#${encodeURIComponent(hash)}` : ''}`
}

/** The landing page of a locale. English is the default locale and lives at the root. */
export function homePath(lang: Lang): string {
  return lang === 'en' ? '/' : `/${lang}/`
}
