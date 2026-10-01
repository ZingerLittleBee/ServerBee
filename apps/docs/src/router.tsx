import { createRouter as createTanStackRouter } from '@tanstack/react-router'

import { NotFound } from '@/components/not-found'
import { createScrollRestoration, keepFragmentEntries } from '@/lib/scroll-restoration'

import { routeTree } from './routeTree.gen'

export function getRouter() {
  const router = createTanStackRouter({
    routeTree,
    defaultPreload: 'intent',
    scrollRestoration: createScrollRestoration(),
    defaultNotFoundComponent: NotFound,
    // On the server, the router redirects a request whose URL differs from the one it builds for it. It built a path's
    // `"` `<` `>` `^` `` ` `` `{` `}` unencoded, and the browser encodes them again to follow the redirect, so such a path
    // redirected to itself until the browser gave up. Given a rewrite, even one that changes nothing, the router builds
    // URLs with the URL parser, which encodes them as the request's URL was.
    rewrite: { output: ({ url }) => url }
  })
  if (typeof window !== 'undefined') {
    keepFragmentEntries(router)
  }
  return router
}
