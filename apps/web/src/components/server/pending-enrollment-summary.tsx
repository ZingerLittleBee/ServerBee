import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { OutstandingEnrollmentSummary } from '@/lib/api-schema'

function formatCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(remainingMs / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}m ${seconds.toString().padStart(2, '0')}s`
}

interface PendingSummaryProps {
  enrollment: OutstandingEnrollmentSummary | null | undefined
}

export function PendingEnrollmentSummary({ enrollment }: PendingSummaryProps) {
  const { t } = useTranslation(['servers'])
  const expiresAt = enrollment ? new Date(enrollment.expires_at).getTime() : null
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (expiresAt == null || expiresAt <= Date.now()) {
      return
    }
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [expiresAt])

  if (!enrollment) {
    return <p className="text-muted-foreground text-xs">{t('card_pending.no_code')}</p>
  }

  if (expiresAt != null && expiresAt > now) {
    return (
      <p className="text-muted-foreground text-xs tabular-nums">
        {t('card_pending.code_expires_in', {
          prefix: enrollment.code_prefix,
          countdown: formatCountdown(expiresAt - now)
        })}
      </p>
    )
  }

  return (
    <p className="text-muted-foreground text-xs">
      {t('card_pending.code_expired', { prefix: enrollment.code_prefix })}
    </p>
  )
}
