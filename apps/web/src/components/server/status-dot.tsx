import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import type { StatusKind } from './status-dot-utils'

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
