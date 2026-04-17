import type { ReactNode } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'

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

// Temporary stubs — replaced in Tasks 8–13.
export function CpuCell(_: { server: ServerMetrics }) {
  return <MetricBarRow icon={null} pct={0} />
}
export function MemoryCell(_: { server: ServerMetrics }) {
  return <MetricBarRow icon={null} pct={0} />
}
export function DiskCell(_: { server: ServerMetrics }) {
  return <MetricBarRow icon={null} pct={0} />
}
export function NetworkCell(_: { server: ServerMetrics }) {
  return <MetricBarRow icon={null} pct={0} />
}
