import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { formatCountdown, useCountdown } from '@/hooks/use-countdown'

export function TemporaryBadge({ expiresAt }: { expiresAt: number | null }) {
  const { t } = useTranslation('servers')
  const remaining = useCountdown(expiresAt)
  return (
    <Badge
      className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400"
      title={t('cap_temporary_tooltip')}
    >
      {t('cap_temporary')}
      {remaining != null && remaining > 0 ? ` · ${formatCountdown(remaining)}` : ''}
    </Badge>
  )
}
