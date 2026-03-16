import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Download, Settings2 } from 'lucide-react'
import { useCallback, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { AnomalyTable } from '@/components/network/anomaly-table'
import { LatencyChart } from '@/components/network/latency-chart'
import { TargetCard } from '@/components/network/target-card'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useServer } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import {
  useNetworkAnomalies,
  useNetworkRecords,
  useNetworkServerSummary,
  useNetworkTargets,
  useSetServerTargets
} from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { NetworkProbeRecord } from '@/lib/network-types'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/_authed/network/$serverId')({
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  }),
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
  const { t } = useTranslation('network')
  const { serverId } = Route.useParams()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { range } = Route.useSearch()
  const navigate = Route.useNavigate()

  const timeRange = useMemo<TimeRangeValue>(() => {
    if (range === 'realtime') {
      return 'realtime'
    }
    const num = Number(range)
    if ([1, 6, 24, 168, 720].includes(num)) {
      return num as TimeRangeValue
    }
    return 'realtime'
  }, [range])

  const [visibleTargets, setVisibleTargets] = useState<Set<string> | null>(null)

  // Manage Targets dialog state
  const [showManageDialog, setShowManageDialog] = useState(false)
  const [selectedTargetIds, setSelectedTargetIds] = useState<Set<string>>(new Set())
  const selectedRef = useRef(selectedTargetIds)
  selectedRef.current = selectedTargetIds

  const isRealtime = timeRange === 'realtime'
  const hours = isRealtime ? 1 : timeRange

  const { data: server, isLoading: serverLoading } = useServer(serverId)
  const { data: summary, isLoading: summaryLoading } = useNetworkServerSummary(serverId)
  const { data: historicalRecords } = useNetworkRecords(serverId, hours, { enabled: !isRealtime })
  // Fetch last 10 min of data as seed for realtime chart (immediate data on first load)
  const { data: seedRecords } = useNetworkRecords(serverId, 1, { enabled: isRealtime })
  const { data: anomalies = [] } = useNetworkAnomalies(serverId, hours)
  const { data: realtimeData } = useNetworkRealtime(serverId)
  const { data: allTargets = [] } = useNetworkTargets()
  const setServerTargets = useSetServerTargets(serverId)

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
    const realtimeFlat: NetworkProbeRecord[] = []
    for (const [targetId, points] of Object.entries(realtimeData)) {
      for (const point of points) {
        realtimeFlat.push({
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
    // Merge seed (historical last 1h) with realtime data for immediate chart display.
    // Realtime points override seed points at the same timestamp via the chart's bucketing.
    const seed = seedRecords ?? []
    const merged = [...seed, ...realtimeFlat]
    // Deduplicate: keep latest entry per (target_id, timestamp_bucket)
    const seen = new Set<string>()
    const deduped: NetworkProbeRecord[] = []
    for (let i = merged.length - 1; i >= 0; i--) {
      const r = merged[i]
      const key = `${r.target_id}:${r.timestamp}`
      if (!seen.has(key)) {
        seen.add(key)
        deduped.push(r)
      }
    }
    deduped.reverse()
    return deduped
  }, [isRealtime, historicalRecords, realtimeData, serverId, seedRecords])

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
      navigate({ search: { range: String(value) } })
    },
    [navigate]
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
    toast.success('CSV exported')
  }, [records, serverId, timeRange])

  const openManageDialog = useCallback(() => {
    // Pre-select targets currently assigned to this server
    const currentIds = new Set(targets.map((t) => t.target_id))
    setSelectedTargetIds(currentIds)
    setShowManageDialog(true)
  }, [targets])

  const toggleSelectedTarget = useCallback((id: string) => {
    setSelectedTargetIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })
  }, [])

  const selectAllTargets = useCallback(() => {
    setSelectedTargetIds(new Set(allTargets.map((t) => t.id)))
  }, [allTargets])

  const deselectAllTargets = useCallback(() => {
    setSelectedTargetIds(new Set())
  }, [])

  const handleSaveTargets = useCallback(() => {
    setServerTargets.mutate(Array.from(selectedRef.current), {
      onSuccess: () => {
        toast.success('Server targets updated')
        setShowManageDialog(false)
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : 'Failed to update server targets')
      }
    })
  }, [setServerTargets])

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
        <p className="text-muted-foreground">{t('server_not_found')}</p>
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
          <ArrowLeft aria-hidden="true" className="size-4" />
          {t('back_to_overview')}
        </Link>

        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <h1 className="font-bold text-2xl">{summary.server_name}</h1>
            <StatusBadge online={summary.online} />
          </div>
          <div className="flex items-center gap-2">
            {isAdmin && (
              <Button onClick={openManageDialog} size="sm" variant="outline">
                <Settings2 aria-hidden="true" className="mr-1 size-4" />
                {t('manage_targets')}
              </Button>
            )}
            <Button disabled={records.length === 0} onClick={exportCsv} size="sm" variant="outline">
              <Download aria-hidden="true" className="mr-1 size-4" />
              {t('export_csv')}
            </Button>
          </div>
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
            {tr.value === 'realtime' ? t('realtime') : tr.label}
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
          <p className="font-mono font-semibold text-lg tabular-nums">
            {stats.avgLatency != null ? `${stats.avgLatency.toFixed(1)} ms` : 'N/A'}
          </p>
          <p className="text-muted-foreground text-xs">{t('avg_latency')}</p>
        </div>
        <div className="rounded-lg border bg-card p-4 text-center">
          <p className="font-mono font-semibold text-lg tabular-nums">{stats.availability.toFixed(1)}%</p>
          <p className="text-muted-foreground text-xs">{t('availability')}</p>
        </div>
        <div className="rounded-lg border bg-card p-4 text-center">
          <p className="font-mono font-semibold text-lg tabular-nums">{stats.targetCount}</p>
          <p className="text-muted-foreground text-xs">{t('targets')}</p>
        </div>
      </div>

      {/* Anomaly table */}
      <AnomalyTable anomalies={anomalies} />

      {/* Manage Targets Dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            setShowManageDialog(false)
          }
        }}
        open={showManageDialog}
      >
        <DialogContent className="sm:max-w-lg" showCloseButton={false}>
          <DialogHeader>
            <div className="flex items-center justify-between">
              <DialogTitle>{t('manage_targets')}</DialogTitle>
              <div className="flex gap-2">
                <Button onClick={selectAllTargets} size="sm" type="button" variant="ghost">
                  {t('select_all')}
                </Button>
                <Button onClick={deselectAllTargets} size="sm" type="button" variant="ghost">
                  {t('deselect_all')}
                </Button>
              </div>
            </div>
          </DialogHeader>

          {allTargets.length === 0 ? (
            <p className="py-4 text-center text-muted-foreground text-sm">{t('no_targets')}</p>
          ) : (
            <div className="max-h-80 space-y-1.5 overflow-y-auto rounded-md border p-3">
              {allTargets.map((target) => (
                // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                <label
                  className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 text-sm hover:bg-muted/40"
                  key={target.id}
                >
                  <Checkbox
                    checked={selectedTargetIds.has(target.id)}
                    onCheckedChange={() => toggleSelectedTarget(target.id)}
                  />
                  <span className="flex-1 font-medium">{target.name}</span>
                  {target.provider && <span className="text-muted-foreground text-xs">{target.provider}</span>}
                  {target.location && <span className="text-muted-foreground text-xs">{target.location}</span>}
                  <span className="rounded-full bg-muted px-2 py-0.5 text-xs uppercase">{target.probe_type}</span>
                </label>
              ))}
            </div>
          )}

          <div className="flex gap-2">
            <Button disabled={setServerTargets.isPending} onClick={handleSaveTargets} size="sm">
              {t('save')}
            </Button>
            <DialogClose render={<Button size="sm" variant="ghost" />}>{t('cancel')}</DialogClose>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
