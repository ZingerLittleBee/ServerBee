import { createFileRoute } from '@tanstack/react-router'

import { markdownResponse } from '@/lib/llms'

const GET = ({ params }: { params: { lang: string } }) => markdownResponse(params.lang, [])

/** The Markdown export of the docs index. */
export const Route = createFileRoute('/$lang/docs.mdx')({
  server: { handlers: { GET, HEAD: GET } }
})
