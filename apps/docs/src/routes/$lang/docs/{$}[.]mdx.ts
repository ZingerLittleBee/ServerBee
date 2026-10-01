import { createFileRoute } from '@tanstack/react-router'

import { markdownResponse } from '@/lib/llms'

const GET = ({ params }: { params: { lang: string; _splat?: string } }) =>
  markdownResponse(params.lang, params._splat?.split('/').filter(Boolean) ?? [])

/** The Markdown export of a page, at the page URL plus `.mdx`. */
export const Route = createFileRoute('/$lang/docs/{$}.mdx')({
  // Without HEAD the request falls through to the app router, which answers with an HTML page.
  server: { handlers: { GET, HEAD: GET } }
})
