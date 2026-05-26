import { cn } from '@/lib/utils'

export type StatusKind = 'online' | 'offline' | 'pending'

interface StatusDotProps {
  className?: string
  status: StatusKind
}

const TONE_BY_STATUS: Record<StatusKind, string> = {
  online: 'animate-pulse bg-emerald-500 shadow-[0_0_0_3px_rgba(16,185,129,0.18)]',
  offline: 'bg-muted-foreground/60',
  pending: 'bg-amber-500'
}

export function StatusDot({ status, className }: StatusDotProps) {
  return (
    <span
      aria-label={status}
      className={cn('inline-block size-2 rounded-full', TONE_BY_STATUS[status], className)}
      data-slot="status-dot"
      role="img"
    />
  )
}

export function deriveServerStatus(s: { has_token?: boolean; online: boolean }): StatusKind {
  if (s.has_token === false) {
    return 'pending'
  }
  return s.online ? 'online' : 'offline'
}
