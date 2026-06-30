import type { ReactNode } from 'react'
import { RingChart } from '@/components/ui/ring-chart'

interface RingMetricProps {
  children: ReactNode
  color: string
  label: string
  value: number
}

export function RingMetric({ color, label, children, value }: RingMetricProps) {
  return (
    <div className="flex items-center gap-2">
      <RingChart color={color} compact label={label} value={value} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[11px] text-muted-foreground">{label}</span>
        <span className="truncate text-[10px] text-muted-foreground tabular-nums">{children}</span>
      </div>
    </div>
  )
}
