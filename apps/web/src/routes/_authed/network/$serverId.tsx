import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Download } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { AnomalyTable } from '@/components/network/anomaly-table'
import { LatencyChart } from '@/components/network/latency-chart'
import { TargetCard } from '@/components/network/target-card'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { useServer } from '@/hooks/use-api'
import { useNetworkAnomalies, useNetworkRecords, useNetworkServerSummary } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { NetworkProbeRecord } from '@/lib/network-types'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/_authed/network/$serverId')({
  component: NetworkDetailPage
})

const COLOR_PALETTE = [
  '#3b82f6',
  '#ef4444',
  '#22c55e',
  '#f59e0b',
  '#8b5cf6',
  '#ec4899',
  '#14b8a6',
  '#f97316',
  '#6366f1',
  '#06b6d4',
  '#84cc16',
  '#e11d48'
]

type TimeRangeValue = 'realtime' | 1 | 6 | 24 | 168 | 720

interface TimeRangeOption {
  label: string
  value: TimeRangeValue
}

const TIME_RANGES: TimeRangeOption[] = [
  { label: 'Realtime', value: 'realtime' },
  { label: '1h', value: 1 },
  { label: '6h', value: 6 },
  { label: '24h', value: 24 },
  { label: '7d', value: 168 },
  { label: '30d', value: 720 }
]

