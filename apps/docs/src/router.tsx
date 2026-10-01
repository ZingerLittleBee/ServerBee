import { createRouter as createTanStackRouter } from '@tanstack/react-router'

import { NotFound } from '@/components/not-found'
import { createScrollRestoration, keepFragmentEntries } from '@/lib/scroll-restoration'

import { routeTree } from './routeTree.gen'

export function getRouter() {
  const router = createTanStackRouter({
    routeTree,
    defaultPreload: 'intent',
    scrollRestoration: createScrollRestoration(),
    defaultNotFoundComponent: NotFound
  })
  if (typeof window !== 'undefined') {
    keepFragmentEntries(router)
  }
  return router
}
