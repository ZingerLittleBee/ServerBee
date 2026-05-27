import { Link } from '@tanstack/react-router'
import { ArrowDown, ArrowUp, Cpu, HardDrive, MemoryStick, Network, Sigma, Wrench } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { StatusDot } from '@/components/server/status-dot'
import { Badge } from '@/components/ui/badge'
import { TableCell, TableRow } from '@/components/ui/table'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import type { PublicServerSummary, PublicStatusConfig } from '@/lib/api-schema'
import { computeTrafficQuota } from '@/lib/traffic'
import { cn, countryCodeToFlag, formatBytes, formatSpeed } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'

interface Props {
  clickable: boolean
  server: PublicServerSummary
  thresholds: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'>
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return 0
  }
  return Math.min(100, Math.max(0, value))
}

function metricPercent(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0
}

function finiteMetric(value: number | null | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function splitValueUnit(formatted: string): { unit: string | null; value: string } {
  const lastSpace = formatted.lastIndexOf(' ')
  if (lastSpace < 0) {
    return { unit: null, value: formatted }
  }
  return { unit: formatted.slice(lastSpace + 1), value: formatted.slice(0, lastSpace) }
}

function valueClassName(value: string): string {
  return value === '0' ? 'text-xs' : 'font-semibold text-foreground text-xs'
}

function renderBytesValue(bytes: number): ReactNode {
  const { value, unit } = splitValueUnit(formatBytes(bytes))
  if (unit == null) {
    return <span className={valueClassName(value)}>{value}</span>
  }
  return (
    <>
      <span className={valueClassName(value)}>{value}</span> <span className="text-[9px]">{unit}</span>
    </>
  )
}

function renderSpeedValue(bytesPerSec: number): ReactNode {
  if (bytesPerSec <= 0) {
    return <span className="text-xs">0</span>
  }
  const { value, unit } = splitValueUnit(formatSpeed(bytesPerSec))
  if (unit == null) {
    return <span className={valueClassName(value)}>{value}</span>
  }
  return (
    <>
      <span className={valueClassName(value)}>{value}</span> <span className="text-[9px]">{unit}</span>
    </>
  )
}

function getBarColor(pct: number): string {
  if (pct > 90) {
    return 'bg-red-500'
  }
  if (pct > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

function getBarTextColor(pct: number): string {
  if (pct > 90) {
    return 'text-red-600 dark:text-red-400'
  }
  if (pct > 70) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-foreground'
}

function PositionIndicator({ pct }: { pct: number }) {
  const clamped = clampPercent(pct)
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
      <div className={cn('h-full rounded-full', getBarColor(clamped))} style={{ width: `${clamped}%` }} />
    </div>
  )
}

function EmptyMetric() {
  return <span className="text-muted-foreground">-</span>
}

function ResourceMetric({ icon, pct, value }: { icon: ReactNode; pct: number; value: ReactNode }) {
  const roundedPct = Math.round(clampPercent(pct))
  return (
    <div className="flex max-w-[160px] flex-col gap-0.5">
      <div className="flex h-4 items-center gap-1.5 font-mono text-[10px] text-muted-foreground tabular-nums">
        <span className="inline-flex size-3.5 flex-none text-muted-foreground">{icon}</span>
        <span className="min-w-0 truncate">{value}</span>
        <span className={cn('ml-auto font-semibold', getBarTextColor(roundedPct))}>{roundedPct}%</span>
      </div>
      <div className="flex h-4 items-center">
        <PositionIndicator pct={pct} />
      </div>
    </div>
  )
}

function DiskMetric({ server }: { server: PublicServerSummary }) {
  const metrics = server.metrics
  if (!metrics) {
    return <EmptyMetric />
  }

  return (
    <div className="grid grid-cols-[max-content_max-content] gap-x-1.5 gap-y-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">
      <span className="flex h-4 items-center gap-1">
        <HardDrive aria-hidden="true" className="size-3.5 flex-none text-muted-foreground" />
        {renderBytesValue(metrics.disk_used)}
      </span>
      <span className="flex h-4 items-center gap-1">
        <Sigma aria-hidden="true" className="size-3.5 flex-none text-muted-foreground" />
        {renderBytesValue(metrics.disk_total)}
      </span>
      <span className="flex h-4 items-center gap-1">
        <span className="inline-flex size-3.5 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-foreground">
          R
        </span>
        {renderSpeedValue(metrics.disk_read_bytes_per_sec)}
      </span>
      <span className="flex h-4 items-center gap-1">
        <span className="inline-flex size-3.5 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-foreground">
          W
        </span>
        {renderSpeedValue(metrics.disk_write_bytes_per_sec)}
      </span>
    </div>
  )
}

function NetworkMetric({ server }: { server: PublicServerSummary }) {
  const { t } = useTranslation('status')
  const metrics = server.metrics
  if (!metrics) {
    return <EmptyMetric />
  }
  const traffic = computeTrafficQuota({
    entry: undefined,
    netInTransfer: metrics.net_in_transfer,
    netOutTransfer: metrics.net_out_transfer
  })

  return (
    <div className="grid grid-cols-[max-content_max-content] gap-x-1.5 gap-y-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">
      <span className="flex h-4 items-center gap-1" title={t('network_total', { defaultValue: 'Network total' })}>
        <Network aria-hidden="true" className="size-3.5 flex-none text-muted-foreground" />
        {renderBytesValue(traffic.used)}
      </span>
      <span className="flex h-4 items-center gap-1" title={t('network_limit', { defaultValue: 'Network limit' })}>
        <Sigma aria-hidden="true" className="size-3.5 flex-none text-muted-foreground" />
        {renderBytesValue(traffic.limit)}
      </span>
      <span className="flex h-4 items-center gap-1" title={t('network_in')}>
        <span className="inline-flex size-3.5 flex-none items-center justify-center rounded-sm bg-muted text-foreground">
          <ArrowDown aria-hidden="true" className="size-2.5" />
        </span>
        {renderSpeedValue(metrics.net_in_speed)}
      </span>
      <span className="flex h-4 items-center gap-1" title={t('network_out')}>
        <span className="inline-flex size-3.5 flex-none items-center justify-center rounded-sm bg-muted text-foreground">
          <ArrowUp aria-hidden="true" className="size-2.5" />
        </span>
        {renderSpeedValue(metrics.net_out_speed)}
      </span>
    </div>
  )
}

function DetailMetric({
  server,
  thresholds,
  uptimePct
}: {
  server: PublicServerSummary
  thresholds: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'>
  uptimePct: number | null
}) {
  const { t } = useTranslation(['status', 'servers'])
  const metrics = server.metrics
  const processCount = metrics ? finiteMetric(metrics.process_count) : 0
  const tcpConn = metrics ? finiteMetric(metrics.tcp_conn) : 0
  const udpConn = metrics ? finiteMetric(metrics.udp_conn) : 0

  return (
    <div className="flex flex-col gap-1.5">
      <UptimeTimeline
        days={server.uptime_daily}
        height={18}
        rangeDays={90}
        redThreshold={thresholds.uptime_red_threshold}
        yellowThreshold={thresholds.uptime_yellow_threshold}
      />
      <div className="flex flex-col gap-0.5">
        <span className="text-muted-foreground text-xs">
          {uptimePct !== null ? `${uptimePct.toFixed(1)}%` : t('uptime_no_data', { ns: 'status' })}
        </span>
        {metrics && (
          <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
            {t('card_proc_conn_label', { ns: 'servers' })} {processCount} / {tcpConn} / {udpConn}
          </span>
        )}
      </div>
    </div>
  )
}

function ServerName({ server, clickable }: { clickable: boolean; server: PublicServerSummary }) {
  const { t } = useTranslation('status')
  const flag = countryCodeToFlag(server.country_code)
  const title = (
    <span className="group/link flex min-w-0 items-center gap-1.5">
      {flag && (
        <span className="shrink-0 text-xs" title={server.country_code ?? ''}>
          {flag}
        </span>
      )}
      <span className="truncate font-medium group-hover/link:underline">{server.name}</span>
    </span>
  )

  return (
    <div className="flex min-w-0 items-center gap-2">
      <StatusDot className="flex-none" status={server.online ? 'online' : 'offline'} />
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-1.5">
          {clickable ? (
            <Link className="min-w-0" params={{ serverId: server.id }} to="/status/server/$serverId">
              {title}
            </Link>
          ) : (
            title
          )}
          {server.in_maintenance && (
            <Badge className="shrink-0" variant="secondary">
              <Wrench className="mr-1 size-3" />
              {t('maintenance')}
            </Badge>
          )}
        </div>
        <div className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
          {server.group_name && (
            <Badge className="max-w-32 truncate px-1.5 py-0 text-[10px]" variant="outline">
              {server.group_name}
            </Badge>
          )}
          {server.region && <span className="truncate">{server.region}</span>}
          {server.os && <span className="truncate">{server.os}</span>}
        </div>
      </div>
    </div>
  )
}

export function ServerSummaryRow({ server, clickable, thresholds }: Props) {
  const metrics = server.metrics
  const uptimePct = computeAggregateUptime(server.uptime_daily) ?? server.uptime_percent
  const memoryPct = metrics ? metricPercent(metrics.mem_used, metrics.mem_total) : 0

  return (
    <TableRow className={cn('h-[72px]', !server.online && 'opacity-45 grayscale')} data-slot="status-server-row">
      <TableCell className="min-w-[220px]">
        <ServerName clickable={clickable} server={server} />
      </TableCell>
      <TableCell className="w-[180px] align-top">
        {metrics ? (
          <ResourceMetric
            icon={<Cpu aria-hidden="true" />}
            pct={metrics.cpu}
            value={`load ${metrics.load_1.toFixed(2)}`}
          />
        ) : (
          <EmptyMetric />
        )}
      </TableCell>
      <TableCell className="w-[180px] align-top">
        {metrics ? (
          <ResourceMetric
            icon={<MemoryStick aria-hidden="true" />}
            pct={memoryPct}
            value={
              <>
                {renderBytesValue(metrics.mem_used)} / {renderBytesValue(metrics.mem_total)}
              </>
            }
          />
        ) : (
          <EmptyMetric />
        )}
      </TableCell>
      <TableCell className="w-[184px] align-top">
        <DiskMetric server={server} />
      </TableCell>
      <TableCell className="hidden w-[184px] align-top lg:table-cell">
        <NetworkMetric server={server} />
      </TableCell>
      <TableCell className="hidden w-[220px] xl:table-cell">
        <DetailMetric server={server} thresholds={thresholds} uptimePct={uptimePct} />
      </TableCell>
    </TableRow>
  )
}
