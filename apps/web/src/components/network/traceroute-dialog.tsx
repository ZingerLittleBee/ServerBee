import { Check, Loader2, Play, Trash2, X } from 'lucide-react'
import type * as React from 'react'
import { useCallback, useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useAuth } from '@/hooks/use-auth'
import {
  useClearTracerouteHistory,
  useDeleteTraceroute,
  useStartTraceroute,
  useTracerouteHistory,
  useTracerouteRecord
} from '@/hooks/use-network-api'
import { useTracerouteStream } from '@/hooks/use-traceroute-stream'
import type { TracerouteHop, TracerouteRecordSummary } from '@/lib/network-types'
import { getHopLossTextClass, isNewSchemaHop, latencyColorClass, type TraceProtocol } from '@/lib/network-types'
import { cn } from '@/lib/utils'
import { useTracerouteRunState, useTracerouteStore } from '@/stores/traceroute-store'

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
      <TableCell className={cn('text-right font-mono', getHopLossTextClass(lossRatio))}>
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

function formatRelativeTime(unixMs: number, t: (key: string, opts?: Record<string, unknown>) => string): string {
  const diff = Date.now() - unixMs
  if (diff < 60_000) {
    return t('recent_just_now')
  }
  if (diff < 3_600_000) {
    return t('recent_minutes_ago', { count: Math.floor(diff / 60_000) })
  }
  if (diff < 86_400_000) {
    return t('recent_hours_ago', { count: Math.floor(diff / 3_600_000) })
  }
  return t('recent_days_ago', { count: Math.floor(diff / 86_400_000) })
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
    <li className="flex items-center gap-1">
      <button
        className={cn(
          'flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted/40',
          isSelected && 'bg-muted'
        )}
        onClick={() => onSelect(record)}
        type="button"
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
        <span className="text-muted-foreground text-xs">{t('recent_hops', { count: record.hop_count })}</span>
        <span className="text-muted-foreground text-xs">{formatRelativeTime(record.started_at, t)}</span>
        {record.has_error ? <X className="size-3 text-destructive" /> : <Check className="size-3 text-emerald-500" />}
      </button>
      {isAdmin && (
        <Button aria-label={t('delete')} onClick={() => onDelete(record.request_id)} size="icon" variant="ghost">
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
      <ScrollArea className="min-h-0 flex-1">
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
      </ScrollArea>
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
    <div className="flex items-start gap-2">
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
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
      </div>
      <Popover>
        <PopoverTrigger render={<Button className="h-7 shrink-0 px-2 text-xs" size="sm" variant="ghost" />}>
          {t('traceroute_view_all_history', { count: total })}
        </PopoverTrigger>
        <PopoverContent align="end" className="flex h-96 w-80 flex-col gap-2 overflow-hidden p-3">
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

// Run state (request id, target, protocol, selected record, latest stream
// frame) lives in the traceroute store so an in-flight run survives the
// network tab unmounting; see `stores/traceroute-store.ts`.
function TracerouteContent({ serverId }: { serverId: string }) {
  const { t } = useTranslation('network')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { selectedRecordId, streamSnapshot, traceProtocol, traceRequestId, traceTarget } =
    useTracerouteRunState(serverId)
  const patchServer = useTracerouteStore((state) => state.patchServer)

  const startTraceroute = useStartTraceroute(serverId)
  const liveStream = useTracerouteStream(serverId, traceRequestId)

  // Persist each full-state frame so a remount can restore the progress view
  // before the next frame arrives.
  useEffect(() => {
    if (liveStream) {
      patchServer(serverId, { streamSnapshot: liveStream })
    }
  }, [liveStream, patchServer, serverId])

  const stream = liveStream ?? (streamSnapshot?.request_id === traceRequestId ? streamSnapshot : null)
  const { data: polled, isFetching: isFetchingRecord } = useTracerouteRecord(
    serverId,
    selectedRecordId ?? (stream?.completed ? null : traceRequestId)
  )
  // Prefer the completed DB record over a possibly stale resumed snapshot
  // (the run may have finished while the tab was unmounted).
  const streamOrRecord = polled?.completed ? polled : (stream ?? polled ?? null)
  const result = selectedRecordId ? (polled ?? null) : streamOrRecord

  const { data: history } = useTracerouteHistory(serverId)
  const deleteMutation = useDeleteTraceroute(serverId)
  const clearMutation = useClearTracerouteHistory(serverId)

  const isRunning = !!traceRequestId && !result?.completed && !result?.error
  const isLoadingRecord = !!selectedRecordId && !polled && isFetchingRecord

  const handleRun = useCallback(() => {
    const trimmed = traceTarget.trim()
    if (!trimmed) {
      return
    }

    patchServer(serverId, { traceRequestId: null, selectedRecordId: null, streamSnapshot: null })
    startTraceroute.mutate(
      { target: trimmed, protocol: traceProtocol },
      {
        onSuccess: (data) => {
          patchServer(serverId, { traceRequestId: data.request_id })
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('traceroute_error'))
        }
      }
    )
  }, [traceTarget, traceProtocol, startTraceroute, t, patchServer, serverId])

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
      patchServer(serverId, {
        traceRequestId: null,
        selectedRecordId: record.request_id,
        traceTarget: record.target,
        ...(record.protocol !== 'legacy' ? { traceProtocol: record.protocol as TraceProtocol } : {})
      })
    },
    [patchServer, serverId]
  )

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {isAdmin && (
        <TracerouteRunForm
          isPending={startTraceroute.isPending}
          isRunning={isRunning}
          onKeyDown={handleKeyDown}
          onRun={handleRun}
          protocol={traceProtocol}
          setProtocol={(value) => patchServer(serverId, { traceProtocol: value })}
          setTarget={(value) => patchServer(serverId, { traceTarget: value })}
          t={t}
          target={traceTarget}
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
        <ScrollArea className="min-h-0 flex-1 rounded-md border">
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
        </ScrollArea>
      )}

      {isLoadingRecord && (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 rounded-md border border-dashed text-muted-foreground text-sm">
          <Loader2 aria-hidden="true" className="size-4 animate-spin" />
          {t('traceroute_loading_record')}
        </div>
      )}

      {isRunning && !(result || isLoadingRecord) && (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 rounded-md border border-dashed text-muted-foreground text-sm">
          <Loader2 aria-hidden="true" className="size-4 animate-spin" />
          {t('traceroute_running')}
        </div>
      )}

      {!(result || isRunning || isLoadingRecord) && (
        <div className="flex min-h-0 flex-1 items-center justify-center rounded-md border border-dashed text-muted-foreground text-sm">
          {t('traceroute_select_or_run')}
        </div>
      )}
    </div>
  )
}

export function TracerouteDialog({
  onOpenChange,
  open,
  serverId
}: {
  onOpenChange: (open: boolean) => void
  open: boolean
  serverId: string
}) {
  const { t } = useTranslation('network')
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="h-[92vh] sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>{t('traceroute')}</DialogTitle>
        </DialogHeader>
        <TracerouteContent serverId={serverId} />
      </DialogContent>
    </Dialog>
  )
}
