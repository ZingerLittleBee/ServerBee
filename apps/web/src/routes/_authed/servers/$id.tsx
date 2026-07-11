import { createFileRoute } from '@tanstack/react-router'
import { lazy, Suspense } from 'react'
import { Skeleton } from '@/components/ui/skeleton'
import { parseServerDetailSearch } from '@/lib/server-detail-nav'

const LazyServerDetailPage = lazy(() => import('./$id-page').then((m) => ({ default: m.ServerDetailPage })))

function ServerDetailPageShell() {
  return (
    <Suspense
      fallback={
        <div className="space-y-6">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-4 w-96" />
          <div className="grid gap-4 lg:grid-cols-2">
            <Skeleton className="h-64" />
            <Skeleton className="h-64" />
          </div>
        </div>
      }
    >
      <LazyServerDetailPage />
    </Suspense>
  )
}

export const Route = createFileRoute('/_authed/servers/$id')({
  component: ServerDetailPageShell,
  // Persist the active tab in the URL so reload / shared link / browser back keeps
  // the same tab (e.g. Security) instead of resetting to Metrics. `tab` is optional
  // so existing links to /servers/$id (range only) stay valid; the page defaults to
  // the metrics tab when it is absent.
  validateSearch: parseServerDetailSearch
})
