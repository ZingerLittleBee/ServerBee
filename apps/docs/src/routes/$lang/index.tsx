import { createFileRoute, redirect } from '@tanstack/react-router'

const languagePath = /^\/[^/?#]*\/?/

export const Route = createFileRoute('/$lang/')({
  beforeLoad: ({ location, params }) => {
    // /{lang} was the landing page, which is now a site of its own (apps/landing), so the docs home answers here. The
    // redirect is temporary, so that this address can be pointed elsewhere later. The rest of the URL comes from
    // location.href, which keeps the request's query as it was sent, as in the redirects of $lang.tsx.
    throw redirect({ href: location.href.replace(languagePath, `/${params.lang}/docs`), replace: true })
  }
})
