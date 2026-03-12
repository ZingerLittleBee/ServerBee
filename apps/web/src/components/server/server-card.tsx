import { Link } from '@tanstack/react-router'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  const value = bytes / 1024 ** i
  return `${value.toFixed(1)} ${units[i]}`
}

function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3600)
  if (days > 0) {
    return `${days}d ${hours}h`
  }
  const minutes = Math.floor((seconds % 3600) / 60)
  if (hours > 0) {
    return `${hours}h ${minutes}m`
  }
  return `${minutes}m`
}

function ProgressBar({ value, label, color }: { value: number; label: string; color: string }) {
  const pct = Math.min(100, Math.max(0, value))
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium">{pct.toFixed(1)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div className={cn('h-full rounded-full transition-all', color)} style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}

export function ServerCard({ server }: ServerCardProps) {
  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0

  return (
    <Link
      className="group block rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
      params={{ id: server.id }}
      to="/servers/$id"
    >
      <div className="mb-3 flex items-center justify-between">
        <h3 className="truncate font-semibold text-sm">{server.name}</h3>
        <StatusBadge online={server.online} />
      </div>

      <div className="space-y-2.5">
        <ProgressBar color="bg-chart-1" label="CPU" value={server.cpu} />
        <ProgressBar color="bg-chart-2" label="Memory" value={memoryPct} />
        <ProgressBar color="bg-chart-3" label="Disk" value={diskPct} />
      </div>

      <div className="mt-3 flex items-center justify-between text-muted-foreground text-xs">
        <div className="flex gap-3">
          <span title="Network In">{formatSpeed(server.net_in_speed)}</span>
          <span title="Network Out">{formatSpeed(server.net_out_speed)}</span>
        </div>
        <span title="Uptime">{formatUptime(server.uptime)}</span>
      </div>
    </Link>
  )
}
