import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

export type SecurityRangeKey = '24h' | '7d' | '30d'

const RANGE_KEYS = ['24h', '7d', '30d'] as const satisfies readonly SecurityRangeKey[]

/**
 * Security time-range control. Bordered segment look with primary active /
 * ghost inactive — no sliding indicator motion.
 */
export function SecurityRangeToggle({
  className,
  onValueChange,
  value
}: {
  className?: string
  onValueChange: (value: SecurityRangeKey) => void
  value: SecurityRangeKey
}) {
  const { t } = useTranslation('security')

  return (
    <div className={cn('flex gap-1 rounded-md border bg-card p-1', className)}>
      {RANGE_KEYS.map((key) => {
        const active = value === key
        return (
          <Button
            className="transition-none"
            key={key}
            onClick={() => onValueChange(key)}
            size="sm"
            variant={active ? 'default' : 'ghost'}
          >
            {t(`range.${key}`, { defaultValue: key })}
          </Button>
        )
      })}
    </div>
  )
}
