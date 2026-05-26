import { createFileRoute, Outlet } from '@tanstack/react-router'
import { StatusHeader } from '@/components/status/status-header'
import { ScrollArea } from '@/components/ui/scroll-area'

export const Route = createFileRoute('/status')({
  component: StatusLayout
})

function StatusLayout() {
  return (
    <div className="flex h-full flex-col bg-background">
      <StatusHeader />
      <ScrollArea className="min-h-0 flex-1">
        <main className="mx-auto max-w-6xl px-4 py-8">
          <Outlet />
        </main>
      </ScrollArea>
    </div>
  )
}
