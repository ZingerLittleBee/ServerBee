import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Clock, Server, Wrench, XCircle } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import { api } from '@/lib/api-client'
import type { PublicStatusPageData, ThemeResolved } from '@/lib/api-schema'
import { cn } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import { isColorTheme, loadThemeCSS } from '@/themes'

export const Route = createFileRoute('/status/$slug')({
  component: PublicStatusPage
})

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type GlobalStatus = 'operational' | 'partial' | 'major'

function deriveGlobalStatus(data: PublicStatusPageData): GlobalStatus {
  const hasActiveIncident = data.active_incidents.length > 0
  const hasCritical = data.active_incidents.some((i) => i.severity === 'critical')
  const allOnline = data.servers.every((s) => s.online || s.in_maintenance)

  if (hasCritical || (!allOnline && hasActiveIncident)) {
    return 'major'
  }
  if (hasActiveIncident || !allOnline) {
    return 'partial'
  }
  return 'operational'
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString()
}

function formatRelative(iso: string, t: (key: string, options?: { count: number }) => string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60_000)
  if (mins < 1) {
    return t('time_just_now')
  }
  if (mins < 60) {
    return t('time_minutes_ago', { count: mins })
  }
  const hours = Math.floor(mins / 60)
  if (hours < 24) {
    return t('time_hours_ago', { count: hours })
  }
  const days = Math.floor(hours / 24)
  return t('time_days_ago', { count: days })
}

function serializeScopedVars(selector: string, vars: Record<string, string>) {
  const declarations = Object.entries(vars)
    .map(([key, value]) => `  --${key}: ${value};`)
    .join('\n')

  return `${selector} {\n${declarations}\n}`
}

export function applyStatusPageTheme(root: HTMLElement, theme: ThemeResolved) {
  root.removeAttribute('data-theme')
  root.querySelector('style[data-status-theme]')?.remove()

  if (theme.kind === 'preset') {
    if (theme.id !== 'default' && isColorTheme(theme.id)) {
      loadThemeCSS(theme.id).then(() => {
        root.setAttribute('data-theme', theme.id)
      })
    }
    return
  }

  const style = document.createElement('style')
  style.dataset.statusTheme = '1'
  style.textContent = [
    serializeScopedVars('.status-page-root', theme.vars_light),
    serializeScopedVars('.status-page-root.dark', theme.vars_dark)
  ].join('\n\n')
  root.appendChild(style)
}

function usePrefersDark() {
  const [prefersDark, setPrefersDark] = useState(() => window.matchMedia('(prefers-color-scheme: dark)').matches)

  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => setPrefersDark(mediaQuery.matches)
    mediaQuery.addEventListener('change', handleChange)
    handleChange()

    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [])

  return prefersDark
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function GlobalStatusBanner({ status, t }: { status: GlobalStatus; t: (key: string) => string }) {
  const config = {
    operational: { bg: 'bg-emerald-500', icon: CheckCircle2, label: t('all_operational') },
    partial: { bg: 'bg-amber-500', icon: AlertTriangle, label: t('partial_outage') },
    major: { bg: 'bg-red-500', icon: XCircle, label: t('major_outage') }
  }
  const c = config[status]
  return (
    <div className={cn('flex items-center gap-3 rounded-lg px-6 py-4 text-white', c.bg)}>
      <c.icon className="size-6" />
      <span className="font-semibold text-lg">{c.label}</span>
    </div>
  )
}

function ServerStatusDot({
  inMaintenance,
  online,
  t
}: {
  inMaintenance: boolean
  online: boolean
  t: (key: string) => string
}) {
  if (inMaintenance) {
    return <span className="inline-block size-3 rounded-full bg-blue-500" title={t('maintenance')} />
  }
  if (online) {
    return <span className="inline-block size-3 rounded-full bg-emerald-500" title={t('online_status')} />
  }
  return <span className="inline-block size-3 rounded-full bg-gray-400" title={t('offline_status')} />
}

