import { Link } from '@tanstack/react-router'
import { ArrowDown, ArrowUp, Clock, Cpu, HardDrive, MemoryStick, Network } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { TagChipRow } from '@/components/server/tag-chip'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import { computeTrafficQuota } from '@/lib/traffic'
import { cn, countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'

export function getBarColor(pct: number): string {
  if (pct > 90) {
    return 'bg-red-500'
  }
  if (pct > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

export function getBarTextColor(pct: number): string {
  if (pct > 90) {
    return 'text-red-600 dark:text-red-400'
  }
  if (pct > 70) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-foreground'
}

interface MetricBarRowProps {
  ariaLabel?: string
  icon: ReactNode
  pct: number
  valueClassName?: string
}

export function MetricBarRow({ icon, pct, ariaLabel, valueClassName }: MetricBarRowProps) {
  const clamped = Math.min(100, Math.max(0, pct))
  const colorBg = getBarColor(clamped)
  const colorText = getBarTextColor(clamped)
  // Only apply role="img" when an ariaLabel is supplied; otherwise the role would be unnamed (a11y anti-pattern).
  const imgProps = ariaLabel ? { role: 'img' as const, 'aria-label': ariaLabel } : {}
  return (
    <div className="flex items-center gap-1.5" {...imgProps}>
      {icon !== null && <span className="inline-flex size-3.5 flex-none text-muted-foreground">{icon}</span>}
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div
          className={cn('h-full rounded-full', colorBg)}
          data-slot="metric-bar-fill"
          style={{ width: `${clamped}%` }}
        />
      </div>
      <span className={cn('w-10 text-right font-mono font-semibold text-xs tabular-nums', colorText, valueClassName)}>
        {Math.round(clamped)}%
      </span>
    </div>
  )
}

// Back-compat: MiniBar keeps its existing public signature but now delegates to MetricBarRow.
export function MiniBar({ pct, sub }: { pct: number; sub?: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <MetricBarRow icon={null} pct={pct} />
      {sub !== undefined && <div className="text-muted-foreground text-xs">{sub}</div>}
    </div>
  )
}

export function CpuCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const cores = server.cpu_cores ?? null
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<Cpu aria-hidden="true" className="size-3.5" />} pct={server.cpu} />
      <div className="pl-5 font-mono text-[10px] text-muted-foreground tabular-nums">
        {cores != null && `${cores} cores · `}load {server.load1.toFixed(2)}
      </div>
    </div>
  )
}
export function MemoryCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const pct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const swapColor = getBarTextColor(swapPct)
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<MemoryStick aria-hidden="true" className="size-3.5" />} pct={pct} />
      <div className="pl-5 font-mono text-[10px] text-muted-foreground tabular-nums">
        {formatBytes(server.mem_used)} / {formatBytes(server.mem_total)} ·{' '}
        <span className={cn('font-medium', swapColor)}>swap {Math.round(swapPct)}%</span>
      </div>
    </div>
  )
}
export function DiskCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const pct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<HardDrive aria-hidden="true" className="size-3.5" />} pct={pct} />
      <div className="flex items-center gap-2 pl-5 font-mono text-[10px] text-muted-foreground tabular-nums">
        <span className="inline-flex items-center gap-1">
          <ArrowDown aria-hidden="true" className="size-2.5" />
          <span className="font-medium text-foreground">{formatSpeed(server.disk_read_bytes_per_sec)}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <ArrowUp aria-hidden="true" className="size-2.5" />
          <span className="font-medium text-foreground">{formatSpeed(server.disk_write_bytes_per_sec)}</span>
        </span>
      </div>
    </div>
  )
}
interface NetworkCellProps {
  entry: TrafficOverviewItem | undefined
  server: ServerMetrics
}

export function NetworkCell({ server, entry }: NetworkCellProps) {
  const { used, limit, pct } = computeTrafficQuota({
    entry,
    netInTransfer: server.net_in_transfer,
    netOutTransfer: server.net_out_transfer
  })
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<Network aria-hidden="true" className="size-3.5" />} pct={pct} />
      <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 pl-5 font-mono text-[10px] text-muted-foreground tabular-nums">
        <span>
          {formatBytes(used)} / {formatBytes(limit)}
        </span>
        {server.online && (
          <>
            <span className="opacity-50">·</span>
            <span className="inline-flex items-center gap-1">
              <ArrowDown aria-hidden="true" className="size-2.5" />
              <span className="font-medium text-foreground">{formatSpeed(server.net_in_speed)}</span>
            </span>
            <span className="inline-flex items-center gap-1">
              <ArrowUp aria-hidden="true" className="size-2.5" />
              <span className="font-medium text-foreground">{formatSpeed(server.net_out_speed)}</span>
            </span>
          </>
        )}
      </div>
    </div>
  )
}

function osEmoji(os: string | null): string {
  if (!os) {
    return ''
  }
  const l = os.toLowerCase()
  if (l.includes('ubuntu') || l.includes('debian') || l.includes('linux')) {
    return '🐧'
  }
  if (l.includes('windows')) {
    return '🪟'
  }
  if (l.includes('macos') || l.includes('darwin')) {
    return '🍎'
  }
  if (l.includes('freebsd') || l.includes('openbsd')) {
    return '😈'
  }
  return ''
}

function relativeTime(thenSec: number, nowMs = Date.now()): string {
  const diffSec = Math.max(0, Math.floor(nowMs / 1000) - thenSec)
  if (diffSec < 60) {
    return `${diffSec}s ago`
  }
  if (diffSec < 3600) {
    return `${Math.floor(diffSec / 60)}m ago`
  }
  if (diffSec < 86_400) {
    return `${Math.floor(diffSec / 3600)}h ago`
  }
  return `${Math.floor(diffSec / 86_400)}d ago`
}

export function UptimeCell({ server }: { server: ServerMetrics }) {
  const { t } = useTranslation(['servers'])
  const emoji = osEmoji(server.os)
  if (!server.online) {
    return (
      <div className="flex flex-col">
        <span className="text-muted-foreground text-xs">{t('offline_label')}</span>
        <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
          {t('last_seen_ago', { time: relativeTime(server.last_active) })}
        </span>
      </div>
    )
  }
  return (
    <div className="flex flex-col">
      <span className="inline-flex items-center gap-1 font-mono text-muted-foreground text-xs tabular-nums">
        <Clock aria-hidden="true" className="size-3" />
        {formatUptime(server.uptime)}
      </span>
      {server.os && (
        <span className="font-mono text-[10px] text-muted-foreground tabular-nums">
          {emoji && <span className="mr-1">{emoji}</span>}
          {server.os}
        </span>
      )}
    </div>
  )
}

export function NameCell({ server, rightSlot }: { rightSlot?: ReactNode; server: ServerMetrics }) {
  const flag = countryCodeToFlag(server.country_code)
  return (
    <div className="flex min-w-0 flex-col">
      <div className="flex min-w-0 items-center gap-1.5">
        <Link
          className="group/link flex min-w-0 items-center gap-1.5"
          params={{ id: server.id }}
          search={{ range: 'realtime' }}
          to="/servers/$id"
        >
          {flag && <span className="text-xs">{flag}</span>}
          <span className="truncate font-medium group-hover/link:underline">{server.name}</span>
        </Link>
        {rightSlot}
      </div>
      <TagChipRow tags={server.tags} />
    </div>
  )
}
