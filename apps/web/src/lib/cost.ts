import type { CostAdvisory, CostInvalidReason } from './api-schema'

type CostFormatOptions = Intl.NumberFormatOptions

const DEFAULT_CURRENCY = 'USD'
const CURRENCY_CODE_PATTERN = /^[A-Za-z]{3}$/

const advisoryKeys: Record<CostAdvisory, string> = {
  expired_billing: 'cost_advisory_expired_billing',
  sleeping_money: 'cost_advisory_sleeping_money',
  idle_burn: 'cost_advisory_idle_burn',
  low_uptime: 'cost_advisory_low_uptime'
}

const invalidReasonKeys: Record<CostInvalidReason, string> = {
  missing_price: 'cost_invalid_missing_price',
  missing_billing_cycle: 'cost_invalid_missing_billing_cycle',
  invalid_billing_cycle: 'cost_invalid_invalid_billing_cycle',
  invalid_price: 'cost_invalid_invalid_price'
}

export function formatCostAmount(amount: number, currency: string | null | undefined, options?: CostFormatOptions) {
  const normalizedCurrency = normalizeCurrency(currency)
  return new Intl.NumberFormat(undefined, {
    style: 'currency',
    currency: normalizedCurrency,
    maximumFractionDigits: 4,
    ...options
  }).format(amount)
}

export function formatCostRate(
  amount: number,
  currency: string | null | undefined,
  unit: string,
  options?: CostFormatOptions
) {
  return `${formatCostAmount(amount, currency, options)}/${unit}`
}

export function getCostAdvisoryKey(advisory: CostAdvisory) {
  return advisoryKeys[advisory]
}

export function getCostInvalidReasonKey(reason: CostInvalidReason) {
  return invalidReasonKeys[reason]
}

function normalizeCurrency(currency: string | null | undefined) {
  const trimmed = currency?.trim()
  return trimmed && CURRENCY_CODE_PATTERN.test(trimmed) ? trimmed.toUpperCase() : DEFAULT_CURRENCY
}
