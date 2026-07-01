import { Link } from '@tanstack/react-router'
import { Wrench } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { CountryFlag } from '@/components/country-flag'
import { CompactMetric } from '@/components/server/compact-metric'
import { MetricValue } from '@/components/server/metric-value'
import { StatusBadge } from '@/components/server/status-badge'
import { Badge } from '@/components/ui/badge'
import { RingChart } from '@/components/ui/ring-chart'
import type { PublicServerSummary } from '@/lib/api-schema'
import { computeTrafficQuota } from '@/lib/traffic'
import { cn, formatBytes, formatUptime } from '@/lib/utils'

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return 0
  }
  return Math.min(100, Math.max(0, value))
}

function getRingColor(pct: number, brandColor: string): string {
  if (pct > 90) {
    return '#ef4444'
  }
  if (pct > 70) {
    return '#f59e0b'
  }
  return brandColor
}

function metricPercent(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0
}

function finiteMetric(value: number | null | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

interface RingMetricProps {
  color: string
  label: string
  subText: ReactNode
  value: number
}

function RingMetric({ color, label, subText, value }: RingMetricProps) {
  return (
    <div className="flex items-center gap-2">
      <RingChart color={color} compact label={label} value={value} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[11px] text-muted-foreground">{label}</span>
        <span className="truncate text-[10px] text-muted-foreground tabular-nums">{subText}</span>
      </div>
    </div>
  )
}

interface Props {
  clickable: boolean
  server: PublicServerSummary
}

export function ServerSummaryCard({ server, clickable }: Props) {
  const { t } = useTranslation(['status', 'servers'])
  const m = server.metrics
  const memPct = m ? metricPercent(m.mem_used, m.mem_total) : 0
  const diskPct = m ? metricPercent(m.disk_used, m.disk_total) : 0
  const swapPct = m ? metricPercent(m.swap_used, m.swap_total) : 0
  const processCount = m ? finiteMetric(m.process_count) : 0
  const tcpConn = m ? finiteMetric(m.tcp_conn) : 0
  const udpConn = m ? finiteMetric(m.udp_conn) : 0
  const traffic = m
    ? computeTrafficQuota({
        entry: undefined,
        netInTransfer: m.net_in_transfer,
        netOutTransfer: m.net_out_transfer
      })
    : { limit: 0, pct: 0, used: 0 }
  const status = server.online ? 'online' : 'offline'

  const body: ReactNode = (
    <div
      className={cn(
        'relative flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm transition-colors',
        clickable && 'hover:border-primary/40 hover:bg-accent/40'
      )}
      data-slot="status-server-card"
    >
      {!server.online && (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 z-10 rounded-lg bg-background/55 backdrop-grayscale"
        />
      )}
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5 truncate">
          <CountryFlag className="text-sm" code={server.country_code} />
          <h3 className="truncate font-semibold text-[13px]">{server.name}</h3>
          {server.region && <span className="shrink-0 text-[11px] text-muted-foreground">{server.region}</span>}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {server.in_maintenance && (
            <Badge variant="secondary">
              <Wrench className="mr-1 size-3" />
              {t('maintenance')}
            </Badge>
          )}
          <StatusBadge status={status} />
        </div>
      </div>

      {server.public_remark && <p className="line-clamp-2 text-muted-foreground text-xs">{server.public_remark}</p>}

      {m ? (
        <>
          <div className="grid grid-cols-2 gap-x-3 gap-y-2">
            <RingMetric
              color={getRingColor(m.cpu, 'var(--color-chart-1)')}
              label={t('cpu')}
              subText={`load ${m.load_1.toFixed(2)}`}
              value={m.cpu}
            />
            <RingMetric
              color={getRingColor(memPct, 'var(--color-chart-2)')}
              label={t('memory')}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(m.mem_used)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(m.mem_total)}
                </>
              }
              value={memPct}
            />
            <RingMetric
              color={getRingColor(diskPct, 'var(--color-chart-3)')}
              label={t('disk')}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(m.disk_used)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(m.disk_total)}
                </>
              }
              value={diskPct}
            />
            <RingMetric
              color={getRingColor(traffic.pct, 'var(--color-chart-4)')}
              label={t('card_traffic_quota', { ns: 'servers' })}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(traffic.used)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(traffic.limit)}
                </>
              }
              value={clampPercent(traffic.pct)}
            />
          </div>

          <div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
            <CompactMetric
              label={t('card_net_in_speed', { ns: 'servers' })}
              value={<MetricValue kind="speed" value={m.net_in_speed} variant="compact" />}
            />
            <CompactMetric
              label={t('card_net_out_speed', { ns: 'servers' })}
              value={<MetricValue kind="speed" value={m.net_out_speed} variant="compact" />}
            />
            <CompactMetric
              label={
                <span className="inline-flex items-center gap-1">
                  <span
                    aria-hidden="true"
                    className="inline-flex size-3.5 flex-none items-center justify-center rounded-full bg-muted font-semibold text-[8px] text-foreground leading-none"
                  >
                    R
                  </span>
                  {t('card_disk_read', { ns: 'servers' })}
                </span>
              }
              value={<MetricValue kind="speed" value={m.disk_read_bytes_per_sec} variant="compact" />}
            />
            <CompactMetric
              label={
                <span className="inline-flex items-center gap-1">
                  <span
                    aria-hidden="true"
                    className="inline-flex size-3.5 flex-none items-center justify-center rounded-full bg-muted font-semibold text-[8px] text-foreground leading-none"
                  >
                    W
                  </span>
                  {t('card_disk_write', { ns: 'servers' })}
                </span>
              }
              value={<MetricValue kind="speed" value={m.disk_write_bytes_per_sec} variant="compact" />}
            />
          </div>

          <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
            <div className="flex items-baseline justify-between">
              <span>{t('uptime')}</span>
              <span className="font-medium text-foreground tabular-nums">{formatUptime(m.uptime)}</span>
            </div>
            <div className="flex items-baseline justify-between">
              <span>{t('card_swap', { ns: 'servers' })}</span>
              <span className="font-medium text-foreground tabular-nums">{`${swapPct.toFixed(0)}%`}</span>
            </div>
            <div className="flex items-baseline justify-between">
              <span>{t('card_load_trend', { ns: 'servers' })}</span>
              <span className="inline-flex items-center gap-1.5 font-medium text-foreground tabular-nums">
                <span>{m.load_5.toFixed(2)}</span>
                <span aria-hidden="true">·</span>
                <span>{m.load_15.toFixed(2)}</span>
              </span>
            </div>
            <div className="flex items-baseline justify-between">
              <span>{t('card_proc_conn_label', { ns: 'servers' })}</span>
              <span className="font-medium text-foreground tabular-nums">
                {`${processCount} / ${tcpConn} / ${udpConn}`}
              </span>
            </div>
          </div>
        </>
      ) : (
        <div className="flex min-h-32 flex-col justify-center gap-1 rounded-md bg-muted/40 px-3 py-4 text-muted-foreground text-xs">
          {server.os && <span>{server.os}</span>}
          <span>{server.online ? t('uptime_no_data') : t('offline')}</span>
        </div>
      )}
    </div>
  )

  if (clickable) {
    return (
      <Link className="block" params={{ serverId: server.id }} to="/status/server/$serverId">
        {body}
      </Link>
    )
  }
  return body
}
