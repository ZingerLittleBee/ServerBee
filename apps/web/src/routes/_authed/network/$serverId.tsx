import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Check, Download, Loader2, Play, Route as RouteIcon, Settings2, Trash2, X } from 'lucide-react'
import { useCallback, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { AnomalyTable } from '@/components/network/anomaly-table'
import { LatencyChart } from '@/components/network/latency-chart'
import { TargetCard } from '@/components/network/target-card'
import { StatusBadge } from '@/components/server/status-badge'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogClose, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useServer } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import {
  useClearTracerouteHistory,
  useDeleteTraceroute,
  useNetworkAnomalies,
  useNetworkRecords,
  useNetworkServerSummary,
  useNetworkTargets,
  useSetServerTargets,
  useStartTraceroute,
  useTracerouteHistory,
  useTracerouteRecord
} from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import { useTracerouteStream } from '@/hooks/use-traceroute-stream'
import { CHART_COLORS } from '@/lib/chart-colors'
import {
  getNetworkProbeTypeLabel,
  getNetworkTargetDisplayLocation,
  getNetworkTargetDisplayName,
  getNetworkTargetDisplayProvider
} from '@/lib/network-i18n'
import type {
  NetworkProbeRecord,
  NetworkProbeTarget,
  NetworkTargetSummary,
  TracerouteHop,
  TracerouteRecordSummary
} from '@/lib/network-types'
import {
  formatLatency,
  formatPacketLoss,
  getLossTextClassName,
  getProviderLabel,
  isNewSchemaHop,
  latencyColorClass,
  type TraceProtocol
} from '@/lib/network-types'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/_authed/network/$serverId')({
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  }),
  component: NetworkDetailPage
})

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

const PROVIDER_KEYS = ['ct', 'cu', 'cm', 'international'] as const

const PROVIDER_TO_KEY: Record<string, string> = {
  Telecom: 'ct',
  Unicom: 'cu',
  Mobile: 'cm',
  International: 'international'
}

function groupTargetsByProvider(targets: NetworkTargetSummary[]) {
  const groups: Record<string, NetworkTargetSummary[]> = {}
  for (const target of targets) {
    const key = PROVIDER_TO_KEY[target.provider] || target.provider || 'unknown'
    if (!groups[key]) {
      groups[key] = []
    }
    groups[key].push(target)
  }
  return groups
}

