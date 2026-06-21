import { AlertTriangle } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ServerCostOverview } from '@/lib/api-schema'
import { formatCostAmount, getCostAdvisoryKey } from '@/lib/cost'

interface CostCellProps {
  entry?: ServerCostOverview
}

export function CostCell({ entry }: CostCellProps) {
  const { t } = useTranslation('servers')

  if (entry === undefined) {
    return <CostFallback label={t('cost_not_set')} />
  }

  if (!entry.configured) {
    return <CostFallback label={t(getUnconfiguredLabelKey(entry.invalid_reason))} />
  }

  const primary = getPrimaryCostLabel(entry, t)

  if (primary === null) {
    return <CostFallback label={t('cost_invalid')} />
  }

  const advisories = entry.advisories ?? []

  return (
    <div className="flex flex-col gap-0.5">
      <div className="h-4 truncate font-mono text-[10px] text-foreground tabular-nums">{primary}</div>
      {advisories.length > 0 && (
        <div className="flex h-4 items-center gap-1 truncate text-[10px] text-amber-600">
          <AlertTriangle aria-hidden="true" className="size-3 shrink-0" />
          <span className="truncate">{t(getCostAdvisoryKey(advisories[0]))}</span>
          {advisories.length > 1 && (
            <span className="text-muted-foreground">{t('cost_advisory_more', { count: advisories.length - 1 })}</span>
          )}
        </div>
      )}
    </div>
  )
}

function CostFallback({ label }: { label: string }) {
  return <span className="text-muted-foreground text-xs">{label}</span>
}

function getUnconfiguredLabelKey(reason: ServerCostOverview['invalid_reason']): string {
  if (reason === 'missing_billing_cycle') {
    return 'cost_price_only'
  }

  if (reason === 'missing_price' || reason == null) {
    return 'cost_not_set'
  }

  return 'cost_invalid'
}

function getPrimaryCostLabel(
  entry: ServerCostOverview,
  t: (key: string, options?: Record<string, string>) => string
): string | null {
  const monthlyEquivalent = entry.cost_per_month_equivalent
  if (monthlyEquivalent != null) {
    return t('cost_month_equivalent', {
      amount: formatCostAmount(monthlyEquivalent, entry.currency, { maximumFractionDigits: 2 })
    })
  }

  const costPerDay = entry.cost_per_day
  if (costPerDay != null) {
    return t('cost_per_day', {
      amount: formatCostAmount(costPerDay, entry.currency, { maximumFractionDigits: 2 })
    })
  }

  return null
}
