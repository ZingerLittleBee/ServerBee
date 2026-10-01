import { createFileRoute } from '@tanstack/react-router'

import { i18n } from '@/lib/i18n'
import { llmsIndex, textResponse } from '@/lib/llms'

const GET = () => textResponse(llmsIndex(i18n.defaultLanguage))

export const Route = createFileRoute('/llms.txt')({
  server: { handlers: { GET, HEAD: GET } }
})
