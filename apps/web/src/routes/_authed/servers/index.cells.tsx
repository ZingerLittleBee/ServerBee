import { ArrowDown, ArrowUp, Cpu, HardDrive, MemoryStick } from 'lucide-react'
import type { ReactNode } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, formatBytes, formatSpeed } from '@/lib/utils'

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
export function NetworkCell(_: { server: ServerMetrics }) {
  return <MetricBarRow icon={null} pct={0} />
}
