import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import type { StatusKind } from './status-dot-utils'

interface StatusDotProps {
  className?: string
  status: StatusKind
}

// No pulse on the online dot: `animate-pulse` is the loading-skeleton animation
// in this app, and a list of 20+ permanently pulsing rows reads as "still
// loading" rather than "healthy". The halo ring carries the online emphasis.
const TONE_BY_STATUS: Record<StatusKind, string> = {
  online: 'bg-status-healthy ring-3 ring-status-healthy/20',
  offline: 'bg-muted-foreground/60',
  pending: 'bg-status-warning'
}

export function StatusDot({ status, className }: StatusDotProps) {
  const { t } = useTranslation('common')
  const label = t(`status.${status}`)
  return (
    <>
      <span className="sr-only">{label}</span>
      <span
        aria-hidden="true"
        className={cn('inline-block size-2 rounded-full', TONE_BY_STATUS[status], className)}
        data-slot="status-dot"
        title={label}
      />
    </>
  )
}