function ProviderColumn({
  getTargetDisplayName,
  provider,
  targets,
  t
}: {
  getTargetDisplayName: (target: NetworkTargetSummary) => string
  provider: string
  targets: NetworkTargetSummary[]
  t: (key: string, options?: { defaultValue?: string }) => string
}) {
  const providerI18nKey = `provider_${provider}`
  const label = t(providerI18nKey, { defaultValue: getProviderLabel(provider) })

  const avgLatency = useMemo(() => {
    const valid = targets.filter((t) => t.avg_latency != null)
    if (valid.length === 0) {
      return null
    }
    return valid.reduce((sum, t) => sum + (t.avg_latency ?? 0), 0) / valid.length
  }, [targets])

  const avgPacketLoss = useMemo(() => {
    if (targets.length === 0) {
      return 0
    }
    return targets.reduce((sum, t) => sum + t.packet_loss, 0) / targets.length
  }, [targets])

  return (
    <Card>
      <CardHeader>
        <CardTitle>{label}</CardTitle>
        <div className="flex gap-3 text-muted-foreground text-xs">
          <span>
            {t('avg_latency')}: <span className="font-mono">{formatLatency(avgLatency)}</span>
          </span>
          <span>
            {t('packet_loss')}: <span className="font-mono">{formatPacketLoss(avgPacketLoss)}</span>
          </span>
        </div>
      </CardHeader>
      <CardContent>
        {targets.length === 0 ? (
          <p className="text-center text-muted-foreground text-sm">{t('no_data')}</p>
        ) : (
          <div className="space-y-2">
            {targets.map((target) => (
              <div
                className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
                key={target.target_id}
              >
                <span className="font-medium">{getTargetDisplayName(target)}</span>
                <div className="flex items-center gap-3 text-xs">
                  <span
                    className={cn(
                      'font-mono',
                      latencyColorClass(target.avg_latency, { failed: target.packet_loss >= 1 })
                    )}
                  >
                    {formatLatency(target.avg_latency)}
                  </span>
                  <span className="text-muted-foreground">{formatPacketLoss(target.packet_loss)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function deriveHopStats(hop: TracerouteHop, isNew: boolean) {
  const legacyRtts = [hop.rtt1, hop.rtt2, hop.rtt3].filter((v): v is number => v != null)
  if (isNew) {
    return {
      lossPct: hop.loss_pct ?? null,
      bestMs: hop.best_ms ?? null,
      avgMs: hop.avg_ms ?? null,
      worstMs: hop.worst_ms ?? null
    }
  }
  const lossPct = legacyRtts.length === 0 ? 100 : ((3 - legacyRtts.length) / 3) * 100
  const bestMs = legacyRtts.length > 0 ? Math.min(...legacyRtts) : null
  const avgMs = legacyRtts.length > 0 ? legacyRtts.reduce((a, b) => a + b, 0) / legacyRtts.length : null
  const worstMs = legacyRtts.length > 0 ? Math.max(...legacyRtts) : null
  return { lossPct, bestMs, avgMs, worstMs }
}

function formatMs(value: number | null | undefined, digits = 1): string {
  return value == null ? '—' : value.toFixed(digits)
}

function HopIpCell({ primaryIp, extraIps }: { primaryIp: string | null; extraIps: string[] }) {
  return (
    <TableCell className="font-mono">
      {primaryIp ?? '* * *'}
      {extraIps.length > 0 && (
        <Tooltip>
          <TooltipTrigger>
            <Badge className="ml-1" variant="secondary">
              +{extraIps.length}
            </Badge>
          </TooltipTrigger>
          <TooltipContent>{extraIps.join(', ')}</TooltipContent>
        </Tooltip>
      )}
    </TableCell>
  )
}

function HopRow({ hop }: { hop: TracerouteHop }) {
  const isNew = isNewSchemaHop(hop)
  const primaryIp = isNew ? (hop.ips?.[0] ?? null) : (hop.ip ?? null)
  const extraIps = isNew && (hop.ips?.length ?? 0) > 1 ? (hop.ips?.slice(1) ?? []) : []
  const dimmed = isNew ? (hop.total_recv ?? 0) === 0 : hop.rtt1 == null && hop.rtt2 == null && hop.rtt3 == null

  const { lossPct, bestMs, avgMs, worstMs } = deriveHopStats(hop, isNew)
  const lossRatio = lossPct == null ? null : lossPct / 100

  return (
    <TableRow className={cn(dimmed && 'opacity-50')}>
      <TableCell className="font-mono">{hop.hop}</TableCell>
      <HopIpCell extraIps={extraIps} primaryIp={primaryIp} />
      <TableCell className="max-w-[200px] truncate text-muted-foreground">{hop.hostname ?? '—'}</TableCell>
      <TableCell className="text-muted-foreground">{hop.asn ?? '—'}</TableCell>
      <TableCell className={cn('text-right font-mono', getLossTextClassName(lossRatio))}>
        {lossPct == null ? '—' : `${lossPct.toFixed(0)}%`}
      </TableCell>
      <TableCell className="text-right font-mono">{formatMs(bestMs)}</TableCell>
      <TableCell className={cn('text-right font-mono', latencyColorClass(avgMs, { failed: dimmed }))}>
        {formatMs(avgMs)}
      </TableCell>
      <TableCell className="text-right font-mono">{formatMs(worstMs)}</TableCell>
      <TableCell className="text-right font-mono">{formatMs(hop.jitter_ms, 2)}</TableCell>
      <TableCell className="text-right font-mono">{formatMs(hop.stddev_ms, 2)}</TableCell>
    </TableRow>
  )
}

function formatRelativeTime(unixMs: number): string {
  const diff = Date.now() - unixMs
  if (diff < 60_000) {
    return 'just now'
  }
  if (diff < 3_600_000) {
    return `${Math.floor(diff / 60_000)}m ago`
  }
  if (diff < 86_400_000) {
    return `${Math.floor(diff / 3_600_000)}h ago`
  }
  return `${Math.floor(diff / 86_400_000)}d ago`
}

interface TracerouteRunFormProps {
  isPending: boolean
  isRunning: boolean
  onKeyDown: (e: React.KeyboardEvent) => void
  onRun: () => void
  protocol: TraceProtocol
  setProtocol: (p: TraceProtocol) => void
  setTarget: (v: string) => void
  t: (key: string, opts?: Record<string, unknown>) => string
  target: string
}

function TracerouteRunForm({
  isPending,
  isRunning,
  onKeyDown,
  onRun,
  protocol,
  setProtocol,
  setTarget,
  t,
  target
}: TracerouteRunFormProps) {
  return (
    <div className="flex gap-2">
      <Input
        disabled={isRunning || isPending}
        onChange={(e) => setTarget(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t('traceroute_target')}
        value={target}
      />
      <Select onValueChange={(v) => setProtocol(v as TraceProtocol)} value={protocol}>
        <SelectTrigger className="w-24">
          <SelectValue>{(value: string) => value?.toUpperCase()}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="icmp">ICMP</SelectItem>
          <SelectItem value="udp">UDP</SelectItem>
          <SelectItem value="tcp">TCP</SelectItem>
        </SelectContent>
      </Select>
      <Button disabled={!target.trim() || isRunning || isPending} onClick={onRun} size="sm">
        {isRunning || isPending ? (
          <Loader2 aria-hidden="true" className="mr-1 size-4 animate-spin" />
        ) : (
          <Play aria-hidden="true" className="mr-1 size-4" />
        )}
        {isRunning ? t('traceroute_running') : t('run_traceroute')}
      </Button>
    </div>
  )
}

interface TracerouteHistoryListProps {
  clearMutation: { mutate: () => void }
  deleteMutation: { mutate: (id: string) => void }
  history: TracerouteRecordSummary[] | undefined
  isAdmin: boolean
  onSelect: (record: TracerouteRecordSummary) => void
  selectedRecordId: string | null
  t: (key: string, opts?: Record<string, unknown>) => string
}

function HistoryRow({
  isAdmin,
  isSelected,
  onDelete,
  onSelect,
  record,
  t
}: {
  isAdmin: boolean
  isSelected: boolean
  onDelete: (id: string) => void
  onSelect: (record: TracerouteRecordSummary) => void
  record: TracerouteRecordSummary
  t: (key: string, opts?: Record<string, unknown>) => string
}) {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: list items are supplemented by explicit icon buttons
    // biome-ignore lint/a11y/noNoninteractiveElementInteractions: history rows act as selection targets
    <li
      className={cn(
        'flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/40',
        isSelected && 'bg-muted'
      )}
      onClick={() => onSelect(record)}
    >
      <span className="flex-1 truncate font-mono">{record.target}</span>
      <Badge variant={record.protocol === 'legacy' ? 'outline' : 'secondary'}>
        {record.protocol === 'legacy' ? (
          <Tooltip>
            <TooltipTrigger>
              <span>legacy</span>
            </TooltipTrigger>
            <TooltipContent>{t('legacy_record_tooltip')}</TooltipContent>
          </Tooltip>
        ) : (
          record.protocol.toUpperCase()
        )}
      </Badge>
      <span className="text-muted-foreground text-xs">{record.hop_count} hops</span>
      <span className="text-muted-foreground text-xs">{formatRelativeTime(record.started_at)}</span>
      {record.has_error ? <X className="size-3 text-destructive" /> : <Check className="size-3 text-emerald-500" />}
      {isAdmin && (
        <Button
          aria-label={t('delete')}
          onClick={(e) => {
            e.stopPropagation()
            onDelete(record.request_id)
          }}
          size="icon"
          variant="ghost"
        >
          <Trash2 className="size-4" />
        </Button>
      )}
    </li>
  )
}

function TracerouteHistoryList({
  clearMutation,
  deleteMutation,
  history,
  isAdmin,
  onSelect,
  selectedRecordId,
  t
}: TracerouteHistoryListProps) {
  const count = history?.length ?? 0
  const handleClear = useCallback(() => {
    // biome-ignore lint/suspicious/noAlert: plan spec requires window.confirm for clear-all
    if (window.confirm(t('clear_all_confirm', { count }))) {
      clearMutation.mutate()
    }
  }, [clearMutation, count, t])

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="font-medium text-sm">
          {t('history')} ({count})
        </h3>
        {isAdmin && count > 0 && (
          <Button onClick={handleClear} size="sm" variant="ghost">
            {t('clear_all')}
          </Button>
        )}
      </div>
      {count === 0 && <p className="text-muted-foreground text-sm">{t('history_empty')}</p>}
      <div className="min-h-0 flex-1 overflow-auto">
        <ul className="space-y-1">
          {history?.map((r) => (
            <HistoryRow
              isAdmin={isAdmin}
              isSelected={selectedRecordId === r.request_id}
              key={r.request_id}
              onDelete={(id) => deleteMutation.mutate(id)}
              onSelect={onSelect}
              record={r}
              t={t}
            />
          ))}
        </ul>
      </div>
    </div>
  )
}

interface TracerouteRecentChipsProps {
  clearMutation: { mutate: () => void }
  deleteMutation: { mutate: (id: string) => void }
  history: TracerouteRecordSummary[] | undefined
  isAdmin: boolean
  onSelect: (record: TracerouteRecordSummary) => void
  selectedRecordId: string | null
  t: (key: string, opts?: Record<string, unknown>) => string
}

const RECENT_CHIPS_LIMIT = 6

function TracerouteRecentChips({
  clearMutation,
  deleteMutation,
  history,
  isAdmin,
  onSelect,
  selectedRecordId,
  t
}: TracerouteRecentChipsProps) {
  const recentChips = useMemo(() => {
    if (!history?.length) {
      return []
    }
    const seen = new Set<string>()
    const out: TracerouteRecordSummary[] = []
    for (const record of history) {
      const key = `${record.target}|${record.protocol}`
      if (seen.has(key)) {
        continue
      }
      seen.add(key)
      out.push(record)
      if (out.length >= RECENT_CHIPS_LIMIT) {
        break
      }
    }
    return out
  }, [history])

  const total = history?.length ?? 0

  if (total === 0) {
    return null
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="text-muted-foreground text-xs">{t('traceroute_recent')}:</span>
      {recentChips.map((record) => {
        const isSelected = selectedRecordId === record.request_id
        return (
          <Button
            className="h-7 gap-1.5 px-2 font-mono text-xs"
            key={record.request_id}
            onClick={() => onSelect(record)}
            size="sm"
            variant={isSelected ? 'secondary' : 'outline'}
          >
            <span className="truncate">{record.target}</span>
            <span className="text-[10px] text-muted-foreground uppercase">
              {record.protocol === 'legacy' ? '·' : record.protocol}
            </span>
            {record.has_error ? (
              <X aria-hidden="true" className="size-3 text-destructive" />
            ) : (
              <Check aria-hidden="true" className="size-3 text-emerald-500" />
            )}
          </Button>
        )
      })}
      <Popover>
        <PopoverTrigger render={<Button className="h-7 px-2 text-xs" size="sm" variant="ghost" />}>
          {t('traceroute_view_all_history', { count: total })}
        </PopoverTrigger>
        <PopoverContent align="end" className="flex h-96 w-80 flex-col gap-2 p-3">
          <TracerouteHistoryList
            clearMutation={clearMutation}
            deleteMutation={deleteMutation}
            history={history}
            isAdmin={isAdmin}
            onSelect={onSelect}
            selectedRecordId={selectedRecordId}
            t={t}
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}

interface TracerouteContentProps {
  protocol: TraceProtocol
  selectedRecordId: string | null
  serverId: string
  setProtocol: (p: TraceProtocol) => void
  setSelectedRecordId: (id: string | null) => void
  setTarget: (v: string) => void
  setTraceRequestId: (id: string | null) => void
  target: string
  traceRequestId: string | null
}

function TracerouteContent({
  protocol,
  selectedRecordId,
  serverId,
  setProtocol,
  setSelectedRecordId,
  setTraceRequestId,
  target,
  traceRequestId,
  setTarget
}: TracerouteContentProps) {
  const { t } = useTranslation('network')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const startTraceroute = useStartTraceroute(serverId)
  const stream = useTracerouteStream(serverId, traceRequestId)
  const { data: polled } = useTracerouteRecord(
    serverId,
    selectedRecordId ?? (stream?.completed ? null : traceRequestId)
  )
  const result = selectedRecordId ? (polled ?? null) : (stream ?? polled ?? null)

  const { data: history } = useTracerouteHistory(serverId)
  const deleteMutation = useDeleteTraceroute(serverId)
  const clearMutation = useClearTracerouteHistory(serverId)

  const isRunning = !!traceRequestId && !result?.completed && !result?.error

  const handleRun = useCallback(() => {
    const trimmed = target.trim()
    if (!trimmed) {
      return
    }

    setTraceRequestId(null)
    setSelectedRecordId(null)
    startTraceroute.mutate(
      { target: trimmed, protocol },
      {
        onSuccess: (data) => {
          setTraceRequestId(data.request_id)
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('traceroute_error'))
        }
      }
    )
  }, [target, protocol, startTraceroute, t, setTraceRequestId, setSelectedRecordId])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        handleRun()
      }
    },
    [handleRun]
  )

  const loadRecord = useCallback(
    (record: TracerouteRecordSummary) => {
      setSelectedRecordId(record.request_id)
      setTarget(record.target)
      if (record.protocol !== 'legacy') {
        setProtocol(record.protocol as TraceProtocol)
      }
    },
    [setSelectedRecordId, setTarget, setProtocol]
  )

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {isAdmin && (
        <TracerouteRunForm
          isPending={startTraceroute.isPending}
          isRunning={isRunning}
          onKeyDown={handleKeyDown}
          onRun={handleRun}
          protocol={protocol}
          setProtocol={setProtocol}
          setTarget={setTarget}
          t={t}
          target={target}
        />
      )}
      {!isAdmin && <p className="text-muted-foreground text-xs">{t('traceroute_readonly_note')}</p>}

      <TracerouteRecentChips
        clearMutation={clearMutation}
        deleteMutation={deleteMutation}
        history={history}
        isAdmin={isAdmin}
        onSelect={loadRecord}
        selectedRecordId={selectedRecordId}
        t={t}
      />

      {stream && !stream.completed && (
        <span className="text-muted-foreground text-xs tabular-nums">
          {t('round_progress', { current: stream.round, total: stream.total_rounds })}
        </span>
      )}

      {result?.error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-destructive text-sm">
          {result.error}
        </div>
      )}

      {result && result.hops.length > 0 && (
        <div className="min-h-0 flex-1 overflow-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-12">{t('hop')}</TableHead>
                <TableHead>{t('ip_address')}</TableHead>
                <TableHead>{t('hostname')}</TableHead>
                <TableHead>{t('asn')}</TableHead>
                <TableHead className="text-right">{t('loss_pct')}</TableHead>
                <TableHead className="text-right">{t('best')}</TableHead>
                <TableHead className="text-right">{t('avg')}</TableHead>
                <TableHead className="text-right">{t('worst')}</TableHead>
                <TableHead className="text-right">{t('jitter')}</TableHead>
                <TableHead className="text-right">{t('stddev')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {result.hops.map((hop) => (
                <HopRow hop={hop} key={hop.hop} />
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {!(result || isRunning) && (
        <div className="flex min-h-0 flex-1 items-center justify-center rounded-md border border-dashed text-muted-foreground text-sm">
          {t('traceroute_select_or_run')}
        </div>
      )}

      {isRunning && !result && (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 rounded-md border border-dashed text-muted-foreground text-sm">
          <Loader2 aria-hidden="true" className="size-4 animate-spin" />
          {t('traceroute_running')}
        </div>
      )}
    </div>
  )
}

export function NetworkDetailPage() {
  const { i18n, t } = useTranslation('network')
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
  const [showTracerouteDialog, setShowTracerouteDialog] = useState(false)
  const [selectedTargetIds, setSelectedTargetIds] = useState<Set<string>>(new Set())
  const selectedRef = useRef(selectedTargetIds)
  selectedRef.current = selectedTargetIds

  // Traceroute lifted state
  const [traceTarget, setTraceTarget] = useState('')
  const [traceProtocol, setTraceProtocol] = useState<TraceProtocol>('icmp')
  const [traceRequestId, setTraceRequestId] = useState<string | null>(null)
  const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null)

  const isRealtime = timeRange === 'realtime'
  const hours = isRealtime ? 1 : timeRange
  // Anomalies are historical events; in realtime mode use 24h so the count matches
  // the badge shown on the network overview card, which is also 24h-based.
  const anomalyHours = isRealtime ? 24 : timeRange

  const { data: server, isLoading: serverLoading } = useServer(serverId)
  const { data: summary, isLoading: summaryLoading } = useNetworkServerSummary(serverId)
  const { data: historicalRecords } = useNetworkRecords(serverId, hours, { enabled: !isRealtime })
  // Fetch last 10 min of data as seed for realtime chart (immediate data on first load)
  const { data: seedRecords } = useNetworkRecords(serverId, 1, { enabled: isRealtime })
  const { data: anomalies = [] } = useNetworkAnomalies(serverId, anomalyHours)
  const { data: realtimeData } = useNetworkRealtime(serverId)
  const { data: allTargets = [] } = useNetworkTargets()
  const setServerTargets = useSetServerTargets(serverId)
  const language = i18n.resolvedLanguage ?? i18n.language
  const getProbeTypeLabel = useCallback((probeType: string) => getNetworkProbeTypeLabel(t, probeType), [t])
  const targetMetadataById = useMemo(() => new Map(allTargets.map((target) => [target.id, target])), [allTargets])
  const getLocalizedTargetDisplayName = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayName(t, language, target),
    [language, t]
  )
  const getLocalizedTargetDisplayProvider = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayProvider(t, language, target),
    [language, t]
  )
  const getLocalizedTargetDisplayLocation = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayLocation(t, language, target),
    [language, t]
  )
  const getSummaryTargetDisplayName = useCallback(
    (target: NetworkTargetSummary) => {
      const targetMetadata = targetMetadataById.get(target.target_id)
      if (!targetMetadata) {
        return target.target_name
      }

      return getLocalizedTargetDisplayName(targetMetadata)
    },
    [getLocalizedTargetDisplayName, targetMetadataById]
  )

  const targets = useMemo(() => summary?.targets ?? [], [summary])

  // Group targets by provider for the "By Provider" tab
  const providerGroups = useMemo(() => groupTargetsByProvider(targets), [targets])

  // Ordered provider keys: known providers first, then any remaining
  const orderedProviderKeys = useMemo(() => {
    const known = PROVIDER_KEYS.filter((k) => providerGroups[k]?.length)
    const remaining = Object.keys(providerGroups).filter(
      (k) => !PROVIDER_KEYS.includes(k as (typeof PROVIDER_KEYS)[number])
    )
    return [...known, ...remaining]
  }, [providerGroups])

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
    toast.success(t('export_csv_success', { defaultValue: 'CSV exported' }))
  }, [records, serverId, timeRange, t])

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
        toast.success(t('server_targets_updated', { defaultValue: 'Server targets updated' }))
        setShowManageDialog(false)
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
    <div className="pb-6">
      {/* Header */}
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          search={{ q: '' }}
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
            <Button onClick={() => setShowTracerouteDialog(true)} size="sm" variant="outline">
              <RouteIcon aria-hidden="true" className="mr-1 size-4" />
              {t('traceroute')}
            </Button>
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
        {server.ipv4 && (
          <span>
            {t('server_ipv4', { defaultValue: 'IPv4' })}: {server.ipv4}
          </span>
        )}
        {server.ipv6 && (
          <span>
            {t('server_ipv6', { defaultValue: 'IPv6' })}: {server.ipv6}
          </span>
        )}
        {server.region && (
          <span>
            {t('server_region', { defaultValue: 'Region' })}: {server.region}
          </span>
        )}
        {server.os && (
          <span>
            {t('server_os', { defaultValue: 'OS' })}: {server.os}
          </span>
        )}
        {summary.last_probe_at && (
          <span>
            {t('last_probe')}:{' '}
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

      {/* Target cards with tabs: All Targets / By Provider */}
      {targets.length > 0 && (
        <Tabs className="mb-4" defaultValue="all">
          <TabsList>
            <TabsTrigger value="all">{t('all_targets')}</TabsTrigger>
            <TabsTrigger value="provider">{t('by_provider')}</TabsTrigger>
          </TabsList>

          <TabsContent value="all">
            <div className="flex flex-wrap gap-2 pt-2">
              {targets.map((target) => (
                <TargetCard
                  color={targetColorMap[target.target_id] ?? CHART_COLORS[0]}
                  displayName={getSummaryTargetDisplayName(target)}
                  key={target.target_id}
                  onToggle={() => toggleTarget(target.target_id)}
                  target={target}
                  visible={effectiveVisible.has(target.target_id)}
                />
              ))}
            </div>
          </TabsContent>

          <TabsContent value="provider">
            <div className="grid gap-4 pt-2 md:grid-cols-2 lg:grid-cols-3">
              {orderedProviderKeys.map((provider) => (
                <ProviderColumn
                  getTargetDisplayName={getSummaryTargetDisplayName}
                  key={provider}
                  provider={provider}
                  t={t}
                  targets={providerGroups[provider]}
                />
              ))}
            </div>
          </TabsContent>
        </Tabs>
      )}

      {/* Latency chart */}
      <div className="mb-4">
        <LatencyChart hours={hours} isRealtime={isRealtime} records={records} targets={chartTargets} />
      </div>

      {/* Bottom stats */}
      <div className="mb-6 grid gap-4 sm:grid-cols-3">
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
      <AnomalyTable anomalies={anomalies} windowHours={anomalyHours} />

      {/* Traceroute Dialog */}
      <Dialog onOpenChange={setShowTracerouteDialog} open={showTracerouteDialog}>
        <DialogContent className="h-[85vh] sm:max-w-5xl">
          <DialogHeader>
            <DialogTitle>{t('traceroute')}</DialogTitle>
          </DialogHeader>
          <TracerouteContent
            protocol={traceProtocol}
            selectedRecordId={selectedRecordId}
            serverId={serverId}
            setProtocol={setTraceProtocol}
            setSelectedRecordId={setSelectedRecordId}
            setTarget={setTraceTarget}
            setTraceRequestId={setTraceRequestId}
            target={traceTarget}
            traceRequestId={traceRequestId}
          />
        </DialogContent>
      </Dialog>

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
                  <span className="flex-1 font-medium">{getLocalizedTargetDisplayName(target)}</span>
                  {target.provider && (
                    <span className="text-muted-foreground text-xs">{getLocalizedTargetDisplayProvider(target)}</span>
                  )}
                  {target.location && (
                    <span className="text-muted-foreground text-xs">{getLocalizedTargetDisplayLocation(target)}</span>
                  )}
                  <span className="rounded-full bg-muted px-2 py-0.5 text-xs">
                    {getProbeTypeLabel(target.probe_type)}
                  </span>
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
