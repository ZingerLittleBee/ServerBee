import { Link } from '@tanstack/react-router'
import { memo, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { MetricValue } from '@/components/server/metric-value'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import type { ServerCostOverview } from '@/lib/api-schema'
import { isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass, type NetworkServerSummary } from '@/lib/network-types'
import { computeTrafficQuota } from '@/lib/traffic'
import { cn, formatBytes, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CountryFlag } from '../country-flag'
import { CostFootnote } from './cost-footnote'
import { NetworkMetricValue } from './network-metric-value'
import { NetworkSquareGrid } from './network-square-grid'
import { PendingActionMenu } from './pending-action-menu'
import { PendingEnrollmentSummary } from './pending-enrollment-summary'
import { RingMetric } from './ring-metric'
import { ServerCardActionMenu } from './server-card-action-menu'
import { buildServerCardNetworkState } from './server-card-network-data'
import { StatusBadge } from './status-badge'
import { deriveServerStatus } from './status-dot-utils'
import { TagChips } from './tag-chips'
import { UpgradeJobBadge } from './upgrade-job-badge'

interface ServerCardProps {
  costEntry?: ServerCostOverview
  networkBucketSeconds?: number
  networkSummary?: NetworkServerSummary
  server: ServerMetrics
  trafficEntry?: TrafficOverviewItem
}

const DEFAULT_NETWORK_BUCKET_SECONDS = 60

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

const ServerCardInner = ({
  server,
  trafficEntry,
  costEntry,
  networkSummary,
  networkBucketSeconds = DEFAULT_NETWORK_BUCKET_SECONDS
}: ServerCardProps) => {
  const { t } = useTranslation(['servers'])
  const { data: realtimeData } = useNetworkRealtime(server.id)
  const upgradeJob = useUpgradeJobsStore((state) => state.jobs.get(server.id))

  const status = deriveServerStatus(server)
  const isPending = status === 'pending'

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const osEmoji = osIcon(server.os)

  const { currentAvgLatency, currentAvgLossRatio, currentTargets, latencyPoints, lossPoints } = useMemo(
    () => buildServerCardNetworkState(networkSummary, realtimeData, networkBucketSeconds),
    [networkSummary, realtimeData, networkBucketSeconds]
  )

  const hasNetworkData = latencyPoints.length > 0

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

  return (
    <div
      className={cn(
        'relative flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm',
        // Pending cards have far less content than active ones; stretch them to
        // fill the grid cell so a "Waiting for agent…" tile matches the height of
        // its data-rich siblings instead of leaving a short, mismatched gap.
        isPending && 'h-full'
      )}
    >
      {!server.online && (
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 z-10 rounded-lg bg-background/55 backdrop-grayscale"
        />
      )}
      <div className="flex items-center justify-between">
        <Link
          className="flex items-center gap-1 truncate border-transparent border-b pb-px hover:border-current"
          params={{ id: server.id }}
          search={{ range: 'realtime' }}
          to="/servers/$id"
        >
          <CountryFlag className="text-sm" code={server.country_code} />
          {osEmoji && (
            <span className="shrink-0 text-sm" title={server.os ?? ''}>
              {osEmoji}
            </span>
          )}
          <h3 className="truncate font-semibold text-[13px]">{server.name}</h3>
        </Link>
        <div className="flex items-center gap-1.5">
          <UpgradeJobBadge job={upgradeJob} />
          {/* Lift the pending pill above the dim overlay so it keeps its amber
              tone as the one bright "needs attention" cue on a muted card. */}
          <StatusBadge className={isPending ? 'relative z-20' : undefined} status={status} />
          {isPending ? (
            <PendingActionMenu serverId={server.id} serverName={server.name} />
          ) : (
            <ServerCardActionMenu server={server} />
          )}
        </div>
      </div>

      {isPending ? (
        <div className="flex min-h-24 flex-1 flex-col items-center justify-center gap-1 rounded-md bg-muted/40 px-3 py-3 text-center">
          <p className="font-medium text-foreground text-sm">{t('card_pending.waiting')}</p>
          <PendingEnrollmentSummary enrollment={server.outstanding_enrollment} />
        </div>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-x-3 gap-y-2">
            <RingMetric
              color={getRingColor(server.cpu, 'var(--color-chart-1)')}
              label={t('col_cpu')}
              value={server.cpu}
            >
              {t('card_load')} <span className="font-medium text-foreground">{formatLoad(server.load1)}</span>
            </RingMetric>
            <RingMetric
              color={getRingColor(memoryPct, 'var(--color-chart-2)')}
              label={t('col_memory')}
              value={memoryPct}
            >
              <span className="font-medium text-foreground">{formatBytes(server.mem_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.mem_total)}
            </RingMetric>
            <RingMetric color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct}>
              <span className="font-medium text-foreground">{formatBytes(server.disk_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.disk_total)}
            </RingMetric>
            <RingMetric
              color={getRingColor(trafficRingPct, 'var(--color-chart-4)')}
              label={t('card_traffic_quota')}
              value={trafficRingPct}
            >
              <span className="font-medium text-foreground">{formatBytes(trafficUsed)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(trafficLimit)}
            </RingMetric>
          </div>

          <div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
            <CompactMetric
              label={t('card_net_in_speed')}
              value={<MetricValue kind="speed" value={server.net_in_speed} variant="compact" />}
            />
            <CompactMetric
              label={t('card_net_out_speed')}
              value={<MetricValue kind="speed" value={server.net_out_speed} variant="compact" />}
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
                  {t('card_disk_read')}
                </span>
              }
              value={<MetricValue kind="speed" value={server.disk_read_bytes_per_sec} variant="compact" />}
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
                  {t('card_disk_write')}
                </span>
              }
              value={<MetricValue kind="speed" value={server.disk_write_bytes_per_sec} variant="compact" />}
            />
          </div>

          {hasNetworkData && (
            <section aria-label={t('card_network_quality')} className="grid grid-cols-2 gap-x-3 gap-y-1">
              <div className="flex items-baseline justify-between">
                <span className="text-[11px] text-muted-foreground">{t('card_latency')}</span>
                <NetworkMetricValue targets={currentTargets}>
                  <span
                    className={`cursor-default font-semibold text-xs tabular-nums ${latencyColorClass(
                      currentAvgLatency,
                      {
                        failed: isLatencyFailure(currentAvgLossRatio)
                      }
                    )}`}
                  >
                    {formatLatency(currentAvgLatency)}
                    <span className="ml-0.5 font-medium text-[10px] text-muted-foreground">ms</span>
                  </span>
                </NetworkMetricValue>
              </div>
              <div className="flex items-baseline justify-between">
                <span className="text-[11px] text-muted-foreground">{t('card_packet_loss')}</span>
                <NetworkMetricValue targets={currentTargets}>
                  <span
                    className={`cursor-default font-semibold text-xs tabular-nums ${getLossTextClassName(currentAvgLossRatio)}`}
                  >
                    {formatPacketLoss(currentAvgLossRatio)}
                  </span>
                </NetworkMetricValue>
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
              <span>{t('card_load_trend')}</span>
              <span className="inline-flex items-center gap-1.5 font-medium text-foreground tabular-nums">
                <span>{formatLoad(server.load5)}</span>
                <span aria-hidden="true">·</span>
                <span>{formatLoad(server.load15)}</span>
              </span>
            </div>
            <div className="flex items-baseline justify-between">
              <span>{t('card_proc_conn_label')}</span>
              <span className="font-medium text-foreground tabular-nums">
                {`${server.process_count} / ${server.tcp_conn} / ${server.udp_conn}`}
              </span>
            </div>
            {trafficDaysRemaining == null ? (
              <div aria-hidden="true" className="invisible flex items-baseline justify-between">
                <span>—</span>
              </div>
            ) : (
              <div className="flex items-baseline justify-between">
                <span>{t('card_traffic_days_left_label')}</span>
                <span className="font-medium text-foreground tabular-nums">
                  {t('card_traffic_days_value', { count: trafficDaysRemaining })}
                </span>
              </div>
            )}
            {costEntry?.configured ? (
              <div className="flex items-baseline justify-between">
                <span>{t('card_cost_label')}</span>
                <CostFootnote entry={costEntry} inline />
              </div>
            ) : (
              <div aria-hidden="true" className="invisible flex items-baseline justify-between">
                <span>—</span>
              </div>
            )}
          </div>

          <TagChips tags={server.tags} />
        </>
      )}
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
  if (
    prev.trafficEntry !== next.trafficEntry ||
    prev.costEntry !== next.costEntry ||
    prev.networkSummary !== next.networkSummary ||
    prev.networkBucketSeconds !== next.networkBucketSeconds
  ) {
    return false
  }
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
    a.has_token === b.has_token &&
    a.outstanding_enrollment?.id === b.outstanding_enrollment?.id &&
    a.outstanding_enrollment?.expires_at === b.outstanding_enrollment?.expires_at &&
    tagsEqual(a.tags, b.tags)
  )
})
