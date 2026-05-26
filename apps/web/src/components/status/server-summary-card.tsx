import { Wrench } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { StatusBadge } from '@/components/server/status-badge'
import { Badge } from '@/components/ui/badge'
import type { PublicServerSummary } from '@/lib/api-schema'
import { cn, countryCodeToFlag, formatSpeed, formatUptime } from '@/lib/utils'

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

interface Props {
  clickable: boolean
  server: PublicServerSummary
}

export function ServerSummaryCard({ server, clickable }: Props) {
  const { t } = useTranslation('status')
  const m = server.metrics
  const memPct = m && m.mem_total > 0 ? (m.mem_used / m.mem_total) * 100 : 0
  const diskPct = m && m.disk_total > 0 ? (m.disk_used / m.disk_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)

  const body: ReactNode = (
    <div
      className={cn(
        'rounded-lg border bg-card p-4 shadow-sm transition-colors',
        clickable && 'hover:border-primary/40 hover:bg-accent/40'
      )}
    >
      <div className="mb-3 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5 truncate">
          {flag && <span className="shrink-0 text-sm">{flag}</span>}
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
          {server.region && <span className="shrink-0 text-muted-foreground text-xs">{server.region}</span>}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {server.in_maintenance && (
            <Badge variant="secondary">
              <Wrench className="mr-1 size-3" />
              {t('maintenance')}
            </Badge>
          )}
          <StatusBadge status={server.online ? 'online' : 'offline'} />
        </div>
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

  if (clickable) {
    // R8 will introduce `/status/server/$serverId` as a TanStack Router route.
    // Until then, use a plain anchor so this round typechecks against the current route tree.
    return (
      <a className="block" href={`/status/server/${server.id}`}>
        {body}
      </a>
    )
  }
  return body
}
