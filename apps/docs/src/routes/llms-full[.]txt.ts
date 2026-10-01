import { createFileRoute } from '@tanstack/react-router'

import { i18n } from '@/lib/i18n'
import { llmsFull, textResponse } from '@/lib/llms'

const GET = async () => textResponse(await llmsFull(i18n.defaultLanguage))

export const Route = createFileRoute('/llms-full.txt')({
  server: { handlers: { GET, HEAD: GET } }
})
