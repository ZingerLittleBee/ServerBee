import { redirect } from '@tanstack/react-router'
import { createMiddleware, createStart } from '@tanstack/react-start'
import { rewritePath } from 'fumadocs-core/negotiation'

import { i18n } from '@/lib/i18n'

// /docs/<slugs>.mdx predates the localized export at /<lang>/docs/<slugs>.mdx; it always meant the English page.
const { rewrite: rewriteLegacyMarkdown } = rewritePath('/docs{/*path}.mdx', `/${i18n.defaultLanguage}/docs{/*path}.mdx`)

const llmMiddleware = createMiddleware().server(({ next, request }) => {
  const url = new URL(request.url)
  const path = rewriteLegacyMarkdown(url.pathname)

  if (path) {
    throw redirect({ href: new URL(`${path}${url.search}`, url).href, statusCode: 308 })
  }

  return next()
})

export const startInstance = createStart(() => {
  return {
    requestMiddleware: [llmMiddleware]
  }
})
