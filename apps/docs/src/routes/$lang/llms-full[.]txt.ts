import { createFileRoute } from '@tanstack/react-router'

import { isDocsLanguage, llmsFull, notFoundText, textResponse } from '@/lib/llms'

const GET = async ({ params }: { params: { lang: string } }) =>
  isDocsLanguage(params.lang) ? textResponse(await llmsFull(params.lang)) : notFoundText()

export const Route = createFileRoute('/$lang/llms-full.txt')({
  server: { handlers: { GET, HEAD: GET } }
})
