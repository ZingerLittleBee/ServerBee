import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, Globe, Server } from 'lucide-react'
import { StatusBadge } from '@/components/server/status-badge'
import { api } from '@/lib/api-client'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/status')({
  component: StatusPage
})

interface StatusMetrics {
  cpu: number
  disk_total: number
  disk_used: number
  load1: number
  mem_total: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  uptime: number
}

interface StatusServer {
  country_code: string | null
  group_id: string | null
  id: string
  metrics: StatusMetrics | null
  name: string
  online: boolean
  os: string | null
  public_remark: string | null
  region: string | null
}

interface StatusGroup {
  id: string
  name: string
}

interface StatusPageResponse {
  groups: StatusGroup[]
  online_count: number
  servers: StatusServer[]
  total_count: number
}

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / 1024 ** i).toFixed(1)} ${units[i]}`
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

function ProgressBar({ value, label, color }: { color: string; label: string; value: number }) {
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

function ServerStatusCard({ server }: { server: StatusServer }) {
  const m = server.metrics
  const memPct = m && m.mem_total > 0 ? (m.mem_used / m.mem_total) * 100 : 0
  const diskPct = m && m.disk_total > 0 ? (m.disk_used / m.disk_total) * 100 : 0

  return (
    <div className="rounded-lg border bg-card p-4 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 truncate">
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
          {server.region && (
            <span className="shrink-0 text-muted-foreground text-xs">
              {server.country_code && `${server.country_code} · `}
              {server.region}
            </span>
          )}
        </div>
        <StatusBadge className="shrink-0" online={server.online} />
      </div>

      {server.public_remark && <p className="mb-3 text-muted-foreground text-xs">{server.public_remark}</p>}

      {m ? (
        <>
          <div className="space-y-2.5">
            <ProgressBar color="bg-chart-1" label="CPU" value={m.cpu} />
            <ProgressBar color="bg-chart-2" label="Memory" value={memPct} />
            <ProgressBar color="bg-chart-3" label="Disk" value={diskPct} />
          </div>
          <div className="mt-3 flex items-center justify-between text-muted-foreground text-xs">
            <div className="flex gap-3">
              <span title="Network In">{formatSpeed(m.net_in_speed)}</span>
              <span title="Network Out">{formatSpeed(m.net_out_speed)}</span>
            </div>
            <span title="Uptime">{formatUptime(m.uptime)}</span>
          </div>
        </>
      ) : (
        <div className="flex h-24 items-center justify-center text-muted-foreground text-xs">
          {server.os && <span className="mr-2">{server.os}</span>}
          {!server.online && <span>Offline</span>}
        </div>
      )}
    </div>
  )
}

function StatusPage() {
  const { data, isLoading, error } = useQuery<StatusPageResponse>({
    queryKey: ['status'],
    queryFn: () => api.get<StatusPageResponse>('/api/status'),
    refetchInterval: 10_000
  })

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="mx-auto flex max-w-6xl items-center justify-between px-4 py-4">
          <div className="flex items-center gap-2">
            <Server className="size-5 text-primary" />
            <span className="font-semibold text-lg">ServerBee</span>
          </div>
          {data && (
            <div className="flex items-center gap-4 text-sm">
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Globe className="size-4" />
                {data.total_count} servers
              </span>
              <span className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
                <Activity className="size-4" />
                {data.online_count} online
              </span>
            </div>
          )}
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-4 py-8">
        {isLoading && (
          <div className="flex min-h-[300px] items-center justify-center">
            <div className="space-y-4 text-center">
              <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
              <p className="text-muted-foreground text-sm">Loading status...</p>
            </div>
          </div>
        )}

        {error && (
          <div className="flex min-h-[300px] items-center justify-center">
            <p className="text-destructive text-sm">Failed to load status data</p>
          </div>
        )}

        {data && <StatusContent data={data} />}
      </main>
    </div>
  )
}

function StatusContent({ data }: { data: StatusPageResponse }) {
  const groupMap = new Map(data.groups.map((g) => [g.id, g.name]))

  const grouped = new Map<string, StatusServer[]>()
  for (const server of data.servers) {
    const key = server.group_id ?? '__ungrouped__'
    const list = grouped.get(key)
    if (list) {
      list.push(server)
    } else {
      grouped.set(key, [server])
    }
  }

  // Sort: named groups first (by group name), ungrouped last
  const sortedKeys = [...grouped.keys()].sort((a, b) => {
    if (a === '__ungrouped__') {
      return 1
    }
    if (b === '__ungrouped__') {
      return -1
    }
    const nameA = groupMap.get(a) ?? ''
    const nameB = groupMap.get(b) ?? ''
    return nameA.localeCompare(nameB)
  })

  if (data.servers.length === 0) {
    return (
      <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
        <p className="text-muted-foreground text-sm">No servers available</p>
      </div>
    )
  }

  // If there's only one group (or only ungrouped), skip the group heading
  const showGroupHeaders = sortedKeys.length > 1 || !sortedKeys.includes('__ungrouped__')

  return (
    <div className="space-y-8">
      {sortedKeys.map((key) => {
        const servers = grouped.get(key) ?? []
        const groupName = key === '__ungrouped__' ? 'Ungrouped' : (groupMap.get(key) ?? 'Unknown')

        return (
          <section key={key}>
            {showGroupHeaders && <h2 className="mb-4 font-semibold text-lg">{groupName}</h2>}
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {servers.map((server) => (
                <ServerStatusCard key={server.id} server={server} />
              ))}
            </div>
          </section>
        )
      })}
    </div>
  )
}
