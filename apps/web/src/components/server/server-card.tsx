import { Link } from '@tanstack/react-router'
import { type ComponentProps, memo, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart } from 'recharts'
import { CompactMetric } from '@/components/server/compact-metric'
import { type ChartConfig, ChartContainer, ChartTooltip } from '@/components/ui/chart'
import { RingChart } from '@/components/ui/ring-chart'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import {
  getCombinedBarColor,
  getCombinedSeverity,
  getLossDotBgClass,
  isLatencyFailure
} from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import { computeTrafficQuota } from '@/lib/traffic'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { buildServerCardNetworkState, type ServerCardMetricPoint } from './server-card-network-data'
import { SeverityBar, type SeverityBarDatum } from './severity-bar'
import { StatusBadge } from './status-badge'
import { UpgradeJobBadge } from './upgrade-job-badge'

interface ServerCardProps {
  server: ServerMetrics
}

const LATENCY_CHART_CONFIG = {
  value: { label: 'Latency', color: 'var(--chart-4)' }
} satisfies ChartConfig

const CHART_BAR_GAP = 2

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
    return '-'
  }
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '-'
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

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }

  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
}

function getSeverityBarData(point: ServerCardMetricPoint): SeverityBarDatum {
  const lossRatio = averageLossRatio(point)
  return {
    combinedSeverity: getCombinedSeverity({ latencyMs: point.value, lossRatio }),
    fillColor: getCombinedBarColor({ latencyMs: point.value, lossRatio }),
    lossRatio,
    value: point.value
  }
}

