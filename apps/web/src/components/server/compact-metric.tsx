import { cn } from '@/lib/utils'

interface CompactMetricProps {
  className?: string
  icon?: React.ReactNode
  label: React.ReactNode
  subValue?: string
  value: React.ReactNode
}

export function CompactMetric({ label, value, subValue, icon, className }: CompactMetricProps) {
  return (
    <div className={cn('flex flex-col gap-0.5', className)}>
      <span className="text-[10px] text-muted-foreground leading-none">
        {icon && <span className="mr-1 inline-flex items-center">{icon}</span>}
        {label}
      </span>
      <div className="flex items-baseline gap-1">
        <span className="font-semibold text-sm">{value}</span>
        {subValue && <span className="text-[10px] text-muted-foreground">{subValue}</span>}
      </div>
    </div>
  )
}
