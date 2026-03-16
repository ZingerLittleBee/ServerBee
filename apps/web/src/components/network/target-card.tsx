import { Eye, EyeOff } from 'lucide-react'
import { formatLatency, formatPacketLoss, type NetworkTargetSummary } from '@/lib/network-types'
import { cn } from '@/lib/utils'

interface TargetCardProps {
  color: string
  onToggle: () => void
  target: NetworkTargetSummary
  visible: boolean
}

export function TargetCard({ target, color, visible, onToggle }: TargetCardProps) {
  return (
    <div
      className={cn(
        'flex min-w-[160px] items-center gap-3 rounded-lg border bg-card px-3 py-2 transition-opacity',
        !visible && 'opacity-50'
      )}
    >
      <div aria-hidden="true" className="size-3 shrink-0 rounded-full" style={{ backgroundColor: color }} />
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium text-sm">{target.target_name}</p>
        <div className="flex items-center gap-2 text-muted-foreground text-xs">
          <span className="font-mono">{formatLatency(target.avg_latency)}</span>
          <span className="text-muted-foreground/60">|</span>
          <span>loss {formatPacketLoss(target.packet_loss)}</span>
        </div>
      </div>
      <button
        aria-label={target.target_name}
        className="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
        onClick={onToggle}
        type="button"
      >
        {visible ? <Eye className="size-4" /> : <EyeOff className="size-4" />}
      </button>
    </div>
  )
}
