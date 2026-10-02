import { createFileRoute } from '@tanstack/react-router'

import { isDocsLanguage } from '@/lib/i18n'
import { llmsIndex, notFoundText, textResponse } from '@/lib/llms'

const GET = ({ params }: { params: { lang: string } }) =>
  isDocsLanguage(params.lang) ? textResponse(llmsIndex(params.lang)) : notFoundText()

export const Route = createFileRoute('/$lang/llms.txt')({
  server: { handlers: { GET, HEAD: GET } }
})
