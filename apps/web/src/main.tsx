import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { NuqsAdapter } from 'nuqs/adapters/tanstack-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import '@/lib/i18n'
import { router } from './router'
import { bootstrapLoader } from './widgets-runtime/loader'
import { mountRuntimeBridge } from './widgets-runtime/runtime-bridge'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, gcTime: 300_000, retry: 1 }
  }
})

mountRuntimeBridge({
  queryClient,
  serversStore: () => [],
  serverByIdStore: () => undefined,
  themeStore: () => ({
    mode: document.documentElement.classList.contains('dark') ? 'dark' : 'light',
    cssVar: (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim()
  }),
  onConfigUpdate: () => {}
})

// Best-effort: don't block rendering. Endpoint may return empty in Plan 1.
bootstrapLoader().catch((e) => console.warn('widget bootstrap failed', e))

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
