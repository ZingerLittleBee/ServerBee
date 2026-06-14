import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import i18next from 'i18next'
import { NuqsAdapter } from 'nuqs/adapters/tanstack-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { toast } from 'sonner'
import './index.css'
import '@/lib/i18n'
import { router } from './router'
import { mountRuntimeBridge } from './widgets-runtime/runtime-bridge'

const queryClient = new QueryClient({
  // Surface load failures globally. Without this, a failed query that a component
  // only branches on via isLoading/data silently renders an empty/zero state,
  // making a backend or auth error look like "no data". Only toast when there is
  // no cached data to fall back on (initial load), so background-refetch failures
  // on a populated screen stay silent. A stable id collapses a burst of
  // simultaneous failures (e.g. the whole page on a dropped connection) into one.
  queryCache: new QueryCache({
    onError: (_error, query) => {
      if (query.state.data === undefined) {
        toast.error(i18next.t('errors.data_load_failed'), { id: 'query-load-error' })
      }
    }
  }),
  defaultOptions: {
    queries: { staleTime: 30_000, gcTime: 300_000, retry: 1 }
  }
})

mountRuntimeBridge({ queryClient })

const root = document.getElementById('root')
if (root) {
  createRoot(root).render(
    <StrictMode>
      <NuqsAdapter>
        <QueryClientProvider client={queryClient}>
          <RouterProvider router={router} />
        </QueryClientProvider>
      </NuqsAdapter>
    </StrictMode>
  )
}
