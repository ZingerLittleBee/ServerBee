import { useTranslation } from 'react-i18next'
import type { ServerCostOverview } from '@/lib/api-schema'
import { formatCostRate } from '@/lib/cost'

interface CostFootnoteProps {
  entry?: ServerCostOverview
  inline?: boolean
}

export function CostFootnote({ entry, inline = false }: CostFootnoteProps) {
  const { t } = useTranslation('servers')

  if (!entry) {
    return null
  }

  return (
    <span className="inline-flex min-w-0 items-center gap-1.5">
      {!inline && <span aria-hidden="true">·</span>}
      {entry.configured ? (
        <ConfiguredFootnote entry={entry} />
      ) : (
        <span className="truncate">{t(getUnconfiguredLabel(entry))}</span>
      )}
    </span>
  )
}

function ConfiguredFootnote({ entry }: { entry: ServerCostOverview }) {
  const { t } = useTranslation('servers')

  if (entry.cost_per_hour == null) {
    return <span>{t('cost_invalid')}</span>
  }

  return (
    <span className="inline-flex min-w-0 items-center gap-1.5 tabular-nums">
      <span className="truncate font-medium text-foreground">
        {formatCostRate(entry.cost_per_hour, entry.currency, 'h', { maximumFractionDigits: 4 })}
      </span>
      {entry.cost_per_month_equivalent != null && (
        <>
          <span aria-hidden="true">·</span>
          <span className="font-medium text-foreground">
            {formatCostRate(entry.cost_per_month_equivalent, entry.currency, 'mo', { maximumFractionDigits: 2 })}
          </span>
        </>
      )}
    </span>
  )
}

function getUnconfiguredLabel(entry: ServerCostOverview) {
  if (entry.invalid_reason === 'missing_billing_cycle') {
    return 'cost_price_only'
  }
  if (entry.invalid_reason === 'missing_price') {
    return 'cost_not_set'
  }
  return 'cost_invalid'
}
