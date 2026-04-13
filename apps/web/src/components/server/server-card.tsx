import { Link } from '@tanstack/react-router'
import { type ComponentProps, memo, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, Cell } from 'recharts'
import { CompactMetric } from '@/components/server/compact-metric'
import { type ChartConfig, ChartContainer, ChartTooltip } from '@/components/ui/chart'
import { RingChart } from '@/components/ui/ring-chart'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { getLatencyBarColor, isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { buildServerCardNetworkState, type ServerCardMetricPoint } from './server-card-network-data'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

const LATENCY_CHART_CONFIG = {
  value: { label: 'Latency', color: 'var(--chart-4)' }
} satisfies ChartConfig

const LOSS_STRIP_CONFIG = {
  value: { label: 'Packet Loss', color: 'var(--chart-5)' }
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

function getLossStripColor(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'var(--color-muted)'
  }
  if (lossRatio < 0.01) {
    return '#10b981'
  }
  if (lossRatio < 0.05) {
    return '#f59e0b'
  }
  return '#ef4444'
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

const DEFAULT_TRAFFIC_LIMIT_BYTES = 1024 ** 4 // 1 TiB fallback when no quota configured

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }

  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
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

  const trafficEntry = trafficOverview?.find((entry) => entry.server_id === server.id)
  const trafficUsed = trafficEntry
    ? trafficEntry.cycle_in + trafficEntry.cycle_out
    : server.net_in_transfer + server.net_out_transfer
  const trafficLimit =
    trafficEntry?.traffic_limit != null && trafficEntry.traffic_limit > 0
      ? trafficEntry.traffic_limit
      : DEFAULT_TRAFFIC_LIMIT_BYTES
  const trafficRawPct = trafficLimit > 0 ? (trafficUsed / trafficLimit) * 100 : 0
  const trafficRingPct = Math.min(trafficRawPct, 100)
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
        <StatusBadge online={server.online} />
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
          value={formatSpeed(server.net_in_speed)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_net_out_speed')}
          value={formatSpeed(server.net_out_speed)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_disk_read')}
          value={formatSpeed(server.disk_read_bytes_per_sec ?? 0)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_disk_write')}
          value={formatSpeed(server.disk_write_bytes_per_sec ?? 0)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_net_total')}
          value={formatBytes(server.net_in_transfer + server.net_out_transfer)}
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
          <div className="mb-1 flex items-center justify-between">
            <span className="text-[10px] text-muted-foreground">{t('card_latency')}</span>
            <div className="flex items-center gap-1 font-medium text-xs">
              <span
                className={latencyColorClass(currentAvgLatency, {
                  failed: isLatencyFailure(currentAvgLossRatio)
                })}
              >
                {formatLatency(currentAvgLatency)}
              </span>
              <span className="text-muted-foreground">·</span>
              <span className={getLossTextClassName(currentAvgLossRatio)}>{formatPacketLoss(currentAvgLossRatio)}</span>
            </div>
          </div>
          <figure aria-label={t('common:a11y.latency_trend')} className="relative z-10 m-0">
            <ChartContainer className="aspect-auto h-7 w-full" config={LATENCY_CHART_CONFIG}>
              <BarChart
                accessibilityLayer
                barCategoryGap={CHART_BAR_GAP}
                data={latencyPoints}
                margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
              >
                <ChartTooltip content={<NetworkChartTooltip t={t} />} cursor={false} />
                <Bar dataKey="value" isAnimationActive={false} radius={2}>
                  {latencyPoints.map((point) => (
                    <Cell
                      fill={getLatencyBarColor({
                        latencyMs: point.value,
                        failed: isLatencyFailure(averageLossRatio(point))
                      })}
                      key={point.timestamp}
                    />
                  ))}
                </Bar>
              </BarChart>
            </ChartContainer>
          </figure>
          <ChartContainer
            aria-label={t('common:a11y.packet_loss_indicator')}
            className="pointer-events-none mt-0.5 aspect-auto h-1 w-full"
            config={LOSS_STRIP_CONFIG}
          >
            <BarChart
              barCategoryGap={CHART_BAR_GAP}
              data={latencyPoints}
              margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
            >
              <Bar dataKey={() => 1} isAnimationActive={false} radius={1}>
                {latencyPoints.map((point) => (
                  <Cell fill={getLossStripColor(averageLossRatio(point))} key={point.timestamp} />
                ))}
              </Bar>
            </BarChart>
          </ChartContainer>
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
