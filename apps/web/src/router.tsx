import { createRouter } from '@tanstack/react-router'
import { NotFound } from './components/not-found'
import { routeTree } from './routeTree.gen'

export const router = createRouter({ routeTree, defaultNotFoundComponent: NotFound })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
