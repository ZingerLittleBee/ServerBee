import { useTranslation } from 'react-i18next'
import type { StatusKind } from '@/components/server/status-dot-utils'
import { cn } from '@/lib/utils'

interface StatusBadgeProps {
  className?: string
  status: StatusKind
}

const PILL_TONE: Record<StatusKind, string> = {
  online: 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
  offline: 'bg-red-500/10 text-red-600 dark:text-red-400',
  pending: 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
}

const DOT_TONE: Record<StatusKind, string> = {
  online: 'bg-emerald-500',
  offline: 'bg-red-500',
  pending: 'bg-amber-500'
}

const LABEL_KEY: Record<StatusKind, string> = {
  online: 'online',
  offline: 'offline',
  pending: 'servers:card_pending.pending_label'
}

export function StatusBadge({ status, className }: StatusBadgeProps) {
  const { t } = useTranslation(['servers'])
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 font-medium text-xs',
        PILL_TONE[status],
        className
      )}
    >
      <span className={cn('size-1.5 rounded-full', DOT_TONE[status])} />
      {t(LABEL_KEY[status], { defaultValue: status })}
    </span>
  )
}
