import { MotionConfig, motion, useReducedMotion } from 'motion/react'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { SPRING_INDICATOR } from '@/lib/ease'
import { cn } from '@/lib/utils'

export type SecurityRangeKey = '24h' | '7d' | '30d'

const RANGE_KEYS = ['24h', '7d', '30d'] as const satisfies readonly SecurityRangeKey[]

/**
 * Security time-range control. Keeps the previous bordered segment look
 * (primary active / ghost inactive) and adds a beUI layoutId spring indicator.
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
  const layoutId = useId()
  const reduce = useReducedMotion()

  return (
    <MotionConfig transition={reduce ? { duration: 0 } : SPRING_INDICATOR}>
      <motion.div className={cn('flex gap-1 rounded-md border bg-card p-1', className)} layoutRoot>
        {RANGE_KEYS.map((key) => {
          const active = value === key
          return (
            <div className="relative" key={key}>
              {active ? (
                <motion.span className="absolute inset-0 bg-primary" layoutId={layoutId} style={{ borderRadius: 6 }} />
              ) : null}
              <Button
                className={cn(
                  'relative z-10',
                  active && 'bg-transparent text-primary-foreground hover:bg-transparent hover:text-primary-foreground'
                )}
                onClick={() => onValueChange(key)}
                size="sm"
                variant="ghost"
              >
                {t(`range.${key}`, { defaultValue: key })}
              </Button>
            </div>
          )
        })}
      </motion.div>
    </MotionConfig>
  )
}
