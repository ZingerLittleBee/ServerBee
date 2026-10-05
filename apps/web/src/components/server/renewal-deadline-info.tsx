import { useTranslation } from 'react-i18next'
import type { ServerResponse } from '@/lib/api-schema'
import { formatDateShort } from '@/lib/format'

export function RenewalDeadlineInfo({ renewal }: { renewal?: ServerResponse['renewal'] }) {
  const { t } = useTranslation('servers')
  if (!renewal) {
    return null
  }

  const originLabels = {
    confirmed: 'renewal_origin_confirmed',
    projected: 'renewal_origin_projected',
    frozen: 'renewal_origin_frozen'
  } as const

  return (
    <span className="block space-y-1 text-muted-foreground text-xs">
      <span className="block">{t(originLabels[renewal.deadline_origin])}</span>
      {renewal.confirmed_expired_at && (
        <span className="block">
          {t('renewal_confirmed_history', {
            date: formatDateShort(renewal.confirmed_expired_at, { timeZone: renewal.billing_timezone }),
            timezone: renewal.billing_timezone
          })}
        </span>
      )}
    </span>
  )
}
