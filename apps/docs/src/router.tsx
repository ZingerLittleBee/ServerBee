import { createRouter as createTanStackRouter } from '@tanstack/react-router'

import { NotFound } from '@/components/not-found'
import { createScrollRestoration, keepFragmentEntries } from '@/lib/scroll-restoration'

import { routeTree } from './routeTree.gen'

/**
 * A query's params as strings, a repeated one as a list. The router parses them as JSON by default, and the server
 * redirects a request whose query does not come back the same, so ?v=1.10 went to ?v=1.1 and ?q="x" to ?q=x. The docs
 * read no params.
 */
function parseSearch(query: string): Record<string, string | string[]> {
  const search: Record<string, string | string[]> = Object.create(null)
  for (const [key, value] of new URLSearchParams(query)) {
    const previous = search[key]
    search[key] = previous === undefined ? value : [previous, value].flat()
  }
  return search
}

function stringifySearch(search: Record<string, unknown>): string {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(search)) {
    if (value !== undefined) {
      for (const item of [value].flat()) {
        params.append(key, String(item))
      }
    }
  }
  const query = params.toString()
  return query ? `?${query}` : ''
}

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
    rewrite: { output: ({ url }) => url },
    parseSearch,
    stringifySearch
  })
  if (typeof window !== 'undefined') {
    keepFragmentEntries(router)
  }
  return router
}
