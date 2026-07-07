import { AlertTriangle, Download, Route as RouteIcon, Settings2 } from 'lucide-react'
import { useCallback, useMemo, useReducer, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { LatencyChart } from '@/components/network/latency-chart'
import { ManageTargetsDialog } from '@/components/network/manage-targets-dialog'
import { TracerouteDialog } from '@/components/network/traceroute-dialog'
import { NetworkDetailContent } from '@/components/status/network-detail-content'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import {
  useNetworkAnomalies,
  useNetworkServerSummary,
  useNetworkTargets,
  useSetServerTargets
} from '@/hooks/use-network-api'
import { useNetworkChartRecords } from '@/hooks/use-network-chart-records'
import { CHART_COLORS } from '@/lib/chart-colors'
import { formatDateTime } from '@/lib/format'
import { getNetworkTargetDisplayName } from '@/lib/network-i18n'
import type { NetworkProbeAnomaly, NetworkTargetSummary } from '@/lib/network-types'
import { cn } from '@/lib/utils'

// The network tab shares the server-detail `range` search param, which uses
// metrics-style keys; probe queries want a window in hours.
const RANGE_KEY_TO_HOURS: Record<string, number> = {
  '1h': 1,
  '6h': 6,
  '24h': 24,
  '7d': 168,
  '30d': 720
}

const RANGE_KEYS = ['realtime', '1h', '6h', '24h', '7d', '30d'] as const

export interface NetworkTabProps {
  /** Called when the viewer picks a new range; keys are metrics-style. */
  onRangeChange?: (rangeKey: string) => void
  /** Metrics-style range key from the shared `range` search param. */
  rangeKey?: string
  serverId: string
}

interface NetworkTabState {
  anomalyOpen: boolean
  selectedTargetIds: Set<string>
  showManageDialog: boolean
  showTracerouteDialog: boolean
  visibleTargets: Set<string> | null
}

type NetworkTabAction =
  | { type: 'set-anomaly-open'; value: boolean }
  | { type: 'set-manage-dialog-open'; value: boolean }
  | { type: 'set-selected-target-ids'; value: Set<string> }
  | { type: 'set-traceroute-dialog-open'; value: boolean }
  | { type: 'set-visible-targets'; value: Set<string> | null }

const INITIAL_NETWORK_TAB_STATE: NetworkTabState = {
  anomalyOpen: false,
  selectedTargetIds: new Set(),
  showManageDialog: false,
  showTracerouteDialog: false,
  visibleTargets: null
}

function networkTabReducer(state: NetworkTabState, action: NetworkTabAction): NetworkTabState {
  switch (action.type) {
    case 'set-anomaly-open':
      return { ...state, anomalyOpen: action.value }
    case 'set-manage-dialog-open':
      return { ...state, showManageDialog: action.value }
    case 'set-selected-target-ids':
      return { ...state, selectedTargetIds: action.value }
    case 'set-traceroute-dialog-open':
      return { ...state, showTracerouteDialog: action.value }
    case 'set-visible-targets':
      return { ...state, visibleTargets: action.value }
    default:
      return state
  }
}

function TimeRangeControls({
  onChange,
  rangeKey,
  t
}: {
  onChange: (rangeKey: string) => void
  rangeKey: string
  t: (key: string, opts?: Record<string, unknown>) => string
}) {
  return (
    <div className="mb-4 flex gap-1">
      {RANGE_KEYS.map((key) => (
        <Button
          className={cn(rangeKey === key && 'bg-primary text-primary-foreground')}
          key={key}
          onClick={() => onChange(key)}
          size="sm"
          variant={rangeKey === key ? 'default' : 'outline'}
        >
          {key === 'realtime' ? t('realtime') : key}
        </Button>
      ))}
    </div>
  )
}

function NetworkStatsGrid({
  anomalies,
  onAnomalyClick,
  stats,
  t
}: {
  anomalies: NetworkProbeAnomaly[]
  onAnomalyClick: () => void
  stats: { availability: number; avgLatency: number | null; targetCount: number }
  t: (key: string, opts?: Record<string, unknown>) => string
}) {
  return (
    <div className="mb-6 grid gap-4 sm:grid-cols-4">
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
      <button
        aria-label={t('anomaly_count')}
        className={cn(
          'cursor-pointer rounded-lg border bg-card p-4 text-center transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          anomalies.length > 0 &&
            'border-amber-300 bg-amber-50 hover:bg-amber-100 dark:border-amber-900/50 dark:bg-amber-900/20 dark:hover:bg-amber-900/30'
        )}
        onClick={onAnomalyClick}
        type="button"
      >
        <p
          className={cn(
            'flex items-center justify-center gap-1.5 font-mono font-semibold text-lg tabular-nums',
            anomalies.length > 0 && 'text-amber-700 dark:text-amber-400'
          )}
        >
          {anomalies.length > 0 && <AlertTriangle aria-hidden="true" className="size-4" />}
          {anomalies.length}
        </p>
        <p className="text-muted-foreground text-xs">{t('anomaly_count')}</p>
      </button>
    </div>
  )
}

function NetworkTabToolbar({
  canExport,
  isAdmin,
  lastProbeAt,
  onExport,
  onManageTargets,
  onOpenTraceroute,
  t
}: {
  canExport: boolean
  isAdmin: boolean
  lastProbeAt: string | null
  onExport: () => void
  onManageTargets: () => void
  onOpenTraceroute: () => void
  t: (key: string, opts?: Record<string, unknown>) => string
}) {
  return (
    <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
      <span className="text-muted-foreground text-sm">
        {lastProbeAt
          ? `${t('last_probe')}: ${formatDateTime(lastProbeAt, {
              month: 'short',
              day: 'numeric',
              hour: '2-digit',
              minute: '2-digit'
            })}`
          : null}
      </span>
      <div className="flex items-center gap-2">
        <Button onClick={onOpenTraceroute} size="sm" variant="outline">
          <RouteIcon aria-hidden="true" className="mr-1 size-4" />
          {t('traceroute')}
        </Button>
        {isAdmin && (
          <Button onClick={onManageTargets} size="sm" variant="outline">
            <Settings2 aria-hidden="true" className="mr-1 size-4" />
            {t('manage_targets')}
          </Button>
        )}
        <Button disabled={!canExport} onClick={onExport} size="sm" variant="outline">
          <Download aria-hidden="true" className="mr-1 size-4" />
          {t('export_csv')}
        </Button>
      </div>
    </div>
  )
}

export function NetworkTab({ onRangeChange, rangeKey, serverId }: NetworkTabProps) {
  const { i18n, t } = useTranslation('network')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const effectiveRangeKey = RANGE_KEYS.includes(rangeKey as (typeof RANGE_KEYS)[number])
    ? (rangeKey as string)
    : 'realtime'
  const isRealtime = effectiveRangeKey === 'realtime'
  const hours = isRealtime ? 1 : RANGE_KEY_TO_HOURS[effectiveRangeKey]
  // Anomalies are historical events; in realtime mode use 24h so the count matches
  // the badge shown on the network overview card, which is also 24h-based.
  const anomalyHours = isRealtime ? 24 : hours

  const [state, dispatch] = useReducer(networkTabReducer, INITIAL_NETWORK_TAB_STATE)
  const selectedRef = useRef(state.selectedTargetIds)
  selectedRef.current = state.selectedTargetIds

  const { data: summary, isLoading: summaryLoading } = useNetworkServerSummary(serverId)
  const { data: anomalies = [] } = useNetworkAnomalies(serverId, anomalyHours)
  const { data: allTargets = [] } = useNetworkTargets()
  const setServerTargets = useSetServerTargets(serverId)
  const language = i18n.resolvedLanguage ?? i18n.language
  const targetMetadataById = useMemo(() => new Map(allTargets.map((target) => [target.id, target])), [allTargets])
  const getSummaryTargetDisplayName = useCallback(
    (target: NetworkTargetSummary) => {
      const targetMetadata = targetMetadataById.get(target.target_id)
      if (!targetMetadata) {
        return target.target_name
      }

      return getNetworkTargetDisplayName(t, language, targetMetadata)
    },
    [language, t, targetMetadataById]
  )

  const targets = useMemo(() => summary?.targets ?? [], [summary])

  // Initialize visible targets to all when summary loads
  const effectiveVisible = useMemo(() => {
    if (state.visibleTargets != null) {
      return state.visibleTargets
    }
    return new Set(targets.map((t) => t.target_id))
  }, [state.visibleTargets, targets])

  const toggleTarget = useCallback(
    (targetId: string) => {
      const current = state.visibleTargets ?? new Set(targets.map((t) => t.target_id))
      const next = new Set(current)
      if (next.has(targetId)) {
        next.delete(targetId)
      } else {
        next.add(targetId)
      }
      dispatch({ type: 'set-visible-targets', value: next })
    },
    [state.visibleTargets, targets]
  )

  const targetColorMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (let i = 0; i < targets.length; i++) {
      map[targets[i].target_id] = CHART_COLORS[i % CHART_COLORS.length]
    }
    return map
  }, [targets])

  const chartTargets = useMemo(
    () =>
      targets.map((t) => ({
        id: t.target_id,
        name: getSummaryTargetDisplayName(t),
        color: targetColorMap[t.target_id] ?? CHART_COLORS[0],
        visible: effectiveVisible.has(t.target_id)
      })),
    [targets, targetColorMap, effectiveVisible, getSummaryTargetDisplayName]
  )

  const { records } = useNetworkChartRecords(serverId, isRealtime ? 0 : hours)

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
    link.download = `network-${serverId}-${effectiveRangeKey}.csv`
    link.click()
    URL.revokeObjectURL(url)
    toast.success(t('export_csv_success', { defaultValue: 'CSV exported' }))
  }, [records, serverId, effectiveRangeKey, t])

  const openManageDialog = useCallback(() => {
    // Pre-select targets currently assigned to this server
    const currentIds = new Set(targets.map((t) => t.target_id))
    dispatch({ type: 'set-selected-target-ids', value: currentIds })
    dispatch({ type: 'set-manage-dialog-open', value: true })
  }, [targets])

  const toggleSelectedTarget = useCallback((id: string) => {
    const next = new Set(selectedRef.current)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
    }
    dispatch({ type: 'set-selected-target-ids', value: next })
  }, [])

  const selectAllTargets = useCallback(() => {
    dispatch({ type: 'set-selected-target-ids', value: new Set(allTargets.map((t) => t.id)) })
  }, [allTargets])

  const deselectAllTargets = useCallback(() => {
    dispatch({ type: 'set-selected-target-ids', value: new Set() })
  }, [])

  const handleSaveTargets = useCallback(() => {
    setServerTargets.mutate(Array.from(selectedRef.current), {
      onSuccess: () => {
        toast.success(t('server_targets_updated', { defaultValue: 'Server targets updated' }))
        dispatch({ type: 'set-manage-dialog-open', value: false })
      },
      onError: (err) => {
        toast.error(
          err instanceof Error
            ? err.message
            : t('server_targets_update_failed', { defaultValue: 'Failed to update server targets' })
        )
      }
    })
  }, [setServerTargets, t])

  if (summaryLoading) {
    return (
      <div className="flex min-h-[300px] items-center justify-center">
        <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  return (
    <div className="mt-4">
      <NetworkTabToolbar
        canExport={records.length > 0}
        isAdmin={isAdmin}
        lastProbeAt={summary?.last_probe_at ?? null}
        onExport={exportCsv}
        onManageTargets={openManageDialog}
        onOpenTraceroute={() => dispatch({ type: 'set-traceroute-dialog-open', value: true })}
        t={t}
      />

      {summary ? (
        <NetworkDetailContent
          anomalies={anomalies}
          anomalyOpen={state.anomalyOpen}
          anomalyWindowHours={anomalyHours}
          chartSlot={
            <div className="mb-4">
              <LatencyChart hours={hours} isRealtime={isRealtime} records={records} targets={chartTargets} />
            </div>
          }
          controlsSlot={
            <TimeRangeControls onChange={(key) => onRangeChange?.(key)} rangeKey={effectiveRangeKey} t={t} />
          }
          extraStatsSlot={
            <NetworkStatsGrid
              anomalies={anomalies}
              onAnomalyClick={() => dispatch({ type: 'set-anomaly-open', value: true })}
              stats={stats}
              t={t}
            />
          }
          getTargetDisplayName={getSummaryTargetDisplayName}
          onAnomalyOpenChange={(open) => dispatch({ type: 'set-anomaly-open', value: open })}
          onToggleTarget={toggleTarget}
          summary={summary}
          variant="admin"
          visibleTargetIds={effectiveVisible}
        />
      ) : (
        <div className="mb-4 rounded-lg border bg-card p-12 text-center text-muted-foreground text-sm">
          {t('no_targets')}
        </div>
      )}

      <TracerouteDialog
        onOpenChange={(open) => dispatch({ type: 'set-traceroute-dialog-open', value: open })}
        open={state.showTracerouteDialog}
        serverId={serverId}
      />

      <ManageTargetsDialog
        allTargets={allTargets}
        isPending={setServerTargets.isPending}
        onDeselectAll={deselectAllTargets}
        onOpenChange={(open) => dispatch({ type: 'set-manage-dialog-open', value: open })}
        onSave={handleSaveTargets}
        onSelectAll={selectAllTargets}
        onToggleTarget={toggleSelectedTarget}
        open={state.showManageDialog}
        selectedTargetIds={state.selectedTargetIds}
      />
    </div>
  )
}
