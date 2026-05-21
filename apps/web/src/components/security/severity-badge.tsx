import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

export type SecurityEventTypeKey = 'ssh_brute_force' | 'ssh_login' | 'port_scan'

interface Props {
  className?: string
  eventType: string
  firstSeen?: boolean
}

const TYPE_TONE: Record<string, string> = {
  ssh_brute_force: 'border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300',
  port_scan: 'border-orange-500/40 bg-orange-500/10 text-orange-600 dark:text-orange-300',
  ssh_login: 'border-blue-500/40 bg-blue-500/10 text-blue-600 dark:text-blue-300'
}

export function EventTypeBadge({ eventType, firstSeen, className }: Props) {
  const { t } = useTranslation('security')
  const tone = TYPE_TONE[eventType] ?? 'border-muted-foreground/30 bg-muted text-muted-foreground'
  const label = t(`event_type.${eventType}`, { defaultValue: eventType })
  return (
    <Badge className={cn('gap-1.5 border', tone, className)} variant="outline">
      {firstSeen && <span className="size-1.5 rounded-full bg-current" />}
      <span>{label}</span>
    </Badge>
  )
}

const SEVERITY_TONE: Record<string, string> = {
  critical: 'border-red-600/50 bg-red-500/15 text-red-600 dark:text-red-300',
  high: 'border-orange-500/50 bg-orange-500/15 text-orange-600 dark:text-orange-300',
  medium: 'border-amber-500/50 bg-amber-500/15 text-amber-600 dark:text-amber-300',
  low: 'border-muted-foreground/30 bg-muted text-muted-foreground'
}

export function SeverityBadge({ severity }: { severity: string }) {
  const { t } = useTranslation('security')
  const tone = SEVERITY_TONE[severity] ?? SEVERITY_TONE.low
  return (
    <Badge className={cn('border', tone)} variant="outline">
      {t(`severity.${severity}`, { defaultValue: severity })}
    </Badge>
  )
}
