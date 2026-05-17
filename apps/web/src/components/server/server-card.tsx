import { Link } from '@tanstack/react-router'
import { memo, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { RingChart } from '@/components/ui/ring-chart'
import { useCostOverview } from '@/hooks/use-cost'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import { computeTrafficQuota } from '@/lib/traffic'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CostFootnote } from './cost-footnote'
import { NetworkSquareGrid } from './network-square-grid'
import { buildServerCardNetworkState } from './server-card-network-data'
import { StatusBadge } from './status-badge'
import { TagChips } from './tag-chips'
import { UpgradeJobBadge } from './upgrade-job-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function osIcon(os: string | null): string {
  if (!os) {
    return ''
  }
  const lower = os.toLowerCase()
  if (lower.includes('ubuntu') || lower.includes('debian') || lower.includes('linux')) {
    return '🐧'
  }
  if (lower.includes('windows')) {
    return '🪟'
  }
  if (lower.includes('macos') || lower.includes('darwin')) {
    return '🍎'
  }
  if (lower.includes('freebsd') || lower.includes('openbsd')) {
    return '😈'
  }
  return ''
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

function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < 0.01) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (lossRatio < 0.05) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '—'
  }
  return `${ms.toFixed(0)}`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '—'
  }
  return `${(lossRatio * 100).toFixed(1)}%`
}

function formatLoad(load: number): string {
  return load.toFixed(2)
}

function renderSpeedValue(bytesPerSec: number): React.ReactNode {
  if (bytesPerSec <= 0) {
    return '0'
  }
  const formatted = formatSpeed(bytesPerSec)
  const lastSpace = formatted.lastIndexOf(' ')
  if (lastSpace < 0) {
    return formatted
  }
  return (
    <>
      {formatted.slice(0, lastSpace)}
      <span className="ml-0.5 font-normal text-[10px] text-muted-foreground">{formatted.slice(lastSpace + 1)}</span>
    </>
  )
}

