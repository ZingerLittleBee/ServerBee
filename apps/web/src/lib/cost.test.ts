import { describe, expect, it } from 'vitest'
import { formatCostAmount, getCostAdvisoryKey } from './cost'

describe('cost utilities', () => {
  it('formats tiny per-second costs without rounding to zero', () => {
    const formatted = formatCostAmount(0.000_001_9, 'USD', { maximumFractionDigits: 8 })
    const digits = formatted.replace(/\D/g, '')

    expect(digits).toContain('0000019')
  })

  it('maps known advisories to translation keys', () => {
    expect(getCostAdvisoryKey('sleeping_money')).toBe('cost_advisory_sleeping_money')
    expect(getCostAdvisoryKey('expired_billing')).toBe('cost_advisory_expired_billing')
  })
})
