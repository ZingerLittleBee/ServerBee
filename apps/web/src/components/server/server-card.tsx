import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { RingChart } from '@/components/ui/ring-chart'
import { UptimeBar } from '@/components/ui/uptime-bar'
import { useNetworkOverview } from '@/hooks/use-network-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { NetworkTargetSummary } from '@/lib/network-types'
import { SPARKLINE_LENGTH, seedFromSummary, summaryStats, toBarData } from '@/lib/sparkline'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

const NULL_BAR_COLOR = 'var(--color-muted)'
const EMPTY_TREND = Array.from({ length: SPARKLINE_LENGTH }, (): number | null => null)

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

function getLatencyColor(ms: number | null): string {
  if (ms == null) {
    return NULL_BAR_COLOR
  }
  if (ms < 50) {
    return '#10b981'
  }
  if (ms < 100) {
    return '#f59e0b'
  }
  return '#ef4444'
}

function getLossColor(loss: number | null): string {
  if (loss == null) {
    return NULL_BAR_COLOR
  }
  if (loss < 1) {
    return '#10b981'
  }
  if (loss < 5) {
    return '#f59e0b'
  }
  return '#ef4444'
}

function getLatencyTextClass(ms: number | null): string {
  if (ms == null) {
    return 'text-muted-foreground'
  }
  if (ms < 50) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (ms < 100) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function getLossTextClass(loss: number | null): string {
  if (loss == null) {
    return 'text-muted-foreground'
  }
  if (loss < 1) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (loss < 5) {
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

function formatPacketLoss(loss: number | null): string {
  if (loss == null) {
    return '-'
  }
  return `${loss.toFixed(1)}%`
}

function formatLoad(load: number): string {
  return load.toFixed(2)
}

function hasTrendData(data: readonly (number | null)[]): boolean {
  return data.some((value) => value != null)
}

function hasTargetSample(target: NetworkTargetSummary): boolean {
  return target.avg_latency != null || target.packet_loss > 0 || target.availability > 0
}

function summarizeTargets(targets: readonly NetworkTargetSummary[]): {
  avgLatency: number | null
  avgLoss: number | null
} {
  const sampledTargets = targets.filter(hasTargetSample)
  if (sampledTargets.length === 0) {
    return { avgLatency: null, avgLoss: null }
  }

  const latencyTargets = sampledTargets.filter((target) => target.avg_latency != null)
  const avgLatency =
    latencyTargets.length === 0
      ? null
      : latencyTargets.reduce((sum, target) => sum + (target.avg_latency ?? 0), 0) / latencyTargets.length
  const avgLoss = (sampledTargets.reduce((sum, target) => sum + target.packet_loss, 0) / sampledTargets.length) * 100

  return { avgLatency, avgLoss }
}

function fallbackTrend(value: number | null): (number | null)[] {
  if (value == null) {
    return EMPTY_TREND
  }

  const trend = [...EMPTY_TREND]
  trend[trend.length - 1] = value
  return trend
}

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation(['servers'])
  const { data: networkOverview = [] } = useNetworkOverview()

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const { latencyData, lossData, avgLatency, avgLoss, hasAnyData } = useMemo(() => {
    const summary = networkOverview.find((entry) => entry.server_id === server.id)

    if (!summary) {
      return { latencyData: EMPTY_TREND, lossData: EMPTY_TREND, avgLatency: null, avgLoss: null, hasAnyData: false }
    }

    const points = seedFromSummary(summary)
    const sparklineLatencyData = toBarData(points, 'latency')
    const sparklineLossData = toBarData(points, 'lossPercent')
    const sparklineStats = summaryStats(points)
    const targetStats = summarizeTargets(summary.targets)
    const avgLatency = targetStats.avgLatency ?? sparklineStats.avgLatency
    const avgLoss = targetStats.avgLoss ?? (sparklineStats.avgLoss == null ? null : sparklineStats.avgLoss * 100)
    const latencyData = hasTrendData(sparklineLatencyData) ? sparklineLatencyData : fallbackTrend(avgLatency)
    const lossData = hasTrendData(sparklineLossData) ? sparklineLossData : fallbackTrend(avgLoss)
    const hasAnyData = avgLatency != null || avgLoss != null || hasTrendData(latencyData) || hasTrendData(lossData)

    return {
      latencyData,
      lossData,
      avgLatency,
      avgLoss,
      hasAnyData
    }
  }, [networkOverview, server.id])

  return (
    <Link
      className="group flex h-full flex-col rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
      params={{ id: server.id }}
      search={{ range: 'realtime' }}
      to="/servers/$id"
    >
      {/* Header */}
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-1.5 truncate">
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
        </div>
        <StatusBadge online={server.online} />
      </div>

      {/* Ring Charts */}
      <div className="mb-3 flex justify-around">
        <RingChart color={getRingColor(server.cpu, 'var(--color-chart-1)')} label={t('col_cpu')} value={server.cpu} />
        <RingChart color={getRingColor(memoryPct, 'var(--color-chart-2)')} label={t('col_memory')} value={memoryPct} />
        <RingChart color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct} />
      </div>

      {/* System Metrics Row */}
      <div className="mb-1.5 grid grid-cols-5 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric className="items-center" label={t('card_load')} value={formatLoad(server.load1)} />
        <CompactMetric className="items-center" label={t('card_processes')} value={server.process_count} />
        <CompactMetric className="items-center" label={t('card_tcp')} value={server.tcp_conn} />
        <CompactMetric className="items-center" label={t('card_udp')} value={server.udp_conn} />
        <CompactMetric className="items-center" label={t('card_swap')} value={`${swapPct.toFixed(0)}%`} />
      </div>

      {/* Network Metrics Row */}
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

      {/* Network Quality */}
      {hasAnyData && (
        <div className="mt-auto border-t pt-3">
          <div className="mb-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_latency')}</span>
              <span className={`font-medium text-xs ${getLatencyTextClass(avgLatency)}`}>
                {formatLatency(avgLatency)}
              </span>
            </div>
            <UptimeBar ariaLabel="Latency trend" data={latencyData} getColor={getLatencyColor} />
          </div>
          <div>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_packet_loss')}</span>
              <span className={`font-medium text-xs ${getLossTextClass(avgLoss)}`}>{formatPacketLoss(avgLoss)}</span>
            </div>
            <UptimeBar ariaLabel="Packet loss trend" data={lossData} getColor={getLossColor} />
          </div>
        </div>
      )}
    </Link>
  )
}
