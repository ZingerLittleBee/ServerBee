import { createFileRoute, redirect } from '@tanstack/react-router'

import { i18n } from '@/lib/i18n'

export const Route = createFileRoute('/')({
  beforeLoad: ({ location }) => {
    // Straight to the docs home of the default language, rather than through /{lang}, which redirects there too. From
    // location.href, so that the query survives, as in the redirects of $lang.tsx.
    throw redirect({ href: `/${i18n.defaultLanguage}/docs${location.href.slice(1)}`, replace: true })
  }
})
