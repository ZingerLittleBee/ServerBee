import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { api } from '@/lib/api-client'
import type { ServiceStatusConfig } from '@/lib/widget-types'

interface ServiceMonitor {
  config_json: string
  consecutive_failures: number
  created_at: string
  enabled: boolean
  id: string
  interval: number
  last_checked_at: string | null
  last_status: boolean | null
  monitor_type: string
  name: string
  notification_group_id: string | null
  retry_count: number
  server_ids_json: string | null
  target: string
  updated_at: string
}

interface ServiceStatusWidgetProps {
  config: ServiceStatusConfig
}

function getStatusColor(monitor: ServiceMonitor): string {
  if (monitor.last_status === null) {
    return 'bg-gray-400'
  }
  if (monitor.last_status === true) {
    if (monitor.consecutive_failures > 0) {
      return 'bg-yellow-500'
    }
    return 'bg-green-500'
  }
  return 'bg-red-500'
}

function getStatusLabel(monitor: ServiceMonitor): string {
  if (monitor.last_status === null) {
    return 'Pending'
  }
  if (monitor.last_status === true) {
    if (monitor.consecutive_failures > 0) {
      return 'Degraded'
    }
    return 'Healthy'
  }
  return 'Down'
}

function formatCheckTime(isoString: string | null): string {
  if (!isoString) {
    return 'Never'
  }
  const diff = Math.max(0, Math.floor((Date.now() - new Date(isoString).getTime()) / 1000))
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

export function ServiceStatusWidget({ config }: ServiceStatusWidgetProps) {
  const monitorIds = config.monitor_ids

  const { data: monitors } = useQuery<ServiceMonitor[]>({
    queryKey: ['service-monitors'],
    queryFn: () => api.get<ServiceMonitor[]>('/api/service-monitors'),
    refetchInterval: 60_000
  })

  const filtered = useMemo(() => {
    if (!monitors) {
      return []
    }
    if (!monitorIds || monitorIds.length === 0) {
      return monitors.filter((m) => m.enabled)
    }
    const idSet = new Set(monitorIds)
    return monitors.filter((m) => idSet.has(m.id))
  }, [monitors, monitorIds])

  if (!monitors) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">Service Status</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">Loading...</div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">Service Status</h3>
      <div className="flex flex-1 flex-wrap content-start gap-2 overflow-auto">
        {filtered.map((monitor) => (
          <div
            className="group relative inline-flex items-center justify-center"
            key={monitor.id}
            title={`${monitor.name} - ${getStatusLabel(monitor)} - Last check: ${formatCheckTime(monitor.last_checked_at)}`}
          >
            <span className={`inline-block size-3.5 rounded-full ${getStatusColor(monitor)} cursor-default`} />
            <div className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-2 hidden -translate-x-1/2 rounded-md bg-popover px-2.5 py-1.5 shadow-md ring-1 ring-border group-hover:block">
              <div className="whitespace-nowrap text-xs">
                <p className="font-medium">{monitor.name}</p>
                <p className="text-muted-foreground">
                  {getStatusLabel(monitor)} &middot; {formatCheckTime(monitor.last_checked_at)}
                </p>
              </div>
            </div>
          </div>
        ))}
        {filtered.length === 0 && (
          <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
            No service monitors
          </div>
        )}
      </div>
    </div>
  )
}
