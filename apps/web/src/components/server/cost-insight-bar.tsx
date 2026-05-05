import { CreditCard } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { TrafficProgress } from '@/components/server/traffic-progress'
import { useCostInsights } from '@/hooks/use-cost'
import type { ResourceValue, ServerCostInsights, ServerResponse } from '@/lib/api-schema'
import {
  formatCostAmount,
  formatCostRate,
  getCostGradeClassName,
  getCostInvalidReasonKey,
  getCostReasonKey
} from '@/lib/cost'
import { cn } from '@/lib/utils'

type CostInsightServer = Pick<
  ServerResponse,
  'billing_cycle' | 'currency' | 'expired_at' | 'price' | 'traffic_limit' | 'traffic_limit_type'
>

interface CostInsightBarProps {
  server: CostInsightServer
  serverId: string
}

const BAR_CLASS_NAME = 'mb-6 rounded-lg border bg-card p-3 text-sm'

export function CostInsightBar({ server, serverId }: CostInsightBarProps) {
  const { t } = useTranslation('servers')
  const costInsights = useCostInsights(serverId)
  const insights = costInsights.data

  if (insights == null) {
    return <FallbackBillingBar server={server} serverId={serverId} />
  }

  if (!insights.configured) {
    return (
      <div className={cn(BAR_CLASS_NAME, 'flex flex-wrap items-center gap-4')}>
        <CreditCard aria-hidden="true" className="size-4 text-muted-foreground" />
        <PriceCycle
          billingCycle={insights.billing_cycle ?? server.billing_cycle}
          currency={insights.currency ?? server.currency}
          price={insights.price ?? server.price}
        />
        <ExpiryStatus expiredAt={server.expired_at} />
        <span className="text-muted-foreground">{t(getInvalidReasonKey(insights.invalid_reason))}</span>
        {server.traffic_limit != null && <TrafficProgress serverId={serverId} />}
      </div>
    )
  }

  return <ConfiguredCostBar insights={insights} server={server} serverId={serverId} />
}

function ConfiguredCostBar({
  insights,
  server,
  serverId
}: {
  insights: ServerCostInsights
  server: CostInsightServer
  serverId: string
}) {
  const { t } = useTranslation('servers')
  const currency = insights.currency ?? server.currency
  const resourceItems = getResourceValueItems(insights.resource_value, currency)
  const reasons = insights.value_score?.reasons ?? []

  return (
    <div className={BAR_CLASS_NAME}>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <CreditCard aria-hidden="true" className="size-4 text-muted-foreground" />
        <PriceCycle
          billingCycle={insights.billing_cycle ?? server.billing_cycle}
          currency={currency}
          price={insights.price ?? server.price}
        />
        <ExpiryStatus expiredAt={server.expired_at} />
        {insights.cost_per_day != null && (
          <span>
            {t('cost_per_day', {
              amount: formatCostAmount(insights.cost_per_day, currency, { maximumFractionDigits: 2 })
            })}
          </span>
        )}
        {insights.cost_per_hour != null && (
          <span>
            {t('cost_per_hour', {
              amount: formatCostAmount(insights.cost_per_hour, currency, { maximumFractionDigits: 4 })
            })}
          </span>
        )}
        {insights.cost_per_second != null && (
          <span>
            {t('cost_per_second', {
              amount: formatCostAmount(insights.cost_per_second, currency, { maximumFractionDigits: 8 })
            })}
          </span>
        )}
        {insights.cycle_cost_elapsed != null && (
          <span>
            {t('cost_burned', {
              amount: formatCostAmount(insights.cycle_cost_elapsed, currency, { maximumFractionDigits: 2 })
            })}
            {insights.cycle_burn_percent != null && (
              <span className="ml-1 text-muted-foreground">({insights.cycle_burn_percent.toFixed(1)}%)</span>
            )}
          </span>
        )}
        {insights.days_remaining != null && <span>{t('cost_days_left', { count: insights.days_remaining })}</span>}
        {insights.value_score != null && (
          <CostMetric label={t('cost_value_score')}>
            <span className="font-mono font-semibold tabular-nums">{Math.round(insights.value_score.score)}</span>
            <span aria-hidden="true" className="mx-1 text-muted-foreground">
              /
            </span>
            <span className={cn('font-medium', getCostGradeClassName(insights.value_score.grade))}>
              {t(`cost_grade_${insights.value_score.grade}`)}
            </span>
          </CostMetric>
        )}
        {server.traffic_limit != null && <TrafficProgress serverId={serverId} />}
      </div>

      {(resourceItems.length > 0 || reasons.length > 0) && (
        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-xs">
          {resourceItems.map((item) => (
            <span className="font-mono tabular-nums" key={item.label}>
              {item.label} {item.value}
            </span>
          ))}
          {reasons.map((reason) => (
            <span key={reason}>{t(getCostReasonKey(reason))}</span>
          ))}
        </div>
      )}
    </div>
  )
}

