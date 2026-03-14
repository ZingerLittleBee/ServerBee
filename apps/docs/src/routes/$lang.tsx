import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'

import { i18n } from '@/lib/i18n'

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ params }) => {
    if (!(i18n.languages as string[]).includes(params.lang)) {
      throw redirect({ to: '/$lang', params: { lang: i18n.defaultLanguage }, replace: true })
    }
  },
  component: () => <Outlet />
})
