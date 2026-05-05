import type { CostInvalidReason, ValueGrade, ValueReason } from './api-schema'

type CostFormatOptions = Intl.NumberFormatOptions

const DEFAULT_CURRENCY = 'USD'
const CURRENCY_CODE_PATTERN = /^[A-Za-z]{3}$/

const gradeClassNames: Record<ValueGrade, string> = {
  excellent: 'text-emerald-600',
  good: 'text-green-600',
  okay: 'text-amber-600',
  poor: 'text-orange-600',
  waste: 'text-red-600'
}

const reasonKeys: Record<ValueReason, string> = {
  idle_burn: 'cost_reason_idle_burn',
  sleeping_money: 'cost_reason_sleeping_money',
  good_memory_value: 'cost_reason_good_memory_value',
  good_disk_value: 'cost_reason_good_disk_value',
  expensive_cpu: 'cost_reason_expensive_cpu',
  healthy_uptime: 'cost_reason_healthy_uptime',
  low_uptime: 'cost_reason_low_uptime',
  expired_billing: 'cost_reason_expired_billing',
  no_price_cycle: 'cost_reason_no_price_cycle',
  insufficient_data: 'cost_reason_insufficient_data',
  free_or_zero_price: 'cost_reason_free_or_zero_price'
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

export function getCostGradeClassName(grade: ValueGrade) {
  return gradeClassNames[grade]
}

export function getCostReasonKey(reason: ValueReason) {
  return reasonKeys[reason]
}

export function getCostInvalidReasonKey(reason: CostInvalidReason) {
  return invalidReasonKeys[reason]
}

function normalizeCurrency(currency: string | null | undefined) {
  const trimmed = currency?.trim()
  return trimmed && CURRENCY_CODE_PATTERN.test(trimmed) ? trimmed.toUpperCase() : DEFAULT_CURRENCY
}
