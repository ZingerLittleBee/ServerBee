import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import type { UnlockStatus } from '@/lib/ip-quality-types'
import { cn } from '@/lib/utils'

// Colors per spec §7: unlocked = green, restricted = amber, blocked = red,
// failed = grey, unsupported = muted. Tones mirror the security severity badge.
const STATUS_TONE: Record<UnlockStatus, string> = {
  unlocked: 'border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-300',
  restricted: 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300',
  blocked: 'border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300',
  failed: 'border-muted-foreground/30 bg-muted text-muted-foreground',
  unsupported: 'border-muted-foreground/20 bg-muted/50 text-muted-foreground/70'
}

const FALLBACK_TONE = 'border-muted-foreground/20 bg-muted/50 text-muted-foreground/70'

interface Props {
  className?: string
  status: UnlockStatus
}

export function UnlockStatusBadge({ status, className }: Props) {
  const { t } = useTranslation('ip-quality')
  const tone = STATUS_TONE[status] ?? FALLBACK_TONE
  const label = STATUS_TONE[status] ? t(`status_${status}`) : t('status_unknown')
  return (
    <Badge className={cn('border', tone, className)} variant="outline">
      {label}
    </Badge>
  )
}
