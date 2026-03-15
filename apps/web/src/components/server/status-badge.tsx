import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'

interface StatusBadgeProps {
  className?: string
  online: boolean
}

export function StatusBadge({ online, className }: StatusBadgeProps) {
  const { t } = useTranslation()
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 font-medium text-xs',
        online
          ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
          : 'bg-red-500/10 text-red-600 dark:text-red-400',
        className
      )}
    >
      <span className={cn('size-1.5 rounded-full', online ? 'bg-emerald-500' : 'bg-red-500')} />
      {online ? t('online') : t('offline')}
    </span>
  )
}
