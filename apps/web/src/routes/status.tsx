import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Activity, Globe, Server } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { StatusBadge } from '@/components/server/status-badge'
import { api } from '@/lib/api-client'
import type { StatusPageResponse, StatusServer } from '@/lib/api-schema'
import { cn, countryCodeToFlag, formatSpeed, formatUptime } from '@/lib/utils'

export const Route = createFileRoute('/status')({
  component: StatusPage
})

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
  const { t } = useTranslation('status')
  const m = server.metrics
  const memPct = m && m.mem_total > 0 ? (m.mem_used / m.mem_total) * 100 : 0
  const diskPct = m && m.disk_total > 0 ? (m.disk_used / m.disk_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)

  return (
    <div className="rounded-lg border bg-card p-4 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-1.5 truncate">
          {flag && <span className="shrink-0 text-sm">{flag}</span>}
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
          {server.region && <span className="shrink-0 text-muted-foreground text-xs">{server.region}</span>}
        </div>
        <StatusBadge className="shrink-0" online={server.online} />
      </div>

      {server.public_remark && <p className="mb-3 text-muted-foreground text-xs">{server.public_remark}</p>}

      {m ? (
        <>
          <div className="space-y-2.5">
            <ProgressBar color="bg-chart-1" label={t('cpu')} value={m.cpu} />
            <ProgressBar color="bg-chart-2" label={t('memory')} value={memPct} />
            <ProgressBar color="bg-chart-3" label={t('disk')} value={diskPct} />
          </div>
          <div className="mt-3 flex items-center justify-between text-muted-foreground text-xs">
            <div className="flex gap-3">
              <span title={t('network_in')}>{formatSpeed(m.net_in_speed)}</span>
              <span title={t('network_out')}>{formatSpeed(m.net_out_speed)}</span>
            </div>
            <span title={t('uptime')}>{formatUptime(m.uptime)}</span>
          </div>
        </>
      ) : (
        <div className="flex h-24 items-center justify-center text-muted-foreground text-xs">
          {server.os && <span className="mr-2">{server.os}</span>}
          {!server.online && <span>{t('offline')}</span>}
        </div>
      )}
    </div>
  )
}

function StatusPage() {
  const { t, i18n } = useTranslation('status')
  const isZh = (i18n.resolvedLanguage ?? i18n.language).startsWith('zh')
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
            <button
              className="text-sm opacity-70 hover:opacity-100"
              onClick={() => i18n.changeLanguage(isZh ? 'en' : 'zh')}
              type="button"
            >
              {isZh ? 'EN' : '中文'}
            </button>
          </div>
          {data && (
            <div className="flex items-center gap-4 text-sm">
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <Globe className="size-4" />
                {data.total_count} {t('servers')}
              </span>
              <span className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
                <Activity className="size-4" />
                {data.online_count} {t('online')}
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
              <p className="text-muted-foreground text-sm">{t('loading')}</p>
            </div>
          </div>
        )}

        {error && (
          <div className="flex min-h-[300px] items-center justify-center">
            <p className="text-destructive text-sm">{t('load_failed')}</p>
          </div>
        )}

        {data && <StatusContent data={data} />}
      </main>
    </div>
  )
}

function StatusContent({ data }: { data: StatusPageResponse }) {
  const { t } = useTranslation('status')
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
        <p className="text-muted-foreground text-sm">{t('no_servers')}</p>
      </div>
    )
  }

  const showGroupHeaders = sortedKeys.length > 1 || !sortedKeys.includes('__ungrouped__')

  return (
    <div className="space-y-8">
      {sortedKeys.map((key) => {
        const servers = grouped.get(key) ?? []
        const groupName = key === '__ungrouped__' ? t('ungrouped') : (groupMap.get(key) ?? t('unknown'))

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
