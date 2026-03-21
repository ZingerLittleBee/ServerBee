import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, Cpu, HardDrive, MemoryStick, Server, Wifi } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ServerCard } from '@/components/server/server-card'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerGroup } from '@/lib/api-schema'
import { formatBytes } from '@/lib/utils'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage
})

function StatCard({
  icon: Icon,
  label,
  value,
  sub
}: {
  icon: typeof Server
  label: string
  sub?: string
  value: string
}) {
  return (
    <div className="flex items-center gap-3 rounded-lg border bg-card p-4">
      <div className="rounded-md bg-muted p-2">
        <Icon className="size-5 text-muted-foreground" />
      </div>
      <div>
        <p className="font-semibold text-lg leading-tight">{value}</p>
        <p className="text-muted-foreground text-xs">{label}</p>
        {sub && <p className="text-muted-foreground text-xs">{sub}</p>}
      </div>
    </div>
  )
}

function DashboardPage() {
  const { t } = useTranslation('dashboard')
  const { data: servers = [] } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })
  const onlineServers = servers.filter((s) => s.online)
  const onlineCount = onlineServers.length

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000
  })

  const stats = useMemo(() => {
    const online = servers.filter((s) => s.online)
    if (online.length === 0) {
      return { avgCpu: 0, avgMem: 0, totalBandwidth: 0 }
    }
    const avgCpu = online.reduce((sum, s) => sum + s.cpu, 0) / online.length
    const avgMem =
      online.reduce((sum, s) => {
        return sum + (s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0)
      }, 0) / online.length
    const totalBandwidth = online.reduce((sum, s) => sum + s.net_in_speed + s.net_out_speed, 0)
    return { avgCpu, avgMem, totalBandwidth }
  }, [servers])

  const groupMap = useMemo(() => new Map(groups?.map((g) => [g.id, g.name]) ?? []), [groups])

  const grouped = useMemo(() => {
    const map = new Map<string, ServerMetrics[]>()
    for (const server of servers) {
      const key = server.group_id ?? '__ungrouped__'
      const list = map.get(key)
      if (list) {
        list.push(server)
      } else {
        map.set(key, [server])
      }
    }
    return map
  }, [servers])

  const sortedKeys = useMemo(() => {
    return [...grouped.keys()].sort((a, b) => {
      if (a === '__ungrouped__') {
        return 1
      }
      if (b === '__ungrouped__') {
        return -1
      }
      return (groupMap.get(a) ?? '').localeCompare(groupMap.get(b) ?? '')
    })
  }, [grouped, groupMap])

  const hasGroups = sortedKeys.length > 1 || (sortedKeys.length === 1 && sortedKeys[0] !== '__ungrouped__')

  return (
    <div>
      <div className="mb-6">
        <h1 className="font-bold text-2xl">{t('title')}</h1>
        <p className="text-muted-foreground text-sm">
          {t('servers_online', { online: onlineCount, total: servers.length })}
        </p>
      </div>

      {servers.length > 0 && (
        <div className="mb-6 grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
          <StatCard
            icon={Server}
            label={t('stat_servers')}
            sub={t('offline_count', { count: servers.length - onlineCount })}
            value={`${onlineCount} / ${servers.length}`}
          />
          <StatCard icon={Cpu} label={t('avg_cpu')} value={`${stats.avgCpu.toFixed(1)}%`} />
          <StatCard icon={MemoryStick} label={t('avg_memory')} value={`${stats.avgMem.toFixed(1)}%`} />
          <StatCard
            icon={Wifi}
            label={t('total_bandwidth')}
            sub={t('per_second')}
            value={formatBytes(stats.totalBandwidth)}
          />
          <StatCard
            icon={onlineCount > 0 ? Activity : HardDrive}
            label={t('online')}
            value={onlineCount > 0 ? t('healthy') : t('no_data')}
          />
        </div>
      )}

      {servers.length === 0 && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_servers_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_servers_description')}</p>
          </div>
        </div>
      )}
      {servers.length > 0 && hasGroups && (
        <div className="space-y-8">
          {sortedKeys.map((key) => {
            const groupServers = grouped.get(key) ?? []
            const groupName = key === '__ungrouped__' ? t('ungrouped') : (groupMap.get(key) ?? t('unknown'))
            return (
              <section key={key}>
                <h2 className="mb-3 font-semibold text-lg">{groupName}</h2>
                <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                  {groupServers.map((server) => (
                    <ServerCard key={server.id} server={server} />
                  ))}
                </div>
              </section>
            )
          })}
        </div>
      )}
      {servers.length > 0 && !hasGroups && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {servers.map((server) => (
            <ServerCard key={server.id} server={server} />
          ))}
        </div>
      )}
    </div>
  )
}
