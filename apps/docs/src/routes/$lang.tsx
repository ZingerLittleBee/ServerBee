import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'

import { i18n, isDocsLanguage } from '@/lib/i18n'
import { docsSiteName } from '@/lib/site'

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ params }) => {
    if (!(i18n.languages as string[]).includes(params.lang)) {
      throw redirect({ to: '/$lang', params: { lang: i18n.defaultLanguage }, replace: true })
    }
  },
  // The title of a page without its own, such as a 404 in Chinese, in the page's language.
  head: ({ params }) => ({
    meta: [{ title: docsSiteName[isDocsLanguage(params.lang) ? params.lang : i18n.defaultLanguage] }]
  }),
  component: () => <Outlet />
})
