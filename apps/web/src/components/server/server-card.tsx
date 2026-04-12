import { Link } from '@tanstack/react-router'
import { type ComponentProps, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Bar, BarChart, Cell } from 'recharts'
import { CompactMetric } from '@/components/server/compact-metric'
import { type ChartConfig, ChartContainer, ChartTooltip } from '@/components/ui/chart'
import { RingChart } from '@/components/ui/ring-chart'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
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

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }

  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
}

function formatTooltipLabel(point: ServerCardMetricPoint): string {
  if (point.synthetic) {
    return 'Current targets'
  }

  return new Date(point.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function NetworkChartTooltip({ active, payload }: ComponentProps<typeof ChartTooltip>) {
  if (!(active && payload?.length)) {
    return null
  }

  const point = payload[0]?.payload as ServerCardMetricPoint | undefined
  if (!point || point.targets.length === 0) {
    return null
  }

  return (
    <div className="grid min-w-48 gap-1.5 rounded-lg border border-border/50 bg-background/95 px-3 py-2 text-xs shadow-xl backdrop-blur-sm">
      <div className="font-medium">{formatTooltipLabel(point)}</div>
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

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation(['servers'])
  const { data: networkOverview = [] } = useNetworkOverview()
  const { data: realtimeData } = useNetworkRealtime(server.id)

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

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4 shadow-sm">
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

      <div className="mb-3 flex justify-around">
        <RingChart color={getRingColor(server.cpu, 'var(--color-chart-1)')} label={t('col_cpu')} value={server.cpu} />
        <RingChart color={getRingColor(memoryPct, 'var(--color-chart-2)')} label={t('col_memory')} value={memoryPct} />
        <RingChart color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct} />
      </div>

      <div className="mb-1.5 grid grid-cols-5 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric className="items-center" label={t('card_load')} value={formatLoad(server.load1)} />
        <CompactMetric className="items-center" label={t('card_processes')} value={server.process_count} />
        <CompactMetric className="items-center" label={t('card_tcp')} value={server.tcp_conn} />
        <CompactMetric className="items-center" label={t('card_udp')} value={server.udp_conn} />
        <CompactMetric className="items-center" label={t('card_swap')} value={`${swapPct.toFixed(0)}%`} />
      </div>

      <div className="mb-3 grid grid-cols-4 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
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
          label={t('card_net_total')}
          value={formatBytes(server.net_in_transfer + server.net_out_transfer)}
        />
        <CompactMetric className="items-center" label={t('col_uptime')} value={formatUptime(server.uptime)} />
      </div>

      {latencyPoints.length > 0 && (
        <div className="mt-auto border-t pt-3">
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
                <span className={getLossTextClassName(currentAvgLossRatio)}>
                  {formatPacketLoss(currentAvgLossRatio)}
                </span>
              </div>
            </div>
            <figure aria-label="Latency trend" className="relative z-10 m-0">
              <ChartContainer className="aspect-auto h-7 w-full" config={LATENCY_CHART_CONFIG}>
                <BarChart
                  accessibilityLayer
                  barCategoryGap={CHART_BAR_GAP}
                  data={latencyPoints}
                  margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
                >
                  <ChartTooltip content={<NetworkChartTooltip />} cursor={false} />
                  <Bar dataKey="value" radius={2}>
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
              aria-label="Packet loss indicator"
              className="pointer-events-none mt-0.5 aspect-auto h-1 w-full"
              config={LOSS_STRIP_CONFIG}
            >
              <BarChart
                barCategoryGap={CHART_BAR_GAP}
                data={latencyPoints}
                margin={{ bottom: 0, left: 0, right: 0, top: 0 }}
              >
                <Bar dataKey={() => 1} radius={1}>
                  {latencyPoints.map((point) => (
                    <Cell fill={getLossStripColor(averageLossRatio(point))} key={point.timestamp} />
                  ))}
                </Bar>
              </BarChart>
            </ChartContainer>
          </div>
        </div>
      )}
    </div>
  )
}