function FallbackBillingBar({ server, serverId }: CostInsightBarProps) {
  return (
    <div className={cn(BAR_CLASS_NAME, 'flex flex-wrap items-center gap-4')}>
      <CreditCard aria-hidden="true" className="size-4 text-muted-foreground" />
      <PriceCycle billingCycle={server.billing_cycle} currency={server.currency} price={server.price} />
      <ExpiryStatus expiredAt={server.expired_at} />
      {server.traffic_limit != null && <TrafficProgress serverId={serverId} />}
    </div>
  )
}

function ExpiryStatus({ expiredAt }: { expiredAt?: string | null }) {
  const { t } = useTranslation('servers')

  if (!expiredAt) {
    return null
  }

  const expiryDate = new Date(expiredAt)
  const isExpired = expiryDate < new Date()
  const daysUntilExpiry = Math.ceil((expiryDate.getTime() - Date.now()) / 86_400_000)

  const expiryColor = (() => {
    if (isExpired) {
      return 'text-destructive'
    }
    if (daysUntilExpiry <= 7) {
      return 'text-yellow-600 dark:text-yellow-400'
    }
    return 'text-muted-foreground'
  })()

  return (
    <span className={cn(expiryColor)}>
      {isExpired
        ? `${t('detail_expired')} ${expiryDate.toLocaleDateString()}`
        : `${t('detail_expires')} ${expiryDate.toLocaleDateString()}`}
      {!isExpired && ` (${t('detail_expires_days', { count: daysUntilExpiry })})`}
    </span>
  )
}

function PriceCycle({
  billingCycle,
  currency,
  price
}: {
  billingCycle?: string | null
  currency?: string | null
  price?: number | null
}) {
  if (price == null) {
    return null
  }

  return (
    <span>
      {formatCostAmount(price, currency, { maximumFractionDigits: 2 })}
      {billingCycle && <span className="text-muted-foreground"> / {billingCycle}</span>}
    </span>
  )
}

function CostMetric({ children, label }: { children: ReactNode; label: string }) {
  return (
    <span>
      <span className="text-muted-foreground">{label}</span> <span className="font-medium">{children}</span>
    </span>
  )
}

function getInvalidReasonKey(reason: ServerCostInsights['invalid_reason']) {
  return reason == null ? 'cost_invalid' : getCostInvalidReasonKey(reason)
}

function getResourceValueItems(resourceValue: ResourceValue | null | undefined, currency: string | null | undefined) {
  if (resourceValue == null) {
    return []
  }

  const items: { label: string; value: string }[] = []

  if (resourceValue.cost_per_cpu_core != null) {
    items.push({
      label: 'CPU',
      value: formatCostRate(resourceValue.cost_per_cpu_core, currency, 'core', { maximumFractionDigits: 4 })
    })
  }
  if (resourceValue.cost_per_gb_memory != null) {
    items.push({
      label: 'RAM',
      value: formatCostRate(resourceValue.cost_per_gb_memory, currency, 'GB', { maximumFractionDigits: 4 })
    })
  }
  if (resourceValue.cost_per_gb_disk != null) {
    items.push({
      label: 'Disk',
      value: formatCostRate(resourceValue.cost_per_gb_disk, currency, 'GB', { maximumFractionDigits: 4 })
    })
  }
  if (resourceValue.cost_per_tb_traffic_limit != null) {
    items.push({
      label: resourceValue.traffic_limit_type ? `Traffic (${resourceValue.traffic_limit_type})` : 'Traffic',
      value: formatCostRate(resourceValue.cost_per_tb_traffic_limit, currency, 'TB', { maximumFractionDigits: 4 })
    })
  }

  return items
}
