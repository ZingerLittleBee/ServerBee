import { useMemo } from 'react'
import { Badge } from '@/components/ui/badge'
import type { DockerEventInfo } from '../types'

interface DockerEventsProps {
  events: DockerEventInfo[]
}

function formatRelativeTime(timestamp: number): string {
  const now = Date.now() / 1000
  const diff = Math.max(0, Math.floor(now - timestamp))

  if (diff < 60) {
    return `${diff}s ago`
  }
  if (diff < 3600) {
    return `${Math.floor(diff / 60)}m ago`
  }
  if (diff < 86_400) {
    return `${Math.floor(diff / 3600)}h ago`
  }
  return `${Math.floor(diff / 86_400)}d ago`
}

function eventTypeBadgeVariant(eventType: string): 'default' | 'secondary' | 'outline' | 'destructive' {
  switch (eventType) {
    case 'container':
      return 'default'
    case 'image':
      return 'secondary'
    case 'network':
      return 'outline'
    case 'volume':
      return 'outline'
    default:
      return 'secondary'
  }
}

export function DockerEvents({ events }: DockerEventsProps) {
  const sortedEvents = useMemo(() => {
    return [...events].sort((a, b) => b.timestamp - a.timestamp)
  }, [events])

  if (sortedEvents.length === 0) {
    return (
      <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-muted-foreground text-sm">No events recorded yet</p>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <h3 className="font-semibold text-lg">Events</h3>
      <div className="max-h-[400px] space-y-1 overflow-y-auto rounded-lg border p-3">
        {sortedEvents.map((event, idx) => (
          <div
            className="flex items-start gap-3 rounded-md px-3 py-2 text-sm odd:bg-muted/30"
            key={`${String(idx)}-${event.timestamp}-${event.event_type}-${event.action}`}
          >
            <span className="w-16 shrink-0 text-right text-muted-foreground text-xs tabular-nums">
              {formatRelativeTime(event.timestamp)}
            </span>
            <Badge className="w-20 justify-center" variant={eventTypeBadgeVariant(event.event_type)}>
              {event.event_type}
            </Badge>
            <span className="font-medium">{event.action}</span>
            <span className="truncate text-muted-foreground">{event.actor_name ?? event.actor_id.slice(0, 12)}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