function ServerRow({
  page,
  server,
  t
}: {
  page: PublicStatusPageData['page']
  server: PublicStatusPageData['servers'][number]
  t: (key: string) => string
}) {
  const uptimePct = computeAggregateUptime(server.uptime_daily)
  return (
    <div className="flex items-center gap-3 rounded-md border px-4 py-3">
      <ServerStatusDot inMaintenance={server.in_maintenance} online={server.online} t={t} />
      <span className="min-w-0 flex-1 truncate font-medium text-sm">{server.server_name}</span>
      {server.in_maintenance && (
        <Badge className="shrink-0" variant="secondary">
          <Wrench className="mr-1 size-3" />
          {t('maintenance')}
        </Badge>
      )}
      <div className="hidden w-64 sm:block">
        <UptimeTimeline
          days={server.uptime_daily}
          height={20}
          rangeDays={90}
          redThreshold={page.uptime_red_threshold}
          yellowThreshold={page.uptime_yellow_threshold}
        />
      </div>
      <span className="w-16 shrink-0 text-right text-muted-foreground text-xs">
        {uptimePct !== null ? `${uptimePct.toFixed(1)}%` : '\u2014'}
      </span>
    </div>
  )
}

function SeverityBadge({ severity, t }: { severity: string; t: (key: string, options?: { count: number }) => string }) {
  const variants: Record<string, 'default' | 'destructive' | 'secondary'> = {
    critical: 'destructive',
    major: 'destructive',
    minor: 'secondary'
  }
  const severityKey = `incidents_severity_${severity}` as const
  return <Badge variant={variants[severity] ?? 'default'}>{t(severityKey)}</Badge>
}

