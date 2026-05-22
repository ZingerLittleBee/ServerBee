import { Badge } from '@/components/ui/badge'
import type { UnlockStatus } from '@/lib/ip-quality-types'
import { cn } from '@/lib/utils'

interface StatusMeta {
  label: string
  tone: string
}

// Colors per spec §7: unlocked = green, restricted = amber, blocked = red,
// failed = grey, unsupported = muted. Tones mirror the security severity badge.
const STATUS_META: Record<UnlockStatus, StatusMeta> = {
  unlocked: {
    label: 'Unlocked',
    tone: 'border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-300'
  },
  restricted: {
    label: 'Restricted',
    tone: 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300'
  },
  blocked: {
    label: 'Blocked',
    tone: 'border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300'
  },
  failed: {
    label: 'Failed',
    tone: 'border-muted-foreground/30 bg-muted text-muted-foreground'
  },
  unsupported: {
    label: 'Unsupported',
    tone: 'border-muted-foreground/20 bg-muted/50 text-muted-foreground/70'
  }
}

const FALLBACK_META: StatusMeta = {
  label: 'Unknown',
  tone: 'border-muted-foreground/20 bg-muted/50 text-muted-foreground/70'
}

interface Props {
  className?: string
  status: UnlockStatus
}

export function UnlockStatusBadge({ status, className }: Props) {
  const meta = STATUS_META[status] ?? FALLBACK_META
  return (
    <Badge className={cn('border', meta.tone, className)} variant="outline">
      {meta.label}
    </Badge>
  )
}
