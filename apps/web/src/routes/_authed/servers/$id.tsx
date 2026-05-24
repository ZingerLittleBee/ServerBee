import { createFileRoute } from '@tanstack/react-router'
import { lazy, Suspense } from 'react'
import { Skeleton } from '@/components/ui/skeleton'

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
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  })
})