function formatTooltipLabel(point: ServerCardMetricPoint, t: (key: string) => string): string {
  if (point.synthetic) {
    return t('current_targets')
  }

  return new Date(point.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function NetworkChartTooltip({
  active,
  payload,
  t
}: ComponentProps<typeof ChartTooltip> & { t: (key: string) => string }) {
  if (!(active && payload?.length)) {
    return null
  }

  const point = payload[0]?.payload as ServerCardMetricPoint | undefined
  if (!point || point.targets.length === 0) {
    return null
  }

  return (
    <div className="grid min-w-48 gap-1.5 rounded-lg border border-border/50 bg-background/95 px-3 py-2 text-xs shadow-xl backdrop-blur-sm">
      <div className="font-medium">{formatTooltipLabel(point, t)}</div>
      <div className="grid gap-1.5">
        {point.targets.map((target) => {
          const failed = isLatencyFailure(target.lossRatio)

          return (
            <div className="flex items-center justify-between gap-3" key={target.targetId}>
              <span className="truncate text-muted-foreground">{target.targetName}</span>
              <div className="flex gap-2 font-medium font-mono tabular-nums">
                <span className={latencyColorClass(target.latency, { failed })}>{formatLatency(target.latency)}</span>
                <span className={getLossTextClassName(target.lossRatio)}>{formatPacketLoss(target.lossRatio)}</span>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

const ServerCardInner = ({ server }: ServerCardProps) => {
  const { t } = useTranslation(['servers'])
  const { data: networkOverview = [] } = useNetworkOverview()
  const { data: realtimeData } = useNetworkRealtime(server.id)
  const { data: trafficOverview } = useTrafficOverview()
  const upgradeJob = useUpgradeJobsStore((state) => state.jobs.get(server.id))

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const networkSummary = networkOverview.find((entry) => entry.server_id === server.id)
  const { currentAvgLatency, currentAvgLossRatio, latencyPoints } = useMemo(
    () => buildServerCardNetworkState(networkSummary, realtimeData),
    [networkSummary, realtimeData]
  )

  const severityPoints = useMemo(
    () => latencyPoints.map((point) => ({ ...point, ...getSeverityBarData(point) })),
    [latencyPoints]
  )

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

  return (
    <div className="flex flex-col rounded-lg border bg-card p-4 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <Link
          className="flex items-center gap-1.5 truncate border-transparent border-b pb-px hover:border-current"
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
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
        </Link>
        <div className="flex items-center gap-1.5">
          <UpgradeJobBadge job={upgradeJob} />
          <StatusBadge online={server.online} />
        </div>
      </div>

      <div className="mb-3 grid grid-cols-4 gap-2">
        <div className="flex flex-col items-center gap-1">
          <RingChart color={getRingColor(server.cpu, 'var(--color-chart-1)')} label={t('col_cpu')} value={server.cpu} />
          <div className="text-[10px] text-muted-foreground tabular-nums">
            {t('card_load')} <span className="font-medium text-foreground">{formatLoad(server.load1)}</span>
          </div>
        </div>
        <div className="flex flex-col items-center gap-1">
          <RingChart
            color={getRingColor(memoryPct, 'var(--color-chart-2)')}
            label={t('col_memory')}
            value={memoryPct}
          />
          <div className="text-[10px] text-muted-foreground tabular-nums">
            <span className="font-medium text-foreground">{formatBytes(server.mem_used)}</span>
            <span className="mx-0.5">/</span>
            {formatBytes(server.mem_total)}
          </div>
        </div>
        <div className="flex flex-col items-center gap-1">
          <RingChart color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct} />
          <div className="text-[10px] text-muted-foreground tabular-nums">
            <span className="font-medium text-foreground">{formatBytes(server.disk_used)}</span>
            <span className="mx-0.5">/</span>
            {formatBytes(server.disk_total)}
          </div>
        </div>
        <div className="flex flex-col items-center gap-1">
          <RingChart
            color={getRingColor(trafficRingPct, 'var(--color-chart-4)')}
            label={t('card_traffic_quota')}
            value={trafficRingPct}
          />
          <div className="text-[10px] text-muted-foreground tabular-nums">
            <span className="font-medium text-foreground">{formatBytes(trafficUsed)}</span>
            <span className="mx-0.5">/</span>
            {formatBytes(trafficLimit)}
          </div>
        </div>
      </div>

      <div className="mb-2 grid grid-cols-5 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric
          className="items-center"
          label={t('card_net_in_speed')}
          value={renderSpeedValue(server.net_in_speed)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_net_out_speed')}
          value={renderSpeedValue(server.net_out_speed)}
        />
        <CompactMetric
          className="items-center"
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
          className="items-center"
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
          className="items-center"
          label={t('card_load_trend')}
          value={`${formatLoad(server.load5)}·${formatLoad(server.load15)}`}
        />
      </div>

      <div className="mb-3 flex flex-wrap items-center justify-center gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
        <span>
          {t('col_uptime')}{' '}
          <span className="font-medium text-foreground tabular-nums">{formatUptime(server.uptime)}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_swap')} <span className="font-medium text-foreground tabular-nums">{`${swapPct.toFixed(0)}%`}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_processes')} <span className="font-medium text-foreground tabular-nums">{server.process_count}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_tcp')} <span className="font-medium text-foreground tabular-nums">{server.tcp_conn}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_udp')} <span className="font-medium text-foreground tabular-nums">{server.udp_conn}</span>
        </span>
        {trafficDaysRemaining != null && (
          <>
            <span aria-hidden="true">·</span>
            <span className="tabular-nums">{t('card_traffic_days_left', { count: trafficDaysRemaining })}</span>
          </>
        )}
      </div>

      {latencyPoints.length > 0 && (
        <div>
          <div className="mb-1 flex items-baseline justify-between">
            <span
              className={`font-semibold text-lg tabular-nums leading-none ${latencyColorClass(currentAvgLatency, {
                failed: isLatencyFailure(currentAvgLossRatio)
              })}`}
            >
              {currentAvgLatency == null || isLatencyFailure(currentAvgLossRatio) ? '—' : currentAvgLatency.toFixed(0)}
              <span className="ml-0.5 font-medium text-[10px] text-muted-foreground">ms</span>
            </span>
            <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
              <span aria-hidden="true" className={`size-1.5 rounded-full ${getLossDotBgClass(currentAvgLossRatio)}`} />
              <span>
                {t('card_packet_loss')}{' '}
                <strong className={`font-semibold tabular-nums ${getLossTextClassName(currentAvgLossRatio)}`}>
                  {formatPacketLoss(currentAvgLossRatio)}
                </strong>
              </span>
            </span>
          </div>
          <figure aria-label={t('common:a11y.latency_trend')} className="relative z-10 m-0">
            <ChartContainer className="aspect-auto h-8 w-full" config={LATENCY_CHART_CONFIG}>
              <BarChart
                accessibilityLayer
                barCategoryGap={CHART_BAR_GAP}
                data={severityPoints}
                margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
              >
                <defs>
                  <pattern
                    height="5"
                    id="latency-fail-stripe"
                    patternTransform="rotate(45)"
                    patternUnits="userSpaceOnUse"
                    width="5"
                  >
                    <rect fill="#ef4444" height="5" width="5" />
                    <line stroke="rgba(0,0,0,0.25)" strokeWidth="2" x1="0" x2="0" y1="0" y2="5" />
                  </pattern>
                </defs>
                <ChartTooltip content={<NetworkChartTooltip t={t} />} cursor={false} />
                <Bar
                  background={{ fill: 'transparent' }}
                  dataKey="value"
                  isAnimationActive={false}
                  shape={(shapeProps) => <SeverityBar {...shapeProps} failPatternId="latency-fail-stripe" />}
                />
              </BarChart>
            </ChartContainer>
          </figure>
        </div>
      )}
    </div>
  )
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
    a.swap_total === b.swap_total
  )
})
