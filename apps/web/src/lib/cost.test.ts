import { describe, expect, it } from 'vitest'
import { formatCostAmount, getCostGradeClassName, getCostReasonKey } from './cost'

describe('cost utilities', () => {
  it('formats tiny per-second costs without rounding to zero', () => {
    const formatted = formatCostAmount(0.000_001_9, 'USD', { maximumFractionDigits: 8 })
    const digits = formatted.replace(/\D/g, '')

    expect(digits).toContain('0000019')
  })

  it('maps waste grade to destructive style', () => {
    expect(getCostGradeClassName('waste')).toContain('text-red')
  })

  it('maps known reasons to translation keys', () => {
    expect(getCostReasonKey('sleeping_money')).toBe('cost_reason_sleeping_money')
  })
})
