import { docs } from 'collections/server'
import { type InferPageType, loader } from 'fumadocs-core/source'
import { lucideIconsPlugin } from 'fumadocs-core/source/lucide-icons'

import { i18n } from './i18n'

export const source = loader({
  i18n,
  source: docs.toFumadocsSource(),
  baseUrl: '/docs',
  plugins: [lucideIconsPlugin()]
})

export async function getLLMText(page: InferPageType<typeof source>) {
  const processed = await page.data.getText('processed')

  return `# ${page.data.title}

${processed}`
}