function NetworkDetailPage() {
  const { serverId } = Route.useParams()
  const [timeRange, setTimeRange] = useState<TimeRangeValue>('realtime')
  const [visibleTargets, setVisibleTargets] = useState<Set<string> | null>(null)

  const isRealtime = timeRange === 'realtime'
  const hours = isRealtime ? 1 : timeRange

  const { data: server, isLoading: serverLoading } = useServer(serverId)
  const { data: summary, isLoading: summaryLoading } = useNetworkServerSummary(serverId)
  const { data: historicalRecords } = useNetworkRecords(serverId, hours, { enabled: !isRealtime })
  const { data: anomalies = [] } = useNetworkAnomalies(serverId, hours)
  const { data: realtimeData, reset: resetRealtime } = useNetworkRealtime(serverId)

  const targets = useMemo(() => summary?.targets ?? [], [summary])

  // Initialize visible targets to all when summary loads
  const effectiveVisible = useMemo(() => {
    if (visibleTargets != null) {
      return visibleTargets
    }
    return new Set(targets.map((t) => t.target_id))
  }, [visibleTargets, targets])

  const toggleTarget = useCallback(
    (targetId: string) => {
      setVisibleTargets((prev) => {
        const current = prev ?? new Set(targets.map((t) => t.target_id))
        const next = new Set(current)
        if (next.has(targetId)) {
          next.delete(targetId)
        } else {
          next.add(targetId)
        }
        return next
      })
    },
    [targets]
  )

  const targetColorMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (let i = 0; i < targets.length; i++) {
      map[targets[i].target_id] = COLOR_PALETTE[i % COLOR_PALETTE.length]
    }
    return map
  }, [targets])

  const chartTargets = useMemo(
    () =>
      targets.map((t) => ({
        id: t.target_id,
        name: t.target_name,
        color: targetColorMap[t.target_id] ?? COLOR_PALETTE[0],
        visible: effectiveVisible.has(t.target_id)
      })),
    [targets, targetColorMap, effectiveVisible]
  )

  const records: NetworkProbeRecord[] = useMemo(() => {
    if (!isRealtime) {
      return historicalRecords ?? []
    }
    // Transform realtime data map into flat records array
    const flat: NetworkProbeRecord[] = []
    for (const [targetId, points] of Object.entries(realtimeData)) {
      for (const point of points) {
        flat.push({
          id: 0,
          server_id: serverId,
          target_id: targetId,
          timestamp: point.timestamp,
          avg_latency: point.avg_latency,
          min_latency: point.min_latency,
          max_latency: point.max_latency,
          packet_loss: point.packet_loss,
          packet_sent: point.packet_sent,
          packet_received: point.packet_received
        })
      }
    }
    return flat
  }, [isRealtime, historicalRecords, realtimeData, serverId])

  // Stats computed from current records
  const stats = useMemo(() => {
    const validRecords = records.filter((r) => r.avg_latency != null)
    const avgLatency =
      validRecords.length > 0
        ? validRecords.reduce((sum, r) => sum + (r.avg_latency ?? 0), 0) / validRecords.length
        : null

    const totalSent = records.reduce((sum, r) => sum + r.packet_sent, 0)
    const totalReceived = records.reduce((sum, r) => sum + r.packet_received, 0)
    const availability = totalSent > 0 ? (totalReceived / totalSent) * 100 : 100

    const targetCount = new Set(records.map((r) => r.target_id)).size

    return { avgLatency, availability, targetCount }
  }, [records])

  const handleTimeRangeChange = useCallback(
    (value: TimeRangeValue) => {
      if (value === 'realtime' && timeRange !== 'realtime') {
        resetRealtime()
      }
      setTimeRange(value)
    },
    [timeRange, resetRealtime]
  )

  const exportCsv = useCallback(() => {
    if (records.length === 0) {
      return
    }
    const header = 'timestamp,target_id,avg_latency,min_latency,max_latency,packet_loss,packet_sent,packet_received'
    const rows = records.map(
      (r) =>
        `${r.timestamp},${r.target_id},${r.avg_latency ?? ''},${r.min_latency ?? ''},${r.max_latency ?? ''},${r.packet_loss},${r.packet_sent},${r.packet_received}`
    )
    const csv = [header, ...rows].join('\n')
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = `network-${serverId}-${timeRange}.csv`
    link.click()
    URL.revokeObjectURL(url)
  }, [records, serverId, timeRange])

  if (serverLoading || summaryLoading) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  if (!(server && summary)) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">Server not found</p>
      </div>
    )
  }

  return (
    <div>
      {/* Header */}
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/network"
        >
          <ArrowLeft className="size-4" />
          Back to overview
        </Link>

        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <h1 className="font-bold text-2xl">{summary.server_name}</h1>
            <StatusBadge online={summary.online} />
          </div>
          <Button disabled={records.length === 0} onClick={exportCsv} size="sm" variant="outline">
            <Download className="mr-1 size-4" />
            Export CSV
          </Button>
        </div>
      </div>

      {/* Server info bar */}
      <div className="mb-6 flex flex-wrap gap-x-4 gap-y-1 rounded-lg border bg-card p-3 text-muted-foreground text-sm">
        {server.ipv4 && <span>IPv4: {server.ipv4}</span>}
        {server.ipv6 && <span>IPv6: {server.ipv6}</span>}
        {server.region && <span>Region: {server.region}</span>}
        {server.os && <span>OS: {server.os}</span>}
        {summary.last_probe_at && (
          <span>
            Last probe:{' '}
            {new Date(summary.last_probe_at).toLocaleString([], {
              month: 'short',
              day: 'numeric',
              hour: '2-digit',
              minute: '2-digit'
            })}
          </span>
        )}
      </div>

      {/* Time range selector */}
      <div className="mb-4 flex gap-1">
        {TIME_RANGES.map((tr) => (
          <Button
            className={cn(timeRange === tr.value && 'bg-primary text-primary-foreground')}
            key={tr.value}
            onClick={() => handleTimeRangeChange(tr.value)}
            size="sm"
            variant={timeRange === tr.value ? 'default' : 'outline'}
          >
            {tr.label}
          </Button>
        ))}
      </div>

      {/* Target cards */}
      {targets.length > 0 && (
        <div className="mb-4 flex flex-wrap gap-2">
          {targets.map((target) => (
            <TargetCard
              color={targetColorMap[target.target_id] ?? COLOR_PALETTE[0]}
              key={target.target_id}
              onToggle={() => toggleTarget(target.target_id)}
              target={target}
              visible={effectiveVisible.has(target.target_id)}
            />
          ))}
        </div>
      )}

      {/* Latency chart */}
      <div className="mb-4">
        <LatencyChart isRealtime={isRealtime} records={records} targets={chartTargets} />
      </div>

      {/* Bottom stats */}
      <div className="mb-6 grid grid-cols-3 gap-4">
        <div className="rounded-lg border bg-card p-4 text-center">
          <p className="font-mono font-semibold text-lg">
            {stats.avgLatency != null ? `${stats.avgLatency.toFixed(1)} ms` : 'N/A'}
          </p>
          <p className="text-muted-foreground text-xs">Avg Latency</p>
        </div>
        <div className="rounded-lg border bg-card p-4 text-center">
          <p className="font-mono font-semibold text-lg">{stats.availability.toFixed(1)}%</p>
          <p className="text-muted-foreground text-xs">Availability</p>
        </div>
        <div className="rounded-lg border bg-card p-4 text-center">
          <p className="font-mono font-semibold text-lg">{stats.targetCount}</p>
          <p className="text-muted-foreground text-xs">Targets</p>
        </div>
      </div>

      {/* Anomaly table */}
      <AnomalyTable anomalies={anomalies} />
    </div>
  )
}
