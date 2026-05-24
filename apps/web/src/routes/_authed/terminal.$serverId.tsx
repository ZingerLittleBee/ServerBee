import { createFileRoute } from '@tanstack/react-router'
import { lazy, Suspense } from 'react'
import { Skeleton } from '@/components/ui/skeleton'

const LazyTerminalPage = lazy(() => import('./terminal.$serverId-page').then((m) => ({ default: m.TerminalPage })))

function TerminalPageShell() {
  return (
    <Suspense
      fallback={
        <div className="flex min-h-0 min-w-0 flex-1 flex-col p-4">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="mt-3 flex-1" />
        </div>
      }
    >
      <LazyTerminalPage />
    </Suspense>
  )
}

export const Route = createFileRoute('/_authed/terminal/$serverId')({
  component: TerminalPageShell
})
