import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

interface MetricCardHeaderProps {
  accent: string
  Icon: LucideIcon
  label: string
  serverName: string
}

export function MetricCardHeader({ Icon, label, serverName, accent }: MetricCardHeaderProps) {
  return (
    <div className="flex items-center gap-2.5">
      <div
        className={cn('flex size-8 shrink-0 items-center justify-center rounded-lg')}
        data-testid="metric-card-icon"
        style={{ backgroundColor: `color-mix(in oklab, var(${accent}) 18%, transparent)` }}
      >
        <Icon className="size-4" style={{ color: `var(${accent})` }} />
      </div>
      <span className="font-semibold text-sm leading-tight">{label}</span>
      <span className="ml-auto truncate text-muted-foreground text-xs leading-tight">{serverName}</span>
    </div>
  )
}