interface RingMetricProps {
  color: string
  label: string
  subText: React.ReactNode
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

const ServerCardInner = ({ server }: ServerCardProps) => {
  const { t } = useTranslation(['servers'])
  const { data: networkOverview = [] } = useNetworkOverview()
  const { data: realtimeData } = useNetworkRealtime(server.id)
  const { data: trafficOverview } = useTrafficOverview()
  const { data: costOverview } = useCostOverview()
  const upgradeJob = useUpgradeJobsStore((state) => state.jobs.get(server.id))

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const networkSummary = networkOverview.find((entry) => entry.server_id === server.id)
  const { currentAvgLatency, currentAvgLossRatio, latencyPoints, lossPoints } = useMemo(
    () => buildServerCardNetworkState(networkSummary, realtimeData),
    [networkSummary, realtimeData]
  )

  const hasNetworkData = latencyPoints.length > 0

  const trafficEntry = trafficOverview?.find((entry) => entry.server_id === server.id)
  const {
    used: trafficUsed,
    limit: trafficLimit,
    pct: trafficRingPct
  } = computeTrafficQuota({
    entry: trafficEntry,
    netInTransfer: server.net_in_transfer,
    netOutTransfer: server.net_out_transfer
  })
  const trafficDaysRemaining = trafficEntry?.days_remaining ?? null
  const costEntry = costOverview?.servers.find((entry) => entry.server_id === server.id)

  return (
    <div className="flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm">
      <div className="flex items-center justify-between">
        <Link
          className="flex items-center gap-1 truncate border-transparent border-b pb-px hover:border-current"
          params={{ id: server.id }}
          search={{ range: 'realtime' }}
          to="/servers/$id"
        >
          {flag && (
            <span className="shrink-0 text-sm" title={server.country_code ?? ''}>
              {flag}
            </span>
          )}
          {osEmoji && (
            <span className="shrink-0 text-sm" title={server.os ?? ''}>
              {osEmoji}
            </span>
          )}
          <h3 className="truncate font-semibold text-[13px]">{server.name}</h3>
        </Link>
        <div className="flex items-center gap-1.5">
          <UpgradeJobBadge job={upgradeJob} />
          <StatusBadge online={server.online} />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-x-3 gap-y-2">
        <RingMetric
          color={getRingColor(server.cpu, 'var(--color-chart-1)')}
          label={t('col_cpu')}
          subText={
            <>
              {t('card_load')} <span className="font-medium text-foreground">{formatLoad(server.load1)}</span>
            </>
          }
          value={server.cpu}
        />
        <RingMetric
          color={getRingColor(memoryPct, 'var(--color-chart-2)')}
          label={t('col_memory')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(server.mem_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.mem_total)}
            </>
          }
          value={memoryPct}
        />
        <RingMetric
          color={getRingColor(diskPct, 'var(--color-chart-3)')}
          label={t('col_disk')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(server.disk_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.disk_total)}
            </>
          }
          value={diskPct}
        />
        <RingMetric
          color={getRingColor(trafficRingPct, 'var(--color-chart-4)')}
          label={t('card_traffic_quota')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(trafficUsed)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(trafficLimit)}
            </>
          }
          value={trafficRingPct}
        />
      </div>

      <div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
        <CompactMetric label={t('card_net_in_speed')} value={renderSpeedValue(server.net_in_speed)} />
        <CompactMetric label={t('card_net_out_speed')} value={renderSpeedValue(server.net_out_speed)} />
        <CompactMetric
          label={
            <span
              aria-label={t('card_disk_read')}
              className="inline-flex size-3 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-[9px] text-foreground leading-none"
              role="img"
            >
              R
            </span>
          }
          value={renderSpeedValue(server.disk_read_bytes_per_sec)}
        />
        <CompactMetric
          label={
            <span
              aria-label={t('card_disk_write')}
              className="inline-flex size-3 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-[9px] text-foreground leading-none"
              role="img"
            >
              W
            </span>
          }
          value={renderSpeedValue(server.disk_write_bytes_per_sec)}
        />
        <CompactMetric
          label={t('card_load_trend')}
          value={`${formatLoad(server.load5)}·${formatLoad(server.load15)}`}
        />
      </div>

      {hasNetworkData && (
        <section aria-label={t('card_network_quality')} className="grid grid-cols-2 gap-x-3 gap-y-1">
          <div className="flex items-baseline justify-between">
            <span className="text-[11px] text-muted-foreground">{t('card_latency')}</span>
            <span
              className={`font-semibold text-xs tabular-nums ${latencyColorClass(currentAvgLatency, {
                failed: isLatencyFailure(currentAvgLossRatio)
              })}`}
            >
              {formatLatency(currentAvgLatency)}
              <span className="ml-0.5 font-medium text-[10px] text-muted-foreground">ms</span>
            </span>
          </div>
          <div className="flex items-baseline justify-between">
            <span className="text-[11px] text-muted-foreground">{t('card_packet_loss')}</span>
            <span className={`font-semibold text-xs tabular-nums ${getLossTextClassName(currentAvgLossRatio)}`}>
              {formatPacketLoss(currentAvgLossRatio)}
            </span>
          </div>
          <NetworkSquareGrid kind="latency" points={latencyPoints} />
          <NetworkSquareGrid kind="loss" points={lossPoints} />
        </section>
      )}

      <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
        <div className="flex items-baseline justify-between">
          <span>{t('col_uptime')}</span>
          <span className="font-medium text-foreground tabular-nums">{formatUptime(server.uptime)}</span>
        </div>
        <div className="flex items-baseline justify-between">
          <span>{t('card_swap')}</span>
          <span className="font-medium text-foreground tabular-nums">{`${swapPct.toFixed(0)}%`}</span>
        </div>
        <div className="flex items-baseline justify-between">
          <span>{t('card_processes')}</span>
          <span className="font-medium text-foreground tabular-nums">{server.process_count}</span>
        </div>
        <div className="flex items-baseline justify-between">
          <span>{t('card_tcp')}</span>
          <span className="font-medium text-foreground tabular-nums">{server.tcp_conn}</span>
        </div>
        <div className="flex items-baseline justify-between">
          <span>{t('card_udp')}</span>
          <span className="font-medium text-foreground tabular-nums">{server.udp_conn}</span>
        </div>
        {trafficDaysRemaining != null && (
          <div className="flex items-baseline justify-between">
            <span>{t('card_traffic_days_left_label')}</span>
            <span className="font-medium text-foreground tabular-nums">
              {t('card_traffic_days_value', { count: trafficDaysRemaining })}
            </span>
          </div>
        )}
        <div className="col-span-2 flex justify-center pt-0.5">
          <CostFootnote entry={costEntry} />
        </div>
      </div>

      <TagChips tags={server.tags} />
    </div>
  )
}

function tagsEqual(a: readonly string[] | undefined, b: readonly string[] | undefined): boolean {
  if (a === b) {
    return true
  }
  if (!(a && b)) {
    return false
  }
  if (a.length !== b.length) {
    return false
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false
    }
  }
  return true
}

export const ServerCard = memo(ServerCardInner, (prev, next) => {
  const a = prev.server
  const b = next.server
  return (
    a.id === b.id &&
    a.online === b.online &&
    a.last_active === b.last_active &&
    a.name === b.name &&
    a.country_code === b.country_code &&
    a.os === b.os &&
    a.mem_total === b.mem_total &&
    a.disk_total === b.disk_total &&
    a.swap_total === b.swap_total &&
    tagsEqual(a.tags, b.tags)
  )
})