function IncidentCard({
  incident,
  t
}: {
  incident: PublicStatusPageData['active_incidents'][number] | PublicStatusPageData['recent_incidents'][number]
  t: (key: string, options?: { count: number }) => string
}) {
  const [expanded, setExpanded] = useState(true)

  return (
    <div className="rounded-lg border p-4">
      <button
        className="flex w-full items-center justify-between text-left"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        <div className="flex items-center gap-2">
          <SeverityBadge severity={incident.severity} t={t} />
          <span className="font-semibold text-sm">{incident.title}</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-xs">{formatRelative(incident.created_at, t)}</span>
          {expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </div>
      </button>
      {expanded && incident.updates.length > 0 && (
        <div className="mt-3 space-y-2 border-muted border-l-2 pl-4">
          {incident.updates.map((update) => (
            <div key={update.id}>
              <div className="flex items-center gap-2">
                <Badge variant="outline">{update.status}</Badge>
                <span className="text-muted-foreground text-xs">{formatDate(update.created_at)}</span>
              </div>
              <p className="mt-1 text-sm">{update.message}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function MaintenanceNotice({ maintenance }: { maintenance: PublicStatusPageData['planned_maintenances'][number] }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-900 dark:bg-blue-950/30">
      <Clock className="mt-0.5 size-4 shrink-0 text-blue-600 dark:text-blue-400" />
      <div className="min-w-0 flex-1">
        <p className="font-semibold text-sm">{maintenance.title}</p>
        {maintenance.description && <p className="mt-1 text-muted-foreground text-xs">{maintenance.description}</p>}
        <p className="mt-1 text-muted-foreground text-xs">
          {formatDate(maintenance.start_at)} &mdash; {formatDate(maintenance.end_at)}
        </p>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

function PublicStatusPage() {
  const { slug } = Route.useParams()
  const { t, i18n } = useTranslation('status')
  const isZh = (i18n.resolvedLanguage ?? i18n.language).startsWith('zh')
  const rootRef = useRef<HTMLDivElement>(null)
  const prefersDark = usePrefersDark()

  const { data, isLoading, error } = useQuery<PublicStatusPageData>({
    queryKey: ['public-status', slug],
    queryFn: () => api.get<PublicStatusPageData>(`/api/status/${slug}`),
    refetchInterval: 30_000
  })

  useEffect(() => {
    if (rootRef.current && data?.theme) {
      applyStatusPageTheme(rootRef.current, data.theme)
    }
  }, [data?.theme])

  return (
    <div className={cn('status-page-root min-h-screen bg-background', prefersDark && 'dark')} ref={rootRef}>
      <header className="border-b">
        <div className="mx-auto flex max-w-4xl items-center justify-between px-4 py-4">
          <div className="flex items-center gap-2">
            <Server className="size-5 text-primary" />
            <span className="font-semibold text-lg">{data?.page.title ?? 'Status'}</span>
          </div>
          <button
            className="text-sm opacity-70 hover:opacity-100"
            onClick={() => i18n.changeLanguage(isZh ? 'en' : 'zh')}
            type="button"
          >
            {isZh ? 'EN' : '\u4E2D\u6587'}
          </button>
        </div>
      </header>

      <main className="mx-auto max-w-4xl space-y-8 px-4 py-8">
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

        {data && <StatusPageContent data={data} />}
      </main>

      {data?.page.custom_css && <style>{data.page.custom_css}</style>}
    </div>
  )
}

function StatusPageContent({ data }: { data: PublicStatusPageData }) {
  const { t } = useTranslation('status')
  const globalStatus = deriveGlobalStatus(data)
  const [showRecent, setShowRecent] = useState(false)

  // Group servers
  const grouped = new Map<string, PublicStatusPageData['servers']>()
  for (const server of data.servers) {
    const key = server.group_name ?? '__default__'
    const list = grouped.get(key)
    if (list) {
      list.push(server)
    } else {
      grouped.set(key, [server])
    }
  }

  return (
    <>
      {/* Global status banner */}
      <GlobalStatusBanner status={globalStatus} t={t} />

      {/* Description */}
      {data.page.description && <p className="text-muted-foreground text-sm">{data.page.description}</p>}

      {/* Active incidents */}
      {data.active_incidents.length > 0 && (
        <section>
          <h2 className="mb-3 font-semibold text-lg">{t('active_incidents')}</h2>
          <div className="space-y-3">
            {data.active_incidents.map((incident) => (
              <IncidentCard incident={incident} key={incident.id} t={t} />
            ))}
          </div>
        </section>
      )}

      {/* Planned maintenance */}
      {data.planned_maintenances.length > 0 && (
        <section>
          <h2 className="mb-3 font-semibold text-lg">{t('planned_maintenance')}</h2>
          <div className="space-y-3">
            {data.planned_maintenances.map((m) => (
              <MaintenanceNotice key={m.id} maintenance={m} />
            ))}
          </div>
        </section>
      )}

      {/* Servers */}
      <section>
        <h2 className="mb-3 font-semibold text-lg">{t('server_status')}</h2>
        {data.servers.length === 0 ? (
          <div className="rounded-lg border border-dashed p-8 text-center text-muted-foreground text-sm">
            {t('no_servers')}
          </div>
        ) : (
          <div className="space-y-6">
            {[...grouped.entries()].map(([group, servers]) => (
              <div key={group}>
                {grouped.size > 1 && group !== '__default__' && (
                  <h3 className="mb-2 font-medium text-muted-foreground text-sm">{group}</h3>
                )}
                <div className="space-y-2">
                  {servers.map((s) => (
                    <ServerRow key={s.server_id} page={data.page} server={s} t={t} />
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Recent incidents (collapsible) */}
      {data.recent_incidents.length > 0 && (
        <section>
          <button
            className="flex w-full items-center justify-between rounded-lg border px-4 py-3 text-left"
            onClick={() => setShowRecent(!showRecent)}
            type="button"
          >
            <h2 className="font-semibold">{t('recent_incidents')}</h2>
            {showRecent ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
          </button>
          {showRecent && (
            <div className="mt-3 space-y-3">
              {data.recent_incidents.map((incident) => (
                <IncidentCard incident={incident} key={incident.id} t={t} />
              ))}
            </div>
          )}
        </section>
      )}
    </>
  )
}
