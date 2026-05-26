import { createFileRoute, Outlet } from '@tanstack/react-router'
import { StatusHeader } from '@/components/status/status-header'

export const Route = createFileRoute('/status')({
  component: StatusLayout
})

function StatusLayout() {
  return (
    <div className="min-h-screen bg-background">
      <StatusHeader />
      <main className="mx-auto max-w-6xl px-4 py-8">
        <Outlet />
      </main>
    </div>
  )
}
