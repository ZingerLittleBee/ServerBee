import { useTranslation } from 'react-i18next'
import type { ServerCostOverview } from '@/lib/api-schema'
import { formatCostRate, getCostGradeClassName, getCostInvalidReasonKey } from '@/lib/cost'
import { cn } from '@/lib/utils'

interface CostFootnoteProps {
  entry?: ServerCostOverview
}

export function CostFootnote({ entry }: CostFootnoteProps) {
  const { t } = useTranslation('servers')

  if (!entry) {
    return null
  }

  return (
    <>
      <span aria-hidden="true">·</span>
      {entry.configured ? <ConfiguredFootnote entry={entry} /> : <span>{t(getUnconfiguredLabel(entry))}</span>}
    </>
  )
}

function ConfiguredFootnote({ entry }: { entry: ServerCostOverview }) {
  const { t } = useTranslation('servers')

  if (entry.cost_per_hour == null) {
    return <span>{t('cost_invalid')}</span>
  }

  return (
    <span className="tabular-nums">
      <span className="font-medium text-foreground">
        {formatCostRate(entry.cost_per_hour, entry.currency, 'h', { maximumFractionDigits: 4 })}
      </span>
      {entry.value_score && (
        <>
          <span aria-hidden="true" className="mx-1">
            ·
          </span>
          <span>{Math.round(entry.value_score.score)}</span>
          <span className={cn('ml-1 font-medium', getCostGradeClassName(entry.value_score.grade))}>
            {t(`cost_grade_${entry.value_score.grade}`)}
          </span>
        </>
      )}
    </span>
  )
}

function getUnconfiguredLabel(entry: ServerCostOverview) {
  if (entry.invalid_reason === 'missing_price') {
    return 'cost_not_set'
  }
  if (entry.invalid_reason === 'missing_billing_cycle') {
    return 'cost_price_only'
  }
  return entry.invalid_reason ? getCostInvalidReasonKey(entry.invalid_reason) : 'cost_invalid'
}
