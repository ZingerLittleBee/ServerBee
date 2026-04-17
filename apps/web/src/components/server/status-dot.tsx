import { cn } from '@/lib/utils'

interface StatusDotProps {
  className?: string
  online: boolean
}

export function StatusDot({ online, className }: StatusDotProps) {
  return (
    <span
      aria-label={online ? 'online' : 'offline'}
      className={cn(
        'inline-block size-2 rounded-full',
        online ? 'animate-pulse bg-emerald-500 shadow-[0_0_0_3px_rgba(16,185,129,0.18)]' : 'bg-muted-foreground/60',
        className
      )}
      data-slot="status-dot"
      role="img"
    />
  )
}
