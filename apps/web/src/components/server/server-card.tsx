import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { MoreHorizontal, RefreshCw, Trash2 } from 'lucide-react'
import { memo, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { CompactMetric } from '@/components/server/compact-metric'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { RingChart } from '@/components/ui/ring-chart'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import { api } from '@/lib/api-client'
import type { OutstandingEnrollmentSummary, ServerCostOverview } from '@/lib/api-schema'
import { isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass, type NetworkServerSummary } from '@/lib/network-types'
import { computeTrafficQuota } from '@/lib/traffic'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CostFootnote } from './cost-footnote'
import { NetworkSquareGrid } from './network-square-grid'
import { NetworkTargetBreakdown } from './network-target-breakdown'
import { RegenerateCodeDialog } from './regenerate-code-dialog'
import { buildServerCardNetworkState, type ServerCardTooltipTarget } from './server-card-network-data'
import { StatusBadge } from './status-badge'
import { deriveServerStatus } from './status-dot'
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

interface RingMetricProps {
  color: string
  label: string
  subText: React.ReactNode
  value: number
}

function RingMetric({ color, label, subText, value }: RingMetricProps) {
  return (
    <div className="flex items-center gap-2">
      <RingChart color={color} compact label={label} value={value} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[11px] text-muted-foreground">{label}</span>
        <span className="truncate text-[10px] text-muted-foreground tabular-nums">{subText}</span>
      </div>
    </div>
  )
}

function NetworkMetricValue({
  children,
  targets
}: {
  children: React.ReactElement
  targets: readonly ServerCardTooltipTarget[]
}) {
  if (targets.length === 0) {
    return children
  }
  return (
    <Tooltip>
      <TooltipTrigger render={children} />
      <TooltipContent className="grid min-w-48 gap-1.5" sideOffset={4}>
        <NetworkTargetBreakdown targets={targets} />
      </TooltipContent>
    </Tooltip>
  )
}

function formatCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(remainingMs / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`
}

interface PendingSummaryProps {
  enrollment: OutstandingEnrollmentSummary | null | undefined
}

/**
 * Renders the one-line enrollment-status hint shown below the "Waiting for agent…"
 * headline on a pending server card. Ticks every second while the code is still
 * within its TTL so operators can see the countdown drain in real time.
 */
function PendingEnrollmentSummary({ enrollment }: PendingSummaryProps) {
  const { t } = useTranslation(['servers'])
  const expiresAt = enrollment ? new Date(enrollment.expires_at).getTime() : null
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (expiresAt == null) {
      return
    }
    // Only tick while there is a meaningful countdown to display; once expired we
    // stop tearing through render cycles for no reason.
    if (expiresAt <= Date.now()) {
      return
    }
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [expiresAt])

  if (!enrollment) {
    return <p className="text-muted-foreground text-xs">{t('card_pending.no_code')}</p>
  }

  if (expiresAt != null && expiresAt > now) {
    return (
      <p className="text-amber-700 text-xs tabular-nums dark:text-amber-400">
        {t('card_pending.code_expires_in', {
          prefix: enrollment.code_prefix,
          countdown: formatCountdown(expiresAt - now)
        })}
      </p>
    )
  }

  return (
    <p className="text-muted-foreground text-xs">
      {t('card_pending.code_expired', { prefix: enrollment.code_prefix })}
    </p>
  )
}

interface PendingActionMenuProps {
  serverId: string
  serverName: string
}

function PendingActionMenu({ serverId, serverName }: PendingActionMenuProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [regenerateOpen, setRegenerateOpen] = useState(false)
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false)

  const deleteMutation = useMutation({
    mutationFn: () => api.delete<void>(`/api/servers/${serverId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers'] })
      toast.success(t('servers:card_pending.deleted'))
      setConfirmDeleteOpen(false)
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : t('servers:card_pending.delete_failed'))
    }
  })

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              aria-label={`${t('servers:card_pending.regenerate_code')} / ${t('servers:card_pending.delete_server')}`}
              onClick={(e) => e.stopPropagation()}
              size="icon-sm"
              variant="ghost"
            />
          }
        >
          <MoreHorizontal aria-hidden="true" className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setRegenerateOpen(true)
            }}
          >
            <RefreshCw aria-hidden="true" className="size-3.5" />
            {t('servers:card_pending.regenerate_code')}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setConfirmDeleteOpen(true)
            }}
          >
            <Trash2 aria-hidden="true" className="size-3.5" />
            {t('servers:card_pending.delete_server')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <RegenerateCodeDialog onOpenChange={setRegenerateOpen} open={regenerateOpen} serverId={serverId} />

      <AlertDialog onOpenChange={setConfirmDeleteOpen} open={confirmDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('servers:card_pending.delete_confirm_title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('servers:card_pending.delete_confirm_description', { name: serverName })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>{t('common:cancel')}</AlertDialogCancel>
            <AlertDialogAction
              disabled={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate()}
              variant="destructive"
            >
              {t('common:delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
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
  const flag = countryCodeToFlag(server.country_code)
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
    <div className="relative flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm">
      {!(server.online || isPending) && (
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
          <h3 className="truncate font-semibold text-[13px]">{server.name}</h3>
        </Link>
        <div className="flex items-center gap-1.5">
          <UpgradeJobBadge job={upgradeJob} />
          <StatusBadge status={status} />
          {isPending && <PendingActionMenu serverId={server.id} serverName={server.name} />}
        </div>
      </div>

      {isPending ? (
        <div className="flex flex-col gap-1 rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-3">
          <p className="font-medium text-amber-700 text-sm dark:text-amber-400">{t('card_pending.waiting')}</p>
          <PendingEnrollmentSummary enrollment={server.outstanding_enrollment} />
        </div>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-x-3 gap-y-2">
            <RingMetric
              color={getRingColor(server.cpu, 'var(--color-chart-1)')}
              label={t('col_cpu')}
              subText={
                <>
                  {t('card_load')} <span className="font-medium text-foreground">{formatLoad(server.load1)}</span>
                </>
              }
              value={server.cpu}
            />
            <RingMetric
              color={getRingColor(memoryPct, 'var(--color-chart-2)')}
              label={t('col_memory')}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(server.mem_used)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(server.mem_total)}
                </>
              }
              value={memoryPct}
            />
            <RingMetric
              color={getRingColor(diskPct, 'var(--color-chart-3)')}
              label={t('col_disk')}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(server.disk_used)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(server.disk_total)}
                </>
              }
              value={diskPct}
            />
            <RingMetric
              color={getRingColor(trafficRingPct, 'var(--color-chart-4)')}
              label={t('card_traffic_quota')}
              subText={
                <>
                  <span className="font-medium text-foreground">{formatBytes(trafficUsed)}</span>
                  <span className="mx-0.5">/</span>
                  {formatBytes(trafficLimit)}
                </>
              }
              value={trafficRingPct}
            />
          </div>

          <div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
            <CompactMetric label={t('card_net_in_speed')} value={renderSpeedValue(server.net_in_speed)} />
            <CompactMetric label={t('card_net_out_speed')} value={renderSpeedValue(server.net_out_speed)} />
            <CompactMetric
              label={
                <span aria-label={t('card_disk_read')} className="inline-flex items-center gap-1" role="img">
                  <span
                    aria-hidden="true"
                    className="inline-flex size-3.5 flex-none items-center justify-center rounded-full bg-muted font-semibold text-[8px] text-foreground leading-none"
                  >
                    R
                  </span>
                  {t('card_disk_read')}
                </span>
              }
              value={renderSpeedValue(server.disk_read_bytes_per_sec)}
            />
            <CompactMetric
              label={
                <span aria-label={t('card_disk_write')} className="inline-flex items-center gap-1" role="img">
                  <span
                    aria-hidden="true"
                    className="inline-flex size-3.5 flex-none items-center justify-center rounded-full bg-muted font-semibold text-[8px] text-foreground leading-none"
                  >
                    W
                  </span>
                  {t('card_disk_write')}
                </span>
              }
              value={renderSpeedValue(server.disk_write_bytes_per_sec)}
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
