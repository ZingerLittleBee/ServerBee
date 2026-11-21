import sitemap from '@astrojs/sitemap'
import { defineConfig, fontProviders } from 'astro/config'

/** Chinese text is set in the reader's system fonts: no web font covers it at a size worth downloading. */
const SANS = ['PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'system-ui', 'sans-serif']

/**
 * A Latin-only family from Fontsource. `fallbacks` ends in a generic family: until the web font loads, Astro stands
 * in the local font of that kind, scaled to the web font's metrics, so text keeps its line breaks when it arrives.
 */
function latin(
  name: string,
  cssVariable: string,
  weights: [string | number, ...(string | number)[]],
  fallbacks: string[] = SANS
) {
  return {
    provider: fontProviders.fontsource(),
    name,
    cssVariable,
    weights,
    fallbacks,
    styles: ['normal' as const] satisfies ['normal'],
    subsets: ['latin'] satisfies [string]
  }
}

export default defineConfig({
  // The canonical origin. Override it when the page is served from another domain.
  site: process.env.SITE_URL ?? 'https://serverbee.app',
  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'zh']
  },
  fonts: [
    latin('Sofia Sans', '--font-sofia', ['400 900']),
    latin('Sofia Sans Condensed', '--font-sofia-condensed', ['400 900']),
    // A proportional stand-in would wrap the install commands differently and move the page when the family loads.
    latin('IBM Plex Mono', '--font-plex-mono', [400, 500], ['monospace'])
  ],
  integrations: [sitemap({ i18n: { defaultLocale: 'en', locales: { en: 'en', zh: 'zh-CN' } } })]
})
