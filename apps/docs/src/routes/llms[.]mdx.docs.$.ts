import { createFileRoute } from '@tanstack/react-router'

import { i18n } from '@/lib/i18n'
import { markdownResponse } from '@/lib/llms'

const GET = ({ params }: { params: { _splat?: string } }) =>
  markdownResponse(i18n.defaultLanguage, params._splat?.split('/').filter(Boolean) ?? [])

/**
 * The first, English-only export URL, still fetched by pages built before the export moved to
 * /<lang>/docs/<slugs>.mdx and by anything that saved one of its links.
 */
export const Route = createFileRoute('/llms.mdx/docs/$')({
  server: { handlers: { GET, HEAD: GET } }
})
